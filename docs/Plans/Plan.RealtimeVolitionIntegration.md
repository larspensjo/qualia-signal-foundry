# Plan: Realtime Volition Integration

## Status

In progress. Phases 1-3 are complete. Phase 4 is the next implementation slice. Its
review-flagged decision gates are resolved: the stable-baseline carrier, the grounded
opportunity-detection scope, and the exact injected baseline/turn-packet text are committed in
`docs/Experiments/Experiment.RealtimeVolitionContextInjection.md`. Read that scaffold before
implementing Phase 4.

- **Phase 1** (extract `qsf_volition`) — Complete. Pure volition domain extracted into
  its own crate; `qsf_realtime_server` does not depend on `qsf_app`.
- **Phase 2** (realtime-owned volition runtime state) — Complete. Each session holds
  isolated in-memory `VolitionRuntimeState` seeded from `realtime_seed_fixture()`.
  Protected-tier tensions (tier-2 explicit-user-intent, tier-3 current-task-completion)
  are in the seed and win arbitration under all modes. The sideband maps trusted
  transcripts to volition events and applies them on each turn boundary. No visible
  behavior change. Validated by `Experiment.RealtimeVolitionStateSeed`.
- **Phase 3** (read-only realtime volition tools) — Complete. Live trusted sideband testing
  verified both `inspect_volition_state` and `select_volition_goals`; diagnostics now record
  trusted completed exchanges as `source: "sideband_trusted"`. The broad help-related selector
  query still returns `no_match`, which is a selector-quality follow-up rather than a phase gate.
  `inspect_volition_state` and `select_volition_goals` are registered in the realtime
  default tool list. Validated by `Experiment.RealtimeVolitionReadOnlyInspection`.

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
  step and the conversational-intensity dial absorbed by Phase 4.

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

As of Phase 3 the pure domain lives in `qsf_volition`, each session carries
`VolitionRuntimeState`, and two read-only volition tools are registered. The remaining
gap Phase 4 closes is that volition does not yet influence the live spoken response.

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

The first realtime integration was read-only and inspectable. Behavioral influence is
added only now that the live system can explain the selected goals, omitted/suppressed
goals, arbitration result, and bounded initiative output.

Behavioral influence is also gated on the default realtime seed already including
protected tier-2 explicit-user-intent (`honor-explicit-user-request`) and tier-3
current-task-completion (`complete-current-task`) tensions/goals, with tests proving
they cannot be displaced by curiosity or exploration goals under any mode bias. That
gate is satisfied by Phase 2 and must remain green for Phase 4.

## Phasing Principles

- Each phase builds, passes focused tests, then passes
  `cargo clippy --all-targets -- -D warnings` and `cargo fmt`.
- UI changes under `crates/qsf_realtime_server/ui/` also pass `npm run check` and
  `npm run fmt`.
- Reducers remain pure and unit-testable. State changes flow through `VolitionEvent`
  applied by `qsf_volition::apply`; selectors and packet builders read snapshots and
  never mutate.
- View/context derivation stays in pure selectors/builders, not inline route handlers
  or UI components.
- Entry points (`main.rs`, `mod.rs`, `lib.rs`) stay thin wrappers.
- New flags or thresholds must default to exercising the new code path.
- The live model initially receives read-only volition inspection only; behavior
  influence is explicit, trace-backed, and bounded.
- Human live-voice testing is required before considering a phase complete when it
  changes the spoken experience.
- Runtime modules and artifact names use stable behavior names, not plan phase names.

## Phase Overview

