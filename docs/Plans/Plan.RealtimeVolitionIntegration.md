# Plan: Realtime Volition Integration

## Status

In progress. Phases 1 and 2 are complete.

- **Phase 1** (extract `qsf_volition`) — Complete. Pure volition domain extracted into
  its own crate; `qsf_realtime_server` does not depend on `qsf_app`.
- **Phase 2** (realtime-owned volition runtime state) — Complete. Each session holds
  isolated in-memory `VolitionRuntimeState` seeded from `realtime_seed_fixture()`.
  Protected-tier tensions (tier-2 explicit-user-intent, tier-3 current-task-completion)
  are in the seed and win arbitration under all modes. The sideband maps trusted
  transcripts to volition events and applies them on each turn boundary. No visible
  behavior change. Validated by `Experiment.RealtimeVolitionStateSeed`.
- **Phase 3** (read-only realtime volition tools) — Not started.

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

Implemented today:

- `crates/qsf_app/src/volition.rs` contains the current pure volition model:
  `VolitionState`, `VolitionEvent`, salience, arbitration, mode bias, goal candidates,
  and bounded initiative output.
- Volition is exercised through `qsf_app` registered experiments such as
  `volition-mode-bias` and `volition-bounded-initiative-execution`.
- `scripts/qsf.ps1 realtime` starts `qsf_realtime_server` and the realtime browser UI.
- `qsf_realtime_server` currently exposes only three read-only live tools:
  `search_memory`, `get_associations`, and `inspect_session_state`.
- `qsf_realtime_server` intentionally does not depend on `qsf_app`.

Therefore the realtime surface cannot currently inspect or use `VolitionState`.

## Architecture Direction

Use the established lean-crate extraction pattern:

```text
qsf_memory
qsf_context
qsf_session
qsf_tools
qsf_volition        <-- new pure domain crate
qsf_realtime_server <-- may depend on qsf_volition, not qsf_app
qsf_app             <-- keeps experiments and can re-export/adapt qsf_volition
```

`qsf_volition` should contain pure domain state, reducers, context-neutral selectors,
arbitration, fixtures, trace structs, and bounded initiative output. It must not depend
on `qsf_app`. The current `GoalSelection` / `GoalSelectionResult` shape in `qsf_app`
carries context assembly data, so those context-attached result types stay in `qsf_app`
or become thin adapters in the caller crates. `qsf_realtime_server` owns live state and
side effects, but any volition state changes still happen through pure volition events.

The first realtime integration should be read-only and inspectable. Behavioral
influence should be added only after the live system can explain the selected goals,
omitted/suppressed goals, arbitration result, and bounded initiative output.

Behavioral influence is also blocked until the default realtime seed includes protected
tier-2 explicit-user-intent and tier-3 current-task-completion tensions/goals, with tests
proving that they cannot be displaced by curiosity or exploration goals under any mode
bias.

## Phasing Principles

- Each phase builds, passes focused tests, then passes
  `cargo clippy --all-targets -- -D warnings` and `cargo fmt`.
- UI changes under `crates/qsf_realtime_server/ui/` also pass `npm run check` and
  `npm run fmt`.
- Reducers remain pure and unit-testable.
- View/context derivation stays in pure selectors/builders, not inline route handlers
  or UI components.
- New flags or thresholds must default to exercising the new code path.
- The live model initially receives read-only volition inspection only; behavior
  influence is explicit, trace-backed, and bounded.
- Human live-voice testing is required before considering a phase complete when it
  changes the spoken experience.
- Runtime modules and artifact names use stable behavior names, not plan phase names.

## Phase Overview

| Phase | Slice | Code? | Human test? | Status | Validation scaffold |
|---|---|---:|---:|---:|---|
| 1 | Extract pure volition domain into `qsf_volition` | Yes | No | Complete | `Experiment.RealtimeVolitionReadOnlyInspection` scaffold can reuse fixtures after extraction |
| 2 | Add realtime-owned `VolitionRuntimeState` seeded per QSF session | Yes | Light | Complete | `Experiment.RealtimeVolitionStateSeed` |
| 3 | Expose read-only realtime volition tools | Yes | Yes | Not started | `Experiment.RealtimeVolitionReadOnlyInspection` |
| 4 | Inject selected volition context into live response creation | Yes | Yes | Not started | `Experiment.RealtimeVolitionContextInjection` |
| 5 | Add trace-backed bounded initiative outputs to the live loop | Yes | Yes | Not started | `Experiment.RealtimeVolitionBoundedInitiative` |
| 6 | Persist, inspect, and consolidate realtime volition state | Yes | Yes | Not started | `Experiment.RealtimeVolitionContinuity` |
| 7 | Surface volition state in the realtime UI | Yes | Yes | Not started | `Experiment.RealtimeVolitionInspectionUi` |

## Phase Details

### Phase 1 - Extract pure volition domain

Move the reusable pure volition model out of `qsf_app` into a new lean crate,
`qsf_volition`.

Build:

