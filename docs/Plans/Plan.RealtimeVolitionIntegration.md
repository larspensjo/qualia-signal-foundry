# Plan: Realtime Volition Integration

## Status

In progress. Phases 1-4 are complete. **Phase 5 is the next implementation slice**:
trace-backed bounded internal initiative outputs in the live loop. Its building blocks
already exist — `qsf_volition::execute_initiative`, the `VolitionEvent::InitiativeExecuted`
reducer event, the shared per-turn injection helper `inject_trusted_turn_context_and_response`,
and the arbitration winner that carries an `InitiativeProposal` — so Phase 5 is mostly a
realtime-side wiring + tracing slice, not new domain modeling. Read the compacted
"Completed Phases 1-4" summary below for the constraints Phase 5 must respect.

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

As of Phase 4, the pure domain lives in `qsf_volition`, each session carries
`VolitionRuntimeState`, two read-only volition tools are registered, and volition now
influences the live spoken response through layered, trace-backed context injection. The
remaining gap Phase 5 closes is that volition does not yet produce bounded internal
initiative outputs (reflection, open-thread surfacing, experiment proposals, context-retrieval
hints) inside the live loop.

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
(layered volition context injection) is now implemented and live; bounded internal
initiative is the remaining behavioral slice and is added only because the live system
can already explain the selected goals, omitted/suppressed goals, arbitration result, and
shaping intensity.

Behavioral influence is also gated on the default realtime seed already including
protected tier-2 explicit-user-intent (`honor-explicit-user-request`) and tier-3
current-task-completion (`complete-current-task`) tensions/goals, with tests proving
they cannot be displaced by curiosity or exploration goals under any mode bias. That
gate is satisfied by Phase 2 and must remain green for Phase 5 and beyond — Phase 5
derives every initiative from the arbitration winner, so the same invariant
automatically bounds which goals can produce initiative.

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
| 5 | Add trace-backed bounded initiative outputs to the live loop | Yes | Yes | Next | `Experiment.RealtimeVolitionBoundedInitiative` |
| 6 | Persist, inspect, and consolidate realtime volition state | Yes | Yes | Not started | `Experiment.RealtimeVolitionContinuity` |
| 7 | Surface volition state in the realtime UI | Yes | Yes | Not started | `Experiment.RealtimeVolitionInspectionUi` |

## Phase Details

### Completed Phases 1-4 (summary)

Phases 1-4 are implemented and validated. The durable outcomes and constraints they
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
  `stable_baseline_hash`), and inspection (`build_state_inspection`). It has no dependency
  on `qsf_app`, providers, HTTP, tokio, or UI. **Constraint for later phases:** keep this
  boundary; route all state change through `VolitionEvent` + `apply`; do not pull realtime
  or context-assembly types into `qsf_volition`.
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
- **Layered context injection (Phase 4).** Volition now shapes the live response through
  layered, trace-backed context built in the shared per-turn helper
  `inject_trusted_turn_context_and_response` in `sideband.rs` (used by **both** the typed
  path and the voice `input_audio_transcription.completed` arm), with pure builders in
  `crates/qsf_realtime_server/src/realtime/volition_injection.rs`. Durable facts Phase 5
  builds on:
  - **Stable baseline** (`build_stable_baseline_instructions` → `render_volition_stance`,
    wrapped with the realtime/project trust-boundary preamble) is composed into the base
    instructions used by both the initial and every per-turn `session.update` (and therefore
    `response.create`); the *content* never changes per turn, verified by a stable
    `stable_baseline_hash`.
  - **Per-turn dynamic packet** (`build_volition_turn_context_packet`) is sent as a single
    system `conversation.item.create` **after** the optional memory item and **before** the
    initial `response.create`. It is computed from the post-mapping snapshot
    (`VolitionStateSnapshot { state, fixture }`, defined in `tools.rs`) using
    `select_goals_ranked` → `detect_opportunities` → `arbitrate_with_mode` →
    `choose_shaping_intensity`. It is computed and injected **independently of memory
    retrieval** (memory packet is `None` on turns with no retrieved memories).
  - **Arbitration + dial guarantees.** `arbitrate_with_mode` returns `None` for empty
    selection (callers short-circuit). The arbitration winner (`ModeArbitrationResult.winner`,
    a `GoalSelection` carrying `.goal` and `.initiative: InitiativeProposal`) respects
    protected tiers and mode bias. `choose_shaping_intensity` clamps to ≤ `Low` when the
    winner is protected (`winner_bias.effective_tier <= PROTECTED_TIER_FLOOR`).
  - **Tracing.** `DiagnosticRecord::VolitionContextInjected { qsf_session_id, exchange_index,
    recorded_at, trace }` carries `VolitionContextInjectionTrace`. The trace's
    `response_create_event_ref` is the per-turn `hash_request_sequence(turn_request_values)`
    value — reuse this same reference style for new per-turn traces rather than minting new
    outbound event ids.
  - **Tool-loop boundary.** The tool-loop continuation `response.create` in
    `handle_response_done_event` must **not** receive a fresh per-turn volition packet; the
    same rule applies to any new per-turn behavior.
  - Validated by `Experiment.RealtimeVolitionContextInjection`, whose "Injected Text Contract"
    pins the rendered baseline and per-turn packet text asserted verbatim in tests.

