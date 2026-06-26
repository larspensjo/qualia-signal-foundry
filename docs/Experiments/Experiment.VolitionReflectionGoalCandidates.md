# Experiment: Volition Reflection-Generated Goal Candidates

## Experiment ID

`Experiment.VolitionReflectionGoalCandidates`

## Status

Implemented (2026-06-26). All automated success criteria met; human verification
pending. Run the experiment with `cargo run -p qsf_app -- experiment
volition-reflection-goal-candidates` to produce a run artifact. The phase sequencing
lives in [Plan.VolitionGoalSystem.md](../Plans/Plan.VolitionGoalSystem.md). The
rationale and terminology live in
[Idea.VolitionGoalSystem.md](../Plans/Idea.VolitionGoalSystem.md).

## Summary

This experiment adds the first path by which goals can enter the volition system from
outside the static fixture: a pure `propose_goal_candidates` function maps scripted open
questions to `ProposedGoalCandidate` values by matching question terms against tension
ids and summaries. Proposed candidates stay in `VolitionState::pending_candidates` until
an explicit accept or reject event moves them. Accepted candidates land in
`VolitionState::accepted_candidates` (a separate map from fixture-seeded goals) and are
not wired into any selector in this phase.

The scripted sequence covers four turns:

1. **Propose** — `propose_goal_candidates` runs on four questions (three match tensions,
   one matches none); `GoalCandidateAdded` events append matched candidates to
   `pending_candidates`.
2. **Accept** — `GoalCandidateAccepted` moves one candidate from `pending_candidates` to
   `accepted_candidates`.
3. **Reject** — `GoalCandidateRejected` removes one candidate from `pending_candidates`;
   the rejection reason is captured in the event log.
4. **Inert** — No review events; the remaining candidate stays in `pending_candidates`
   unchanged.

No effect is executed in any turn.

## Motivation

After arbitration conflict resolution, all goals are hand-authored in the static
fixture. Before any reflection-driven goal can influence behavior, the project needs
evidence that:

- A pure, model-free proposer can generate candidates from open questions without
  auto-accepting them;
- The pending/accepted boundary is structurally enforced (no silent promotion);
- Accepted candidates are recorded separately from fixture goals, establishing a clear
  source-of-truth boundary;
- Replay produces identical state and event logs (determinism);
- No accepted candidate is wired into `select_goals_with_salience` before selector
  integration for accepted candidates.

## Related Documents

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionGoalFixture.md
Experiments/Experiment.VolitionTraceBackedInitiative.md
Experiments/Experiment.VolitionSalienceAndSatisfaction.md
Experiments/Experiment.VolitionArbitrationConflict.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

## Hypothesis

A pure keyword-matching proposer can route open questions to plausible tension
categories with enough precision to be a useful seed for human review, without
requiring a model call or auto-accepting any candidate.

## What This Experiment Does NOT Measure

- Model-generated question quality (this phase uses scripted questions only).
- Whether accepted candidates improve selector output (wiring is deferred to selector integration for accepted candidates).
- Whether the keyword matching is precise enough for production use (it is a starting
  point; richer matching is a future phase decision).

## Scripted Inputs

Four open questions designed to exercise three distinct tension categories plus one
no-match case:

| Question | Expected match |
|---|---|
| "Is continuity preserved across sessions?" | `continuity-preservation` |
| "Are there coherence issues when speculative ideas are blended with current facts?" | `coherence-maintenance` |
| "What research questions remain about the memory system design?" | `research-curiosity` |
| "What time is it in milliseconds?" | no tension match |

## New Code

- **`ProposedGoalCandidate`** (`volition.rs`) — private-field struct with a `try_new`
  constructor that rejects empty `proposal_evidence`. Implements `Serialize` /
  `Deserialize`.
- **`GoalCandidateProposalResult`** — return type of `propose_goal_candidates`; holds
  `candidates` and `unmatched_questions`.
- **`VolitionState::pending_candidates`** / **`accepted_candidates`** — two new
  collections, initialized empty by `from_fixture`.
- **`VolitionEvent::GoalCandidateAdded`**, **`GoalCandidateAccepted`**,
  **`GoalCandidateRejected`** — three new event variants handled by the pure `apply`
  reducer.
- **`propose_goal_candidates`** — pure, deterministic function; no model call.

## Success Criteria

### Automated (all must pass before the experiment is considered complete)

- [x] `ProposedGoalCandidate` cannot be constructed with an empty `proposal_evidence`
  list.
- [x] `GoalCandidateAdded` appends to `pending_candidates`; does not auto-accept.
- [x] `GoalCandidateAccepted` without a prior `GoalCandidateAdded` for the same id is a
  no-op (reducer does not panic).
- [x] `GoalCandidateAccepted` with a valid `EvidenceRef` moves the candidate to
  `accepted_candidates`.
- [x] `GoalCandidateRejected` removes the candidate from `pending_candidates`.
- [x] A remaining (neither accepted nor rejected) candidate stays in
  `pending_candidates` across ticks.
- [x] `accepted_candidates` is keyed by goal id; goals in it are distinct from
  fixture-seeded goals in `VolitionState::goals`.
- [x] `propose_goal_candidates` is deterministic: same input produces identical output.
- [x] Existing reducer branches and selector outputs are unchanged; all prior
  event-handling unit tests still pass.
- [x] No effect is executed; `accepted_candidates` map is not fed into any selector in
  this phase.
- [x] Replay produces identical state and event logs.
- [x] `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.

### Human (requires running the experiment and reading the report)

- [ ] Proposed candidates are clearly distinct from fixture-seeded accepted goals in the
  per-turn snapshot.
- [ ] The accept/reject trace answers "why was this accepted or rejected?" from the
  evidence ref and reason fields alone.
- [ ] Nothing in the output implies a candidate was active or influenced behavior before
  acceptance.
- [ ] `executed_effects=0` on every turn.
- [ ] The remaining pending candidate is still in `pending_candidates` after the inert
  turn — unchanged.

## Results / Interpretation

_To be filled in after the first human review run._

## Failure Modes

- `propose_goal_candidates` uses keyword matching against tension ids and summaries;
  questions using synonyms or domain-specific phrasing may not match any tension.
- Candidate ids are derived from question content (slug); duplicate questions would
  produce duplicate ids.

## Follow-Up Questions

- **Selector integration**: How should `accepted_candidates` wire into
  `select_goals_with_salience` — merged into fixture goals or as a parallel selector
  layer?
- Should accepted candidates inherit `activation_keywords` from their matched tensions
  rather than defaulting to empty?
- Should a cap on `pending_candidates` length be introduced once the experiment reveals
  accumulation risks?