| Phase | Slice | Code? | Human test? | Status | Validation scaffold |
|---|---|---:|---:|---:|---|
| 1 | Extract pure volition domain into `qsf_volition` | Yes | No | Complete | `Experiment.RealtimeVolitionReadOnlyInspection` scaffold reuses fixtures after extraction |
| 2 | Add realtime-owned `VolitionRuntimeState` seeded per QSF session | Yes | Light | Complete | `Experiment.RealtimeVolitionStateSeed` |
| 3 | Expose read-only realtime volition tools | Yes | Yes | Complete | `Experiment.RealtimeVolitionReadOnlyInspection` |
| 4 | Layered volition context injection — stable baseline plus dynamic goals/intentions with opportunity detection + shaping-intensity dial | Yes | Yes | Next | `Experiment.RealtimeVolitionContextInjection` |
| 5 | Add trace-backed bounded initiative outputs to the live loop | Yes | Yes | Not started | `Experiment.RealtimeVolitionBoundedInitiative` |
| 6 | Persist, inspect, and consolidate realtime volition state | Yes | Yes | Not started | `Experiment.RealtimeVolitionContinuity` |
| 7 | Surface volition state in the realtime UI | Yes | Yes | Not started | `Experiment.RealtimeVolitionInspectionUi` |

## Phase Details

### Completed Phases 1-3 (summary)

Phases 1-3 are implemented and validated. The durable outcomes and constraints they
carry into later work:

- **Pure domain crate.** `qsf_volition` owns tensions, goals, `VolitionState`,
  `VolitionEvent`, `apply`, salience, arbitration (`arbitrate`, `arbitrate_with_mode`,
  `Mode`, `PROTECTED_TIER_FLOOR = 3`), candidate proposals, `InitiativeOutput`, the
  seed fixtures (`static_fixture()`, `realtime_seed_fixture()`), term normalization
  (`normalize_terms`), selection (`select_goals_ranked`, `RankedSelectionResult`,
  `GoalSelection`), and inspection (`build_state_inspection`). It has no dependency on
  `qsf_app`, providers, HTTP, tokio, or UI. Context-attached selection/report adapter
  types stay in `qsf_app`. **Constraint for later phases:** keep this boundary; do not
  pull realtime or context-assembly types into `qsf_volition`, and route all state
  change through `VolitionEvent` + `apply`.
- **Per-session runtime state.** `crates/qsf_realtime_server/src/realtime/volition.rs`
  defines `VolitionRuntimeState` (`state` + `fixture`), seeded from
  `realtime_seed_fixture()` on session creation, isolated per session, in-memory only
  (no persistence yet). The protected seed goals `honor-explicit-user-request` (tier-2)
  and `complete-current-task` (tier-3) are present and start `Accepted`. **Constraint:**
  these protected-tier goals must beat tier-7 curiosity/exploration goals under
  `Neutral`, `Focused`, and `Exploratory`; this invariant gates Phase 4 and must stay
  green.
- **Trusted-turn mapping.** `events_for_trusted_transcript(transcript, &state, &fixture,
  new_tick)` is the pure, deterministic, model-free map from a trusted user transcript
  to `VolitionEvent`s (tick lifecycle → `TickAdvanced` → `GoalActivated` for keyword
  matches on `Accepted`/`Active` goals; cooldown and retired goals are not activated).
  `apply_trusted_transcript_to_volition` in `sideband.rs` calls it once per trusted turn
  boundary (text turn, `StartTurn`, and post-`Interrupt`). **Lessons/constraints:** the
  volition tick advances on the trusted user-turn boundary; mapping overhead is held
  under 10 ms on the deterministic test path (regression test exists); the live path is
  `Neutral`-only — mode never changes from inferred sideband signals, only from an
  explicit `VolitionEvent::ModeChanged`; no automatic goal satisfaction; provider/browser
  diagnostic events never mutate volition state.
- **Read-only tools.** `inspect_volition_state` and `select_volition_goals`
  (`crates/qsf_realtime_server/src/realtime/volition_tools.rs`) are registered in the
  default allow-list, permission-checked, budget-capped (≤6 selected, ≤8 omitted in
  model-visible output), deterministic, and emit a parseable `volition_tool_trace`
  observation summary that carries no secrets. Trusted sideband exchanges record to
  diagnostics with `source: "sideband_trusted"`, `trust: "trusted"`. **Constraints
  carried into Phase 4:** `select_volition_goals` is the *full ranked inspection detail*
  surface and must stay distinct from the compact ambient injection packet — the packet
  must never be merely a copy of the tool output, and the tool output must never be
  merely the packet. **Known follow-up:** the broad help-related query still returns
  `no_match`; this is a selector-quality refinement, not a Phase 4 prerequisite.

