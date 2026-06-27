# Experiment: Volition Bounded Initiative Execution

## Experiment ID

`Experiment.VolitionBoundedInitiativeExecution`

## Status

Planned. Automated success criteria and scripted sequence are defined below. The
phase sequencing lives in [Plan.VolitionGoalSystem.md](../Plans/Plan.VolitionGoalSystem.md).
The rationale and terminology live in
[Idea.VolitionGoalSystem.md](../Plans/Idea.VolitionGoalSystem.md).

## Summary

This experiment covers two sub-slices:

1. **Selector wiring** — wire accepted candidates (from the previous phase) into
   `select_goals_with_salience` so they compete alongside fixture goals in selection
   and arbitration. Accepted candidates derive `activation_keywords` from matched
   tension id parts at proposal time (e.g. `continuity-preservation` →
   `["continuity", "preservation"]`). Their dynamic lifecycle state (`GoalDynamicState`)
   is seeded in `VolitionState::goals` at acceptance time, so salience, cooldown, and
   retirement apply immediately.

2. **Initiative execution** — translate the arbitration winner into a bounded
   `InitiativeOutput`, apply it as a `VolitionEvent::InitiativeExecuted`, and record
   the full chain: goal → delta → arbitration → execution → output. No write-capable
   external action is produced; `InitiativeExecuted` carries a purely structural record
   describing what a runtime system *would* do.

The scripted sequence covers five turns:

1. **Proposal** — `propose_goal_candidates` on one matched question; derived keywords
   verified on the produced candidate.
2. **Accept** — `GoalCandidateAccepted` seeds a `GoalDynamicState` entry and makes the
   accepted goal visible to `select_goals_with_salience`.
3. **Arbitration** — input that matches both one fixture goal and the accepted goal;
   `select_goals_with_salience` + `arbitrate` run; tier ordering verified.
4. **Execution** — `execute_initiative` called on the arbitration winner;
   `InitiativeExecuted` applied; `GoalDynamicState::last_initiative_output` verified.
5. **Outcome** — `GoalProgressObserved` or `GoalSatisfied` applied; lifecycle
   advancement verified.

`executed_effects = 0` on every turn (no external side effect).

## Motivation

After the reflection-generated candidate phase, accepted candidates exist in
`VolitionState::accepted_candidates` but are invisible to the selector. Before any
accepted candidate can influence behavior, the project needs evidence that:

- Accepted candidates wire into `select_goals_with_salience` transparently, using the
  same keyword-match and salience machinery as fixture goals.
- Keyword derivation from tension id parts is deterministic and produces plausible
  activation terms.
- The arbitration layer handles accepted candidates and fixture goals uniformly — tier
  and priority ordering is unchanged.
- The chain from a winning initiative to a structured `InitiativeOutput` is inspectable
  and replayable, with no model call or external write required.
- `executed_effects = 0` is maintained; `InitiativeExecuted` is purely a record.

## Related Documents

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionGoalFixture.md
Experiments/Experiment.VolitionTraceBackedInitiative.md
Experiments/Experiment.VolitionSalienceAndSatisfaction.md
Experiments/Experiment.VolitionArbitrationConflict.md
Experiments/Experiment.VolitionReflectionGoalCandidates.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

## Hypothesis

Accepted candidates wired into the selector via tension-derived activation keywords will
be selected, arbitrated, and executed on the same code paths as fixture goals, producing
an inspectable `InitiativeOutput` for the arbitration winner without requiring a model
call or external action. All four `AllowedEffect` → `InitiativeOutput` variant mappings
are verified by direct unit tests on `execute_initiative`; the scripted sequence
exercises the single winning path.

## What This Experiment Does NOT Measure

- Whether the derived activation keywords are semantically optimal (tension id parts are
  a starting point; richer derivation is a future decision).
- Whether `InitiativeOutput` content is high-quality (scripted output only; no model
  judgment).
- Cross-session goal persistence — accepted candidates and their lifecycle state are
  session-local in this phase.
- Initiative repetition, cooldown, or annoyance under sustained use (covered by future
  experiment work).

## Scripted Inputs

One question used throughout (matched to `continuity-preservation`):

```text
"Is continuity preserved across sessions?"
```

Derived activation keywords: `["continuity", "preservation"]`.

Turn 3 arbitration input is chosen to match both the accepted goal (via `continuity` or
`preservation`) and a fixture goal (e.g. `resurface-open-thread` via `thread` or
`open`):

```text
"The open continuity thread has not been resolved across sessions."
```

## New Code

- `ProposedGoalCandidate` gains `activation_keywords: Vec<String>` field; `try_new`
  accepts it; `into_goal()` passes it through to `Goal::activation_keywords`.
- `ProposedGoalCandidateRaw` (the serde shadow struct) gains `activation_keywords:
  Vec<String>` annotated with `#[serde(default)]`. Phase 6 run artifacts that lack
  the field deserialize as empty-keyword candidates rather than failing. An empty
  keyword list never matches any input, so Phase 6 artifacts are preserved as
  historical output without influencing the selector.
- `propose_goal_candidates` populates `activation_keywords` by splitting each matched
  tension id on `-` (e.g. `continuity-preservation` → `["continuity", "preservation"]`).
