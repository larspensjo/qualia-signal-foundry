# Review: Plan.FrameworkMVP

Reviewer notes on [docs/Plans/Plan.FrameworkMVP.md](../Plans/Plan.FrameworkMVP.md), with particular attention to integrating the newly added [crates/engine_logging/](../../crates/engine_logging/) crate.

Date: 2026-05-09
Status of plan reviewed: Proposed

## Overall verdict

The plan is sound. Scope is appropriate for an MVP, the phasing is incremental and testable, and the observability-first ordering (Phase 2 before behavior) matches the architectural intent in [docs/Architecture/Architecture.StateAndObservability.md](../Architecture/Architecture.StateAndObservability.md).

The most significant change required is **structural**: the plan currently assumes a single application crate (`src/main.rs`, `src/lib.rs`, internal modules), but the imported [engine_logging](../../crates/engine_logging/Cargo.toml) crate uses workspace inheritance (`edition.workspace = true`, `log.workspace = true`, `simplelog.workspace = true`). Adopting it as-is forces a Cargo workspace from day one. That is not a problem, but the plan's "Initial Crate Strategy" section needs to be revised rather than followed.

A secondary point: `engine_logging` provides the **developer/operator log facade**, not the structured event log or trace log defined in Phase 2. The plan should make this distinction explicit so the three observability layers stay clearly separated.

## Major item: workspace bootstrap and `engine_logging` integration

### What the plan currently says

