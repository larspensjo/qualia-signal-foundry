# Decision log

Purpose: durable record of deliberate commitments — the source of truth for what the
project has agreed to do going forward.

## How to use
### How to add new entries
- One entry per decision. Decisions are commitments, not summaries of work.
- Implementation summaries and bug-fix postmortems belong in `EngineeringDiary.md`.
  A bug fix earns a decision-log entry only when it produces a durable rule, and the
  rule itself is the entry, not the fix.
- Reversals of prior decisions get their own entry referencing the original.
- A plan in itself, or change thereof, is not a decision until it is committed to.
- Keep entries concise and reference concrete artifacts.
- New entries go to the end of the file.

### When to add new entries
- Architecture commitments
- Technology or library choices
- Naming, structural, or coding conventions adopted project-wide
- Safety and scope boundaries
- Experiment outcomes promoted into accepted design
- Reusable rules derived from incidents
- The decisions are typically updated during planning, not during implementation (unless something unexpected happened).

### How to use the decision log during development
- Do not modify older entries if they were commited.

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
Refs: docs/Experiments/Experiment.StreamingTranscriptionMVP.md,
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
crates/qsf_app/src/experiments/streaming_transcription_mvp.rs

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
crates/qsf_app/src/memory/reviewed_memory_draft.rs

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
Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs

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
crates/qsf_app/Cargo.toml

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
https://developers.openai.com/docs/guides/migrate-to-responses

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
crates/qsf_app/src/experiments/multi_turn_text_loop.rs:597-601

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
docs/DecisionLog.md#2026-05-17---allowed_tools-on-modelrole-is-removed-as-unenforced

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
docs/DecisionLog.md#2026-05-14---realtime-voice-providers-cannot-execute-tools-directly

## 2026-05-18 - Model tool dispatch fails fast
Decision: Model tool dispatch returns an error as soon as any requested tool call fails,
even if earlier calls in the same batch completed successfully.
Context: Tool execution emits per-call requested, completed, and failed events, so partial
progress remains visible in observability artifacts. Returning partial results alongside
an error would require a new caller contract and could let a model continue from an
incomplete tool batch as if it were coherent.
Consequences: Callers must treat a failed model tool batch as failed. Any future partial
result behavior needs an explicit result type that distinguishes completed calls from the
failing call.
Refs: crates/qsf_app/src/models/tool_dispatch.rs

## 2026-05-20 - openai Cargo feature removed
Decision: The `openai` Cargo feature is removed from `qsf_app`. Real-provider
code for OpenAI Chat Completions, realtime transcription, and realtime voice
sessions compiles unconditionally. Provider selection at runtime remains explicit
via `QSF_MODEL_PROVIDER` / `QSF_TRANSCRIPT_PROVIDER` /
`QSF_VOICE_SESSION_PROVIDER` per the 2026-05-11 decision.
Context: The feature gate was an early hedge from when real-provider code was
experimental. It now adds CI complexity, hides drift behind a flag, and conflicts
with cross-session continuity work that touches code in previously feature-gated
paths. Removing the gate also unblocks the planned voice/text loop unification.
Consequences: `cargo build` / `cargo test` exercise the full path. API keys still
do not switch the runtime away from mocks; provider selection is the single
decision point.
Refs: crates/qsf_app/Cargo.toml, crates/qsf_app/src/models,
crates/qsf_app/src/audio,
docs/DecisionLog.md#2026-05-11---model-access-uses-explicit-roles-and-optional-provider-adapters

## 2026-05-20 - Text-loop continuity uses a manifest-backed state directory
Decision: The multi-turn text loop persists awake continuation under a gitignored
`state/text-loop/` directory by default. Boot mode is classified from the continuity
manifest plus the previous `SessionState`; `AwakeContinuation` resumes only when the
resume-breaking parts of `SessionConfig` still match the current run, otherwise the
loop downgrades to `ColdStart`. Runtime-only limit overrides such as
`allow_over_limit` do not break awake continuation. `ConsolidatedBrief` starts a new
session with a predecessor id until Stage 4 wires brief injection.
Context: Cross-session continuity needs state that survives per-run artifacts without
promoting every session detail into durable memory. A manifest provides a small commit
record for the current session state and later sleep outputs while keeping the reducer
pure and testable.
Consequences: Launching from a different process working directory creates a different
default `state/text-loop/`; operators can pin continuity with `QSF_STATE_DIR`. Resume
decisions are visible through `SessionResumed` events, including config-drift downgrades.
Stage 4 sleep must update or clear sleep metadata as it consumes the manifest.
Changing runtime limit overrides can continue the same awake session while recomputing
limit state against the new run configuration.
Refs: crates/qsf_app/src/session, crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/observability/event_log.rs

## 2026-05-20 - Sleep auto-promotes routine memory candidates
Decision: Sleep promotes `SleepReport.memory_candidates` into the cross-session
memory store as `Observation` records automatically, with structural validation and
normalized-string deduplication. `SleepReport.decision_candidates` are emitted as a
`ReviewedMemoryDraft` with `kind = Decision` for manual review through the existing
reviewed-memory workflow. This refines the 2026-05-16 decision: explicit review
remains the boundary for high-stakes `Decision` records, while routine observations
flow through automatically so cross-session continuity is observable in normal use.
Context: The explicit-only conversion boundary protected manual review, but it also
made ordinary continuity invisible unless the operator ran a separate promotion path.
`Design.CrossSessionContinuity.md` keeps the manual boundary for decisions while
letting routine memory candidates become inspectable, persisted observations. Retrieval
decay is time-based from `last_reinforced_at`, with a 30-day half-life as the starting
default.
Consequences: Sleep writes through a commit protocol where the manifest is the last
file written, so idempotent re-execution can recover from partial writes. Live loops
can retrieve the resulting memory store, reinforce retrieved memories, and form or
strengthen co-retrieval associations. Future memory categories that carry decision or
preference weight should choose explicitly between automatic observation promotion and
manual reviewed-memory promotion.
Refs: docs/Plans/Design.CrossSessionContinuity.md,
docs/Plans/Plan.CrossSessionContinuity.md,
crates/qsf_app/src/sleep/auto_promote.rs,
crates/qsf_app/src/sleep/commit.rs,
crates/qsf_app/src/memory/co_retrieval.rs,
docs/DecisionLog.md#2026-05-16---sleep-to-memory-conversion-is-explicit-and-separate

