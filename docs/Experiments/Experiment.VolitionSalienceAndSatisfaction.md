# Experiment: Volition Salience and Satisfaction

## Experiment ID

`Experiment.VolitionSalienceAndSatisfaction`

## Status

Implemented (2026-06-25). Code is in place; run
`cargo run -p qsf_app -- experiment volition-salience-and-satisfaction` to produce
artifacts. Results section pending a run. This scaffold defines the scope for the
salience/satisfaction/blocking/cooldown slice of the volition system; the phase
sequencing lives in
[Plan.VolitionGoalSystem.md](../Plans/Plan.VolitionGoalSystem.md), and the rationale,
terminology, lifecycle states, and candidate state shapes live in
[Idea.VolitionGoalSystem.md](../Plans/Idea.VolitionGoalSystem.md). Volition is recorded
as a research surface in [DecisionLog.md](../DecisionLog.md) (2026-05-15 "Volition is an
explicit research surface").

## Summary

This experiment extends the completed static fixture
([Experiment.VolitionGoalFixture.md](Experiment.VolitionGoalFixture.md)) and
trace-backed initiative slice
([Experiment.VolitionTraceBackedInitiative.md](Experiment.VolitionTraceBackedInitiative.md)),
both of which are stateless, by adding the first *durable-within-a-run* volition state.
Across a scripted multi-turn sequence, events activate, progress, satisfy, block, or
weaken goals through a pure reducer, so that goal selection on a later turn depends on
what happened on earlier turns.

The experiment executes no effect and adds no write-capable external action. The only
new coupling is that per-goal salience and lifecycle status feed goal *selection*
ordering and visibility. It tests the idea doc's reward-as-evidence-backed-update
discipline: progress and satisfaction must reference observable evidence, never a model
assertion.

## Motivation

Selection so far is recomputed from scratch each turn against an immutable fixture.
Before goals can ever drive bounded internal initiative, the project needs evidence
that a small, pure, replayable state layer can:

- raise and decay salience deterministically as events arrive;
- record progress and satisfaction only from observable evidence references;
- move goals through an explicit lifecycle (`Active`, `Blocked`, `Satisfied`,
  `Cooldown`, back to `Accepted`, or `Retired`) without hidden tick logic;
- keep blocked goals visible as unresolved tensions instead of silently dropping them;
- suppress recently satisfied goals during cooldown without losing them.

This experiment reduces uncertainty around:

- the minimal `VolitionState` shape and the deterministic logical clock (`tick`);
- which events are needed and whether each lifecycle advance is its own event;
- whether a validated `EvidenceRef` is enough to keep satisfaction grounded;
- whether salience-aware selection stays legible and avoids feeling noisy or nagging;
- default decay, cooldown, and retirement thresholds that a standard run actually
  crosses.

## Related Documents

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionGoalFixture.md
Experiments/Experiment.VolitionTraceBackedInitiative.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.StateAndObservability.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

## Hypothesis

A small, pure `VolitionState` updated only through explicit `VolitionEvent` transitions
can make goal salience rise on relevant activity, decay deterministically, satisfy and
cool down on evidence-backed events, keep blocked goals visible, and retire unproductive
goals — yielding replayable per-turn snapshots and salience-aware selection — while
executing no effect and adding no external agency.

## Scope

### In Scope

- Reuse of the static fixture and deterministic selector from the goal-fixture and
  trace-backed-initiative slices.
- Extending `GoalStatus` with the runtime lifecycle states `Active`, `Blocked`, and
  `Satisfied` (keeping `Proposed`/`Accepted`/`Cooldown`/`Retired`).
- A pure `VolitionState` holding per-goal dynamic state separate from the read-only
  fixture: runtime `status`, `salience`, `reinforcement_count`,
  `progress_evidence_refs`, `last_activated_tick`, `last_satisfied_tick`, and
  `cooldown_until_tick`, seeded from the fixture's `Accepted` goals.
- A deterministic logical `tick` (monotonic, advanced per processed event) so decay and
  cooldown are replayable without wall-clock time.
- A `VolitionEvent` enum with one variant per transition so every lifecycle advance is
  explicit: `GoalActivated`, `GoalProgressObserved`, `GoalSatisfied`, `GoalBlocked`,
  `GoalDecayed` (salience-only, never a status change), `GoalCooldownElapsed`
  (`Cooldown -> Accepted`), and `GoalRetired` (`-> Retired`).
- A validated `EvidenceRef` newtype (non-empty, non-whitespace, fallible constructor)
  carried by progress- and satisfaction-bearing events and stored in
  `progress_evidence_refs`.
- A pure `apply(state, event) -> state` reducer as the only place status changes; the
  selector never mutates lifecycle.
- A salience-aware selector (candidate name `select_goals_with_salience`) that reuses
  the existing relevance scoring, adds a salience term, suppresses `Cooldown` goals, and
  keeps `Blocked` goals visible with a distinct blocked reason. The existing stateless
  selector stays unchanged.
- A registered `volition-salience-and-satisfaction` experiment that replays a scripted
  multi-turn input/event sequence, snapshots `VolitionState` per turn, and writes the
  salience/lifecycle trace.
- Unit tests for salience rise/decay, evidence-backed progress and satisfaction,
  empty-evidence rejection, cooldown suppression and elapse, blocked-goal visibility,
  retirement, irrelevant-goal inertness, and replay determinism.

### Out of Scope

- Executing any initiative effect (attention changes, memory retrieval, project
  introspection, queued reflection, experiment creation, or response shaping).
- Full multi-goal conflict resolution / arbitration across simultaneous goals.
- Wiring goal salience into the runtime `AttentionState` (deferred to the
  bounded-initiative slice).
- Reflection- or model-generated goal candidates and goal acceptance/promotion.
- Durable cross-session goal persistence or goal-as-memory storage.
- Model calls, semantic delta detection, or model-assisted evidence judgment.
- Architecture document updates or new decision-log entries before evidence exists.

## Setup

- OS/runtime: existing Rust workspace, run through the experiment registry.
- Candidate command once implemented:

```text
cargo run -p qsf_app -- experiment volition-salience-and-satisfaction
```

- Input data: the existing static volition fixture plus a scripted multi-turn sequence
  of inputs and events designed so that, in one standard run, at least one goal's
  salience rises then decays, one goal is satisfied with evidence and enters cooldown,
  one goal is blocked and stays visible, and one goal retires after staying
  unproductive past the threshold.
- No model provider, network, audio, browser UI, or write-capable external effect.
- Reuse existing `InputReceived` and `TraceRecorded` observability unless implementation
  proves a new event type is required for legibility.

## Procedure

```text
1. Load the static volition fixture and seed VolitionState from its Accepted goals.
2. For each scripted turn, record an InputReceived-style event and advance the tick.
3. At the boundary, derive the events for the turn (activation/progress/satisfaction/
   blocking from matched evidence; decay/cooldown-elapse/retirement from the tick).
4. Apply each event through the pure reducer; never mutate lifecycle in the selector.
5. Run the salience-aware selector and record selected, suppressed (cooldown), and
   visible-but-blocked goals.
6. Snapshot VolitionState after the turn and record a TraceRecorded event.
7. Assert no effect event or external mutation was emitted.
8. Write a salience/lifecycle trace report comparing per-turn salience, status, and
   selection across the sequence.
```

Reducers must stay pure; all event emission and side effects stay at the experiment
boundary.

## Baseline

- The stateless selection behavior from the prior slices is the guardrail: with an empty
  `VolitionState`, salience-aware selection must match the existing selector output.
- An irrelevant/direct-task input is the standing baseline: it must leave salience at
  zero, activate no goal, and keep irrelevant goals out of context.

## Measurements

### Quantitative Measurements

- per-goal salience trajectory across turns
- number of evidence-backed progress and satisfaction events recorded
- number of goals suppressed during cooldown and number returned after elapse
- number of blocked goals kept visible
- number of retired goals
- number of executed effects emitted (must be zero)
- state-snapshot and selection-output equality across repeated runs (replay)

### Qualitative Observations

- Does rising/falling salience track relevant activity in a legible way?
- Do cooldown suppression and blocked-goal visibility feel useful rather than noisy or
  nagging? (human judgement)
- Is the lifecycle readable from the per-turn snapshots without inferring hidden logic?
- Does the evidence discipline keep satisfaction grounded?

## Success Criteria

The experiment is successful if a scripted multi-turn run produces deterministic,
replayable `VolitionState` snapshots in which salience rises on relevant activity and
decays by the deterministic rule; progress and satisfaction are recorded only from
`EvidenceRef` evidence; satisfied goals enter cooldown and return after elapse; blocked
goals stay visible; unproductive goals retire; irrelevant goals stay inert and out of
context; and no effect is executed. Useful negative results also count: for example,
evidence that the salience term distorts selection, that default thresholds are poorly
chosen, or that blocked-goal visibility is not actually legible.

## Failure Criteria

- A goal is satisfied or progressed without an `EvidenceRef` (empty evidence accepted).
- A lifecycle transition happens without an explicit event (hidden tick or selector
  logic advances status).
- The experiment emits or executes any initiative effect.
- A blocked goal is silently dropped instead of staying visible.
- Salience-aware selection diverges from the stateless baseline when `VolitionState` is
  empty.
- Snapshots or selection output are not replayable across runs.
- The default scripted run does not exercise decay, cooldown, and retirement.

## Required Observability

- input events
- per-turn `VolitionState` snapshot (status, salience, evidence refs, ticks)
- selected, cooldown-suppressed, and blocked-but-visible goals
- the `VolitionEvent` sequence applied per turn
- explicit no-execution marker
- written salience/lifecycle trace report

## Risks and Confounders

- **Hidden lifecycle logic:** cooldown expiry or retirement done implicitly would break
  replayability. Mitigation: one explicit event per transition; the selector never
  changes status.
- **Evidence laundering:** a free-form string would let weak evidence mark satisfaction.
  Mitigation: a validated `EvidenceRef` and an empty-evidence rejection test.
- **Salience drift:** float salience could diverge across replay. Mitigation: integer
  points and a deterministic decay rule (confirm during implementation).
- **Arbitration creep:** salience-aware selection must not become multi-goal conflict
  resolution. Mitigation: keep salience a per-goal ordering term only.
- **Annoyance:** rising salience and resurfacing could feel nagging. Mitigation: human
  review of the per-turn trace, plus cooldown and retirement.

## Expected Output

- extended `GoalStatus`, a `VolitionState`, a `VolitionEvent` enum, an `EvidenceRef`
  type, and a pure `apply` reducer
- a salience-aware selector alongside the unchanged stateless selector
- a registered `volition-salience-and-satisfaction` experiment
- event log, per-turn state snapshots, and a salience/lifecycle trace report
- unit tests for the pure reducer, evidence validation, and salience-aware selection
- follow-up questions for arbitration and bounded-initiative work

## Workflow & Documents To Update

Per [ProjectWorkflow.md](../ProjectFrame/ProjectWorkflow.md), this experiment remains a
validation scaffold until implemented and observed. Documentation follow-through:

- **This experiment doc:** fill in Results / Interpretation after a run.
- **[Experiment.Backlog.md](Experiment.Backlog.md):** keep status aligned as the
  experiment moves from planned to running or completed.
- **[Plan.VolitionGoalSystem.md](../Plans/Plan.VolitionGoalSystem.md):** keep only the
  pointer to this scaffold and phase status.
- **Only if evidence warrants it later:** update architecture docs with an
  Implementation Status note or add a decision-log entry.

## Results

Pending. Not yet run.