- New crate `crates/qsf_volition`.
- Move or copy-then-delete the pure domain pieces from `crates/qsf_app/src/volition.rs`:
  tensions, goals, `VolitionState`, `VolitionEvent`, `apply`, selection, salience,
  arbitration, mode bias, candidate proposals, `InitiativeOutput`, static fixture, and
  trace structs.
- Do not move the current context-attached `GoalSelectionResult` shape as-is. Extract
  context-neutral selection/arbitration records first; leave `ContextFragment`,
  `ContextBudget`, `ContextAssembly`, and context assembly helpers in `qsf_context` /
  caller adapters.
- Keep experiment-specific markdown/report code in `qsf_app`.
- Update `qsf_app` experiments to import from `qsf_volition` or a thin `qsf_app`
  facade.
- Keep `qsf_volition` dependencies minimal: likely `serde`, `serde_json`, `time` if
  still needed, and no `qsf_app` dependency. A dependency on `qsf_context` is allowed
  only if the extracted type is genuinely context-domain data rather than an app
  selection/report adapter.

Verify:

- Existing volition experiment unit tests still pass.
- Add direct `qsf_volition` tests for reducer determinism, mode floor immunity,
  accepted-candidate selector wiring, and initiative output stability.
- Assert `qsf_volition` has no dependency on `qsf_app`, provider crates, HTTP, tokio,
  UI code, or context-attached selection result/reporting adapters.
- Full `cargo test` after extraction.

Human test:

- Not required; this is a pure refactor if behavior is unchanged.

Open questions:

- Should the static fixture live in `qsf_volition` as a reference fixture, or in
  `qsf_app` as experiment data? Default assumption: keep it in `qsf_volition` until a
  file-backed fixture format exists, because realtime needs a default seed.
- Should `EvidenceRef` accept realtime diagnostic references? Default assumption: yes,
  if they are stable artifact references and not raw provider payload dumps.

### Phase 2 - Add realtime-owned volition runtime state

Add volition state to each realtime session without exposing it to the model yet.

Build:

- Add a `volition` field to `qsf_realtime_server::state::SessionRuntime`, likely a
  small wrapper such as `VolitionRuntimeState`.
- Seed it from `qsf_volition::static_fixture()` using `VolitionState::from_fixture`.
- Populate the default realtime volition seed with tier-2 explicit-user-intent and
  tier-3 current-task-completion tensions/goals before any later phase can activate
  behavioral influence. These protected-tier goals may be fixture-backed or introduced
  through a small realtime seed adapter, but they must be present in arbitration tests
  before context injection is enabled.
- Track a monotonic volition tick using existing realtime turn/exchange boundaries.
- Add pure helper functions in the realtime server for mapping trusted realtime events
  into candidate `VolitionEvent`s. Keep mapping conservative:
  - final trusted user transcript can activate/select goals,
  - completed trusted exchange can record progress evidence,
  - explicit mode changes can use `ModeChanged`,
  - no automatic satisfaction unless the evidence contract is clear.
- Do not persist volition state yet unless the phase explicitly expands to cover
  schema compatibility.

Verify:

- New session starts with default fixture-backed volition state.
- Stable default realtime session id and random session id both get isolated in-memory
  volition state.
- Realtime session stop removes in-memory state with the session runtime.
- Mapping helpers are pure and unit-tested against scripted trusted-turn summaries.
- No provider/browser diagnostic event mutates volition state directly.
- A goal backed by a tier-2 tension wins arbitration over a tier-7 curiosity goal under
  `Neutral`, `Focused`, and `Exploratory` modes.
- Measure the trusted-turn path from `input_audio_transcription.completed` to
  `response.create` with and without volition event mapping active; the mapping-only
  delta must stay under 10 ms on the local deterministic test path.

Human test:

- Light manual test: start `qsf.ps1 realtime`, confirm the server runs normally and
  no visible behavior changes.

Open questions:

- Which realtime event should advance the volition tick: final user transcript,
  trusted exchange completion, or sideband response creation? Default assumption:
  trusted user turn boundary, because it is stable and aligns with goal activation.
- Should the initial realtime mode always be `Neutral`? Default assumption: yes. This
  plan is `Neutral`-only in the live path until an explicit realtime mode-change trigger
  is added. If mode changes are introduced, the earliest acceptable trigger is an
  explicit session configuration or operator/user command that emits
  `VolitionEvent::ModeChanged`; inferred sideband detection must not silently change
  mode.

### Phase 3 - Expose read-only realtime volition tools

Make volition accessible in realtime through read-only tools, preserving the existing
tool permission boundary.

Build:

- Add read-only realtime tool definitions and registry entries:
  - `inspect_volition_state`: compact current mode, tick, active/accepted/blocked
    goals, salience, cooldown, pending/accepted candidates, and last initiative output
    summaries.
  - `select_volition_goals`: given a user-visible query string, return selected goals,
    omitted goals, salience contribution, and budget reasons without mutating state.
  - Optional later in the phase: `inspect_volition_arbitration`, returning the current
    arbitration result for selected goals.
