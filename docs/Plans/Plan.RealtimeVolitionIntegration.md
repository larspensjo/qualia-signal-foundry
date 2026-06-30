# Plan: Realtime Volition Integration

## Status

In progress. Phases 1-6 are complete. **Phase 7 is the next implementation slice**:
surface volition state in the realtime browser UI. The backend state and trace contracts
are now stable — every trusted turn already computes a compact `VolitionStateInspection`
and a rich per-turn decision (selection, arbitration winner, shaping intensity, bounded
initiative output plus its `surfaced` / `suppression_reason` / `rendered_line_present`
outcome), and the realtime server already proves out a live read-only inspection surface:
the **realtime turn context inspector** publishes a per-session capture over a `watch`
channel and a `turn_context` events-socket message, which the browser parses, reduces, and
renders in a collapsible panel. So Phase 7 is primarily a *mirror of that established
inspector pattern for volition* — a backend capture type + per-session watch channel +
events-socket forward, plus a UI parser / reducer field / view-model selector / collapsible
panel — not new domain or persistence work. Read the compacted "Completed Phases 1-6"
summary below for the constraints Phase 7 must respect.

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
initiative outputs, and their causal traces are available in the live spoken loop — and,
with Phase 7, visible in the browser UI for live operational inspection.

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

As of Phase 6, the pure domain lives in `qsf_volition`, each session carries
`VolitionRuntimeState`, two read-only volition tools are registered, volition influences
the live spoken response through layered, trace-backed context injection, the arbitration
winner produces bounded internal initiative outputs surfaced through the per-turn volition
channel, and realtime volition state is persisted across sessions through a reviewed-seed
continuity boundary with artifact-backed consolidation. The remaining gap Phase 7 closes
is that none of this volition state is visible in the realtime browser UI: it is inspectable
only through tools, diagnostics artifacts, and the offline sleep consolidation pass.

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
arbitration, fixtures, trace structs, bounded initiative output, continuity snapshots,
and consolidation extraction. It must not depend on `qsf_app`. The context-attached
`GoalSelection` / `GoalSelectionResult` shapes carry context assembly data, so those
result types stay in `qsf_app` or become thin adapters in the caller crates.
`qsf_realtime_server` owns live state and side effects, but any volition state changes
still happen through pure volition events.

The first realtime integration was read-only and inspectable. Behavioral influence
(layered volition context injection), bounded internal initiative, and durable
reviewed-seed continuity are all implemented and live. The remaining work is operational
visibility: surfacing the already-computed volition state and per-turn decision in the
browser UI. Phase 7 follows the realtime turn context inspector precedent (a per-session
`watch`-channel capture forwarded on the events socket and rendered by a pure UI
reducer/selector), which already enforces per-session isolation and no-secret payloads and
gives live updates during the spoken conversation — the property the human test needs.

Behavioral influence is also gated on the default realtime seed already including
protected tier-2 explicit-user-intent (`honor-explicit-user-request`) and tier-3
current-task-completion (`complete-current-task`) tensions/goals, with tests proving
they cannot be displaced by curiosity or exploration goals under any mode bias. That
gate is satisfied by Phase 2 and remains green through Phase 6 (the reviewed-seed merge
cannot displace the protected tiers). Phase 7 is read-only inspection and introduces no
state mutation, so it cannot affect this invariant — but the panel must render protected
goals truthfully (including their arbitration tiers and protected status) so an operator
can confirm the invariant holds live.

## Phasing Principles

- Each phase builds, passes focused tests, then passes
  `cargo clippy --all-targets -- -D warnings` and `cargo fmt`.
- UI changes under `crates/qsf_realtime_server/ui/` also pass `npm run check` and
  `npm run fmt`.
- Reducers remain pure and unit-testable. Backend state changes flow through
  `VolitionEvent` applied by `qsf_volition::apply` (via `guard.volition.apply_events`);
  the UI reducer (`reduceConversationState`) stays pure; selectors and packet/view-model
  builders read snapshots and never mutate.
- View/context derivation stays in pure selectors/builders, not inline route handlers
  or UI components.
- Entry points (`main.rs`, `mod.rs`, `lib.rs`) stay thin wrappers.
- New flags or thresholds must default to exercising the new code path.
- Behavior influence is explicit, trace-backed, and bounded; no external write-capable
  effect is introduced by this plan.
- Human live-voice testing is required before considering a phase complete when it
  changes the spoken experience or its live inspection surfaces.
- Runtime modules and artifact names use stable behavior names, not plan phase names.

## Phase Overview

