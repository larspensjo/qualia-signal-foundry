# Decision log

Purpose: durable record of deliberate commitments — the source of truth for what the
project has agreed to do going forward.

## How to use
- One entry per decision. Decisions are commitments, not summaries of work.
- Implementation summaries and bug-fix postmortems belong in `EngineeringDiary.md`.
  A bug fix earns a decision-log entry only when it produces a durable rule, and the
  rule itself is the entry, not the fix.
- Reversals of prior decisions get their own entry referencing the original.
- A plan in itself, or change thereof, is not a decision until it is committed to.
- Keep entries concise and reference concrete artifacts.
- New entries go to the end of the file.

Use the decision log for:
- Architecture commitments
- Technology or library choices
- Naming, structural, or coding conventions adopted project-wide
- Safety and scope boundaries
- Experiment outcomes promoted into accepted design
- Reusable rules derived from incidents

## Entry Template

## YYYY-MM-DD - <decision title>
Decision: <the rule, in present tense>
Context: <why this was decided now>
Consequences: <what this constrains or implies going forward>
Refs: path/to/file.rs, experiment, prior decision (for reversals), etc.

## 2026-05-09 - Unidirectional event-reducer-state flow
Type: Decision
Decision: The runtime loop updates state exclusively through pure reducer functions of the
form (State, Event) → State. Side effects are isolated and fed back as events.
Context: Explainable state transitions, pure-function testability, and clean separation
between what happened (events) and what changed (state). Established in Agents.md and
mirrored in Architecture.RuntimeLoop.md.
Consequences: Side-effect-producing operations (model calls, tool invocations, I/O) must
not modify state directly. They must emit events that the reducer then processes.
Refs: docs/Architecture/Architecture.RuntimeLoop.md, Agents.md

## 2026-05-09 - Diary and decision-log document contracts
Decision: `EngineeringDiary.md` is the chronological "what happened" log (every
submitted code change, plus research, planning, surprises, and observations) at a
granularity of one entry per logical change. `DecisionLog.md` is reserved for deliberate
commitments only.
Context: The two documents had overlapping templates — the decision log accepted
`Implementation` and `Bug Fix` types, which duplicated what the diary covered. The split
makes each document's purpose unambiguous and lets the decision log stay short and
authoritative.
Consequences: Implementation summaries and bug-fix postmortems do not produce
decision-log entries; a fix only earns one when it yields a durable rule, and the rule
itself is the entry. Diary entries that implement a prior decision should reference it
in their Refs line.
Refs: docs/EngineeringDiary.md, docs/DecisionLog.md,
docs/ProjectFrame/ProjectWorkflow.md

## 2026-05-10 - Transcript-first realtime speech integration
Decision: The first real audio provider integration uses streaming transcription as
partial and final transcript events. Full speech-to-speech realtime sessions and live
translation remain separate later experiments.
Context: OpenAI's May 2026 realtime speech models make realtime audio more practical,
but the framework is still centered on observable event flow, pure reducers, and
replaceable side-effect providers. Streaming transcription fits that boundary with
less complexity than a full voice agent. The model IDs were checked against current
OpenAI API documentation on 2026-05-10.
Consequences: `gpt-realtime-whisper` is the first OpenAI realtime speech target.
`gpt-realtime-2` is reserved for later voice-session experiments, and
`gpt-realtime-translate` is reserved for translation-specific experiments. Realtime
providers emit QSF events and do not own runtime state, memory promotion, tool
permissions, or decisions.
Refs: docs/Plans/Plan.FrameworkMVP.md,
docs/Experiments/Experiment.StreamingTranscriptionMVP.md,
docs/Architecture/Architecture.AudioLoop.md,
https://developers.openai.com/api/docs/guides/realtime-transcription,
https://developers.openai.com/api/docs/models/gpt-realtime-2,
https://developers.openai.com/api/docs/models/gpt-realtime-translate

