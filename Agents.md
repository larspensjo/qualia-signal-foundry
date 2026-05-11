# Repo Instructions

## Workflow
- Build with `cargo build`.
- When a task is complete, run `cargo clippy --all-targets -- -D warnings` and then `cargo fmt`.
- For plan-driven work, entries in the EngineeringDiary.md should be about the code change, not the plan. The git commit message, on the other hand, can reference the plan.
- When creating complex plans, they should be divided into incremental phases that can be tested.

## Planning & Documentation
- When creating or saving plan documents, always save them to the `docs/plans/` folder unless explicitly told otherwise.

## Architecture
- Preserve the unidirectional data flow: input -> action -> reducer -> state -> render, with side effects isolated and fed back as actions.
- Reducers must stay pure and unit-testable.
- Keep entry points (`main.rs`, `mod.rs` and `lib.rs`) files as thin wrappers only.
- Keep shared constants and behavior DRY; prefer one source of truth over duplicated definitions.
- Name runtime modules after stable behavior or domain concepts, not temporary plan phases or milestones.

## Testing
- Bug fixes should include a regression test when practical.
- Prefer tests of reducer behavior, emitted effects, and public contracts over internal details.
- `use super::*;` is acceptable inside an inline `#[cfg(test)]` block, but extracted test files (e.g. `tests.rs`) must use explicit imports.

## Logging
- Use `engine_logging` for runtime logging.
- Include enough context in error logs to identify the failing job, URL, or operation.

## Diary
- Keep `docs/DecisionLog.md` up to date for noteworthy implementations, important decisions, and bug fixes with reusable lessons.
- Keep diary entries short and reference concrete artifacts.
- Add new entries to the end.