- Keep the tool/injection boundary explicit: later automatic context injection is
  ambient, compact, and always present; `select_volition_goals` is an on-demand query
  for explicit user questions or non-default reasoning. The tool should return ranked
  detail and trace fields, not duplicate the one-line context packet used by injection.
- Keep outputs budget-capped and summarized. Do not dump the entire fixture or all
  traces into the model context.
- Add the tools to the realtime allow-list by default so the new path is exercised.
- Extend `inspect_session_state` only if needed to include a high-level
  `volition_present` boolean; prefer separate volition tools for details.

Verify:

- Permission checks allow only registered read-only volition tools.
- Non-allow-listed or malformed volition tool calls are denied/recovered using the
  existing tool loop behavior.
- Tool outputs contain no `OPENAI_API_KEY`, raw browser relay payloads, or untrusted
  diagnostic-only transcripts.
- `select_volition_goals` is deterministic for the same state and query.
- Tool execution records persist to trusted turns like the existing read-only tools.

Human test:

- In a live realtime session, ask about current goals or why a topic matters; confirm
  the model can call the volition inspection tool and speak a grounded answer.
- Confirm the answer distinguishes "the system is simulating goals" from claiming real
  subjective desire.

Trace completeness contract:

`volition_tool_trace` must contain:

- `qsf_session_id`
- `tool_name`
- `volition_tick`
- `mode`
- `input_query` if applicable
- `selected_goal_ids`
- `omitted_goal_ids`
- `salience_snapshot`
- `arbitration_result` if requested
- `state_snapshot_hash`
- `artifact_or_record_reference`

Automated verification must parse the persisted tool execution result or diagnostic
record and assert these fields exist when the corresponding tool is used.

The `Experiment.RealtimeVolitionReadOnlyInspection` scaffold must record that
`select_volition_goals` is full ranked inspection detail, while later context injection
is compact ambient framing. Verification should fail if the tool output is just a copy
of the injection packet.

### Phase 4 - Inject selected volition context into live response creation

Let volition influence the live spoken response by adding a compact, traceable context
packet before `response.create`.

Build:

- Add a pure `build_volition_context_packet` function in `qsf_realtime_server` or
  `qsf_volition` adapter code.
- Gate this phase on the protected-tier seed/test from realtime volition state seeding:
  tier-2 user-intent and tier-3 task-completion goals must exist and must beat
  tier-7 curiosity/exploration goals regardless of mode bias.
- On each trusted user turn, select goals from the current `VolitionState`, arbitrate,
  and build a compact context fragment for the sideband's existing context injection
  path.
- Include only bounded fields:
  - winning goal id/title/summary,
  - arbitration status,
  - mode,
  - one-line rationale,
  - suppressed/omitted count with reason categories,
  - instruction that this is simulated internal state, not a claim of consciousness.
- Record a diagnostic or trace record before response creation.
- Default to enabled once implemented. If a config switch is added, default it on for
  local development and tests unless a safety reason requires otherwise.

Verify:

- Pure packet builder tests for empty selection, single selection, conflict, blocked
  goals, cooldown, and mode-biased arbitration.
- Sideband tests confirm the packet is injected before `response.create`.
- Latency tests confirm volition selection adds bounded overhead.
- Latency tests include the same
  `input_audio_transcription.completed` -> `response.create` boundary used by the
  seeding baseline, so added selection/arbitration cost is compared against the phase-2
  mapping-only measurement.
- Trusted exchange promotion still works and records the relevant context assembly.
- No raw volition fixture dump exceeds the context budget.

Human test:

- Live conversation on a volition-relevant topic. Confirm the response is subtly
  steered by active goals without becoming verbose or self-obsessed.
- Live conversation on a direct user task. Confirm protected tiers and user intent
  dominate curiosity/exploration.

Trace completeness contract:

`volition_context_injection_trace` must contain:

- `qsf_session_id`
- `exchange_index`
- `input_transcript_ref`
- `volition_tick_before`
- `events_applied`
- `selector_output`
- `omitted_or_suppressed_candidates`
- `arbitration_result`
- `mode_bias_outcomes`
- `context_packet_hash`
- `context_packet_token_estimate`
- `response_create_event_ref`

Automated verification must parse generated artifacts and assert that an injected
packet has a preceding trace and a subsequent response-create reference.

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

Create focused experiment scaffolds as phases begin:

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
  influence live response context.
- A new `docs/Architecture/Architecture.VolitionSystem.md` once the extracted
  `qsf_volition` crate exists and the implementation status can name real modules.
- `docs/Architecture/Architecture.StateAndObservability.md` when volition traces or
  persistence artifacts are added.
- `docs/ProjectFrame/ProjectVision.md` if the final project target or realtime
  consciousness-simulation framing changes.
- `docs/DecisionLog.md` for durable commitments: crate boundary, live tool scope
  expansion, persistence boundary, behavioral influence boundary, or any change to
  protected-tier safety.

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
