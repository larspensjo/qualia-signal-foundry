# Plan: Multi-Turn Text Loop

## Status

Draft

## Purpose

Extend the existing single-turn experiments with a multi-turn human-in-the-loop text
conversation. The first stage tests whether the simulation feels continuous when a
single human keeps talking and the system retains everything that came before in the
same session.

The goal is presence and continuity across turns, not autonomous agency. The human
drives every cycle; the system never produces a turn without a human input.

## Context

Current state:

- The text-owned voice loop proves single-turn ownership of response generation:
  speech input -> memory retrieval -> context assembly -> conversational responder
  -> speech output, recorded as QSF events with selectable memory sources.
- Memory retrieval, association traversal, the deterministic mock model client, and
  the OpenAI-backed `ModelClient` adapter are all in place.
- The reviewed memory promotion workflow (sleep report -> draft -> acceptance ->
  durable fixture) is complete, so memory content can be curated outside the live
  loop.
- The runtime loop is event-reducer-state with side effects isolated and fed back as
  events (Decision 2026-05-09).
- Every experiment so far is single-turn. There is no `SessionState` and no mechanism
  for a model call to see what happened earlier in the same run.

Each existing voice or text experiment treats every turn as a cold start. That makes
session continuity impossible to study and forces the model to rely entirely on
retrieved memory rather than recent context.

## Principles

- The human drives every turn. The system never initiates a turn on its own.
- The interaction modality for the first iteration is text; voice can adopt the same
  session-state abstraction in a later stage.
- Live turns remain read-only against durable memory. Session-internal summaries
  (introduced in stage 2) live in session state only; promotion to durable memory
  still goes through the existing reviewed-memory pipeline.
- The reducer stays pure. Side effects (stdin read, model call, memory retrieval,
  stdout write) emit events that the reducer processes.
- Every stage produces a written experiment report. The report is the gate to the
  next stage.
- Prompt caching is a first-class design constraint, not an implementation detail.
  The prompt assembly discipline must keep the prefix byte-stable so cached input
  tokens are reused on every turn after the first.
- Model choice is configuration, not code. Each stage's report compares at least two
  model tiers.

## Architecture

A new experiment, `multi-turn-text-loop`, registered alongside the existing
experiments:

```
crates/qsf_app/src/experiments/multi_turn_text_loop.rs
crates/qsf_app/src/experiments/registry.rs   (extended)
```

A new prompt-assembly module that composes on top of the existing
fragment-budget context assembly. Per-turn retrieval feeds `ContextAssembly`
first (preserving the existing selected/omitted/budget observability); the new
module serializes the assembled selection into a cache-friendly chat-message
list.

`ContextBudget` (the `ConversationalResponder` role default is 4 fragments /
600 estimated tokens) governs **per-turn fragment selection only** — how
many newly-retrieved memory fragments are admitted into the current turn's
user message. It does not bound the total prompt size. Total prompt size is
tracked separately via `input_tokens` from the model usage response and is
bounded by the model's context window; running out of context-window room is
a stage-2 concern handled via summarization.

```
crates/qsf_app/src/conversation/prompt.rs    (new)
crates/qsf_app/src/conversation/mod.rs       (new)
```

Reused without change:

- `Experiment` trait, `RunContext`, event log, trace log, markdown report writer.
- `ContextAssembler` and `ContextAssembly` (selected, omitted, budget tracking).
- `ModelClient` boundary and `ConversationalResponder` role.
- Memory module: `MemoryFixture`, file-backed loader, association-weighted retrieval.
- Deterministic mock model client for default and test runs.

The experiment runs through an internal helper `run_with_io_and_components(input,
output, ...)` that takes `impl BufRead` and `impl Write`. `Experiment::run` is a
thin wrapper that binds real stdin/stdout and env configuration. The experiment
exits cleanly on EOF or a `:quit` line.

## State Shape

`SessionState` is owned by the experiment runtime and lives for one CLI run. Stage 1
does not persist it across runs.

