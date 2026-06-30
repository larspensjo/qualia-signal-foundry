# Plan: Realtime Volition Integration

## Status

In progress. Phases 1-5 are complete. **Phase 6 is the next implementation slice**:
persist, inspect, and consolidate realtime volition state. The building blocks already
exist — the realtime server writes per-session continuity artifacts (`session-state.json`,
`continuity-manifest.json`, `memory-store.json`) on every trusted turn and reloads the
memory store across sessions, `VolitionState` and the compact `VolitionStateInspection`
already serialize, every volition turn already records trace-backed diagnostics
(`VolitionContextInjected`, `RealtimeBoundedInitiative`) carrying snapshots, and a
sleep/consolidation pass with reviewed-promotion gating already lives in `qsf_app`. So
Phase 6 is primarily a persistence-boundary decision plus a write / pure-extraction /
reviewed-reseed slice, not new domain modeling. Read the compacted "Completed Phases 1-5"
summary below for the constraints Phase 6 must respect.

This plan connects the completed offline volition slices to
the first-class realtime voice surface (`scripts/qsf.ps1 realtime`) without weakening
the realtime server's trust boundary or turning internal initiative into external
agency.

Companion documents:

- [`Plan.VolitionGoalSystem.md`](Plan.VolitionGoalSystem.md) - completed offline
  volition slices: fixture selection, salience, arbitration, candidate goals, bounded
  initiative execution, and mode bias.
- [`Plan.RealtimeVoiceConversation.md`](Plan.RealtimeVoiceConversation.md) - realtime
  voice server, sideband, memory injection, and read-only tool loop.
- [`Idea.VolitionGoalSystem.md`](Idea.VolitionGoalSystem.md) - rationale and
  terminology for tensions, goals, salience, arbitration, and initiative.
- [`Design.RealtimeVoiceConversation.md`](Design.RealtimeVoiceConversation.md) -
  realtime trust boundary and three-plane architecture.
- [`Design.VolitionBriefReconciliation.md`](Design.VolitionBriefReconciliation.md) -
  reconciliation of the volition brief with this plan, including the opportunity-detection
  step and the conversational-intensity dial absorbed by the now-complete context-injection
  slice.

This is a multi-phase plan because the work crosses crate boundaries, realtime tools,
sideband context injection, durable/session state, UI inspection, and human live-voice
evaluation. Each phase should be validated by a focused experiment scaffold under
`docs/Experiments/`.

## Goal

Make the volition system accessible from `qsf.ps1 realtime`, so the live voice
conversation can inspect and eventually use QSF-owned volition state while preserving:

- realtime responsiveness,
- explicit event/reducer/state flow,
- read-only tool safety by default,
- inspectable traces for why a goal or initiative influenced behavior,
- the `qsf_realtime_server` no-`qsf_app` dependency boundary,
- the distinction between simulated internal initiative and uncontrolled external
  agency.

The intended end state is not a new chat prompt. It is a realtime-accessible layer of
the consciousness simulation: active tensions, selected goals, mode bias, bounded
initiative outputs, and their causal traces are available in the live spoken loop.

## Starting Point

This section describes the baseline at plan creation; the Status section above records
what is now complete. Implemented at plan creation:

- `crates/qsf_app/src/volition.rs` contained the original pure volition model.
- Volition was exercised through `qsf_app` registered experiments such as
  `volition-mode-bias` and `volition-bounded-initiative-execution`.
- `scripts/qsf.ps1 realtime` starts `qsf_realtime_server` and the realtime browser UI.
- `qsf_realtime_server` originally exposed only three read-only live tools:
  `search_memory`, `get_associations`, and `inspect_session_state`.
- `qsf_realtime_server` intentionally does not depend on `qsf_app`.

As of Phase 5, the pure domain lives in `qsf_volition`, each session carries
`VolitionRuntimeState`, two read-only volition tools are registered, volition influences
the live spoken response through layered, trace-backed context injection, and the
arbitration winner produces bounded internal initiative outputs (reflection, open-thread
surfacing, experiment proposals, context-retrieval hints) surfaced through the per-turn
volition channel. The remaining gap Phase 6 closes is that realtime volition state is
in-memory only: it is written to continuity/diagnostics artifacts, but nothing decides
what survives across sessions or consolidates it for review.

## Architecture Direction

Use the established lean-crate extraction pattern:

```text
qsf_memory
qsf_context
qsf_session
qsf_tools
qsf_volition        <-- pure domain crate (exists)
qsf_realtime_server <-- may depend on qsf_volition, not qsf_app
qsf_app             <-- keeps experiments and can re-export/adapt qsf_volition
```

`qsf_volition` contains pure domain state, reducers, context-neutral selectors,
arbitration, fixtures, trace structs, and bounded initiative output. It must not depend
on `qsf_app`. The context-attached `GoalSelection` / `GoalSelectionResult` shapes carry
context assembly data, so those result types stay in `qsf_app` or become thin adapters
in the caller crates. `qsf_realtime_server` owns live state and side effects, but any
volition state changes still happen through pure volition events.

The first realtime integration was read-only and inspectable. Behavioral influence
(layered volition context injection) and bounded internal initiative are now both
implemented and live; the bounded-initiative slice was added only because the live system
could already explain the selected goals, omitted/suppressed goals, arbitration result,
and shaping intensity. The remaining work is durability: deciding and implementing what
realtime volition survives across sessions and how it is consolidated for review.

