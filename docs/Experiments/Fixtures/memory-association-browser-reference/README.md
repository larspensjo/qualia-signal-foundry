# Memory Association Browser Reference

This fixture is a tracked reference continuity bundle for QA testing the memory association browser. It intentionally lives under `docs/Experiments/Fixtures/` because the runtime defaults, `state/` and `runs/`, are local generated-output folders and are ignored by Git.

## Contents

- `text-loop/session-state.json` captures a mock multi-turn text-loop session after resuming from a consolidated sleep brief.
- `text-loop/continuity-manifest.json` captures the manifest after a follow-up awake session, so `sleep_pending` is true and the next resume mode is `awake_continuation`.
- `text-loop/consolidated-brief.json` and `text-loop/archive/` preserve the sleep output that bootstrapped the follow-up session.
- `text-loop/memory-store.json` is curated from generated state: it includes the phase-four seed memory records, the generated sleep-promoted memory, live co-retrieval associations, varied edge weights, mixed memory kinds, reinforcement counts, and some unreinforced records.

## QA Focus

This graph is intended to exercise node labels, memory-kind filtering, tag display, edge rendering, edge-weight sorting or thresholds, reinforcement metadata, and browser handling of a continuity bundle that includes both generated sleep state and curated reference graph coverage.
