# Design: Cross-Session Continuity

Status: Draft  
Maturity: Sketch  
Area: Memory system, Sleep phase, Session state

## Purpose

This document specifies the design for the first cross-session continuity slice of
Qualia Signal Foundry. The goal is to make a returning session *feel different from*
a new one, by persisting `SessionState` across runs, by allowing the sleep phase to
auto-promote structurally-valid memory candidates into a cross-session memory
store, and by letting the live loop form associations between co-retrieved
memories during a turn.

Cross-session continuity is currently the single biggest gap against the project
thesis (`docs/ProjectFrame/ProjectVision.md`). Almost all the substrate already
exists — the `MemoryRecord` and `Association` schemas, the reviewed-memory draft
pipeline, the file-backed memory source, the sleep-phase session-summary pipeline.
This plan connects those pieces.

## Scope

In scope for this slice:

- Persist `SessionState` for the multi-turn text loop across runs
- Two resume modes — *awake continuation* if no sleep ran, *consolidated brief*
  if sleep ran
- Sleep auto-promotes structurally-valid memory candidates and cross-turn
  associations into a cross-session memory store
- Live loop creates/strengthens associations between co-retrieved memories during a
  turn
- Time-based, idempotent memory decay computed at retrieval time
- Make the `openai` Cargo feature permanent (companion task; mechanical but
  broad — see `openai` feature removal section for scope)

Out of scope for this slice (named explicitly so the plan does not drift):

- Voice loop changes — the voice loop has no `SessionState` module today and
  unifying it with the text loop is its own design problem; a follow-up plan
  picks that up
- Embedding-based deduplication of promoted memory candidates
- A retrieval-and-use signal (reinforcement fires on retrieval, not on
  "the model actually used this memory")
- A retention policy for `state/text-loop/archive/`
- SQLite-backed storage — kept as a future migration target; the file layout
  is designed to map cleanly to tables
- Live-loop *new memory creation* (this slice covers association create/strengthen
  and reinforcement only)
- Automatic session-end sleep triggering — sleep remains manual
- Cross-loop unified `SessionState`

## Background and constraints

### The 2026-05-16 decision

The decision *"Sleep-to-memory conversion is explicit and separate"* explicitly
blocks implicit promotion. This plan reverses that decision in a scoped way:
sleep auto-promotes structurally-valid candidates, except `Decision`-kind
candidates which continue to require explicit review through the
`accept-reviewed-memory` experiment.

The reversal must be recorded in `docs/DecisionLog.md` as part of this plan's
delivery.

### The 2026-05-10 schema-versioning decision

`MemoryRecord` and `Association` each carry an independent `schema_version: u16`
field. The cross-session memory store reuses these existing schemas. This plan
makes one pure additive change: it adds `last_reinforced_at: Option<OffsetDateTime>`
to `MemoryRecord`. Per the 2026-05-10 decision, *"pure additive changes (new
optional fields with serde defaults) do not bump the version"*, so
`MEMORY_RECORD_SCHEMA_VERSION` stays at 1. Existing v1 records deserialize with
`last_reinforced_at = None`; the decay formula falls back to `created_at` when
the field is absent. `Association` already has `last_reinforced_at` as a
required field today, so it is unchanged.

A future cross-run store may use a different policy; this plan does not trigger
that.

### Architecture constraints carried forward

- Unidirectional `input -> action -> reducer -> state -> render` flow
  (2026-05-09 decision) — preserved by emitting events for memory-store deltas
  and handling persistence in isolated effect handlers
- Reducers stay pure and unit-testable
- `state/` is runtime persistence only; it is *not* a reflection input
  (reflection accesses only the curated project documents listed by the
  self-reflection plan)
- `runs/` is for inspectable per-run artifacts and is regularly cleaned;
  persistent state lives separately

## Architecture overview

### Storage layout

A new gitignored directory at repo root:

```
state/text-loop/
  continuity-manifest.json   # current session id, latest sleep id, resume mode, schema_version
  session-state.json         # persisted SessionState (overwritten at session-end)
  memory-store.json          # cross-session memory store (records + associations)
  consolidated-brief.json    # produced by sleep; consumed on next session boot
  archive/
    session-<id>.json        # written at session-end before overwriting session-state.json
    sleep-<id>.json          # historical consolidated briefs
```