Behavioral influence is also gated on the default realtime seed already including
protected tier-2 explicit-user-intent (`honor-explicit-user-request`) and tier-3
current-task-completion (`complete-current-task`) tensions/goals, with tests proving
they cannot be displaced by curiosity or exploration goals under any mode bias. That
gate is satisfied by Phase 2 and must remain green for every behavioral phase —
realtime bounded initiative derives every initiative from the arbitration winner, so the
same invariant automatically bounds which goals can produce initiative, and Phase 6
persistence/reseed must not introduce a path that lets a reviewed seed displace the
protected tiers.

## Phasing Principles

- Each phase builds, passes focused tests, then passes
  `cargo clippy --all-targets -- -D warnings` and `cargo fmt`.
- UI changes under `crates/qsf_realtime_server/ui/` also pass `npm run check` and
  `npm run fmt`.
- Reducers remain pure and unit-testable. State changes flow through `VolitionEvent`
  applied by `qsf_volition::apply` (via `guard.volition.apply_events`); selectors and
  packet builders read snapshots and never mutate.
- View/context derivation stays in pure selectors/builders, not inline route handlers
  or UI components.
- Entry points (`main.rs`, `mod.rs`, `lib.rs`) stay thin wrappers.
- New flags or thresholds must default to exercising the new code path.
- Behavior influence is explicit, trace-backed, and bounded; no external write-capable
  effect is introduced by this plan.
- Human live-voice testing is required before considering a phase complete when it
  changes the spoken experience.
- Runtime modules and artifact names use stable behavior names, not plan phase names.

## Phase Overview

| Phase | Slice | Code? | Human test? | Status | Validation scaffold |
|---|---|---:|---:|---:|---|
| 1 | Extract pure volition domain into `qsf_volition` | Yes | No | Complete | `Experiment.RealtimeVolitionReadOnlyInspection` scaffold reuses fixtures after extraction |
| 2 | Add realtime-owned `VolitionRuntimeState` seeded per QSF session | Yes | Light | Complete | `Experiment.RealtimeVolitionStateSeed` |
| 3 | Expose read-only realtime volition tools | Yes | Yes | Complete | `Experiment.RealtimeVolitionReadOnlyInspection` |
| 4 | Layered volition context injection — stable baseline plus dynamic goals/intentions with opportunity detection + shaping-intensity dial | Yes | Yes | Complete | `Experiment.RealtimeVolitionContextInjection` |
| 5 | Add trace-backed bounded initiative outputs to the live loop | Yes | Yes | Complete | `Experiment.RealtimeVolitionBoundedInitiative` |
| 6 | Persist, inspect, and consolidate realtime volition state | Yes | Yes | Next | `Experiment.RealtimeVolitionContinuity` |
| 7 | Surface volition state in the realtime UI | Yes | Yes | Not started | `Experiment.RealtimeVolitionInspectionUi` |

## Phase Details

### Completed Phases 1-5 (summary)

Phases 1-5 are implemented and validated. The durable outcomes and constraints they
carry into later work:

- **Pure domain crate (Phase 1).** `qsf_volition` owns tensions, goals, `VolitionState`,
  `VolitionEvent`, `apply`, salience, arbitration (`arbitrate`, `arbitrate_with_mode`,
  `Mode`, `ModeArbitrationResult`, `PROTECTED_TIER_FLOOR = 3`), candidate proposals,
  bounded initiative (`InitiativeProposal`, `InitiativeOutput`, `execute_initiative`),
  the seed fixtures (`static_fixture()`, `realtime_seed_fixture()`), term normalization
  (`normalize_terms`, `grounded_terms_from_text`), selection (`select_goals_ranked`,
  `RankedSelectionResult`, `GoalSelection`), opportunity detection (`detect_opportunities`,
  `OpportunitySignal`), the shaping dial (`choose_shaping_intensity`, `ShapingIntensity`,
  `ShapingIntensityInputs`, `ReceptivenessHint`), stance rendering (`render_volition_stance`,
  `stable_baseline_hash`), and inspection (`build_state_inspection` → `VolitionStateInspection`).
  All volition types derive `Serialize`/`Deserialize`. It has no dependency on `qsf_app`,
  providers, HTTP, tokio, or UI. **Constraint for later phases:** keep this boundary; route
  all state change through `VolitionEvent` + `apply`; do not pull realtime or
  context-assembly types into `qsf_volition`.
- **Per-session runtime state (Phase 2).**
  `crates/qsf_realtime_server/src/realtime/volition.rs` defines `VolitionRuntimeState`
  (`state` + `fixture`), seeded from `realtime_seed_fixture()` on session creation,
  isolated per session, in-memory only (no persistence yet). The protected seed goals
  `honor-explicit-user-request` (tier-2) and `complete-current-task` (tier-3) are present
  and start `Accepted`. **Constraint:** these protected-tier goals must beat tier-7
  curiosity/exploration goals under `Neutral`, `Focused`, and `Exploratory`; this
  invariant gates all behavioral phases and must stay green.
- **Trusted-turn mapping (Phase 2).** `events_for_trusted_transcript(...)` is the pure,
  deterministic, model-free map from a trusted user transcript to `VolitionEvent`s
  (tick lifecycle → `TickAdvanced` → `GoalActivated` for keyword matches on
  `Accepted`/`Active` goals; cooldown and retired goals are not activated).
  `apply_trusted_transcript_to_volition(guard, transcript)` in `sideband.rs` calls it once
  per trusted turn boundary (text turn, `StartTurn`, and post-`Interrupt`) and applies the
  events via `guard.volition.apply_events`. **Lessons/constraints:** the volition tick
  advances on the trusted user-turn boundary; the live path is `Neutral`-only — mode never
  changes from inferred sideband signals, only from an explicit `VolitionEvent::ModeChanged`;
  no automatic goal satisfaction; provider/browser diagnostic events never mutate volition
  state.