## 2026-05-20 - Post-hoc browser tools use Rust backend + browser frontend split
Decision: Browser-based post-hoc inspection tools (starting with the Memory Association
Browser) are served by a dedicated Rust crate, `qsf_browser_server`, that owns file
access, persisted-format ownership, schema validation, derived data, and the
visualization DTO contract. The TypeScript/Vite/PixiJS frontend consumes that
visualization API only and never reads persisted JSON files directly. Memory record,
association, and store-loading types are extracted from `qsf_app` into a shared
`qsf_memory` crate that both `qsf_app` and `qsf_browser_server` depend on. The Live
Activation Dashboard remains a separate concern and will be served by `qsf_app`
itself when implemented, because LAD needs real-time data from the running simulation.
Context: `docs/RustBackendBrowserFrontend.md` proposed keeping Rust as the semantic
layer for browser visualizations to avoid duplicating domain semantics in TypeScript
and to keep the UI from coupling to internal storage details. Review of
`Design.MemoryAssociationBrowser.md` flagged that depending on all of `qsf_app` would
drag model providers, audio providers, and experiments into a post-hoc inspection
binary, so memory types are extracted to a shared crate before the browser server is
built. The MAB and LAD have different latency and coupling constraints; sharing one
backend would conflate sealed-artifact inspection with live runtime observability.
Consequences: `qsf_browser_server` depends on `qsf_memory` (not `qsf_app`) for store
loading. New post-hoc inspection tools that read sealed artifacts may add route
groups to `qsf_browser_server`. Live observability tools must not be added to
`qsf_browser_server`; they belong in `qsf_app`. The TypeScript frontend defines DTO
type mirrors but does not own persisted-format knowledge.
Refs: docs/Plans/Design.MemoryAssociationBrowser.md,
docs/Plans/Idea.MemoryAssociationBrowser.md,
docs/Plans/Design.LiveActivationDashboard.md,
docs/RustBackendBrowserFrontend.md

## 2026-05-22 - PowerShell launcher is the Windows development entry point
Decision: Windows local-development documentation presents `scripts/qsf.ps1` as the
happy path for common launches, while raw Cargo and npm commands remain documented as
fallback and debugging references. Checked-in, non-secret launcher profiles live under
`scripts/`, and argument completion is opt-in by dot-sourcing
`scripts/qsf-completion.ps1`. The launcher parameter for selecting a profile is
`-LaunchProfile`; `-Profile` is only a compatibility alias.
Context: Starting experiments, the browser API, and the Vite UI required repeated
Cargo, npm, and environment-variable setup. A thin PowerShell entry point now makes
defaults and profile environment changes visible without changing the Rust CLIs or
mutating the caller's shell permanently.
Consequences: New Windows operator workflows should prefer extending the launcher when
they compose existing commands, but underlying binaries must stay independently
runnable and documented. Completion setup must not silently edit user shell profiles.
Refs: scripts/qsf.ps1, scripts/qsf.profiles.json, scripts/qsf-completion.ps1,
README.md, docs/Plans/Plan.PowerShellLauncher.md

## 2026-05-22 - Sleep auto-promotes candidate associations
Decision: Sleep automatically promotes valid `SleepReport.association_candidates` into
the durable memory store when both endpoint memory candidates were promoted in the same
sleep commit. Association candidates do not require a human review step; invalid links
will be handled by future reinforcement and decay policy rather than by manual
approval.
Context: A real sleep run over persisted session state produced a useful association
candidate between newly promoted identity memories, but the commit path silently dropped
it because only co-retrieval associations were persisted. The intended behavior is for
sleep to shape the association graph directly.
Consequences: Sleep-generated links can immediately affect memory browsing and
association-weighted retrieval. The decay algorithm for weak or invalid associations
remains an open design point and must be handled separately. Decision candidates remain
outside this rule.
Refs: crates/qsf_app/src/sleep/auto_promote.rs,
crates/qsf_app/src/experiments/sleep_phase_session_summary.rs,
docs/DecisionLog.md#2026-05-20---sleep-auto-promotes-routine-memory-candidates

## 2026-05-23 - Durable associations require present endpoints
Decision: Sleep commit writes durable associations only when both endpoint memory IDs
exist in the destination memory store. Co-retrieval IDs from session context that are
not present in the current store are ignored rather than persisted as broken graph
edges.
Context: A sleep run over `state/qa-memory-browser-real` correctly promoted a
candidate association between newly created sleep memories, but also wrote 25
co-retrieval associations to fixture-style IDs that were visible in the prior session
context and absent from the destination store.
Consequences: Memory browser counts, graph rendering, retrieval, and future decay work
operate on associations whose endpoints are real store records. If a future workflow
wants associations to external or archived memories, it must introduce an explicit
reference model rather than reusing durable in-store associations.
Refs: crates/qsf_app/src/sleep/auto_promote.rs,
state/qa-memory-browser-real/memory-store.json