The `volition_tool_trace` (Phase 3) and `volition_context_injection_trace` (Phase 4)
contracts remain the model for trace fields and parsing-based verification: a record carries
`qsf_session_id`, an exchange/tick reference, the selection/arbitration/mode-bias outcomes, a
content hash, and a resolvable reference to the outbound `response.create`.

### Phase 5 - Add trace-backed bounded initiative outputs to the live loop

Let the arbitration winner produce a bounded **internal** `InitiativeOutput` on each trusted
turn, surface it gently to the live model through the existing per-turn volition channel,
record a `realtime_bounded_initiative_trace`, and — for context-retrieval initiatives only —
feed query-term hints into the **next** existing memory/context injection pass. No external
write-capable effect is introduced: nothing writes files, creates plans, runs commands, or
triggers external tools.

Initiative is derived from the arbitration winner that Phase 4 already computes, so it
inherits the protected-tier and mode-bias guarantees for free: a curiosity/exploration goal
can only produce initiative if it actually wins arbitration, which the protected-tier
invariant prevents whenever user-intent/task-completion goals are present.

#### Integration map (where this hooks in)

All references are in `crates/qsf_realtime_server/src/realtime/` unless noted.

- **Shared per-turn helper** `inject_trusted_turn_context_and_response` (`sideband.rs`)
  already computes `ranked`, `opportunities`, `arbitration` (`arbitrate_with_mode`), and
  `intensity`, then builds and sends the volition turn packet and writes
  `VolitionContextInjected`. Phase 5 extends this single helper, which covers both the typed
  and voice paths. The retrieval call for the turn is `retrieve_session_memories(state,
  qsf_session_id, transcript, RetrievalStrategy::AssociationWeighted, ...)` and the injection
  input is `MemoryInjectionRequest` (`injection.rs`).
- **Pure domain (reuse, do not re-implement):**
  `qsf_volition::execute_initiative(&InitiativeProposal, &Goal) -> InitiativeOutput` is pure
  and deterministic. `InitiativeOutput` variants: `ReflectionRequested { proposed_question }`,
  `ContextRetrievalRequested { query_terms }`, `ExperimentProposed { hypothesis, scope }`,
  `OpenThreadSurfaced { thread_summary }`. `VolitionEvent::InitiativeExecuted { goal_id,
  effect, output, rationale, tick }` already exists; `apply` sets the goal `Active` and stores
  `last_initiative_output` on its dynamic state. The only mutation path is
  `guard.volition.apply_events(...)`.
- **Diagnostics:** add a `DiagnosticRecord::RealtimeBoundedInitiative { .. }` variant
  (`diagnostics.rs`) carrying the new trace, written alongside the existing
  `VolitionContextInjected` record for the same trusted turn.
- **Per-connection sideband state:** `SidebandRuntimeState` (`sideband.rs`) is the natural
  place to stash context-retrieval hints for the next turn.

#### Build (incremental, each step independently reviewable)

