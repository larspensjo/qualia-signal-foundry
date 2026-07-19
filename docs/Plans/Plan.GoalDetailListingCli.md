# Plan: `qsf_app goals` — goal detail listing CLI

Status: Proposed — not started
Maturity: Candidate
Area: Developer tooling / Volition (introspection) / CLI

## Why this plan exists

Today the only ways to inspect volition goals — `build_state_inspection`
(`crates/qsf_volition/src/inspection.rs`) and the sleep-time consolidation report
(`build_volition_consolidation_report` in `crates/qsf_volition/src/consolidation.rs`) — expose
thin summaries: id, title, salience, cooldown, visibility, and pattern counts. Neither shows a
goal's full definition (activation keywords and their weight classes, tension refs, allowed
effects, satisfaction condition, scope, base priority, evidence refs, source reference,
visibility) merged with its live runtime state. When diagnosing why a persisted realtime persona
behaves the way it does, there is no single view of "here is every goal in force, defined in full,
with its dynamic state and the tier math that governs its arbitration."

This plan adds a developer diagnostic CLI subcommand, `qsf_app goals`, that reads a persisted
realtime continuity snapshot and prints every goal — fixture-defined and live-formed — in full
detail. It is engineering tooling, not a consciousness-simulation experiment: verification is unit
tests on a pure report builder plus one manual CLI run against real state. No `Experiment.*.md`
scaffold is warranted (`ProjectWorkflow.md`, Plans vs Experiments; the outcome is not in doubt and
no simulation mechanism is under question).

**Done** = running `qsf_app goals` against a persisted realtime continuity snapshot prints every
goal with full definition plus dynamic state, provenance-labeled, with raw and mode-biased tiers,
covered by unit tests on a pure builder.

### Naming and ephemerality

This document owns the ephemeral phase labels below. Durable artifacts it produces — the report
builder, the CLI subcommand, the shared tier helper, the architecture-doc update — name the
behavior ("goal detail report", "effective-tension resolution", "mode-biased tier"), never a plan
phase number (`Agents.md`; `ProjectWorkflow.md`). The blindspot-finding numbers from the brief are
cross-references into that review, not runtime names.

## Decisions carried in from the brief (with reasons)

These were settled in the brief's brainstorm and blindspot review. They are recorded here so the
implementation does not silently re-open them.

1. **Surface: a developer CLI subcommand** in `qsf_app` (a sibling of `sleep`, `list-experiments`),
   not a model-facing tool and not UI. No model-visible behavior changes.
2. **Data source: snapshot only.** The tool shows real persisted runtime state. If no snapshot
   exists, it errors clearly. There is no fixture-only fallback mode.
3. **Session resolution reuses `qsf_session::resolve_continuity_session_dir`**
   (`crates/qsf_session/src/continuity.rs`) with default `--state-dir state/realtime`. In practice
   the realtime identity is the rolling `default` session; the resolver picks `default` when
   present, resolves a single non-`default` session, and errors on ambiguity. An optional
   `--session <id>` override targets `-RandomSessionId` runs. The resolved session id prints in the
   output header. Snapshot path comes from the manifest exactly as the sleep path resolves it
   (`snapshot_path_from_manifest` in `crates/qsf_app/src/experiments/volition_continuity.rs`).
4. **Output: human console only**, through the existing `crate::console::styling` helpers,
   respecting the global `--no-color` flag. No `--json` flag in v1.
5. **Full detail always.** Every goal prints its full definition plus dynamic state (status,
   salience, cooldown_until_tick, last_activated_tick, admitted_tick). Goal counts are small; no
   compact mode, filters, or paging in v1. Subconscious goals are shown unfiltered (developer
   tool). Retired goals print in full (live-formed retired goals keep their definitions in
   `state.accepted_candidates`).
6. **Architecture: pure builder + thin CLI.** A pure, unit-testable builder lives in `qsf_volition`
   as a sibling of `build_state_inspection`; the CLI subcommand only loads, resolves, calls, and
   renders. Rendering lives in a `qsf_app` console module. This honors the repo's pure
   selector/view-model rule and thin-entry-point rule.
7. **Both goal populations covered.** Fixture-defined goals and live-formed accepted candidates
   (whose definitions live in `state.accepted_candidates`).