- **Read-only tools (Phase 3).** `inspect_volition_state` and `select_volition_goals`
  (`volition_tools.rs`) are registered in the default allow-list, permission-checked,
  budget-capped, deterministic, and emit a parseable `volition_tool_trace` summary that
  carries no secrets. Trusted sideband exchanges record to diagnostics with
  `source: "sideband_trusted"`, `trust: "trusted"`. **Constraint:** `select_volition_goals`
  is the *full ranked inspection detail* surface and stays distinct from the compact ambient
  injection packet.
- **Layered context injection (Phase 4).** Volition shapes the live response through
  layered, trace-backed context built in the shared per-turn helper
  `inject_trusted_turn_context_and_response` in `sideband.rs` (used by **both** the typed
  path and the voice `input_audio_transcription.completed` arm), with pure builders in
  `crates/qsf_realtime_server/src/realtime/volition_injection.rs`. Durable facts:
  - **Stable baseline** (`build_stable_baseline_instructions` → `render_volition_stance`,
    wrapped with the realtime/project trust-boundary preamble) is composed into the base
    instructions used by both the initial and every per-turn `session.update` (and therefore
    `response.create`); the *content* never changes per turn, verified by a stable
    `stable_baseline_hash`. The composed default lives in `state.rs`
    (`default_instructions_with_volition_baseline`).
  - **Per-turn dynamic packet** (`build_volition_turn_context_packet`) is sent as a single
    system `conversation.item.create` **after** the optional memory item and **before** the
    initial `response.create`. It is computed from the post-mapping snapshot using
    `select_goals_ranked` → `detect_opportunities` → `arbitrate_with_mode` →
    `choose_shaping_intensity`, **independently of memory retrieval**.
  - **Arbitration + dial guarantees.** `arbitrate_with_mode` returns `None` for empty
    selection (callers short-circuit). The winner (`ModeArbitrationResult.winner`, a
    `GoalSelection` carrying `.goal` and `.initiative: InitiativeProposal`) respects protected
    tiers and mode bias. `choose_shaping_intensity` clamps to ≤ `Low` when the winner is
    protected (`winner_bias.effective_tier <= PROTECTED_TIER_FLOOR`).
  - **Tracing.** `DiagnosticRecord::VolitionContextInjected { qsf_session_id, exchange_index,
    recorded_at, trace }` carries `VolitionContextInjectionTrace`; its
    `response_create_event_ref` is the per-turn `hash_request_sequence(turn_request_values)`
    value — reuse this reference style for new per-turn traces.
  - **Tool-loop boundary.** The tool-loop continuation `response.create` in
    `handle_response_done_event` must **not** receive a fresh per-turn volition packet.
  - Validated by `Experiment.RealtimeVolitionContextInjection`, whose "Injected Text Contract"
    pins the rendered baseline and per-turn packet text asserted verbatim in tests.
