# Experiment: Realtime Volition Continuity

## Experiment ID

`Experiment.RealtimeVolitionContinuity`

## Status

Completed. Phase 6 code is implemented and automated verification passed on 2026-06-30.
Human live-voice verification is still pending, so this experiment is not yet complete.

## Summary

Validate that realtime volition state survives across sessions in a useful but not
sticky way: the server writes a versioned continuity snapshot, the sleep pass can inspect
that snapshot plus diagnostics, and the next session only picks up a reviewed seed that
was explicitly accepted by a human.

This is the live-validation companion to the "realtime volition continuity" behavior in
`docs/Plans/Plan.RealtimeVolitionIntegration.md`.

## Decisions Resolved Before Implementation

### D1 - Reviewed seed is explicit and human-gated

The durable cross-session seed is a dedicated `volition-seed.reviewed.json` artifact in
the continuity root. It is written only by an explicit reviewed-acceptance step, not by
automatic sleep promotion.

### D2 - Live snapshots are written for inspection, not replay

The server writes `volition-state.json` in lockstep with continuity promotion, but
`create_session` does not restore the prior raw snapshot verbatim. New sessions reseed
from the fixture plus the reviewed durable seed only.

### D3 - Sleep reads a neutral projection

The sleep/consolidation pass reads the continuity snapshot, manifest, reviewed seed, and
diagnostics JSONL from the same state root, then projects the diagnostics into
volition-native consolidation inputs.

### D4 - Suppression is first-class

Proposed initiatives that do not surface are still recorded in diagnostics and are
distinguished by structured suppression reason fields rather than by missing records.

## Related Documents

```text
docs/Plans/Plan.RealtimeVolitionIntegration.md
docs/Plans/Design.RealtimeVolitionContinuity.md
docs/Architecture/Architecture.RealtimeSessionServer.md
docs/Architecture/Architecture.StateAndObservability.md
docs/Architecture/Architecture.VolitionSystem.md
docs/DecisionLog.md
crates/qsf_realtime_server/src/realtime/sideband.rs
crates/qsf_realtime_server/src/realtime/volition_continuity.rs
crates/qsf_volition/src/continuity.rs
crates/qsf_volition/src/consolidation.rs
```

## Hypothesis

A live realtime session can persist continuity artifacts that help the next session and
the sleep pass without making volition state sticky or bypassing human review.

## Scope

### In Scope

- Versioned `VolitionContinuitySnapshot` writes.
- Reviewed seed acceptance and session seeding from the reviewed artifact only.
- Consolidation over snapshots plus initiative outcomes from diagnostics JSONL.
- Artifact-grounded report output for recurring, blocked, candidate-transition, mode
  change, and unacted-initiative patterns.

### Out of Scope

- UI surfacing of continuity state.
- Automatic promotion of durable volition changes.
- Any write-capable external effect beyond the continuity/reviewed-seed artifacts.

## Trace Completeness Contract

`volition_continuity_snapshot` must contain:

- `schema_version`
- `qsf_session_id`
- `recorded_at`
- `seed_fixture_id`
- `state`
- `inspection`

Each per-turn initiative outcome used by consolidation must contain:

- `qsf_session_id`
- `exchange_index`
- `recorded_at`
- `response_create_event_ref`
- `winning_goal_id`
- `initiative_output`
- `surfaced`
- `suppression_reason`
- `rendered_line_present`
- `artifact_reference`

The consolidation report items must each contain:

- a pattern kind
- a goal or candidate id
- a count or tick range
- an artifact reference
- a promotion status when the item represents a proposed durable change

## Procedure

### Automated Verification

1. Assert a continuity snapshot round-trips through persistence and reload.
2. Assert the snapshot serializer is byte-stable for the same value.
3. Assert a reviewed seed can be accepted and then loaded at session start.
4. Assert missing or corrupt reviewed seeds fall back to the fixture and emit a
   diagnostics note.
5. Assert the consolidation report resolves items to real snapshot and diagnostics
   artifacts.

### Human Test Steps

1. Start two realtime sessions with the same default session id.
2. Accept a reviewed volition seed between them.
3. Confirm the second session starts at `Neutral`, benefits from the reviewed seed, and
   does not carry stale live drift from the first session.
4. Confirm the consolidation report cites artifact references instead of free-form
   claims.

## Results

Automated verification passed. Human live-voice verification is pending.