8. **Definition provenance is labeled per goal** (blindspot 2). The snapshot persists full
   definitions only for live-formed goals; fixture-goal definitions are *reconstructed from current
   code* by rebuilding the seed fixture and can drift from what was live when the snapshot was
   recorded. Output labels each goal: "definition reconstructed from current code (fixture <id>)"
   vs "definition recorded in snapshot". The snapshot schema is **not** changed to persist fixture
   definitions (rejected: runtime-persistence blast radius for a read-only diagnostic).
9. **Unknown fixture id is a hard error** (blindspot 3). The seed-fixture resolver returns `None`
   for anything but the realtime seed fixture, and existing callers no-op silently. The CLI must
   instead fail with an explicit error naming the unknown id and the known fixture ids.
   Non-realtime personas are out of scope for v1 (rejected: graceful degradation — dead code today,
   revisit when a second persona exists).
10. **One source of truth for effective-tier math** (blindspot 4). The effective-tier-from-tensions
    logic exists three times: the authoritative `arbitration::effective_tension_for_goal` (returns
    tier + tension id/title), `continuity::goal_effective_tier`, and the public
    `reducer::effective_tier_from_tension_ids`. This plan promotes the authoritative one to a shared
    public helper and refactors the other two to delegate (Agents.md: one source of truth, prefer
    long-term solutions). The report builder uses that helper. A goal with effective tier `u8::MAX`
    (no parent tensions in the fixture) is flagged explicitly as a missing-tension-assignment
    misconfiguration signal.
11. **Tension display inline per goal**: tension title, arbitration tier, and the goal's effective
    tier.
12. **Mode bias shown per goal** (blindspot 6). Raw effective tier is not what governs arbitration
    when mode bias applies. The header shows the snapshot's `state.mode`, and each goal shows BOTH
    the raw effective tier and the mode-biased tier (protected floor immune), e.g. "tier 6
    (mode-biased 4 under exploratory)"; the biased annotation is omitted when it equals the raw
    tier.
13. **No divergent tier math across reporting surfaces** (blindspot 5, verified). The sleep
    pipeline's `build_volition_consolidation_report` + `render_markdown_report` render a *pattern*
    report (recurring-selected, often-blocked, candidate transitions, mode changes, unacted
    initiatives) over the same snapshot — a sleep-time artifact written to the state dir, and it
    carries **no full definitions and no tier annotations**. A distinct on-demand full-definition
    CLI view is therefore warranted. The two surfaces share the promoted tier helper so their tier
    math cannot diverge; this plan adds no second tier computation.

## Artifact boundary (no trace-completeness contract required)

`ProjectWorkflow.md` requires a trace-completeness contract only when a plan relies on traces to
explain a behavioral chain. This tool does not: it reads one existing persisted artifact and
renders it. Recorded explicitly so the boundary is unambiguous:

- **Input artifact**: the persisted `VolitionContinuitySnapshot` at
  `<state_dir>/continuity/<session_id>/<manifest-named file>` (default `volition-state.json`), plus
  the seed fixture reconstructed in-process from the snapshot's `seed_fixture_id`.
- **Output**: human-readable console text only. Ephemeral; the tool writes no artifact, mutates no
  state, and records no events or traces.
- **Verification** is unit tests on the pure builder (structured fields) and pure render helpers,
  plus a manual run. There is no generated artifact to parse.

---

## Phase: Single-source effective-tier and mode-biased-tier helpers

The smallest independent slice, and a prerequisite for the builder: consolidate the duplicated
tier math into one public source of truth before any new caller consumes it. Pure refactor,
behavior-preserving.

**Work**

- In `crates/qsf_volition/src/arbitration.rs`, promote `effective_tension_for_goal` to a public
  helper (naming it for the behavior, e.g. `effective_tension_for_goal`) returning
  `(effective_tier, tension_id, tension_title)`, unchanged in logic. Keep the `u8::MAX` /
  empty-string contract for a goal with no parent tensions in the fixture.
- Add a public mode-biased-tier helper in `arbitration.rs` that, given a goal (or its resolved
  effective tension) and a `Mode`, returns the existing `BiasOutcome` (raw `effective_tier`,
  `bias_applied`, `biased_tier`, `protected`). Route `sort_qualified`'s per-goal bias computation
  through this same helper so the arbitration sort and the report builder share one implementation
  (blindspot 5: no divergent tier math). `BiasOutcome`, `Mode::tension_delta`, `compute_bias_outcome`,
  and `PROTECTED_TIER_FLOOR` already exist; this exposes them behind one entry point rather than
  adding a parallel path.
