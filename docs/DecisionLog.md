# Decision log

Purpose: durable record of deliberate commitments — the source of truth for what the
project has agreed to do going forward.

## How to use
### How to add new entries
- One entry per decision. Decisions are commitments, not summaries of work.
- Implementation summaries and bug-fix postmortems belong in commits, pull requests,
  reports, or the project document whose current content changed.
- Reversals of prior decisions get their own entry referencing the original.
- Keep entries concise.
- New entries go to the end of the file.

### When to add new entries
- Architecture commitments
- Technology or library choices
- Naming, structural, or coding conventions adopted project-wide
- Safety and scope boundaries
- Experiment outcomes promoted into accepted design
- Reusable rules derived from incidents
- The decisions are sometimes updated during planning, not during implementation (unless something unexpected happened).
- Creating a plan isn't decision point if it was already obviously part of the project.

### How to use the decision log during development
- Do not modify older entries if they were commited.

## Entry Template
```
## YYYY-MM-DD - <decision title>
Decision: <the rule, in present tense>
Context: <why this was decided now>
Consequences: <what this constrains or implies going forward at a high level without referencing code>
```

## 2026-05-09 - Unidirectional event-reducer-state flow
Type: Decision
Decision: The runtime loop updates state exclusively through pure reducer functions of the
form (State, Event) → State. Side effects are isolated and fed back as events.
Context: Explainable state transitions, pure-function testability, and clean separation
between what happened (events) and what changed (state). Established in Agents.md and
mirrored in Architecture.RuntimeLoop.md.
Consequences: Side-effect-producing operations (model calls, tool invocations, I/O) must
not modify state directly. They must emit events that the reducer then processes.

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

## 2026-05-10 - Memory schema versioning is per record type and run artifacts are sealed
Decision: `MemoryRecord` and `Association` each carry an independent `schema_version: u16`
field from v1. The live runtime reads and writes only the current version. Past memory
artifacts are immutable and never migrated in place; versioned readers for older artifacts
live in a separate compatibility module used for replay and analysis. Pure additive
changes (new optional fields with serde defaults) do not bump the version; removed,
renamed, or semantically changed fields do.
Context: The memory-record implementation introduces durable memory records. The framework's replay
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

## 2026-05-11 - Model access uses explicit roles and optional provider adapters
Decision: Model invocations are expressed as typed `ModelRole` plus `ModelRequest` pairs and execute through a synchronous `ModelClient` boundary. The OpenAI-backed path remains an optional adapter over `openai_provider_kit` and is selected explicitly by configuration rather than automatically when `OPENAI_API_KEY` happens to be present.
Context: The model-role work needed deterministic experiments and a real OpenAI-backed path without forcing the whole runtime loop async or letting ambient environment variables silently change behavior.
Consequences: Mock and OpenAI clients share one provider-agnostic contract, model-role traces can stay uniform across providers, and future async or multi-provider work changes the adapter/effects boundary rather than every call site. Possessing an API key alone does not switch the runtime away from deterministic mock behavior.

## 2026-05-12 - Real audio providers remain explicit evaluation paths
Decision: Real streaming transcription inputs are selected explicitly through
`QSF_TRANSCRIPT_PROVIDER` and `QSF_TRANSCRIPT_INPUT_SOURCE`; the default path remains
deterministic simulation, and provider adapters report transcript metadata rather than
persisting raw audio.
Context: The realtime transcription work introduced OpenAI Realtime WebSocket transcription plus prerecorded
WAV and live microphone evaluation paths. Real audio depends on credentials, devices,
permissions, network behavior, and local recording conditions, so it should not activate
just because those capabilities are compiled in.
Consequences: Tests and normal experiment runs stay deterministic by default. WAV and
microphone evaluation are opt-in side-effect paths, and any future provider must preserve
the same no-secret/no-raw-audio observability boundary.

## 2026-05-13 - Feature-gated audio providers need explicit compile checks
Decision: Real-audio readiness includes compiling the `qsf_app/openai` feature
path, not only running the default simulated transcript tests.
Context: The default build kept deterministic streaming transcription tests green, but
the OpenAI realtime transcription adapter had drifted against current CPAL and
Tungstenite APIs. Real WAV and microphone smoke tests depend on this feature-gated path.
Consequences: Changes to optional audio adapters should include at least one targeted
`--features openai` compile or test pass before considering the phase ready for real
audio evaluation.

## 2026-05-13 - Realtime transcription optimizes for latency first
Decision: The OpenAI realtime transcription adapter defaults to `gpt-realtime-whisper`.
`gpt-4o-transcribe` remains an evaluation alternative for accuracy-sensitive runs.
Context: Live realtime transcription tests proved the provider boundary. A follow-up model review
rechecked the official OpenAI model catalog and realtime transcription guide, which
list `gpt-realtime-whisper` as the lowest-latency streaming transcription path for
live audio and transcript deltas. The project values realtime presence, so latency is
the first defaulting criterion for realtime transcription.
Consequences: `gpt-realtime-whisper` is the first provider-backed transcription
target. Accuracy comparisons should use explicit model selection rather than changing
the default away from the realtime path. Full speech-to-speech work remains separate
and should use the documented Realtime conversation model family.

## 2026-05-14 - Realtime voice providers cannot execute tools directly
Decision: Realtime voice-session providers are side-effect adapters. Provider tool-call
requests are recorded as QSF `ToolRequested` events with automatic execution disabled
until the QSF tool permission boundary explicitly handles them.
Context: The realtime voice-session work introduces full provider events, including possible
function/tool requests from realtime models. The project needs voice-native behavior
without letting provider sessions bypass reducers, memory rules, or tool permissions.
Consequences: Realtime voice providers may report requested tool calls, but they must
not invoke tools or mutate runtime state directly. Voice-session experiments stay
explicitly selected and remain observable through QSF events and traces.

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

## 2026-05-14 - Voice-loop latency reports include model runtime
Decision: Text-owned voice-loop latency totals include transcript dispatch, memory
retrieval, context assembly, model-role runtime, and speech output.
Context: The first live memory-context run showed successful answer ownership but the
generated report undercounted total turn latency by omitting the OpenAI model-call
duration. That made comparisons against provider-owned realtime voice misleading.
Consequences: Generated reports and latency events now expose each stage separately
and use a total observed turn latency that includes model runtime. Future comparison
reports should use those corrected fields.

## 2026-05-15 - Voice memory source is explicit and opt-in
Decision: The text-owned voice loop loads memory through a `VoiceLoopMemorySource`
boundary. The deterministic memory-and-context fixture remains the default, and file-backed memory
is selected explicitly with `QSF_VOICE_MEMORY_SOURCE=file` and `QSF_VOICE_MEMORY_FILE`.
Context: Live voice turns proved that memory retrieval can participate in the answer
path, but the toy fixture made retrieval quality arbitrary for real spoken prompts.
The next step needed a more grounded source without making normal tests depend on
ambient files or prior runs.
Consequences: Deterministic tests and default runs stay stable. File-backed voice
memory can be evaluated deliberately, and every run records the loaded source in
`voice-memory-source.json` plus generated diagnostics.

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

## 2026-05-16 - Sleep-to-memory conversion is explicit and separate
Decision: Sleep reports may be converted into file-backed memory drafts only through an
explicit conversion command or experiment that writes a separate run directory; sleep
summarization and live voice turns do not promote memory implicitly.
Context: Reviewed memory promotion needs a bridge from provisional sleep output to
voice-loop memory without weakening the manual review boundary.
Consequences: Conversion artifacts remain inspectable before acceptance, source sleep
runs are left unchanged, and the text-owned voice loop only uses converted memory when
configured through the explicit file-backed memory source.

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

## 2026-05-17 - Multi-turn recall is scoped to summarized turns
Decision: The multi-turn text loop's `recall_turn` tool may return verbatim text only
for turns that have aged into warm summaries.
Context: Active verbatim turns are already present in the prompt. The recall tool exists
to recover older detail without permanently inflating every request, so allowing active
turn recall would add token cost without extending continuity.
Consequences: Recall execution validates that the requested `turn_id` is summarized
before returning verbatim text. Future wider recall behavior should be introduced as a
deliberate policy change, not as an implicit side effect of tool plumbing.

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

## 2026-05-24 - Launcher text-loop runs avoid demo memory by default
Decision: `scripts/qsf.ps1 app -Experiment multi-turn-text-loop` passes an empty
file-backed session-memory fixture unless the caller explicitly selects demo/fixture
memory; the text loop still resumes from a persisted `state/text-loop/memory-store.json`
when that store exists.
Context: A fresh text-loop state still retrieved project-memory records because the
Rust experiment's fallback source is the deterministic memory-and-context fixture. That is useful
for repeatable demos but surprising for launcher-driven manual testing of a new
session.
Consequences: Local Windows launcher runs model "new session" as empty memory by
default. Demo retrieval remains available through `-DemoMemory`,
`-SessionMemorySource fixture`, or the `demo-memory` launch profile. Raw Cargo runs
still exercise the experiment's in-code fallback unless configured separately.

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

## 2026-05-27 - Live/sleep split for association work
Decision: Mechanical association work — drop-driven and session-end co-retrieval
edges — runs in the live loop. Sleep hosts pluggable proposers for non-obvious
associations, exposed through a `SleepAssociationProposer` interface. The sleep
prompt is reworded accordingly to target non-obvious connections rather than
mechanical co-occurrence.
Context: Before this split, sleep duplicated cross-turn co-retrieval work the
live loop could already do deterministically, and the sleep prompt asked the
model for associations it had no advantage producing. The associative-recall
proposer-interface work moved the mechanical work into the live loop and introduced the proposer interface with two initial
proposers (`LlmCandidateProposer`, `SafetyNetCoRetrievalProposer`).
Consequences: Mechanical association edges land deterministically without
waiting for sleep; sleep work focuses on signals the model is actually suited to
provide. New proposer ideas must enter through `Ideas.AssociationProposers.md`
with a measurable signal before promotion. The sleep prompt rewording is part of
this same commitment, not a separate decision.