## 2026-05-24 - Launcher text-loop runs avoid demo memory by default
Decision: `scripts/qsf.ps1 app -Experiment multi-turn-text-loop` passes an empty
file-backed session-memory fixture unless the caller explicitly selects demo/fixture
memory; the text loop still resumes from a persisted `state/text-loop/memory-store.json`
when that store exists.
Context: A fresh text-loop state still retrieved project-memory records because the
Rust experiment's fallback source is the deterministic Phase 4 fixture. That is useful
for repeatable demos but surprising for launcher-driven manual testing of a new
session.
Consequences: Local Windows launcher runs model "new session" as empty memory by
default. Demo retrieval remains available through `-DemoMemory`,
`-SessionMemorySource fixture`, or the `demo-memory` launch profile. Raw Cargo runs
still exercise the experiment's in-code fallback unless configured separately.
Refs: scripts/qsf.ps1, scripts/qsf.profiles.json, README.md,
docs/Experiments/Fixtures/session-memory.empty.json

## 2026-05-25 - Zero-signal memories are not retrieved by default
Decision: Keyword/tag live retrieval does not select durable memories that have no
query keyword, tag, association, or explicit profile/identity relevance signal.
Only selected memories are eligible for live reinforcement.
Context: A live QA run showed that a high-importance assistant-name memory could be
retrieved and reinforced on unrelated volition turns because retrieval always selected
the top scored records, even when score came only from importance and recency.
Consequences: `RetrievalResult.omitted` now includes relevance-gated records with
`RetrievedMemory.skip_reason`, traces can explain why a candidate was skipped, and
reinforcement events report relevance, over-limit, and no-store skipped ids.
Profile/identity queries keep a narrow phrase-shaped allowance for identity-tagged
memories.
Refs: crates/qsf_app/src/memory/retrieval.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
docs/Architecture/Architecture.StateAndObservability.md

## 2026-05-26 - Default sleep runs execute full side effects without synthetic memory
Decision: `sleep-phase-session-summary` keeps the normal sleep commit path active
for the default command, including continuity brief writes, processed-range
tracking, cross-turn association persistence, and promotion of any memory
candidates present in the sleep report. The deterministic mock sleep summarizer
must not emit fabricated memory candidates.
Context: The mock sleep fixture had a static memory candidate about model-role
events. Because the normal sleep commit path promotes routine memory candidates,
that fixture output became a durable memory even when it was not grounded in the
actual prior session.
Consequences: The default command remains a full sleep-session exercise, while
the mock provider no longer injects fake memories. Real or custom providers can
still produce memory candidates that flow through the existing promotion path.
Refs: crates/qsf_app/src/models/mock_model.rs,
crates/qsf_app/src/experiments/sleep_phase_session_summary.rs

## 2026-05-27 - Live/sleep split for association work
Decision: Mechanical association work — drop-driven and session-end co-retrieval
edges — runs in the live loop. Sleep hosts pluggable proposers for non-obvious
associations, exposed through a `SleepAssociationProposer` interface. The sleep
prompt is reworded accordingly to target non-obvious connections rather than
mechanical co-occurrence.
Context: Before this split, sleep duplicated cross-turn co-retrieval work the
live loop could already do deterministically, and the sleep prompt asked the
model for associations it had no advantage producing. Phase 5 of
`Plan.AssociativeRecallAndDropDrivenAssociations.md` moved the mechanical work
into the live loop and introduced the proposer interface with two initial
proposers (`LlmCandidateProposer`, `SafetyNetCoRetrievalProposer`).
Consequences: Mechanical association edges land deterministically without
waiting for sleep; sleep work focuses on signals the model is actually suited to
provide. New proposer ideas must enter through `Ideas.AssociationProposers.md`
with a measurable signal before promotion. The sleep prompt rewording is part of
this same commitment, not a separate decision.
Refs: crates/qsf_app/src/sleep/proposers/llm_candidate.rs,
crates/qsf_app/src/sleep/proposers/safety_net_co_retrieval.rs,
docs/Architecture/Architecture.SleepPhase.md,
docs/Plans/Plan.AssociativeRecallAndDropDrivenAssociations.md,
docs/Plans/Ideas.AssociationProposers.md

## 2026-06-03 - Shared session directory is the continuity root
Type: Decision
Decision: The multi-turn text loop, text-owned voice loop, and peer `voice-loop`
surface default to the shared `state/session/` continuity root. Legacy
`state/text-loop/` state remains a read-only fallback for continuity and is never
rewritten in place.
Context: Phase 6 moved the text loop onto the shared resolver so voice and text
runs continue one session by default rather than splitting into separate
continuity universes.
Consequences: New cross-session state should land in `state/session/`; any future
directory change needs explicit compatibility handling and a read-only fallback
story for existing `state/text-loop/` artifacts.
Refs: crates/qsf_app/src/session/state_directory.rs,
crates/qsf_app/src/session/resume.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
crates/qsf_app/src/experiments/voice_loop.rs

## 2026-06-06 - Provider preambles stay out of promotable sleep memory
Decision: Realtime provider preambles and provider lifecycle metadata may be persisted
for observability, but provider preamble text must not enter QSF prompt assembly,
sleep summarizer input, memory candidates, future context hints, or consolidated
brief summaries. Sleep may surface aggregate provider diagnostics in non-promotable
report fields.
Context: Realtime voice sessions now persist provider preambles alongside finalized
transcripts and response lifecycle facts in shared exchange records. Sleep also reads
voice exchanges for consolidation, so the provider/QSF ownership boundary needs to
survive the handoff into memory and next-session briefs.
Consequences: Finalized user transcripts and completed responses can be consolidated
through sleep, but provider-authored preamble text remains diagnostic-only. Future
provider-owned cognition experiments must opt in explicitly and cannot rely on the
default shared sleep path to promote provider preambles.
Refs: crates/qsf_app/src/session/sleep_records.rs,
crates/qsf_app/src/experiments/sleep_phase_session_summary.rs,
docs/Architecture/Architecture.AudioLoop.md,
docs/Architecture/Architecture.SleepPhase.md