```rust
struct SessionState {
    started_at: SystemTime,
    config: SessionConfig,
    turns: Vec<Turn>,
}

struct SessionConfig {
    model_id: String,
    max_turns: usize,
    memory_source: MemorySourceConfig,
}

struct Turn {
    index: usize,
    started_at: SystemTime,
    completed_at: SystemTime,
    user_input: String,
    context_assembly: ContextAssembly,    // selected + omitted under budget
    retrieved_memory_block: String,        // serialization of selected fragments
    assistant_response: String,
    model_id: String,
    model_latency_ms: u64,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
    full_request_hash: ContentHash,        // hash of the exact messages sent
}
```

Invariants:

- `Turn` is frozen once appended. Its `retrieved_memory_block` and
  `full_request_hash` are locked when the turn completes. Later turns retrieving
  different memories never rewrite a prior turn.
- `turns` is append-only. No removal, no reordering. Failed turns are not appended.
- `SessionConfig` is constant for the session.

`ContentHash` is a newtype around `[u8; 32]` holding a SHA-256 hash of a
canonical message serialization. The canonical form, computed before hashing,
is:

```
for each message in order:
    write u32 little-endian: role.bytes().len()
    write role.bytes()
    write u32 little-endian: content.bytes().len()
    write content.bytes()
```

The length-prefixed encoding avoids delimiter ambiguity and is independent of
provider wire format. Switching providers does not invalidate prior hashes as
long as the same `(role, content)` pairs are sent. SHA-256 is over-engineered
for an internal consistency check but is unambiguous and lives behind one
small `sha2` dependency; BLAKE3 is an acceptable substitute if a faster hash
is preferred. The choice is locked during stage-1 implementation.

`Turn.context_assembly` stores the full `ContextAssembly` (selected and
omitted fragments under their budget) so the per-turn markdown report can
render selection rationale without re-running retrieval. With `max_turns`
bounded at 10 in stage 1, the in-memory cost is small. A future stage can
swap the embedded fragments for IDs plus a session-scoped lookup table if
serialization size becomes a concern.

## Reducer And Event Model

A pure reducer drives `SessionState` from a local `SessionEvent` enum. Side
effects (stdin read, memory retrieval, context assembly, model call, stdout
write) live in orchestration code that emits `SessionEvent` values; only the
reducer mutates state.

```rust
enum SessionEvent {
    SessionStarted(SessionConfig),
    InputReceived { input: String },
    MemoryRetrieved { ids: Vec<MemoryId>, fragments: Vec<ContextFragment> },
    ContextAssembled(ContextAssembly),
    PromptAssembled { full_request_hash: ContentHash,
                      message_count: usize, total_bytes: usize },
    ModelRoleCompleted { response: String, latency_ms: u64,
                         input_tokens: u32, cached_input_tokens: u32,
                         output_tokens: u32 },
    ModelRoleFailed { error_summary: String },
    TurnCompleted(Turn),
    SessionLimitReached { current: usize, max: usize, override_active: bool },
    SessionEnded { reason: SessionEndReason },
}

fn reduce_session(state: SessionState, event: SessionEvent) -> SessionState { ... }
```

`SessionEvent` is the reducer's input domain. The shared `EventType` enum
(used for `events.jsonl` observability) gets new variants `SessionStarted`,
`TurnCompleted`, `PromptAssembled`, `SessionLimitReached`, and `SessionEnded`
to mirror existing precedents like `SleepPhaseRequested`/`SleepPhaseCompleted`.
Existing events (`InputReceived`, `MemoryRetrievalRequested`, `MemoryRetrieved`,
`ContextAssemblyRequested`, `ContextAssembled`, `ModelRoleRequested`,
`ModelRoleCompleted`, `ModelRoleFailed`) are reused unchanged.

### Reducer-Event-Log Mapping

For each runtime event the orchestration code performs two flows in order:

1. **Update state.** Construct a `SessionEvent`, call
   `reduce_session(state, event)`, replace the state binding.
2. **Record observability.** Call `RunContext::record_event(EventType,
   payload, trace_id)` to write to `events.jsonl`.

The two types are deliberately separate. `SessionEvent` carries typed
fields (`Vec<MemoryId>`, `ContentHash`, `ContextAssembly`) for direct use by
`reduce_session`. `EventType` is a string-tagged enum whose payloads are
`serde_json::Value`; the orchestration code serializes the relevant
`SessionEvent` fields into the JSON payload.

Most variants map 1:1:

| `SessionEvent` variant | `EventType` variant | Status |
|---|---|---|
| `SessionStarted` | `SessionStarted` | new |
| `InputReceived` | `InputReceived` | existing |
| `MemoryRetrieved` | `MemoryRetrieved` | existing |
| `ContextAssembled` | `ContextAssembled` | existing |
| `PromptAssembled` | `PromptAssembled` | new |
| `ModelRoleCompleted` | `ModelRoleCompleted` | existing, extended payload |
| `ModelRoleFailed` | `ModelRoleFailed` | existing |
| `TurnCompleted` | `TurnCompleted` | new |
| `SessionLimitReached` | `SessionLimitReached` | new |
| `SessionEnded` | `SessionEnded` | new |

Variants like `SessionEvent::InputReceived` and `EventType::InputReceived`
share names but are distinct types in distinct enums; Rust's namespacing
keeps them unambiguous. The orchestration code is the only place that maps
between them. The reducer never sees `EventType`; the event-log writer
never sees `SessionEvent`.

`MemoryRetrievalRequested`, `ContextAssemblyRequested`, and
`ModelRoleRequested` are observability-only events (they record that a side
effect started, not a state change). They have no `SessionEvent`
counterpart.

## Event Sequence Per Turn

```
SessionStarted { config }                       once at startup

  for each turn N:
    InputReceived { input }                     (existing event)
    MemoryRetrievalRequested { query }          (existing event)
    MemoryRetrieved { memory_ids, fragments }   (existing event)
    ContextAssemblyRequested { ... }            (existing event)
    ContextAssembled { selected, omitted,       (existing event)
                       used_estimated_tokens }
    PromptAssembled { full_request_hash,        (new event)
                      message_count, total_bytes }
    ModelRoleRequested { role: ConversationalResponder, model_id }
    ModelRoleCompleted { response, latency_ms,  (existing event, extended payload)
                         input_tokens, cached_input_tokens,
                         output_tokens }
    TurnCompleted { turn: Turn }                (new event; reducer appends)

SessionEnded { reason: EofOrQuitOrError }       once at shutdown
```

If `turns.len() == max_turns` and `QSF_SESSION_ALLOW_OVER_LIMIT` is not set, the
next `InputReceived` produces a `SessionLimitReached` event in place of the
normal turn sequence, and no model call is made.

`PromptAssembled.full_request_hash` is a hash over the canonical serialization
of the exact `(role, content)` pairs sent to the model on this turn. The
byte-stability invariant is checked turn-over-turn (see the next section).

## Prompt Assembly And Caching Discipline

For turn N, the assembler produces messages in this order:

```
[0]   system    = SESSION_SYSTEM_PROMPT          constant string
[1]   user      = format_turn(turns[0])          frozen
[2]   assistant = turns[0].assistant_response    frozen
[3]   user      = format_turn(turns[1])          frozen
[4]   assistant = turns[1].assistant_response    frozen
  ...
[2N-1] user     = format_turn(turns[N-1])        frozen
[2N]  assistant = turns[N-1].assistant_response  frozen
[2N+1] user     = format_new_turn(input_N, retrieved_N)   the only varying message
```

`format_turn` is the same templating function used both when freezing a completed
turn and when re-rendering prior turns from `SessionState`. Same inputs produce the
same bytes. That is the byte-stability invariant in one line.

`format_new_turn(input, retrieved)`:

```
[Retrieved memory]
- <memory 1 summary>
- <memory 2 summary>

[User]
<user input verbatim>
```

If no memory was retrieved for a turn, the `[Retrieved memory]` block is omitted
entirely. Inlining retrieval inside the user message means per-turn retrieval choice
never perturbs any prior message.

`SESSION_SYSTEM_PROMPT` is one stable constant in the source tree. It explains the
system's framing in compact language and contains no per-run variables, no
timestamps, no turn counters.

The discipline forbids:

- Updating a prior turn's retrieval block after the fact.
- Re-ordering memory bullets inside a frozen turn.
- Stamping a timestamp or turn counter into the system prompt.
- Any dynamic prefix content.

Verification:

Each turn records `full_request_hash` over the canonical serialization of the
exact `(role, content)` pairs sent to the model. On turn N+1, the assembler
computes `prior_request_prefix_hash` over the first `turn_N.message_count`
messages of the new request and asserts equality with turn N's
`full_request_hash`. The hash is over a stable canonical serialization (role
then content, length-prefixed or delimiter-escaped) so message-boundary tricks
cannot produce a false match.