[Plan.FrameworkMVP.md:260-282](../Plans/Plan.FrameworkMVP.md#L260-L282) — "Initial Crate Strategy":
> The project should initially be a Rust application crate with internal modules.
> ... the first MVP should avoid premature crate splitting unless the implementation becomes clearly easier.

[Plan.FrameworkMVP.md:194-256](../Plans/Plan.FrameworkMVP.md#L194-L256) — "Proposed Initial Repository Shape" lists `src/main.rs`, `src/lib.rs`, and module subfolders.

### Reality on disk

[crates/engine_logging/Cargo.toml](../../crates/engine_logging/Cargo.toml) declares:

```toml
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[dependencies]
log.workspace = true
simplelog.workspace = true
```

That mandates a root workspace `Cargo.toml` with `[workspace.package]` and `[workspace.dependencies]` sections. The repo currently has no root `Cargo.toml`, so today `cargo build` against `engine_logging` will fail.

### Recommended plan edits

1. **Replace "Initial Crate Strategy" with a workspace-from-day-one strategy.** Rationale: the splitting is no longer premature — it has already happened by virtue of importing `engine_logging`. Suggested initial layout:

   ```text
   Cargo.toml                       # workspace root
   crates/
     engine_logging/                # already present
     qsf_app/                       # binary + framework modules
       Cargo.toml
       src/
         main.rs
         lib.rs
         experiments/
         runtime/
         observability/
         memory/
         context/
         tools/
         models/
         sleep/
         reports/
   ```

   The internal modules from the original "Proposed Initial Repository Shape" stay; they just live inside `qsf_app/src/` instead of `src/`. Further splitting (`qsf_runtime`, `qsf_memory`, etc.) can still be deferred to when implementation pressure justifies it.

2. **Add a root `Cargo.toml` spec to Phase 1.** It needs:
   - `[workspace] members = ["crates/*"]`
   - `[workspace.package]` with `edition`, `license`, `authors`, `rust-version` (consistent with the existing [LICENSE](../../LICENSE))
   - `[workspace.dependencies]` with at minimum `log` and `simplelog` (versions to be chosen) so `engine_logging` resolves
   - Whatever shared dependencies the framework will pull in next (e.g. `serde`, `serde_json`, `tokio`, `anyhow`)

3. **Add an explicit "Logging Strategy" section to the plan** that distinguishes the three observability layers and their owners:

   | Layer | Purpose | Format | Crate / module |
   |---|---|---|---|
   | Developer/operator log | Diagnostic messages, errors, free-text traces during dev | `log`-style lines | `engine_logging` (`engine_info!`, `engine_warn!`, …) |
   | Event log | Chronological, structured events that drive the reducer | JSON Lines | `observability/event_log.rs` |
   | Trace log | Why something happened (memory/context/tool/model/sleep) | JSON Lines | `observability/trace.rs` |

   Reducers and the runtime loop write to the event log. Side effects write to the trace log. Both may *also* call `engine_*` macros for human-readable diagnostics, but those macros must never be the system of record for state transitions.

4. **Bind `engine_logging` initialization to the experiment runner.** [crates/engine_logging/src/lib.rs:94-116](../../crates/engine_logging/src/lib.rs#L94-L116) hardcodes `./engine.log` in `initialize()`. The plan wants per-run output under `runs/<timestamp>-<experiment-id>/` ([Plan.FrameworkMVP.md:872-902](../Plans/Plan.FrameworkMVP.md#L872-L902)). Use [`initialize_to_path`](../../crates/engine_logging/src/lib.rs#L144) instead, e.g. `runs/<id>/engine.log`, so each run's diagnostic log lives next to its `events.jsonl` and `traces.jsonl`.

5. **Adopt the `engine_*` macros from the start.** [crates/engine_logging/src/lib.rs:26](../../crates/engine_logging/src/lib.rs#L26) carries a TODO: "Replace all log:: with the macros below." Make this a project rule from day one — cheap to enforce on a green field, expensive later. Add to [Agents.md](../../Agents.md) as a Logging rule: "Use `engine_logging` macros, not `log::*` directly."

6. **Update Phase 1 verification.** It currently runs `cargo build`, `cargo test`, `cargo run -- --help`. Add: `cargo build -p engine_logging` and a smoke test that calls `engine_logging::initialize_for_tests()` from the placeholder experiment.

## Naming concern: "engine"

You flagged that "engine" is a misdirection inherited from a game engine. Two reasonable paths:

- **Keep the name.** Lowest-cost option. Document in a Decision entry that "engine" here means "the qsf runtime engine" and is not related to a game engine. The macros stay short and familiar.
- **Rename now.** A find-and-replace of `engine_logging` → `qsf_logging` (or `foundry_logging`) and the macros `engine_*` → `qsf_*` is mechanically trivial today and only gets harder. Worth doing only if the misdirection is likely to confuse future readers (or future-you reading old commits).

Recommendation: **keep the name for now**, capture the rename as a candidate decision in [docs/DecisionLog.md](../DecisionLog.md), and revisit once 2–3 framework modules actually use the macros and you have a feel for whether the name reads wrong in real call sites.

## Smaller observations on `engine_logging`

These are not blocking, but worth noting before the crate's surface gets locked in by callers.

1. **`set_sim_tick` / `get_sim_tick`** ([lib.rs:9-24](../../crates/engine_logging/src/lib.rs#L9-L24)) is the most obvious game-engine residue. The framework's nearest analog is "step ordinal" inside an experiment run, not a real-time simulation tick. Options: (a) drop both functions until something needs them; (b) repurpose as `set_step` / `get_step` keyed per experiment; (c) leave dormant. I'd drop them — `#![deny(missing_docs)]` plus dead API is a needless tax.
2. **`#![deny(missing_docs)]`** ([lib.rs:1](../../crates/engine_logging/src/lib.rs#L1)) is a strong policy. Fine to keep, but the plan should call it out so contributors don't fight it.
3. **`engine.log` as a default filename** is fine if you keep the "engine" name; if you rename the crate, the default filename should follow.
4. **No structured/JSON output.** `simplelog` writes plain text. That's the right choice for the developer log layer, and it's why the structured event log (Phase 2) must be a separate concern. Worth stating explicitly in the plan so no one tries to grep `engine.log` for state transitions.
5. **Initialization is process-global and idempotent** (safely no-ops if a logger is already set). That's exactly right for a binary that hosts experiments serially. If experiments ever run in-process concurrently, the per-run file routing in `initialize_to_path` becomes the wrong shape — flag this in the plan's Risks section.
6. **Privacy boundary.** [Plan.FrameworkMVP.md:1376-1380](../Plans/Plan.FrameworkMVP.md#L1376-L1380) says "API keys are not logged" — this constraint applies to `engine_*` macros too, and dev logs are easier to leak than the structured event log. Worth restating under "Logging Strategy."

## Smaller observations on the plan itself

Independent of `engine_logging`:

1. **`Experiment.FrameworkSkeletonMVP` template path.** [Plan.FrameworkMVP.md:1203](../Plans/Plan.FrameworkMVP.md#L1203) refers to `ExperimentTemplate.md`; the actual file is [Experiment.Template.md](../Experiments/Experiment.Template.md). Fix the reference.
2. **Verification checklist wording.** [Plan.FrameworkMVP.md:1248](../Plans/Plan.FrameworkMVP.md#L1248) says "Tests run." Replace with "Tests pass" so the checklist captures intent, not just invocation.
3. **`runs/` in `.gitignore`.** Plan suggests this at [Plan.FrameworkMVP.md:893-896](../Plans/Plan.FrameworkMVP.md#L893-L896). Confirm and add to Phase 1 explicitly. Also gitignore `engine.log` at the repo root in case anyone calls `initialize()` from a working directory.
4. **OpenAI provider path on disk.** [Plan.FrameworkMVP.md:335](../Plans/Plan.FrameworkMVP.md#L335) hardcodes `C:/Users/larsp/src/web_page_filet_mignon/...` in an example `[patch]` block. That's fine as an example, but worth a note that the path is the author's local layout and other contributors should adjust.
5. **Cargo.lock guidance.** [Plan.FrameworkMVP.md:325](../Plans/Plan.FrameworkMVP.md#L325) is correct: commit `Cargo.lock` for the binary. Worth restating in Phase 1 verification.
6. **State Update Model.** The plan does not yet reflect the unidirectional reducer commitment recorded in [docs/DecisionLog.md](../DecisionLog.md) and [Architecture.RuntimeLoop.md](../Architecture/Architecture.RuntimeLoop.md). Add a short subsection under "Runtime Loop MVP" pointing to that decision so the plan and the decision stay in sync.

## Suggested additions to the "Open Questions" section

- **RQ-Framework-LoggingScope.** Should each experiment's `engine.log` live under `runs/<id>/`, or should there also be a long-lived process-level log capturing the binary's lifecycle outside of any experiment?
- **RQ-Framework-LogCrateName.** Keep `engine_logging` (game-engine residue, but cheap) or rename to `qsf_logging` / `foundry_logging` before callers proliferate?
- **RQ-Framework-SimTick.** Does the framework need the per-thread tick API from `engine_logging`, or should it be removed?
- **RQ-Framework-LogLevels.** [Architecture.StateAndObservability.md:497-516](../Architecture/Architecture.StateAndObservability.md#L497-L516) defines five logging levels (Minimal, Normal, Research, Replay, Debug). These don't map cleanly onto `log`'s five levels. Is a custom level scheme worth it, or do we treat the architecture levels as documentation-only?

## Suggested additions to "Decision Candidates"

- Adopt `engine_logging` as the workspace's developer/operator log facade. Structured event/trace logs remain separate.
- The MVP uses a Cargo workspace from day one (driven by `engine_logging`'s workspace-inherited fields), with framework code in a `qsf_app` crate.
- Per-run diagnostic logs are written to `runs/<id>/engine.log` via `initialize_to_path`.

## Suggested next steps

1. Edit [Plan.FrameworkMVP.md](../Plans/Plan.FrameworkMVP.md) per the recommendations above (workspace strategy, logging strategy section, Phase 1 changes, open questions).
2. Land a workspace `Cargo.toml` and a placeholder `qsf_app` crate so `cargo build` succeeds end-to-end.
3. Once accepted, capture the workspace + logging decisions in [docs/DecisionLog.md](../DecisionLog.md).
