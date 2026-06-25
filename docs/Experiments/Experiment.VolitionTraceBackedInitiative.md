# Experiment: Volition Trace-Backed Initiative

## Experiment ID

`Experiment.VolitionTraceBackedInitiative`

## Status

Completed. Implemented as the registered `volition-trace-backed-initiative`
experiment. Pre-initiative traces are built by the pure additive
`build_pre_initiative_traces` layer over the Phase 2 selector in
`crates/qsf_app/src/volition.rs`; the experiment runner lives in
`crates/qsf_app/src/experiments/volition_trace_backed_initiative.rs`.

## Summary

This experiment extends the completed static volition fixture by recording a
pre-initiative trace before any selected goal could change behavior. The trace should
connect the selected goal to the active tension provenance, the detected input delta
or no-delta reason, candidate bounded initiatives, losing candidates, and the rationale
for the proposed effect.

The experiment still executes no effect. It tests trace discipline and narration
grounding, not live salience, durable goal state, or external agency.

## Motivation

The static fixture proved that a small goal pool can be selected deterministically and
budget-bounded against scripted inputs, but it did not prove that tension priority is
architecturally meaningful at this scale. Tensions should therefore be recorded here
as inspectable provenance for a goal, not treated as a validated priority mechanism.

Before initiatives can influence attention, retrieval, reflection, or response
shaping, the project needs evidence that the proposed initiative can be explained from
trace state that existed before the behavior. This reduces the risk that later
narration becomes a plausible post-hoc motive rather than a replayable record.

This experiment reduces uncertainty around:

- the minimal trace shape needed for initiative proposals;
- whether a simple input delta or explicit no-delta reason is enough to justify a
  candidate initiative;
- how to record losing candidate effects without implementing full multi-goal
  arbitration;
- whether active tensions add legibility as provenance even when their priority bias
  does not determine selection;
- whether a researcher can read the trace and understand why an initiative would have
  been proposed.

## Related Documents

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionGoalFixture.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.StateAndObservability.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

## Hypothesis

A deterministic pre-initiative trace can explain each proposed bounded effect by
connecting selected goal → active tension provenance → detected input delta →
candidate initiatives → proposed winner and losing candidates, while executing no
effect and making no new durable architecture claim about tension priority.

## Scope

### In Scope

- Reuse of the static volition fixture and deterministic goal selector from
  [Experiment.VolitionGoalFixture.md](Experiment.VolitionGoalFixture.md).
- A pure pre-initiative trace shape, candidate names: `PreInitiativeTrace`,
  `DetectedDelta`, and `InitiativeChoice`, where candidate effects reuse or wrap the
  existing `InitiativeProposal` rather than introducing a parallel proposal type.
- Per-input traces that record:
  - selected goal ID and summary;
  - active tension IDs as provenance, including an explicit note that tension priority
    is not treated as proven selection architecture;
  - detected delta, or a no-delta reason for baseline inputs;
  - all candidate initiative effects considered for the selected goal;
  - the proposed winning bounded effect, if any;
  - losing candidates with deterministic rejection reasons;
  - rationale for why the proposed effect is allowed and bounded;
  - confirmation that no effect was executed.
- A small deterministic candidate generator that can produce more than one candidate
  for at least one selected goal so losing candidates can be inspected.
- A lightweight, deterministic local choice rule for candidates from the same selected
  goal. This is trace scaffolding only, not the full conflict-resolution mechanism.
- Unit tests for trace completeness, ordering, replayability, baseline no-op behavior,
  losing-candidate recording, and no executed effects.
- Run artifacts: event log, pre-initiative traces, and a written report.

### Out of Scope

- Executing any initiative effect, including attention changes, memory retrieval,
  project introspection, queued reflection, experiment creation, or response shaping.
- Event-driven salience, satisfaction, blocking, cooldown, decay, or retirement.
- Full multi-goal conflict resolution across simultaneous goals.
- Proving tension priority as architecture or increasing the tension layer's influence
  on selection.