The `volition_tool_trace` contract from Phase 3 (`qsf_session_id`, `tool_name`,
`volition_tick`, `mode`, `input_query`, `selected_goal_ids`, `omitted_goal_ids`,
`salience_snapshot`, `arbitration_result`, `volition_snapshot_hash`,
`artifact_or_record_reference`) remains the model for trace fields and parsing-based
verification.

### Phase 4 - Layered volition context injection into live response creation

Let volition influence the live spoken response by adding layered, traceable context before
`response.create`. This phase is not one universal volition blob: each layer has a distinct
lifetime, carrier, and injection point.

Layer ordering for the first implementation:

1. **Stable baseline / personality rendering** — constant across sessions and injected in
   the realtime `session.update` instructions. In project terms this is a compact rendering
   of the configured tension set, declared priors, arbitration stance, project trust
   boundary, and default `Mode`; it is not a separate mutable personality object and must
   not invent desires outside the configured volition state.
2. **Stable drives / tensions summary** — optional compact companion to the baseline when it
   helps explain the system's default orientation. Refreshed only when configured
   drive/tension state changes, not every turn.
3. **Per-turn active goals and intentions** — selected after a trusted user turn, opportunity
   detection, and volition event mapping; injected before the initial `response.create` for that
   turn.
4. **Per-turn memory context** — remains a separate retrieval/context-management layer
   (`build_memory_injection_packet`); Phase 4 composes ordering with it but must not merge
   memory retrieval and volition rendering into one unstructured prompt.
5. **Plans** — deferred multi-turn layer. Only inject plan context once a dedicated active-plan
   representation exists.

This phase absorbs Adaptation B from
[`Design.VolitionBriefReconciliation.md`](Design.VolitionBriefReconciliation.md): behavioral
influence is built around an explicit **opportunity-detection** step (brief §4.1) and a
**conversational-intensity dial** (brief §14) from the start, rather than retrofitting them.
Both are pure, deterministic, inspectable, and rule-based (no model call) in this slice.

#### Integration map (where this hooks in)

All sideband references are `crates/qsf_realtime_server/src/realtime/sideband.rs` unless noted.

- **Initial `session.update`** is built in `connect_and_run_once` from
  `session_config(...)` via `build_openai_realtime_conversation_session_update(...)`. This
  is the carrier for the stable baseline (Layer 1/2).
- **Per-turn voice path** is the `"conversation.item.input_audio_transcription.completed"`
  arm of `handle_provider_event`: it calls `apply_trusted_transcript_to_volition`, then
  builds the memory packet (`build_memory_injection_packet`), then sends
  `session.update` + `conversation.item.create` + `build_openai_realtime_response_create`.
- **Per-turn typed path** is `handle_text_turn`, which mirrors the voice path and also
  calls `apply_trusted_transcript_to_volition`. The dynamic turn packet (Layer 3) must be
  injected in **both** paths; the duplicated memory-packet/response-create sequence should
  be factored into one shared helper rather than duplicating the new logic.
- **Tool-loop continuation `response.create`** is built in `handle_response_done_event`
  after tool execution. It must **not** receive a fresh volition turn packet.
- **Pure selectors already available** in `qsf_volition`: `normalize_terms`,
  `select_goals_ranked`, `arbitrate_with_mode`, `Mode`, `PROTECTED_TIER_FLOOR`,
  `build_state_inspection`. The per-session snapshot type `VolitionStateSnapshot` exists in
  `crates/qsf_realtime_server/src/realtime/tools.rs`.
- **Diagnostics** are written through `crate::diagnostics::DiagnosticWriter` /
  `DiagnosticRecord`; the injection trace adds a new record variant here.

#### Build (incremental, each step independently reviewable)

