# Plan: Multi-Turn Text Loop

## Status

Stages 1, 2, and 3 are complete.

This document is now an archival roadmap plus one active follow-up:
**Stage 3.1: Provider-Native OpenAI Function Calling**. Do not delete this plan yet;
it still records the session-state invariants, prompt-caching contract, open
questions, and the rationale for the follow-up work.

## Purpose

Extend single-turn QSF experiments into a human-driven multi-turn text session where
the model can see prior turns, older turns can age into warm summaries, and
summarized detail can be recalled without permanently inflating every prompt.

The human drives every cycle. The system never initiates a turn without human input.
Live turns remain read-only against durable memory; session-local summaries and
recalls do not auto-promote into durable memory.

## Completed Summary

Implemented artifacts:

- Runtime: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
- Prompt assembly: `crates/qsf_app/src/conversation/prompt.rs`
- Model boundary: `crates/qsf_app/src/models`
- Shared event types: `crates/qsf_app/src/observability/event_log.rs`
- Reports:
  - `docs/Experiments/Report.MultiTurnTextLoop.Stage3.2026-05-17.md`
  - `runs/2026-05-17-055619-multi-turn-text-loop` (mock recall run)
  - `runs/2026-05-17-061331-multi-turn-text-loop` (live OpenAI continuity run)

Accepted implementation shape:

- `multi-turn-text-loop` is registered as a normal experiment.
- `SessionState` is in-memory for one CLI run.
- Completed turns are append-only and frozen.
- Warm summaries are append-only session-local records.
- Recall records are frozen into the completed turn that used the tool.
- Failed model/tool turns are not appended.
- Prompt requests are hashed with a canonical length-prefixed role/content encoding.
- Prompt prefix stability is verified turn-over-turn except when intentionally
  invalidated by warm-summary ageing.

## Core Invariants

- Reducers stay pure. Side effects emit events; state changes happen through
  `SessionEvent` reduction.
- `Turn` records are append-only. Later retrieval, summarization, or recall never
  rewrites earlier turns.
- Durable memory is read-only during live turns.
- Session summaries and recall records are session-local unless a later explicit
  promotion workflow is added.
- Prompt assembly uses one renderer for frozen prior turns and the current turn.
  Same stored inputs must produce the same bytes.
- `PromptAssembled` is emitted before the model call that uses that prompt. A recall
  turn emits a second `PromptAssembled` after the tool message is added and before
  the follow-up model call.
- `ContextBudget` controls only per-turn retrieved-memory fragment selection. It is
  not the total prompt budget.
- Prompt caching metrics are interpreted with OpenAI's 1024 input-token floor in
  mind.

## Current State Shape

Conceptually:

```rust
struct SessionState {
    started_at: SystemTime,
    config: SessionConfig,
    turns: Vec<Turn>,
    summarized_turns: Vec<TurnSummary>,
    ended_reason: Option<SessionEndReason>,
}

struct Turn {
    index: usize,
    user_input: String,
    context_assembly: ContextAssembly,
    retrieved_memory_block: String,
    assistant_response: String,
    recalled_turns: Vec<RecallRecord>,
    model_id: String,
    model_latency_ms: u64,
    input_tokens: u32,
    cached_input_tokens: u32,
    output_tokens: u32,
    full_request_hash: ContentHash,
    message_count: usize,
}

struct TurnSummary {
    turn_index: usize,
    summarized_after_turn_index: usize,
    summary: String,
    model_id: String,
}

struct RecallRecord {
    call_id: String,
    turn_id: usize,
    tool_name: String,
    verbatim_text: String,
    latency_ms: u64,
}
```

The real structs include timestamps and additional reducer bookkeeping. See the
runtime source for exact fields.

## Prompt Assembly Contract

Normal active-turn order:

```text
system: stable session prompt plus warm summaries, if any
user: prior active turn 0 rendered with frozen retrieved memory
assistant: prior active turn 0 response
...
user: current input rendered with current retrieved memory
```

Recall-augmented prior-turn order:

```text
user: prior active turn rendered with frozen retrieved memory
tool: frozen recall_turn result message(s)
assistant: prior active turn response
```

Warm summaries are appended to the system message as an "earlier in this session"
block. That intentionally invalidates the prompt prefix when ageing occurs. After the
ageing event, prefix stability resumes from the new prompt shape.

Recalled verbatim text is added as a tool message in the turn where the recall
happened. It then becomes part of future prompt prefixes.

## Configuration