Rationale for two top-level state files instead of one envelope:
`memory-store.json` is also the file pointed to by
`QSF_SESSION_MEMORY_SOURCE=file`, so it reuses the existing file-backed memory
source. `SessionState` has a different lifecycle (overwritten each session) than
memory (append/refine). Splitting matches lifecycle.

All writes go through a temp-file-then-rename pattern. **Windows-safe atomic
replacement primitive:** this plan uses `tempfile::NamedTempFile::persist`
(the `tempfile` crate already exists in the workspace as a transitive
dependency for tests; promote it to a direct dependency here). On Windows,
`tempfile` uses `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING` and is the
recommended primitive for atomic file replacement. A focused test confirms
replacement over an existing destination on Windows.

A `QSF_STATE_DIR` environment variable overrides the default `state/` location
so tests and concurrent experiments can use isolated state.

### Commit protocol for multi-file writes

Per-file atomic rename is *not* a cross-file transaction. A crash mid-way
through sleep could leave inconsistent state — for example, an updated
`memory-store.json` but a manifest still pointing to the old session as
unconsolidated, which would cause double-promotion on the next sleep.

This plan addresses that by making the **manifest the commit record**, written
last in every multi-file sequence, combined with **idempotent re-execution**:

1. Sleep first writes all derived files (`memory-store.json` updates,
   `consolidated-brief.json`, archive entries, plus the existing reviewed-memory
   draft used for `Decision`-kind candidates — see auto-promotion section)
2. Sleep writes the manifest last, flipping `sleep_pending = false` and
   recording `last_sleep_consumed_session_id`
3. Sleep is a pure function of `(SessionState, current memory-store)`. If sleep
   crashes after step 1 but before step 2, the next sleep invocation re-derives
   the same outputs from the same inputs and writes them again. Idempotent
   re-execution recovers from partial writes.
