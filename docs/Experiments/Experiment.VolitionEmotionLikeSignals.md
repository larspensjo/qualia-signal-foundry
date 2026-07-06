# Experiment: Volition Emotion-Like Signals

## Experiment ID

`Experiment.VolitionEmotionLikeSignals`

## Status

Offline validated (2026-07-06); live check pending. The reducer lifecycle facts, the pure
`derive_signals` module, the `volition-emotion-signals` harness with artifact re-derivation, the
operator-panel realtime surfacing, and the UI "Functional signals" section are implemented, and
every automated success criterion passes. The live browser check (Procedure step 6) and the
qualitative operator interpretability review have not been run. See Results.

## Summary

This experiment tests whether the volition system can derive evidence-backed functional
signals from recorded goal lifecycle state and show them in the operator panel without feeding
them back into arbitration, initiative, context injection, or model-visible introspection.

The first signal set is `coherence_decline`, `frustration`, `satisfaction`, and `boredom`.
True D4 `tension` is reserved for a future unresolved current-conflict substrate.

## Motivation

The realtime volition panel can show goals, salience, arbitration, and shaping, but it does
not yet summarize motivational texture as named, evidence-derived signals. This experiment
reduces uncertainty about whether those summaries can remain honest instrument readouts rather
than anthropomorphic claims.

## Related Documents

- [Plan.VolitionMotivationalTexture.md](../Plans/Plan.VolitionMotivationalTexture.md)
- [Design.VolitionBriefReconciliation.md](../Plans/Design.VolitionBriefReconciliation.md)
- [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md)
- [DecisionLog.md](../DecisionLog.md)

## Hypothesis

Pure derivation from `VolitionState` plus fixture data can produce useful functional signal
rows, each with non-empty evidence resolving to recorded state, while keeping signals confined
to offline traces and the realtime operator panel.

## Scope

### In Scope

- Reducer lifecycle facts needed for exact derivation:
  `blocked_count`, `last_blocked_tick`, and `last_satisfied_evidence_ref`.
- Pure `derive_signals(state, fixture)` over recorded state.
- Offline `volition-emotion-signals` harness and artifact parser.
- Top-level realtime `VolitionInspectionCapture.signals` for the operator panel.
- UI parser/view-model rows that include evidence text for every signal.

### Out of Scope

- Feeding signals into arbitration, salience, selection, initiative, or context injection.
- Exposing signals through `VolitionStateInspection` or the `inspect_volition_state` tool.
- True D4 `tension` from unresolved selected-goal conflict.
- Curiosity, attachment, sustained N-tick boredom, and model-authored signal narration.

## Setup

- Rust workspace default build.
- `qsf_app` offline experiment runner.
- `qsf_realtime_server` browser UI for operator-panel surfacing.
- Deterministic scripted fixtures; no live model is required for the offline harness.

## Procedure

1. Add reducer lifecycle facts with serde defaults.
2. Implement pure signal derivation and unit tests for presence and absence.
3. Register and run `volition-emotion-signals`.
4. Parse generated artifacts and verify the trace contract.
5. Add top-level realtime capture signals and UI view-model rows.
6. Run a short realtime browser check where the operator can inspect evidence-backed rows.

## Baseline

The baseline is the current volition inspection panel, which shows lifecycle and arbitration
state but has no derived functional signal rows.

## Measurements

### Quantitative Measurements

- Number of expected signal records emitted.
- Number of absence cases correctly producing no signal.
- Artifact parser pass/fail for required trace fields.
- UI parser/view-model test pass/fail.

### Qualitative Observations

- Whether panel rows read as evidence-backed instrument output.
- Whether labels avoid implying subjective feeling.
- Whether evidence text is enough to understand why a signal appeared.

## Success Criteria

- `derive_signals` is pure and deterministic.
- Every emitted signal carries non-empty evidence resolvable from recorded state.
- The harness can re-derive every traced signal from `traces.jsonl`.
- No code path outside the harness and operator capture consumes signals.
- The realtime panel displays signal rows without exposing them through `inspect_volition_state`.

## Failure Criteria

- A signal depends on information not present in recorded state or trace artifacts.
- Cold-start state emits boredom.
- Satisfaction evidence cannot be distinguished from progress-only evidence.
- A model-visible tool or context-injection path receives signals in this phase.

## Required Observability

- Applied lifecycle `VolitionEvent`s relevant to each derivation.
- Dynamic state snapshot used for derivation.
- Signal kind, intensity, thresholds, and evidence.
- Trace ids linked from `events.jsonl` using existing `TraceRecorded` event patterns.
- Human-readable report summarizing per-signal outcomes.

### Trace Completeness Contract

Required fields per `emotion-signal-derivation` trace record:

```text
tick
signal_kind
intensity
thresholds_used
evidence
events_applied
dynamic_state_snapshot
artifact_or_report_reference
```

Artifact boundary:

```text
events.jsonl:
  Existing chronological experiment event stream. It records generic TraceRecorded events that
  link to trace ids; it does not become a new lifecycle-event log format.

traces.jsonl:
  Lifecycle facts and derivation boundary for this experiment. Each emotion-signal-derivation
  trace record carries the VolitionEvents needed to reconstruct the relevant state slice, the
  dynamic_state_snapshot, and the emitted signal evidence.

  Scenario steps that assert a signal is absent are recorded as emotion-signal-absence-check
  trace records, carrying the same events_applied, dynamic_state_snapshot, and
  artifact_or_report_reference payload plus expected_absent_kinds and signals_present, so
  absence claims are artifact-backed and re-derivable like presence claims.

human-readable report:
  Summary and review checklist derived from trace records.
```

