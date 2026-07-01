# Experiment: Goal Coherence Under a Protected Floor

## Status

Implemented. Pure resolution lives in `qsf_volition::coherence`
([crates/qsf_volition/src/coherence.rs](../../crates/qsf_volition/src/coherence.rs)); the
`CoherenceJudge` adapter seam and offline harness live in `qsf_app`
([crates/qsf_app/src/models/coherence_judge.rs](../../crates/qsf_app/src/models/coherence_judge.rs),
[crates/qsf_app/src/experiments/volition_goal_coherence.rs](../../crates/qsf_app/src/experiments/volition_goal_coherence.rs)).
Live wiring (Phase 2) is not yet implemented.

## Summary

Validate that the simulation keeps its goal set internally coherent under an immutable
protected floor: a proposed goal is **admitted** only when it does not contradict a more
fundamental goal, an incompatible candidate is **rejected**, and a periodic whole-set sweep
**cancels** the less-fundamental goal of a contradicting pair. Contradiction is detected by a
model judge (mock-by-default here); the pure reducer resolves the verdict deterministically
using the existing goal lifecycle events.

This is the offline engine behind the "distinct, coherent agent" behavior in
[Plan.VolitionMotivationalTexture.md](../Plans/Plan.VolitionMotivationalTexture.md). The live,
human-testable version (goal formation from discussion, off-hot-path rejection) is a later
phase and out of scope here.

## Decisions Resolved Before Implementation

### D1 - Adopted goals belong to the simulation; no ownership tag