| Env var | Default | Notes |
|---|---|---|
| `QSF_CONVERSATION_MODEL` | `gpt-5.4-mini` | Main responder model for this experiment. |
| `QSF_SESSION_MAX_TURNS` | `10` | Hard stop unless the override is enabled. |
| `QSF_SESSION_ALLOW_OVER_LIMIT` | `false` | Allows manual long-session experiments when set to `true`. |
| `QSF_SESSION_WARM_THRESHOLD` | `6` | Default exercises summarization before the ten-turn limit. |
| `QSF_SESSION_MEMORY_SOURCE` | `phase_four_fixture` | `file` selects a JSON `MemoryFixture`. |
| `QSF_SESSION_MEMORY_FILE` | unset | Required when memory source is `file`. |
| `QSF_MODEL_PROVIDER` | `mock` | `openai` must be explicit; an API key alone does not select OpenAI. |

## Completed Stage Stubs

### Stage 1: Hot Tier Only - Complete

Implemented append-only session state, cache-stable prompt assembly, per-turn memory
retrieval/context assembly, model usage telemetry, session lifecycle events, and the
mock-model integration test.

Verification completed:

- `cargo test multi_turn_text_loop --lib`
- `cargo build`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`

### Stage 2: Warm Tier (Summarization) - Complete

Implemented `QSF_SESSION_WARM_THRESHOLD`, session-local `TurnSummary` records,
`SessionTurnSummarizer`, warm-summary prompt rendering, intentional cache-prefix
invalidation after ageing, and report diagnostics.

Decision promoted:

- `2026-05-17 - Multi-turn warm tier ages by active turn count`

Verification included deterministic runs crossing the default warm threshold and live
OpenAI continuity testing in Stage 3 verification.

### Stage 3: Recall Tool - Complete For QSF Runtime And Mock

Implemented model-boundary tool definitions, `ModelToolCall`, `ModelMessageRole::Tool`,
`recall_turn(turn_id)`, `RecallRecord`, `ToolExecuted`, prompt reassembly after recall,
and deterministic mock-model recall coverage.

Accepted policy:

- `recall_turn` may return verbatim text only for summarized turns.
- Active turns are already in the prompt and cannot be recalled.
- Multi-round tool-call follow-ups fail without appending a turn.

Verification:

- Mock run `runs/2026-05-17-055619-multi-turn-text-loop` executed
  `recall_turn(0)` successfully.
- Live OpenAI run `runs/2026-05-17-061331-multi-turn-text-loop` verified multi-turn
  continuity and warm summaries, but not live recall execution.

Known limitation:

- The OpenAI adapter currently compiles with the expanded model boundary but does not
  forward provider-native tool definitions or parse provider-native tool calls.

## Stage 3.1: Provider-Native OpenAI Function Calling

### Status

Not started. Phase 0 pending.

Goal: make the live OpenAI-backed multi-turn loop exercise the same recall path as the
deterministic mock model by forwarding `recall_turn` as a provider-native tool,
parsing returned tool calls, sending tool result messages correctly, and measuring
real recall behavior.

### Open Questions To Surface Before Implementation

- Should provider-native tool support be added to `openai_provider_kit` directly, or
  should `qsf_app` temporarily bypass the kit for tool-capable OpenAI requests?
- Should the provider boundary stay chat-completions based for Stage 3.1, or migrate
  to a responses-style API if that is the current best-supported OpenAI function
  calling surface?
- What is the canonical provider-agnostic representation of a tool-result message?
  This splits into two sub-questions:
  - Does `ModelMessage` need a new `tool_call_id` field, a sibling type, or does the
    linkage stay on `ModelToolCall.call_id` and the message remains `{role, content}`?
  - Should tool-result messages use a structured type distinct from `ModelMessage`,
    or extend the existing message struct with a `tool_call_id`?
- Should unsupported providers reject requests containing tools, or silently degrade
  to text-only behavior? The current recommendation is to fail loudly for explicit
  tool requests.
- Should `allowed_tools` on `ModelRole` become enforced at `invoke_model_role` time
  before provider dispatch? Today `allowed_tools` is set on the role but never
  enforced — the runtime passes the tool list via `with_tools(...)` instead. Answer
  in Phase 0 or Phase 1 and promote to `docs/DecisionLog.md`. If the answer is "no
  enforcement," the field should be removed or documented as advisory.

### Phase 0: Provider Surface Check

Task:

- Inspect current official OpenAI API documentation for the recommended function
  calling surface and exact request/response payload shape.
- Inspect `openai_provider_kit` to decide whether it can be extended cleanly.
  The kit is pulled from a pinned external git rev (`ca28629`); extending it
  requires a fork-and-rev-bump cycle, while bypassing it duplicates auth, retry,
  and telemetry logic inside `qsf_app`. Record the chosen strategy and rationale
  in `docs/DecisionLog.md`.
- Commit to the `allowed_tools` enforcement policy (advisory-only, removed, or
  enforced at dispatch).

Verification:

- Notes in the implementation PR or review summarize chosen endpoint and payload
  shape.
- Decision Log entries for: kit extension vs bypass, chosen OpenAI surface, and
  `allowed_tools` enforcement policy.
- No code changes are accepted until this ambiguity is resolved.

External human testing:

- Not needed; this is source/API research.

### Phase 1: Extend Provider-Agnostic Model Types

Task:

- Add any missing fields needed to represent provider-native tool messages, likely
  `tool_call_id` or equivalent.
- Keep non-tool text requests unchanged.
- Ensure structured serialization remains safe for event and trace logs.

Verification:

- Unit tests for serializing/deserializing `ModelToolDefinition`, `ModelToolCall`, and
  tool result messages.
- Existing mock and non-tool OpenAI compile tests still pass.
- Test that two sequential prompts following a live recall yield identical prefix
  hashes through the recall turn. A live provider assigns `tool_call_id` non-
  deterministically; the freshly-arrived id must be frozen into the `Turn` and
  the hashing must treat it as opaque-but-recorded so the next turn's prefix is
  still stable.

External human testing:

- Not needed.

### Phase 2: Add OpenAI Request Serialization

Task:

- Serialize `ModelRequest.tools` into the selected OpenAI request payload.
- Serialize tool result messages with the provider's required role and call id.
- Preserve `max_completion_tokens`, temperature handling, response format handling,
  and prompt-caching telemetry.

Verification:

- Unit tests against JSON request bodies prove:
  - no `tools` field is emitted for text-only requests
  - `recall_turn` schema is emitted for tool-capable requests
  - tool result messages include the provider-required call id
  - existing gpt-5 max-token behavior is unchanged
- Test target is tied to the Phase 0 kit decision:
  - If extending the kit: struct-level assertions in `qsf_app` plus a kit-level
    wire test.
  - If bypassing the kit: JSON snapshot tests inside `qsf_app`.

External human testing:

- Not needed.

### Phase 3: Parse OpenAI Tool Calls

Task:

- Parse provider tool-call responses into `ModelToolCall`.
- Preserve text responses as the normal path.
- Treat malformed tool-call arguments as provider response errors with sanitized
  diagnostics.
- Preserve finish reason and usage telemetry.

Verification:

- Unit tests for:
  - text-only response
  - single `recall_turn` call
  - multiple tool calls
  - invalid JSON arguments
  - missing tool call id
  - cached-token parsing still works

External human testing:

- Not needed.

### Phase 4: Integrate With Multi-Turn Runtime

Task:

- Remove the current OpenAI TODO fallback that maps `ModelMessageRole::Tool` to
  `ChatRole::User` in `map_message_role`. Delete or rewrite the
  `message_role_mapping_matches_provider_roles` test that asserts the lossy
  fallback.
- Ensure `ModelRoleRequested` records when tools are actually sent to OpenAI.
- The recall turn must emit events in this order (additions to the existing
  text-only path in **bold**):
  1. `ContextAssembled`
  2. `PromptAssembled` (initial)
  3. `ModelRoleRequested` (with tools)
  4. **`ToolRequested`** (per tool call)
  5. **`ToolExecuted`** (per successful recall)
  6. **`PromptAssembled`** (second, after tool messages added)
  7. **`ModelRoleRequested`** (follow-up, without tools)
  8. `TurnCompleted`
- The existing `ToolFailed` path applies when live recall execution fails; a
  failed tool call does not append the turn, consistent with Stage 3 policy
  (line 192).
- If a provider returns a tool call when no tools were sent, fail the turn and log
  enough provider context for diagnosis.

Verification:

- Existing mock Stage 3 tests still pass.
- Add an OpenAI-adapter unit test using mocked HTTP responses for a full
  tool-call-then-tool-result exchange.
- `cargo test multi_turn_text_loop --lib`
- `cargo build -p qsf_app --features openai`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`