**Step 4a — Opportunity detection (pure, `qsf_volition`).**
Add an `OpportunitySignal` type and a pure `detect_opportunities(input_terms: &[GroundedTerm],
state: &VolitionState, fixture: &VolitionFixture) -> Vec<OpportunitySignal>` in a new
`crates/qsf_volition/src/opportunity.rs` (re-exported from `lib.rs`). `GroundedTerm` carries
the normalized text plus the original input text/span so each signal can cite a real
grounding ref (normalized terms alone cannot reproduce original spans). Each signal carries a
`kind` (`ExpressedUncertainty`, `IntroducedContradiction`, `OpenGoalTopicMatch`) and a
`grounding_ref` that cites the grounding input term/span or a goal id — no invented
opportunities. `UnresolvedPriorTopic` is **deferred**: `qsf_volition` has no prior-topic /
continuity source to ground it, so it is out of scope until a continuity source is passed in
from the adapter (do not pull realtime/context state into the pure crate to fake it).
Derivation is deterministic and rule/keyword-based (reusing `normalize_terms`); no model
call. Unit tests: each emitted signal cites a grounding ref; unrelated input emits none;
output is deterministic.

**Step 4b — Shaping-intensity dial (pure, `qsf_volition`).**
Add `ShapingIntensity { None, Low, Medium, High }` and a pure
`choose_shaping_intensity(arbitration: &ModeArbitrationResult, opportunities:
&[OpportunitySignal], receptiveness: ReceptivenessHint) -> ShapingIntensity` in a new
`crates/qsf_volition/src/shaping.rs`. Inputs are inspectable only: arbitration result,
winning-goal salience/unresolvedness, opportunity signals, and a receptiveness/flow hint.
The **protected-tier cap** holds by construction: when the arbitration winner is protected
(`winner_bias.effective_tier <= PROTECTED_TIER_FLOOR`, i.e. tiers 1-3 such as
safety/boundary, explicit user intent, current task completion), intensity is clamped to at
most `Low` — mirroring the mode-bias `PROTECTED_TIER_FLOOR` invariant. Semantics: `Low` =
gentle bias/observation/follow-up; `Medium` = reintroduce an unresolved thread or prefer a
branch; `High` = explicitly prioritize a simulator goal (rare, documented justifying
conditions). Unit tests: deterministic for the same inputs; a protected winner clamps to
≤ `Low` under every `Mode`; `High` requires its documented conditions and never co-occurs
with a protected winner.

**Step 4c — Layer renderers (pure builders).**
- In `qsf_volition`, add a context-neutral `render_volition_stance(fixture, mode) -> String`
  that renders the configured tension set, priors, arbitration stance, and default mode as
  bounded text with no session/turn-specific facts and no claim of real desire,
  consciousness, or subjective experience.
- In a new `crates/qsf_realtime_server/src/realtime/volition_injection.rs` (analogue of
  `injection.rs`), add `build_stable_baseline_instructions(...)` that wraps
  `render_volition_stance(...)` with the realtime/project trust-boundary preamble, and
  `build_volition_turn_context_packet(snapshot, ranked, arbitration: Option<ModeArbitrationResult>,
  opportunities, intensity) -> Option<VolitionTurnPacket>` that renders the bounded per-turn
  system text plus the structured trace. Because `arbitrate_with_mode` returns `None` for an
  empty selection, callers short-circuit and return `None` before building; the builder also
  treats arbitration as optional and returns `None` rather than rendering an empty packet.
  Bounded fields only: winning goal id/title/summary; arbitration status;
  mode; opportunity signals (kind + grounding ref); chosen shaping intensity + the inputs
  that set it; one-line rationale; suppressed/omitted count with reason categories; and an
  instruction stating the allowed shaping intensity for this turn and that this is simulated
  internal state, not a claim of consciousness. Reuse `build_state_inspection` where useful;
  do not duplicate the ranked detail produced by `select_volition_goals`. Unit tests cover
  empty selection, single selection, conflict, blocked goals, cooldown, and mode-biased
  arbitration, plus the no-secret / bounded-length / no-false-desire assertions for the
  baseline.