```
prior_request_prefix_hash(turn N+1, len = turn_N.message_count)
  ==
turn_N.full_request_hash
```

`cached_input_tokens / input_tokens` is logged per turn. **Caveat:** OpenAI prompt
caching requires prompts of at least 1024 input tokens; below that floor,
`cached_input_tokens` is always zero regardless of prefix stability. A healthy session
shows `cached_input_tokens > 0` only on turns where `input_tokens >= 1024`, with the
ratio climbing as later turns accumulate cached prefix. The stage-1 report
records both metrics per turn so the floor effect is visible, not confused with
broken caching.

Sources: OpenAI prompt caching guide
(<https://developers.openai.com/api/docs/guides/prompt-caching>).

## Configuration

| Env var | Default | Notes |
|---|---|---|
| `QSF_CONVERSATION_MODEL` | `gpt-5.4-mini` | The `ConversationalResponder` role default is `gpt-5.4-nano`, chosen for voice-loop latency; multi-turn text has more latency budget and continuity research benefits from slightly more capability, so this experiment overrides to mini. Stage 1 comparison covers at least nano and mini. |
| `QSF_SESSION_MAX_TURNS` | `10` | Hard stop on model calls. The N+1th `InputReceived` produces `SessionLimitReached` instead of a turn unless `QSF_SESSION_ALLOW_OVER_LIMIT=true`. |
| `QSF_SESSION_ALLOW_OVER_LIMIT` | `false` | When `true`, lifts the hard stop at `max_turns`. Explicit override for manual long-session experiments. |
| `QSF_SESSION_MEMORY_SOURCE` | `phase_four_fixture` | `file` selects a JSON `MemoryFixture`. |
| `QSF_SESSION_MEMORY_FILE` | unset | Path to JSON fixture when source is `file`. |

Default behavior is deterministic against the existing phase-four fixture, matching
the discipline of Decision 2026-05-12. Real model calls require explicit OpenAI
configuration through the existing `openai` feature flag; the deterministic mock
model client is the default.

## Error Handling

- Model call fails: emit `ModelRoleFailed` with sanitized error (no API keys, no raw
  payloads). The turn is not appended to `SessionState`. The user receives a "model
  unavailable, try again or `:quit`" message. The append-only invariant means failed
  turns never enter the cache prefix.
- Cache diagnostics absent: `cached_input_tokens` defaults to `0`. The report flags any
  session where no turn reported cache data, since that indicates broken telemetry.
- Memory source missing: fall back to the deterministic fixture, log the fallback as
  an event. The session continues with reduced grounding rather than crashing.
- Session reaches `max_turns`: the next `InputReceived` emits
  `SessionLimitReached { current, max, override_active: false }` and does not
  trigger a model call. The user is told to `:quit` or restart with
  `QSF_SESSION_ALLOW_OVER_LIMIT=true`. If the override is active at startup, the
  event still fires (for observability) but the session continues. Automatic
  summarization is a stage-2 feature. **Known stage-1 limitation:** restarting
  with the override loses the in-memory session (no persistence in stage 1), so
  the override is most useful when set deliberately before a long-session
  experiment begins. Session persistence in a later stage would allow
  resumption without losing prior turns.
- Stdin EOF or `:quit`: clean shutdown, emit `SessionEnded`, write the markdown
  report, exit 0.

## Testing

Three layers:

1. Pure unit tests.
   - **Prompt assembler:** byte-stability invariant against canned
     `SessionState` values — turn N+1's `prior_request_prefix_hash` equals
     turn N's `full_request_hash`; retrieval changes never perturb prior
     turns; system prompt is constant across turns; `[Retrieved memory]` block
     is omitted entirely when no memory was retrieved.
   - **Reducer:** one test per `SessionEvent` variant, covering
     `SessionStarted`, `InputReceived`, `MemoryRetrieved`, `ContextAssembled`,
     `PromptAssembled`, `ModelRoleCompleted`, `ModelRoleFailed`, `TurnCompleted`,
     `SessionLimitReached`, `SessionEnded`. Required assertions:
     `ModelRoleFailed` must not append a turn; `SessionLimitReached` without
     override active must not append a turn; `TurnCompleted` must append in
     order and freeze the turn record.
2. Mock-model integration test. Drives a three-turn session via the
   `run_with_io_and_components(input, output, ...)` helper, supplying in-memory
   `BufRead` and `Write` implementations. Deterministic mock model client.
   Asserts:
   - three turns appended in order
   - `events.jsonl` contains the expected event sequence including
     `ContextAssembled` and `PromptAssembled`
   - markdown report renders without panicking
   - `prior_request_prefix_hash(turn N+1)` equals `turn_N.full_request_hash`
     for every consecutive pair
3. Live-model smoke test (opt-in). Gated by the existing `openai` feature flag
   and the existing model-client selection mechanism (an `OPENAI_API_KEY` alone
   must not activate it, per Decision 2026-05-11). Excluded from default
   `cargo test`; documented as a manual run.
   - Five-turn canned-input session against `gpt-5.4-mini`. The fixture is
     sized so that by turn N=3 the prompt exceeds 1024 input tokens (OpenAI's
     documented prompt-caching minimum).
   - Assertions: for each turn, record `input_tokens`; assert
     `cached_input_tokens > 0` **only** on turns where `input_tokens >= 1024`. The
     full-request / prior-prefix hash invariant holds across all turns.
   - A cache miss above the floor with a verified-stable prefix is treated as
     a telemetry failure: the report records selected provider, selected
     model, request timing, and prefix hash for diagnosis.

## Stage 1: Hot Tier Only

Goal: prove the session-state plumbing and the cache-stability invariant in
isolation.

Behavior:

- New `multi-turn-text-loop` experiment, registered.
- `SessionState`, `Turn`, and `SessionEvent` types as specified above.
- Pure `reduce_session(state, event) -> state` function; orchestration code
  only mutates state by emitting `SessionEvent` values into the reducer.
- `run_with_io_and_components(input: impl BufRead, output: impl Write, ...)`
  helper hosts the turn loop. `Experiment::run` binds real stdin/stdout and env
  configuration.
- Memory retrieval per turn through the existing association-weighted path.
  Selected/omitted fragments pass through the existing `ContextAssembler` under
  a budget; selection and omissions are recorded in the `ContextAssembled`
  event before prompt assembly.
- Prompt assembler serializes the assembled selection into the new user message
  and produces the cache-friendly chat-message list.
- Model role invocation through the existing `ConversationalResponder` boundary
  with `model_id` from `QSF_CONVERSATION_MODEL`.
- `cached_input_tokens`, `input_tokens`, and `output_tokens` captured from the
  model response usage and recorded in the `ModelRoleCompleted` event.
- New events `SessionStarted`, `PromptAssembled`, `TurnCompleted`,
  `SessionLimitReached`, `SessionEnded` added to the shared `EventType` enum.
- `SessionLimitReached` is emitted instead of a turn sequence when
  `turns.len() == max_turns` and the override is not active.
- Generated markdown report includes per-turn `input_tokens` / `cached_input_tokens`
  / `output_tokens` / `model_latency_ms`, the cache hit ratio curve, the
  hash-invariant verification result, and any cache misses above the 1024-token
  floor flagged as telemetry concerns.

Out of scope for stage 1:

- Summarization of older turns.
- Recall tool.
- Session persistence across runs.
- Session-aware retrieval (each turn retrieves from the latest user input only).
- Voice modality.
- Between-turn reflection or any cognitive activity while the human is silent.

Verification:

- All unit tests pass (prompt assembler + reducer variants).
- Mock-model integration test passes including the
  `prior_request_prefix_hash` / `full_request_hash` invariant.
- Live-model smoke test against `gpt-5.4-mini` produces `cached_input_tokens > 0`
  on every turn where `input_tokens >= 1024`. Turns below the floor are
  recorded but do not constitute test failures.
- A manually-driven five-turn session feels continuous (qualitative review
  noted in the stage report).
- Cross-model comparison: the same canned five-turn input is run against at
  least two model tiers (nano and mini at minimum); results are recorded in
  the stage report.

Stage report: `docs/Experiments/Report.MultiTurnTextLoop.<date>.md` covering config,
per-turn token and latency tables, cache hit ratio curve, qualitative continuity
notes, cross-model comparison, what worked, what did not, open questions feeding
stage 2, decision candidates.

## Stage 2: Warm Tier (Summarization)

Goal: let sessions outlive the stage-1 cache-friendly ceiling without losing
coherence or blowing the context window. The motivation is sustained continuity,
not token cost.

Sketch:

- New `QSF_SESSION_WARM_THRESHOLD` env var (default ~10).
- When `turns.len() > threshold`, the oldest aged-out turn(s) move from `turns` to a
  new `summarized_turns: Vec<TurnSummary>` list. A summarizer model role produces a
  one-sentence summary per aged-out turn.
- Summaries appear as a stable "earlier in this session" block in the system message.
  This invalidates the cache prefix once per ageing event. The stage report measures
  cache hit rate before and after the first age-out.
- `Turn` records continue to be append-only. `summarized_turns` is also append-only.
- Out of scope: summary refresh, multi-pass summarization, summary editing,
  promotion of summaries to durable memory.

Stage report includes: cache hit rate curve across the first ageing event, summary
drift across long sessions, response quality with summaries versus verbatim, summary
model comparison.

## Stage 3: Recall Tool

Goal: let the model expand a summarized turn back to verbatim text on demand, so
older detail remains retrievable without permanently inflating the prompt.

Sketch:

- Adds OpenAI function-calling support for the conversational responder.
- One tool: `recall_turn(turn_id) -> verbatim_text`.
- New `ToolExecuted` event type, the executed counterpart to the existing
  `ToolRequested` event. Scoped to this experiment; the realtime voice provider
  boundary from Decision 2026-05-14 remains unchanged.
- Recalled verbatim text enters the chat history as a tool message and is preserved
  into future turns' cache prefix.
- Out of scope: arbitrary tool registration, tool authorization policies,
  cross-experiment tool sharing.

Stage report includes: how often the model uses recall, whether recalled context
improves responses, latency cost of tool round-trips, cross-model differences in
tool-use behavior.

## Open Questions

- Should the system prompt mention that this is a research environment, or stay
  neutral framing only? (Stage 1 picks a neutral framing; revisit in the report.)
- Do the new session events (`SessionStarted`, `TurnCompleted`, `PromptAssembled`,
  `SessionLimitReached`, `SessionEnded`) belong in the shared `EventType` enum
  long-term, or should some stay experiment-local payloads? The plan adds them
  to the shared enum mirroring `SleepPhaseRequested`/`SleepPhaseCompleted`;
  stage-1 review can revisit.
- Stage 2: should the warm threshold be turn count, token count, or both?
- Stage 2: should summarization use the same model as the responder or a smaller
  one by default?
- Stage 2: when multiple turns age out at once, should the system batch them
  into a single cache-prefix invalidation, or accept one invalidation per
  aged-out turn? This affects both cache economics and implementation
  complexity, and is worth measuring in the stage-2 report.
- Stage 3: should the recall tool also be able to recall summarized-but-not-aged-out
  turns, or only summarized ones?
- Future stage: should retrieval become session-aware (using recent turns to bias
  the retrieval query), and if so, does that integrate with the same memory
  retrieval path or a parallel one?
- Future stage: how does between-turn reflection fit, and is it a separate
  experiment or an option on this one?

## Decision Candidates

The following are likely to become decision-log entries once stage 1 is reviewed:

- The session-state runtime invariant: `Turn` is frozen on append; `turns` is
  append-only; the prompt prefix is byte-stable.
- The prompt assembly contract for cache-friendly conversations.
- The configuration-driven model choice pattern for multi-turn experiments.
- Live turns remain read-only against durable memory; session-internal artifacts do
  not auto-promote.

## Refs

- docs/Architecture/Architecture.RuntimeLoop.md
- docs/Architecture/Architecture.ContextManagement.md
- docs/Architecture/Architecture.MemorySystem.md
- docs/Concepts/Concept.MultiModelMind.md
- docs/DecisionLog.md (2026-05-09, 2026-05-11, 2026-05-12, 2026-05-14, 2026-05-15,
  2026-05-16 entries)
- docs/Plans/Idea.VolitionGoalSystem.md
- docs/Plans/Idea.SelfReflectionProjectIntrospection.md
- docs/Plans/Idea.LiveActivationDashboard.md
