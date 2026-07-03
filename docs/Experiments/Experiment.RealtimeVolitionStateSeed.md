# Experiment: Realtime Volition State Seed

## Experiment ID

`Experiment.RealtimeVolitionStateSeed`

## Status

Completed

**Superseded (2026-07-03):** this experiment originally validated the retired
dev-assistant realtime seed, whose protected goals were the tier-2 `explicit-user-intent`
and tier-3 `current-task-completion` tensions. That seed no longer exists.
`realtime_seed_fixture()` now seeds the curiosity-observer persona, whose protected goals
are `person-respect` (tier 1), `epistemic-integrity` (tier 2), and
`present-person-priority` (tier 3). The current seed is validated by
[Experiment.CuriosityPersonaSeed.md](Experiment.CuriosityPersonaSeed.md). Historical
result text below records the original run and is kept as-is; the Summary and Hypothesis
have been updated to name the current protected tensions.

## Summary

Validates that each realtime session receives an isolated, fixture-backed `VolitionState`
on creation, that a trusted user transcript is mapped to `VolitionEvent`s and applied to
that state, and that the protected-tier tensions win arbitration over curiosity or
exploration goals under all modes. (Current protected tensions: `person-respect`,
`epistemic-integrity`, `present-person-priority`; see the supersession note above.)

This experiment corresponds to Phase 2 of `Plan.RealtimeVolitionIntegration.md`: adding
realtime-owned volition runtime state before any live behavioral influence is enabled.

## Motivation

- Confirms that `qsf_realtime_server` can hold in-memory volition state without depending
  on `qsf_app`.
- Proves that protected-tier goals are always present in the realtime seed and cannot be
  displaced by mode bias.
- Establishes the baseline latency budget for transcript-to-event mapping before context
  injection (Phase 4) is layered on.

## Related Documents

```text
docs/Plans/Plan.RealtimeVolitionIntegration.md
docs/Architecture/Architecture.RealtimeSessionServer.md
crates/qsf_volition/src/fixture.rs
crates/qsf_realtime_server/src/realtime/volition.rs
crates/qsf_realtime_server/src/state.rs
```

## Hypothesis

A `VolitionRuntimeState` seeded from `realtime_seed_fixture()` starts each session with
isolated in-memory state, advances its tick on each trusted transcript, activates goals
whose keywords appear in the transcript, and keeps protected-tier goals immune to mode
bias under all three modes.

## Scope

### In Scope

- `VolitionRuntimeState` creation and fixture seeding.
- `events_for_trusted_transcript` mapping helper (pure, deterministic).
- Protected-tier arbitration: tier-2 and tier-3 goals beating tier-7 curiosity under
  Neutral, Focused, and Exploratory modes.
- Session isolation: two concurrent sessions have independent volition state.
- Session cleanup: removing a session removes its in-memory volition state.
- Sideband integration: trusted transcript dispatches event mapping on StartTurn and
  Interrupt dispositions.

### Out of Scope

- Volition state persistence across sessions (Phase 6).
- Read-only realtime tools (`inspect_volition_state`, Phase 3).
- Context injection before `response.create` (Phase 4).
- Bounded initiative outputs in the live loop (Phase 5).
- UI inspection panel (Phase 7).

## Setup

- Rust workspace: `cargo test` on `qsf_volition` and `qsf_realtime_server`.
- No external services required for automated tests.
- Light manual test: `qsf.ps1 realtime` to confirm server starts normally with no visible
  behavior change.

## Procedure

Automated (already passing):

1. `cargo test -p qsf_volition` — fixture tests for `realtime_seed_fixture()` and
   protected-tier arbitration.
2. `cargo test -p qsf_realtime_server` — `VolitionRuntimeState` unit tests,
   `events_for_trusted_transcript` unit tests, session seeding and isolation tests.

Manual (light):

1. Start `qsf.ps1 realtime`.
2. Speak a few turns.
3. Confirm the server runs normally, audio and transcript loop works, and no errors appear
   in the diagnostic log related to volition.

## Baseline

Phase 1 (extract `qsf_volition`): realtime server had no volition state at all.