**Step 4d — Wire the stable baseline into the shared base instructions.**
Carrier decision (committed, see `Experiment.RealtimeVolitionContextInjection`): render the
baseline deterministically and include the identical text in the **base instructions used by
both the initial and every per-turn `session.update`** (and therefore by `response.create`,
which is built from the same `config.instructions`). `session.update` replaces session
config, so a baseline placed only in the initial `session.update` would be overwritten by the
next per-turn `session.update`. Routing it through the shared base instructions gives one
effective instruction-composition path across the initial `session.update`, the per-turn
`session.update`, the initial `response.create`, and tool-loop continuation, so the baseline
can never be silently dropped or overridden; the field is re-sent but its content never
changes, verified by a stable `stable_baseline_hash`. Record `stable_baseline_hash` for the
trace.

**Step 4e — Wire the dynamic per-turn packet (shared helper).**
Factor the duplicated per-turn "build memory packet → send items → send `response.create`"
sequence shared by `handle_text_turn` and the voice transcription arm into one helper. After
`apply_trusted_transcript_to_volition`, compute — from the **post-event** snapshot
(`VolitionStateSnapshot { state, fixture }`) — opportunities (4a), `select_goals_ranked` for
the transcript, `arbitrate_with_mode(selected, fixture, state.mode)`, and
`choose_shaping_intensity` (4b). Build the turn packet (4c) and, when `Some`, send it as an
additional system `conversation.item.create` (via `build_openai_realtime_conversation_item_create`)
**after** the memory item and **before** the initial `response.create`. The volition packet is
computed and injected **independently of memory retrieval**: `build_memory_injection_packet`
returns `None` on turns with no retrieved memories, and the current sideband only sends memory
payloads inside that `Some` branch, so the volition packet must be sent outside that branch.
Per-turn ordering is: optional memory `session.update` + memory item, then optional volition
item, then `response.create`. Do not inject a fresh packet on tool-loop continuation
`response.create` in `handle_response_done_event`. Because
`select_goals_ranked` needs the post-mapping state, either have
`apply_trusted_transcript_to_volition` return the applied events/snapshot or read the guard's
`volition.state` immediately after applying — keep the computation pure and the mutation
confined to the existing `apply_events` call.

**Step 4f — Trace + diagnostics.**
Add a `DiagnosticRecord::VolitionContextInjected { .. }` variant carrying the
`volition_context_injection_trace` fields below, and write it before the `response.create`
for the trusted user turn. Add a latency observation around volition selection on the same
`input_audio_transcription.completed -> response.create` boundary the seeding baseline used,
so the added selection/arbitration cost is comparable to the Phase 2 mapping-only
measurement.

**Step 4g — Default-on, docs, and experiment scaffold.**
Default the behavior to enabled. If a config switch is added, default it on for local
development and tests (per AGENTS.md, the default must exercise the new path); the default
shaping policy keeps `High` rare and never lets curiosity/exploration override protected
tiers. Create `docs/Experiments/Experiment.RealtimeVolitionContextInjection.md` (it does not
exist yet) with the trace completeness contract and the **exact rendered baseline text and
dynamic turn-packet text** specified before implementation, then asserted in tests so the
sideband injection contract is explicit, not implicit. Update the documentation listed under
"Documentation To Update".

#### Verify

- Stable baseline rendering tests: deterministic output for the same configured tension/mode
  state; no session/user-turn-specific facts; no claim of real desire, consciousness, or
  subjective experience; bounded length.
- Pure turn-packet builder tests for empty selection, single selection, conflict, blocked
  goals, cooldown, and mode-biased arbitration.
- Opportunity-detection tests: each emitted signal cites a grounding input span or
  goal/memory id; unrelated input emits no signals; detection is deterministic and
  model-free.
- Shaping-intensity tests: the dial is deterministic for the same inputs; a protected
  arbitration winner (`effective_tier <= PROTECTED_TIER_FLOOR`) clamps intensity to at most
  `Low` under every mode; `High` requires the documented justifying conditions and never
  co-occurs with a protected winner.
- Sideband tests confirm the stable baseline is present in the initial `session.update` and
  the dynamic turn packet is injected before the initial `response.create` for a trusted user
  turn (both the typed and the voice path).
