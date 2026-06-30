# Design: Realtime Volition Continuity

## Decision

Realtime volition state is written for inspection, not blindly reloaded. Each new
session seeds from `realtime_seed_fixture()` plus an explicit reviewed seed artifact
(`volition-seed.reviewed.json`) if one exists. The live `VolitionRuntimeState` stays
session-local and is never restored verbatim from a prior `volition-state.json`
snapshot.

## Boundary

- `qsf_realtime_server` writes the versioned `volition-state.json` snapshot alongside
  the existing continuity artifacts on continuity-promoted trusted turns.
- `qsf_app` reads the continuity snapshot, manifest, reviewed seed, and diagnostics
  JSONL from the same state root, then runs pure consolidation over volition-native
  inputs.
- The reviewed seed is only written by an explicit human-run acceptance step modeled
  on reviewed-memory acceptance. No automatic promotion path affects future sessions.
- `Mode` resets to `Neutral` on every new session unless the reviewed seed explicitly
  says otherwise.

## State Root Layout

For a given `qsf_session_id`, the realtime state root is:

```text
<state_dir>/continuity/<qsf_session_id>/
  session-state.json
  continuity-manifest.json
  memory-store.json
  volition-state.json
  volition-seed.reviewed.json
<state_dir>/diagnostics/<qsf_session_id>.jsonl
```

## Consequences

- Continuity stays useful but not sticky.
- The sleep/consolidation pass can inspect raw continuity evidence without making
  seeding depend on prior live snapshots.
- Reviewed durable changes remain auditable and human-gated.