| Phase | Slice | Code? | Human test? | Status | Validation scaffold |
|---|---|---:|---:|---:|---|
| 1 | Extract pure volition domain into `qsf_volition` | Yes | No | Complete | `Experiment.RealtimeVolitionReadOnlyInspection` scaffold reuses fixtures after extraction |
| 2 | Add realtime-owned `VolitionRuntimeState` seeded per QSF session | Yes | Light | Complete | `Experiment.RealtimeVolitionStateSeed` |
| 3 | Expose read-only realtime volition tools | Yes | Yes | Complete | `Experiment.RealtimeVolitionReadOnlyInspection` |
| 4 | Layered volition context injection — stable baseline plus dynamic goals/intentions with opportunity detection + shaping-intensity dial | Yes | Yes | Complete | `Experiment.RealtimeVolitionContextInjection` |
| 5 | Add trace-backed bounded initiative outputs to the live loop | Yes | Yes | Complete | `Experiment.RealtimeVolitionBoundedInitiative` |
| 6 | Persist, inspect, and consolidate realtime volition state | Yes | Yes | Complete | `Experiment.RealtimeVolitionContinuity` |
| 7 | Surface volition state in the realtime UI | Yes | Yes | Next | `Experiment.RealtimeVolitionInspectionUi` |

## Phase Details

### Completed Phases 1-6 (summary)

Phases 1-6 are implemented and validated. The durable outcomes and constraints they
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
  context-assembly types into `qsf_volition`. **Note for Phase 7:** `VolitionStateInspection`
  (`crates/qsf_volition/src/inspection.rs`) is the compact state snapshot, but its
  `GoalStatusSummary` entries carry only id/title/salience/cooldown/last-activated — they do
  **not** carry arbitration tiers or protected status. Per-turn arbitration tiers and protected
  status live in the realtime `VolitionArbitrationSummary` / `VolitionTurnPacketSummary` /
  `VolitionModeBiasOutcome` (see Phase 4), which Phase 7's capture must surface for the
  protected-tier human test.
- **Per-session runtime state (Phase 2).**
  `crates/qsf_realtime_server/src/realtime/volition.rs` defines `VolitionRuntimeState`
  (`state` + `fixture`), seeded from `realtime_seed_fixture()` on session creation,
  isolated per session. The protected seed goals `honor-explicit-user-request` (tier-2) and
  `complete-current-task` (tier-3) are present and start `Accepted`. **Constraint:** these
  protected-tier goals must beat tier-7 curiosity/exploration goals under `Neutral`,
  `Focused`, and `Exploratory`; this invariant gates all behavioral phases and must stay
  green.
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
    `choose_shaping_intensity`, **independently of memory retrieval**. **It returns `None`
    when arbitration is `None` or the ranked selection is empty** (a "no-selection turn"); on
    such turns `response.create` still fires but no volition packet, no winner, and no
    `VolitionContextInjected` / `RealtimeBoundedInitiative` diagnostic is produced. Phase 7's
    capture contract must handle this case.
  - **Arbitration + dial guarantees.** `arbitrate_with_mode` returns `None` for empty
    selection (callers short-circuit). The winner (`ModeArbitrationResult.winner`, a
    `GoalSelection` carrying `.goal` and `.initiative: InitiativeProposal`) respects protected
    tiers and mode bias. `choose_shaping_intensity` clamps to ≤ `Low` when the winner is
    protected (`winner_bias.effective_tier <= PROTECTED_TIER_FLOOR`).
  - **Per-turn arbitration/protection fields.** `VolitionArbitrationSummary` carries
    `winner_goal_id/title/summary`, `winner_effective_tier`, `winner_biased_tier`, and
    `loser_count`; `VolitionTurnPacketSummary` carries `protected_tier_active` and a
    `mode_bias_outcomes: Vec<VolitionModeBiasOutcome>` (each `goal_id/title/effective_tier/
    biased_tier/protected`). These are the values Phase 7 surfaces for the protected-tier
    human test.
  - **Tracing.** `DiagnosticRecord::VolitionContextInjected { qsf_session_id, exchange_index,
    recorded_at, trace }` carries `VolitionContextInjectionTrace` (selected goal ids/titles/
    summaries, omitted/suppressed counts, opportunity signals, arbitration result incl.
    winner goal id/title/summary/tiers, mode-bias outcomes, shaping intensity, injected
    layers, `initiative_line`); its `response_create_event_ref` is the per-turn
    `hash_request_sequence(turn_request_values)` value — reuse this reference style for new
    per-turn traces. **This record is only written on selection turns** (the
    `if let Some(turn_packet)` branch in `sideband.rs`).
  - **Tool-loop boundary.** The tool-loop continuation `response.create` in
    `handle_response_done_event` must **not** receive a fresh per-turn volition packet.
  - Validated by `Experiment.RealtimeVolitionContextInjection`, whose "Injected Text Contract"
    pins the rendered baseline and per-turn packet text asserted verbatim in tests.
