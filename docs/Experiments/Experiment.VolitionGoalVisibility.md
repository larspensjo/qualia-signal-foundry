# Experiment: Volition Goal Visibility

## Experiment ID

`Experiment.VolitionGoalVisibility`

## Status

Offline-validated (2026-07-06). The scaffold and trace contract were written before implementation
(per ProjectWorkflow and Agents.md); the conscious/subconscious visibility slice of
[Plan.VolitionMotivationalTexture.md](../Plans/Plan.VolitionMotivationalTexture.md) is now built and
the offline harness passes with artifact re-derivation. Browser and live-voice review remain open —
see Results and Final Status.

## Summary

This experiment tests whether a goal can carry a `Subconscious` visibility attribute that biases
salience and arbitration **exactly** like any other goal, yet is narrated only on introspection
or when its behavior forces an explanation — without adding a separate runtime path. Visibility
is an introspection-*surfacing filter*, not a second selection/arbitration engine
([DecisionLog 2026-07-06](../DecisionLog.md#2026-07-06---subconscious-volition-goals-use-reduced-ambient-exposure)).

The forced-surfacing conditions are recorded facts only: a subconscious goal is surfaced when it
renders an initiative line this turn, or when it is named as the conflicting goal in a
`DeclinedCandidate`. An `inspect_volition_state` call is itself the ask for introspection, so the
tool always reports subconscious goals — in a separate labeled section, never silently merged.

## Motivation

The realtime volition system already reads goals, salience, arbitration, and shaping. It cannot
yet hold a *background disposition* that shapes behavior while staying out of ordinary narration.
This experiment reduces uncertainty about whether such a disposition can be made behaviorally
meaningful (reduced ambient exposure) while remaining fully inspectable to the operator, traces,
and explicit introspection — with no anthropomorphic "hidden feeling" claim (guardrail D4).

## Related Documents

- [Plan.VolitionMotivationalTexture.md](../Plans/Plan.VolitionMotivationalTexture.md)
- [Design.VolitionBriefReconciliation.md](../Plans/Design.VolitionBriefReconciliation.md)
- [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md)
- [DecisionLog.md](../DecisionLog.md)
- [Experiment.VolitionEmotionLikeSignals.md](Experiment.VolitionEmotionLikeSignals.md) (the
  derive-on-demand, presence-and-absence, artifact-re-derivation patterns this experiment reuses)

## Hypothesis

A `GoalVisibility` attribute plus a pure `qsf_volition::visibility` derivation can (1) leave
`select_goals_ranked` and `arbitrate_with_mode` bit-identical whatever the visibility mix, and
(2) drive a surfacing filter that hides ordinary subconscious goals from simulator-facing status
lists and reduces their ambient turn text, while forced-surfacing (rendered initiative or
coherence conflict) and explicit introspection still expose full, evidence-backed detail.

## Scope

### In Scope

- `GoalVisibility` enum (`Conscious` | `Subconscious`) on `Goal` and `ProposedGoalCandidate`,
  `#[serde(default) = Conscious]` for back-compat; one subconscious seed goal in
  `realtime_seed_fixture` (a non-protected background-disposition goal).
- Reducer-backed rendered-initiative evidence (`last_initiative_tick`,
  `last_rendered_initiative_tick`, `last_rendered_initiative_ref`) distinguishing a rendered
  initiative line from a suppressed internal one.
- Pure `qsf_volition::visibility` deriving forced-surfacing (rendered-initiative + coherence
  conflict) on demand, never stored.
- Sectioned `inspect_volition_state` / `select_volition_goals` output (`subconscious_goals`),
  operator-panel badges, turn-trace visibility fields, and reduced ambient injection for ordinary
  subconscious winners.
- Offline `volition-goal-visibility` harness with artifact re-derivation.

### Out of Scope

- Any runtime path that flips a goal's visibility (D3: definitions are runtime-immutable).
- Live-formed subconscious candidates (`LiveGoalFormationJudge` output stays `Conscious`); a
  sleep-consolidation path forming subconscious goals.
- Visibility feeding selection, arbitration, salience, the surfacing gate's decision logic, or
  coherence — visibility is presentation only.
- New event types or event-log format changes.

## Setup

- Rust workspace default build.
- `qsf_app` offline experiment runner.
- `qsf_realtime_server` browser UI for operator-panel badges.
- Deterministic scripted fixtures; no live model required for the offline harness.

## Procedure

1. Add `GoalVisibility` and the `visibility` field (serde-default) to `Goal`,
   `ProposedGoalCandidate`, the seed fixture (one subconscious goal), and `GoalStatusSummary`.
2. Add reducer-backed rendered-initiative evidence and implement pure
   `qsf_volition::visibility::forced_surfaced_goals`, with presence/absence unit tests including a
   suppressed `InitiativeExecuted` that must **not** force surface, and the no-runtime-effect
   invariant test.
3. Section the `inspect_volition_state` / `select_volition_goals` tool output.
4. Badge the operator panel; keep it fully unfiltered.
5. Add visibility fields to the turn trace; persist the rendered-initiative fact.
6. Reduce ambient injection for ordinary subconscious winners; keep conscious and forced-surfaced
   winners at full detail.
7. Register and run `volition-goal-visibility`; parse artifacts and verify the trace contract.

## Baseline

The current volition system, in which every goal is uniformly narrated wherever it is selected or
wins arbitration, with no notion of a background disposition and no subconscious section in the
introspection tools.

## Measurements

### Quantitative Measurements

- Number of subconscious goals correctly filtered from simulator-facing status lists.
- Number of forced-surfacing conditions correctly derived (and suppressed cases correctly *not*
  derived).
- Ambient exposure treatment (`ordinary`, `reduced_subconscious`, `forced_surfaced_subconscious`)
  per turn.
- Artifact parser pass/fail for required trace fields; harness re-derivation pass/fail.
- Invariant check: identical `select_goals_ranked` / `arbitrate_with_mode` under a visibility flip.
- UI parser/view-model test pass/fail.

### Qualitative Observations

- Whether the operator panel badges and sections read clearly and hide nothing.
- Whether a sectioned introspection reply reads as an honest instrument readout of a background
  tendency, not a claimed hidden feeling.
- Whether reduced ambient text still gives the response model enough to shape coherently.

## Success Criteria

- `select_goals_ranked` and `arbitrate_with_mode` produce identical results when a goal's
  visibility is flipped `Conscious` ↔ `Subconscious` with all else equal.
- `qsf_volition::visibility` is pure, deterministic, derived on demand, and never stored.
- An ordinary subconscious goal (selected, no forcing condition) is absent from the simulator-facing
  status lists and present, badged, on the operator capture.
- A subconscious goal that renders an initiative line, or is named in a coherence decline, is
  reported as forced surfaced with resolvable evidence; a *suppressed* internal initiative is not.
- `inspect_volition_state` reports subconscious goals only in the labeled `subconscious_goals`
  section; `select_volition_goals` keeps `arbitration` truthful, adds `winner_visibility`, and
  places subconscious selected goals in `subconscious_goals` with their selection role.
- Ordinary subconscious winners render reduced ambient text; conscious and forced-surfaced winners
  render full labeled detail; the trace keeps the full winner identity in all cases.
- The harness re-derives every surfaced subconscious goal's forcing condition from recorded state.

## Failure Criteria

- A visibility flip changes selection or arbitration output.
- A suppressed internal initiative is counted as forced surfacing.
- A subconscious goal leaks into an ordinary simulator-facing status/selected list.
- The operator panel or a trace hides a subconscious goal.
- A forcing condition depends on information not present in recorded state or trace artifacts.
- A live-formed candidate is emitted `Subconscious`, or a runtime path flips visibility.

## Required Observability

- Per-goal `visibility` on inspection summaries and on `selector_output` entries.
- Arbitration-winner visibility.
- For every surfaced subconscious goal: `goal_id`, condition kind, evidence reference, tick.
- For rendered-initiative forcing: the recorded rendered/surfaced flag, suppression reason,
  initiative tick, rendered tick, and artifact/request reference.
- Ambient exposure treatment on the turn packet trace.
- The introspection tool's full JSON output captured as a trace artifact.
- Human-readable report summarizing per-scenario outcomes.

### Trace Completeness Contract

Required fields per `goal-visibility-derivation` trace record:

```text
tick
scenario
subconscious_goal_ids
selector_output_visibility            (per selected goal: goal_id, visibility)
arbitration_winner_visibility         (winner_goal_id, visibility) or null
forced_surfaced                       ([{goal_id, condition, evidence_ref, tick}])
suppressed_initiative_not_surfaced    ([{goal_id, suppression_reason, initiative_tick}])
ambient_exposure_treatment            (ordinary | reduced_subconscious | forced_surfaced_subconscious)
introspection_tool_output_ref
dynamic_state_snapshot
artifact_or_report_reference
```

Artifact boundary:

```text
events.jsonl:
  Existing chronological experiment event stream. It records generic TraceRecorded events that
  link to trace ids. No new lifecycle event types are introduced by this experiment;
  InitiativeExecuted gains defaulted rendered-evidence fields but suppressed initiative
  executions remain distinguishable from rendered ones.

traces.jsonl:
  Lifecycle facts and derivation boundary. Each goal-visibility-derivation record carries the
  VolitionEvents needed to reconstruct the relevant state slice, the dynamic_state_snapshot,
  the derived forced-surfacing conditions, and the ambient exposure treatment. Scenario steps
  that assert a subconscious goal is NOT forced surfaced are recorded as
  goal-visibility-absence-check records carrying the same events_applied,
  dynamic_state_snapshot, and artifact reference plus expected_absent_goal_ids and
  forced_surfaced_present, so absence claims are artifact-backed and re-derivable.

human-readable report:
  Per-scenario summary and review checklist derived from the trace records.
```

Automated verification:

```text
- Parse traces.jsonl and assert every required field exists.
- Reconstruct VolitionState from the recorded dynamic_state_snapshot, independently replay the
  recorded VolitionEvents from a fixture-seeded state, and cross-check the two reconstructions
  agree.
- Re-derive forced_surfaced_goals and goal_visibility fresh and assert the traced conditions,
  evidence refs, ticks, and ambient exposure treatment match.
- Prove that a suppressed internal initiative does not force surface.
- Assert the visibility-flip invariant: select_goals_ranked / arbitrate_with_mode are identical.
- Assert events.jsonl TraceRecorded entries link to actual goal-visibility-derivation trace ids.
```

## Risks and Confounders

- "Subconscious" can read as a hidden-feeling claim if the label lacks its forcing evidence.
- Reduced ambient text could starve the response model of guidance needed for coherent shaping.
- A forced-surfacing derivation that reads `last_initiative_output` instead of the rendered flag
  would wrongly surface suppressed internal initiatives.
- The invariant is only as strong as the scenarios that exercise selection and arbitration.

## Expected Output

- `volition-goal-visibility` run artifacts.
- `traces.jsonl` records for `goal-visibility-derivation` and `goal-visibility-absence-check`.
- `events.jsonl` `TraceRecorded` links.
- Human-readable report.
- Unit, tool-layer, and UI tests covering presence, absence, sectioning, and the invariant.

## Scenarios

The offline harness drives one subconscious goal through:

- **(a) selected, biasing, no forcing condition** — absent from simulator-facing status lists,
  present with a badge on the operator capture; ambient treatment `reduced_subconscious`.
- **(b) wins arbitration with a suppressed initiative** — not forced surfaced.
- **(c) wins arbitration with a rendered initiative line** — forced surfaced; ambient treatment
  `forced_surfaced_subconscious`.
- **(d) named as the conflicting goal in a coherence decline** — forced surfaced.
- **(e) `inspect_volition_state` call** — reported in the `subconscious_goals` section.
- **Invariant** — identical selection and arbitration outcomes when the same goal is `Conscious`.

## Results

### Offline harness (validated, 2026-07-06)

`cargo run -p qsf_app -- experiment volition-goal-visibility` completed end-to-end. Artifacts land
under `runs/<timestamp>-volition-goal-visibility/`:

- `traces.jsonl` — 5 `goal-visibility-derivation` records (one per scenario), 1
  `goal-visibility-absence-check` (the suppressed-initiative proof), and 1
  `goal-visibility-invariant` record. Each derivation carries all ten required fields, the recorded
  `dynamic_state_snapshot`, the derived `forced_surfaced` set, and the `ambient_exposure_treatment`.
- `events.jsonl` — `TraceRecorded` links to the derivation trace ids.
- `goal-visibility-report.md` — per-scenario summary with a human checklist.

The in-run verifier re-derives each result rather than comparing a value to itself: it reconstructs
`VolitionState` from the recorded snapshot, independently replays the recorded events and
cross-checks the two agree, then re-derives `forced_surfaced_goals`, the ambient exposure, and the
per-selected-goal visibility and asserts they match the trace; it re-runs the visibility flip and
confirms selection/arbitration equality; and it proves the suppressed-initiative goal executed but
was not forced. RED checks confirm the re-derivation is real: tampering a not-forced scenario's
`forced_surfaced` fails with "forced_surfaced mismatch", and tampering a rendered scenario's
`ambient_exposure_treatment` fails with "ambient_exposure mismatch".

Observed per scenario: `selected-no-forcing` and `introspection-read` → `reduced_subconscious`,
0 forced; `suppressed-initiative` → `reduced_subconscious`, 0 forced (with an absence record proving
the executed initiative did not surface); `rendered-initiative` and `coherence-conflict` →
`forced_surfaced_subconscious`, 1 forced.

### Automated tests (passing)

- `qsf_volition::visibility` unit tests: rendered-initiative and coherence-conflict each force a
  subconscious goal; a suppressed initiative and a rendered initiative on a *conscious* goal do
  not; both conditions yield one entry each; and the visibility-flip invariant leaves
  `select_goals_ranked` / `arbitrate_with_mode` identical across queries.
- Reducer, model, candidate, and inspection unit tests: `GoalVisibility` serde default
  (`Conscious`), `ProposedGoalCandidate` defaulted-internal visibility with the schema-hint
  exclusion guard, rendered-vs-suppressed `InitiativeExecuted` bookkeeping, and `GoalStatusSummary`
  back-compat.
- Tool-layer tests: `inspect_volition_state` / `select_volition_goals` section subconscious goals
  (including a subconscious selected non-winner and a subconscious winner) with `winner_visibility`.
- Realtime injection/capture tests: conscious winners render the ordinary `Active goal` line;
  ordinary subconscious winners render reduced text (identity withheld) with the full winner in the
  trace; forced-surfaced subconscious winners render labeled full detail; the capture carries
  `forced_surfaced`, `winner_visibility`, and `ambient_exposure`, all back-compat-deserializing.
- UI parser/view-model tests (vitest, 91/91): visibility, winner visibility, ambient exposure, and
  forced surfacing parse and default for older captures; the panel badges subconscious goals and
  shows forced-surfacing status without hiding any.
- `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`, and `npm run check` /
  `npm run fmt` in `crates/qsf_realtime_server/ui/` are clean.

### External human testing (recommended, not yet run)

The browser operator-panel review and a live-voice introspection ask (per the acceptance criteria)
remain to be run, combined with Phase 2's still-open live formation voice test.

## Interpretation

```text
Observed: One subconscious goal biases selection and arbitration identically to a conscious goal
          (proven by the visibility-flip invariant), is sectioned out of simulator-facing lists,
          reads reduced in ordinary ambient text, and surfaces with full labeled evidence only when
          it renders an initiative or is named in a coherence conflict. Every result re-derives from
          recorded state alone; a suppressed internal initiative never forces surfacing.
Interpreted: Visibility can be implemented as an introspection-surfacing filter — a pure derivation
          over recorded facts plus a presentation choice — without a separate runtime path, keeping
          "subconscious" behaviorally meaningful while fully inspectable to the operator and traces.
Uncertain: Whether the reduced ambient text gives the live response model enough to shape coherently,
          and whether the "subconscious" label reads as an honest instrument readout in live voice,
          both await the external human session.
```

## Follow-Up Questions

- Does the operator prefer a display label other than "subconscious" for the badge?
- What live-voice wording keeps a sectioned introspection reply honest ("a background tendency I
  can report") across personas?

## Follow-Up Experiments

- A sleep-consolidation path that could form subconscious goals from cross-session reinforcement.
- Multi-turn plans (Phase 5) layered on the coherent-agent substrate.

## Decision Candidates

- Candidate: keep visibility a fixture-authored definition attribute with no runtime mutation path
  unless a later reviewed decision introduces one.

## Final Status

Offline-validated (2026-07-06). Every automated success criterion passes: the pure derivation, the
sectioned tools, the operator-panel badges, the reduced ambient injection, and the offline harness
with artifact re-derivation and the visibility-flip invariant. The browser operator-panel review and
the live-voice introspection ask remain the only open items, to be run with Phase 2's live formation
voice test.