## 2026-05-10 - Memory schema versioning is per record type and run artifacts are sealed
Decision: `MemoryRecord` and `Association` each carry an independent `schema_version: u16`
field from v1. The live runtime reads and writes only the current version. Past memory
artifacts are immutable and never migrated in place; versioned readers for older artifacts
live in a separate compatibility module used for replay and analysis. Pure additive
changes (new optional fields with serde defaults) do not bump the version; removed,
renamed, or semantically changed fields do.
Context: Phase 4 of Plan.FrameworkMVP introduces memory records. The framework's replay
goal — "would the same input retrieve the same memories?" — is incompatible with
forward-migrating old run artifacts, which would distort historical evidence. Adding the
version field at v1 is cheap; retrofitting it later is not. Memory records and
associations evolve on independent timelines and should not share a version.
Consequences: Every persisted memory record and association carries its own
`schema_version`. The live `MemoryStore` errors loudly on off-version records rather than
attempting to interpret them. Compatibility readers are written only when a schema
actually changes, and are kept out of the live runtime. A future cross-run shared memory
store (for example, from sleep-phase consolidation) is out of scope and may use a
different policy.
Refs: docs/Plans/Design.MemorySchemaVersioning.md,
docs/Plans/Plan.FrameworkMVP.md,
docs/Architecture/Architecture.MemorySystem.md

## 2026-05-11 - Model access uses explicit roles and optional provider adapters
Decision: Model invocations are expressed as typed `ModelRole` plus `ModelRequest` pairs and execute through a synchronous `ModelClient` boundary. The OpenAI-backed path remains an optional adapter over `openai_provider_kit` and is selected explicitly by configuration rather than automatically when `OPENAI_API_KEY` happens to be present.
Context: Phase 6 needed deterministic model-role experiments and a real OpenAI-backed path without forcing the whole runtime loop async or letting ambient environment variables silently change behavior.
Consequences: Mock and OpenAI clients share one provider-agnostic contract, model-role traces can stay uniform across providers, and future async or multi-provider work changes the adapter/effects boundary rather than every call site. Possessing an API key alone does not switch the runtime away from deterministic mock behavior.
Refs: Cargo.toml, crates/qsf_app/src/models,
crates/qsf_app/src/experiments/model_role_smoke.rs,
docs/Architecture/Architecture.ModelRoles.md

## 2026-05-12 - Real audio providers remain explicit evaluation paths
Decision: Real streaming transcription inputs are selected explicitly through
`QSF_TRANSCRIPT_PROVIDER` and `QSF_TRANSCRIPT_INPUT_SOURCE`; the default path remains
deterministic simulation, and provider adapters report transcript metadata rather than
persisting raw audio.
Context: Phase 9 introduced OpenAI Realtime WebSocket transcription plus prerecorded
WAV and live microphone evaluation paths. Real audio depends on credentials, devices,
permissions, network behavior, and local recording conditions, so it should not activate
just because those capabilities are compiled in.
Consequences: Tests and normal experiment runs stay deterministic by default. WAV and
microphone evaluation are opt-in side-effect paths, and any future provider must preserve
the same no-secret/no-raw-audio observability boundary.
Refs: crates/qsf_app/src/audio/transcript_provider.rs,
crates/qsf_app/src/experiments/streaming_transcription_mvp.rs,
docs/Plans/Plan.FrameworkMVP.md

## 2026-05-13 - Feature-gated audio providers need explicit compile checks
Decision: Phase 9 real-audio readiness includes compiling the `qsf_app/openai` feature
path, not only running the default simulated transcript tests.
Context: The default build kept deterministic streaming transcription tests green, but
the OpenAI realtime transcription adapter had drifted against current CPAL and
Tungstenite APIs. Real WAV and microphone smoke tests depend on this feature-gated path.
Consequences: Changes to optional audio adapters should include at least one targeted
`--features openai` compile or test pass before considering the phase ready for real
audio evaluation.
Refs: crates/qsf_app/src/audio/transcript_provider.rs,
docs/EngineeringDiary.md