**Step 5a — Pure initiative trace + model-facing rendering (new module
`volition_initiative.rs`).**
- Add `RealtimeBoundedInitiativeTrace` with the trace-contract fields below, including a
  `bounded_or_external_output` shape whose `external_effect_executed` is `false` by
  construction, and `context_retrieval_hint_terms: Option<Vec<String>>` populated only for
  `ContextRetrievalRequested`.
- Add a pure `render_initiative_line(output: &InitiativeOutput, intensity: ShapingIntensity)
  -> Option<String>`: a single bounded, model-facing line for `ReflectionRequested`,
  `OpenThreadSurfaced`, and `ExperimentProposed`; `None` for `ContextRetrievalRequested`
  (hint-only, never a model instruction) and `None` when `intensity == ShapingIntensity::None`.
  The text must not claim real desire/consciousness and must not authorize any external
  action — mirror the existing turn-packet guidance language.
- Keep this function a pure map from `output` + `intensity`. The context-dependent surfacing
  suppression — the protected-winner genuine-opportunity gate and the anti-nag alternation rule —
  is applied by the shared helper in Step 5c, not here, because it depends on the arbitration
  winner and the opportunity signals rather than on the rendered output alone.
- Add a pure `build_realtime_bounded_initiative_trace(...)` constructor analogous to
  `build_volition_context_injection_trace`.
- Unit tests: each renderable variant produces a bounded-length line; `ContextRetrievalRequested`
  and `None` intensity render `None`; every trace has `external_effect_executed == false`;
  rendering and trace construction are deterministic.

**Step 5b — Stash for context-retrieval hints (`SidebandRuntimeState`).**
- Add `pending_context_retrieval_hints: Vec<String>` to `SidebandRuntimeState`. It must
  **survive across the turn boundary** (it is consumed on the next turn), so it is *not*
  cleared in `clear_in_flight_response_state`; it is cleared explicitly after consumption
  (Step 5e). Also add the anti-nag marker `previous_turn_surfaced_goal_id: Option<String>` used
  in Step 5c. Like the hint stash, it must **survive across the turn boundary** and so is *not*
  cleared in `clear_in_flight_response_state`; Step 5c sets and clears it explicitly. (Do not use
  a `last_initiative_goal_id` that is updated to the winner every turn: that suppresses a repeated
  winner forever after its first repeat — see the resolved anti-nag decision below.)

**Step 5c — Derive and apply the initiative (shared helper).**
- After `arbitration` and `intensity` are computed and `arbitration` is `Some`, take
  `arbitration.winner` and call `execute_initiative(&winner.initiative, &winner.goal)`.
- Re-acquire the session guard, capture `state_snapshot_before` (a compact
  `build_state_inspection` over the live volition state — see the resolved snapshot-granularity
  decision below),
  apply `guard.volition.apply_events(vec![VolitionEvent::InitiativeExecuted { goal_id:
  winner.goal.id.clone(), effect: winner.initiative.effect, output: output.clone(), rationale:
  winner.initiative.rationale.clone(), tick: guard.volition.state.tick }])`, capture
  `state_snapshot_after`, then drop the guard. Keep the computation pure and the mutation
  confined to this single `apply_events` call; do not run initiative on the tool-loop
  continuation `response.create` in `handle_response_done_event`.
- Surfacing gate (always record to diagnostics; the gate only decides whether the model-facing
  line in Step 5d is emitted). Suppress the line when **any** of these hold:
  1. `intensity == ShapingIntensity::None`.
  2. **Protected-winner genuine-opportunity gate.** The winner is protected
     (`arbitration.winner_bias.effective_tier <= PROTECTED_TIER_FLOOR`) **and** the turn carries
     no genuine opportunity signal beyond the winner's own topic self-match. A genuine signal is
     any `opportunities` entry that is *not* an `OpenGoalTopicMatch` grounded on the winner's own
     goal id — i.e. an `ExpressedUncertainty`, an `IntroducedContradiction`, or an
     `OpenGoalTopicMatch` whose `grounding_ref` is a `GoalId` other than `winner.goal.id`. This is
     the durable resolution of the protected-winner surfacing policy: a protected winner does not
     reflect on ordinary direct requests, only when the conversation itself invites it.
  3. **Anti-nag alternation.** `winner.goal.id` equals `runtime_state.previous_turn_surfaced_goal_id`
     (the immediately preceding trusted turn surfaced this same goal).