External human testing:

- Not yet; this phase can use mocked provider responses.

### Phase 5: Live OpenAI Recall Smoke Test

Task:

- Run a real OpenAI multi-turn session that forces summarization and then asks for
  exact older details strongly enough that the model should call `recall_turn`.
- Record whether the model calls the tool, how many round trips occur, added latency,
  and whether the final answer uses recalled verbatim content.

Suggested manual command:

```powershell
$env:OPENAI_API_KEY = "<key>"
$env:QSF_MODEL_PROVIDER = "openai"
$env:QSF_CONVERSATION_MODEL = "gpt-5.4-mini"
$env:QSF_SESSION_WARM_THRESHOLD = "2"
cargo run -p qsf_app --features openai -- experiment multi-turn-text-loop
```

Suggested prompts:

```text
My exact project code phrase is blue copper lantern.
The reducer rule is that side effects return as events before state changes.
I prefer concrete verification over vague summaries.
Please answer using the exact project code phrase from the older summarized turn.
:quit
```

Verification:

- The warm-summary block rendered by `format_system_prompt` already includes
  per-summary turn ids (`"- Turn {turn_index}: {summary}"`). Confirm this is
  sufficient for the model to issue `recall_turn(turn_id=…)` before the smoke
  test.
- If `ToolRequested` is present in `events.jsonl`:
  - `events.jsonl` contains at least one `ToolExecuted`.
  - The recall turn contains a non-empty `recalled_turns` list.
  - The follow-up model answer includes or correctly uses the recalled exact
    phrase.
  - The report records recall count and per-call latency.
