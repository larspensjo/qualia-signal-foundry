# Repo Instructions

## Workflow
- Build with `cargo build`.
- When a task is complete, run `cargo clippy --all-targets -- -D warnings` and then `cargo fmt`.
- For changes under a crate's `ui/` directory, run `npm run check` and then `npm run fmt` from that directory. `npm run check` covers both `tsc --noEmit` and Biome lint.
- When launching npm through `Start-Process`, use `npm.cmd` explicitly instead of bare `npm`; PowerShell may resolve bare `npm` to `npm.ps1`, which can open in Notepad depending on file association.
- When implementing changes, update the relevant project documents when the change affects current behavior, workflow, architecture, experiments, or durable decisions.
- When creating complex plans, they should be divided into incremental phases that can be tested.

## Planning & Documentation
- Distinguish the two planning tracks (see `docs/ProjectFrame/DocumentStatus.md` and `docs/ProjectFrame/ProjectWorkflow.md`): a multi-phase effort gets a phased `docs/Plans/Plan.*.md`; a single self-contained, testable slice gets a `docs/Experiments/Experiment.*.md`. A `Plan.*.md` sequences phases, each validated by an `Experiment.*.md` scaffold.
- When creating a plan, make it clear how to verify each phase, and point out where external human testing is recommended. Save plans to the `docs/Plans/` folder unless explicitly told otherwise. Check with `docs/DecisionLog.md`.
- When implementing a plan, surface its open questions or ambiguities before silently resolving them.
- When adding a feature behind a config flag or threshold, the default values must exercise the new code path.
- When creating a plan, check `docs/ProjectFrame/ProjectWorkflow.md` for what documents should be updated, and include that in the plan.
- When a plan or experiment relies on traces to explain a behavioral chain, include a trace completeness contract per `docs/ProjectFrame/ProjectWorkflow.md`: required trace fields, artifact boundary, and artifact-parsing verification.
- Plans and reviews are ephemeral documents, deleted after external review. Never refer to plan phase numbers from durable repository documents (experiment specs, architecture, the decision log, or code); name the behavior instead.
- Prefer plans with proper long term solutions, even if more work or refactoring are required.

## Architecture
- Preserve the unidirectional data flow: input -> action -> reducer -> state -> render, with side effects isolated and fed back as actions.
- Reducers must stay pure and unit-testable.
- Keep view-derivation logic in pure selectors/view-models, not inline in components; components consume state and render. This is the `state -> render` analogue of pure reducers.
- Keep entry points (`main.rs`, `mod.rs` and `lib.rs`) files as thin wrappers only.
- Keep shared constants and behavior DRY; prefer one source of truth over duplicated definitions.
- Name runtime modules after stable behavior or domain concepts, not temporary plan phases or milestones.

## Testing
- Bug fixes should include a regression test when practical.
- Prefer tests of reducer behavior, emitted effects, and public contracts over internal details.
- `use super::*;` is acceptable for tests, but using explicit imports is preferable otherwise.
- UI: prefer tests of reducers, selectors/view-models, pure helpers, and the effect layer (`api/client.ts`) over component-render details. Test components by role/text for user-visible behavior only when it lives nowhere else; avoid snapshots and assertions on internal state or DOM structure.

## Logging
- Use `engine_logging` for runtime logging.
- Include enough context in error logs to identify the failing job, URL, or operation.

## Decisions
- Keep `docs/DecisionLog.md` up to date for noteworthy decisions. Typically, this can be the reason a plan was created.
- See the How to use section in the beginning.