- **Bounded internal initiative (Phase 5).** The arbitration winner produces a bounded
  *internal* `InitiativeOutput` on each trusted turn, surfaced gently through the existing
  per-turn volition packet and traced — with no external write-capable effect. Durable facts:
  - Module `crates/qsf_realtime_server/src/realtime/volition_initiative.rs` owns
    `RealtimeBoundedInitiativeTrace`, `RealtimeBoundedOrExternalOutput`
    (`external_effect_executed: false` by construction), the pure
    `render_initiative_line(output, intensity)` (a bounded line for reflection / open-thread /
    experiment; `None` for `ContextRetrievalRequested` and for `None` intensity), and
    `build_realtime_bounded_initiative_trace`.
  - Initiative is derived from the arbitration winner
    (`execute_initiative(&winner.initiative, &winner.goal)`) inside
    `inject_trusted_turn_context_and_response` and applied through the single
    `VolitionEvent::InitiativeExecuted` `apply_events` call; the tool-loop continuation
    `response.create` produces no initiative. Initiative is only computed when arbitration is
    `Some` (selection turns); on no-selection turns there is no initiative and
    `previous_turn_surfaced_goal_id` is cleared.
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
  - **Constraints:** every initiative is recorded to diagnostics even when not surfaced (the
    consolidation input is the diagnostics stream, not only surfaced lines);
    `external_effect_executed` is `false` by construction and must stay so; the protected-tier
    invariant bounds which goals can ever win.
    Validated by `Experiment.RealtimeVolitionBoundedInitiative.md`.
- **Reviewed-seed continuity + consolidation (Phase 6).** Realtime volition is *written, not
  blindly reloaded*: a versioned snapshot is persisted per session, a pure consolidation pass
  proposes durable changes from artifacts, and only an explicit human-run reviewed-acceptance
  step can re-seed future sessions. No new write-capable external effect. Durable facts:
  - **Pure crate additions (`qsf_volition`).** `continuity.rs` defines the versioned
    `VolitionContinuitySnapshot` (own `schema_version`, forward-compatible loader mirroring
    `SessionState::upgrade_schema_version`, caller-supplied RFC3339 `recorded_at` string so the
    crate stays time-dependency-free) and the pure reviewed-seed merge `apply_reviewed_seed`
    with enforced invariants (every fixture protected goal kept at its original tier/effects;
    reviewed ids cannot overwrite/alias a fixture id; reviewed goals cannot be admitted at or
    below `PROTECTED_TIER_FLOOR`; order-independent). `consolidation.rs` defines the
    deterministic `VolitionConsolidationReport` over snapshots + per-turn outcome records;
    every item carries an `artifact_reference`, and every proposed durable change carries a
    `promotion_status` distinguishing `proposed` from `human-promoted`.
  - **Realtime persistence (`qsf_realtime_server`).** `realtime/volition_continuity.rs` writes
    `volition-state.json` into the continuity session dir using the atomic temp-file+rename
    pattern, **in lockstep with `session-state.json` continuity promotion** (degraded/
    non-promoted exchanges record diagnostics but produce no snapshot — observable but
    non-seedable).
    **Current `create_session` seeding behavior (corrected; see RECONCILIATION ITEM below).**
    As implemented in `state.rs::create_session`, when `volition-state.json` exists the raw
    snapshot is loaded via `VolitionContinuitySnapshot::load_or_upgrade` and
    `runtime.volition.state` (including `tick`) is **restored from the snapshot**, and *then*
    the reviewed seed (`volition-seed.reviewed.json`) is applied on top via
    `apply_reviewed_seed_to_runtime`; a missing/corrupt seed degrades to whatever state the
    snapshot left (or the plain fixture when no snapshot exists), with a logged diagnostics
    note. `Mode` resets to `Neutral` each session.
    **RECONCILIATION ITEM (carried into Phase 7):** this restores `tick` and goal state from
    the raw snapshot, which diverges from the originally-stated reviewed-seed-only intent
    ("reads only the reviewed seed, never the raw snapshot; tick resets to 0 each session").
    The divergence must be reconciled — either fix `create_session` to match the
    reviewed-seed-only intent, or accept and document snapshot-restore as the intended
    behavior — **before** Phase 7 ships any UI documentation or test that asserts tick-reset
    semantics. Phase 7's panel must display whatever `tick` / `captured_at` the capture
    actually carries from live runtime state and must **not** assert "tick resets each
    session" until this item is resolved.
  - **Extended bounded-initiative trace (`qsf_realtime_server`).** `RealtimeBoundedInitiativeTrace`
    now records `surfaced: bool`, `suppression_reason: Option<VolitionSuppressionReason>`
    (`Intensity` | `ProtectedNoOpportunity` | `AntiNagRepeat` | `NonRenderableOutput`), and
    `rendered_line_present: bool` (no rendered text, no secrets). **This is part of the field set
    Phase 7 surfaces in the UI.**
  - **Manifest reference (`qsf_session`).** `ContinuityManifest` gained
    `#[serde(default)] current_volition_snapshot_path: Option<PathBuf>`; the schema version is
    unchanged (additive field, serde-default backfills legacy files), and resume support resolves
    it.
  - **Orchestration + human gate (`qsf_app`).** The `volition-continuity` experiment runs the
    sleep consolidation pass — it reads realtime artifacts via a minimal versioned `serde_json`
    projection (preserving the `qsf_app` → no-`qsf_realtime_server` boundary) and only
    *proposes* durable changes (`promotion_status: proposed`); it never calls `auto_promote` for
    volition seeds. The `accept-reviewed-volition-seed` experiment is the explicit human gate
    (modeled on `accept_reviewed_memory.rs`) that writes `volition-seed.reviewed.json` with a
    human-promotion marker.
  - **Constraints carried into Phase 7:** the protected tier-2/tier-3 goals remain present and
    dominant after any reviewed reseed under `Neutral`/`Focused`/`Exploratory`; cross-session
    durable change requires the explicit human reviewed-acceptance step; `Mode` is always
    `Neutral` at session start; the snapshot-restore-vs-reviewed-seed-only divergence (above) is
    open and must be reconciled before any tick-reset claim is shipped.
  - **Lessons from human testing (commit "Fix RFC3339 serialization and tick-reset bugs").**
    Snapshot/seed timestamps are RFC3339 *strings* supplied by the caller. Any UI display of
    `tick` / `captured_at` must reflect that timestamps are RFC3339 strings and must source
    `tick` from the live runtime state carried in the capture (do not hard-code a session-start
    value while the snapshot-restore reconciliation item is open).
    Validated by `Experiment.RealtimeVolitionContinuity.md`.