- Sideband tests confirm tool-loop continuation `response.create` calls do not duplicate
  fresh volition turn packets.
- Latency tests confirm volition selection adds bounded overhead, measured on the same
  `input_audio_transcription.completed -> response.create` boundary used by the seeding
  baseline, compared against the Phase 2 mapping-only measurement.
- Trusted exchange promotion still works and records the relevant context assembly.
- No raw volition fixture dump exceeds the context budget.
- `cargo test`, then `cargo clippy --all-targets -- -D warnings`, then `cargo fmt`.

#### Human test

- Live conversation on a volition-relevant topic. Confirm the response is subtly steered by
  active goals without becoming verbose or self-obsessed.
- Live conversation on a direct user task. Confirm protected tiers and user intent dominate
  curiosity/exploration.
- Confirm spoken framing still distinguishes simulated internal state from a claim of real
  subjective desire.

#### Trace completeness contract

`volition_context_injection_trace` must contain:

- `qsf_session_id`
- `exchange_index`
- `injected_layers` with layer name, carrier, and injection point
- `stable_baseline_hash`
- `input_transcript_ref`
- `volition_tick_before`
- `events_applied`
- `opportunity_signals` (each with kind + grounding ref)
- `selector_output`
- `omitted_or_suppressed_candidates`
- `arbitration_result`
- `mode_bias_outcomes`
- `protected_tier_active`
- `shaping_intensity` and `shaping_intensity_inputs`
- `context_packet_hash`
- `context_packet_token_estimate`
- `response_create_event_ref` — the existing per-turn `current_request_hash`
  (`hash_request_sequence` over the outbound turn request sequence), which already
  deterministically covers the `response.create` payload. Use this stable reference rather
  than adding a new outbound client event id; the `VolitionContextInjected` record and the
  turn's recorded request hash carry the same value so the link is verifiable.

Automated verification must parse generated artifacts (the diagnostics JSONL) and assert
that an injected packet has a preceding `VolitionContextInjected` trace and a subsequent
resolvable `response_create_event_ref`, that the stable baseline layer was already present
for the session, that every opportunity signal carries a grounding ref, and that
`shaping_intensity` is at most `Low` whenever `protected_tier_active` is true. The artifact
boundary is the diagnostics record stream; the verification parses persisted records, not
in-memory structs.

#### Open questions

- **Stable baseline persistence vs per-turn `session.update` replacement (resolved).**
  Committed: render the baseline deterministically and include the identical text in the base
  instructions used by **both** the initial and per-turn `session.update` (and therefore by
  `response.create`), so the *content* never changes (verified by a stable
  `stable_baseline_hash`) even though the field is re-sent. This gives one effective
  instruction-composition path and prevents the baseline from being silently dropped or
  overridden. The persistent system `conversation.item.create` alternative is rejected (harder
  to protect against history truncation; splits the stance from where response shaping reads
  instructions). See `Experiment.RealtimeVolitionContextInjection` (decision D1) and Step 4d.
- **Crate placement of stance rendering, opportunity detection, and the intensity dial.**
  Recommended default: keep `render_volition_stance`, `detect_opportunities`, and
  `choose_shaping_intensity` in `qsf_volition` (context-neutral, pure), and keep only the
  realtime/project trust-boundary wrapper and the injection carrier in `qsf_realtime_server`.
  Confirm this split, since the baseline's trust-boundary language is realtime-specific.
- **Receptiveness/flow hint source.** The dial wants a conversation flow/receptiveness input,
  but the realtime path exposes no such signal today. Recommended default for this slice: a
  neutral constant hint, with a derived receptiveness signal (e.g. interruption history, turn
  cadence) as a follow-up. Confirm before implementing Step 4b, or accept the neutral default.
- **Continuity ordering (Adaptation A, deferred decision).** The brief's headline behavior is
  *persistent unfinished business* across sessions, but cross-session persistence is currently
  the persistence phase below. Decide whether a minimal continuity slice — e.g. recurring
  `Blocked`/open-thread goal ids only — should precede or interleave with this phase so
  behavioral influence can resurface prior threads. Not yet committed; see
  [`Design.VolitionBriefReconciliation.md`](Design.VolitionBriefReconciliation.md).