## 2026-06-07 - Experiment runner supplies workspace root
Decision: Experiments that need repository-relative resources receive the
workspace root through an explicit experiment-runner `--workspace-root` option,
which is canonicalized into `RunContext` and exposed by accessor. Production code
must not derive the live workspace root from `CARGO_MANIFEST_DIR` or the process
current directory.
Context: The project-doc introspection channel needs absolute paths for its
repo-root search corpus and allowlist, and the existing runtime context only
tracked per-run artifact paths. The launcher already knows the repository root,
so the runner boundary is the appropriate place to pass it explicitly.
Consequences: Launcher-backed app runs pass the script-derived repo root; direct
CLI runs of workspace-dependent experiments must pass `--workspace-root <path>`.
Future repo-relative runtime resources should reuse the `RunContext` accessor
rather than adding ad hoc path resolution.
Refs: crates/qsf_app/src/cli.rs,
crates/qsf_app/src/experiments/registry.rs,
crates/qsf_app/src/runtime/run_context.rs,
scripts/qsf.ps1

## 2026-06-07 - Session ageing lives under session
Decision: Warm-turn summarization retries, token-budget ageing, cross-turn
co-retrieval persistence, and session-end flush behavior belong to
`crate::session::ageing` rather than the multi-turn text experiment.
Context: Phase 3 needed one shared ageing boundary so the text loop and future
session-owned callers can share the same policy and side effects while reducers
stay pure and emit `SessionEvent`s.
Consequences: Ageing policy changes should land in `session/ageing.rs`; the
experiment should only orchestrate inputs, outputs, and shared ageing calls.
Future voice or session surfaces that need the same ageing behavior should call
the shared module instead of copying the text-loop implementation.
Refs: crates/qsf_app/src/session/ageing.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/experiments/text_owned_voice_loop.rs

## 2026-06-07 - Project-doc introspection v1 scope
Decision: Project-doc introspection v1 is framed-self only, exposed to the
`ConversationalResponder` role only, with no source-code access, no write effects,
and a default allowlist that excludes `docs/Reviews/**` and
`docs/EngineeringDiary.md`.
Context: Self-reflection design and implementation planning narrowed the first
live introspection channel to read-only project documentation so the responder
can ground self-questions without broad repository access or autonomous
development agency.
Consequences: Active-self, episodic-self, pattern-self, meta-memory, source-code,
write-capable, and non-live-role introspection are deferred to follow-on designs.
Refs: docs/Plans/Design.ProjectDocIntrospection.md,
docs/Plans/Plan.ProjectDocIntrospection.md, config/project-doc-introspection.toml

## 2026-06-08 - Human inspiration is not a human ceiling
Decision: Qualia Signal Foundry uses human cognition as an inspiration and
comparison point, but the simulated mind may deliberately include non-human or
super-human capabilities when they serve the research goal.
Context: The project aims to simulate consciousness-like behavior while also
exploring capacities that biological humans do not have, such as exact temporal
awareness, broader memory access, faster reflection, parallel cognitive roles,
and structured self-observation.
Consequences: Designs should state whether a capability is human-like,
non-human, or super-human. Super-human capabilities should be represented as
explicit, inspectable signals, state, tools, model roles, or traceable processes
rather than hidden shortcuts. Human limitations should be simulated only when
they create useful presence, continuity, or research contrast.
Refs: README.md, docs/ProjectFrame/ProjectVision.md,
docs/ProjectFrame/NonGoals.md

## 2026-06-08 - Launcher owns non-secret QSF environment
Decision: `scripts/qsf.ps1` clears all known and ambient non-secret `QSF_*` process
variables from launched app child processes before applying launcher defaults,
explicit launcher flags, and selected profile values. Secret-like variables, including
API keys, remain inherited and are checked only as profile prerequisites.
Context: Local runs could change behavior based on stale `QSF_*` variables left in the
operator shell, which made the launcher less reproducible than its documented defaults.
Consequences: Launcher-backed app runs are deterministic with respect to non-secret
QSF runtime configuration. New non-secret app environment knobs should be added to the
launcher-managed list or exposed as launcher flags/profiles; raw Cargo runs remain free
to use ambient environment variables directly.
Refs: scripts/qsf.ps1, scripts/qsf.Tests.ps1, README.md

## 2026-06-09 - Lean session crate owns pure session contracts
Decision: A lean `qsf_session` crate will own the shared pure session surface:
session events, live-session reducer/state, `Exchange`, persistence DTOs,
continuity manifest, and the per-`Exchange` provider event records/kinds.
`RunContext`, provider clients, memory retrieval, tools, OpenAI dependencies,
CPAL dependencies, and the run-log `EventType` taxonomy stay outside that crate.
Context: The realtime voice server needs reducer and persistence contracts without
pulling the full `qsf_app` runtime or audio/model provider graph into a live
server crate.
Consequences: Session extraction is a behavior-preserving refactor. `qsf_app`
may re-export the session surface, but provider, memory, tool, and runtime context
dependencies must cross explicit adapter boundaries. `EventType` remains in
`qsf_app`; `qsf_session` owns the provider-event records embedded in `Exchange`.
Refs: docs/Plans/Design.RealtimeVoiceConversation.md,
docs/Architecture/Architecture.RealtimeSessionServer.md