## 2026-05-13 - Realtime transcription optimizes for latency first
Decision: The OpenAI realtime transcription adapter defaults to `gpt-realtime-whisper`.
`gpt-4o-transcribe` remains an evaluation alternative for accuracy-sensitive runs.
Context: Phase 9 live tests proved the provider boundary. A follow-up model review
rechecked the official OpenAI model catalog and realtime transcription guide, which
list `gpt-realtime-whisper` as the lowest-latency streaming transcription path for
live audio and transcript deltas. The project values realtime presence, so latency is
the first defaulting criterion for Phase 9.
Consequences: `gpt-realtime-whisper` is the first provider-backed transcription
target. Accuracy comparisons should use explicit model selection rather than changing
the default away from the realtime path. Full speech-to-speech work remains separate
and should use the documented Realtime conversation model family.
Refs: crates/qsf_app/src/audio/transcript_provider.rs,
docs/Plans/Plan.FrameworkMVP.md,
docs/Experiments/Experiment.StreamingTranscriptionMVP.md,
https://developers.openai.com/api/docs/guides/realtime-transcription,
https://developers.openai.com/api/docs/models/gpt-realtime-whisper

## 2026-05-14 - Realtime voice providers cannot execute tools directly
Decision: Realtime voice-session providers are side-effect adapters. Provider tool-call
requests are recorded as QSF `ToolRequested` events with automatic execution disabled
until the QSF tool permission boundary explicitly handles them.
Context: Phase 10 introduces full voice-session provider events, including possible
function/tool requests from realtime models. The project needs voice-native behavior
without letting provider sessions bypass reducers, memory rules, or tool permissions.
Consequences: Realtime voice providers may report requested tool calls, but they must
not invoke tools or mutate runtime state directly. Voice-session experiments stay
explicitly selected and remain observable through QSF events and traces.
Refs: crates/qsf_app/src/audio/voice_session_provider.rs,
crates/qsf_app/src/experiments/realtime_voice_session.rs,
docs/Experiments/Experiment.RealtimeVoiceSessionMVP.md

## 2026-05-14 - Voice answers retrieve memory before context assembly
Decision: Text-owned voice responses retrieve memory after final speech becomes
`InputReceived` and before `ContextAssemblyRequested`.
Context: Slice 5 showed that the text-owned voice loop proved QSF response ownership
but only used fixed runtime context. The next slice needed memory participation without
letting audio providers own answer content or bypass the existing context budget.
Consequences: Voice turns now reuse the existing association-weighted memory retrieval
path and log `MemoryRetrievalRequested`/`MemoryRetrieved` before selected fragments are
passed to `ConversationalResponder`. The first slice selects one memory candidate so
the three voice-boundary fragments remain inside the four-fragment context budget.
Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md,
docs/Experiments/Report.VoiceLoopComparison.2026-05-14.md

## 2026-05-14 - Voice-loop latency reports include model runtime
Decision: Text-owned voice-loop latency totals include transcript dispatch, memory
retrieval, context assembly, model-role runtime, and speech output.
Context: The first live memory-context run showed successful answer ownership but the
generated report undercounted total turn latency by omitting the OpenAI model-call
duration. That made comparisons against provider-owned realtime voice misleading.
Consequences: Generated reports and latency events now expose each stage separately
and use a total observed turn latency that includes model runtime. Future comparison
reports should use those corrected fields.
Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md,
docs/Experiments/Report.VoiceLoopComparison.2026-05-14.md

## 2026-05-15 - Voice memory source is explicit and opt-in
Decision: The text-owned voice loop loads memory through a `VoiceLoopMemorySource`
boundary. The deterministic Phase 4 fixture remains the default, and file-backed memory
is selected explicitly with `QSF_VOICE_MEMORY_SOURCE=file` and `QSF_VOICE_MEMORY_FILE`.
Context: Live voice turns proved that memory retrieval can participate in the answer
path, but the toy fixture made retrieval quality arbitrary for real spoken prompts.
The next step needed a more grounded source without making normal tests depend on
ambient files or prior runs.
Consequences: Deterministic tests and default runs stay stable. File-backed voice
memory can be evaluated deliberately, and every run records the loaded source in
`voice-memory-source.json` plus generated diagnostics.
Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
crates/qsf_app/src/memory/fixtures.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md