- If `ToolRequested` is absent: the report records this as a model-behavior finding
  (not a code defect) and proposes a follow-up (system-prompt change, stronger tool
  description, or a different model).
- Failed live runs preserve the provider error chain without credentials.
- Cost caution: each recall round trip roughly doubles input tokens for the affected
  turn. Keep `QSF_SESSION_MAX_TURNS` at its default (10) during the smoke test.

External human testing:

- Required. A human should judge whether recalled verbatim context improved the answer
  and whether the model overused or underused the tool.

### Stage 3.1 Report

Write or update a report under `docs/Experiments/` covering:

- provider endpoint and payload choice
- mock-provider and mocked-HTTP test coverage
- live OpenAI recall success/failure
- recall-use frequency
- latency cost of tool round trips
- whether recalled context improved answers
- open questions for session-aware retrieval or persistence

## Future Candidate Phases

### Session-Aware Retrieval

Use recent active turns and warm summaries to bias the retrieval query instead of
retrieving from the latest user input only.

Verification should compare selected memory IDs and answer quality against the current
latest-input-only retrieval path.

### Session Persistence And Resume

Persist `SessionState` so a session can resume after process exit without losing
turns, summaries, hashes, or recall records.

Verification should replay a saved session and prove that prompt prefix hashes remain
stable after reload.

### Voice Adoption

Let the text-owned voice loop adopt the same session-state abstraction after the text
path is stable.

## Remaining Open Questions

- Should total warm ageing eventually be token-pressure based, turn-count based, or
  both?
- Should multiple age-outs be batched into one cache-prefix invalidation?
- Should session-aware retrieval use the existing memory retrieval path with an
  enriched query, or a parallel session-retrieval module?
- How does between-turn reflection fit, and is it a separate experiment or an option
  on this one?
- Should live recall ever be allowed for non-summarized turns, or is the current
  summarized-only decision permanent?
- Should session-local summaries or recall records ever become candidates for the
  reviewed durable-memory pipeline?

## Decisions Already Promoted

- `2026-05-09 - Unidirectional event-reducer-state flow`
- `2026-05-11 - Model access uses explicit roles and optional provider adapters`
- `2026-05-17 - Multi-turn warm tier ages by active turn count`
- `2026-05-17 - Multi-turn recall is scoped to summarized turns`

## Refs

- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
- `crates/qsf_app/src/conversation/prompt.rs`
- `crates/qsf_app/src/models`
- `docs/Architecture/Architecture.RuntimeLoop.md`
- `docs/Architecture/Architecture.ContextManagement.md`
- `docs/Architecture/Architecture.MemorySystem.md`
- `docs/Concepts/Concept.MultiModelMind.md`
- `docs/DecisionLog.md`
- `docs/Experiments/Report.MultiTurnTextLoop.Stage3.2026-05-17.md`
- `docs/Plans/Idea.VolitionGoalSystem.md`
- `docs/Plans/Idea.SelfReflectionProjectIntrospection.md`
- `docs/Plans/Idea.LiveActivationDashboard.md`