## 2026-06-09 - Browser realtime voice uses a dedicated live server
Decision: Browser-based realtime voice uses a dedicated `qsf_realtime_server`
crate for live side effects. The browser owns the WebRTC media plane. The QSF
server owns ephemeral-token minting, SDP rendezvous, and the
`{qsf_session_id <-> provider call_id}` binding. `qsf_browser_server` remains a
read-only post-hoc inspection server.
Context: The existing browser server intentionally avoids live runtime side
effects, while the realtime voice plan requires credential handling, SDP proxying,
provider call binding, reducer access, and later sideband/tool control.
Consequences: Live realtime routes must not be added to `qsf_browser_server`.
Browser media can flow directly to the provider, but all credentials and session
bindings remain server-side and observable through QSF events/traces.
Refs: docs/Architecture/Architecture.RealtimeSessionServer.md,
docs/Plans/Design.RealtimeVoiceConversation.md,
https://developers.openai.com/api/docs/guides/realtime-webrtc

## 2026-06-09 - Browser-relayed realtime events are diagnostic until sideband authority
Decision: Phase-2 browser-relayed realtime provider events are untrusted,
diagnostic-only facts. They may be persisted for inspection, but they are excluded
from sleep consolidation, continuity promotion, and durable memory. Trusted live
voice exchanges begin when the Phase-3 server-side sideband becomes the
authoritative event source.
Context: The browser can observe useful media/session events, but it is not an
authoritative source for provider facts. The server-side sideband can attach to
the same realtime call via `call_id` and observe/control the session from the
server boundary.
Consequences: Event records and exchanges need an explicit trust/source marker.
Sleep and continuity code must filter diagnostic browser-relay records. The
browser relay can prove UI, media, and reducer wiring without changing durable
memory.
Refs: docs/Architecture/Architecture.RealtimeSessionServer.md,
docs/Plans/Design.RealtimeVoiceConversation.md,
https://developers.openai.com/api/docs/guides/realtime-server-controls

## 2026-06-09 - Realtime browser voice MVP defaults
Decision: The first browser realtime voice MVP uses `gpt-realtime-2`, voice
`marin`, `reasoning_effort = medium`, `output_modalities = ["audio"]`, and
provider `server_vad` with automatic response creation and interruption enabled.
The browser client secret lifetime is governed by provider-returned
`client_secret.expires_at`. The provider `call_id` binding is active-call scoped,
invalidated on stop/error/expiry, and retained only for a short diagnostic cleanup
grace.
Context: The project needs concrete defaults so Phase 2 can exercise the new code
path by default. Current OpenAI docs identify `gpt-realtime-2` as the most capable
realtime voice model, recommend `marin`/`cedar` for voice quality, expose
`server_vad` for turn detection, and provide `expires_at` for client secrets.
Consequences: Phase-2 tests and manual verification should expect these defaults.
Changing model, voice, VAD mode, or binding lifetime later requires an explicit
decision or provider-drift note rather than an incidental implementation change.
Refs: docs/Plans/Design.RealtimeVoiceConversation.md,
https://developers.openai.com/api/docs/models/gpt-realtime-2,
https://developers.openai.com/api/reference/resources/realtime/subresources/client_secrets

## 2026-06-09 - Realtime provider event mapping is identity-explicit
Decision: In speech-to-speech mode, a QSF exchange is a paired user
audio/transcript item and assistant response keyed by provider item/response ids.
`call_id` identifies the provider call, `event_id` supports deduplication and
trace correlation, `item_id`/`previous_item_id` reconstruct conversation order,
and `response_id` tracks assistant response lifecycle. Exchange completion must
carry an explicit QSF exchange identity, such as `exchange_index`, so overlapping
events cannot complete the wrong exchange.
Context: The existing reducer was adequate for a single-turn bridge, but
full-duplex provider events can overlap, arrive out of order, duplicate, or finish
after interruption.
Consequences: Phase 1 applies the completion identity change before provider
integration. Phase 2 must include reducer tests for out-of-order transcript
completion, duplicate provider events, interruption before `response.created`,
response completion after interruption, and two user turns before the prior
response finishes.
Refs: docs/Architecture/Architecture.RealtimeSessionServer.md,
docs/Plans/Design.RealtimeVoiceConversation.md,
crates/qsf_app/src/session/live_state.rs

## 2026-06-09 - Realtime tools are read-only and execution-recorded
Decision: Tools exposed to live realtime voice sessions are allow-listed and
read-only. Realtime model tool-call requests are recorded as `ToolRequested`, but
that request is not execution evidence. QSF decides permission, executes the tool
server-side, records permission/result/error/timing, returns a
`function_call_output` item to the provider, and resumes the response.
Context: Earlier realtime voice work deliberately prevented providers from
executing tools directly. The live sideband design now needs a positive execution
path without weakening the QSF permission and observability boundary.
Consequences: Do not overload `auto_executed` as proof of execution. Phase 4 must
prove both allowed read-only execution and denied non-allow-listed calls, with
records linked by provider `call_id` or tool-call id.
Refs: docs/Architecture/Architecture.ToolSystem.md,
docs/Plans/Design.RealtimeVoiceConversation.md,
crates/qsf_app/src/audio/voice_session_provider.rs

## 2026-06-09 - Realtime voice conversation is the target operating mode
Decision: Realtime voice conversation is the intended primary operating mode of
Qualia Signal Foundry. Named experiments, experiment reports, and
`qsf_app` experiment-runner paths are validation scaffolds for building and
measuring that mode, not the final category for the live runtime. When
`qsf_realtime_server` and the browser UI exist, `scripts/qsf.ps1` should expose a
first-class realtime conversation launcher path rather than only
`app -Experiment <name>`.
Context: The project has used `qsf.ps1` mostly to start tests, browser tools, and
named experiments. The realtime voice conversation plan represents the long-term
interaction goal of the project, so documentation and launcher design should not
frame it as merely another experiment.
Consequences: Plans may still create `Experiment.*` documents for verification,
but those docs are evidence and test harnesses. README, launcher documentation,
and future operator workflows should distinguish current experiment-centric
development from the target realtime conversation mode. The exact launcher command
name can be decided when the realtime server/UI entry point is implemented.
Refs: README.md, docs/ProjectFrame/ProjectVision.md,
docs/Plans/Plan.RealtimeVoiceConversation.md,
docs/Plans/Design.RealtimeVoiceConversation.md,
docs/Architecture/Architecture.RealtimeSessionServer.md, scripts/qsf.ps1