- **Bounded internal initiative (Phase 5).** The arbitration winner produces a bounded
  *internal* `InitiativeOutput` on each trusted turn, surfaced gently through the existing
  per-turn volition packet and traced — with no external write-capable effect. Durable facts:
  - New module `crates/qsf_realtime_server/src/realtime/volition_initiative.rs` owns
    `RealtimeBoundedInitiativeTrace`, `RealtimeBoundedOrExternalOutput`
    (`external_effect_executed: false` by construction), the pure
    `render_initiative_line(output, intensity)` (a bounded line for reflection / open-thread /
    experiment; `None` for `ContextRetrievalRequested` and for `None` intensity), and
    `build_realtime_bounded_initiative_trace`.
  - Initiative is derived from the arbitration winner
    (`execute_initiative(&winner.initiative, &winner.goal)`) inside
    `inject_trusted_turn_context_and_response` and applied through the single
    `VolitionEvent::InitiativeExecuted` `apply_events` call; the tool-loop continuation
    `response.create` produces no initiative.
  - `SidebandRuntimeState` carries `pending_context_retrieval_hints` (survives the turn
    boundary; consumed by the next turn's `retrieve_session_memories` query, then cleared) and
    `previous_turn_surfaced_goal_id` (anti-nag alternation; set only on surfaced turns, cleared
    otherwise).
  - Surfacing gate (record always, surface conditionally): suppressed when intensity is `None`,
    when a protected winner lacks a genuine opportunity signal beyond its own topic self-match,
    or when the same goal surfaced on the immediately preceding trusted turn.
  - Tracing: `DiagnosticRecord::RealtimeBoundedInitiative { qsf_session_id, exchange_index,
    recorded_at, trace }`, written next to `VolitionContextInjected` for the same turn and
    sharing its `response_create_event_ref`; the trace embeds compact
    `state_snapshot_before/after` (`VolitionStateInspection`).
  - **Constraints for Phase 6:** every initiative is recorded to diagnostics even when not
    surfaced (the consolidation input is the diagnostics stream, not only surfaced lines);
    `external_effect_executed` is `false` by construction and must stay so; the protected-tier
    invariant bounds which goals can ever win, and persistence/reseed must not weaken it.
    Validated by `Experiment.RealtimeVolitionBoundedInitiative.md`.

The `volition_tool_trace` (Phase 3), `volition_context_injection_trace` (Phase 4), and
`realtime_bounded_initiative_trace` (Phase 5) contracts remain the model for trace fields and
parsing-based verification: a record carries `qsf_session_id`, an exchange/tick reference, the
selection/arbitration/mode-bias outcomes, a content hash, and a resolvable reference to the
outbound `response.create`.

### Phase 6 - Persist, inspect, and consolidate realtime volition state

Decide and implement what parts of realtime volition survive across QSF sessions, write a
versioned per-session volition snapshot alongside the continuity artifacts the realtime server
already persists, feed those snapshots and the existing volition diagnostics into a pure
extraction step, and surface a reviewed consolidation that can re-seed future sessions only
after explicit human acceptance. No new write-capable external effect is introduced:
consolidation proposes, humans accept.

The realtime server already writes `session-state.json` and updates `continuity-manifest.json`
on every trusted turn (`crates/qsf_realtime_server/src/realtime/sideband.rs`) and reloads
`memory-store.json` across sessions (`crates/qsf_realtime_server/src/realtime/memory_store.rs`),
and every volition turn already records `VolitionContextInjected` and `RealtimeBoundedInitiative`
diagnostics carrying compact `VolitionStateInspection` snapshots. So Phase 6 is a
persistence-boundary decision plus a write / pure-extraction / reviewed-reseed slice, not new
domain modeling.

#### Persistence-boundary decision (record in the design note and DecisionLog before coding)

- Realtime volition is **written, not blindly reloaded**. Each session keeps seeding live
  `VolitionRuntimeState` from `realtime_seed_fixture()` plus any reviewed durable seed; it never
  restores a prior session's live `VolitionState` verbatim. This keeps continuity "useful but
  not sticky" and prevents stale-goal traps.
- Cross-session durable change (accepted candidates, seed adjustments) flows **only through an
  explicit human-run reviewed-acceptance step**, mirroring the reviewed-memory acceptance
  workflow in `crates/qsf_app/src/experiments/accept_reviewed_memory.rs` (which writes a
  reviewed artifact, `voice-memory.reviewed.json`) — **not** the sleep `auto_promote` path.
  `crates/qsf_app/src/sleep/auto_promote.rs` promotes sleep memory candidates *automatically*
  and is therefore explicitly **not** the human gate for volition seeds. The consolidation pass
  only *proposes* durable volition changes; a human must run the reviewed-acceptance step to
  write the durable reviewed volition seed before it can affect any future session. Nothing
  auto-promotes an accepted goal into future behavior.
- `Mode` resets to `Neutral` on every new session (the realtime live path is already
  `Neutral`-only).
- Pure pattern extraction lives in `qsf_volition` (no `qsf_app`/realtime deps); the realtime
  server only writes artifacts; orchestration lives in the `qsf_app` sleep pass. This preserves
  the `qsf_realtime_server` no-`qsf_app` boundary and reducer purity.

Why this over the alternatives: a full `VolitionState` reload would auto-carry accepted
candidates and dynamic drift across sessions, violating the "not sticky" requirement and the
no-auto-promotion safety boundary. A compact-derived-memory-only artifact would lose the
per-turn detail the sleep pass needs to spot recurring/blocked patterns. The chosen boundary
keeps full detail in append-only artifacts for review while the live seed stays deterministic
and bounded — the proper long-term solution per AGENTS.md.

#### Integration map (where this hooks in)

- Continuity paths + `create_session` + `SessionRuntime`: `crates/qsf_realtime_server/src/state.rs`
  (`continuity_session_dir`, `continuity_session_state_path`, `continuity_manifest_path`,
  `continuity_memory_store_path`).
- Trusted-turn persist flow that writes `session-state.json` + updates the manifest:
  `crates/qsf_realtime_server/src/realtime/sideband.rs` (`promote_completed_trusted_exchanges` →
  `persist_session_state` + `ContinuityManifest::load_or_default`/`persist`, around the
  trusted-exchange recording).
- Live volition seeding: `VolitionRuntimeState::new()` in
  `crates/qsf_realtime_server/src/realtime/volition.rs`.
- Graceful-degradation precedent: `MemoryStore::load_or_empty` in
  `crates/qsf_realtime_server/src/realtime/memory_store.rs`.
- Continuity manifest schema: `ContinuityManifest` in `crates/qsf_session/src/manifest.rs`
  (`CONTINUITY_MANIFEST_SCHEMA_VERSION`, atomic temp-file persist, hard-fail on version mismatch).
  Schema-version + migration precedent: `SessionState::upgrade_schema_version` and
  `crates/qsf_session/src/resume.rs`.
- Compact snapshot projection: `build_state_inspection` → `VolitionStateInspection` in
  `crates/qsf_volition/src/inspection.rs`.
- Existing volition diagnostics: `DiagnosticRecord::VolitionContextInjected` and
  `RealtimeBoundedInitiative` in `crates/qsf_realtime_server/src/diagnostics.rs`.
- Sleep orchestration + proposer flow: `crates/qsf_app/src/sleep/`
  (`summarize_session`, `commit`, the proposer/reviewed-memory flow). Note `auto_promote`
  here is **automatic** sleep-memory promotion and is **not** the human gate for volition
  seeds.
- Explicit reviewed-artifact acceptance (the human gate this phase reuses):
  `crates/qsf_app/src/experiments/accept_reviewed_memory.rs` (writes
  `docs/Experiments/Fixtures/voice-memory.reviewed.json`) — the precedent for the volition
  reviewed-acceptance step that writes `volition-seed.reviewed.json`.
- `qsf_app` depends on `qsf_volition` and `qsf_session` but **not** `qsf_realtime_server`; keep
  it that way (see the Step 6a parsing-boundary decision).

#### Build (incremental, each step independently reviewable)

**Step 6a — Design note + DecisionLog (no code).** Record the persistence-boundary decision
above and the chosen defaults for the open questions below. The persistence shape is non-trivial
(it spans three crates and a human-review gate), so AGENTS.md requires the note before
implementation. Decide explicitly, before any later step:

- **Reviewed volition seed artifact + promotion marker + acceptance workflow.** Define the
  durable reviewed seed (default: a dedicated `volition-seed.reviewed.json` in the continuity
  root) carrying an explicit human-promotion marker (e.g., `promoted_by` / `promoted_at` /
  source consolidation `artifact_reference`), and the explicit acceptance step that writes it.
  Model that step on `accept_reviewed_memory.rs` (a human-run reviewed-artifact acceptance),
  generalized for volition; it is **not** the sleep `auto_promote` path. `create_session`
  consumes only this reviewed artifact.
- **Diagnostics/trace parsing boundary for `qsf_app`.** `qsf_app` must read realtime volition
  artifacts without gaining a `qsf_realtime_server` dependency. Default: keep the consolidation
  *inputs* volition-native (Step 6g) and have the sleep pass project the realtime artifacts into
  them via a minimal, explicitly-versioned `serde_json` projection (or small neutral structs in
  `qsf_volition` / a new `qsf_diagnostics` crate). Do not widen the realtime server's boundary;
  prefer moving a shared trace schema down a crate over an upward dependency. Pin the choice
  here and add tests against real diagnostic JSONL records.
- **Snapshot persistence cadence.** Volition snapshots are written **in lockstep with
  `session-state.json` continuity promotion** — only for continuity-promoted trusted exchanges
  (see Step 6c). Diagnostics from degraded/non-promoted exchanges remain observable evidence for
  consolidation but are **non-seedable** and never produce a snapshot.
- **Realtime state-root layout contract.** Document that for a given `qsf_session_id`, continuity
  artifacts live under `<state_dir>/continuity/<qsf_session_id>/` (`session-state.json`,
  `continuity-manifest.json`, `memory-store.json`, `volition-state.json`, plus the root
  `volition-seed.reviewed.json`) and diagnostics under
  `<state_dir>/diagnostics/<qsf_session_id>.jsonl`, so the sleep reader can resolve all inputs
  from one state root.

Output: a short `docs/Plans/Design.*.md` (or an addition to an existing volition design note)
plus a `docs/DecisionLog.md` entry naming the behavior "realtime volition continuity".

**Step 6b — Versioned volition snapshot type (pure).** Add a serializable
`VolitionContinuitySnapshot { schema_version: u16, recorded_at: String, qsf_session_id,
seed_fixture_id, state: VolitionState, inspection: VolitionStateInspection }` with its own
`schema_version` and a forward-compatible loader that mirrors
`SessionState::upgrade_schema_version` (accept the current and known-older versions, never
panic). `recorded_at` is an RFC3339 timestamp **string supplied by the caller** so `qsf_volition`
stays time-dependency-free and the type stays pure; the realtime server passes the trusted-turn
timestamp it already records. Pure round-trip + upgrade unit tests, plus a byte-stability test
that serializes the **same** snapshot value twice and asserts identical bytes (stable over the
`BTree`-backed state; not two freshly-timestamped snapshots). Prefer `qsf_volition` to keep it
pure and reusable; if it must reference realtime-only fields, put the thin wrapper in
`crates/qsf_realtime_server/src/realtime/volition.rs` and keep the inner types from
`qsf_volition`.

**Step 6c — Write the snapshot on the trusted-turn boundary (`qsf_realtime_server`).** In the
same `sideband.rs` flow that promotes and writes `session-state.json`
(`promote_completed_trusted_exchanges` → `persist_session_state`), also write
`volition-state.json` into the continuity session dir from the live `VolitionRuntimeState`, using
the same atomic temp-file+rename pattern (`ContinuityManifest::persist` is the precedent). Write
the snapshot **in lockstep with continuity promotion**: persist it only for continuity-promoted
trusted exchanges, so `session-state.json` and `volition-state.json` never disagree about which
exchanges are durable. Degraded/non-promoted exchanges still record
`VolitionContextInjected`/`RealtimeBoundedInitiative` diagnostics (observable but non-seedable)
but produce no snapshot. Add `continuity_volition_snapshot_path(&self, id)` to `AppState`
alongside the existing continuity-path helpers. Default-on (no flag), so it exercises the new
path on every continuity-promoted trusted turn per AGENTS.md.

**Step 6d — Reference the snapshot from the manifest, backward-compatibly (`qsf_session`).** Add
`#[serde(default)] current_volition_snapshot_path: Option<PathBuf>` to `ContinuityManifest` and set
it in the realtime persist flow. Keep `CONTINUITY_MANIFEST_SCHEMA_VERSION = 1` — the field is
additive and serde-default backfills legacy files, so legacy manifests still load and the existing
hard-fail-on-mismatch behavior is unchanged. Update the `persist_then_reload_preserves_all_fields`
golden test and add a regression test that a legacy manifest JSON without the field loads with
`current_volition_snapshot_path == None`.

**Step 6e — Record surfacing outcome on the durable bounded-initiative trace
(`qsf_realtime_server`).** The current `RealtimeBoundedInitiativeTrace` records the winning goal,
output, hint terms, and before/after snapshots, but not whether the initiative was surfaced or why
it was suppressed, and `VolitionContextInjected` omits the rendered initiative line — so
correlating records by `response_create_event_ref` can prove an initiative was *generated* but
cannot distinguish anti-nag suppression, protected-goal suppression, `ShapingIntensity::None`, or
an inherently non-renderable `ContextRetrievalRequested` output. Extend the durable trace with
`surfaced: bool` and a structured `suppression_reason`
(`None` | `Intensity` | `ProtectedNoOpportunity` | `AntiNagRepeat` | `NonRenderableOutput`), plus
`rendered_line_present: bool` (no rendered text, no secrets). This is a prerequisite for the
proposed-but-not-surfaced consolidation category in Step 6g/6h. Add parsing tests that each
suppression reason round-trips from a real diagnostics JSONL record.

**Step 6f — Seed-time load with graceful degradation + reviewed reseed (`qsf_realtime_server`
+ pure merge in `qsf_volition`).** On `create_session`, after seeding from
`realtime_seed_fixture()`, apply only the *reviewed durable seed* (`volition-seed.reviewed.json`)
if one exists. `create_session` reads **only** the reviewed seed artifact — it never reads or
restores the prior live `volition-state.json` snapshot (that raw artifact is consumed solely by
the sleep/consolidation reader in Step 6h). Mode stays `Neutral`. A corrupt or missing **reviewed
seed** degrades to the plain fixture (log + a diagnostics note, mirroring
`MemoryStore::load_or_empty`) and never panics; raw-snapshot corruption is out of scope for
seeding and is handled by the sleep reader.

The reseed is applied by a **pure `qsf_volition` merge/apply function**
(`apply_reviewed_seed(fixture, reviewed_seed) -> VolitionState`) with enforced invariants: every
fixture protected goal (tier-2 `honor-explicit-user-request`, tier-3 `complete-current-task`)
remains present with its original tier/effects; reviewed additions cannot overwrite or alias a
fixture goal id; reviewed goals cannot be admitted at or below `PROTECTED_TIER_FLOOR`; and the
merge is order-independent. Unit-test the merge directly, and assert post-merge arbitration still
resolves the protected goals as dominant under `Neutral`, `Focused`, and `Exploratory`.

**Step 6g — Pure consolidation extraction (`qsf_volition`, new `consolidation.rs`).** Add pure
functions over a sequence of volition snapshots / per-turn outcomes that compute a
`VolitionConsolidationReport`: recurring selected/winning goals, often-blocked goals,
accepted/rejected candidates, mode changes, and bounded initiatives proposed-but-not-surfaced or
not-acted-on. The proposed-but-not-surfaced category is derived from the per-turn outcome
record's `surfaced` / `suppression_reason` fields (Step 6e), not inferred from record presence.
Every report item carries artifact references (snapshot / diagnostics record identifiers), never
free-form claims, and every *proposed durable change* carries a `promotion_status` distinguishing
`proposed` from `human-promoted`. Inputs are volition-native (`Vec<VolitionStateInspection>` plus
a small per-turn outcome record that includes the surfacing outcome), so `qsf_volition` stays free
of realtime/`qsf_app` types. Deterministic, unit-tested.

**Step 6h — Wire extraction into the sleep pass + explicit reviewed acceptance (`qsf_app`).** The
`qsf_app` sleep pass reads the realtime continuity volition artifacts (`volition-state.json`, the
manifest) + the diagnostics JSONL for the session (resolved from the one state root per the Step
6a layout contract), projects them into the volition-native inputs via the Step 6a parsing
boundary, runs `qsf_volition` consolidation, and emits a consolidation section whose claims cite
artifact references. The sleep pass only **proposes** durable changes (accepted-goal promotions,
seed adjustments) with `promotion_status: proposed`; it must **not** call `auto_promote` for
volition seeds. A durable reviewed volition seed (`volition-seed.reviewed.json`) is written
**only** by the explicit human-run reviewed-acceptance step defined in Step 6a (modeled on
`accept_reviewed_memory.rs`), which stamps the human-promotion marker. Reuse `summarize_session` /
the proposer-commit flow for emitting the proposal section rather than adding a parallel path, but
keep the durable seed write behind the manual acceptance step.

**Step 6i — Experiment scaffold + docs.** Create
`docs/Experiments/Experiment.RealtimeVolitionContinuity.md` with the trace completeness contract
below. Update `docs/Architecture/Architecture.StateAndObservability.md` (new artifact + snapshot
trace, the extended bounded-initiative trace fields, and the state-root layout contract),
`docs/Architecture/Architecture.VolitionSystem.md` (consolidation behavior + reviewed-seed merge
invariants), `docs/Architecture/Architecture.RealtimeSessionServer.md` (volition persistence +
reviewed-seed acceptance), `docs/Experiments/Experiment.Backlog.md`, and `docs/DecisionLog.md`
(persistence boundary + reviewed-acceptance gate).

#### Verify

- **Round-trip determinism:** a `VolitionContinuitySnapshot` persisted and reloaded is
  structurally equal; serializing the **same** snapshot value twice is byte-identical; the
  upgrade loader accepts the current and known-older `schema_version` without panic.
- **Legacy artifact loading:** a legacy continuity manifest with no `current_volition_snapshot_path`
  loads (field defaults to `None`); existing realtime continuity directories still resume.
- **Snapshot cadence:** a continuity-promoted trusted exchange writes `volition-state.json` in
  lockstep with `session-state.json`; a degraded/non-promoted exchange writes neither (its
  volition mutations remain only in diagnostics, observable but non-seedable).
- **Seed-time graceful degradation:** a corrupt or missing **reviewed seed**
  (`volition-seed.reviewed.json`) falls back to `realtime_seed_fixture()` and emits a diagnostics
  note; no panic, session still starts. `create_session` never reads the raw `volition-state.json`
  snapshot.
- **Reviewed-seed merge safety:** the pure `apply_reviewed_seed` keeps every fixture protected
  goal present at its original tier/effects, rejects reviewed ids that overwrite/alias fixture
  ids, rejects reviewed goals at/below `PROTECTED_TIER_FLOOR`, and is order-independent.
- **Not-sticky guarantee:** a new session with the stable default session id seeds from the fixture
  with `Mode::Neutral` and does not silently carry the prior run's accepted candidates or dynamic
  drift.
- **Surfacing-aware consolidation:** the proposed-but-not-surfaced report category is computed from
  parsed `surfaced` / `suppression_reason` trace fields and correctly separates anti-nag,
  protected-no-opportunity, intensity-`None`, and non-renderable suppression from genuinely
  surfaced initiatives.
- **Consolidation determinism + grounding:** extraction is deterministic and every report item
  resolves to a real snapshot/diagnostics artifact reference (parsed from the persisted files, not
  in-memory structs).
- **Human-review gate:** a consolidation report alone does **not** alter the next session; a
  durable seed change takes effect only after the explicit human-run reviewed-acceptance step
  writes `volition-seed.reviewed.json` with a human-promotion marker. `auto_promote` is never
  invoked for volition seeds, and an unpromoted proposal does not alter the next session's seed.
- **Protected-tier safety:** a reviewed reseed cannot lower or displace the protected tier-2/tier-3
  goals; assert protected goals remain present and dominant after any reseed under `Neutral`,
  `Focused`, and `Exploratory`.
- **State-root resolution:** the sleep reader resolves `session-state.json`, `volition-state.json`,
  the manifest, the reviewed seed, and the diagnostics JSONL from a single realtime state root for
  a given `qsf_session_id`.
- `cargo test`, then `cargo clippy --all-targets -- -D warnings`, then `cargo fmt`.

#### Human test

- Run two realtime sessions with the stable default session id. Confirm the second session
  benefits from reviewed continuity (e.g., a candidate promoted via the reviewed-acceptance step is
  available) but is not trapped in stale goals, and that mode starts `Neutral`.
- Confirm the consolidation output cites volition artifact references rather than free-form claims,
  and that no cross-session accepted-goal change took effect until the explicit reviewed-acceptance
  step was run.

#### Trace completeness contract

`volition_continuity_snapshot` must contain:

- `schema_version`
- `qsf_session_id`
- `recorded_at` (RFC3339 string)
- `seed_fixture_id`
- the serialized `VolitionState`
- the compact `VolitionStateInspection`

The per-turn bounded-initiative outcome (from the extended `realtime_bounded_initiative_trace`,
Step 6e) must additionally carry:

- `surfaced` (bool)
- `suppression_reason` (`None` | `Intensity` | `ProtectedNoOpportunity` | `AntiNagRepeat` |
  `NonRenderableOutput`)
- `rendered_line_present` (bool, no rendered text, no secrets)

so the proposed-but-not-surfaced / not-acted-on consolidation category is grounded in recorded
fields rather than inferred from record presence.

`volition_consolidation_report` items must each contain:

- the pattern kind (recurring-selected / often-blocked / accepted-or-rejected-candidate /
  mode-change / unacted-initiative)
- the involved goal or candidate id
- a count or tick range
- an `artifact_reference` to the snapshot(s) / diagnostics record(s) the item was derived from
- for any proposed durable change, a `promotion_status` field distinguishing proposed from
  human-promoted

The artifact boundary is the per-session continuity directory (`volition-state.json` plus the root
`volition-seed.reviewed.json`) plus the diagnostics JSONL stream. Automated verification parses the
persisted files and asserts that every consolidation claim resolves to a real artifact and that no
durable seed change exists without a recorded human-promotion marker.

#### Resolved defaults (from the plan's stated assumptions)

- Mode does not persist; it resets to `Neutral` each session unless a reviewed durable memory
  explicitly says otherwise.
- Accepted goal candidates are reviewed memory for cross-session continuity and live state for
  per-session mechanics — they do not auto-carry across sessions.
- Volition snapshots are written in lockstep with `session-state.json` continuity promotion;
  degraded/non-promoted exchanges remain observable in diagnostics but are non-seedable (no
  snapshot).
- The cross-session durable gate is the explicit human-run reviewed-acceptance step (modeled on
  `accept_reviewed_memory.rs`), **not** the automatic sleep `auto_promote` path.

#### Open questions (decide in the Step 6a design note; defaults noted)

- **Which dynamic `VolitionState` fields, if any, carry across same-default-session-id reloads** —
  tick, salience, cooldowns, `last_activated_tick`? Default: none carry; every session starts at the
  fixture (tick 0) and only the reviewed seed differs. Revisit only if `Design.Chronoception.md`
  requires tick continuity, in which case carry tick alone and document it.
- **Should reviewed volition-seed acceptance be a new dedicated command/experiment, or a
  generalization of the existing reviewed-memory acceptance into a shared reviewed-artifact
  workflow?** Default: ship a dedicated volition reviewed-acceptance step first (smallest correct
  slice, modeled on `accept_reviewed_memory.rs`), and only generalize into a shared
  reviewed-artifact abstraction once a second consumer appears — avoid building the abstraction
  before there are two users.
- **Where does the durable reviewed volition seed live and what is its shape** — a reviewed-memory
  record consumed at seed time, or a dedicated `volition-seed.reviewed.json` in the continuity root?
  Default: a dedicated `volition-seed.reviewed.json` in the continuity root carrying an explicit
  human-promotion marker, written only by the reviewed-acceptance step (Step 6a/6h), for an
  explicit and auditable cross-session channel. (This replaces the earlier `auto_promote`-gated
  phrasing: `auto_promote` is automatic and is not the human gate.)
- **Diagnostics/trace parsing boundary for `qsf_app`** — decided in Step 6a. `qsf_app` currently
  depends on `qsf_volition`/`qsf_session` but not `qsf_realtime_server`. Default: keep extraction
  inputs volition-native (Step 6g) and have the sleep pass project the realtime artifacts into them
  via a minimal versioned `serde_json` projection (or small neutral structs in
  `qsf_volition`/`qsf_diagnostics`); add a `qsf_app → qsf_realtime_server` dependency only as a last
  resort, preferring to move a shared schema down a crate over widening the realtime server's
  boundary.

### Phase 7 - Surface volition state in the realtime UI

Add a lightweight inspection view in the realtime browser UI after the backend state
and tool contracts are stable.

Build:

- Add a Volition panel or tab to the realtime UI.
- Show compact state: mode, tick, active/winning goal, selected goals, suppressed
  goals, pending candidates, last bounded initiative output, and trace links or ids.
- Prefer dense operational UI over explanatory marketing copy.
- Do not expose secrets or raw provider payloads.
- Keep UI state derived from API responses/selectors, not duplicated component logic.

Verify:

- `npm run check` and `npm run fmt` in `crates/qsf_realtime_server/ui/`.
- UI reducer/selector tests for the volition view model.
- Browser smoke test confirms no overlapping text at desktop/mobile widths.
- Backend endpoint tests enforce session isolation and no-secret output.

Human test:

- During a live conversation, inspect the panel and confirm it helps explain behavior
  without interrupting the spoken interaction.

## Experiments To Create

Create focused experiment scaffolds as phases begin. `Experiment.RealtimeVolitionStateSeed`,
`Experiment.RealtimeVolitionReadOnlyInspection`, `Experiment.RealtimeVolitionContextInjection`,
and `Experiment.RealtimeVolitionBoundedInitiative` exist and are implemented;
`Experiment.RealtimeVolitionContextInjection` carries the realized injected-text contract.
`Experiment.RealtimeVolitionContinuity` is the next scaffold to create (Phase 6).

- `docs/Experiments/Experiment.RealtimeVolitionStateSeed.md`
- `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md`
- `docs/Experiments/Experiment.RealtimeVolitionContextInjection.md`
- `docs/Experiments/Experiment.RealtimeVolitionBoundedInitiative.md`
- `docs/Experiments/Experiment.RealtimeVolitionContinuity.md`
- `docs/Experiments/Experiment.RealtimeVolitionInspectionUi.md`

Each experiment should define a trace completeness contract before implementation,
including required fields, artifact boundary, and artifact-parsing verification.

## Cross-Cutting Safety Boundaries

- Realtime volition must not claim real subjective experience.
- Volition outputs are simulated internal state and bounded proposals.
- Write-capable external effects are out of scope for this plan.
- Provider/browser-relayed diagnostic events do not directly mutate volition state.
- Protected arbitration tiers remain immune to mode bias.
- User intent and current task completion outrank curiosity/exploration.
- Cross-session durable volition change requires an explicit human-run reviewed-acceptance
  step; the automatic sleep `auto_promote` path is never the gate for volition seeds.
- All live tools remain permission-checked and recover from denial.
- `OPENAI_API_KEY` and other secrets never appear in volition traces, tool outputs,
  UI payloads, diagnostics, or reports.

## Documentation To Update

Per [`ProjectWorkflow.md`](../ProjectFrame/ProjectWorkflow.md), update documents when
the work starts or changes durable behavior:

- This plan as phases start, complete, or change shape.
- The per-phase `Experiment.*.md` files under `docs/Experiments/`.
- `docs/Experiments/Experiment.Backlog.md` when each realtime-volition experiment is
  promoted from planned to running/completed.
- `docs/Architecture/Architecture.RealtimeSessionServer.md` when volition state or
  tools (including bounded initiative and persistence) become part of the realtime server.
- `docs/Architecture/Architecture.ToolSystem.md` when live volition tools are added.
- `docs/Architecture/Architecture.ContextManagement.md` when volition context packets or
  context-retrieval initiative hints influence live response context.
- `docs/Architecture/Architecture.VolitionSystem.md` for the extracted `qsf_volition`
  crate, the bounded-initiative behavior, and the volition consolidation/extraction behavior.
- `docs/Architecture/Architecture.StateAndObservability.md` when volition traces
  (including `realtime_bounded_initiative_trace`) or persistence artifacts are added.
- `docs/ProjectFrame/ProjectVision.md` if the final project target or realtime
  consciousness-simulation framing changes.
- `docs/DecisionLog.md` for durable commitments: crate boundary, live tool scope
  expansion, behavioral influence boundary, the realtime bounded-initiative boundary,
  persistence/consolidation boundary, the reviewed-acceptance gate, or any change to
  protected-tier safety.

This plan is ephemeral. Durable documents should refer to named behaviors such as
"realtime volition inspection", "volition context injection", "realtime bounded
initiative", or "realtime volition continuity", not to this plan's phase numbers.

## Acceptance Criteria For The Whole Plan

- `qsf.ps1 realtime` starts a live voice session where volition state is available
  through read-only inspection and UI/API surfaces.
- The live sideband can inject compact, trace-backed volition context before
  response creation.
- Bounded internal initiatives can be produced and explained without executing
  external effects.
- Realtime volition behavior is covered by automated tests, parsed trace-artifact
  verification, and human live-voice testing.
- The realtime server still does not depend on `qsf_app`.
- The extracted volition domain is pure, reusable, and documented as current
  architecture after implementation.
- The final user-facing surface remains realtime voice conversation, not an offline
  experiment-only path.