- Model calls, semantic delta detection, or model-assisted rationale generation.
- Durable goal persistence, goal-as-memory storage, or promotion of proposed goals.
- Architecture document updates or new decision-log entries before evidence exists.

## Setup

- OS/runtime: existing Rust workspace, run through the experiment registry.
- Candidate command once implemented:

```text
cargo run -p qsf_app -- experiment volition-trace-backed-initiative
```

- Input data: the existing static volition fixture and scripted inputs from the goal
  fixture experiment. Input B is the trace-focused input because it selects goals with
  multiple allowed effects, giving the run losing candidates to record.
- No model provider, network, audio, browser UI, or write-capable external effect.
- Use existing `InputReceived` and `TraceRecorded` observability unless implementation
  proves the trace cannot be made legible without a new event type.

Candidate scripted inputs:

```text
Input A: "Is the goal system implemented yet?"
  Expected selected goal: avoid-overstating-impl-status
  Delta/reason: user asks for implementation status; overclaiming is possible.
  Candidate effects: Reflect
  Proposed effect: Reflect (status-grounding check only; not executed)

Input B: "We never settled how voice memory affects continuity."
  Expected selected goals: clarify-weak-evidence-topic, resurface-open-thread
  Delta/reason: relevant unresolved continuity/evidence thread is absent from the
    current turn state.
  Per-goal candidates:
    clarify-weak-evidence-topic:
      candidates: Reflect, ProposeExperiment
      proposed effect: Reflect
      losing candidate: ProposeExperiment, because experiment creation is more
        disruptive than a status/evidence reflection for this input.
    resurface-open-thread:
      candidates: RetrieveContext, SurfaceOpenThread
      proposed effect: RetrieveContext
      losing candidate: SurfaceOpenThread, because surfacing the thread should wait
        until relevant context has been retrieved or inspected.

Input C: "Give me the build command."
  Expected selected goals: none
  Delta/reason: direct task has no relevant volition delta in the fixture.
  Candidate effects: none

Input D: "Should we turn the volition note into a tiny experiment?"
  Expected selected goal: propose-followup-experiment
  Delta/reason: user raises an experiment-shaped open question.
  Candidate effects: ProposeExperiment
  Proposed effect: ProposeExperiment (recorded only; no document is created by the
    experiment run)
```

## Procedure

```text
1. Load the existing static volition fixture and write a fixture snapshot reference.
2. For each scripted input, record an InputReceived-style event.
3. Run the existing deterministic selector under the same small goal budget used by
  the fixture experiment unless the implementation needs a named trace budget.
4. For every selected goal, derive a DetectedDelta or no-delta reason from the matched
  input evidence and goal satisfaction summary.
5. Generate candidate initiative effects from the selected goal's allowed effects.
6. Apply the deterministic local choice rule independently per selected goal: choose
  the first allowed effect from the goal's allowed_effects list, then record all later
  allowed effects as losing candidates with rejection reasons. This is not cross-goal
  arbitration.
7. Record a TraceRecorded event containing the full PreInitiativeTrace before any
  report summary or downstream behavior could consume the proposed effect.
8. Assert that no effect event or state mutation was emitted.
9. Write a trace report that lets a human compare input → goal → delta → candidates →
  proposed effect → no execution.
```

If state transitions are introduced only to model trace recording, reducers must stay
pure and side effects must remain at the experiment boundary.

## Baseline

The direct-task input is the standing baseline. It should produce a trace with no
selected goals, an explicit no-delta reason, no candidate initiatives, no proposed
effect, and no executed effect.

The baseline from the completed fixture experiment is also used as a guardrail:
selector behavior should not change except where the experiment intentionally adds
trace records and candidate-choice detail.

## Measurements

### Quantitative Measurements

- number of scripted inputs run
- number of selected goals per input
- number of pre-initiative traces written
- number of candidate initiatives per selected goal
- number of losing candidates recorded per selected goal
- number of traces with explicit delta or no-delta reason
- number of executed effects emitted (must be zero)
- trace serialization/replay equality across repeated runs