Goals are not classified user / simulator / shared. Every admitted goal belongs to the
simulation; origin is at most a background association. See
[DecisionLog 2026-06-30](../DecisionLog.md#2026-06-30---adopted-goals-belong-to-the-simulation-coherence-replaces-goal-provenance).

### D2 - Model detects, pure reducer resolves, reusing existing lifecycle events

Contradiction detection is a side effect in a `CoherenceJudge` adapter that returns a
`CoherenceVerdict`. The verdict is recorded in the trace and handed to pure resolution
functions that emit **existing** `VolitionEvent`s — `GoalCandidateAccepted` (admit),
`GoalCandidateRejected { reason }` (reject; reason carries the conflicting goal id), and
`GoalRetired` (cancel). No new event types are introduced; the model never mutates state. See
[DecisionLog 2026-06-30](../DecisionLog.md#2026-06-30---goal-coherence-is-model-judged-off-the-hot-path-and-repaired-during-sleep).

### D3 - Protected floor is immutable and model-independent

A candidate whose effective tier is at or below the protected floor is rejected by a
deterministic gate **before** the model is consulted. That pre-judge rejection has no verdict:
its trace record carries `judge_ref: null`, `contradictions: []`, and
`resolution: reject_protected_floor`. The sweep never cancels a floor goal; a floor-vs-floor
contradiction is flagged for human review (a trace-only record, no state change), not
auto-resolved. Protected goals may still change dynamic state (salience, status) through normal
lifecycle events — immutability is about core membership, not dynamic state.

### D4 - Candidate source is the existing reflection path

Candidates come from `propose_goal_candidates()` over open questions. Live discussion-formed
candidates are a later phase reusing this same engine.

### D5 - Sweep tie-break is fully deterministic

Effective tier is computed from a goal's or candidate's `tension_ids` against
`fixture.tensions` — never by looking the id up in `fixture.goals` (which returns `u8::MAX` for
accepted or proposed candidates). The sweep snapshot merges `fixture.goals` with
`state.accepted_candidates`. On an equal-tier contradiction: treat a missing
`last_activated_tick` as tick 0 (the existing convention), cancel the goal with the greater
activation tick, and on a tick tie cancel the lexicographically greater goal id. Cancellation
reuses `GoalRetired`; because that event carries no reason, the incompatibility reason and
conflicting goal id live in the trace record.

## Related Documents

```text
docs/Plans/Plan.VolitionMotivationalTexture.md
docs/Plans/Design.VolitionBriefReconciliation.md
docs/Architecture/Architecture.VolitionSystem.md
docs/DecisionLog.md
crates/qsf_volition/src/coherence.rs
crates/qsf_volition/src/reducer.rs
crates/qsf_volition/src/candidate.rs
crates/qsf_app/src/experiments/
crates/qsf_app/src/observability/trace.rs
crates/qsf_app/src/runtime/run_context.rs
```

## Hypothesis

An offline engine can admit, reject, and cancel goals so the set stays free of contradictions
with more fundamental goals, using a model only to *detect* contradictions while a pure reducer
*resolves* them into existing lifecycle events — and the trace stream can explain every admit /
reject / cancel decision from recorded facts alone.

## Scope

### In Scope

- Pure resolution over synthetic `CoherenceVerdict`s: admission (admit / reject-by-tier /
  admit-and-cancel-less-fundamental / reject-dominates-with-suppressed-cancellation) and sweep
  (cancel-less-fundamental / equal-tier tie-break / floor-vs-floor flag).
- The deterministic pre-judge hard tier-floor admission gate.
- A generalized effective-tier helper that tiers `Goal`s and `ProposedGoalCandidate`s from
  `tension_ids`, and a sweep snapshot merging `fixture.goals` with `state.accepted_candidates`.
- A `CoherenceJudge` seam with a deterministic mock (scripted verdict) and an opt-in
  model-backed impl over the `ModelRole` boundary.
- An offline harness that records each check as a `TraceRecord` in `traces.jsonl`, emits the
  lifecycle events to `events.jsonl`, and applies them through `apply()`.

### Out of Scope

- Live-loop wiring, off-hot-path admission, and goal formation from discussion (later phase).
- The sleep-pass integration of the sweep (later phase; here the sweep runs in the harness).
- Any arbitration/salience feedback from coherence state.
- Any external write-capable effect.

## Setup

- A fixture with: at least one protected-floor goal; one malleable goal; a scripted candidate
  that contradicts a more-fundamental goal (reject case); a scripted candidate more fundamental
  than a contradicting existing goal (admit-and-cancel case); a **protected-tier proposed
  candidate** (to exercise the pre-judge floor gate and tiering-from-`tension_ids`); and an
  **accepted candidate absent from `fixture.goals`** (to exercise sweep tiering of
  `accepted_candidates`).
- Mock `CoherenceJudge` returning scripted verdicts keyed to the fixture.
- A `RunContext` run directory with `traces.jsonl` and `events.jsonl`.

## Procedure

### Automated Verification

1. **Admit (no contradiction):** a candidate with no contradiction is admitted; a
   `GoalCandidateAccepted` event is emitted; the goal set gains one `Accepted` goal.
2. **Reject by tier:** a candidate contradicting a more-fundamental goal emits
   `GoalCandidateRejected` whose reason names the conflicting goal; no `GoalCandidateAccepted`;
   goal set otherwise unchanged.
3. **Admit and cancel:** a candidate strictly more fundamental than a contradicting existing
   goal emits `GoalRetired` for that goal then `GoalCandidateAccepted` for the candidate (retire
   before accept).
4. **Hard tier-floor gate:** a candidate at or below the protected floor produces a
   `reject_protected_floor` record with `judge_ref: null` and `contradictions: []`, no
   `GoalCandidateAccepted`, and no model call.
5. **Sweep cancel:** a sweep over two contradicting existing goals emits `GoalRetired` for the
   higher-tier-number goal only; the lower-tier-number goal is untouched.
6. **Equal-tier tie-break:** a sweep over two equal-tier contradicting goals cancels the greater
   `last_activated_tick` (missing = 0); a tick tie cancels the lexicographically greater id.
7. **Floor-vs-floor flag:** a verdict pairing two floor goals produces a `flagged` resolution
   and emits no `GoalRetired`.
8. **Reject dominates (multi-conflict):** a candidate with one rejecting conflict and one
   cancelling conflict is not admitted; the cancelling target is recorded in
   `suppressed_cancellations_due_to_rejection` and no `GoalRetired` is emitted.
9. **Candidate tiering:** a protected-tier proposed candidate is tiered from `tension_ids`
   (not `u8::MAX`) and hits the floor gate; an accepted candidate absent from `fixture.goals`
   is tiered correctly when evaluated in a sweep.
10. **Trace parse:** parse `traces.jsonl` and `events.jsonl` and assert every
    `goal-coherence-check` record satisfies the trace contract below and that its resolution is
    recomputable from the recorded contradictions and effective tiers.

### Human Test Steps

None. Phase 1 is offline and deterministic. Human voice verification belongs to the live
formation/rejection phase.

## Trace Completeness Contract

Each coherence check writes one `TraceRecord` with operation `goal-coherence-check` to
`traces.jsonl`. That record is authoritative for the check, including the lifecycle events it
resolved to — `qsf_app`'s `events.jsonl`, consistent with every other experiment in this
codebase, carries only the framework `EventType` stream (`InputReceived`, `TraceRecorded`, ...)
and does not hold raw `VolitionEvent`s as first-class entries; a `TraceRecorded` event links
back to its `goal-coherence-check` trace by `trace_id`.

Required fields (in the trace record's `details`):

- `trigger` — `admission` or `sweep`
- `tick`
- `candidate` — `{ goal_id, effective_tier }` (admission only; `null` for a sweep)
- `goal_set_snapshot` — evaluated goals (merged `fixture.goals` + `accepted_candidates`,
  excluding already-retired goals) as `{ goal_id, effective_tier, status, last_activated_tick }`
- `judge_ref` — model role + prompt-version id, or `null` for a pre-judge floor rejection
- `contradictions` — list of `{ goal_a, goal_b, rationale }`; empty for a pre-judge floor
  rejection
- `hard_tier_floor_rejected` — bool (admission only; `null` for a sweep)
- `resolution` — admission: `{ kind: "judged", admitted, rejected_by, cancelled_goal_ids,
  suppressed_cancellations_due_to_rejection }`, or `{ kind: "reject_protected_floor" }` for the
  pre-judge gate; sweep: `{ cancellations: [{ cancelled_goal_id, conflicting_goal_id, tie_break
  }], flagged: [{ goal_a, goal_b }] }`
- `events_emitted` — the emitted `VolitionEvent`s (`GoalCandidateAccepted` /
  `GoalCandidateRejected` / `GoalRetired`) — the authoritative record of what the reducer
  applied for this check
- `goal_status_before` / `goal_status_after` — one object each, keyed by affected goal id
- `artifact_or_record_reference` — a stable pointer back to this check (`trigger` + `tick`)

Artifact boundary:

```text
traces.jsonl (TraceRecord, operation "goal-coherence-check"):
  Authoritative record of each check: the recorded model verdict (or null for a pre-judge
  floor rejection), the deterministic resolution, and the events_emitted the reducer applied.

events.jsonl:
  The framework EventType stream (InputReceived, TraceRecorded, ...). A TraceRecorded event
  links back to its goal-coherence-check trace by trace_id; it does not duplicate the
  VolitionEvents themselves.

pure state:
  Changes only through apply() over the events_emitted recorded in traces.jsonl; never holds
  the model output.
```

Parsing verification:

- Parse `traces.jsonl` and `events.jsonl`, not in-memory structs.
- Assert each admission record's `resolution` is recomputable from `{ contradictions, effective
  tiers }` — replay determinism is asserted over resolution and events for a fixed verdict, not
  over the model output.
- Assert a rejected candidate produced a `GoalCandidateRejected` and no `GoalCandidateAccepted`,
  with no goal-set change beyond the rejection.
- Assert an admit-and-cancel emitted `GoalRetired` for exactly the higher-tier-number goal
  before the `GoalCandidateAccepted`.
- Assert an equal-tier sweep tie-break followed the None-as-0 / greater-tick /
  greater-id rule.
- Assert a floor-vs-floor contradiction produced a `flagged` resolution and emitted no
  `GoalRetired`.
- Assert a below-floor candidate produced a `reject_protected_floor` record with
  `judge_ref: null` and `contradictions: []` regardless of any scripted verdict.

## Expected Output

- A goal set that, after admission and a sweep, contains no goal contradicting a more
  fundamental one, with every admit / reject / cancel decision explained by a
  `goal-coherence-check` trace record and its emitted lifecycle events.
- Pure resolution unit tests that pass over synthetic verdicts without any model call.

## Results

Implemented as `qsf_app`'s `volition-goal-coherence` experiment, run over a purpose-built
9-goal/5-tension fixture (disjoint tension id-terms so `propose_goal_candidates` matches each
scripted question to exactly one tension, per D4). All ten automated-verification items and
every parsing-verification bullet pass:

- Five admission checks cover admit (no contradiction), reject-by-tier, admit-and-cancel
  (`GoalRetired` before `GoalCandidateAccepted`), the hard tier-floor gate (`judge_ref: null`,
  `contradictions: []`, no model call), and reject-dominates (rejection suppresses the
  cancelling conflict without retiring it).
- One sweep covers cancel-less-fundamental — including tiering an admitted candidate absent
  from `fixture.goals` purely from its own `tension_ids` — both tie-break rules (greater
  activation tick, then greater goal id on a tick tie), and a floor-vs-floor pair that is
  flagged and never cancelled.
- `qsf_app::experiments::volition_goal_coherence::tests` parses `traces.jsonl` and
  `events.jsonl` from disk (not in-memory structs) and replays each admission's resolution from
  its own recorded `contradictions` and `goal_set_snapshot` effective tiers, independent of the
  scripted model verdict that produced them.
- `ModelBackedCoherenceJudge` and `ScriptedCoherenceJudge` both validate every returned
  contradiction against the queried goal set before returning a verdict, failing with context
  on an unknown goal id or a self-contradiction rather than letting an unreal id silently tier
  as `u8::MAX` or produce a no-op retirement.