## 2026-05-15 - Self-reflection can use project introspection as perception
Decision: Explore self-reflection by letting selected model roles inspect project
documentation, experiment artifacts, traces, and eventually source code through
read-only, on-demand introspection tools.
Context: The project documentation and implementation are part of the system's
evolving research record, but loading the whole repository into every live context
would violate the context-budget discipline and blur the boundary between reflection
and autonomous development agency.
Consequences: The first useful shape is documentation-only introspection for an
offline reflection role, with source inspection delayed until retrieval, permissions,
and observability are proven. Introspection results should enter context as compact
observations with references, not raw repository dumps.
Refs: docs/Plans/Idea.SelfReflectionProjectIntrospection.md,
docs/Concepts/Concept.ToolsAsPerception.md,
docs/Architecture/Architecture.ContextManagement.md

## 2026-05-15 - Volition is an explicit research surface
Decision: Explore a goal or volition system as inspectable simulation state that can
create internal initiative, such as revisiting open questions, requesting reflection,
or proposing experiments.
Context: Existing docs mention goals, tensions, motivational models, and research
planning, but they do not yet separate simulated initiative from uncontrolled external
agency. Human biological drives such as survival and reproduction are not the right
default for this project.
Consequences: Early goal-system work should avoid specifying final goals and should
start with read-only, observable fixtures that influence attention, reflection, and
proposals only. The introspection mechanism should be able to inspect active goals
and explain how they affected behavior.
Refs: docs/Plans/Idea.VolitionGoalSystem.md,
docs/Plans/Idea.SelfReflectionProjectIntrospection.md,
docs/ProjectFrame/NonGoals.md

## 2026-05-15 - Experiment artifacts use stable behavior names
Decision: Generated experiment summaries, report titles, and boundary descriptions use
stable behavior names instead of temporary phase numbers.
Context: A review of `crates/qsf_app/src/experiments/` found that milestone labels
were leaking into runtime artifacts and tests, making reports more likely to rot as
the roadmap evolves.
Consequences: Reports and outcome summaries now describe the behavior under test.
Shared constructors cover the transcript-provider runtime boundary, failure recorders
emit consistent sanitized events and engine logs, and timing conversions use one
saturating helper surface.
Refs: crates/qsf_app/src/experiments,
crates/qsf_app/src/audio/mod.rs,
crates/qsf_app/src/observability/trace.rs

## 2026-05-16 - Sleep-to-memory conversion is explicit and separate
Decision: Sleep reports may be converted into file-backed memory drafts only through an
explicit conversion command or experiment that writes a separate run directory; sleep
summarization and live voice turns do not promote memory implicitly.
Context: Reviewed memory promotion needs a bridge from provisional sleep output to
voice-loop memory without weakening the manual review boundary.
Consequences: Conversion artifacts remain inspectable before acceptance, source sleep
runs are left unchanged, and the text-owned voice loop only uses converted memory when
configured through the explicit file-backed memory source.
Refs: crates/qsf_app/src/experiments/reviewed_memory_draft.rs,
crates/qsf_app/src/memory/reviewed_memory_draft.rs,
docs/Plans/Plan.ReviewedMemoryPromotion.md

## 2026-05-17 - Multi-turn warm tier ages by active turn count
Decision: The multi-turn text loop warm tier ages the oldest active verbatim turns by
turn count, keeps completed `Turn` records append-only, stores session-local summaries
as an append-only prefix, and uses the summarizer role's default model unless a future
configuration point explicitly changes it.
Context: Stage 2 needed a concrete summarization trigger before token-pressure
heuristics exist, and review found that silently reusing the conversational model made
the summarizer role default meaningless.
Consequences: `QSF_SESSION_WARM_THRESHOLD` controls active verbatim turns only; aged
turns remain available in session records and reports but are skipped during prompt
assembly. Summary model changes should happen through role defaults or a deliberate
summary-model configuration variable, not by inheriting the responder model implicitly.
Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/models/model_role.rs