- After deciding: set `runtime_state.previous_turn_surfaced_goal_id = Some(winner.goal.id.clone())`
  **only when the line is actually surfaced**, and set it to `None` on any non-surfaced turn. This
  yields correct alternation for a repeated winner (A/A/A → surface, suppress, surface) instead of
  permanent suppression. Document the rule and the A/A/A expectation in the experiment scaffold.

**Step 5d — Surface the bounded line into the existing per-turn volition channel.**
- When `render_initiative_line(...)` is `Some` and not suppressed, include it as an additional
  bounded section of the **existing** volition turn packet text rather than emitting a second
  system item — this keeps the per-turn token budget and the Phase 4 ordering (memory item →
  volition item → `response.create`) intact. Extend `build_volition_turn_context_packet` (or a
  thin wrapper in `volition_injection.rs`) to accept an `Option<&str>` initiative line and a
  matching `Option` field in `VolitionTurnPacketSummary` so the rendered line is covered by the
  packet hash and token estimate.
  - Decision (made from repo context, not a blocking question): ride inside the existing
    single per-turn system item. Rationale: one system item keeps token accounting and
    response ordering centralized in the Phase 4 path and avoids a second
    `conversation.item.create` whose placement relative to memory/volition items would need a
    new contract. Recorded alternative: a separate bounded system item after the volition
    packet — revisit only if initiative text must be independently togglable from the stance.
- For `ContextRetrievalRequested { query_terms }`, do **not** render to the model; push
  `query_terms` onto `runtime_state.pending_context_retrieval_hints` for the next turn.

**Step 5e — Consume context-retrieval hints on the next turn.**
- At the top of the shared helper, if `runtime_state.pending_context_retrieval_hints` is
  non-empty, augment the retrieval query used by `retrieve_session_memories` (e.g.
  `format!("{transcript} {}", hints.join(" "))`), set
  `hint_consumed_by_next_memory_injection = true` in that turn's bounded-initiative trace
  (referencing the prior `exchange_index`), then clear the stash. When empty, retrieval is
  unchanged. The hint feeds the existing memory/context injection path only — it is never
  promoted to an immediate tool call.

**Step 5f — Diagnostics + trace.**
- Add `DiagnosticRecord::RealtimeBoundedInitiative { qsf_session_id, exchange_index,
  recorded_at, trace }` and write it for the trusted user turn (next to the existing
  `VolitionContextInjected` write). Reuse the same `response_create_event_ref` value
  (`hash_request_sequence` over the turn request sequence) so the initiative trace links to the
  same `response.create`. Update any exhaustive `match` over `DiagnosticRecord` (e.g. the
  diagnostics summary/verification code in `sideband.rs`) to handle the new variant.

**Step 5g — Default-on, experiment scaffold, and docs.**
- Default the behavior on (no flag), consistent with Phase 4. Surfacing is bounded by the Step 5c
  gate, **not** by the shaping dial alone: the dial returns `Low` for a protected winner whenever
  any opportunity exists (and the protected goals' common keywords make that nearly every turn, via
  their own `OpenGoalTopicMatch`), so the dial by itself would surface a reflection on ordinary
  direct requests. The protected-winner genuine-opportunity gate plus anti-nag alternation are what
  keep protected/direct turns quiet so curiosity cannot derail. If a config switch is added, it must
  default to exercising the new path (per AGENTS.md).
- Create `docs/Experiments/Experiment.RealtimeVolitionBoundedInitiative.md` with the trace
  completeness contract below and the **exact rendered initiative-line text per variant**,
  asserted verbatim in tests so the surfaced-initiative contract is explicit, not implicit.
- Update the documentation listed under "Documentation To Update".

#### Verify

- `execute_initiative` is deterministic and side-effect-free (covered by existing
  `qsf_volition` tests; keep them green). The `InitiativeExecuted` `apply_events` call is the
  only state mutation introduced; reducer purity is preserved.
- Step 5a render/trace unit tests pass, including the bounded-length and
  `external_effect_executed == false` assertions and the `ContextRetrievalRequested`/`None`
  → `None` rendering.