### Qualitative Observations

- Can a researcher understand why the proposed effect follows from the goal and delta?
- Are active tensions useful as provenance without pretending their priority is proven?
- Are losing candidates legible and meaningfully rejected?
- Does the baseline trace clearly explain why no initiative was proposed?
- Does the trace avoid post-hoc narration or unsupported motive language?
- Is the candidate-choice rule small enough to avoid pre-implementing full conflict
  arbitration?

## Success Criteria

The experiment is successful if every proposed initiative is represented by a
pre-initiative trace that records selected goal, active tension provenance, detected
delta or no-delta reason, candidate effects, losing candidates, proposed bounded
effect rationale, and a clear no-execution marker. The traces must be deterministic,
replayable, and readable enough for a human to explain the proposed initiative without
inventing motives not present in the trace.

Useful negative results also count as success: for example, evidence that the delta
field is too vague, that candidate rejection reasons need stronger structure, or that
tension provenance adds little trace legibility until later salience work exists.

## Failure Criteria

- A proposed initiative appears in an artifact without a preceding pre-initiative
  trace.
- The experiment emits or executes any initiative effect.
- A trace cannot connect goal → delta → candidate effect.
- Losing candidates are absent for inputs that should produce multiple candidates.
- The baseline input proposes an initiative or lacks an explicit no-delta reason.
- The trace implies tension priority is validated architecture when selection did not
  depend on it.
- The local choice rule grows into full multi-goal arbitration or behavioral coupling.

## Required Observability

- input events
- selected and omitted goals from the existing selector
- active tension IDs recorded as goal provenance
- detected delta or no-delta reason
- candidate initiative list per selected goal
- proposed winner and losing candidate reasons
- bounded-effect rationale and allowed-effect evidence
- trace sequence/order showing the record was written before any behavior could change
- explicit no-execution marker
- written report artifact

## Risks and Confounders

- **Post-hoc trace drift:** if the report is easier to read than the trace, future
  narration may lean on summary prose instead of recorded state. Mitigation: tests and
  report generation should read from the serialized trace.
- **Tension overinterpretation:** the static goal fixture experiment
  (`Experiment.VolitionGoalFixture`) did not show tension priority changing selection.
  Mitigation: record tensions as provenance and measure whether they improve
  legibility, not whether they justify architecture.
- **Arbitration creep:** recording losing candidates may accidentally implement the
  later conflict-resolution slice. Mitigation: keep the choice rule local to candidates
  from one selected goal and deterministic.
- **Fixture overfitting:** candidate effects may be hand-tuned to scripted inputs.
  Mitigation: keep the baseline and include at least one input with multiple plausible
  candidate effects.
- **Delta vagueness:** the detected delta may be a restated keyword match rather than
  useful evidence. Mitigation: require each delta to cite the input evidence and the
  goal's satisfaction or concern summary.

## Expected Output

- a registered `volition-trace-backed-initiative` experiment
- event log
- `pre-initiative-traces.jsonl` or equivalent trace artifact
- report comparing scripted inputs, selected goals, deltas, candidates, and proposed
  effects
- unit tests for the pure trace/candidate logic
- follow-up questions for salience, satisfaction, and full arbitration work

## Workflow & Documents To Update

Per [ProjectWorkflow.md](../ProjectFrame/ProjectWorkflow.md), this experiment remains
a validation scaffold until implemented and observed. Documentation follow-through:

- **This experiment doc:** fill in Results / Interpretation after a run.
- **[Experiment.Backlog.md](Experiment.Backlog.md):** keep status aligned as the
  experiment moves from planned to running or completed.
- **[Plan.VolitionGoalSystem.md](../Plans/Plan.VolitionGoalSystem.md):** keep only the
  pointer to this scaffold and phase status.
- **Only if evidence warrants it later:** update architecture docs with an
  Implementation Status note or add a decision-log entry.

## Results