## 2026-06-03 - Shared session directory is the continuity root
Type: Decision
Decision: The multi-turn text loop, text-owned voice loop, and peer `voice-loop`
surface default to the shared `state/session/` continuity root. Legacy
`state/text-loop/` state remains a read-only fallback for continuity and is never
rewritten in place.
Context: The shared-session resolver work moved the text loop onto the shared resolver so voice and text
runs continue one session by default rather than splitting into separate
continuity universes.
Consequences: New cross-session state should land in `state/session/`; any future
directory change needs explicit compatibility handling and a read-only fallback
story for existing `state/text-loop/` artifacts.

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

## 2026-06-07 - Session ageing lives under session
Decision: Warm-turn summarization retries, token-budget ageing, cross-turn
co-retrieval persistence, and session-end flush behavior belong to
`crate::session::ageing` rather than the multi-turn text experiment.
Context: The session-ageing extraction needed one shared ageing boundary so the text loop and future
session-owned callers can share the same policy and side effects while reducers
stay pure and emit `SessionEvent`s.
Consequences: Ageing policy changes should land in `session/ageing.rs`; the
experiment should only orchestrate inputs, outputs, and shared ageing calls.
Future voice or session surfaces that need the same ageing behavior should call
the shared module instead of copying the text-loop implementation.

## 2026-06-07 - Project-doc introspection v1 scope
Decision: Project-doc introspection v1 is framed-self only, exposed to the
`ConversationalResponder` role only, with no source-code access, no write effects,
and a default allowlist that excludes `docs/Reviews/**.
Context: Self-reflection design and implementation planning narrowed the first
live introspection channel to read-only project documentation so the responder
can ground self-questions without broad repository access or autonomous
development agency.
Consequences: Active-self, episodic-self, pattern-self, meta-memory, source-code,
write-capable, and non-live-role introspection are deferred to follow-on designs.

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

## 2026-06-09 - Browser-relayed realtime events are diagnostic until sideband authority
Decision: Browser-relayed realtime provider events are untrusted,
diagnostic-only facts. They may be persisted for inspection, but they are excluded
from sleep consolidation, continuity promotion, and durable memory. Trusted live
voice exchanges begin when the server-side sideband becomes the
authoritative event source.
Context: The browser can observe useful media/session events, but it is not an
authoritative source for provider facts. The server-side sideband can attach to
the same realtime call via `call_id` and observe/control the session from the
server boundary.
Consequences: Event records and exchanges need an explicit trust/source marker.
Sleep and continuity code must filter diagnostic browser-relay records. The
browser relay can prove UI, media, and reducer wiring without changing durable
memory.

## 2026-06-09 - Realtime browser voice MVP defaults
Decision: The first browser realtime voice MVP uses `gpt-realtime-2`, voice
`marin`, `reasoning_effort = medium`, `output_modalities = ["audio"]`, and
provider `server_vad` with automatic response creation and interruption enabled.
The browser client secret lifetime is governed by provider-returned
`client_secret.expires_at`. The provider `call_id` binding is active-call scoped,
invalidated on stop/error/expiry, and retained only for a short diagnostic cleanup
grace.
Context: The project needs concrete defaults so the browser realtime path can exercise the new code
path by default. Current OpenAI docs identify `gpt-realtime-2` as the most capable
realtime voice model, recommend `marin`/`cedar` for voice quality, expose
`server_vad` for turn detection, and provide `expires_at` for client secrets.
Consequences: Browser realtime tests and manual verification should expect these defaults.
Changing model, voice, VAD mode, or binding lifetime later requires an explicit
decision or provider-drift note rather than an incidental implementation change.

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
Consequences: The completion identity change lands before provider
integration. Browser media work must include reducer tests for out-of-order transcript
completion, duplicate provider events, interruption before `response.created`,
response completion after interruption, and two user turns before the prior
response finishes.

## 2026-06-09 - Realtime tools are read-only and execution-recorded
Decision: Tools exposed to live realtime voice sessions are allow-listed and
read-only. Realtime model tool-call requests are recorded as `ToolRequested`, but
that request is not execution evidence. QSF decides permission, executes the tool
server-side, records permission/result/error/timing, returns a
`function_call_output` item to the provider, and resumes the response.
Context: Earlier realtime voice work deliberately prevented providers from
executing tools directly. The live sideband design now needs a positive execution
path without weakening the QSF permission and observability boundary.
Consequences: Do not overload `auto_executed` as proof of execution. Realtime tool-loop verification must
prove both allowed read-only execution and denied non-allow-listed calls, with
records linked by provider `call_id` or tool-call id.

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

## 2026-06-09 - qsf_session extraction shipped with qsf_app compatibility wrappers
Decision: `qsf_session` owns the pure session contracts, including the live and
persisted state DTOs, reducer functions, exchange records, continuation/resume
classification, continuity manifest, sleep records, and the foundational context
and content-hash value types. `qsf_app` keeps the effectful launcher/runtime edge,
compatibility wrappers, and resume schema-upgrade logging.
Context: The session-crate extraction completed the crate extraction and the reducer completion identity
update. The resume loader also had to preserve the existing schema-upgrade log in
`qsf_app` while moving the file I/O and schema upgrade logic into `qsf_session`.
Consequences: Future crates such as the realtime server can depend on
`qsf_session` without the heavy `qsf_app` graph. `qsf_app` remains the thin facade
for existing call sites until later phases replace them.

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
referenced below. Browser-media planning review found the prior design fused two distinct
OpenAI flows (minting an ephemeral secret AND server-proxying the SDP), which is
internally inconsistent: an ephemeral secret exists to let the untrusted browser talk
directly to OpenAI, but the server was also proxying the exchange. The server-side
flow is the only one consistent with the declared trust boundary (the browser is
untrusted) and with the server-side sideband, which attaches to the server-captured
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
until the server-side sideband validates it.
Reverses: "Browser realtime voice uses a dedicated live server" (ephemeral-token
minting) and "Realtime browser voice MVP defaults" (browser client-secret lifetime),
both 2026-06-09.

## 2026-06-09 - Realtime browser UI lives under qsf_realtime_server/ui
Decision: The live browser surface for realtime voice conversation lives in a
dedicated Vite + TypeScript + Biome + Vitest project at
`crates/qsf_realtime_server/ui/`, separate from `qsf_browser_server/ui/`.
Context: The read-only browser server must stay decoupled from live voice
concerns, and the realtime server needs its own build and verification boundary.
Consequences: Launcher wiring, frontend checks, and UI assets for the realtime
slice target the dedicated crate-local UI directory instead of reusing the
inspection server UI.

## 2026-06-09 - Browser relay artifacts stay diagnostic-only and self-describing
Decision: Browser-relayed provider events are persisted only as untrusted
diagnostics outside the shared continuity root, and the diagnostic records carry
explicit source/trust markers plus the provider identity fields needed for
correlation and replay.
Context: The browser relay is intentionally untrusted. The same
artifacts must remain understandable alongside authoritative sideband
events, so the record shape needs to declare its trust level instead of relying
on storage location alone.
Consequences: Browser relay events do not feed sleep consolidation or continuity
promotion. Diagnostic persistence must record `call_id`, `event_id`, `item_id`,
`previous_item_id`, and `response_id` alongside the exchange payload.

## 2026-06-09 - Realtime reducer overlap finalizes the prior exchange
Decision: When a new user turn arrives before the previous response finishes,
the live reducer finalizes the prior exchange first, marks it interrupted if the
response was still streaming, and treats late lifecycle events for that exchange
as no-ops.
Context: Browser speech-to-speech exchanges can arrive out of order and can be
interrupted mid-response. The single-active-exchange reducer must stay stable in
the face of duplicate or stale provider events.
Consequences: The live reducer keeps one active exchange at a time, suppresses
stale response ids after interruption or completion, and leaves provider event
`event_id` deduplication to the server translator boundary.

## 2026-06-09 - Provider drift: `reasoning_effort` is not forwarded to OpenAI realtime calls
Decision: The accepted browser realtime session default `reasoning_effort = medium` is
kept as QSF session metadata (still returned to the browser in the allocation
response) but is **not** forwarded in the OpenAI `/v1/realtime/calls` session
object.
Context: First live verification of the server-side SDP exchange
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
Context: The browser realtime voice path made a live browser voice session work, which already emits the live
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

## 2026-06-10 - Sideband uses the server-captured call_id websocket with bearer auth
Decision: The realtime sideband connects to
`wss://api.openai.com/v1/realtime?call_id=...` and authenticates with the
server-held `OPENAI_API_KEY` in the Authorization header.
Context: OpenAI's realtime server-controls guide documents the server-side
websocket attach path for an in-progress WebRTC call. This was verified against
the live docs during implementation to confirm the server-side attach shape.
Consequences: The browser never receives a credential. The realtime server must
keep the key server-side, build the websocket URL from the captured `call_id`,
and treat any drift from this attach shape as a docs-updating event.

## 2026-06-10 - Authoritative realtime sideband supersedes the browser relay
Decision: The server-side sideband attached to the server-captured `call_id` is
the authoritative trusted source for live realtime exchanges. The browser relay
remains diagnostic-only and must not feed continuity.
Context: The server-owned websocket control plane can
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
Decision: The realtime default is `server_vad` with
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