- `GoalCandidateAccepted` reducer branch additionally inserts `GoalDynamicState::initial()`
  into `state.goals` for the accepted goal id.
- `select_goals_with_salience` gains a second pass over `state.accepted_candidates.values()`
  after `fixture.goals`, using `state.goals` for dynamic state lookup (identical logic).
- New `InitiativeOutput` enum (one variant per `AllowedEffect`):
  ```rust
  pub enum InitiativeOutput {
      ReflectionRequested { proposed_question: String },
      ContextRetrievalRequested { query_terms: Vec<String> },
      ExperimentProposed { hypothesis: String, scope: GoalScope },
      OpenThreadSurfaced { thread_summary: String },
  }
  ```
- `GoalDynamicState` gains `last_initiative_output: Option<InitiativeOutput>`.
- New `VolitionEvent::InitiativeExecuted { goal_id, effect, output, rationale, tick }`.
  Reducer: sets status to `Active`, records `last_activated_tick`, stores output in
  `GoalDynamicState::last_initiative_output`. No-op if the goal id is unknown (no panic).
- New pure function `execute_initiative(initiative: &InitiativeProposal, goal: &Goal) -> InitiativeOutput`.
  Maps `AllowedEffect` → `InitiativeOutput` deterministically from goal summary, title,
  and matched terms; no model call.
- New `volition-bounded-initiative-execution` experiment registered in the experiment
  registry.

## Success Criteria

### Automated (all must pass before the experiment is considered complete)

- [ ] Accepted candidate's `activation_keywords` are non-empty and derived from its
  matched tension id parts.
- [ ] `GoalCandidateAccepted` reducer inserts a `GoalDynamicState` entry into
  `state.goals` for the accepted goal id.
- [ ] Accepted candidate appears in `select_goals_with_salience` output when the input
  matches its derived keywords.
- [ ] Accepted candidate does NOT appear in `select_goals_with_salience` output when no
  input keyword matches (same gate as fixture goals).
- [ ] Accepted candidate competes in `arbitrate` alongside fixture goals; tier ordering
  is determined by the shared fixture tensions.
- [ ] `execute_initiative` is deterministic: same `InitiativeProposal` + `Goal` →
  identical `InitiativeOutput`.
- [ ] Each `AllowedEffect` variant maps to the correct `InitiativeOutput` variant
  (covered by direct unit tests on `execute_initiative`; the scripted sequence
  exercises the arbitration winner's path only).
- [ ] `InitiativeExecuted` stores the output in `GoalDynamicState::last_initiative_output`.
- [ ] `InitiativeExecuted` sets the goal's status to `Active` and records
  `last_activated_tick`.
- [ ] The accepted goal's salience, cooldown, and lifecycle transitions are managed by
  the same reducer branches as fixture goals (no parallel code path).
- [ ] All prior tests pass; existing selector and reducer behaviour is unchanged.
- [ ] `cargo test` and `cargo clippy --all-targets -- -D warnings` pass.
- [ ] `executed_effects = 0` on every experiment turn.
- [ ] Replay produces identical state and event logs.

### Human (requires running the experiment and reading the report)

- [ ] The accepted candidate is visually distinct from fixture goals in the selection
  output (source identifiable from `source_reference` and `evidence_refs`).
- [ ] The derived activation keywords read as natural match terms for the source question.
- [ ] The `InitiativeOutput` for each `AllowedEffect` variant reads as a plausible
  internal action — a reflection question, a retrieval query, an experiment hypothesis,
  or a thread summary.
- [ ] The per-turn trace chain — goal → tension provenance → delta → arbitration →
  `execute_initiative` output — answers "why did this initiative execute?" without
  requiring external context.
- [ ] Nothing in the output implies that the initiative caused a real external
  write-capable action.

## Results / Interpretation

_To be filled in after the first human review run._

## Failure Modes

- Keyword derivation from tension id parts may be too coarse: a question about "memory
  continuity" matched to `continuity-preservation` gets keywords `["continuity",
  "preservation"]` but not `"memory"`, so inputs containing only `"memory"` would not
  activate the accepted goal.
- The selector's second pass over `state.accepted_candidates` must not inadvertently
  expose Cooldown or Retired accepted goals; the same status-gate logic used for fixture
  goals must apply.
- An accepted goal whose `tension_ids` name tensions not in the current fixture will
  have `effective_tier = u8::MAX` in arbitration — it will always lose. This is correct
  behaviour but could be surprising if the tension fixture is later modified.

## Follow-Up Questions

- Should activation keyword derivation be enriched (e.g. include normalized question
  terms in addition to tension id parts)?
- When should an accepted candidate's `GoalDynamicState` be removed from `state.goals`
  if the goal is later rejected from `accepted_candidates`? (Currently there is no
  "un-accept" path.)
- Cross-session persistence: if an accepted candidate should survive across sessions,
  which fields belong in live state and which in a durable memory record? (Deferred from
  this phase per the cross-cutting open question in the plan.)
- Should a single accepted candidate be able to produce multiple `InitiativeExecuted`
  events across turns, or should execution require re-activation each time?