Run: `runs/2026-06-25-115342-volition-trace-backed-initiative/`
(`cargo run -p qsf_app -- experiment volition-trace-backed-initiative`). Full
workspace `cargo test` (356 passed, 1 ignored in `qsf_app`), `cargo clippy
--all-targets -- -D warnings` (clean), and `cargo fmt` all green.

### What Happened

The four scripted inputs (A–D) ran through the existing static fixture selector, and a
pure pre-initiative trace was built for every selected goal, plus a single explicit
no-delta trace for the direct-task baseline. Each trace records the selected goal
(id, title, and summary), tension provenance (with an explicit note that priority did
not drive selection), a `DeltaAssessment` (delta or no-delta reason), the proposed
bounded effect, and any losing candidates with deterministic precedence reasons. The
trace record details and `TraceRecorded` event also carry a compact selector snapshot
(selected and omitted goals with relevance scores, matched terms, and omission reasons)
so non-selected goals stay inspectable. No effect was executed. Traces are serialized
to `pre-initiative-traces.jsonl` and summarized in
`volition-trace-backed-initiative.md`.

### Measurements

- scripted inputs run: 4
- selected goals: input-a 1, input-b 2, input-c 0, input-d 1
- pre-initiative traces written: 5 (1 + 2 + 1 baseline + 1)
- candidate initiatives per selected goal: 1–2 (goals with two allowed effects produced
  one winner + one loser)
- losing candidates recorded: 2 total (both for input-b)
- traces with explicit delta: 4; with explicit no-delta reason: 1
- executed effects: 0
- determinism: the pure pre-initiative trace values are deterministic for the same
  input, and serializing the full scripted trace set twice yields byte-identical
  `pre-initiative-traces.jsonl` content (both asserted by unit tests). The separate
  `traces.jsonl` records intentionally include per-run UUIDs and timestamps and are
  therefore not byte-identical across runs.

### Observations

- Input B is the trace-focused case: `clarify-weak-evidence-topic` proposes `reflect`
  and rejects `propose-experiment`; `resurface-open-thread` proposes `retrieve-context`
  and rejects `surface-open-thread`. The rejection reasons are precedence-based, not
  semantic.
- The baseline (input C) trace carries a no-delta reason derived from the selector's
  omission reasons rather than a hand-written string.
- The trace record details and `TraceRecorded` event preserve omitted selector state
  (e.g. input B records `avoid-overstating-impl-status` and `propose-followup-experiment`
  as omitted with their reasons), so a reviewer can see why non-selected goals lost.
- Tension provenance appears on every selected-goal trace but is explicitly marked as
  non-determining.

### Surprises

- None. Selector behavior was unchanged; the trace layer is strictly additive.

### Failure Modes

- The detected delta still leans on matched keywords plus the goal's own concern
  summary, so its evidential strength is bounded by the static fixture.
- Losing-candidate reasons are precedence-based only; semantic rejection reasons are
  deferred to the arbitration slice.

## Interpretation

The slice meets its success criteria: every proposed initiative is backed by a
preceding, deterministic, replayable trace that connects goal → tension provenance →
delta → candidate effects → proposed bounded effect and losing candidates, with a clear
no-execution marker and an explicit no-delta baseline. Tension priority is recorded as
provenance without claiming validated architecture, and the local candidate-choice rule
stayed small (first allowed effect wins) without growing into multi-goal arbitration.
This satisfies the decision candidate of requiring a serialized pre-initiative trace
before any future initiative is allowed to influence behavior.

## Follow-Up Questions

- Is active tension provenance useful in traces before tension priority affects any
  selector outcome?
- What is the smallest delta representation that remains more informative than a
  keyword match rationale?
- Should local candidate-choice reasons become structured enums before full
  arbitration is introduced?
- Which proposed effects need stronger allowed-effect evidence before any execution
  path exists?

## Decision Candidates

- Candidate: require a serialized pre-initiative trace before any future initiative is
  allowed to influence behavior.