## 2026-05-17 - Multi-turn recall is scoped to summarized turns
Decision: The multi-turn text loop's `recall_turn` tool may return verbatim text only
for turns that have aged into warm summaries.
Context: Active verbatim turns are already present in the prompt. The recall tool exists
to recover older detail without permanently inflating every request, so allowing active
turn recall would add token cost without extending continuity.
Consequences: Recall execution validates that the requested `turn_id` is summarized
before returning verbatim text. Future wider recall behavior should be introduced as a
deliberate policy change, not as an implicit side effect of tool plumbing.
Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
docs/Plans/Plan.MultiTurnTextLoop.md

## 2026-05-17 - Stage 3.1 bypasses openai_provider_kit for tool-capable requests
Decision: Stage 3.1 of the multi-turn text loop writes OpenAI-specific tool-capable
HTTP request/response handling directly in `qsf_app` rather than extending
`openai_provider_kit`.
Context: The kit (pinned at `ca28629`) has no tool support at any layer — `LlmRequest`
lacks a `tools` field, `ChatMessage` and `ChatRole` have no `Tool` variant or
`tool_call_id`, the wire-format structs omit `tools`/`tool_choice`, and response parsing
ignores `tool_calls` entirely. Adding tool support would touch 4 of 5 source files in
the crate. The kit itself is a thin reqwest wrapper (~200 lines of meaningful code).
Forking, modifying most of the crate, and maintaining a fork is more overhead than
writing the OpenAI-specific serialization directly in `qsf_app`.
Consequences: `qsf_app` gains a new module (e.g., `models/openai_tool_client.rs`) that
handles tool-capable Chat Completions requests. Existing non-tool OpenAI requests
continue through the kit path unchanged. Auth, error mapping, and usage parsing are
duplicated in the new module for the tool path. Future kit upgrades or a migration to
the Responses API can replace the bypass module without affecting the provider-agnostic
model boundary.
Refs: crates/qsf_app/src/models/openai_provider.rs,
crates/qsf_app/Cargo.toml,
docs/Plans/Plan.MultiTurnTextLoop.md,
docs/Reviews/Review.Plan.MultiTurnTextLoop.Stage3.1.2026-05-17.md

## 2026-05-17 - Stage 3.1 uses Chat Completions, not Responses API
Decision: Stage 3.1 sends tool definitions and tool results through the Chat
Completions API (`/v1/chat/completions`), not the newer Responses API.
Context: OpenAI's Responses API is recommended for new projects but Chat Completions
is explicitly not deprecated and continues to be fully supported. The existing
`openai_provider_kit` and the non-tool OpenAI path already use Chat Completions.
Migrating to Responses would require changing both the tool and non-tool paths for
consistency, which is out of scope for Stage 3.1.
Consequences: Tool definitions use the `{"type":"function","function":{...}}` wrapper
shape. Tool results use `{"role":"tool","tool_call_id":"...","content":"..."}`.
`finish_reason: "tool_calls"` signals a tool call. A future migration to the Responses
API should be a separate phase that changes both paths together.
Refs: https://developers.openai.com/docs/guides/function-calling,
https://developers.openai.com/docs/guides/migrate-to-responses,
docs/Plans/Plan.MultiTurnTextLoop.md

## 2026-05-17 - allowed_tools on ModelRole is removed as unenforced
Decision: The `allowed_tools` field is removed from `ModelRole`. Tool authorization
is expressed solely through the tool list passed to `ModelRequest::with_tools()`.
Context: `allowed_tools` was set on the role in
`conversational_responder_role_with_recall_tool()` but never read or enforced anywhere
in the dispatch path. The actual tool list is always passed via
`ModelRequest.with_tools()`. An unenforced declaration is misleading and is technical
debt. If per-role tool authorization is needed later, it should be enforced at the
provider dispatch boundary with a clear error on mismatch.
Consequences: `ModelRole::allowed_tools` is deleted. The
`conversational_responder_role_with_recall_tool()` helper no longer sets it. No
behavior change — no code ever read the field. If enforcement is added later, it
belongs in `invoke_model_role` or the provider adapter, not as a passive annotation.
Refs: crates/qsf_app/src/models/model_role.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs:597-601,
docs/Plans/Plan.MultiTurnTextLoop.md