- Should opportunity detection stay purely rule-based, or may a later slice add a
  model-assisted classifier that emits the same grounded `OpportunitySignal` shape through the
  event/reducer path? Default: rule-based only in this slice.
- Should the shaping-intensity policy be fixed, or exposed as an inspectable per-session
  autonomy-level setting (brief §19.1)? Default: fixed conservative policy here; a user-set
  autonomy level is a follow-up.
- **Exact rendered baseline text and dynamic turn-packet text (resolved).** Specified in
  `Experiment.RealtimeVolitionContextInjection` under "Injected Text Contract" (the baseline
  rendered from the configured tension set with the trust-boundary preamble, and the per-turn
  packet template), to be asserted verbatim in tests so the sideband injection contract is
  explicit, not implicit.

### Phase 5 - Add bounded initiative outputs to the live loop

Allow volition to produce bounded internal initiative outputs in realtime, still
without executing external effects.

Build:

- Reuse `qsf_volition::execute_initiative` to produce `InitiativeOutput`.
- Treat initiative output as an internal context/action proposal, not as an external
  side effect.
- Supported first outputs should remain internal and read-only:
  - request reflection,
  - request context retrieval as query-term hints for the next sideband memory/context
    injection pass, not as an immediate tool call,
  - surface an open thread,
  - propose an experiment for later human review.
- Add a realtime reducer/action mapping for `InitiativeExecuted` that stores the last
  output in volition state.
- Feed the output into response context, diagnostics, or the next existing memory
  injection pass only; do not write files, create plans, run commands, or trigger
  external tools from initiative output.

Verify:

- Initiative execution is deterministic and side-effect-free.
- `executed_external_effects = 0` or equivalent is recorded in each trace.
- Direct user intent and task-completion goals cannot be displaced by curiosity or
  exploration outputs.
- Tool loop cap and realtime interruption behavior remain unchanged.

Human test:

- In live voice, ask an open-ended research question and confirm the system can
  surface a relevant internal initiative without taking action.
- Confirm it does not repeatedly nag or derail the conversation.

Trace completeness contract:

`realtime_bounded_initiative_trace` must contain:

- `qsf_session_id`
- `exchange_index`
- `winning_goal_id`
- `initiative_proposal`
- `allowed_effect`
- `initiative_output`
- `bounded_or_external_output` with explicit `external_effect_executed: false`
- `context_retrieval_hint_terms` when the output is `ContextRetrievalRequested`
- `hint_consumed_by_next_memory_injection` when those terms are passed to the existing
  sideband memory/context injection path
- `rationale`
- `state_snapshot_before`
- `state_snapshot_after`
- `artifact_or_record_reference`

Automated verification must assert every initiative output has a prior arbitration
winner and no external-effect execution.

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
`Experiment.RealtimeVolitionContextInjection` exist; the latter now carries the Phase 4
injected-text contract and resolved decision gates.

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
  tools become part of the realtime server.
- `docs/Architecture/Architecture.ToolSystem.md` when live volition tools are added.
- `docs/Architecture/Architecture.ContextManagement.md` when volition context packets
  influence live response context (Phase 4).
- A new `docs/Architecture/Architecture.VolitionSystem.md` once the extracted
  `qsf_volition` crate exists and the implementation status can name real modules.
- `docs/Architecture/Architecture.StateAndObservability.md` when volition traces or
  persistence artifacts are added.
- `docs/ProjectFrame/ProjectVision.md` if the final project target or realtime
  consciousness-simulation framing changes.
- `docs/DecisionLog.md` for durable commitments: crate boundary, live tool scope
  expansion, behavioral influence boundary (Phase 4), persistence boundary, or any
  change to protected-tier safety.

This plan is ephemeral. Durable documents should refer to named behaviors such as
"realtime volition inspection" or "volition context injection", not to this plan's
phase numbers.

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