- Refactor `continuity::goal_effective_tier` (`crates/qsf_volition/src/continuity.rs`) to delegate
  to the promoted helper (take `.0`), removing its private duplicate.
- Refactor the public `reducer::effective_tier_from_tension_ids`
  (`crates/qsf_volition/src/reducer.rs`) to delegate to the promoted helper, preserving its current
  public signature and `u8` return (its experiment callers keep compiling unchanged). If the
  promoted helper keys off a `&Goal`, add a thin tension-ids entry so this wrapper has a single
  implementation to call; keep exactly one body of tier-minimum logic in the crate.
- Re-export any newly public helpers through `crates/qsf_volition/src/lib.rs` (the facade already
  does `pub use arbitration::*`).

**Verification (automated)**

- `cargo build`.
- `cargo test -p qsf_volition` green. The existing arbitration tests (tier selection,
  lexicographic tension tie-break, floor immunity, `u8::MAX` no-tension goal, mode deltas) and the
  continuity reviewed-seed tier-rejection tests already pin the behavior; they must pass unchanged,
  proving the refactor is behavior-preserving. Add a focused unit test asserting the promoted
  helper and the mode-biased helper return the same tier/`BiasOutcome` the arbitration sort uses
  for a known goal+mode (one function, not two).
- `cargo clippy --all-targets -- -D warnings` then `cargo fmt`.

**Human testing**: not required this phase.

---

## Phase: Pure goal-detail report builder in `qsf_volition`

Build the pure view-model that merges each goal's definition with its dynamic state, resolves
tensions and tiers through the promoted helper, and labels provenance — with all tests at this
level, before any rendering or CLI glue.

**Work**

- New module in `crates/qsf_volition/src/` named for the behavior (e.g. `goal_detail.rs`),
  re-exported via `lib.rs`, exposing `build_goal_detail_report(state: &VolitionState, fixture:
  &VolitionFixture) -> GoalDetailReport` (sibling of `build_state_inspection`).