- A curiosity/exploration goal never produces a **surfaced** initiative when a protected
  user-intent/task-completion goal is present, because it never wins arbitration — assert
  under `Neutral`, `Focused`, and `Exploratory`.
- Protected-winner genuine-opportunity gate: a protected winner on an ordinary direct request
  (only its own `OpenGoalTopicMatch`, no other signal) is recorded but **not surfaced**; a
  protected winner on a turn that also carries `ExpressedUncertainty`/`IntroducedContradiction`
  (or another goal's `OpenGoalTopicMatch`) **is** surfaced. The second case exercises the
  default-on surfacing path (per AGENTS.md).
- Context-retrieval hint round-trip: a `ContextRetrievalRequested` on turn N augments turn
  N+1 retrieval and the stash is cleared; turns with no pending hints retrieve unchanged.
- Tool-loop continuation `response.create` produces no initiative and no new
  `RealtimeBoundedInitiative` record.
- Anti-nag: with the same goal winning and surfaceable on three consecutive trusted turns
  (A/A/A), turns 1 and 3 surface while turn 2 is suppressed — proving alternation rather than
  permanent suppression. All three are still recorded to diagnostics.
- Latency: initiative derivation + `apply_events` adds bounded overhead, measured on the same
  `input_audio_transcription.completed -> response.create` boundary used by the Phase 4
  selection measurement.
- Artifact-parsing verification (diagnostics JSONL): every `RealtimeBoundedInitiative` record
  has a prior arbitration winner for the same `exchange_index`, `external_effect_executed`
  is `false`, and any `context_retrieval_hint_terms` are matched by a later turn's
  `hint_consumed_by_next_memory_injection == true`.
- `cargo test`, then `cargo clippy --all-targets -- -D warnings`, then `cargo fmt`.

#### Human test

- In live voice, ask an open-ended research question and confirm the system can surface a
  relevant internal initiative (reflection / open thread / experiment proposal) without
  taking action.
- Confirm it does not repeatedly nag or derail the conversation, and that a direct user task
  keeps protected tiers dominant (no curiosity-driven initiative surfaces).
- Confirm spoken framing still distinguishes simulated internal state from a claim of real
  subjective desire.

#### Trace completeness contract

`realtime_bounded_initiative_trace` must contain:

- `qsf_session_id`
- `exchange_index`
- `winning_goal_id`
- `initiative_proposal`
- `allowed_effect`
- `initiative_output`
- `bounded_or_external_output` with explicit `external_effect_executed: false`
- `context_retrieval_hint_terms` when the output is `ContextRetrievalRequested`
- `hint_consumed_by_next_memory_injection` recorded on the **consuming** turn when those terms
  are passed to the existing sideband memory/context injection path (a forward link by
  `exchange_index`, since the JSONL is append-only)
- `rationale`
- `state_snapshot_before`
- `state_snapshot_after`
- `response_create_event_ref` — the per-turn `hash_request_sequence` value, reused from the
  Phase 4 injection trace so both records for the turn carry the same reference
- `artifact_or_record_reference`

The artifact boundary is the diagnostics record stream. Automated verification parses the
persisted records (not in-memory structs) and asserts that every initiative output has a
prior arbitration winner and that no external effect was executed.

#### Resolved decisions (were open questions)

Confirmed before implementation; see the 2026-06-30 decision-log entry "Realtime
bounded-initiative surfacing, anti-nag cadence, and trace granularity".

- **Protected-winner surfacing policy.** A protected-tier winner surfaces a line only when the
  turn carries a genuine opportunity signal beyond the winner's own topic self-match (Step 5c
  gate). Full suppression on every direct request was rejected because `ProjectVision.md`
  prioritizes presence and appropriate reflection over task completion; surfacing on every
  protected turn was rejected as a per-turn tic. The gate is a realtime-side rule layered on the
  shared shaping dial, so the Phase 4 context-injection intensity behavior is unchanged.