## 2026-06-09 - qsf_session extraction shipped with qsf_app compatibility wrappers
Decision: `qsf_session` owns the pure session contracts, including the live and
persisted state DTOs, reducer functions, exchange records, continuation/resume
classification, continuity manifest, sleep records, and the foundational context
and content-hash value types. `qsf_app` keeps the effectful launcher/runtime edge,
compatibility wrappers, and resume schema-upgrade logging.
Context: Phase 1 completed the crate extraction and the reducer completion identity
update. The resume loader also had to preserve the existing schema-upgrade log in
`qsf_app` while moving the file I/O and schema upgrade logic into `qsf_session`.
Consequences: Future crates such as the realtime server can depend on
`qsf_session` without the heavy `qsf_app` graph. `qsf_app` remains the thin facade
for existing call sites until later phases replace them.
Refs: crates/qsf_session/src/*, crates/qsf_app/src/session/*,
docs/Plans/Design.RealtimeVoiceConversation.md,
docs/Architecture/Architecture.StateAndObservability.md

## 2026-06-09 - Realtime WebRTC uses a server-side SDP exchange, not ephemeral tokens
Decision: `qsf_realtime_server` initializes the browser realtime call via a
server-side SDP exchange. The browser sends its SDP offer to the server; the server
POSTs it to the OpenAI realtime calls endpoint authenticated with the server-held
`OPENAI_API_KEY`, captures the provider `call_id` first-hand, stores the
`{qsf_session_id <-> call_id}` binding, and returns the SDP answer. No ephemeral
client secret is minted or returned to the browser, and no credential of any kind
leaves the server. Media (audio RTP) still flows directly browser<->OpenAI, so only
signaling is proxied and no media latency is added.
Context: Reverses the ephemeral-token portion of the two 2026-06-09 realtime entries
referenced below. Phase-2 planning review found the prior design fused two distinct
OpenAI flows (minting an ephemeral secret AND server-proxying the SDP), which is
internally inconsistent: an ephemeral secret exists to let the untrusted browser talk
directly to OpenAI, but the server was also proxying the exchange. The server-side
flow is the only one consistent with the declared trust boundary (the browser is
untrusted) and with the Phase-3 sideband, which attaches to the server-captured
`call_id`; a browser-reported `call_id` could not be authoritative.
Consequences: `POST /api/realtime/session` allocates a `qsf_session_id` and returns
only non-secret session config (it does not call OpenAI); `POST /api/realtime/sdp`
authenticates to OpenAI with the API key, not an ephemeral secret. `AppState` needs no
per-session client-secret store, and the provider-returned `client_secret.expires_at`
lifetime no longer applies. The exact `/v1/realtime/calls` endpoint, headers, `call_id`
location, and session-config schema must still be verified against the live API at
implementation time, recording drift before changing defaults. Fallback if the
server-side path cannot supply session config or return the `call_id` to the server:
the ephemeral-token flow with an explicitly browser-reported (untrusted) `call_id`
until the Phase-3 sideband validates it.
Reverses: "Browser realtime voice uses a dedicated live server" (ephemeral-token
minting) and "Realtime browser voice MVP defaults" (browser client-secret lifetime),
both 2026-06-09.
Refs: docs/Plans/Plan.RealtimeVoiceConversation.md,
docs/Plans/Design.RealtimeVoiceConversation.md,
docs/Architecture/Architecture.RealtimeSessionServer.md,
https://developers.openai.com/api/docs/guides/realtime-webrtc

## 2026-06-09 - Realtime browser UI lives under qsf_realtime_server/ui
Decision: The live browser surface for realtime voice conversation lives in a
dedicated Vite + TypeScript + Biome + Vitest project at
`crates/qsf_realtime_server/ui/`, separate from `qsf_browser_server/ui/`.
Context: The read-only browser server must stay decoupled from live voice
concerns, and the realtime server needs its own build and verification boundary.
Consequences: Launcher wiring, frontend checks, and UI assets for the realtime
slice target the dedicated crate-local UI directory instead of reusing the
inspection server UI.
Refs: crates/qsf_realtime_server/ui/*,
docs/Architecture/Architecture.RealtimeSessionServer.md

## 2026-06-09 - Phase-2 relay artifacts stay diagnostic-only and self-describing
Decision: Browser-relayed provider events are persisted only as untrusted
diagnostics outside the shared continuity root, and the diagnostic records carry
explicit source/trust markers plus the provider identity fields needed for
correlation and replay.
Context: Phase 2 intentionally keeps the browser relay untrusted. The same
artifacts must remain understandable when Phase 3 adds authoritative sideband
events, so the record shape needs to declare its trust level instead of relying
on storage location alone.
Consequences: Phase-2 relay events do not feed sleep consolidation or continuity
promotion. Diagnostic persistence must record `call_id`, `event_id`, `item_id`,
`previous_item_id`, and `response_id` alongside the exchange payload.
Refs: crates/qsf_realtime_server/src/diagnostics.rs,
crates/qsf_realtime_server/src/realtime/routes.rs,
crates/qsf_session/src/exchange.rs,
docs/Architecture/Architecture.StateAndObservability.md

## 2026-06-09 - Realtime reducer overlap finalizes the prior exchange
Decision: When a new user turn arrives before the previous response finishes,
the live reducer finalizes the prior exchange first, marks it interrupted if the
response was still streaming, and treats late lifecycle events for that exchange
as no-ops.
Context: Phase-2 speech-to-speech exchanges can arrive out of order and can be
interrupted mid-response. The single-active-exchange reducer must stay stable in
the face of duplicate or stale provider events.
Consequences: The live reducer keeps one active exchange at a time, suppresses
stale response ids after interruption or completion, and leaves provider event
`event_id` deduplication to the server translator boundary.
Refs: crates/qsf_session/src/live_state.rs,
crates/qsf_realtime_server/src/realtime/routes.rs

## 2026-06-09 - Provider drift: `reasoning_effort` is not forwarded to OpenAI realtime calls
Decision: The accepted Phase-2 session default `reasoning_effort = medium` is
kept as QSF session metadata (still returned to the browser in the allocation
response) but is **not** forwarded in the OpenAI `/v1/realtime/calls` session
object.
Context: First live verification of the server-side SDP exchange (Phase 2)
returned `400 unknown_parameter` for `session.reasoning_effort`. The realtime
calls session schema does not accept that field; `gpt-realtime-2` itself passed
schema validation. Recorded here per Plan decision 4 ("record any drift
explicitly before changing accepted defaults"). The SDP handler also now
surfaces the provider error body instead of swallowing it.
Consequences: `OpenAiRealtimeSessionRequest` drops `reasoning_effort`; a
regression test asserts it is absent from the forwarded body while the
browser-facing default keeps it. Remaining accepted defaults
(`gpt-realtime-2`/`marin`/`["audio"]`/`server_vad`) are unverified past this
point and may surface further drift on continued live testing.
Refs: crates/qsf_realtime_server/src/state.rs,
crates/qsf_realtime_server/src/realtime/routes.rs,
docs/Plans/Plan.RealtimeVoiceConversation.md

## 2026-06-09 - Live Activation Dashboard merges into the realtime operator page
Decision: The live activation dashboard and the live voice-conversation controls are one
web app at a single URL, split into two strictly separated planes — a control plane (the
conversation, side-effecting) and an observation plane (the dashboard, read-only,
one-way, and non-blocking: it never feeds an action back into the runtime and its failure
cannot affect the conversation). The signal projector and activation/decay state live in
TypeScript; Rust stays domain-pure, emitting domain events/traces (and, for live use, a
read-only one-way event stream) but never dashboard signals. Rust owns the event/trace
schema; the signal schema is a TypeScript-owned presentation contract. Offline replay
stays a separate mode of the same view, fed from sealed run artifacts.
Context: Phase 2 made a live browser voice session work, which already emits the live
event stream the dashboard's deferred live-tail phase needed; a researcher wanting to
"talk and watch the mind light up" should not juggle two pages. Keeping presentation
logic out of Rust preserves the project's control-versus-observation boundary and matches
the intended split (Rust internals, TypeScript presentation, future WebGL/3D on the GPU).
Consequences: The dashboard lives under `crates/qsf_realtime_server/ui/`, sharing the app
shell with the conversation controls; the realtime server gains a read-only event-stream
endpoint but emits no presentation signals. Live tail and offline replay share one
TypeScript projector over one event schema (deterministic, Vitest-tested). A blinded,
replay-only/metadata-only mode stays available to avoid live-dashboard experiment bias.
The read-only memory association browser remains a separate surface, foldable in later.
Refs: docs/Plans/Idea.LiveActivationDashboard.md,
docs/Plans/Design.LiveActivationDashboard.md,
docs/Architecture/Architecture.RealtimeSessionServer.md, crates/qsf_realtime_server/ui/

## 2026-06-10 - Realtime memory and protocol helpers live in lean shared crates
Decision: Retrieval scoring lives in `qsf_memory`, context assembly lives in
`qsf_context`, and realtime JSON builders/parsers live in
`qsf_realtime_protocol`. `qsf_session` depends on `qsf_context`, and
`qsf_realtime_server` depends on the lean domain crates instead of `qsf_app`.
Context: The realtime server needs shared memory/context/protocol logic without
dragging in the full app runtime. Keeping these helpers in lean crates preserves
the server boundary and allows the live sideband to reuse them directly.
Consequences: `qsf_app` keeps compatibility facades, but the source of truth for
retrieval, context assembly, and realtime protocol payloads now lives in the lean
crates. Future shared logic should follow the same dependency direction.
Refs: crates/qsf_memory/src/retrieval.rs, crates/qsf_memory/src/co_retrieval.rs,
crates/qsf_context/src/lib.rs, crates/qsf_realtime_protocol/src/lib.rs,
crates/qsf_session/src/context.rs, crates/qsf_app/src/context/mod.rs

## 2026-06-10 - Sideband uses the server-captured call_id websocket with bearer auth
Decision: The realtime sideband connects to
`wss://api.openai.com/v1/realtime?call_id=...` and authenticates with the
server-held `OPENAI_API_KEY` in the Authorization header.
Context: OpenAI's realtime server-controls guide documents the server-side
websocket attach path for an in-progress WebRTC call. This was verified against
the live docs during implementation to confirm the Phase-3 attach shape.
Consequences: The browser never receives a credential. The realtime server must
keep the key server-side, build the websocket URL from the captured `call_id`,
and treat any drift from this attach shape as a docs-updating event.
Refs: crates/qsf_realtime_server/src/realtime/sideband.rs,
docs/Architecture/Architecture.RealtimeSessionServer.md,
https://developers.openai.com/api/docs/guides/realtime-server-controls

## 2026-06-10 - Authoritative realtime sideband supersedes the browser relay
Decision: The server-side sideband attached to the server-captured `call_id` is
the authoritative trusted source for live realtime exchanges. The browser relay
remains diagnostic-only and must not feed continuity.
Context: Phase 3 introduces a server-owned websocket control plane that can
inject context before `response.create`, observe provider events first-hand, and
promote only trusted completed exchanges into the shared continuity root.
Consequences: Trusted sideband exchanges may be sleep/continuity eligible after
promotion; browser relay exchanges stay untrusted diagnostics outside the
continuity root. `qsf_realtime_server` stays on the lean
`qsf_session`/`qsf_memory`/`qsf_context`/`qsf_realtime_protocol` boundary and
does not depend on `qsf_app`.
Refs: crates/qsf_realtime_server/src/realtime/sideband.rs,
crates/qsf_realtime_server/src/realtime/routes.rs,
docs/Architecture/Architecture.RealtimeSessionServer.md,
docs/Architecture/Architecture.StateAndObservability.md,
docs/Architecture/Architecture.MemorySystem.md

## 2026-06-10 - Realtime per-turn injection disables automatic response creation
Decision: The Phase-3 realtime default is `server_vad` with
`turn_detection.create_response = false`, and the sideband owns `response.create`
timing after memory injection.
Context: Automatic provider response creation can start before the server has
retrieved relevant memory and injected the working context. Manual response
control is the required default for the control/context plane.
Consequences: Browser allocation responses, UI defaults, and server-side session
updates all use the manual-response path. Fast no-LLM injection should remain
latency-parity with the former automatic path, while slower injection paths stay
explicit.
Refs: crates/qsf_realtime_server/src/state.rs,
crates/qsf_realtime_server/ui/src/realtime.ts,
docs/Architecture/Architecture.RealtimeSessionServer.md,
docs/Architecture/Architecture.AudioLoop.md

## 2026-06-10 - Sideband gaps degrade transport trust until verified recovery
Decision: Any unrecoverable or potentially lossy sideband disconnect marks
transport trust degraded until the sideband reconnects and receives a
`session.updated` acknowledgement. Any exchange active during the gap is
permanently non-promotable, but later exchanges may promote after recovery.
Context: Once the sideband becomes authoritative, a disconnect can mean missed
provider events. With `create_response = false`, however, the sideband is the
only actor that can trigger an assistant response, so a reconnected and
configured sideband restores trust for fresh exchanges.
Consequences: Gap-window exchanges, incomplete exchanges, failed exchanges, and
per-exchange conversion defects are not promoted into the shared continuity
root. The browser relay is unaffected because it remains diagnostic-only.
Refs: crates/qsf_realtime_server/src/realtime/sideband.rs,
crates/qsf_realtime_server/src/realtime/routes.rs,
docs/Architecture/Architecture.RealtimeSessionServer.md,
docs/Architecture/Architecture.StateAndObservability.md

## 2026-06-10 - Phase-4 live tool scope is the three read-only perception tools
Decision: The Phase-4 realtime allow-list is exactly `search_memory`,
`get_associations`, and `inspect_session_state` — new read-only tools implemented
in `qsf_realtime_server`. No existing `qsf_app` tool is exposed to the live model
this phase. Long-term intent: the live model eventually gets the full tool set,
as its own later phase, once the required data services move past the
no-`qsf_app` boundary.
Context: External review of the Phase-4 plan flagged the allow-list scope as a
blocking product decision; exposing existing `qsf_app` tools live would require
either moving `ProjectDocService`/durable-session access into lean crates or
breaking the `qsf_realtime_server`-must-not-depend-on-`qsf_app` boundary. Owner
confirmed the three-tool scope 2026-06-10.
Consequences: Phase 4 proves the tool-loop machinery (permission decisions,
execution records, exchange boundary, credential hygiene) against server-owned
data only. The generic `qsf_tools` registry core is designed so the later
full-exposure phase is an additive change.
Refs: docs/Plans/Plan.RealtimeVoiceConversation.md,
docs/Plans/Review.RealtimeVoiceConversation.phase4.Plan.codex.json

## 2026-06-10 - Tool execution records persist onto durable turns
Decision: Live tool activity is recorded as a `ToolExecutionRecord` (permission
decision, status, budget-capped result summary, error, timing, per-response model
usage, linking provider `call_id`) and persists onto durable `Turn` records
behind serde defaults. `auto_executed` on `ToolRequestRecord` is not execution
evidence.
Context: The only durable record of a realtime conversation is the promoted
`Turn` list; live-only records would leave no artifact of what tools ran or were
denied (logs only), keep tool activity out of the read-only inspection surface,
and deprive Phase-5 extraction/ageing of provenance and usage signal. Owner
confirmed persistence 2026-06-10 after external review flagged it as a blocking
schema decision.
Consequences: The persisted session-state schema gains tool records behind
`#[serde(default)]`; the schema golden tests are updated and legacy artifacts
must still load. Result summaries are budget-capped — tool payloads are never
dumped into durable state.
Refs: crates/qsf_session/src/exchange.rs, crates/qsf_session/src/state.rs,
crates/qsf_session/tests/session_state_schema.rs,
docs/Plans/Plan.RealtimeVoiceConversation.md

## 2026-06-10 - Exchange model use aggregates across the realtime tool loop
Decision: For a turn containing model-invoked tool calls, the exchange's
`ExchangeModelUse` aggregates token counts and total latency across all
`response.done` events of the turn; each `ToolExecutionRecord` carries its own
per-response usage and timing; the request hash and message count reflect the
final spoken response's request sequence.
Context: With server-owned `response.create`, a tool loop produces multiple
provider responses per turn, but the sideband tracked a single
request-hash/message-count slot reset after each `response.done`. Recording only
the final response would silently under-report cost and latency for trusted
promotion and Phase-5 presence research.
Consequences: Trusted promotion carries full-turn usage; token/latency accounting
across a tool call is covered by tests; per-call detail lives on the execution
record rather than inflating exchange-level fields.
Refs: crates/qsf_realtime_server/src/realtime/sideband.rs,
docs/Plans/Plan.RealtimeVoiceConversation.md
