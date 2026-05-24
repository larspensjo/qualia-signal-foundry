# Experiment Fixtures

Tracked fixtures in this folder provide repeatable inputs or reference state for experiments and QA. Generated runtime folders such as `runs/` and `state/` stay local and gitignored, so durable examples that should survive checkout belong here instead.

## Fixtures

- [voice-memory.example.json](voice-memory.example.json) is a small file-backed memory source for repeatable text-owned voice-loop retrieval tests.
- [session-memory.empty.json](session-memory.empty.json) is an empty file-backed session memory source used by the launcher when `multi-turn-text-loop` should start without the deterministic demo fixture.
- [memory-association-browser-reference](memory-association-browser-reference/README.md) is a curated continuity bundle for QA testing memory association browsing, including session state, a continuity manifest, a consolidated sleep brief, and a self-contained memory graph.
