# Experiment Fixtures

Tracked fixtures in this folder provide repeatable inputs or reference state for experiments and QA. Generated runtime folders such as `runs/` and `state/` stay local and gitignored, so durable examples that should survive checkout belong here instead.

## Fixtures

- [voice-memory.example.json](voice-memory.example.json) is a small file-backed memory source for repeatable text-owned voice-loop retrieval tests.
- [session-memory.empty.json](session-memory.empty.json) is an empty file-backed session memory source used by the launcher when `multi-turn-text-loop` should start without the deterministic demo fixture.
- [memory-association-browser-reference](memory-association-browser-reference/README.md) is a curated continuity bundle for QA testing memory association browsing, including session state, a continuity manifest, a consolidated sleep brief, and a self-contained memory graph.
- [realtime-probe/](realtime-probe/README.md) is the synthetic phrase script, warm-start seed bundle, and offline trace-contract ledger for headless scripted realtime conversation runs. Generated run directories remain under the gitignored `state/` boundary.
- [volition-seed.reviewed.draft.json](volition-seed.reviewed.draft.json) is the fill-in input for the `accept-reviewed-volition-seed` experiment. An operator adds reviewed goal overrides to `accepted_goals`, updates the promotion and source-artifact fields, then runs `.\scripts\qsf.ps1 app -Experiment accept-reviewed-volition-seed -LaunchProfile realtime-state -VolitionDraft docs/Experiments/Fixtures/volition-seed.reviewed.draft.json` to promote the reviewed seed.
