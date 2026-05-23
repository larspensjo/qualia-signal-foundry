# Design: Associative Recall And Drop-Driven Associations

## Status

Draft

## Summary

This design refactors how the simulation uses associative memory so that retrieval
behaves more like human associative recall, and so that the cross-turn association
work moves out of sleep — leaving sleep free for non-obvious association work.

The shape is:

- During a live turn, retrieved memories trigger automatic expansion of their
  immediate neighbors via persisted `Association` edges. Neighbors enter the
  prompt as a **hint** block, clearly distinguished from direct retrievals.
- When hot context approaches its token budget, the live loop batch-drops the
  oldest contiguous turns and runs a **cross-turn 3-turn-window** co-retrieval
  pass over the dropping block plus a small overlap into still-hot turns. New
  or strengthened associations are persisted immediately.
- Session-end (`:quit` and similar clean exits) triggers the same cross-turn
  pass over the remaining hot turns so no turn dies un-co-retrieved.
- Sleep stops running its cross-turn pass as default. Its association work is
  reorganized behind a pluggable `AssociationProposer` interface so future
  non-obvious-association strategies (two-hop bridges, common-substring
  detection, cross-session co-retrieval) can be added as experiments without
  re-plumbing.
- The console gets two visual signals: color-coded direct/hint memory blocks,
  and a drop-event marker line. Voice output remains direct simulation answer
  only — no meta information.

The persistence shape of `Association` does not change.

**Reinforcement policy (clarified):** Weights only change via *co-retrieval*
events. Co-retrieval is live (today: per-turn pairwise; new in this design:
cross-turn on drop and session-end) and sleep (existing safety net via a
proposer, plus any new proposers). Simply *being retrieved into context* does
not strengthen edges — that is what "no reinforcement on use" means.

## Current State

This design slots into an existing live-loop mechanism that the design must not
break:

- **Same-turn pairwise reinforcement (live, every turn).** End of every turn,
  [multi_turn_text_loop.rs:571](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs#L571)
  calls `apply_live_memory_reinforcement`, which uses
  [memory/co_retrieval.rs](../../crates/qsf_app/src/memory/co_retrieval.rs)
  (`generate_deltas`, `CO_RETRIEVAL_STRENGTHEN_DELTA = 0.05`,
  `MAX_NEW_ASSOCIATIONS_PER_TURN = 5`). Every pair of memories retrieved in
  the same turn becomes a candidate edge; new edges are created up to the
  cap, existing edges are strengthened. Deltas are persisted immediately.
  **This mechanism is retained unchanged.**
- **Cross-turn 3-turn-window co-retrieval (sleep today).**
  `build_cross_turn_associations` in
  [sleep/auto_promote.rs:96-171](../../crates/qsf_app/src/sleep/auto_promote.rs#L96-L171)
  walks `session.turns[*].context_assembly.retrieved_memory_ids()` with a
  window of 3 and produces associations between memories retrieved in
  nearby-but-distinct turns. **This is what moves to the live loop on
  drop/session-end** (see [Batched Context Drops](#batched-context-drops)).
- **Default retrieval strategy.** `SESSION_RETRIEVAL_STRATEGY =
  RetrievalStrategy::AssociationWeighted`
  ([multi_turn_text_loop.rs:45](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs#L45)).
  The scoring already includes one-hop association weight from keyword
  seeds. This design switches the live-loop strategy to `KeywordTag` so
  retrieval and hint expansion together stay strict single-hop. See
  [Direct-Retrieval Strategy When Hints Are Active](#direct-retrieval-strategy-when-hints-are-active).

## Goals

- Make associative recall automatic during live turns: the model receives
  immediate-neighbor memories as hints without having to ask.
- Move mechanical association work (the co-retrieval window) into the live loop
  so sleep can focus on harder problems.
- Free sleep to host pluggable, experimental association proposers.
- Surface what is being injected into the prompt and when drops happen, on the
  console, in a way that complements (does not duplicate) the Live Activation
  Dashboard.

## Non-Goals

- No semantic / "reminded-of" associations without a stored edge. Only persisted
  `Association` neighbors are surfaced.
- No second-hop expansion in the live loop. Immediate neighbors only.
- No reinforcement-on-use. Association weights only increase during sleep.
- No new Live Activation Dashboard content. LAD continues to show subsystem
  activity, not memory contents.
- No voice output of meta information. Voice is for the direct simulation
  answer only.
- No implementation of new proposers beyond the existing LLM-candidate path and
  the safety-net co-retrieval proposer. New proposer strategies are parked in
  `docs/Plans/Ideas.AssociationProposers.md` as experiment candidates.
- No change to the `Association` persistence schema.

## Design Choices

### Associative Recall In The Live Turn

Pipeline for each user input:

```text
User input
  -> retrieve_memories(...)            // unchanged: keyword/tag scoring
  -> select top N as "directs"
  -> expand_neighbors(directs, store)  // NEW: pure function
                                       //      for each direct, find immediate
                                       //      Association edges (both directions);
                                       //      collect unique neighbors not already
                                       //      in directs
  -> assemble context with two labeled fragment groups
  -> format prompt with two clearly-labeled blocks
  -> send to model
```

A new `ContextSourceKind::MemoryHint` lives next to today's
`ContextSourceKind::Memory`. The existing `ContextAssembly` (budget, selection,
omission) handles hints uniformly with directs; the difference is in
formatting and budget priority. Each hint fragment records the direct memory
it expanded from, the association reason, and the weight.

The prompt formatter emits two blocks. Indicative wording (final text shifts
during implementation):

```text
=== Memories retrieved for this turn ===
- memory.foo  (matched: "decay", "halflife")
- memory.bar  (matched: "association")

=== Associated memories (hints — may or may not be relevant) ===
- memory.baz  (via memory.foo — "co-retrieved 2026-04-18")
- memory.qux  (via memory.bar — "both describe runtime architecture")
```

The model receives a single short instruction every turn explaining how to
read each block: directs are evidence retrieved for the current turn; hints
are loosely connected and should only be used if they help.

### Direct-Retrieval Strategy When Hints Are Active

In a single live retrieval pass, the system never reaches two hops.

- Direct retrieval uses `RetrievalStrategy::KeywordTag` (not
  `AssociationWeighted`) so association weight does not contribute an
  implicit hop during direct scoring.
- Hint expansion does the one explicit hop, populating the hint block.

Cross-turn two-hop reach is allowed but only **response-mediated**: if the
model's reply in turn N actually references hint memory X, then in
turn N+1 the query keywords pick up X directly (it is now in the
recent transcript), and *its* neighbors become hints. The second hop has
been earned by the simulation engaging with the first hint, not by
retrieval scoring leaking it through.

Implication: `SESSION_RETRIEVAL_STRATEGY` (and
`VOICE_MEMORY_RETRIEVAL_STRATEGY`) change from `AssociationWeighted` to
`KeywordTag` as part of Phase 1. `AssociationWeighted` remains available
in the codebase (`memory_and_context.rs` still exercises it as an
experiment surface) and remains the right strategy inside sleep when
sleep needs association-aware retrieval. The change applies to the
live-loop strategy constants only.

### Hint Budget

Hints share the same `ContextAssembly` budget as direct retrievals, with
safeguards.

`assemble_context` today sorts all fragments by `score` and selects greedily
([context_assembler.rs:36-44](../../crates/qsf_app/src/context/context_assembler.rs#L36-L44)).
A high-score hint would beat a lower-score direct in that scheme. Direct
priority is a hard rule of this design, so single-pass score sorting is
not sufficient.

**Two-pass assembly:** Phase 1 introduces a source-priority rule. Either:

- **Two-pass API**: `assemble_context_with_priority(directs, hints,
  budget)` runs direct fragments through assembly first, then runs hints
  against the remaining budget. Two `ContextAssembly` results are merged.
- **Source-priority comparator**: keep one `assemble_context` call but
  sort by `(source_priority, score)`, where `ContextSourceKind::Memory` >
  `ContextSourceKind::MemoryHint`.

The plan picks one. The contract a test must enforce: when budget cannot
fit all fragments, a hint cannot evict a direct.

Other rules:

- Hard cap: at most `MAX_HINTS_PER_TURN = 8` (default ON; tunable).
- Any hint that would push the assembly over budget is omitted, not
  truncated.
- A hint already present in directs is deduplicated.

At the start of a session and for several turns, the budget should not be
under pressure; the safeguards are there for cases where it is.

### Hint Expansion Direction

`expand_neighbors` is **undirected**: for a direct memory M, the hint pool
includes any association edge where M is either `from_memory_id` or
`to_memory_id`. The other endpoint becomes a hint candidate.

Rationale: from the simulation's recall perspective, the directional
semantics of `from`/`to` on a stored edge are mostly an artifact of how the
edge was created (e.g., the alphabetically-ordered pair from
`generate_deltas`). Neighbors-in-the-graph is the useful notion.

This differs from `association_paths_by_target`
([memory/retrieval.rs](../../crates/qsf_app/src/memory/retrieval.rs)) which
walks outgoing edges only for the `AssociationWeighted` scoring path —
that scoring code is unchanged; only the new `expand_neighbors` function
is undirected.

Tests cover three cases: incoming-only edge, outgoing-only edge, and a
reciprocal pair (both directions exist).

### Reinforcement Policy

Two distinct kinds of reinforcement exist in the code today and both are
preserved:

- **Record-level reinforcement (on use, existing).** Every memory selected
  by retrieval into a turn's context has its `reinforcement_count`
  incremented and `last_reinforced_at` updated. This affects retrieval
  recency decay
  ([memory/retrieval.rs::compute_recency_decay](../../crates/qsf_app/src/memory/retrieval.rs)).
  Hints DO count as "use" for this purpose — being included as a hint
  strengthens the record's recency and reinforcement, even though no
  association edge is touched.
- **Association-edge reinforcement (on co-retrieval, existing for
  same-turn; new in this design for cross-turn).** Edges only change
  weight via co-retrieval events:
  - Same-turn pairwise (live, every turn, existing).
  - Cross-turn 3-turn window (live, on drop and session-end, new).
  - Sleep proposers (safety-net cross-turn for ranges the live loop did
    not cover; future proposers).

What is explicitly OFF: simply being retrieved into context does not
strengthen *association edges*. Edge weight changes require co-occurrence
with another memory.

This preserves current code-level behavior and pins it down so it does not
drift.

### Batched Context Drops

"Drop" here means **aging turns out of the active verbatim prompt range**, not
removing `Turn` records from session state. The 2026-05-17 decision
*"Multi-turn warm tier ages by active turn count"* establishes that aged
turns remain available in records and reports but are skipped during
prompt assembly. This design extends that mechanism with a batched,
token-budget-driven trigger.

The live loop watches hot-context token usage (the verbatim prompt
contribution). When it crosses the high-water threshold at end-of-turn:

- Identify the oldest contiguous block of active verbatim turns whose
  aging brings hot context back below a low-water mark.
- Age that block into the warm-summary tier in a single batch (one
  summarizer invocation per turn already happens today on aging; the
  new behavior is the *batching trigger*).
- The append-only `turns` vector is unchanged. `summarized_turns.len()`
  grows by the batch size. `recall_turn`, sleep input, and reports keep
  seeing the full historical record.

This is deliberately batched rather than gradual. Prompt caching makes one
big prefix change cheaper than many small ones; a single batch amortizes the
cache miss.

Configurable defaults:

- High-water threshold: 80% of model window.
- Low-water target after the batch ages out: 50% of model window.

These are starting values; they need a real long session to tune. Relating
to existing config: today `QSF_SESSION_WARM_THRESHOLD` controls active
verbatim turn count. The new threshold acts in parallel as a token-budget
governor that can age out a *batch* of turns at once when token pressure
crosses the line, even if `QSF_SESSION_WARM_THRESHOLD` would not have
fired yet. The plan should decide how the two thresholds compose (see
Open Questions).

#### Coverage Rule

Any turn that leaves hot context goes through the co-retrieval pass exactly
once. Triggers:

- **Batch-drop event** (token threshold crossed): covers the bulk case.
- **Session-end flush** (normal exit like `:quit`, clean `SessionEnded`):
  runs co-retrieval over the remaining hot turns before sleep is invoked.
- **Crash / unclean exit:** the not-yet-processed turns are picked up on
  next session boot, with a sleep safety-net proposer as backstop.

Coverage is tracked by `ProcessedRange` entries inside the memory store
itself (see [Crash Idempotency](#crash-idempotency)). The safety-net
proposer skips ranges already recorded, making re-runs idempotent.

The cross-turn algorithm itself is the existing
`build_cross_turn_associations` from
[crates/qsf_app/src/sleep/auto_promote.rs:96-171](../../crates/qsf_app/src/sleep/auto_promote.rs#L96-L171),
extracted as a pure function the live loop and the sleep safety-net both
call. The window is the existing `CROSS_TURN_ASSOCIATION_WINDOW = 3`. It
straddles the boundary: when a block drops, the window extends 3 turns into
still-hot turns so associations form between dropping and adjacent living
turns.

This is distinct from the existing same-turn pairwise mechanism
(`co_retrieval::generate_deltas`) which keeps running every turn unchanged.
The new cross-turn pass complements it; it does not replace it.

#### Event Flow For Aging And Persistence

The chain follows the 2026-05-09 unidirectional event-reducer-state flow
decision. Side effects (memory-store I/O, summarizer model invocation)
are isolated; the reducer only sees events.

```text
1. Reducer (end of turn): inspects hot-context token estimate. If above
   high-water mark, emits side-effect request "run cross-turn pass + age
   batch."

2. Side effect (impure):
     a. Determines the aging range (pure function over current state).
     b. Runs cross-turn co-retrieval (pure function; see Endpoint
        Validation below) over the aging range plus 3-turn overlap into
        still-active verbatim turns.
     c. In ONE memory-store write, persists:
          - the new/strengthened associations, AND
          - a ProcessedRange entry recording session_id, turn range,
            and the timestamp.
        See Crash Idempotency below.
     d. Invokes the warm summarizer for each aging turn (existing path).

3. Side effect emits `TurnsAgedAndCoRetrieved { range, new_count,
   strengthened_count, persisted_at }` event.

4. Reducer applies the event:
     - Extends `summarized_turns` by the batch.
     - Reloads or mutates the in-memory memory snapshot so the next turn
       sees the freshly-persisted associations (see Live Snapshot
       Refresh below).
     - `Turn` records remain in append-only state.turns; no removal.
```

Session-end follows the same chain, triggered by the clean `SessionEnded`
reducer path before sleep handoff.

#### Crash Idempotency

The durable record of "did we process this range" lives **inside the memory
store**, not in session state. This avoids any cross-file commit problem:

- `MemoryStoreContents` gains a `processed_ranges: Vec<ProcessedRange>`
  field with `#[serde(default)]`. Each entry is `{ session_id,
  first_turn_index, last_turn_index, kind: "live_batch" | "session_end" |
  "sleep_safety_net", at }`.
- The atomic file write of memory-store.json includes both the new
  associations AND the matching `ProcessedRange`. Either both are durable
  or neither is.
- On boot or sleep-safety-net, ranges already covered by an entry are
  skipped. The same-range, same-kind ProcessedRange is uniquely
  identifying — a second attempt is a no-op even if it produces the
  same deltas, because the proposer sees the range as processed.
- No separate `co_retrieved_at` field on `Turn` is needed. Range coverage
  is queried by joining `processed_ranges` with `session_id` + turn
  indices; this is also serializable to the diagnostics output if
  desired.

Bumps `MEMORY_STORE_SCHEMA_VERSION` if the store has a top-level schema
version (verify in implementation); otherwise the field is purely
additive via `#[serde(default)]`.

#### Endpoint Validation In The Cross-Turn Function

Per the 2026-05-23 decision *"Durable associations require present
endpoints"*, durable associations must only be created when both endpoint
memory IDs exist in the destination store.

The extracted cross-turn function carries this contract:

```rust
pub fn generate_cross_turn_deltas(
    retrievals_per_turn: &[Vec<String>],
    existing_associations: &[Association],
    known_record_ids: &HashSet<String>,
    window: usize,
    session_id: &str,
    now: OffsetDateTime,
) -> Vec<CoRetrievalDelta>;
```

The `known_record_ids` parameter is mandatory. Callers compute it from
`MemoryStoreContents::records`. The function MUST skip any pair where
either endpoint is missing. Regression coverage equivalent to
`cross_turn_retrievals_skip_ids_missing_from_current_store` is ported
when the function moves.

#### Live Snapshot Refresh

Today `apply_live_memory_reinforcement` persists association/record
changes but the in-memory `SessionMemorySourceSnapshot` (loaded once at
[multi_turn_text_loop.rs:280](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs#L280))
is not refreshed within the same process. This is a pre-existing gap
the design must close, otherwise hints will never see newly-created
edges within a session.

Phase 1 includes refresh on persistence. Two acceptable implementations
(plan picks one):

- **Reload on change**: after `apply_live_memory_reinforcement` and after
  the new drop/session-end pass, reload the persisted store into the
  snapshot. Simple; one disk read per persistence event.
- **In-place mutation**: same-turn and drop-pass code paths mutate the
  snapshot in lockstep with the persisted store. Cheaper; requires
  discipline to keep paths synchronized.

Test: an association created in turn N is present as a hint candidate
for turn N+1's retrieval, same process.

### Interaction With `warm_threshold` / `TurnSummary`

The existing session state already has a notion of "warm-summarized" turns
that sits between full-detail fresh turns and any future drop. This design
does not pick one of the two options below; the implementation plan must.

- **Option A:** warm-summarized turns are an intermediate stage; drops
  operate on summarized turns. Co-retrieval window operates over the
  dropping summaries.
- **Option B:** drops bypass warm summary entirely; full turns drop
  directly when threshold is crossed.

Recording explicitly as an open implementation question (see Open Questions).

### Sleep Changes

Sleep stops running the cross-turn pass on the whole session by default.
The cross-turn algorithm moves to the shared `memory/co_retrieval.rs`
module so both the live loop and the sleep safety-net call the same
function.

Sleep keeps doing:

- Generating durable memory candidates from session text
  (`summarize_session` -> `SleepReport.memory_candidates` -> promotion).
- The LLM-proposed `association_candidates` between newly-promoted memory
  candidates.
- Reinforcement and creation of associations *via the proposer pipeline*
  (today: LLM-candidate proposer; safety-net cross-turn proposer for
  ranges the live loop did not cover; new proposers in future). This is
  no longer an exclusive sleep privilege — the live loop strengthens
  existing edges through its same-turn and cross-turn paths.

#### Pluggable `AssociationProposer` Interface

```rust
trait AssociationProposer {
    fn name(&self) -> &str;                     // e.g. "two-hop-bridge"
    fn propose(
        &self,
        store: &MemoryStoreContents,
        session: &SessionState,
        as_of: OffsetDateTime,
    ) -> Vec<ProposedAssociation>;
}
```

`ProposedAssociation` carries `from_id`, `to_id`, `weight`, `reason`. The
sleep pipeline runs an enabled set of proposers (config-driven), merges their
candidates, deduplicates against the store and against each other, and
persists the result.

Two proposers ship with this design:

- `LlmCandidateProposer`: wraps the existing LLM-proposed candidates flow
  in `build_sleep_candidate_associations`. Default ON.
- `SafetyNetCoRetrievalProposer`: runs the shared cross-turn pass over
  any session-turn ranges not yet recorded in
  `MemoryStoreContents::processed_ranges`, in case the live loop missed
  them (crash recovery, code path that skipped the flush, etc.). On
  success, writes a `kind: "sleep_safety_net"` ProcessedRange in the
  same atomic store write as the new associations. Default ON.

No other proposer is implemented in this design. New strategies are added
later as experiments, each one entering as a new `AssociationProposer`
implementation.

#### Sleep Summarizer Prompt

The sleep prompt is revised to emphasize "find non-obvious connections" and
no longer instruct the model to mechanically link co-retrieved memories.
The new prompt text is left as an implementation detail; the design
constraint is that the LLM-candidate proposer should produce candidates
that current code-based proposers (today: co-retrieval) would not.

### Console Visual Feedback

Console = text and content. Live Activation Dashboard = subsystem activity.
Voice output = direct simulation answer only. These boundaries are recorded
here as a project rule so future tools do not drift into duplication.

Color scheme (256-color terminal):

| Element | Style |
| --- | --- |
| User input (typed; later, transcribed voice) | default white, unmodified |
| Model response (direct simulation answer) | default white, unmodified |
| `=== Memories retrieved for this turn ===` | cyan, dim header + bright body |
| `=== Associated memories (hints) ===` | amber/yellow, dim throughout |
| `--- aged N turns from prompt; +K associations, *J strengthened ---` | dim gray, italic if supported |
| Errors / sleep failures | red, bright |

Starting color codes: cyan = 51 (header 44), amber = 214 (dim 172), gray = 240.
These are starting values; tune in implementation if a terminal renders them
poorly.

Color is disabled when stdout is not a TTY, when `NO_COLOR` env var is set,
or when `--no-color` is passed. `engine_logging` output is independent and
unchanged.

Both blocks print to the console once per turn, immediately before the model
response. The format printed is the literal text that goes into the prompt
— what the user sees is what the model sees. No separate "summary line" is
emitted; the block headers are the summary.

The drop-event marker prints inline between turns, immediately when the
batch drop completes. The session-end flush emits its own marker
(`--- session-end flush; +K associations ---`) so the user sees the
coverage was completed.

## Implementation Phases

Each phase ships independently and leaves the system testable.

### Phase 1: Hint Expansion In Retrieval

- Add `ContextSourceKind::MemoryHint`.
- Add `expand_neighbors(directs, store)` as a pure, undirected function.
  See [Hint Expansion Direction](#hint-expansion-direction).
- Implement source-priority assembly (two-pass API or source-priority
  comparator; see [Hint Budget](#hint-budget)) so a hint cannot evict a
  direct memory under budget pressure.
- Implement live snapshot refresh
  ([Live Snapshot Refresh](#live-snapshot-refresh)) wired into the
  existing per-turn `apply_live_memory_reinforcement` path. Without this,
  hints cannot reflect associations created within the same process.
- Update prompt formatter to emit the two labeled blocks.
- Change `SESSION_RETRIEVAL_STRATEGY` and
  `VOICE_MEMORY_RETRIEVAL_STRATEGY` from `AssociationWeighted` to
  `KeywordTag` so retrieval and hint expansion together stay strict
  single-hop. See
  [Direct-Retrieval Strategy When Hints Are Active](#direct-retrieval-strategy-when-hints-are-active).
- No sleep changes, no aging-policy changes.

Verification:

- Unit tests: `expand_neighbors` dedup, broken-edge handling, weight
  ordering, hint cap; explicit cases for incoming-only edge,
  outgoing-only edge, and reciprocal pair (per
  [Hint Expansion Direction](#hint-expansion-direction)).
- Context-assembly tests with mixed directs and hints, including the
  budget-pressure case where a hint must NOT evict a direct.
- Snapshot-refresh test: an association created by same-turn
  reinforcement in turn N is selectable as a hint in turn N+1's
  retrieval, same process.
- Strategy-change test: `KeywordTag` is the active strategy for
  `multi_turn_text_loop` and `text_owned_voice_loop`.
- Prompt-snapshot tests showing both blocks.
- **Human testing:** run a multi-turn session against a fixture with
  known associations; eyeball the prompt; confirm hints appear and
  feel appropriate.

Docs to update:

- `docs/EngineeringDiary.md` entry for the change.
- No Architecture doc update yet; defer until Phase 4.

### Phase 2: Console Color And Drop Marker

- Color-coded direct and hint blocks.
- Drop-event marker line (prints with zero counts in this phase because
  Phase 4 has not landed; proves the wiring).
- TTY / `NO_COLOR` / `--no-color` handling.

Verification:

- Snapshot tests of colored output (color enabled).
- Tests for `NO_COLOR`, `--no-color`, and non-TTY disabling color.
- **Human testing:** run a session in a real terminal; confirm legibility
  on dark and light themes.

Docs to update:

- `docs/EngineeringDiary.md` entry.

### Phase 3: Add Cross-Turn Variant Next To Existing Same-Turn

The shared module `crates/qsf_app/src/memory/co_retrieval.rs` already
exists and hosts `generate_deltas` (same-turn pairwise). The cross-turn
variant lives in `sleep/auto_promote.rs::build_cross_turn_associations`
and isn't yet available to the live loop.

- Move `build_cross_turn_associations` into `memory/co_retrieval.rs` as a
  pure function alongside `generate_deltas`. Suggested name and signature:
  `generate_cross_turn_deltas(retrievals_per_turn, existing_associations,
  known_record_ids, window, session_id, now) -> Vec<CoRetrievalDelta>`.
  Returns the same `CoRetrievalDelta` variants as the same-turn path so
  persistence code applies both kinds of deltas through one route.
- `known_record_ids` is a mandatory parameter. The function MUST skip
  pairs where either endpoint is missing, preserving the
  2026-05-23 *"Durable associations require present endpoints"*
  decision.
- Sleep keeps calling the cross-turn variant on the whole session in
  this phase (no behavior change). The caller passes
  `MemoryStoreContents::records` IDs as `known_record_ids`.
- Same-turn `apply_live_memory_reinforcement` is untouched.

Verification:

- Existing `auto_promote` tests pass unchanged (regression).
- New unit tests for `generate_cross_turn_deltas` against fixtures
  equivalent to those currently exercising
  `build_cross_turn_associations`, including the ported regression
  `cross_turn_retrievals_skip_ids_missing_from_current_store`.
- No new behavior observable from the application side.

Docs to update:

- `docs/EngineeringDiary.md` entry.

### Phase 4: Aging Policy, Live Cross-Turn Co-Retrieval, Session-End Flush

Implements the event/reducer/effect flow from
[Event Flow For Aging And Persistence](#event-flow-for-aging-and-persistence).

- Add `processed_ranges: Vec<ProcessedRange>` to `MemoryStoreContents`
  with `#[serde(default)]`. See
  [Crash Idempotency](#crash-idempotency).
- Reducer-side: threshold check at end of turn; emits a side-effect
  descriptor (no state mutation here). Composition with
  `QSF_SESSION_WARM_THRESHOLD` resolved per Open Questions.
- Side-effect side: pure aging-range calculation, pure
  `generate_cross_turn_deltas` call with `known_record_ids` parameter,
  single-write persistence of associations + ProcessedRange, then
  summarizer invocations for aging turns, then emits
  `TurnsAgedAndCoRetrieved` event.
- Reducer-side: handles `TurnsAgedAndCoRetrieved` by extending
  `summarized_turns` by the batch; `state.turns` is unchanged
  (append-only preserved per 2026-05-17 decision).
- Snapshot refresh (per
  [Live Snapshot Refresh](#live-snapshot-refresh)) applied after each
  live persistence event (also retrofitted to same-turn reinforcement
  in Phase 1).
- Implement session-end flush triggered by clean `SessionEnded`, using
  the same side-effect chain.

Verification:

- Reducer unit tests: threshold check, `TurnsAgedAndCoRetrieved`
  application, append-only invariant on `state.turns`. Pure-function
  inputs/outputs only.
- Side-effect unit tests: aging-range calculation, delta generation
  with endpoint validation, single-write persistence ordering.
- End-to-end coverage tests: no aging + clean exit, multiple aging
  events + clean exit, single batch + clean exit, aging with no clean
  exit (simulated crash → re-boot picks up via sleep safety net).
- Idempotency: a `ProcessedRange` already present in the store causes
  subsequent passes (live re-boot, sleep safety net) over the same
  range to skip, producing no new edges or weight changes. Also at the
  recovery level: a session that crashed mid-pass is recovered by sleep
  safety net with no double-strengthening, because either the single
  store write completed (range marked) or it did not (no associations
  persisted; fresh attempt is correct).
- Persistence: `processed_ranges` survives serialization round-trip with
  an explicit serde test.
- Endpoint validation: ported regression for
  `cross_turn_retrievals_skip_ids_missing_from_current_store`.
- Logging: every aging event and flush logs `session_id`, `state_dir`,
  range, `new_count`, `strengthened_count`, `aged_turn_count`. Errors
  during persistence log the same context with a sanitized error
  summary.
- **Human testing:** force a long session via a script; confirm aging
  events fire at the configured threshold and associations accumulate;
  confirm `:quit` flush completes coverage. Confirm `recall_turn` and
  reports still see the aged turns.

Docs to update:

- `docs/Architecture/Architecture.MemorySystem.md` Implementation Status:
  cross-turn association creation now occurs in the live loop on aging
  and session-end; sleep retains safety-net coverage; store carries
  `processed_ranges`.
- `docs/Architecture/Architecture.RuntimeLoop.md` Implementation Status:
  token-budget aging policy, event flow, and session-end flush.
- `docs/EngineeringDiary.md` entry.

### Phase 5: Proposer Interface And Sleep Prompt Rewording

- Implement `AssociationProposer` trait and registry.
- Wrap existing LLM-candidate flow as `LlmCandidateProposer`.
- Add `SafetyNetCoRetrievalProposer` (idempotent against
  `MemoryStoreContents::processed_ranges`).
- Reword sleep summarizer prompt away from mechanical co-retrieval and
  toward non-obvious connections.

Verification:

- Trait and registry tests.
- Proposer composition tests (dedup across proposers).
- **Human testing:** run sleep on a session with no drops; safety net
  fires. Run sleep on a session with multiple drops; safety net is a
  no-op.

Docs to update:

- `docs/Architecture/Architecture.SleepPhase.md` Implementation Status:
  sleep no longer runs the co-retrieval window directly; pluggable
  proposers; sleep prompt rewording.
- `docs/EngineeringDiary.md` entry.

### Phase 6: Ideas Backlog And Decision Entry

No code.

- Create `docs/Plans/Ideas.AssociationProposers.md` with the initial set
  of strategy ideas (two-hop bridge, common-substring/n-gram,
  cross-session co-retrieval, tag-overlap-rarity). Each entry lists
  signal, risk of noise, and what evaluation would prove or refute its
  value. The doc notes that evaluation requires a corpus of memories
  first (real session or fixture).
- Final wording cleanup pass on the Architecture docs touched in
  Phase 4 and Phase 5, if anything needs sharpening with both pieces
  landed.
- `docs/DecisionLog.md` entry for the durable architectural commitment:
  *Mechanical association work runs in the live loop on drop and
  session-end; sleep hosts pluggable proposers for non-obvious
  associations.*

## Defaults That Exercise New Code

- `MAX_HINTS_PER_TURN = 8` (default ON; the default multi-turn-text-loop
  prompt includes hints).
- Hot-context high-water threshold = 80%, low-water target = 50%
  (default ON; given a long-enough session, drops fire on the default
  path).
- `SafetyNetCoRetrievalProposer` enabled by default (covers short
  sessions and crash-recovery cases).
- Color rendering ON when running in a TTY without `NO_COLOR` or
  `--no-color`.

## Cross-Cutting Acceptance Criteria

- Reducers stay pure. Drop detection, neighbor expansion, and
  co-retrieval are pure functions over inputs.
- `engine_logging` records carry: `session_id`, exchange/turn index,
  dropping range, association counts (new and strengthened), proposer
  name when sleep runs proposers, and reason for any skipped pass.
- Every phase that changes runtime behavior ends with a
  `docs/EngineeringDiary.md` entry.
- Architecture doc Implementation Status sections updated at Phase 4 and
  Phase 5.
- `Association` persistence schema stays at
  `ASSOCIATION_SCHEMA_VERSION = 1`. The new
  `MemoryStoreContents::processed_ranges` field is purely additive via
  `#[serde(default)]`; existing store files load unchanged. Verify
  during implementation whether a `MemoryStoreContents` schema-version
  bump is required by repository conventions.
- `Turn` / `Exchange` records are not extended with new fields by this
  design; append-only invariant on `state.turns` is preserved per the
  2026-05-17 decision.

## Open Implementation Questions

These are deliberately left for the implementation plan to resolve.

1. **`QSF_SESSION_WARM_THRESHOLD` interaction with token-budget aging.**
   Two thresholds now potentially trigger aging: the existing
   active-turn-count threshold and the new token-budget threshold. The
   plan must decide composition: do they OR together (whichever fires
   first), is the token-budget threshold the only trigger and the
   count-based one becomes redundant, or does the count threshold remain
   a per-turn aging step while the token-budget threshold is the only
   *batch* trigger?
2. **Crash-recovery responsibility.** With `processed_ranges` in the
   store, a crash that left no range marked means the live loop can
   retry on next boot. Sleep safety net also runs against the same
   `processed_ranges`. Plan should pick: re-attempt eagerly on next live
   boot, or rely on sleep safety net only? Suggested default per the
   external review: sleep safety net only.
3. **Where the cross-turn variant lives.** `memory/co_retrieval.rs`
   (recommended; same module as same-turn) or a sibling. First time
   another consumer needs it can split.
4. **`MemoryHint` visibility in diagnostics.** Should hints appear in
   `multi_turn_text_loop` report output and trace payloads, or only in
   the prompt and live console?
5. **Whether the sleep prompt rewording is a separate DecisionLog entry.**
   The live/sleep split is one durable commitment; the prompt rewording is
   a tactical change. The plan can fold them together or split them.
6. **Live snapshot refresh strategy.** Reload-on-change (simpler, one
   disk read per persistence) versus in-place snapshot mutation
   (cheaper, requires path discipline). See
   [Live Snapshot Refresh](#live-snapshot-refresh).
7. **Source-priority implementation.** Two-pass assembly API vs
   source-priority comparator on the existing single-pass `assemble_context`.
   See [Hint Budget](#hint-budget).
8. **Model-context-window source.** What does the 80%/50% threshold
   measure against? The model's documented max tokens for the configured
   `model_id`? A configurable QSF override? Plan needs to name the
   source and where it comes from.
9. **Cross-turn co-retrieval input scope.** Should the cross-turn pass
   consume only the memories selected into `ContextAssembly` (today's
   sleep behavior via `turn.context_assembly.retrieved_memory_ids()`),
   or all candidates the retrieval step returned (including omitted
   ones)?
10. **Session-end flush failure behavior.** If the flush side effect
    fails (e.g., disk error), does that block clean `:quit`, or does it
    log and defer recovery to the sleep safety-net proposer on next
    boot? Suggested default: log and defer; never block exit.

## Risks

- Hint blocks could become noisy if the store accumulates many low-weight
  edges. Mitigation: the `MAX_HINTS_PER_TURN` cap, and weight-ordered
  selection inside the cap.
- Token caching benefits depend on batched drops being genuinely batched.
  If the high-water threshold is set too low, drops fire too often and
  the cache benefit vanishes. Mitigation: monitor in human testing and
  tune defaults.
- The pluggable proposer interface invites a proliferation of weak
  proposers. Mitigation: each proposer must enter through an experiment
  in `Ideas.AssociationProposers.md`, with a measurable signal it picks
  up on.
- The `processed_ranges` ledger could become misleading if turns are
  re-indexed or re-ordered after recording. Mitigation: indices are
  scoped by `session_id` and the append-only `state.turns` invariant
  (per 2026-05-17 decision) means indices are stable for the lifetime
  of a session. Cross-session indices are namespaced by `session_id`.
- The single-write atomic persistence of associations + ProcessedRange
  depends on the OS-level file replace being atomic. `MemoryStore::persist`
  already uses an atomic replace pattern; this design relies on that.
  Mitigation: verify the atomic-replace behavior is in place during
  Phase 4, and add a fault-injection test if practical.
- Console color choices may not be legible on all terminals. Mitigation:
  `NO_COLOR` and `--no-color` switches, plus willingness to tune the
  starting palette during human testing.

## Verification For The Full Design

When all phases land:

```text
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Plus one human-tested long session in a real terminal demonstrating
hints, a batched drop, the `:quit` flush, and a sleep run that ends with
the proposer pipeline shape.

## Documents To Update

Per `docs/ProjectFrame/ProjectWorkflow.md`:

- This document is the design surface; it lives in `docs/Plans/`.
- `docs/Plans/Ideas.AssociationProposers.md` is created in Phase 6 with
  the initial proposer ideas.
- `docs/EngineeringDiary.md` receives an entry per phase that changes
  code.
- `docs/Architecture/Architecture.MemorySystem.md` is updated at Phase 4
  (Implementation Status: live-loop association creation) and Phase 6
  (proposer model in the architecture text).
- `docs/Architecture/Architecture.RuntimeLoop.md` is updated at Phase 4
  (drop policy, session-end flush).
- `docs/Architecture/Architecture.SleepPhase.md` is updated at Phase 5
  (sleep no longer runs co-retrieval directly; proposer model).
- `docs/DecisionLog.md` receives one entry at Phase 6 for the live/sleep
  responsibility split.

## Refs

- `crates/qsf_app/src/sleep/auto_promote.rs`
- `crates/qsf_app/src/memory/retrieval.rs`
- `crates/qsf_app/src/memory/association.rs`
- `crates/qsf_app/src/sleep/session_summary.rs`
- `crates/qsf_memory/src/association.rs`
- `docs/Plans/Plan.VoiceLoopUnification.md`
- `docs/Plans/Design.MemoryAssociationBrowser.md`
- `docs/ProjectFrame/ProjectVision.md`
- `docs/ProjectFrame/ProjectWorkflow.md`