Automated verification:

```text
- Parse traces.jsonl and assert required fields exist.
- Re-derive signals from the included dynamic state snapshot, replaying included VolitionEvents
  where needed by the scenario.
- Assert traced signal kind, intensity, thresholds, and evidence match the re-derived result.
- Assert events.jsonl TraceRecorded entries link to actual emotion-signal-derivation trace ids.
```

## Risks and Confounders

- Names like boredom and frustration can read as subjective claims if evidence text is weak.
- Windowed historical state can make old coherence declines look current unless tick/age is shown.
- Constants chosen for fixture coverage may not be good live defaults.
- Panel-only signals may still invite later model narration if the boundary is weakened.

## Expected Output

- `volition-emotion-signals` run artifacts.
- `traces.jsonl` records for `emotion-signal-derivation`.
- `events.jsonl` `TraceRecorded` links.
- Human-readable report.
- Unit and UI tests covering presence, absence, parser, and view-model behavior.

## Results

### Offline harness (validated)

`cargo run -p qsf_app -- experiment volition-emotion-signals` completed end-to-end (also via
`scripts/qsf.ps1 app -Experiment volition-emotion-signals`). Nine deterministic scenarios drive
each signal on and off, giving every kind at least one presence and one absence case. Artifacts
land under `runs/<timestamp>-volition-emotion-signals/`:

- `traces.jsonl` — 6 `emotion-signal-derivation` records (one per emitted signal) carrying all
  eight required fields, plus 9 `emotion-signal-absence-check` records (one per scenario) listing
  the kinds asserted absent and the signals actually present.
- `events.jsonl` — `TraceRecorded` entries linking to the derivation trace ids (the existing
  experiment pattern, not a new lifecycle-event shape).
- `emotion-signal-report.md` — per-signal presence/absence summary with a human checklist.

The in-run verifier re-derives each signal genuinely rather than comparing a value to itself: it
reconstructs `VolitionState` from the recorded `dynamic_state_snapshot`, independently replays the
recorded `events_applied` from a fixture-seeded state and cross-checks the two reconstructions
agree, then calls `derive_signals` fresh and asserts the re-derived kind, intensity, thresholds,
and evidence match the trace record. Absence records re-derive from the snapshot and assert none
of the expected-absent kinds appears. Any mismatch fails the run. A RED check (stubbing the
per-record verifier to pass) made the tamper tests fail, confirming the re-derivation is real.

### Automated tests (passing)

- `qsf_volition::signals` unit tests: each signal appears exactly when its evidence exists and is
  absent otherwise (below-threshold blocks, never-activated goals, progress-only evidence,
  cold-start boredom), with every emitted signal's evidence resolving to present state; intensity
  monotonicity, deterministic ordering, and serde round-trips covered.
- Reducer unit tests for the new lifecycle fields, including re-blocking after satisfaction resets
  the counters and back-compat deserialization of snapshots without the fields.
- Realtime capture tests: the top-level `signals` array matches `derive_signals` for an active
  state, is empty on cold start, serializes on the wire, and back-compat-deserializes when absent.
- UI parser / view-model tests (vitest, 58/58): all four kinds parse into camelCased per-kind
  evidence, a missing `signals` key defaults to an empty list, malformed entries are dropped
  without discarding the message, and each kind renders one evidence-backed row (no bare emotion
  word without its evidence).
- `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`, and `npm run check` /
  `npm run fmt` in `crates/qsf_realtime_server/ui/` are clean.

### Pending (human)

- Procedure step 6 — the live browser session where the operator provokes a coherence decline and
  a satisfied goal and confirms the panel rows match the capture — has not been run.
- The qualitative operator interpretability review (do the rows read as honest instrument
  readouts, not claimed feelings?) has not been done.

## Interpretation

```text
Observed: The offline harness derives all four signals on their evidence and none off it,
          re-derives every recorded signal from its artifacts, and passes the full automated
          suite; the realtime capture and browser panel surface evidence-backed rows.
Interpreted: Pure derivation over recorded VolitionState plus fixture data can produce
          evidence-backed functional-signal rows while keeping signals confined to offline
          traces and the operator panel, with the structural gate holding (no arbitration,
          injection, or tool consumer).
Uncertain: The interpretability question — whether the rows read as honest instrument readouts
          rather than anthropomorphic claims — is not settled offline; it needs the live browser
          check and operator review. Threshold constants are chosen for fixture coverage, not
          proven as live defaults.
```

## Follow-Up Questions

- Does live operator review prefer the label `boredom` or a less anthropomorphic display label?
- What current-conflict substrate would be needed for true D4 `tension`?
- Should any signal ever become model-visible, and if so under what wording constraints?

## Follow-Up Experiments

- True unresolved-conflict tension signal.
- Sustained salience-history boredom.
- Tool-facing signal introspection review.

## Decision Candidates

- Candidate: Keep functional signals operator-panel only unless a later D4 review explicitly
  approves model-visible exposure.

## Final Status

Open. All automated success criteria are met offline; final status is held until the live browser
check and the operator interpretability review are run.