- `GoalDetailReport` carries the snapshot `mode` and an ordered `Vec<GoalDetail>`. Each
  `GoalDetail` is a serde-deriving struct (matching the crate's inspection-type conventions) with:
  - **Definition**: id, title, summary, scope, base_priority, tension refs, activation keywords
    (term + weight class), allowed effects, satisfaction condition summary, evidence refs,
    source_reference, visibility.
  - **Dynamic state**: status, salience, cooldown_until_tick, last_activated_tick, admitted_tick.
  - **Tension display**: for each parent tension, its title and arbitration tier; plus the goal's
    resolved effective tension (id + title) and raw effective tier from the promoted helper.
  - **Mode-biased tier**: the `BiasOutcome` (or its fields) for `state.mode`, so the renderer can
    show "tier N" or "tier N (mode-biased M under <mode>)" and omit the annotation when biased ==
    raw.
  - **Provenance**: an enum-labeled field — `ReconstructedFromFixture` (definition rebuilt from
    current code for fixture id X), `RecordedInSnapshot` (live-formed, from
    `state.accepted_candidates`), or `DefinitionUnavailable` (goal present in `state.goals` but
    absent from both fixture and accepted_candidates — a drift diagnostic).
  - **Missing-tension flag**: true when the effective tier is `u8::MAX` (no parent tensions), so the
    renderer can flag the misconfiguration signal.
- Iterate `state.goals` as the authoritative dynamic set (as `build_state_inspection` does),
  resolving each definition from `fixture.goals` first, then `state.accepted_candidates`, setting
  provenance accordingly. Additionally surface any `accepted_candidates` entry with no `state.goals`
  dynamic record as a `DefinitionUnavailable`-adjacent diagnostic rather than dropping it silently.
  Order goals deterministically (e.g. by status group then id) so output and tests are stable.
- The builder is the single place provenance is decided: it knows fixture goals came from the
  reconstructed fixture and accepted-candidate goals came from the snapshot. It stays pure — it
  takes the already-reconstructed fixture and the snapshot state and computes; it performs no I/O
  and does not itself decide the fixture is unknown (that is the CLI's job, per the next phase).

**Verification (automated)**

- `cargo test -p qsf_volition` with new unit tests covering:
  - A fixture goal renders full definition and is labeled `ReconstructedFromFixture`.
  - A live-formed accepted candidate (added + accepted via the reducer, as the inspection tests do)
    renders full definition and is labeled `RecordedInSnapshot`.
  - A retired live-formed goal still renders its full definition.
  - A subconscious seed goal (`assemble-world-picture` in the realtime seed fixture) appears
    unfiltered with `Subconscious` visibility.
  - Effective-tier and tension-title resolution match the promoted helper for a known goal.
  - A goal with no parent tensions sets the missing-tension flag (`u8::MAX`).
  - The mode-biased tier differs from raw under a biasing mode for a band goal and equals raw for a
    protected-floor goal; the report's `mode` reflects `state.mode`.
- `cargo clippy --all-targets -- -D warnings` then `cargo fmt`.

**Human testing**: not required this phase.

---

## Phase: `goals` CLI subcommand, rendering, and documentation

Wire the builder to real persisted state through a thin subcommand, render it to the console, make
the seed-fixture resolver a shared single source of truth, and update the durable docs.

**Work**

- **Shared seed-fixture resolver.** Promote the private `fixture_for_seed_id` from
  `volition_continuity.rs` into `qsf_volition` as a public `fixture_for_seed_id(&str) ->
  Option<VolitionFixture>` (its natural home — `REALTIME_SEED_FIXTURE_ID` and
  `realtime_seed_fixture` already live there), and refactor the existing sleep-experiment caller to
  use it (one source of truth). The CLI maps `None` to an explicit error naming the unknown id and
  the known ids (blindspot 3); the sleep path keeps its existing silent-`None` behavior, which is
  correct for that path.
- **CLI loading glue** in `crates/qsf_app/src/` (a thin loading helper, not inline in `cli.rs`):
  resolve the session dir via `qsf_session::resolve_continuity_session_dir` (with the `--session`
  override taking precedence by resolving `<state_dir>/continuity/<session>` directly), resolve the
  snapshot path from the manifest exactly as the sleep path does, load via
  `VolitionContinuitySnapshot::load_or_upgrade`, and error clearly when no snapshot exists (name the
  resolved path). Reconstruct the fixture via the shared resolver; error on unknown fixture id.
  Then call `build_goal_detail_report(&snapshot.state, &fixture)`.
- **`goals` subcommand** in `crates/qsf_app/src/cli.rs` (kept a thin dispatcher): flags
  `--state-dir` (default `state/realtime`, matching `sleep`) and `--session <id>` (optional
  override). The global `--no-color` already exists. The handler loads, resolves, calls the builder,
  and prints the rendered report. Header prints: resolved session id, seed fixture id, snapshot
  `recorded_at`, and `state.mode`. Because default `--state-dir state/realtime` drives the real code
  path with no extra flags, the new path is exercised by default (Agents.md: defaults exercise the
  new path).
- **Rendering module** under `crates/qsf_app/src/console/` (e.g. `goal_detail_view.rs`, added to
  `console/mod.rs`): a pure `render_goal_detail_report(report, header fields, ColorMode) -> String`
  using `crate::console::styling` (`ColorMode::for_stdout`, `paint`, existing `Style`s). It renders
  each goal's definition, dynamic state, inline tensions with tiers, the raw/mode-biased tier line
  (omitting the biased annotation when equal), the provenance label, and the missing-tension flag
  when set. Rendering is a pure string producer so it is unit-testable without a TTY.

**Verification (automated)**

- `cargo build`.
- CLI parse tests in `cli.rs` (mirroring the existing `sleep`/`ingest-world` tests): `goals`
  defaults `--state-dir` to `state/realtime`; `--session` parses; `--no-color` accepted before and
  after the subcommand.
- Loading-glue tests using a temp state dir seeded with a snapshot (reuse the
  `write_snapshot_fixture` pattern from `volition_continuity.rs` tests): a present snapshot resolves
  and builds a report; an absent snapshot errors with a message naming the path; an unknown
  `seed_fixture_id` errors naming the unknown id and known ids.
- Render-helper unit tests (pure string): color-disabled output contains a known goal's title,
  tension title, tier line, and provenance label; the mode-biased annotation is present under a
  biasing mode and absent when biased == raw; color-enabled output emits escape codes and resets
  (as `styling` tests do). Prefer these role/text assertions over snapshotting the whole layout.
- `cargo test -p qsf_app` green.
- `cargo clippy --all-targets -- -D warnings` then `cargo fmt`.

**Human testing (recommended, required for real confidence)**: run `qsf_app goals` (via `qsf.ps1`
if it grows a wrapper, or `cargo run -p qsf_app -- goals`) against the operator's real
`state/realtime` continuity state, and against a `-RandomSessionId` run using `--session <id>`.
Evidence to collect: every expected fixture goal and any live-formed goals print with full
definitions; provenance labels read correctly (fixture goals "reconstructed from current code",
live-formed "recorded in snapshot"); tension tiers and any mode-biased annotations match the
snapshot's mode; no goal is missing; `--no-color` produces clean plain text. Confirm the
no-snapshot and unknown-fixture error messages are legible when pointed at an empty or foreign state
dir.

---

## Documents to create or update (`ProjectWorkflow.md`)

- **Update** `docs/Architecture/Architecture.VolitionSystem.md` — its inspection/introspection
  section and Implementation Status band: record that goal introspection now has three surfaces
  over a continuity snapshot (the thin `build_state_inspection`, the sleep-time pattern
  consolidation report, and the on-demand full-definition `qsf_app goals` CLI), and that
  effective-tier and mode-biased-tier math is a single shared helper in `arbitration`. Refresh the
  `Last reviewed:` date. Name the behavior, not this plan's phases.
- **Decision log** (`docs/DecisionLog.md`): per its own How-to-use section, a read-only diagnostic
  CLI is an implementation detail carried by code, tests, and the commit — not on its own a durable
  commitment. The one candidate worth considering is the durable rule that **effective-tier and
  mode-biased-tier math has a single public source of truth in `qsf_volition::arbitration`, and all
  goal-reporting surfaces consume it** (a naming/structural convention derived from blindspots 4 and
  5). Record that entry only if the team wants it treated as settled; otherwise leave it to the
  commit. Do not log the CLI feature itself.
- **Handoff** (`docs/Handoff.md`): update only if landing a phase changes a Now/Next/Horizon
  recommendation (pointer, not content).
- **Do not** cite this plan's phase labels from any durable document; name the behavior.

## Exit criteria (whole plan)

- Effective-tier and mode-biased-tier math is a single public helper in `qsf_volition::arbitration`;
  `continuity::goal_effective_tier` and `reducer::effective_tier_from_tension_ids` delegate to it.
- A pure `build_goal_detail_report` exists in `qsf_volition`, unit-tested, merging full definitions
  with dynamic state for both fixture and live-formed goals, with provenance labels, raw and
  mode-biased tiers, and a missing-tension flag.
- `qsf_app goals` loads a persisted realtime snapshot (default `state/realtime`, `--session`
  override), reconstructs the fixture, renders every goal in full to the console honoring
  `--no-color`, errors clearly on a missing snapshot, and errors on an unknown seed fixture id.
- The seed-fixture resolver is a single shared function used by both the sleep path and the CLI.
- `Architecture.VolitionSystem.md` reflects the new introspection surface and the shared tier
  helper.
- A manual run against real state confirms the output.

## Open Questions (surfaced, not silently resolved)

1. **`--session` override semantics vs the resolver.** The brief specifies reusing
   `resolve_continuity_session_dir` (which auto-picks `default`/single/ambiguous) *and* adding a
   `--session <id>` override for random-session runs. The plan resolves this by having `--session`
   bypass auto-resolution and address `<state_dir>/continuity/<session>` directly, erroring if that
   session's manifest/snapshot is absent. Flagged in case the operator would prefer `--session` to
   instead disambiguate *within* the resolver's candidate set (identical for the common cases;
   differs only when a named session lacks a manifest).
2. **Ordering of goals in the output.** The brief does not specify an order. The plan chooses a
   deterministic order (status group, then id) for stable output and tests. If the operator prefers
   a different grouping (e.g. by effective tier, or definition source), it is a one-line change in
   the builder; surfaced rather than assumed.
3. **`qsf.ps1` wrapper.** The brief describes `qsf_app goals` as analogous to `sleep`/
   `list-experiments`; it does not state whether `qsf.ps1` should grow a `goals` shortcut. The plan
   ships the `qsf_app` subcommand only; a launcher shortcut is out of scope unless the operator
   wants one (trivial follow-up).