## 2026-06-10 - Live tool scope is the three read-only perception tools
Decision: The realtime allow-list is exactly `search_memory`,
`get_associations`, and `inspect_session_state` — new read-only tools implemented
in `qsf_realtime_server`. No existing `qsf_app` tool is exposed to the live model
for this scope. Long-term intent: the live model eventually gets the full tool set
once the required data services move past the
no-`qsf_app` boundary.
Context: External review of the realtime tool-loop plan flagged the allow-list scope as a
blocking product decision; exposing existing `qsf_app` tools live would require
either moving `ProjectDocService`/durable-session access into lean crates or
breaking the `qsf_realtime_server`-must-not-depend-on-`qsf_app` boundary. Owner
confirmed the three-tool scope 2026-06-10.
Consequences: The read-only realtime scope proves the tool-loop machinery (permission decisions,
execution records, exchange boundary, credential hygiene) against server-owned
data only. The generic `qsf_tools` registry core is designed so the later
full-exposure phase is an additive change.

## 2026-06-10 - Tool execution records persist onto durable turns
Decision: Live tool activity is recorded as a `ToolExecutionRecord` (permission
decision, status, budget-capped result summary, error, timing, per-response model
usage, linking provider `call_id`) and persists onto durable `Turn` records
behind serde defaults. `auto_executed` on `ToolRequestRecord` is not execution
evidence.
Context: The only durable record of a realtime conversation is the promoted
`Turn` list; live-only records would leave no artifact of what tools ran or were
denied (logs only), keep tool activity out of the read-only inspection surface,
and deprive live-memory extraction/ageing of provenance and usage signal. Owner
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
promotion and presence research.
Consequences: Trusted promotion carries full-turn usage; token/latency accounting
across a tool call is covered by tests; per-call detail lives on the execution
record rather than inflating exchange-level fields.
Refs: crates/qsf_realtime_server/src/realtime/sideband.rs,
docs/Plans/Plan.RealtimeVoiceConversation.md

## 2026-06-10 - Generic tool registry core
Decision: The reusable tool trait, request, permission, result, metadata, and dynamic
registry core live in `qsf_tools`; app-specific tool context access stays behind
`qsf_app` adapters, and realtime tools use the same lean registry without depending
on `qsf_app`.
Context: Realtime sideband tool execution needs to preserve the
`qsf_realtime_server` no-`qsf_app` boundary.
Consequences: Concrete app tools can remain in `qsf_app`, while server-owned tools can
be registered and permission-checked through the shared core.
Refs: crates/qsf_tools, crates/qsf_app/src/tools,
crates/qsf_realtime_server/src/realtime/tools.rs

## 2026-06-10 - Realtime tool loop boundary
Decision: Realtime function-call responses do not finalize a trusted exchange. The
sideband records the request, executes or denies the tool outside the session lock,
records the resolution, sends `function_call_output`, and creates the next response.
A per-turn cap forces the follow-up response to disable tools.
Context: A single spoken turn can contain multiple provider `response.done` events
when the model calls tools before speaking.
Consequences: Trusted promotion waits for the eventual spoken response, model usage is
aggregated across the tool loop, and lock-free execution leaves stop/degraded races
observable.
Refs: crates/qsf_realtime_server/src/realtime/sideband.rs,
crates/qsf_session/src/exchange.rs

## 2026-06-10 - Realtime tool denial recovery
Decision: Realtime tool calls that are not allow-listed, exceed read-only bounds, have
malformed arguments, or hit the loop cap are recorded as denied or failed and still
receive structured `function_call_output` so the model can recover verbally.
Context: Leaving a provider function call unanswered stalls the live conversation.
Consequences: Denials are durable tool execution records, not execution evidence, and
the sideband remains responsible for returning a provider-visible result.
Refs: crates/qsf_realtime_server/src/realtime/sideband.rs,
crates/qsf_realtime_server/src/realtime/tools.rs

## 2026-06-10 - Realtime function-call wire shape
Decision: The realtime tool loop uses OpenAI Realtime function tools declared on `session.tools` with
`tool_choice`, returns results as `conversation.item.create` items of type
`function_call_output`, and then sends `response.create`.
Context: Official OpenAI Realtime tool documentation describes function tools as
application-executed calls that return `function_call_output`; it also documents
session-level `tools`, `tool_choice: "auto"`, and sending `response.create` after the
tool output.
Consequences: QSF keeps private memory and permission logic server-side while using
the provider's current tool-call continuation surface.
Refs: https://developers.openai.com/api/docs/guides/realtime-mcp,
crates/qsf_realtime_protocol/src/lib.rs

## 2026-06-11 - Realtime voice uses a stable default session id
Decision: Browser realtime voice sessions use the stable QSF session id `default`
unless the operator explicitly starts `qsf_realtime_server` with
`--random-session-id`.
Context: The live realtime tool test showed that UUID-per-run session ids made local
memory reuse brittle: the memory store was seeded under one session directory while
the live call used another. The operator preference is for a reusable default
identity unless random isolation is explicitly requested.
Consequences: Local realtime memory and continuity artifacts live at
`state/realtime/continuity/default` in the normal path. Random UUID sessions remain
available for isolated experiments. The server rejects a second active default
session instead of silently replacing the runtime.
Refs: crates/qsf_realtime_server/src/state.rs,
crates/qsf_realtime_server/src/cli.rs

## 2026-06-12 - Continuation noise and stale provider events are diagnostic-only
Decision: While a response or tool continuation is in flight, allow-listed courtesy
transcripts are ignored as diagnostic-only records, and stale or superseded provider
events are audited as diagnostic-only records instead of mutating the live exchange.
Context: The realtime sideband can otherwise reuse or overwrite the active exchange
across interruptions and cancelled continuations, which corrupts durable turns.
Consequences: Short continuation noise does not emit `response.create` or touch
`session_state`, stale response.created/done events stay inert, and the live exchange
must be explicitly finalized before the next user turn starts.
Refs: crates/qsf_realtime_server/src/realtime/turn_integrity.rs,
crates/qsf_realtime_server/src/realtime/sideband.rs,
crates/qsf_realtime_server/src/diagnostics.rs

## 2026-06-12 - Session inspection reports completed turns and active exchange presence
Decision: `inspect_session_state` reports trusted durable completion count as
`completed_exchange_count` and exposes active exchange presence explicitly through
`active_exchange_present` plus the existing active-exchange details.
Context: The previous blended exchange count double-counted promoted turns and also
folded in retained but untrusted live exchanges, which made the inspection output
ambiguous.
Consequences: Tool output and downstream consumers must treat completed promoted turns
as the auditable count, and active exchanges are reported separately rather than being
mixed into the completion total.
Refs: crates/qsf_realtime_server/src/realtime/tools.rs

## 2026-06-13 - Realtime sideband owns interruption decisions
Decision: Browser realtime voice sessions keep provider `server_vad` enabled but set
`interrupt_response = false`; QSF sideband logic, not provider auto-cancel, owns
whether detected speech interrupts an in-flight response.
Context: Live testing showed assistant audio or empty VAD tails could trigger provider
auto-interruption before QSF received the final transcript and could classify it as
noise.
Consequences: The provider should not cancel assistant speech on raw VAD alone.
Final transcripts remain the sideband's decision point for starting, ignoring, or
interrupting turns; genuine interruptions send `response.cancel`, and empty final
transcripts are diagnostic-only.
Refs: crates/qsf_realtime_server/src/state.rs,
crates/qsf_realtime_server/src/realtime/sideband.rs,
crates/qsf_realtime_protocol/src/lib.rs