The `volition_tool_trace` (Phase 3), `volition_context_injection_trace` (Phase 4), and
`realtime_bounded_initiative_trace` (Phase 5/6) contracts remain the model for trace fields and
parsing-based verification: a record carries `qsf_session_id`, an exchange/tick reference, the
selection/arbitration/mode-bias outcomes (and, for initiative, the surfacing outcome), a content
hash, and a resolvable `response_create_event_ref` to the outbound `response.create`. The
realtime turn context inspector (`crates/qsf_realtime_server/src/realtime/turn_context.rs` +
the `turn_context_tx` watch channel in `state.rs` + the `push_turn_context` events-socket forward
in `routes.rs` + `parseTurnContextMessage` / `latestTurnContext` / the "Last turn context" panel
in the UI) is the structural precedent Phase 7 mirrors.

### Phase 7 - Surface volition state in the realtime UI

Add a lightweight, dense, read-only inspection surface in the realtime browser UI that shows the
already-computed volition state and per-turn decision for the live session, updating on every
trusted turn. This phase introduces **no** state mutation, no new tool, and no external effect:
it publishes a compact, no-secret capture of state the backend already derives and renders it in
the browser.

#### Decided architecture (from repository context)

Mirror the **realtime turn context inspector** rather than adding a polling HTTP endpoint. The
volition capture is published over a per-session `watch` channel and forwarded on the existing
events socket as a `kind: "volition_state"` message; the browser parses it into a pure reducer
field, derives a view-model with a pure selector, and renders a collapsible panel.

Rationale (and why this supersedes the original "backend endpoint" wording, which predated the
turn context inspector):

- The watch-channel push pattern already enforces **per-session isolation** (the sender lives on
  the per-session `SessionRuntime`) and **no-secret / no-payload contents** (the capture is a
  compact `VolitionStateInspection` plus a decision summary — never provider payloads,
  `conversation.item.create` / `response.create` bodies, or instruction text), and is already
  covered by late-subscriber / stale-session tests we can mirror.
- It gives **live updates during the spoken conversation**, which is exactly what the human test
  requires ("inspect the panel during a live conversation"); an HTTP GET would require polling and
  would not reflect per-turn decisions as they happen.
- It avoids duplicating session-isolation logic in a new route and keeps the UI state derived from
  one socket stream, consistent with the existing `turn_context` / `sideband_status` handling.

The original phase verification spoke of "backend endpoint tests"; those are realized here as
capture-builder tests, watch-channel late-subscriber tests, events-socket forwarding /
session-isolation tests, and a backend integration test that cross-links the published capture to
the diagnostics JSONL — same intent, aligned with the house pattern.