- **Anti-nag cadence.** Consecutive-turn alternation: the same goal is not surfaced on two
  adjacent trusted turns, tracked by `previous_turn_surfaced_goal_id` set only on surfaced turns
  and cleared on non-surfaced turns (Step 5b/5c). A longer tick-based cooldown is the documented
  upgrade path if live voice testing shows alternation still nags; if added, its state lives in
  `SidebandRuntimeState`, not the pure domain (do not conflate with the arbitration
  `cooldown_until_tick`).
- **`state_snapshot_before/after` granularity.** The compact `build_state_inspection` projection,
  which already carries the dynamic fields `InitiativeExecuted` mutates (goal status grouping,
  `last_activated_tick`, `last_initiative_summaries`, `tick`). The parsed verification asserts the
  **winning goal's** transition (status `Accepted -> Active`, a new `last_initiative_summaries`
  entry for `winning_goal_id`, tick advance) rather than diffing a full `VolitionState` clone.
- **Initiative line carrier.** Confirmed as Step 5d's default: ride inside the existing single
  per-turn volition system item. A separately addressable initiative item is revisited only if
  initiative text must be independently togglable from the stance.

#### Open questions

- Should initiative derivation stay purely rule-based (winner → `execute_initiative`), or may a
  later slice add a model-assisted proposer that emits the same `InitiativeOutput` shape through
  the event path? Default: rule-based only in this slice.

### Phase 6 - Persist, inspect, and consolidate realtime volition state

Decide and implement what parts of realtime volition survive across sessions.

Build:

- Add a design note before implementation if persistence shape is non-trivial.
- Choose a persistence boundary:
  - full `VolitionState` snapshot in realtime continuity,
  - compact derived memory records,
  - or diagnostics-only snapshots plus sleep/consolidation extraction.
- If adding durable schema fields, use `#[serde(default)]`, update golden tests, and
  preserve legacy artifact loading.
- Add extraction/consolidation logic so sleep-like passes can review:
  - recurring selected goals,
  - often-blocked goals,
  - accepted/rejected candidates,
  - mode changes,
  - bounded initiatives proposed but not acted on.
- Keep manual review for durable goal/candidate changes that could steer future
  behavior.

Verify:

- Persisted state reloads deterministically.
- Legacy realtime continuity artifacts still load.
- Corrupt/missing volition persistence degrades gracefully to the default fixture.
- Sleep/consolidation output cites volition artifact references rather than free-form
  claims.
- Human-reviewed promotion is required before cross-session accepted goal changes
  become durable behavior.

Human test:

- Run two realtime sessions with the stable default session id and confirm continuity
  is useful but not sticky in a way that traps the system in stale goals.

Open questions:

- Should mode persist across sessions or reset to `Neutral`? Default assumption:
  reset to `Neutral` unless a reviewed durable memory explicitly says otherwise.
- Are accepted goal candidates durable memories, live state, or both? Default
  assumption: reviewed memory for continuity, live state for per-session mechanics.

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
`Experiment.RealtimeVolitionReadOnlyInspection`, and
`Experiment.RealtimeVolitionContextInjection` exist and are implemented; the last carries the
realized injected-text contract. `Experiment.RealtimeVolitionBoundedInitiative` is the next
scaffold to create (Phase 5).

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
  tools (including bounded initiative) become part of the realtime server.
- `docs/Architecture/Architecture.ToolSystem.md` when live volition tools are added.
- `docs/Architecture/Architecture.ContextManagement.md` when volition context packets or
  context-retrieval initiative hints influence live response context.
- `docs/Architecture/Architecture.VolitionSystem.md` for the extracted `qsf_volition`
  crate and the bounded-initiative behavior once Phase 5 names real modules.
- `docs/Architecture/Architecture.StateAndObservability.md` when volition traces
  (including `realtime_bounded_initiative_trace`) or persistence artifacts are added.
- `docs/ProjectFrame/ProjectVision.md` if the final project target or realtime
  consciousness-simulation framing changes.
- `docs/DecisionLog.md` for durable commitments: crate boundary, live tool scope
  expansion, behavioral influence boundary, the realtime bounded-initiative boundary
  (Phase 5), persistence boundary, or any change to protected-tier safety.

This plan is ephemeral. Durable documents should refer to named behaviors such as
"realtime volition inspection", "volition context injection", or "realtime bounded
initiative", not to this plan's phase numbers.

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