## 2026-06-13 - Interruptions are captured as diagnostics, not durable continuity
Decision: Sideband-owned interruptions and their presence/timing signals are recorded to
the per-session diagnostics log only. The durable continuity/memory schema
(`session-state.json`, `continuity-manifest.json`, promoted `Turn`s) and its golden tests
carry no interruption representation. Interrupted exchanges stay non-promotable and absent
from the continuity root. Richer durable interruption signals (pause/silence durations,
barge-in classification, topic-shift flags) are added later only if presence evaluation
shows a concrete need.
Context: Resolves the open question of how deeply to represent interruptions. Inspecting
the code showed interruptions are not durable at all today: the reducer pushes the
interrupted exchange into `completed_exchanges`, which is `#[serde(skip)]` (in-memory
only, guarded by `persist_keeps_completed_exchanges_in_memory_only`), and promotion skips
non-promotable indices, so the `InterruptionRecord` is written to neither the continuity
root nor the diagnostics log. A durable record therefore has to be added regardless of
direction. Diagnostics-only keeps the trust boundary clean (interrupted/incomplete
material stays out of trusted long-term memory), avoids golden-test churn for an
unvalidated research feature, is still durable-on-disk for after-the-fact presence
analysis, and matches `Concept.RealtimePresence` ("log interruptions without
over-interpreting them").
Consequences: The interrupted trusted exchange (with its `InterruptionRecord` + timing) is
emitted to the diagnostics log at the point it is currently dropped, reusing/extending
`DiagnosticRecord` rather than the durable schema. Interruption and degraded-exchange
observability is read from diagnostics, not the continuity root; extraction into long-term
memory continues to draw only from trusted promoted turns.
Refs: crates/qsf_realtime_server/src/diagnostics.rs,
crates/qsf_realtime_server/src/realtime/sideband.rs,
crates/qsf_session/src/live_state.rs, crates/qsf_session/src/persistence.rs,
docs/Concepts/Concept.RealtimePresence.md

## 2026-06-13 - Realtime live memory extraction uses trusted continuity root
Decision: Live memory extraction runs in `qsf_app` against the realtime
continuity root `state/realtime/continuity/default`, reads only promoted trusted
`SessionState.turns` as the canonical transcript source, treats matching
persisted exchanges as metadata only, and falls back to a smoke input when the
continuity root is absent or malformed.
Context: Live-memory extraction needed a pass that could reuse the existing sleep
summarizer and review/commit machinery without giving `qsf_realtime_server` a
`qsf_app` dependency. Inspecting the persisted continuity artifacts showed that
turns already hold the canonical transcript, tool records, and memory context,
while exchanges are useful only as metadata for provenance and interruption
observability. The existing sleep commit path already knows how to route a
report through review, memory promotion, and association deduplication.
Consequences: Extraction provenance is labeled inline in the input text, the
commit path can stay shared, realtime ageing explicitly reuses the existing
warm-turn summary path before consolidation, and malformed or missing continuity
artifacts do not block extraction. Latency and interruption observability remain
diagnostics-only and are kept outside durable continuity.
Refs: crates/qsf_app/src/experiments/live_memory_extraction.rs,
crates/qsf_app/src/experiments/sleep_phase_session_summary.rs,
crates/qsf_realtime_server/src/realtime/sideband.rs,
docs/Experiments/Experiment.LiveMemoryExtraction.md;
implements: Live memory extraction + presence / interruption refinement

## 2026-06-14 - `realtime` is the first-class live-conversation launcher command
Type: Decision
Decision: The realtime voice conversation operator surface is the launcher command
`qsf.ps1 realtime`. It starts `qsf_realtime_server` (foreground) and the realtime
Vite UI (separate window) together, requires `OPENAI_API_KEY` to be present (verified
without printing), and uses the fixed ports 3940 (server) and 5174 (UI) rather than
exposing `-Port`/`-BindHost`.
Context: The realtime server and UI entry points have stabilized, so the previously
deferred operator command could be named. The realtime UI's Vite dev proxy is pinned
to the server on `127.0.0.1:3940`, so a configurable server port would silently break
the proxy; the server itself reads only `OPENAI_API_KEY` and consumes none of the
launcher's managed `QSF_*` variables, so the command applies no environment defaults.
Consequences: Live conversation is launched with one command instead of two manual
terminals. `realtime` intentionally has no port/host flags; changing the ports
requires updating both `qsf_realtime_server`'s `cli.rs` default and the UI
`vite.config.ts` proxy in lockstep. The experiment runner stays the harness for
regression and fixture-backed validation.
Refs: scripts/qsf.ps1, crates/qsf_realtime_server/src/cli.rs,
crates/qsf_realtime_server/ui/vite.config.ts

## 2026-06-24 - Engineering diary archived
Decision: `docs/EngineeringDiary.md` is archived and is no longer an active required project
document. Implementation chronology is carried by the git commit log; current
project knowledge is recorded by updating the relevant active document, and durable
commitments remain in `docs/DecisionLog.md`.
Context: The diary mostly duplicated information already available in commit
history, while also creating a second place that every implementation change had to
maintain. This reverses the active-diary portion of the 2026-05-09 diary and
decision-log document contracts decision.
Consequences: Repository workflow instructions no longer require a diary entry per
logical change, the project-document status model treats archived documents as
historical context only, and project-document introspection no longer advertises a
live `Diary` document kind.
reverses: 2026-05-09 - Diary and decision-log document contracts

## 2026-06-26 - Tension arbitration tier is separate from selection weight
Decision: `Tension` exposes two distinct ordinal fields: `priority_bias`
(`TensionPriority` — selection-weight provenance only, never drives arbitration) and
`arbitration_tier: u8` (lower value wins cross-goal conflict resolution). The existing
`TENSION_PRIORITY_NOTE` remains accurate and unchanged. Probabilistic arbitration is
out of scope by default; if introduced, it must be gated behind an explicit experiment
mode flag and recorded in traces.
Context: Cross-goal arbitration (Phase 5 of the volition plan) required a deterministic
conflict order. Using `priority_bias` for arbitration would have conflated "general
tension importance" with "conflict precedence," made the idea doc's 8-tier order
ambiguous across five priority levels, and silently broken the Phase 3
provenance-only invariant.
Consequences: Every new tension must declare both fields explicitly. `priority_bias`
must not be promoted to an arbitration driver. The idea doc's 8-tier order maps to
`arbitration_tier` values; tiers 2 (user intent), 3 (task completion), 6 (experiment
mode), and 8 (optional exploration) are not yet covered by any fixture tension and will
be assigned when new tensions are added.

## 2026-06-27 - Mode bias may reorder only within the biasable band; protected tiers are immune
Decision: A `Mode`'s declared `bias_vector()` may shift arbitration ordering only within the
biasable band (effective tier ≥ 4). Tiers 1–3 (safety/boundary, explicit user intent, current
task completion) are immune: their `bias_applied` is always 0, and a biased band goal is clamped
to `>= PROTECTED_TIER_FLOOR + 1` so it can never enter the floor. The mode is explicit,
inspectable `VolitionState`, changed only by a `ModeChanged` event.
Context: The mode/bias slice proved that an inspectable, event-sourced bias can deterministically
flip the arbitration winner among band goals while being structurally unable to displace a
protected-floor goal. The floor immunity holds by construction (clamp arithmetic, not runtime
policy), making it verifiable without relying on mode-level access control.
Consequences: Every new tension at tier ≤ 3 is automatically immune; future modes may add new
tension keys to their bias vector but must not lower the floor constant. Probabilistic arbitration
and salience/selection bias remain out of scope and require an explicit experiment gate if
introduced.

## 2026-06-27 - Realtime voice is the target surface for the consciousness simulation
Decision: The project's final target is a realtime voice-accessible simulation of
consciousness-like behavior. Offline experiments, reports, launchers, and inspection
tools are research scaffolding for that target, not competing end states.
Context: The volition work currently runs through `qsf_app` experiments, while the
first-class live surface is `qsf.ps1 realtime`. The project framing needed to make
explicit that volition, memory, perception tools, self-reflection, consolidation, and
observability should eventually be reachable from the live realtime voice surface.
Consequences: Plans that mature core simulated-mind subsystems should identify how
they become available to realtime voice without bypassing trust boundaries or
inspectability. Experiment-only access is acceptable for early validation but is not
the final integration shape.

## 2026-06-27 - Realtime volition extraction keeps context assembly outside `qsf_volition`
Decision: The `qsf_volition` crate contains pure volition domain state, reducers,
context-neutral selection/arbitration records, fixtures, and bounded initiative output.
Context-attached selection results, experiment reports, and realtime context packets
stay in caller adapters such as `qsf_app` or `qsf_realtime_server`; context assembly
stays in the shared `qsf_context` crate. `qsf_volition` must not depend on `qsf_app`.
Context: The current `qsf_app` volition selector carries `ContextFragment`,
`ContextBudget`, and `ContextAssembly` through `GoalSelection` and
`GoalSelectionResult`. Moving that surface wholesale would either create a bad
`qsf_volition -> qsf_app` dependency or blur the extraction boundary needed by the
realtime server.
Consequences: The extraction starts from the pure reusable core. Realtime can depend on
`qsf_volition` without importing experiment/report code, while each caller chooses how
to turn selected goals into context fragments or reports.

## 2026-06-27 - Realtime volition retrieval initiatives are memory-injection hints
Decision: In realtime volition, `ContextRetrievalRequested` is an internal hint to the
next sideband memory/context injection pass. It contributes query terms to existing
retrieval/context assembly; it does not directly invoke `search_memory` or any other
tool and does not execute an external effect.
Context: Bounded initiative output must remain internal unless a later plan explicitly
expands the realtime permission model. Treating retrieval requests as immediate tool
calls would make a supposedly internal initiative cross into external effect execution.
Consequences: Realtime bounded-initiative traces must record the hint terms, whether the
next memory-injection pass consumed them, and `external_effect_executed: false`.
External tool execution from volition initiative remains out of scope.

## 2026-06-28 - `realtime` supervises both processes and opens the browser
Decision: `qsf.ps1 realtime` launches the realtime server and the Vite dev server as
supervised child processes, opens the default browser at the resolved UI URL, and then
blocks in a wait loop until Ctrl+C or until either child exits. On exit it terminates
both process trees with `taskkill /T /F`. The Vite UI port is resolved as the first free
port at or above the preferred `5174` and passed to Vite with `--strictPort`, so the
opened URL always matches the bound port. The server port stays pinned to `3940`.
Context: The earlier shape ran the server in the foreground and only closed the UI
window, which left Vite's child `node` process holding the port after Ctrl+C and never
auto-opened the browser. Running the server foreground also made Ctrl+C cleanup
unreliable; a `Start-Sleep` wait loop runs the cleanup `finally` deterministically.
Consequences: One command brings up the full live surface and one Ctrl+C tears it down
with no orphaned port holders. The UI port may differ from `5174` when it is busy, so
durable references should treat `5174` as the preferred port, not a guarantee. This
refines the 2026-06-14 `realtime` launcher decision.

## 2026-06-29 - Volition context injection carrier and grounding boundary
Type: Decision
Decision: Realtime volition context injection layers two carriers. The stable baseline
(configured tensions, priors, arbitration stance, trust boundary, default mode) is rendered
deterministically and carried in the shared base instructions sent with both the initial and
every per-turn `session.update`, so its content is identical every turn (verified by a stable
baseline hash) and is never carried as a separate replaceable conversation item. The per-turn
dynamic packet is computed and injected independently of memory retrieval, after any memory
item and before the initial `response.create`, and never on tool-loop continuation. Opportunity
detection is rule-based over grounded input terms (normalized text plus original span) and goal
ids; every signal cites a grounding ref, and `UnresolvedPriorTopic` is deferred until a
continuity source exists. Injected volition context is simulated internal state and never claims
real desire, consciousness, or subjective experience, and never authorizes external action.
Context: The Phase 4 plan review stopped for manual feedback because the baseline carrier and
the exact injected text were unresolved behavioral/safety decisions, and `session.update`
replaces session config so a baseline placed only in the initial update would be silently
overwritten. The exact rendered baseline and turn-packet text are fixed in
`Experiment.RealtimeVolitionContextInjection` and asserted in tests.
Consequences: There is one effective instruction-composition path across the initial and
per-turn `session.update` and `response.create`, so the baseline cannot be dropped or
overridden. The injection trace links to the existing per-turn request-sequence hash rather
than a new client event id. Curiosity/exploration cannot override protected tiers, and shaping
intensity is clamped to at most `Low` when a protected tier wins arbitration.

## 2026-06-30 - Realtime bounded-initiative surfacing, anti-nag cadence, and trace granularity
Type: Decision
Decision: Realtime bounded internal initiative is derived from the arbitration winner and
recorded to diagnostics on every trusted turn, but the model-facing initiative line is surfaced
only under bounded conditions. A protected-tier winner (effective tier at or below the protected
floor) surfaces a line only when the turn carries a genuine opportunity signal beyond the
winner's own topic self-match — expressed uncertainty, an introduced contradiction, or an
open-goal-topic match grounded on a different goal; otherwise the protected turn stays silent.
The anti-nag rule is consecutive-turn alternation: the same goal is not surfaced on two adjacent
trusted turns, tracked by a previous-turn-surfaced marker that is set only when a line surfaces
and cleared on any non-surfaced turn (not by a last-initiative marker). Bounded-initiative state
snapshots use the compact state-inspection projection, and the trace contract asserts the
winning goal's before/after transition (status, last activation, last initiative output) rather
than a full state clone. The surfaced line rides inside the existing single per-turn volition
system item. Context-retrieval initiatives are never surfaced and never executed; they only
stash next-turn memory-injection hints. The behavior is default-on with no flag.
Context: The Phase 5 plan review stopped for manual feedback because the plan claimed the shaping
dial would keep protected and `None` turns quiet, but the protected goal `honor-explicit-user-
request` matches common words and its own keyword match counts as an opportunity, so the dial
returns `Low` and would surface a reflection on ordinary direct requests. The project vision
prioritizes presence and appropriate reflection over task completion, so full suppression on
every direct request was rejected in favor of surfacing only when the moment genuinely invites
it. The proposed `last_initiative_goal_id` anti-nag marker was also shown to suppress a repeated
winner indefinitely.
Consequences: A direct user request stays focused unless the conversation itself shows
uncertainty, contradiction, or another open thread, at which point a single bounded reflection
may surface; the same goal cannot repeat on back-to-back turns. The surfacing gate is a
realtime-side rule layered on top of the shared shaping dial, so the already-shipped context-
injection intensity behavior is unchanged. Initiative stays auditable through the diagnostics
record even when no line is spoken, and the trace verifies the state transition the initiative
event caused. A longer tick-based cooldown remains the documented upgrade path if live testing
shows alternation still nags. Curiosity and exploration still cannot surface initiative while a
protected goal wins arbitration.

## 2026-06-30 - Realtime context inspection is live and value-faithful
Type: Decision
Decision: Realtime turn-context inspection exposes the latest trusted turn's provider
payload as a live presentation surface, not as a persisted experiment artifact. The
inspector captures the verbatim JSON value sequence sent for the initial context-injected
request, publishes it over the per-session events socket as a `turn_context` message, and
anchors fidelity with the same request hash used by the diagnostics trace. It excludes the
connect-time session setup and tool-loop continuation requests unless a later decision
expands the inspection boundary.
Context: The realtime UI needed to answer "what did the model receive for this turn?"
without asking researchers to reconstruct base instructions, memory context, and volition
packets from hashes and summaries. This is an observability presentation mode over the live
send path rather than a behavioral experiment, and persisting the full repeated payload every
turn would bloat diagnostics without improving the immediate inspection workflow.
Consequences: Realtime observability surfaces may use per-session `watch` channels and
`kind`-discriminated events-socket messages for latest-only live presentation. Durable
diagnostics continue to store compact hashes and structured traces; literal payload
persistence, history, and pretty-printing require separate explicit follow-up work.

## 2026-06-30 - Realtime volition continuity
Type: Decision
Decision: Realtime volition continuity is written for inspection but never blindly
reloaded. New sessions seed from `realtime_seed_fixture()` plus an explicit reviewed
seed artifact, and durable cross-session volition changes require a human-run reviewed
acceptance step.
Context: Phase 6 needed a durable boundary that preserves useful continuity without
making live volition sticky or turning diagnostics into an automatic promotion path.
Consequences: `volition-state.json` remains an inspectable continuity artifact, not the
session seed of record. `volition-seed.reviewed.json` is the only durable seed consumed
at session start, the sleep/consolidation pass can propose changes but not apply them,
and reviewed-seed loading failures degrade to the plain fixture with a diagnostics note.

## 2026-06-30 - Realtime volition inspection uses the events socket
Type: Decision
Decision: Realtime volition inspection is published over the existing per-session
events socket as a `volition_state` message backed by a `watch` channel, not by a
polling HTTP endpoint. The capture is latest-only, read-only, and no-secret; it
mirrors the live volition snapshot plus an optional decision summary. No-selection
trusted turns publish a state-only capture and do not fabricate a winner.
Context: Phase 7 needed a browser-visible volition surface that updates during the
spoken turn and preserves the same session-isolation pattern as the turn-context
inspector. The events-socket push path already fits that behavior and avoids a new
polling route that would add latency and duplicate session lookup logic.
Consequences: The realtime UI consumes a push stream rather than polling state.
Session-local watch channels remain the transport for live inspection surfaces, and
operator-facing volition details stay bounded to the compact capture shape.

## 2026-06-30 - Adopted goals belong to the simulation; coherence replaces goal provenance
Type: Decision
Decision: Every goal the simulation adopts belongs to the simulation, whatever its
origin (user input, reflection, or perception). The system does not classify goals by
owner (user / simulator / shared); origin may be kept only as a background memory or
association, never as a separate class of goal held on someone's behalf. Because it owns
its goals the simulation must stay coherent with them: a malleable goal may exist only if
it does not contradict a more fundamental (lower-tier) goal. The protected tier floor is
immutable; only goals above it form, change, or are cancelled.
Context: Detailing the first phase of the motivational-texture work reopened the imported
brief's "user vs simulator vs shared goals" idea (§12). On reflection an ownership tag is
the wrong model: what makes the system read as a separate agent is not a label but its
capacity to own its goals and decline input that would make it incoherent. This supersedes
the brief's provenance concept and refines the 2026-05-15 volition stance; the rule that
the protected core is immutable at runtime stands.
Consequences: Goal-conflict explanation becomes truthful through a detected incompatibility
rather than a narrated one. New goals are admitted only when consistent with the protected
core, so the system can reject requests that would violate it. The brief's §12 is retired as
not-adopted. A goal carries at most a remembered origin, not an owner classification.
Identity stays anchored to the immutable protected core.

## 2026-06-30 - Goal coherence is model-judged off the hot path and repaired during sleep
Type: Decision
Decision: Contradiction between goals is detected by model judgment, not by hand-declared
incompatibility links, from the first implementation. Detection is a side effect that lives
in an adapter; its verdict is recorded as an inspectable trace artifact and fed back into
the pure reducer as events, which resolve it deterministically (cancel the higher-tier-number
goal, never one at or below the protected floor). Judging happens off the live turn's
critical path: discussion may form a goal candidate live, but coherence checking and any
rejection happen after the turn or during sleep, not within the same response. Sleep performs
a periodic whole-goal-set coherence sweep in a single model round-trip to catch drift between
goals that have come to contradict each other.
Context: A simulation that owns and believes its goals (companion entry above) needs a way to
tell when two goals are incompatible. Incompatibility is semantic, so a model is the right
judge and determinism is neither required nor desired. The live loop is latency-sensitive and
candidate formation is rare, so per-turn model judging on the hot path is not worth its cost;
a coherence repair is real and useful even when it is not felt within the same breath. Cost is
bounded by judging once per new candidate and once per sleep rather than per goal.
Consequences: This keeps a non-deterministic judge inside the project's evidence-based,
inspectable stance and its unidirectional event-reducer-state flow: the model detects, the
reducer resolves. Goal admission, rejection, and cancellation are explained by trace artifacts
a researcher can replay. Turn latency is unaffected by coherence work. A freshly formed goal
that proves incompatible is retired before it can shape later turns, with the rejection
surfacing on a later turn or under introspection. Live, in-the-moment rejection would be a
separate decision.

## 2026-07-01 - Live goal formation and coherence detection run as one cache-structured model call per turn
Type: Decision
Decision: In the realtime loop, live goal formation and coherence detection run as a single
cache-structured model call once per trusted turn, off the hot path (after the response is
dispatched); the pure reducer resolves the result into the existing goal-lifecycle events. There
is no heuristic pre-filter gate — the call runs every trusted turn. A rejected candidate is
recorded as a durable, injectable session context record (the conflicting goal id + rationale),
model-visible from the next turn onward; the model decides whether and how to voice it. Sleep
performs whole-history goal formation and the whole-set coherence sweep. To let the realtime
server run these calls, the model layer (`ModelClient`, `ModelRole`, `invoke_model_role`,
`CoherenceJudge`) is extracted into a shared crate that both `qsf_app` and `qsf_realtime_server`
depend on, with the `ModelClient` boundary exposing a stable-prefix / cache-breakpoint seam.
Context: Detailing the live wiring of the goal-coherence engine (companion 2026-06-30 entry)
needed a formation trigger for discussion-formed candidates and a place to run admission. The
formation call runs off the hot path, and prompt caching bills cache reads at ~0.1x base input
over a goal-set prefix that stays byte-stable until the goal set changes (a prefix-hash rule
re-warms the cache when an admission, retirement, or sweep changes the set) — so a per-turn model
call is cheap and non-blocking, and the simplest uniform design (run every turn, one combined
formation+detection call) beats a heuristic gate and minimizes round-trips. The realtime server
has no model access today and deliberately does not depend on `qsf_app`, so the model layer is
shared rather than duplicated or reached across a heavy dependency.
Consequences: Turn latency is unaffected; the agent can form a goal and, within the same session,
decline one that contradicts a more fundamental goal. Because the turn's model context is built
before the response is sent while admission runs after, a rejection is model-visible from the next
turn, not the turn that formed it. The decline is inspectable, evidence-backed session context the
model may act on rather than a scripted line, so it cannot nag. The extraction requires the
`ModelClient` abstraction to express cache breakpoints — it reports provider cached-token usage
today but has no cache-breakpoint request field — so that boundary check is the first
implementation step. Emergent goals and accumulated drift are caught in the sleep pass. This
refines, and does not reverse, the 2026-06-30 "model-judged off the hot path and repaired during
sleep" decision.

## 2026-07-01 - Live goal formation and off-hot-path coherence: cache boundary is an application-level marker, model layer moves to a new `qsf_models` crate, `ModelInvoker` decouples callers from RunContext
Type: Decision
Decision: Implementing the previous entry's "first implementation step" found that neither
`openai_provider_kit`'s `LlmRequest` nor the raw OpenAI Chat Completions API expose a
`cache_control`-style request field — OpenAI's own prompt caching is automatic over a
byte-stable prefix of the raw request, with no explicit breakpoint to set. The cache-breakpoint
boundary is therefore implemented as an application-level seam:
`ModelRequest::with_stable_prefix_message_count` / `stable_prefix_hash` mark and hash the leading
messages a caller declares stable (system instructions + goal set), used to populate the trace's
`cached_prefix_ref` / `prefix_cache_eligible` fields — not forwarded to any provider request.
Separately, the model layer (`ModelClient`, `ModelRequest`/`ModelMessage`, `ModelRole`/
`ModelRoleId`, `CoherenceJudge`, and the new `LiveGoalFormationJudge`) moves out of `qsf_app` into
a new shared crate, `qsf_models`, rather than into `qsf_volition` (deliberately pure/no-IO) or
directly into `qsf_realtime_server` (which would leave `qsf_app` without it). `CoherenceJudge`'s
and `LiveGoalFormationJudge`'s `judge`/`form_and_detect` methods no longer take `&mut RunContext`
directly; they take `&mut dyn ModelInvoker` (also defined in `qsf_models`), a seam that
decouples model callers from any one observability backend. `qsf_app`'s `RunContext` implements
`ModelInvoker` by delegating to the existing `invoke_model_role` (kept in `qsf_app`, unchanged
behavior and unchanged offline call sites via `&mut RunContext`'s coercion to `&mut dyn
ModelInvoker`); the realtime loop uses a trace-free `DirectModelInvoker` and records its own
`DiagnosticRecord::LiveGoalFormationPerformed` around the whole formation call instead of a
per-model-call trace.
Context: The prior entry's D6 named `ModelClient`/`ModelRole`/`invoke_model_role`/`CoherenceJudge`
for extraction and a Claude-`cache_control`-shaped cache seam as the first implementation step,
written before checking what the actual model client wraps. Inspection during implementation
confirmed the only `ModelClient` impls are OpenAI-backed (`OpenAiProviderModelClient`,
`OpenAiToolClient`) and a mock — no Anthropic client exists anywhere in the app — so an
Anthropic-shaped cache-breakpoint field would have had nothing to attach to. `invoke_model_role`
was also found to be hard-wired to `RunContext`/`TraceRecord`/`EventType` (qsf_app-only
constructs), and `CoherenceJudge::judge` took `&mut RunContext` directly in its trait signature,
neither of which `qsf_realtime_server` has an equivalent of (it has `DiagnosticWriter`/
`DiagnosticRecord` instead).
Consequences: The cache-prefix hash is honest, inspectable state describing what this
application declares stable — it does not claim a provider-side caching guarantee that doesn't
exist for the OpenAI path today, and it would carry real meaning without further change if an
explicit-breakpoint provider is added later. The `qsf_models` crate boundary and the
`ModelInvoker` seam mean `qsf_app`'s existing offline model-invocation tests, traces, and events
are unchanged (verified: all pre-existing `qsf_app` tests pass unmodified), while
`qsf_realtime_server` gains model-invocation capability without depending on `qsf_app` or
duplicating the model-client code. This refines, and does not reverse, the 2026-07-01 "one
cache-structured model call per turn" decision above.

## 2026-07-02 - Sleep goal maintenance auto-applies formation and sweep; declined candidates become reducer-derived state
Type: Decision
Decision: The sleep/consolidation pass runs whole-history goal formation and a whole-set coherence
sweep that **auto-apply** to the persisted volition snapshot
(`run_sleep_volition_goal_maintenance`, invoked from `commit_cross_session_sleep`), through the
same shared pure resolvers the live per-turn hook uses (`resolve_formed_candidate` /
`resolve_sweep`). The separate `consolidate_session_volition` report remains reviewable-only and
human-gated; only the goal-maintenance pass mutates durable state, matching the
Experiment.LiveGoalFormationAndCoherence D5 "sleep does whole-history formation and the whole-set
sweep" scope and the offline harness. Separately, coherence-declined candidates are no longer a
side-channel list written next to the event stream: they are reducer-derived
`VolitionState::declined_candidates`, populated when `apply` folds a `GoalCandidateRejected` event
carrying a `CoherenceDecline { conflict, rationale }`, deduplicated by title and capped at a
window, so replaying the event stream reproduces the injected coherence context.
Context: A review of the live goal formation and off-hot-path coherence change found the real
sleep pass never gained the formation/sweep behavior it was scoped for (only the offline harness
exercised it), the declined-candidate state and the ~55-line candidate-resolution block were
duplicated between the live hook and the offline harness, and the injected `coherence` layer used
a `"protected_floor"` sentinel stored in a goal-id field. The auto-apply choice for sleep goal
maintenance was made deliberately (over a proposal-only alternative) to match D5 and keep the
offline harness a faithful validation of live behavior.
Consequences: One shared resolver and one declined-candidate type back both the live hook and the
offline harness, so a semantic change can no longer desync them. A protected-floor decline renders
honestly ("is below the protected floor tier") instead of naming a non-existent goal. The default
mock client forms nothing and sweeps nothing, so the sleep goal-maintenance path runs end to end
by default. This refines, and does not reverse, the 2026-07-01 decisions above; it does not change
the reviewable-only, human-gated nature of the `consolidate_session_volition` continuity report.

## 2026-07-03 - Realtime persona replaced with curiosity-observer; personas are data

Decision: The realtime seed persona is the outward-facing curiosity-observer roster
(`realtime_seed_fixture()`): three protected tensions (person-respect, epistemic-integrity,
present-person-priority) and four malleable ones (knowledge-stewardship, person-curiosity,
ai-trajectory-concern, world-curiosity). A personality change must not change code, constants
excepted: mode bias now lives in per-tension `focused_bias` / `exploratory_bias` fixture data,
not in a hardcoded vector.

Context: The prior dev-assistant persona had goals about the QSF project itself and coupled one
personality datum — mode bias — to code via `Mode::bias_vector()`. The curiosity-observer persona
runs the pending live goal-formation voice test against a persona for which goal-formation
conversations are natural.

Consequences: This **amends the 2026-06-27 "mode bias may reorder only within the biasable band"
decision**, which declared `Mode::bias_vector()` the source of truth. The revised rule: mode labels
(`Neutral` / `Focused` / `Exploratory`) stay fixed; each tension's own `focused_bias` /
`exploratory_bias` supplies the bias delta (read via `Mode::tension_delta`); tiers 1–3 remain
code-enforced bias-immune. Seed-fixture goals are immune to idle retirement (only live-formed
accepted candidates retire). On resume, a continuity snapshot that is incompatible with the active
fixture (a persona swap changed the goal ids) is **discarded** and the session restarts from the
seed. This **reverses the approved design's stated preference for reconciliation**
(`Design.curiosity-observer-persona.md`): reconciliation would preserve live-formed accepted
candidates and tick continuity across the swap, but for this one-time id replacement that state
belongs to the retired persona and is not worth a reconciler's complexity; the accepted cost is
losing those candidates and tick continuity whenever an incompatible snapshot is dropped. If a
future slice evolves a fixture *within* a persona era (adding or removing a goal without a full id
swap), reconciliation should be revisited then. First-class thesis/library support (a thesis
lifecycle on the memory system) is deferred to a later slice.

One seed goal's activation keywords carry a known, accepted keyword-tuning cost: the malleable goal
`learn-what-drives-this-person` includes the activation keyword `"me"`, which co-occurs with almost
any first-person direct request ("help me", "tell me"). As a result, ordinary first-person direct
requests now also register as a genuine opportunity for the person-curiosity goal, nudging the
bounded-initiative suppression logic to surface curiosity initiatives more often than the prior
persona did. This is accepted as a known consequence to be tuned after the live voice test, per the
near-universal `i`/`my`/`me` keyword tuning already flagged in
`Experiment.CuriosityPersonaSeed.md`'s Open Items — not a bug to fix now.

## 2026-07-03 - `realtime` launcher manages the server environment and pins `QSF_MODEL_PROVIDER=openai`

Decision: `qsf.ps1 realtime` applies a managed environment delta around the server start, the same
mechanism `app` launches use: it sets `QSF_MODEL_PROVIDER=openai` and clears every other non-secret
`QSF_*` variable for the child process, restoring the launcher's own session afterwards. The
realtime command is inherently OpenAI-backed (it already refuses to start without
`OPENAI_API_KEY`), so the provider is launcher-managed rather than user-remembered.
Context: The first curiosity-observer voice session ran with `QSF_MODEL_PROVIDER` unset. The
sideband model roles fall back to the mock client in that case, and the mock live-goal-formation
judge always returns "no candidate" — so live goal formation silently ran as a no-op for the whole
session (sub-millisecond `live_goal_formation_performed` records, nothing proposed), voiding the
formation half of the voice test. The launcher's stated principle — it controls all non-secret
`QSF_*` variables for deterministic behavior — already applied to `app` launches but not to
`realtime`, which inherited whatever was ambient.
Consequences: A plain `.\scripts\qsf.ps1 realtime` now runs the formation/coherence judges against
the real provider with no manual environment setup, and ambient `QSF_*` leftovers cannot leak into
a live session. Anyone needing a mock-judged realtime session must launch the server directly
rather than through the launcher. This refines the 2026-06-28 `realtime` supervision decision and
narrows the 2026-07-02 observation that the mock default exercises the sleep path end to end: that
default remains right for offline runs, but a live voice session must not inherit it silently.

## 2026-07-04 - Project-level handoff document with three recommendation levels
Decision: The project keeps one project-level `docs/Handoff.md` as its resume point. It recommends
the next step at three abstraction levels — Now (immediate action), Next (active plan phase),
Horizon (direction, e.g. elaborating an idea into a plan) — with one primary recommendation per
level plus at most one or two alternates. Each entry is a pointer (recommendation + one-line
rationale + link), never the content itself; it is updated in place only when an event changes a
recommendation at some level, and stays within a two-minute read. The full rules live in
`ProjectWorkflow.md` (Handoff Discipline).

Context: The workstream-scoped `Handoff.Volition.md` had grown into a ~320-line mirror of
experiment findings, plan status, and fixture data — duplicating content that belongs in
experiment Results, architecture, and the backlog, and going stale between sessions. The three
levels formalize the hierarchy the document was already reaching for, and a project-level scope
matches the Horizon level, which is inherently project-wide.

Consequences: Workstream handoffs are retired; `Handoff.Volition.md`'s content moved to its proper
homes (experiment Results, `Architecture.VolitionSystem.md`, `Experiment.Backlog.md`, the active
plan) and the file is deleted. Events that change no recommendation (e.g. a negative test with
nothing new) do not touch the handoff. The handoff is never authoritative for anything — readers
follow its links; other documents must not cite it as evidence.

## 2026-07-04 - Weighted goal activation with a global arbitration qualification threshold
Decision: Activation keywords carry coarse curated weight classes (Weak = 1, Normal = 4,
Strong = 8) in fixture data; a goal's match strength is the sum of its matched-term weights and
is the single strength quantity behind both the ranked relevance display and arbitration
qualification. A goal must reach one global fixture-level qualification threshold (default 4)
before it can win arbitration; qualification is a pure partition step inside arbitration, before
the existing tier sort, which is unchanged among qualified goals. Protected tiers get no
qualification exemption — their protection (never cancelled by coherence, decline-backoff, floor
semantics) governs cancellation, not speaking. When no goal qualifies, volition stays quiet for
the turn and records a dedicated below-qualification-threshold suppression instead of promoting a
weak winner or falling back to a default goal. Per-tier thresholds, corpus-derived weights,
stemming, and phrase matching are deliberately deferred.

Context: Live voice evidence (2026-07-04, `Experiment.CuriosityPersonaSeed.md` /
`Experiment.LiveGoalFormationAndCoherence.md`) showed binary token activation plus
strength-blind tier sorting letting a protected goal win the initiative line on a stopword
(`what`/`do`) against a five-term on-topic match. Resolved in the 2026-07-04 brainstorm; validated by
`Experiment.WeightedGoalActivation.md`. The long-term semantic direction is preserved
separately in `Idea.SemanticGoalActivation.md`; this deterministic lexical layer ships first
and doubles as its no-GPU fallback and evaluation harness.

Consequences: Personas stay data-only — weights and the threshold are fixture data, readable
and tunable without code changes. Persisted goals (continuity snapshots, reviewed seeds,
live-formed candidates) need a compatibility reader that accepts legacy plain-string keywords
as Normal; live-formed goals qualify on a single model-supplied keyword until the formation
schema is extended. Initiative frequency drops slightly on idle or stopword-only turns, which
the anti-nag layer already established as acceptable. Traces must record matched terms with
their weight classes plus the threshold in force so qualification outcomes stay auditable.

## 2026-07-04 - Experiment documents are scoped to consciousness-simulation mechanisms
Decision: An `Experiment.*.md` is reserved for reducing uncertainty about a
consciousness-simulation mechanism (memory, volition, continuity, presence, context
assembly, arbitration, and the like). Routine engineering work — UI controls, refactors,
build tooling, launcher flags, dependency bumps — does not earn an experiment even when it
is a self-contained testable slice; it is carried by its code, its tests, and its commit,
and promoted to a plan, architecture note, or decision only if it affects those.

Context: The workflow's "single self-contained, testable slice → an Experiment" rule of
thumb read as scope-neutral, inviting experiment documents for ordinary feature work whose
outcome was never in doubt. That dilutes the validation track, which exists to make the
project's research legible, and buries genuine mechanism experiments among engineering
chores. The trigger was a browser-UI mute control that is plainly engineering, not research.

Consequences: The test for an experiment is now "does this probe a simulation mechanism
whose behavior is uncertain?", not merely "is this a testable slice?". `ProjectWorkflow.md`
(Document Tracks, the `docs/Experiments/` responsibility, and Experiment Discipline) states
the scope. Engineering slices proceed without an experiment document; if such a slice raises
a real question about how a mechanism behaves, that question — not the feature — becomes the
experiment.

## 2026-07-04 - Weighted goal activation validated; threshold 4 confirmed
Decision: The qualification threshold default of 4 is confirmed for both shipped fixtures and
kept as the shipped value. Weighted goal activation is accepted design, not a candidate.
Context: The human voice retest (`Experiment.WeightedGoalActivation.md`, session 2026-07-04)
confirmed every success criterion: the natural AI-transition probe let `track-the-ai-transition`
win at match strength 16 and fire `ProposeExperiment`, a stopword-only turn recorded a
below-qualification-threshold suppression with no initiative, and injection latency stayed at
0 ms. Confirms the 2026-07-04 weighted-goal-activation decision above.
Consequences: The threshold stays fixture data and can be retuned per persona without code
changes if later evidence warrants; no per-tier thresholds are introduced. The value is
validated for these two fixtures, not proven optimal across all personas.

## 2026-07-05 - Realtime sleep consolidation is a first-class command
Decision: Sleep/consolidation after a realtime session is exposed as `qsf.ps1 sleep`
and `qsf_app sleep`, not only as an experiment. The launcher defaults to
`state/realtime` and the OpenAI provider so the command processes the same continuity
state produced by `qsf.ps1 realtime`; `-Provider mock` remains available for local
smoke runs. The command writes the same inspectable sleep artifacts and manifest-last
state commit as the existing sleep flow.

Context: Realtime voice is now a normal operating mode, and its follow-up
consolidation is routine workflow rather than a mechanism experiment. Running a real
session and then having to invoke `sleep-phase-session-summary` through the experiment
launcher made the operational boundary unclear.

Consequences: The experiment harness can still validate the sleep machinery, but the
supported user workflow is realtime session first, first-class sleep update second,
then resume from the consolidated brief. Sleep remains explicit and inspectable; this
does not introduce background or periodic sleep.

## 2026-07-05 - Realtime browser explains volition's per-turn effect via presentation-only selectors
Decision: The realtime browser explains volition's effect on the latest reply purely in the
TypeScript view-model layer, from the existing `volition_state` and `turn_context` captures
correlated by their shared per-attempt request hash — no new wire fields, no reducer changes, no
server-behavior changes.
Context: The right-hand panel was a flat dump of volition-domain fields (tick, tiers, salience,
thresholds) that assumed familiarity with the volition model and never connected a decision to
the answer it shaped.
Consequences: Per-turn explanation stays on the read-only, non-blocking observation plane;
richer explanations must come from correlating existing captures (or extending a capture), not
from coupling the UI to server internals. The injected text is located client-side by the
packet's prose prefix, pinned by a Rust guard test across every packet-emitting path. Deferred
follow-ups (revisit after using the prototype): binding each capture to the specific transcript
answer that produced it, surfacing the standing persona/stable-baseline stance, and captioning
interrupted/failed turns.

## 2026-07-05 - Sleep launcher backs up state and reports an itemized change view

Decision: `qsf.ps1 sleep` backs up the target state directory to
`state/backups/<name>-<timestamp>/` (keeping the newest five) before invoking the
sleep update, and `qsf.ps1 restore` rolls back from those backups, backing up the
current state first to `state/backups/<name>-restore-<timestamp>/` so a restore is
undoable. Restore undo snapshots are excluded from `restore latest` and
sleep-backup pruning. The sleep command reports an itemized change view (memories
added, associations added/strengthened, goal changes, state files written)
rendered from a structured `SleepChangeRecord` that is also written as a
`sleep-changes.json` run artifact. Rollback safety was chosen over a `--dry-run`
mode: sleep output depends on a live model call either way, and a backup keeps the
real run as the single code path instead of maintaining a plan/apply split.

Context: Sleep auto-applies memory promotion, association changes, and goal
maintenance (2026-05-20, 2026-05-22, 2026-07-02), so an operator had no way to
preview or undo a bad consolidation, and the command reported only artifact paths.

Consequences: Operators can run sleep freely and roll back regretted consolidations.
The change view and `sleep-changes.json` make each sleep run reviewable at a glance;
backups live inside the git-ignored `state/` tree. The launcher remains the
supported operator surface for backup/restore; raw `cargo run -p qsf_app -- sleep`
does not create backups.

## 2026-07-05 - Continuity directory layout is one source of truth in qsf_session
Decision: The on-disk continuity layout (the `continuity/<session_id>/` nesting, the
stable `default` session id, and the resolver that maps a state-directory root to a
concrete session directory) is defined once in the shared session crate and used by both
the realtime writer and the sleep reader.
Context: The realtime server nested continuity state under the state directory while the
sleep command read the manifest directly from the root, so the two disagreed on the
manifest path and sleep silently fell back to a smoke-test transcript against a real
persisted session. Duplicated layout knowledge is what let them drift.
Consequences: Any new reader or writer of continuity state resolves paths through the
shared helpers rather than re-deriving them. Sleep understands both the nested realtime
layout and the flat text-loop layout; an ambiguous multi-session root is an explicit
error rather than a silent wrong guess.

## 2026-07-05 - Memory browser launcher opens the Vite UI on a free port
Decision: `qsf.ps1 browser` and `qsf.ps1 workbench` now start both the memory
browser API and the Vite UI, choose the first free UI port at or above 5173, and
open the Vite page automatically. The Rust API root remains an informational
health page rather than the primary operator surface.

Context: The browser API root linked to `http://localhost:5173/`, but that port
can already belong to another local application. Operators had to discover and
start the Vite UI separately even though the useful memory browser experience is
the UI, not the API landing page.

Consequences: The launcher owns the workbench lifecycle and avoids hardcoded UI
port collisions. Raw API launches remain possible through `cargo run`; raw UI
launches can still be run from `crates/qsf_browser_server/ui` with an explicit
Vite port.

## 2026-07-05 - Diagnostics card shows reducer-owned transition history
Decision: The realtime browser Diagnostics card shows transition history — a
collapsed recent-event ticker and a 60 s runtime-phase swimlane — instead of a
single overwritten last-event field. History lives in reducer state (an event log
and a phase timeline) with wall-clock timestamps carried on actions so reducers
stay pure; all geometry and formatting derive from pure selectors, and the canvas
strip is a dumb consumer redrawn on a clock interval.
Context: Relay events — especially partial_transcript bursts — overwrote the single
last-event field faster than a human could read, so transitions were invisible.
Reducers may not read a clock, so timestamps are stamped at each action's dispatch
site and threaded through as data.
Consequences: Diagnostics history survives Stop for post-hoc review and clears when
a new session is allocated. Any future action that should appear in the ticker must
carry a wall-clock timestamp stamped at its dispatch site.

## 2026-07-06 - Phase lane shows activity time with compressed idle gaps
Decision: The realtime diagnostics phase lane's x-axis is activity time, not wall-clock
time. An idle stretch longer than a short cap (3 s) renders as its first 3 s at true
scale plus a fixed-width hatched break band labeled with the real gap duration; while
the live trailing idle exceeds the cap the lane freezes and reads "paused" until the
next activity. History pruning uses the same activity-time window.
Context: On a wall-clock axis, waiting for the user to respond scrolled all activity out
of the 60 s window — the lane was pure idle exactly when a finished exchange should be
reviewable. Chosen over a display-only freeze, which would still have expired history at
the moment of resume.
Consequences: Lane geometry and history retention are bounded by activity, not elapsed
time — history survives arbitrarily long waits. Gridline offsets on the lane read as
activity time; wall-clock durations appear only on break-band labels, and wall-clock
timestamps remain in the event ticker and tick tooltips.

## 2026-07-06 - Volition functional signals are visualization-first and operator-panel only
Decision: Emotion-like functional signals derived from volition state are visualization-first.
A signal is a pure, deterministic value recomputed on demand from recorded `VolitionState`
plus fixture data — no new mutable emotion object, never stored, never an input to anything
but display. The gate is structural, not a runtime flag: signal derivation has no code path
into arbitration, salience, selection, initiative, or context injection, so there is nothing
to toggle and no config flag is needed (the default build exercises the new path, per
Agents.md). The first signal set, with each signal's functional definition:
- `coherence_decline` — a coherence-engine rejection recorded in `declined_candidates`;
  evidence is the rejected candidate title, conflict, rationale, and tick. Deliberately not
  labeled `tension`: true tension stays reserved for an unresolved current conflict among
  selected goals, which needs substrate this slice does not build.
- `frustration` — a goal repeatedly `Blocked` despite activation: reducer-maintained
  `blocked_count` at or above a named threshold, with `last_activated_tick` present.
- `satisfaction` — a recent `GoalSatisfied` with exact event evidence:
  `last_satisfied_tick` plus `last_satisfied_evidence_ref`.
- `boredom` — every non-retired goal's salience below a named threshold at the current tick,
  guarded by prior activity (at least one prior goal activation, or a named minimum
  elapsed-tick threshold) so cold-start state never counts as bored.
Deferred signals: true D4 `tension` (needs unresolved current-conflict state), `curiosity`
(needs an explicit open-delta representation), and `attachment` (needs settled cross-session
reinforcement semantics). Signals attach only to the top-level realtime
`VolitionInspectionCapture` consumed by the operator panel; `VolitionStateInspection` and the
`inspect_volition_state` tool are unchanged — exposing signals to the model would edge from
visualization toward narration input and deserves its own D4 review later.

Context: Review of the volition motivational-texture signal slice found that historical
coherence declines were being mislabeled as tension, model-visible inspection would move the
feature from visualization into self-narration input, and sustained N-tick boredom could not
be proven from the current `VolitionState` shape. The slice needs inspectable motivational
texture consistent with the anti-anthropomorphic stance (D4 of
`Design.VolitionBriefReconciliation.md`; DecisionLog 2026-05-15, 2026-06-27, 2026-06-30):
"emotion" is only ever a named, evidence-derived functional signal, never a felt state.

Consequences: Reducer additions for this slice are lifecycle facts only, such as exact
satisfaction evidence and block repetition counters — no stored emotion state. The browser
may render evidence-backed signal rows, but the panel never shows a bare emotion word without
its evidence, and models do not see or narrate signals unless a later decision deliberately
opens that boundary. Future true tension, sustained boredom, curiosity, attachment, or
model-visible signals require separate substrate and review.
Refs: docs/Plans/Plan.VolitionMotivationalTexture.md,
docs/Experiments/Experiment.VolitionEmotionLikeSignals.md

## 2026-07-06 - Subconscious volition goals use reduced ambient exposure
Decision: Goal visibility is a presentation and surfacing filter, not a separate runtime path.
`Subconscious` goals participate in salience, selection, arbitration, coherence, and surfacing
gate decisions exactly like conscious goals. In ordinary model-visible ambient turn context,
however, a subconscious arbitration winner is reduced rather than rendered as the normal full
`Active goal: {title} — {summary}` line. Full subconscious detail remains available to operator
panel views, traces, explicit `inspect_volition_state` / `select_volition_goals` tool calls when
sectioned and labeled, and forced-surfacing cases backed by evidence such as a rendered
initiative line or coherence conflict.

Context: The conscious/subconscious visibility slice needed to decide whether a subconscious
winner should be hidden, reduced, or rendered in full to the response model. Rendering it in full
would make "subconscious" mostly a UI/introspection label; hiding it entirely would weaken the
model's ability to follow the selected shaping contract. Reduced ambient exposure preserves
enough guidance for coherent behavior while making ordinary subconscious context less freely
narratable.

Consequences: Selection and arbitration tests must stay identical across visibility mixes.
Ambient injection traces must record the winner visibility and exposure treatment, with full
winner identity preserved trace-side even when rendered text is reduced. Tool outputs that expose
subconscious goals must section and label them instead of silently merging them into ordinary
goal lists. Suppressed internal initiatives do not count as forced surfacing; only rendered
initiative lines or other evidence-backed forcing conditions may justify full surfaced detail.

## 2026-07-07 - Realtime diagnostics track session token usage by model and class
Decision: The realtime diagnostics page now shows a session-scoped Tokens card fed by a server-side token ledger. The ledger records provider-reported usage per `(role, model)` and splits counts into fresh text/audio input, cached input, and text/audio output. The meter counts completed realtime responses even when they are cancelled or stale, and it also preserves goal-formation spend captured at the model-invoker seam when a billed call later fails validation.

Context: The realtime page already exposed turn, volition, and phase diagnostics, but token spend was only available as raw per-call data on the server. The new card makes the session total visible without introducing a dollar table or changing persisted exchange schema.

Consequences: Reconnecting browsers heal from the latest token snapshot over the events socket, and operators can compare realtime voice spend against goal-formation spend within a session. The split remains diagnostics-only; it does not feed arbitration or persisted exchange records.

## 2026-07-07 - Realtime voice model default is gpt-realtime-2.1 with a single source of truth
Decision: The default OpenAI realtime voice model is `gpt-realtime-2.1`, and the model id is defined in exactly one place, the shared realtime protocol layer, from which the app and the realtime session server both consume it.
Context: OpenAI released `gpt-realtime-2.1` (2026-07-06) as an incremental upgrade over `gpt-realtime-2` with improved alphanumeric recognition, silence/noise handling, and interruption behavior, at unchanged pricing and with no session schema changes. The previous default was duplicated as a string literal across the app, server, and protocol crates.
Consequences: Future voice-model bumps are a one-line change in the protocol layer. Runtime code and tests reference the shared constant instead of repeating the literal; documentation states the current model id where it describes accepted defaults.