The panel is a collapsible `<details>` block in the existing diagnostics aside (next to "Last turn
context"), not a new tab framework — consistent with the current single-view UI and the
"dense operational UI, no marketing copy" requirement.

**Every trusted turn must update the panel — including no-selection turns.** On a no-selection
turn there is no winner and no `VolitionContextInjected` / `RealtimeBoundedInitiative` diagnostic
(see Phase 4 summary). The capture therefore carries the always-present `VolitionStateInspection`
plus an **optional** decision summary that is absent on no-selection turns; the panel renders an
explicit "no volition decision this turn" state in that case rather than fabricating a winner.

#### Integration map (where this hooks in)

- Capture precedent: `crates/qsf_realtime_server/src/realtime/turn_context.rs`
  (`TurnContextCapture`, `build_turn_context_capture`, no-secret + RFC3339 tests).
- Per-session watch channel precedent: `crates/qsf_realtime_server/src/state.rs`
  (`turn_context_tx`, `subscribe_turn_context`, `turn_context_sender`,
  `turn_context_watch_holds_value_for_late_subscriber`).
- Publish point: `crates/qsf_realtime_server/src/realtime/sideband.rs`
  (`inject_trusted_turn_context_and_response` and the shared
  `send_response_create_and_capture` it calls at **both** branch exits — the selection-turn
  `if let Some(turn_packet)` branch and the no-selection fallthrough — which is where
  `turn_context_tx.send_replace(...)` already runs and where all decision values are in scope).
- Events-socket forward precedent: `crates/qsf_realtime_server/src/realtime/routes.rs`
  (`subscribe_turn_context`, `push_turn_context`, the `select!` change-forward, late-subscriber
  push).
- Reusable decision values (selection turns): `VolitionContextInjectionTrace` /
  `VolitionArbitrationSummary` / `VolitionTurnPacketSummary` / `VolitionModeBiasOutcome` (selected
  goal ids/titles/summaries, omitted/suppressed counts, arbitration winner id/title/summary +
  `winner_effective_tier` / `winner_biased_tier`, `protected_tier_active`, `mode_bias_outcomes`,
  shaping intensity, `initiative_line`) in `volition_injection.rs`;
  `RealtimeBoundedInitiativeTrace` (`winning_goal_id`, `initiative_output`, `surfaced`,
  `suppression_reason`, `rendered_line_present`, `response_create_event_ref`) in
  `volition_initiative.rs`; compact `VolitionStateInspection` from `build_state_inspection` in
  `qsf_volition/src/inspection.rs` (always present, both turn kinds).
- UI precedent: `crates/qsf_realtime_server/ui/src/realtime.ts`
  (`parseTurnContextMessage`, `TurnContextCapture`, `latestTurnContext`, the
  `turn_context_captured` action + stale-session guard, `INITIAL_STATE` reset — note
  `latestTurnContext` is cleared only on `session_allocated` and **preserved** on `stopped`),
  `crates/qsf_realtime_server/ui/src/main.ts` (relay-socket `message` wiring, the
  `turn-context-details` `<details>` panel, `render()`),
  `crates/qsf_realtime_server/ui/src/styles.css` (`.turn-context-*` styles), and
  `crates/qsf_realtime_server/ui/src/realtime.test.ts` (parser + reducer + stale-session +
  preserve-on-stopped tests).

#### Build (incremental, each step independently reviewable)

**Step 7a — Backend capture type (`qsf_realtime_server`, new module
`crates/qsf_realtime_server/src/realtime/volition_inspection_capture.rs`; register in
`realtime/mod.rs`).** Define a serializable `VolitionInspectionCapture` mirroring
`TurnContextCapture` (snake_case wire, no `rename_all`) carrying:

- `qsf_session_id: String`
- `exchange_index: usize`
- `captured_at` (RFC3339; `#[serde(with = "time::serde::rfc3339")]`, using the trusted-turn
  timestamp already in scope)
- `response_create_event_ref: String` (reuse the per-turn `request_hash` ref so the capture
  cross-links to the same turn's `turn_context` capture **always**, and to the same turn's
  `VolitionContextInjected` / `RealtimeBoundedInitiative` records **only when a decision was
  made**)
- `inspection: VolitionStateInspection` (compact: mode, tick, goal groups, candidate counts,
  last initiative summaries) — **always present, both turn kinds**
- `decision: Option<VolitionTurnDecisionSummary>` — `Some` on selection turns, **`None` on
  no-selection turns** (where there is no winner). The `VolitionTurnDecisionSummary` struct
  carries: `winner_goal_id`, `winner_goal_title`, `winner_effective_tier: u8`,
  `winner_biased_tier: u8`, `protected_tier_active: bool`,
  `mode_bias_outcomes: Vec<VolitionModeBiasOutcome>` (compact per-selected-goal tiers/protected,
  reusing the existing type), `selected_goal_ids: Vec<String>`,
  `omitted_or_suppressed_goal_ids: Vec<String>`, `shaping_intensity: ShapingIntensity`,
  `last_initiative_output_kind: Option<String>`, `last_initiative_surfaced: bool`,
  `last_initiative_suppression_reason: Option<VolitionSuppressionReason>`,
  `last_initiative_rendered_line_present: bool`.

Add `build_volition_inspection_capture(...)` assembled entirely from values already computed in
`inject_trusted_turn_context_and_response` (decision built from the arbitration/initiative values
on selection turns; `decision: None` otherwise). Add unit tests mirroring `turn_context.rs`:
`captured_at_serializes_as_string`, a builder-preserves-fields test for both a selection turn
(decision `Some`, tier/protection fields populated) and a no-selection turn (decision `None`,
inspection still present), and a **broadened no-leak test** asserting the serialized capture
contains no `OPENAI_API_KEY`, no `Bearer `, no stable-baseline preamble fragment (e.g. "The
following describes your simulated volition stance"), and no `conversation.item.create` /
`response.create` / provider message-content payload. Pure, deterministic, no provider payloads,
no instructions text.

**Step 7b — Per-session watch channel (`crates/qsf_realtime_server/src/state.rs`).** Add
`volition_inspection_tx: watch::Sender<Option<VolitionInspectionCapture>>` to `SessionRuntime`
alongside `turn_context_tx` (default `watch::channel(None).0`), plus
`subscribe_volition_inspection()` and `volition_inspection_sender()` accessors mirroring
`subscribe_turn_context` / `turn_context_sender`. Add a late-subscriber unit test mirroring
`turn_context_watch_holds_value_for_late_subscriber` (a value sent before subscription is still
observed by a late subscriber).

**Step 7c — Publish on every trusted-turn boundary (`crates/qsf_realtime_server/src/realtime/sideband.rs`).**
Build the `VolitionInspectionCapture` from the already-computed selection / arbitration /
initiative values (decision `Some`) or as a state-only capture (decision `None`) in
`inject_trusted_turn_context_and_response`, and publish it via `send_replace(Some(capture))` on
the session's `volition_inspection_tx` **next to the existing `turn_context_tx.send_replace`** so
it fires on **both** the selection-turn branch and the no-selection fallthrough. Thread the
capture into `send_response_create_and_capture` (next to its `turn_context_tx` argument) so the
single helper publishes both captures together for every trusted turn; do **not** restrict
publishing to the `if let Some(turn_packet)` branch. The tool-loop continuation `response.create`
in `handle_response_done_event` must **not** publish a volition capture (same boundary rule as
`turn_context` and the per-turn volition packet). Default-on (no flag), so every trusted turn
exercises the new path. Mirror the existing test that the capture is not published when the
outbound send fails, if it can be expressed without excessive harness.

**Step 7d — Forward on the events socket (`crates/qsf_realtime_server/src/realtime/routes.rs`).**
Mirror the `turn_context` forwarding: subscribe to `volition_inspection_rx`, add
`push_volition_inspection(socket, capture)` that sends a `{ kind: "volition_state", ... }`
message, push the held value to late subscribers on subscribe, and forward on change in the
`select!`. Preserve the existing "sender dropped (session removed) → stop watching, keep relaying"
behavior. Add a forwarding test that a capture published for the active session is delivered as a
`volition_state` message.

**Step 7e — UI parser + reducer + selector (`crates/qsf_realtime_server/ui/src/realtime.ts`).**
Mirror `parseTurnContextMessage` / `latestTurnContext` / `turn_context_captured`:

- Add a `VolitionInspectionCapture` TS interface (camelCase properties, snake_case wire mapping)
  with `inspection` always present and `decision: VolitionTurnDecisionSummary | null` (including
  the tier/protection fields and the `suppressionReason` discriminant union).
- Add `parseVolitionStateMessage(raw): VolitionInspectionCapture | null` returning the capture for
  `kind: "volition_state"` and `null` otherwise, with full per-field type guards (string / number
  / array / nested object), handling `decision === null` for no-selection turns, like the
  turn-context parser.
- Add `latestVolitionState: VolitionInspectionCapture | null` to `ConversationState` and
  `INITIAL_STATE`. **Reset semantics must actually mirror `latestTurnContext`:** clear it on
  `session_allocated` only, and **preserve it on `stopped`** (so diagnostics stay visible after
  Stop), relying on the stale-session guard to ignore late messages. (The current reducer and its
  test `preserves latestTurnContext on stopped so diagnostics remain visible` confirm this is the
  inspector's real behavior; the earlier "reset on stopped" wording was incorrect.)
- Add `{ type: "volition_state_captured"; capture }` to `ConversationAction` with the **same
  stale-session guard** as `turn_context_captured` (ignore captures whose `qsfSessionId` differs
  from the active `sessionId`).
- Add a pure view-model selector `selectVolitionPanelModel(state)` that derives the dense display
  rows. On a decision-present capture: mode, tick, winner goal id+title, **winner effective/biased
  tier and protected status**, compact mode-bias outcomes (id + tier + protected) for selected
  goals, selected goal ids, omitted/suppressed ids, pending + accepted candidate counts, last
  initiative kind + `surfaced` / `suppressionReason` / `renderedLinePresent`, and the
  `responseCreateEventRef` trace id. On a `decision === null` capture: render the state snapshot
  (mode, tick, goal groups, candidate counts) plus an explicit "no volition decision this turn"
  marker and no fabricated winner. Show a stable "no volition state yet" model when
  `latestVolitionState` is `null`.

**Step 7f — UI wiring + panel (`crates/qsf_realtime_server/ui/src/main.ts` + `styles.css` +
`index.html` if needed).** Add a collapsible `<details>` "Volition state" panel in the diagnostics
aside next to "Last turn context"; add a `data-role="volition-state-body"` ref in `collectRefs`;
wire the relay-socket `message` listener to call `parseVolitionStateMessage` → dispatch
`volition_state_captured`; render from `selectVolitionPanelModel` inside the existing `render()`.
Dense operational rows (label/value lines), including the protected-tier display so an operator can
read protected status at a glance, and the "no volition decision this turn" state. No marketing
copy, no raw provider payloads, no secrets, no instruction text. Reuse the existing panel /
`<details>` styling; add minimal `.volition-state-*` rules only if needed and confirm no
overlapping text at desktop/mobile widths.

**Step 7g — Experiment scaffold + docs.** Create
`docs/Experiments/Experiment.RealtimeVolitionInspectionUi.md` with the trace completeness contract
below (define it before implementation). Update:

- `docs/Architecture/Architecture.RealtimeSessionServer.md` — the volition inspection capture, the
  per-session watch channel, and the `volition_state` events-socket message as part of the realtime
  server's live inspection surfaces.
- `docs/Architecture/Architecture.StateAndObservability.md` — the `volition_state` capture fields
  (state snapshot + optional decision summary incl. tiers/protection) and its conditional cross-link
  to volition diagnostics via `response_create_event_ref`.
- `docs/Experiments/Experiment.Backlog.md` — promote `RealtimeVolitionInspectionUi` to running/
  completed as it lands.
- `docs/DecisionLog.md` — the "realtime volition inspection UI" decision and the
  push-over-events-socket-vs-HTTP-endpoint resolution (mirroring the turn context inspector).

Do **not** add UI documentation or tests asserting "tick resets each session" until the Phase 6
snapshot-restore-vs-reviewed-seed-only reconciliation item is closed (see the Phase 6 summary).

#### Verify

- **Backend (`cargo test`):** capture builder tests for a selection turn and a no-selection turn
  (decision `Some`/`None`, tier/protection fields populated when present), `captured_at`
  serializes as a string, fields preserved; broadened no-leak test (no `OPENAI_API_KEY` /
  `Bearer ` / stable-baseline preamble fragment / `conversation.item.create` / `response.create`
  / provider message payload); watch late-subscriber test; events-socket forwarding test that a
  `volition_state` message is delivered for the active session.
- **Backend integration / cross-link (`cargo test`):** using the existing trusted-turn harness,
  run a trusted text/audio turn through `inject_trusted_turn_context_and_response`, receive the
  published `VolitionInspectionCapture` from the watch channel (or socket forward), parse the
  per-session diagnostics JSONL, and assert that on a **selection turn** the capture's
  `response_create_event_ref` matches the `VolitionContextInjected` and (when present)
  `RealtimeBoundedInitiative` records for the same exchange; and that on a **no-selection turn**
  the capture is still published with `decision: None` and **no** corresponding
  `VolitionContextInjected` record exists, while the `turn_context` capture and `response.create`
  still occur.
- **Session isolation:** a capture published for session A is not delivered to session B's socket;
  and the UI reducer drops a `volition_state_captured` action whose `qsfSessionId` differs from the
  active session (stale-session guard).
- **UI (`npm run check` then `npm run fmt` in `crates/qsf_realtime_server/ui/`):**
  - reducer test: `volition_state_captured` updates `latestVolitionState`, is ignored for a
    non-active session id, is **preserved on `stopped`**, and is cleared by `session_allocated`;
  - parser test: `parseVolitionStateMessage` accepts a well-formed `kind: "volition_state"`
    message (decision present and decision `null`) and rejects malformed / other-kind / non-JSON
    input;
  - selector test: `selectVolitionPanelModel` derives the expected dense rows from a
    decision-present capture, **including the winner tiers and protected status proving a
    protected winner renders as protected/dominant without relying on hard-coded goal ids**;
    derives the "no volition decision this turn" rows from a `decision: null` capture; and returns
    the null-state fallback when `latestVolitionState` is `null`.
- **No-secret / no-payload UI assertion:** a serialized capture / rendered panel model contains no
  `OPENAI_API_KEY` / `Bearer `, no stable-baseline preamble fragment, and no raw provider payload
  (`conversation.item.create` / `response.create` / message-content fields).
- **Browser smoke check:** open the panel at desktop and mobile widths; confirm no overlapping
  text and that the panel collapses/expands.
- `cargo clippy --all-targets -- -D warnings`, then `cargo fmt`.

#### Human test

- Run `scripts/qsf.ps1 realtime`, start a live conversation, and open the "Volition state" panel.
  Confirm it updates on **every** trusted turn — including a turn whose user input matches no goal
  keywords, where the panel shows the state snapshot plus "no volition decision this turn" rather
  than a stale or fabricated winner. On selection turns, confirm the winner / selected / suppressed
  goals and the last bounded-initiative outcome (surfaced vs suppression reason) match what was
  spoken, and that inspecting it does not interrupt the spoken interaction.
- Confirm the panel renders the protected tier-2/tier-3 goals truthfully — present in the state
  snapshot and, when one wins, shown with its effective/biased tier and protected status — so an
  operator can visually confirm the protected-tier invariant live.
- After pressing Stop, confirm the last volition panel remains visible (matching the "Last turn
  context" inspector's preserve-on-stop behavior).

#### Trace completeness contract

The `volition_state` capture must contain:

- `qsf_session_id`
- `exchange_index`
- `captured_at` (RFC3339 string)
- `response_create_event_ref` — **always** resolves to the same turn's `turn_context` capture
  (both turn kinds); on **selection turns** it additionally resolves to the same turn's
  `VolitionContextInjected` and, when an initiative was produced, `RealtimeBoundedInitiative`
  diagnostics records. On **no-selection turns** there are intentionally no such volition
  diagnostic records to resolve to.
- the compact `VolitionStateInspection` (mode, tick, goal groups, candidate counts, last initiative
  summaries) — **always present**
- `decision: Option<VolitionTurnDecisionSummary>` — present on selection turns, absent on
  no-selection turns; when present it carries `winner_goal_id`, `winner_goal_title`,
  `winner_effective_tier`, `winner_biased_tier`, `protected_tier_active`, `mode_bias_outcomes`,
  `selected_goal_ids`, `omitted_or_suppressed_goal_ids`, `shaping_intensity`,
  `last_initiative_output_kind`, `last_initiative_surfaced`, `last_initiative_suppression_reason`,
  `last_initiative_rendered_line_present`

The artifact boundary is the events-socket message stream (parsed in UI tests) plus the per-session
diagnostics JSONL, cross-linked by `response_create_event_ref`. Automated verification (the backend
integration test above) parses a captured `volition_state` message and asserts: on a selection turn
its `response_create_event_ref` matches the corresponding `VolitionContextInjected` (and present
`RealtimeBoundedInitiative`) diagnostics records for the same exchange; on a no-selection turn the
capture is published with `decision: None` and no such volition diagnostic exists; and in all cases
the capture carries no secret and no raw provider payload.

#### Open questions (product UX; not resolvable from repository context)

- **No-selection display (decided default).** No-selection trusted turns publish a capture with
  `decision: None`; the panel shows the state snapshot plus an explicit "no volition decision this
  turn" marker and no fabricated winner. (Default chosen so the panel still updates on every
  trusted turn per the human test; revisit only if operators find the marker noisy.)
- **Preserve panel after Stop (decided default).** Mirror the "Last turn context" inspector:
  preserve `latestVolitionState` on `stopped`, clear it only on `session_allocated`, and rely on
  the stale-session guard for late messages.
- **Scrollback (open).** Should the panel show only the **latest** trusted-turn volition snapshot
  (matching the "Last turn context" inspector), or keep a short **scrollback** of the last N turns'
  volition decisions so an operator can see recurring / repeatedly-suppressed goals live? Default if
  unanswered: latest-only, mirroring the turn context inspector — a bounded scrollback can be a
  follow-up once the latest-only surface proves useful in human testing. (This affects only
  `ConversationState` shape and the panel render; the capture and backend wiring are identical
  either way.)

## Experiments To Create

Create focused experiment scaffolds as phases begin. `Experiment.RealtimeVolitionStateSeed`,
`Experiment.RealtimeVolitionReadOnlyInspection`, `Experiment.RealtimeVolitionContextInjection`,
`Experiment.RealtimeVolitionBoundedInitiative`, and `Experiment.RealtimeVolitionContinuity` exist
and are implemented; `Experiment.RealtimeVolitionContextInjection` carries the realized
injected-text contract. `Experiment.RealtimeVolitionInspectionUi` is the next scaffold to create
(Phase 7).

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
- The realtime UI inspection surface is read-only: it never mutates volition state and never
  introduces a new tool or external effect.
- `OPENAI_API_KEY` and other secrets never appear in volition traces, tool outputs,
  UI payloads, diagnostics, reports, or the realtime UI inspection capture; the inspection capture
  additionally carries no provider request/response payloads and no instruction text.

## Documentation To Update

Per [`ProjectWorkflow.md`](../ProjectFrame/ProjectWorkflow.md), update documents when
the work starts or changes durable behavior:

- This plan as phases start, complete, or change shape.
- The per-phase `Experiment.*.md` files under `docs/Experiments/`.
- `docs/Experiments/Experiment.Backlog.md` when each realtime-volition experiment is
  promoted from planned to running/completed.
- `docs/Architecture/Architecture.RealtimeSessionServer.md` when volition state or
  tools (including bounded initiative, persistence, and the UI inspection surface) become
  part of the realtime server.
- `docs/Architecture/Architecture.ToolSystem.md` when live volition tools are added.
- `docs/Architecture/Architecture.ContextManagement.md` when volition context packets or
  context-retrieval initiative hints influence live response context.
- `docs/Architecture/Architecture.VolitionSystem.md` for the extracted `qsf_volition`
  crate, the bounded-initiative behavior, and the volition consolidation/extraction behavior.
- `docs/Architecture/Architecture.StateAndObservability.md` when volition traces
  (including `realtime_bounded_initiative_trace`), persistence artifacts, or the
  `volition_state` UI capture are added.
- `docs/ProjectFrame/ProjectVision.md` if the final project target or realtime
  consciousness-simulation framing changes.
- `docs/DecisionLog.md` for durable commitments: crate boundary, live tool scope
  expansion, behavioral influence boundary, the realtime bounded-initiative boundary,
  persistence/consolidation boundary, the reviewed-acceptance gate, the realtime volition
  inspection UI surface, or any change to protected-tier safety.

This plan is ephemeral. Durable documents should refer to named behaviors such as
"realtime volition inspection", "volition context injection", "realtime bounded
initiative", "realtime volition continuity", or "realtime volition inspection UI", not to
this plan's phase numbers.

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