## 2026-05-17 - ToolContext uses typed borrowed-state accessors
Decision: Tool execution keeps a single `Tool` trait, and tools receive runtime
state through typed accessors on `ToolContext` such as `session_state()`, not through
`std::any::Any` downcasts.
Context: Migrating `recall_turn` into `ToolRegistry` showed that borrowed session
state cannot be downcast through `Any` because `Any` requires `'static`. A fake
`as_any` marker would make failed downcasts silent and misleading.
Consequences: New runtime state exposed to tools must be added deliberately to
`ToolContext` instead of being hidden behind untyped downcasts. Shared state types
needed by tools live outside experiment driver modules.
Refs: crates/qsf_app/src/tools/tool_registry.rs,
crates/qsf_app/src/tools/recall_turn_tool.rs,
crates/qsf_app/src/session/mod.rs,
docs/Reviews/Review.ToolSystemBridge.Phase3.md

## 2026-05-18 - allowed_tools is retained and enforced
Decision: `ModelRole.allowed_tools` is retained as the role-level allow-list for
model-callable tools and is enforced at the model tool-call dispatch boundary.
Context: This reverses the 2026-05-17 decision "allowed_tools on ModelRole is removed
as unenforced." That removal was recorded but never executed; its consequences already
identified dispatch-boundary enforcement as the right future home; and the role is the
natural source for declaring what a model may call, while `ModelRequest::with_tools()`
is request-local derived state.
Consequences: Production model requests derive advertised tool definitions from
`role.allowed_tools`, and model-emitted tool calls whose names are not listed by the
role fail before registry execution. Future roles that need tools must list them in
`allowed_tools`; future request builders must keep `ModelRequest.tools` in sync with
that declaration.
Refs: crates/qsf_app/src/models/model_role.rs,
crates/qsf_app/src/models/tool_dispatch.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
docs/DecisionLog.md#2026-05-17---allowed_tools-on-modelrole-is-removed-as-unenforced,
docs/Plans/Plan.ToolSystemBridge.md

## 2026-05-18 - Tool execution boundary is the ToolRegistry
Decision: All tool execution flows through `ToolRegistry`. `ModelToolDefinition` and
`ModelToolCall` describe only the model-facing wire shape and must be marshalled into
`ToolRequest` / `ToolResult` before a tool runs. `ModelRole.allowed_tools` composes
with `ToolPermission`; both must permit a call. Tool lifecycle events use
`ToolRequested` -> `ToolCompleted` / `ToolFailed`.
Context: The registry tools and model-call tool protocol previously evolved as
parallel surfaces: `recall_turn` bypassed the registry, `CalculatorTool` had no
model-facing schema, `ModelRole.allowed_tools` was unenforced, and `ToolCompleted` /
`ToolExecuted` described the same success moment. The realtime voice tool-boundary
decision from 2026-05-14 already depended on a registry-owned execution boundary.
Consequences: New tools land as `Tool` implementations and expose a
`ModelToolDefinition` when model-callable. Provider adapters parse model tool calls but
do not execute tools directly. Realtime voice and future provider paths route tool
requests through the same registry boundary before any runtime state or external
capability is touched.
Refs: crates/qsf_app/src/tools,
crates/qsf_app/src/models/tool_dispatch.rs,
crates/qsf_app/src/models/model_client.rs,
docs/DecisionLog.md#2026-05-14---realtime-voice-providers-cannot-execute-tools-directly,
docs/Plans/Plan.ToolSystemBridge.md