4. The manifest records `last_sleep_consumed_session_id`. Sleep no-ops if that
   session was already consolidated (manifest's recorded id matches the
   session-state's id and `sleep_pending == false`).

Recovery test (added to Phase 4 verification): inject a fault between steps 1
and 2 and rerun sleep — final state matches a clean sleep run.

Live-loop association appends use the same `tempfile::persist` primitive.
Because the manifest's `sleep_pending` does not change during live-loop turns,
no cross-file commit protocol is needed for live writes.

### Continuity manifest

`continuity-manifest.json` fields:

- `schema_version: u16` — starts at 1
- `current_session_id: String`
- `current_session_state_path: PathBuf`
- `last_sleep_run_id: Option<String>`
- `last_sleep_brief_path: Option<PathBuf>`
- `sleep_pending: bool` — true after session-end, false after the next sleep
  consumes that session. Semantics: *"the most recent persisted session has
  not been consolidated yet."* This single flag drives the resume-mode
  decision (see below); no timestamp comparison is needed.
- `last_sleep_consumed_session_id: Option<String>` — the `session_id` that the
  most recent sleep consumed. Used by sleep to detect already-consolidated
  state and no-op cleanly on re-execution
- `resume_mode: ResumeMode` — recomputed on read; persisted for inspection

### Resume modes

The multi-turn text loop boot path splits resume resolution into a thin I/O
wrapper and a pure classifier:

- `resume::load_resume_inputs(state_dir) -> ResumeInputs` — reads the manifest
  and SessionState files from disk
- `resume::classify_resume_mode(inputs: &ResumeInputs) -> ResumeMode` — pure,
  unit-testable, no I/O

The classifier returns one of:

- `ColdStart` — manifest missing, empty, or `session-state.json` missing.
  Behaves like today's text loop: fresh `SessionState`, default memory source.
- `AwakeContinuation` — manifest and `session-state.json` exist AND
  `sleep_pending == true`. The most recent session ended but was not yet
  consolidated; we treat the new run as a continuation. The previous
  `SessionState` is loaded but passed through a pure
  `prepare_awake_continuation(state, new_config) -> SessionState` function
  that:
  - **carries forward**: `session_id`, `started_at`, `turns`,
    `summarized_turns`, `config` (compared against `new_config`; a config
    mismatch falls back to `ConsolidatedBrief`-style fresh start)
  - **clears**: `ended_reason`, `last_model_error`, `last_input`
  - **recomputes**: `limit_reached` against the new config (a session that
    ended at the previous turn limit might now be under or over the new
    limit)
  - **resets**: `last_prompt_hash = None` and
    `prefix_invalidated_since_last_prompt = true` (the new run is a different
    process; the prefix cache cannot be assumed warm)
  The text-loop run id rotates (new run directory) but the persisted
  `SessionState.session_id` stays the same. Unit tests cover resuming after
  quit, after EOF, after model error, and after session-limit reached.
- `ConsolidatedBrief` — manifest and `session-state.json` exist AND
  `sleep_pending == false` AND `last_sleep_run_id` is present. The most
  recent session was consolidated by a sleep run. The previous `SessionState`
  is *not* loaded. The consolidated brief is injected as a small first-turn
  context fragment. The memory store loads with all the sleep-promoted
  records. `SessionState` starts fresh with `previous_session_id` recorded.

These three modes are mutually exclusive. `sleep_pending` is the single
authoritative signal that distinguishes AwakeContinuation from
ConsolidatedBrief; no timestamp comparison is involved in the resume decision.

A new event type `SessionResumed { mode, previous_session_id, brief_path }` is
emitted and surfaces in the run's markdown report. The resume decision is
inspectable without reading the manifest directly.

Default-exercising rule (Agents.md): the default config loads the manifest if
it exists. `ColdStart` only fires when no state exists — so the second-ever run
in a fresh checkout exercises the new code path.

### Sleep auto-promotion

Sleep inputs extend today's flow:

- The persisted `SessionState` (not just transcript text — so sleep can reason
  about turn indices and which memories were retrieved per turn)
- The current `memory-store.json` (so sleep avoids duplicate-promoting and can
  strengthen existing records)

Sleep outputs are written under the commit protocol above (derived files
first; manifest last). Concretely:

1. **Append promoted memory records to `memory-store.json`.** The existing
   `SleepReport` schema has two separate top-level fields,
   `memory_candidates: Vec<SleepMemoryCandidate>` and
   `decision_candidates: Vec<String>`. This plan treats them differently:
   - **`memory_candidates`** are auto-promoted as `MemoryRecord` with
     `kind = MemoryRecordKind::Observation` (matching what
     `reviewed_memory_draft::convert_memory_candidate` already does today).
     `SleepMemoryCandidate` has no `kind` field; introducing model-driven
     kind classification is out of scope for this slice. A candidate is
     auto-promoted when:
     - Structural validation passes (non-empty summary, clamped `importance`
       — same checks `reviewed_memory_draft.rs` already performs)
     - No near-duplicate exists in the store. *This slice uses normalized
       string match on title+summary; embedding-based dedup is a follow-up.*
   - **`decision_candidates`** are routed through the existing
     `reviewed-memory-draft` pipeline. The sleep experiment produces a
     `ReviewedMemoryDraft` in its sleep-run directory containing the
     decision candidates as `MemoryRecord` entries with
     `kind = MemoryRecordKind::Decision`. The researcher accepts or rejects
     these through the existing `accept-reviewed-memory` experiment exactly
     as today. No new dead-end JSON is introduced in `state/`. This
     preserves the spirit of the 2026-05-16 boundary for the highest-stakes
     record category while letting routine observations flow through
     automatically.
2. **Form cross-turn associations.** Sleep scans the session's turns and, for
   any pair of memories retrieved within a configurable turn-distance window
   (default 3 turns), creates or strengthens an association tagged
   `cross-turn co-occurrence in session <id>`. Reinforcement timestamps on
   associations use the session's `as_of` time (see below), not `now`.
3. **Write `consolidated-brief.json`** with:
   - `previous_session_summary` (one paragraph, from the sleep report)
   - `future_context_hints` (already produced today)
   - `open_questions` (already produced today)
   - `promoted_count`, `new_associations_count` for inspection
4. **Update the manifest last** — `last_sleep_run_id`, `last_sleep_brief_path`,
   `last_sleep_consumed_session_id = SessionState.session_id`,
   `sleep_pending = false`. This is the commit step.

**No `decay_recomputed_at` field is persisted.** Decay is computed at retrieval
time as a function of `last_reinforced_at` and the retrieval-time clock.
Persisting a wall-clock decay timestamp would break byte-idempotency for no
benefit.

**Deterministic `as_of` for sleep writes.** All sleep-side timestamp writes
(association reinforcement, any future timestamped sleep output) use
`as_of = max(turn.completed_at for turn in SessionState.turns)`, not `now`.
This is purely a function of the input SessionState. Consequence: **running
sleep twice in a row on the same SessionState produces byte-identical state
files**, because no input has changed. The Phase 4 verification asserts this
directly with a diff on every affected state file, not just `memory-store.json`.

### Decay formula and retrieval integration

`retrieval::score_record` today produces a `RetrievalScore` with components
`recency`, `keyword`, `tag`, `association`, `importance`, `reinforcement`. The
existing `recency` component is rank-based (the i-th newest record by
`created_at` gets `(N-i)/N`). This plan **replaces** the rank-based recency
with a time-based decay against `last_reinforced_at`, keeping the score struct
shape unchanged:

```
recency = exp(-age_days / DECAY_HALFLIFE_DAYS)

age_days = (now - last_reinforced_at.unwrap_or(created_at)).as_days()
```

Starting default: `DECAY_HALFLIFE_DAYS = 30`. The `reinforcement` component
remains the existing capped linear `(reinforcement_count.min(5)) / 5.0`. The
total-score weighting per strategy (`AssociationWeighted`, `KeywordTag`,
`RecencyOnly`) is unchanged. No new score component is introduced.

`last_reinforced_at` is the new `Option<OffsetDateTime>` field added to
`MemoryRecord`. Serde shape:

```rust
#[serde(default, with = "time::serde::rfc3339::option")]
pub last_reinforced_at: Option<OffsetDateTime>,
```

When absent (existing v1 records that predate this plan), the formula falls
back to `created_at`. A regression test deserializes a fixture-shaped v1 record
without the field and asserts the fallback. Reinforcement (live-loop retrieval
and sleep-side strengthening) sets `last_reinforced_at = Some(timestamp)`
where the timestamp source is `now` for live-loop writes and the session's
`as_of` for sleep-side writes (per the deterministic `as_of` rule above).
Sleep never mutates `reinforcement_count` downward; it only recomputes the
effective weight surface. The reversal Decision-Log entry will record these
defaults.

### Memory-store source-of-truth resolution

Today `build_session_memory_source_from_env()` in
`multi_turn_text_loop.rs` defaults to the hard-coded
`phase_four_fixture`, with `QSF_SESSION_MEMORY_SOURCE=file` switching to a
file. This plan refines that resolution:

1. If `state/text-loop/memory-store.json` exists, it is the source of truth
   for retrieval and the destination for live-loop reinforcement and
   co-retrieval writes.
2. If it does not exist (a true cold start in a fresh checkout):
   - The retrieval source falls back to whatever
     `QSF_SESSION_MEMORY_SOURCE` resolves to today (default fixture)
   - Live-loop reinforcement writes are **disabled** for that run with a
     trace entry — there is no persistent destination to write to. Writing to
     the fixture would conflict with deterministic test expectations.
3. The first sleep run on a fresh checkout creates `memory-store.json`,
   seeded with the auto-promoted candidates plus any associations formed
   during the session.
4. `QSF_SESSION_MEMORY_SOURCE=file` with an explicit `QSF_SESSION_MEMORY_FILE`
   continues to work — pointing at a path outside `state/text-loop/` puts
   the loop in a no-write mode for the same reason as (2).

The retrieved memory ids that drive live-loop reinforcement and co-retrieval
must belong to the persistent store; otherwise the in-memory deltas reference
ids that the store has never seen. This is enforced by sourcing both retrieval
and writes from the same `MemoryStore` handle.

### Live-loop co-retrieval

After `MemoryRetrieved`, a new pure function inspects the selected memory ids
and produces a list of association deltas. Deltas are emitted as events; the
reducer applies them; an effect handler writes them to `memory-store.json` at
turn end. Reducer purity is preserved: the reducer folds deltas into an
in-memory association table; the disk write is an isolated side effect fed back
as a `MemoryStorePersisted` event.

Two delta types:

1. **`CoRetrievalAssociationDelta`** — for every unordered pair `(a, b)` in
   the retrieved set with `a != b`:
   - If an association already exists in either direction →
     `Strengthen { from, to, reason_addendum: "co-retrieved in turn N" }`
   - Otherwise →
     `Create { from, to, initial_weight: CO_RETRIEVAL_INITIAL_WEIGHT,
       reason: "co-retrieved in turn N of session X" }`
   - The reducer caps the number of new associations created per turn (default
     5). Excess pairs strengthen-only; no creates. The cap requires a
     **deterministic ordering** of candidate pairs to preserve replayability:
     pairs are ordered by descending joint retrieval score (sum of the two
     memories' retrieval scores), tie-broken lexicographically on
     `(from_memory_id, to_memory_id)` with `from_memory_id < to_memory_id`.
     The top `MAX_NEW_ASSOCIATIONS_PER_TURN` create; the rest strengthen if a
     prior association exists, otherwise are dropped with a trace entry.
2. **`ReinforcementDelta`** — for each retrieved memory id, bump
   `reinforcement_count` and update `last_reinforced_at = now`.

The disk write happens via temp-file-then-rename at most once per turn (and
only if deltas were produced). A session that crashes mid-turn cannot corrupt
the store; the store reflects only fully-completed turns.

### Constants (default-exercising)

- `CO_RETRIEVAL_INITIAL_WEIGHT = 0.3`
- `MAX_NEW_ASSOCIATIONS_PER_TURN = 5`
- `CO_RETRIEVAL_STRENGTHEN_DELTA = 0.05`
- `CROSS_TURN_ASSOCIATION_WINDOW = 3`
- `REINFORCE_BOOST = 0.1`
- `DECAY_HALFLIFE_DAYS = 30`

All configurable through env vars. Defaults are chosen to exercise the new code
path on the default test fixtures.

### New events and traces

The existing event log uses a flat `EventType` enum with a separate
`payload: serde_json::Value`. New variants added to `EventType`:

- `SessionResumed`
- `CoRetrievalAssociationsProposed`
- `MemoryReinforced`
- `MemoryStorePersisted`

Payload schemas (JSON):

```
SessionResumed:
  { mode: "ColdStart" | "AwakeContinuation" | "ConsolidatedBrief",
    previous_session_id: Option<String>,
    brief_path: Option<String> }

CoRetrievalAssociationsProposed:
  { turn_index: usize,
    proposed_count: usize,
    created_count: usize,
    strengthened_count: usize,
    dropped_count: usize }

MemoryReinforced:
  { ids: Vec<String>,
    count: usize,
    timestamp_source: "live_now" | "sleep_as_of" }

MemoryStorePersisted:
  { path: String,
    records_count: usize,
    associations_count: usize }
```

New trace records:

- Resume-decision trace (which manifest fields drove the choice)
- Auto-promote trace (which candidates were promoted, which were skipped and why)

### `openai` feature removal

The `[features] openai` gate is removed from `crates/qsf_app/Cargo.toml`; the
dependencies move to unconditional `[dependencies]`. All
`#[cfg(feature = "openai")]` gates are stripped from `crates/qsf_app/src/`.
This is **mechanical but broad** — 4 files carry the gates today, with
~50 cfg sites across `audio/voice_session_provider.rs` (18),
`audio/transcript_provider.rs` (23+), `models/openai_provider.rs` (3), and
`models/mod.rs` (1). The work also touches the workspace `Cargo.toml`, the
README setup section, and any architecture doc that mentions the flag.

The runtime still defaults to mock provider via `QSF_MODEL_PROVIDER` — the
2026-05-11 decision stands: an API key alone does not switch behavior.

The README is updated; the `--features openai` flag goes away. A short
Decision-Log entry records the change. Phase 1 is its own separable
checkpoint so this scope risk does not entangle the persistence work.

## File touchpoints

These are anticipated; the implementation plan will refine the list.

New modules:

- `crates/qsf_app/src/session/persistence.rs` — serialize/deserialize
  `SessionState`, atomic write via `tempfile::NamedTempFile::persist`
- `crates/qsf_app/src/session/resume.rs` — `load_resume_inputs` (I/O wrapper)
  plus pure `classify_resume_mode` and `ResumeMode` enum
- `crates/qsf_app/src/session/continuation.rs` — pure
  `prepare_awake_continuation(state, new_config) -> SessionState`
- `crates/qsf_app/src/session/manifest.rs` — `ContinuityManifest` type,
  read/write, `schema_version`
- `crates/qsf_app/src/memory/store.rs` — wraps `memory-store.json`
  read/append/atomic-write; resolves source-of-truth per the resolution rules
- `crates/qsf_app/src/memory/co_retrieval.rs` — pure delta-generation logic
  with deterministic ordering
- `crates/qsf_app/src/sleep/auto_promote.rs` — pure candidate-promotion filter
  and cross-turn association builder; uses session `as_of` for timestamps
- `crates/qsf_app/src/sleep/commit.rs` — multi-file commit protocol helper
  (derived files then manifest)

Modified modules:

- `crates/qsf_app/src/session/mod.rs` — add `session_id`,
  `previous_session_id` fields
- `crates/qsf_app/src/memory/memory_record.rs` — add
  `last_reinforced_at: Option<OffsetDateTime>` (additive; no schema-version
  bump per 2026-05-10 decision)
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` — wire boot
  resolver, emit new events, write store at turn end
- `crates/qsf_app/src/experiments/sleep_phase_session_summary.rs` — extend to
  consume `session-state.json`, write to memory store + brief via the
  commit-protocol helper, and emit a `ReviewedMemoryDraft` with
  `kind = Decision` for any decision candidates in the sleep report
- `crates/qsf_app/src/memory/retrieval.rs` — replace rank-based `recency`
  component in `score_record` with time-based decay against
  `last_reinforced_at`; preserve `RetrievalScore` shape
- `crates/qsf_app/Cargo.toml` — `tempfile` becomes a direct dependency (was
  transitive)
- `crates/qsf_app/src/observability/event_log.rs` — new event variants
- `crates/qsf_app/src/observability/trace.rs` — new trace records
- `crates/qsf_app/Cargo.toml` — remove `openai` feature
- `.gitignore` — add `state/`

## Phasing and verification

The plan will be split into five phases. Each phase has automated verification
and a human-testable golden path (Agents.md: "make it clear how to verify each
step").

**Phase 1 — `openai` feature removal.**
- Auto: `cargo build`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test` all pass without `--features openai`
- Human: `cargo run -p qsf_app -- list-experiments` lists all experiments

**Phase 2 — Memory store + decay + retrieval integration.**
- Auto: pure unit tests for `decay::effective_weight`, `memory::store`
  round-trip, idempotent serialization
- Human: existing multi-turn text loop runs unchanged because `state/` doesn't
  exist yet (validates `ColdStart` path is non-breaking)

**Phase 3 — `SessionState` persistence + boot resolver + AwakeContinuation.**
- Auto: unit tests for `classify_resume_mode` against synthesized
  `ResumeInputs`; `SessionState` serde round-trip; atomic replacement test
  over an existing destination on Windows
- Auto: `prepare_awake_continuation` tests covering resume after quit, EOF,
  model error, and session-limit reached (each case asserts which fields
  carry forward, are cleared, or are recomputed)
- Auto: regression test deserializing a fixture-shaped v1 `MemoryRecord`
  without `last_reinforced_at`, asserting fallback to `created_at`
- Human golden path: run multi-turn text loop, exchange a couple of turns,
  quit cleanly. State files written. Re-run; second session loads previous
  turns; `summarized_turns` and `turns` are non-empty on turn 1.
- Human edge: delete `state/text-loop/`; re-run; `ColdStart` works.

**Phase 4 — Sleep auto-promotion + consolidated brief + ConsolidatedBrief resume.**
- Auto: unit tests for `sleep::auto_promote` filter (structural validation,
  near-duplicate dedup); `decision_candidates` routed through a
  `ReviewedMemoryDraft` with `kind = Decision`; cross-turn-association window
  logic
- Auto idempotency: run the sleep consolidation twice against the same
  `state/` directory; assert **byte-identical diffs on every affected file**
  — `memory-store.json`, `consolidated-brief.json`,
  `continuity-manifest.json`, and any archive entries
- Auto partial-write recovery: simulate a crash between sleep's derived-file
  writes and the manifest commit (e.g., drop the test harness mid-write);
  rerun sleep; assert final state matches a clean sleep run
- Human golden path: run a session; run sleep; run a second session. Resume
  mode logs `ConsolidatedBrief`. Memory store contains sleep-promoted
  `Observation` records. A `ReviewedMemoryDraft` with `kind = Decision` is
  present in the sleep run dir for any decision candidates; running
  `accept-reviewed-memory` against it works unchanged. Brief content appears
  in turn 1's context.

**Phase 5 — Live-loop co-retrieval associations + reinforcement.**
- Auto: reducer tests for delta generation, strengthen-vs-create branching,
  per-turn cap
- Human: run a multi-turn session where retrieval likely returns ≥2 memories
  per turn. After session, `memory-store.json` contains new associations with
  `reason` strings tagged `co-retrieved in turn N`.

## Documentation updates required

Per `docs/ProjectFrame/ProjectWorkflow.md`:

- `docs/EngineeringDiary.md` — one entry per phase as it lands; each entry
  references this plan and the relevant Decision-Log entries
- `docs/DecisionLog.md` — two new entries:
  1. Refinement of 2026-05-16 — sleep auto-promotes `memory_candidates` from
     `SleepReport` as `Observation` records; `decision_candidates` continue to
     require manual review through `accept-reviewed-memory`. The entry must
     explicitly reference the original 2026-05-16 entry.
  2. `openai` feature removal — real-provider code unconditional; provider
     selection remains explicit
- `docs/Architecture/Architecture.MemorySystem.md` — Implementation Status
  moves cross-session store, decay, reinforcement, association building from
  "not yet implemented" → implemented; `Last reviewed:` bumped
- `docs/Architecture/Architecture.SleepPhase.md` — Implementation Status
  reflects auto-promotion and cross-turn associations; `Last reviewed:` bumped
- `docs/Architecture/Architecture.StateAndObservability.md` — Session State
  gains cross-session lifetime; new event types listed; `Last reviewed:`
  bumped
- `docs/Plans/Plan.CrossSessionContinuity.md` — the implementation plan (this
  document's successor)
- `docs/Experiments/Experiment.CrossSessionContinuity.md` — recommended: the
  session/sleep/session golden-path human test as a documented experiment so
  it produces a Report once exercised
- `docs/Plans/Idea.VoiceLoopUnification.md` — stub that names this plan as a
  prerequisite

## Open questions

These remain open and should be re-examined during implementation or via
follow-up experiments:

1. **Decay defaults.** `DECAY_HALFLIFE_DAYS = 30` and `REINFORCE_BOOST = 0.1`
   are guesses. The first real cross-session run will inform tuning. Worth a
   follow-up experiment.
2. **Co-retrieval initial weight.** `0.3` is only 0.1 above the manual-draft
   threshold `MIN_DRAFT_ASSOCIATION_WEIGHT = 0.2`. Revisit after observing live
   runs.
3. **Model-driven memory kind classification.** Today all sleep-promoted
   memory records land as `Observation` because `SleepMemoryCandidate` has no
   `kind` field. A follow-up may extend the sleep summarizer to categorize
   candidates (Concept, ArchitectureNote, Question, Experiment), at which
   point the auto-promote filter may need to gate certain kinds. Out of
   scope for this slice.
4. **Archive retention.** `state/text-loop/archive/` grows unbounded. No
   retention policy in this slice; follow-up.
5. **State directory location.** Defaulting to `state/` at repo root. A
   `QSF_STATE_DIR` env override is included. The default could alternatively
   be inside `target/`; current bias is repo root because state should survive
   `cargo clean`.
6. **Cross-turn association window.** Default 3 turns. Sleep-only, not
   live-loop. Easy to change.
7. **Brief lifetime.** Once consumed by the next session, does
   `consolidated-brief.json` get archived or deleted? Proposed: moved to
   `archive/`. Open for review.
8. **Voice-loop unification follow-up.** This plan deliberately leaves the
   voice loop untouched. The follow-up plan must design a shared
   `SessionState` that handles voice's event-driven shape (interrupts, partial
   transcripts). Flagged as the next plan's open problem.

## What this design deliberately does not do

- No voice-loop changes (next plan)
- No embedding-based dedup (string match only)
- No retrieval-and-use signal (reinforcement fires on retrieval)
- No retention policy for `archive/`
- No SQLite (future option preserved by file layout)
- No live-loop new memory creation (only association create/strengthen and
  reinforcement)
- No automatic session-end sleep
- No cross-loop unified `SessionState`

## References

- `docs/ProjectFrame/ProjectVision.md` — project thesis on continuity
- `docs/Architecture/Architecture.MemorySystem.md` — current memory-system state
- `docs/Architecture/Architecture.SleepPhase.md` — current sleep-phase state
- `docs/Architecture/Architecture.StateAndObservability.md` — state taxonomy and
  observability requirements
- `docs/DecisionLog.md` — 2026-05-09 unidirectional flow, 2026-05-10 memory
  schema versioning, 2026-05-11 explicit provider selection, 2026-05-16
  sleep-to-memory boundary (to be reversed by this plan)
- `crates/qsf_app/src/session/mod.rs` — current `SessionState`
- `crates/qsf_app/src/memory/reviewed_memory_draft.rs` — existing structural
  validation logic the auto-promote filter reuses
- `crates/qsf_app/src/sleep/sleep_report.rs` — current sleep report schema