## Measurements

### Quantitative

- All automated tests pass: 0 failures.
- Trusted-turn mapping-only delta: measured by
  `events_for_trusted_transcript_mapping_overhead_is_under_10ms` in
  `crates/qsf_realtime_server/src/realtime/volition.rs`. The test runs 200 iterations of
  the full mapping path, subtracts a `normalize_terms`-only baseline, and asserts the delta
  is under 10 ms total (≪ 0.05 ms per turn in practice on the local path).

### Qualitative

- Server starts and handles voice turns normally.
- No diagnostic errors attributed to volition state initialization.

## Trace Completeness Contract

Not applicable for this phase. The volition state in Phase 2 is internal-only and not
exposed to the model or logged in diagnostic records. The key observability artifact is
the automated test suite rather than runtime traces.

When Phase 3 adds `inspect_volition_state`, a `volition_tool_trace` contract will be
added to `Experiment.RealtimeVolitionReadOnlyInspection`.

## Success Criteria

- All automated tests in `qsf_volition` and `qsf_realtime_server` pass.
- `cargo clippy --all-targets -- -D warnings` is clean.
- Light manual test: server runs without degradation.
- Protected-tier goals (tier-2, tier-3) win arbitration over tier-7 curiosity under
  Neutral, Focused, and Exploratory modes (asserted in test suite).
- Two concurrent sessions have independent in-memory volition state (asserted in test
  suite).

## Failure Criteria

- Any automated test fails.
- Clippy warnings introduced.
- Server fails to start or voice loop degrades after the change.

## Expected Output

- Passing `cargo test` across all crates.
- Clean clippy and fmt.
- `Experiment.RealtimeVolitionStateSeed.md` (this file) filled in as Completed.
- `Experiment.Backlog.md` updated to show this experiment as Completed.

## Results

### What Happened

Phase 2 implemented and validated. All automated tests pass.

Files changed:

- `crates/qsf_volition/src/fixture.rs`: Added `realtime_seed_fixture()` with tier-2
  (`explicit-user-intent`) and tier-3 (`current-task-completion`) tensions and goals.
  Added 11 tests for the realtime fixture and protected-tier arbitration.
- `crates/qsf_realtime_server/Cargo.toml`: Added `qsf_volition` dependency.
- `crates/qsf_realtime_server/src/realtime/volition.rs`: New file with
  `VolitionRuntimeState` struct and `events_for_trusted_transcript` mapping helper plus 9
  unit tests.
- `crates/qsf_realtime_server/src/realtime/mod.rs`: Added `volition` module.
- `crates/qsf_realtime_server/src/state.rs`: Added `volition: VolitionRuntimeState` field
  to `SessionRuntime` and 4 integration tests for seeding, isolation, and cleanup.
- `crates/qsf_realtime_server/src/realtime/sideband.rs`: Added
  `apply_trusted_transcript_to_volition` helper called on trusted transcript at
  `StartTurn` and `Interrupt` dispositions.

### Measurements

- 0 test failures across all crates.
- 0 clippy warnings.
- Mapping delta verified by `events_for_trusted_transcript_mapping_overhead_is_under_10ms`:
  200-iteration delta (mapping minus normalize-only baseline) asserted under 10 ms.

### Observations

- The protected-tier floor correctly blocks mode bias from entering tiers 1–3.
- Session isolation holds because `VolitionRuntimeState` is owned by `SessionRuntime` and
  not shared across sessions.

### Surprises

None.

### Failure Modes

None observed.

## Interpretation

Observed: The realtime server now holds per-session in-memory volition state, advances
the tick on trusted transcripts, and the protected-tier goals correctly dominate
curiosity/exploration goals under all mode variants.

Interpreted: The trust boundary is preserved (no `qsf_app` dependency introduced), the
reducer remains pure, and the sideband integration path is fast because all mapping is
synchronous and in-memory.

## Follow-Up Experiments

```text
Experiment.RealtimeVolitionReadOnlyInspection  (Phase 3)
Experiment.RealtimeVolitionContextInjection    (Phase 4)
```

## Final Status

Useful Result
