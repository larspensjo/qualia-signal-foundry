# Plan: Framework MVP

## Status

Proposed

## Summary

This plan defines the minimum implementation framework needed to start running experiments in Qualia Signal Foundry.

The goal is not to build the full consciousness simulation. The goal is to create a small, observable, extensible framework that can support the first experiments around associative memory, context budgeting, sleep-phase summaries, tool-as-perception, model roles, and eventually audio-loop presence.

The MVP should favor:

- clear structure
- small scope
- explicit event flow
- strong observability
- easy experiment creation
- minimal assumptions about final architecture
- replaceable components
- Rust-friendly implementation
- compatibility with OpenAI API access through the existing `openai_provider_kit` library

The framework should make it possible to run early experiments without hard-coding every experiment as a separate one-off prototype.

## Goal

Build the smallest useful framework that can run and observe the first experiments.

The framework should support this shape:

```text
Experiment
  -> input events
  -> runtime loop
  -> state update
  -> optional memory/context/tool/model step
  -> output events
  -> event log
  -> traces
  -> experiment report
```

The MVP should let the project begin learning from real behavior rather than only discussing architecture.

## Non-Goals

The Framework MVP is not trying to implement:

- real consciousness
- a full agent platform
- a production assistant
- a polished UI
- persistent identity
- complete long-term memory
- complex multi-model orchestration
- general plugin infrastructure
- autonomous external action
- full real-time audio
- video input
- production-grade storage
- large-scale embeddings
- full prompt management
- cloud deployment
- multi-user operation

These may become relevant later, but they should not block the first experiments.

## Related Documents

```text
ProjectFrame/ProjectVision.md
ProjectFrame/NonGoals.md
ProjectFrame/ProjectWorkflow.md

Concepts/Concept.AssociativeMemory.md
Concepts/Concept.ContextBudget.md
Concepts/Concept.SleepPhase.md
Concepts/Concept.ToolsAsPerception.md
Concepts/Concept.RealtimePresence.md
Concepts/Concept.ExternalInputs.md
Concepts/Concept.MultiModelMind.md

Architecture/Architecture.Overview.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.ToolSystem.md
Architecture/Architecture.SleepPhase.md
Architecture/Architecture.ModelRoles.md
Architecture/Architecture.StateAndObservability.md
Architecture/Architecture.AudioLoop.md

Experiments/Experiment.Backlog.md
Experiments/Experiment.Template.md
Experiments/Experiment.AssociativeMemoryToyModel.md
Experiments/Experiment.ContextBudgetRetrievalTest.md
Experiments/Experiment.SleepPhaseSessionSummary.md
Experiments/Experiment.ToolAsPerceptionCalculator.md
Experiments/Experiment.StreamingTranscriptionMVP.md
Experiments/Experiment.AudioLoopMVP.md
Experiments/Experiment.RealtimeVoiceSessionMVP.md
```

## First Supported Experiments

The Framework MVP should be designed around these first experiments.

### Primary First Experiment

```text
Experiment.AssociativeMemoryToyModel
```

Why this should be first:

- tests a central concept
- requires no audio hardware
- requires no complex API streaming
- can use deterministic data
- exercises memory, context, ranking, and traceability
- gives useful architecture feedback early

### Secondary First Experiment

```text
Experiment.ContextBudgetRetrievalTest
```

Why this is closely related:

- tests how memory/context candidates are selected
- forces useful tradeoffs
- exercises observability
- informs the context manager design
- can reuse the same memory fixtures as the associative memory experiment

### Useful Early Tool Experiment

```text
Experiment.ToolAsPerceptionCalculator
```

Why this is useful:

- provides a safe first tool
- has deterministic output
- exercises tool registry, tool request, tool result, and tool trace concepts
- tests the idea that tools are perception before action

### Useful Early Sleep Experiment

```text
Experiment.SleepPhaseSessionSummary
```

Why this is useful:

- tests a minimal sleep phase
- produces summaries, memory candidates, open questions, and decision candidates
- helps determine what future cross-session continuity needs

### Later First-Presence Experiment

```text
Experiment.StreamingTranscriptionMVP
Experiment.AudioLoopMVP
```

Why this should probably come after the framework skeleton:

- audio introduces hardware, latency, transcription, TTS, and streaming complexity
- the framework should already have events and traces before audio is added
- audio should plug into the same event and observability model
- streaming transcription is the first useful real-time audio integration because it
  produces text events without requiring the full speech-to-speech loop
- full audio output and interruption behavior should follow only after transcript
  events are observable and replayable

## MVP Architecture Shape

The first implementation should be shaped around replaceable subsystems.

```text
Experiment runner
  -> runtime loop
  -> state store
  -> event log
  -> trace store
  -> memory subsystem
  -> context subsystem
  -> tool subsystem
  -> model role subsystem
  -> sleep subsystem
```

Not every subsystem needs to be fully implemented immediately. Some can start as stubs or simple in-memory implementations.

## Proposed Initial Repository Shape

A possible Rust workspace layout:

```text
Cargo.toml                       # workspace root
Cargo.lock                       # committed because qsf_app is a binary/application
crates/
  engine_logging/                # already present; developer/operator logging facade
  qsf_app/                       # binary + framework modules
    Cargo.toml
    src/
      main.rs
      lib.rs

      experiments/
        mod.rs
        associative_memory_toy_model.rs
        context_budget_retrieval_test.rs
        sleep_phase_session_summary.rs
        tool_as_perception_calculator.rs

      runtime/
        mod.rs
        event.rs
        runtime_loop.rs
        state.rs

      observability/
        mod.rs
        event_log.rs
        trace.rs
        metrics.rs

      memory/
        mod.rs
        memory_record.rs
        association.rs
        retrieval.rs
        fixtures.rs

      context/
        mod.rs
        context_fragment.rs
        context_budget.rs
        context_assembler.rs

      tools/
        mod.rs
        tool_registry.rs
        tool_request.rs
        tool_result.rs
        calculator_tool.rs

      models/
        mod.rs
        model_role.rs
        model_client.rs
        openai_provider.rs
        mock_model.rs

      sleep/
        mod.rs
        sleep_report.rs
        session_summary.rs

      reports/
        mod.rs
        markdown_report.rs
```

This structure is only a starting point. It should be adjusted when implementation reveals better boundaries.

Important: the internal modules from the original single-crate shape still exist; they just live inside `crates/qsf_app/src/` instead of root `src/`.

## Initial Crate Strategy

The project should use a Cargo workspace from day one.

This is no longer premature crate splitting. The repository already contains `crates/engine_logging`, and that crate uses workspace-inherited fields such as:

```toml
edition.workspace = true
license.workspace = true
authors.workspace = true
rust-version.workspace = true

[dependencies]
log.workspace = true
simplelog.workspace = true
```

Therefore the MVP requires a root `Cargo.toml` with workspace package metadata and shared dependencies before `cargo build` can succeed.

Recommended root workspace shape:

```toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
edition = "2024"
license = "MIT"
authors = ["Lars Pensjö"]
rust-version = "1.85"

[workspace.dependencies]
log = "0.4"
simplelog = "0.12"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "2"
clap = { version = "4", features = ["derive"] }
uuid = { version = "1", features = ["v4", "serde"] }
time = { version = "0.3", features = ["formatting", "parsing", "serde"] }
```

The exact dependency versions can be adjusted during implementation, but Phase 1 should include enough shared dependencies to compile both `engine_logging` and the placeholder `qsf_app` crate.

Recommended initial crates:

```text
crates/engine_logging
  Developer/operator log facade. Already present.

crates/qsf_app
  Binary application and framework modules for the MVP.
```

Further splitting should be deferred until implementation pressure justifies it. Possible later crates:

```text
crates/qsf_core
crates/qsf_experiments
crates/qsf_memory
crates/qsf_audio
crates/qsf_tools
```

The first MVP should avoid creating those crates until clear boundaries have emerged from working experiments.

## Logging Strategy

The project needs three separate observability layers. They should not be collapsed into one logging mechanism.

| Layer | Purpose | Format | Owner |
|---|---|---|---|
| Developer/operator log | Human-readable diagnostics, warnings, errors, implementation notes during runs | `log`-style text lines | `engine_logging` |
| Event log | Chronological structured facts: what happened | JSON Lines | `qsf_app::observability::event_log` |
| Trace log | Explanations and measurements: why something happened, what was selected, what was omitted, how long it took | JSON Lines | `qsf_app::observability::trace` |

`engine_logging` is the developer/operator logging facade. It is not the structured event log and it is not the trace log.

Rules:

```text
- Use `engine_logging` macros from day one instead of calling `log::*` directly.
- Use `initialize_to_path` from the experiment runner so each run writes `runs/<run-id>/engine.log`.
- Keep `events.jsonl` as the chronological system-of-record for events.
- Keep `traces.jsonl` as the system-of-record for context, memory, tool, model, and sleep explanations.
- Developer logs may reference event IDs, trace IDs, run IDs, and experiment IDs.
- Developer logs must not be required to reconstruct state transitions.
- API keys, authorization headers, raw secrets, and sensitive local paths must not be logged by any layer.
```

The current `engine_logging` name is acceptable for the MVP, but the project should keep a decision candidate open for renaming it to `qsf_logging` or `foundry_logging` before many call sites accumulate.

The current `set_sim_tick` / `get_sim_tick` functions are inherited game-engine residue. The MVP should either remove them or replace them with experiment/run terminology before they are used.

`engine_logging` uses process-global logger initialization. That is acceptable while experiments run serially in one binary process. If experiments later run concurrently in-process, per-run diagnostic log routing will need to be revisited.

## OpenAI Provider Kit Dependency Strategy

The project will use an existing Rust library for OpenAI API access:

```text
openai_provider_kit
```

This library currently lives in another public Rust project:

```text
https://github.com/larspensjo/web_page_filet_mignon
```

The recommended transition workflow is:

1. Commit a pinned Git dependency in Qualia Signal Foundry.
2. Use a local path override only during local development.
3. Do not commit the local path override.
4. Update the pinned revision deliberately when the provider kit changes.
5. Later, when the provider kit is split into its own repository, update the dependency URL.

### Committed Dependency

In the committed `crates/qsf_app/Cargo.toml`, use a pinned Git dependency when the OpenAI-backed model client is introduced:

```toml
[dependencies]
openai_provider_kit = {
  git = "https://github.com/larspensjo/web_page_filet_mignon",
  package = "openai_provider_kit",
  rev = "<known-good-commit-sha>"
}
```

The `rev` should be replaced with a specific known-good commit SHA.

This keeps public builds repeatable while the provider kit is still moving.

### Cargo.lock

If Qualia Signal Foundry is an application/binary, commit `Cargo.lock`.

This gives public users a buildable and repeatable dependency set.

### Local Development Override

During local development, this patch may be temporarily added to the root `Cargo.toml`:

```toml
[patch."https://github.com/larspensjo/web_page_filet_mignon"]
openai_provider_kit = { path = "C:/Users/larsp/src/web_page_filet_mignon/crates/openai_provider_kit" }
```

This allows local builds to use the working copy of `openai_provider_kit`. The path shown above is the author's local Windows layout; other contributors should adjust the path for their machine and must not commit it.

Important:

```text
Do not commit the local path override.
```

The committed dependency should remain GitHub-buildable for other users.

### Why This Strategy

This strategy is preferred because:

- public repo users can build without access to a local filesystem path
- pinning `rev` avoids accidental breakage from changes on `main`
- local development can still iterate quickly across both projects
- provider-kit updates become deliberate changes
- the later split to `rs-openai-provider-kit` only requires a dependency URL change

### Avoid for Now

Avoid committing:

```toml
branch = "main"
```

Using `branch = "main"` is convenient but makes fresh builds depend on whatever the provider-kit state happens to be at that moment. During transition, pinned commits are safer.

## Model Access MVP

The framework should define a small abstraction over model calls without overengineering provider support.

A candidate shape:

```text
ModelRole
  describes why the model is being called.

ModelRequest
  contains role, input, context, and output expectations.

ModelResponse
  contains text, structured output if any, usage metrics, and trace data.

ModelClient
  sends the request to an implementation.

MockModelClient
  deterministic or fixture-based model for tests.

OpenAiProviderModelClient
  adapter around openai_provider_kit.
```

The MVP should support both:

```text
Mock model
  useful for deterministic experiments and tests.

OpenAI-backed model
  useful for experiments that need real model behavior.
```

The first experiments should not require the OpenAI client unless they specifically need model reasoning.

## OpenAI Realtime Speech Direction

The project should incorporate the new OpenAI realtime speech models in stages rather
than treating them as one audio feature.

Current model mapping:

```text
gpt-realtime-whisper
  First target for real audio integration.
  Use for streaming speech-to-text and transcript deltas.

gpt-4o-transcribe
  Evaluation alternative for accuracy-sensitive transcription.
  Use to compare transcript quality when lowest latency is less important.

gpt-realtime-2
  Later target for full speech-to-speech realtime presence.
  Use for interruption, preambles, tool-call transparency, and voice-native reasoning
  only after transcript events and latency traces are working.

gpt-realtime-translate
  Separate translation experiment.
  Do not fold it into the first framework or audio MVP unless multilingual presence
  becomes an active research question.
```

The realtime transcription model was rechecked against the OpenAI API
documentation on 2026-05-13 after live Phase 9 evaluation. The default remains
`gpt-realtime-whisper` because the project prioritizes low-latency transcript
deltas for presence experiments. `gpt-4o-transcribe` remains useful as an
accuracy comparison target.

```text
https://platform.openai.com/docs/guides/realtime-transcription
https://developers.openai.com/api/docs/models/gpt-realtime-2
https://developers.openai.com/api/docs/models/gpt-realtime-translate
```

Integration rule:

```text
Realtime providers are side-effect adapters.
They emit QSF events back into the runtime loop.
They do not own runtime state, memory promotion, tool permissions, or decisions.
```

The first OpenAI realtime implementation should therefore be a transcript provider,
not a full voice agent. Its outputs should become structured events such as:

```text
AudioInputStarted
AudioInputChunkCaptured
AudioPartialTranscript
AudioFinalTranscript
AudioInputEnded
AudioTranscriptionFailed
LatencyMeasurementRecorded
```

Only finalized transcript events should enter the normal input -> action -> reducer
-> state -> render flow by default. Partial transcripts may be logged and traced
first, then used by later experiments if they prove useful.

## API Key Handling

The framework should not hard-code API keys.

A simple MVP approach:

```text
OPENAI_API_KEY environment variable
```

Possible future improvement:

```text
.env file support
configuration file
secret store integration
```

Early implementation should avoid logging secrets.

The event log and trace output must not include raw API keys or authorization headers.

## Runtime Loop MVP

The runtime loop should process structured events.

A minimal flow:

```text
InputEvent
  -> RuntimeLoop::handle_event
  -> State update
  -> Optional subsystem call
  -> OutputEvent
  -> EventLog append
  -> Trace append
```

The runtime loop should be simple enough that experiments can follow what happened.

### State Update Model

The runtime loop must follow the accepted unidirectional reducer commitment recorded in `docs/DecisionLog.md` and mirrored in `Architecture.RuntimeLoop.md`.

Required shape:

```text
Input event
  -> pure reducer: (State, Event) -> State
  -> emitted effects or side-effect requests
  -> side effects run outside the reducer
  -> side-effect results return as new events
```

Rules:

```text
- Reducers stay pure and unit-testable.
- Model calls, tool invocations, file I/O, logging, and report writing do not mutate runtime state directly.
- Meaningful state transitions are represented as events.
- Event records explain what happened.
- Trace records explain why something happened.
```

### Candidate Event Types

Initial event types:

```text
ExperimentStarted
ExperimentCompleted
InputReceived
MemoryRetrievalRequested
MemoryRetrieved
ContextAssemblyRequested
ContextAssembled
ToolRequested
ToolCompleted
ToolFailed
ModelRoleRequested
ModelRoleCompleted
SleepPhaseRequested
SleepPhaseCompleted
OutputProduced
ErrorOccurred
TraceRecorded
```

Audio-specific events can come later:

```text
AudioInputStarted
AudioPartialTranscript
AudioFinalTranscript
SpeechPlaybackStarted
SpeechPlaybackCompleted
UserInterrupted
```

## State MVP

The first state model should be modest.

Candidate state:

```text
RuntimeState
  experiment_id
  current_step
  active_focus
  recent_events
  last_context
  last_memory_retrieval
  last_tool_result
  last_model_response
```

Avoid building a complex persistent self-model in the MVP.

The system should keep state explicit and inspectable.

## Event Log MVP

The event log is the chronological record of what happened.

The first version can write JSON Lines:

```text
runs/<run-id>/events.jsonl
```

Each event should include:

```text
event_id
experiment_id
timestamp
event_type
payload
trace_id
```

The payload should be structured but can remain flexible early.

## Trace MVP

Traces explain why something happened.

The first version can write JSON Lines:

```text
runs/<run-id>/traces.jsonl
```

Trace records should include:

```text
trace_id
experiment_id
timestamp
operation
input_summary
output_summary
details
latency_ms
error
```

Useful trace types:

```text
MemoryRetrievalTrace
ContextAssemblyTrace
ToolInvocationTrace
ModelRoleTrace
SleepPhaseTrace
```

The trace system should start simple but should exist from the beginning.

## Metrics MVP

Initial metrics can be minimal.

Examples:

```text
operation latency
number of selected memories
number of omitted memories
estimated context size
tool call count
model call count
model input tokens
model output tokens
estimated cost
```

Metrics can be recorded inside traces first, then extracted into separate reports later.

## Memory MVP

The memory MVP should support the associative memory experiment.

Candidate memory record:

```text
MemoryRecord
  id
  summary
  tags
  kind
  importance
  created_at
  last_retrieved_at
  reinforcement_count
  source
```

Candidate association:

```text
MemoryAssociation
  from_id
  to_id
  weight
  reason
```

Candidate retrieval result:

```text
RetrievedMemory
  memory_id
  score
  score_breakdown
  association_path
  reason
```

The MVP can use in-memory fixtures before persistent storage.

### Initial Retrieval Strategies

Implement enough to compare:

```text
recency-only
tag/keyword match
association-weighted
hybrid scoring
```

The scoring does not need to be perfect. It needs to be visible.

## Context Management MVP

The context manager should assemble a small set of fragments under a budget.

Candidate context fragment:

```text
ContextFragment
  id
  kind
  summary
  source
  estimated_tokens
  relevance_score
```

Candidate context budget:

```text
ContextBudget
  max_fragments
  max_estimated_tokens
```

Candidate assembly output:

```text
ContextPackage
  selected_fragments
  omitted_fragments
  estimated_tokens
  assembly_trace
```

The MVP should always record omitted fragments. Omitted context is important for debugging.

## Tool System MVP

The tool MVP should support:

```text
Experiment.ToolAsPerceptionCalculator
```

Candidate components:

```text
ToolRegistry
ToolRequest
ToolResult
ToolExecutor
CalculatorTool
ToolTrace
```

The calculator tool should have:

```text
category: ComputeOnly
side_effect_level: None
```

The result should become an observation before it can become context.

A minimal flow:

```text
ToolRequest
  -> permission check
  -> tool execution
  -> ToolResult
  -> ToolObservation event
  -> candidate context fragment
  -> trace
```

## Model Role MVP

The model role system should start small.

Candidate model roles:

```text
MockResponder
MemoryExtractor
SleepSummarizer
ResearchPlanner
Critic
```

Only implement roles that are needed for the first experiments.

A model role should define:

```text
role_id
purpose
allowed_tools
context_budget
model_client
output_expectation
```

Do not build a full multi-agent system in the MVP.

## OpenAI-Backed Model MVP

The OpenAI-backed model client should be introduced behind the same `ModelClient` abstraction used by the mock model.

A possible flow:

```text
ModelRequest
  -> OpenAiProviderModelClient
  -> openai_provider_kit
  -> ModelResponse
  -> ModelRoleTrace
```

The wrapper should record:

```text
model role
model name or profile
input summary
output summary
latency
token usage, if available
estimated cost, if available
errors
```

The wrapper should not expose provider-specific details to the rest of the framework unless needed.

## Sleep Phase MVP

The first sleep phase should be simple.

Input:

```text
recent session log or experiment log
```

Output:

```text
SleepReport
  session_summary
  memory_candidates
  open_questions
  decision_candidates
  future_context_hints
```

This can initially be manual, mock-model-based, or OpenAI-backed.

The important part is that the sleep output is explicit and reviewable.

The sleep phase should not silently create accepted decisions.

## Experiment Runner MVP

The experiment runner should allow running named experiments from the command line.

Possible command shape:

```powershell
cargo run -- experiment associative-memory-toy-model
cargo run -- experiment context-budget-retrieval-test
cargo run -- experiment tool-as-perception-calculator
cargo run -- experiment sleep-phase-session-summary
```

The exact CLI can change.

A simple first CLI is enough:

```powershell
cargo run -- associative-memory-toy-model
```

But a named experiment runner will scale better.

### Experiment Trait

A possible Rust shape:

```text
Experiment
  id()
  description()
  run()
```

The runner should:

```text
create experiment directory
initialize event log
initialize trace log
run experiment
write summary report
print output path
```

## Report MVP

Each experiment should produce a small Markdown report.

Possible path:

```text
runs/<timestamp>-<experiment-id>/Report.md
```

The report should include:

```text
experiment id
status
summary
configuration
key measurements
observations
failure modes
follow-up questions
decision candidates
links to logs and traces
```

The report should not replace the experiment document. It is an execution artifact.

## Data and Output Folder

Recommended generated-output folder:

```text
runs/
```

Example:

```text
runs/
  2026-05-09-153000-associative-memory-toy-model/
    Report.md
    engine.log
    events.jsonl
    traces.jsonl
    memory-fixture.json
    retrieval-comparison.md
```

The `runs/` folder may or may not be committed depending on size and usefulness.

Recommended `.gitignore` entries:

```text
runs/
engine.log
```

`engine.log` is included as a safety net in case a developer accidentally calls the default `engine_logging::initialize()` from the repository root instead of the per-run `initialize_to_path()` helper.

For useful example outputs, create a separate committed folder later:

```text
examples/experiment-runs/
```

## Configuration MVP

A small configuration file may be useful, but should not block the first implementation.

Possible config:

```text
config/default.toml
```

Candidate fields:

```toml
[models]
default_provider = "mock"

[context]
max_fragments = 5
max_estimated_tokens = 1000

[logging]
level = "research"
```

The first version can also use hard-coded defaults plus environment variables.

Avoid building a complex configuration system too early.

## Testing MVP

The first framework should include unit tests for core deterministic logic.

Test candidates:

```text
memory retrieval scoring
context budget selection
tool request validation
calculator tool execution
event serialization
trace serialization
experiment runner dispatch
```

Useful tests:

- associative retrieval returns expected memories
- context manager respects max fragments
- omitted fragments are recorded
- calculator rejects invalid input
- trace IDs connect related events
- mock model produces deterministic output

## Implementation Order

The immediate practical order is:

```text
1. Update this plan to reflect workspace-from-day-one.
2. Add root Cargo.toml.
3. Add crates/qsf_app.
4. Wire qsf_app to engine_logging.
5. Use initialize_to_path for per-run logs.
6. Create placeholder experiment runner.
7. Verify cargo build, cargo build -p engine_logging, cargo test, and cargo run -p qsf_app -- --help.
8. Only after the workspace/logging implementation lands, record accepted workspace and logging decisions in docs/DecisionLog.md.
```

The key sequencing point is that the buildable workspace comes before framework behavior.


### Phase 1: Workspace and Project Skeleton

Goal:

Create a buildable Cargo workspace with the existing `engine_logging` crate and a placeholder `qsf_app` crate.

Tasks:

```text
1. Add root Cargo.toml with [workspace], resolver = "2", [workspace.package], and [workspace.dependencies].
2. Include at minimum log and simplelog in [workspace.dependencies] so engine_logging resolves.
3. Add qsf_app as the initial binary/application crate under crates/qsf_app.
4. Add qsf_app/src/main.rs and qsf_app/src/lib.rs as thin wrappers.
5. Add the initial qsf_app module folders.
6. Add basic CLI entry point with --help output.
7. Add placeholder experiment runner.
8. Wire qsf_app to engine_logging.
9. Add a smoke path that calls engine_logging::initialize_for_tests() from a test or placeholder experiment.
10. Add .gitignore entries for runs/, root engine.log, and local Cargo patch overrides if needed.
11. Commit Cargo.lock because the workspace contains an application/binary.
```

Verification:

```powershell
cargo build
cargo build -p engine_logging
cargo test
cargo run -p qsf_app -- --help
```

Expected result:

The workspace builds end-to-end, `engine_logging` resolves through workspace dependencies, and `qsf_app` has a placeholder way to run experiments.

### Phase 2: Event Log and Trace MVP

Goal:

Create observability before complex behavior.

Tasks:

```text
1. Define event record.
2. Define trace record.
3. Write JSON Lines event log.
4. Write JSON Lines trace log.
5. Create per-run output directory.
6. Initialize developer/operator logging with `engine_logging::initialize_to_path(runs/<run-id>/engine.log)`.
7. Write a minimal report file.
```

Verification:

```text
Run a placeholder experiment and inspect engine.log, events.jsonl, traces.jsonl, and Report.md.
```

Expected result:

Every experiment has observable output from the beginning, with developer logs, events, traces, and reports kept as separate artifacts.

### Phase 3: Experiment Runner MVP

Goal:

Make experiments first-class.

Tasks:

```text
1. Define experiment trait or equivalent.
2. Register named experiments.
3. Add run context.
4. Add output directory creation.
5. Add report writing.
6. Add first placeholder experiment.
```

Verification:

```powershell
cargo run -- experiment associative-memory-toy-model
```

Expected result:

The named experiment runs and writes output.

### Phase 4: Memory and Context MVP

Goal:

Support the first central experiment.

Tasks:

```text
1. Define memory records.
2. Define associations.
3. Add memory fixtures.
4. Implement recency-only retrieval.
5. Implement keyword/tag retrieval.
6. Implement association-weighted retrieval.
7. Define context fragments.
8. Define context budget.
9. Implement context selection and omitted-fragment logging.
10. Write retrieval traces.
```

Verification:

```text
Run Experiment.AssociativeMemoryToyModel.
Run Experiment.ContextBudgetRetrievalTest.
Inspect selected and omitted memories.
```

Expected result:

The framework can compare memory/context strategies.

### Phase 5: Tool-as-Perception MVP

Goal:

Support a safe first tool experiment.

Tasks:

```text
1. Define tool registry.
2. Define tool request.
3. Define tool result.
4. Add permission and side-effect metadata.
5. Implement calculator tool.
6. Add tool trace.
7. Add tool result as candidate context fragment.
```

Verification:

```text
Run Experiment.ToolAsPerceptionCalculator.
Check tool request, result, permission, trace, and context inclusion.
```

Expected result:

The system has a safe, deterministic tool flow.

### Phase 6: Model Role and OpenAI Client MVP

Goal:

Allow experiments to use either mock model behavior or real OpenAI-backed model calls.

Tasks:

```text
1. Define ModelRole.
2. Define ModelRequest.
3. Define ModelResponse.
4. Implement MockModelClient.
5. Add pinned openai_provider_kit dependency.
6. Implement OpenAiProviderModelClient adapter.
7. Read API key from environment.
8. Log model role traces.
9. Avoid logging secrets.
```

Verification:

```text
Run a mock model experiment.
Run a small OpenAI-backed smoke test if API key is available.
Confirm traces include role, latency, and output summary.
Confirm traces do not include API keys.
```

Expected result:

The project can use real models through a replaceable abstraction.

### Phase 7: Sleep Phase MVP

Goal:

Support minimal post-session consolidation.

Tasks:

```text
1. Define sleep input bundle.
2. Define sleep report.
3. Create mock sleep summarizer.
4. Optionally create OpenAI-backed sleep summarizer.
5. Extract memory candidates.
6. Extract open questions.
7. Extract decision candidates.
8. Write sleep trace and report.
```

Verification:

```text
Run Experiment.SleepPhaseSessionSummary.
Inspect sleep report and trace.
Confirm accepted decisions are not created automatically.
```

Expected result:

The framework can perform a controlled, inspectable sleep-phase summary.

### Phase 8: Audio Preparation Layer

Goal:

Prepare the framework for audio without implementing the full audio loop yet.

Tasks:

```text
1. Add audio event types.
2. Add placeholder audio input events.
3. Add latency trace fields for audio.
4. Define where transcription and TTS would enter the runtime loop.
5. Keep real audio implementation deferred.
```

Verification:

```text
Run a simulated audio event through the runtime loop.
Confirm event and trace output.
```

Expected result:

The later streaming transcription and audio MVPs have a clean place to plug in.

### Phase 9: Streaming Transcription MVP

Goal:

Test real-time speech input as structured transcript events before building a full
voice loop.

Tasks:

```text
1. Create Experiment.StreamingTranscriptionMVP.
2. Define a TranscriptProvider abstraction.
3. Implement a simulated transcript provider for deterministic tests.
4. Add an OpenAI realtime transcription adapter using gpt-realtime-whisper.
5. Record partial transcript events.
6. Record final transcript events.
7. Record audio/transcription latency traces.
8. Ensure finalized transcript events enter the runtime loop as normal input events.
9. Avoid logging API keys, raw authorization headers, or unnecessary raw audio.
```

Verification:

```text
Run the simulated transcript provider.
Run the OpenAI-backed provider when OPENAI_API_KEY is available.
Confirm partial and final transcript events are written.
Confirm latency traces identify transcription timing.
Confirm a final transcript event produces a runtime input event without bypassing the reducer.
Confirm reducers remain pure and receive transcript results only as events.
```

Expected result:

The framework can observe live speech as event-stream input without taking on full
speech synthesis, playback, or interruption behavior.

### Phase 10: Realtime Voice Session MVP

Goal:

Evaluate full speech-to-speech presence after transcript-first observability exists.

Tasks:

```text
1. Define a RealtimeSessionProvider abstraction.
2. Add a gpt-realtime-2-backed provider behind that abstraction.
3. Map provider session events into QSF event records.
4. Record preambles, response start, response completion, and interruption events.
5. Keep provider tool calls routed through QSF tool permission boundaries.
6. Trace reasoning effort, latency, model name, and voice/session configuration.
7. Keep transcript, memory, and state updates inside QSF-owned reducers and effects.
```

Verification:

```text
Run a small realtime voice session when OPENAI_API_KEY and audio devices are available.
Inspect events and traces for turn timing, provider events, and interruptions.
Confirm provider events do not bypass QSF state or tool boundaries.
```

Expected result:

The project can test whether gpt-realtime-2 improves perceived presence while keeping
the runtime loop, memory system, and observability model intact.

### Phase 11: Realtime Translation Experiment (conditional on multilingual scope)

Goal:

Keep live translation separate from core audio presence until it becomes a targeted
research question.

Tasks:

```text
1. Create Experiment.RealtimeTranslationMVP only when multilingual presence is in scope.
2. Add a gpt-realtime-translate adapter behind a translation-specific provider boundary.
3. Record source transcript, translated transcript, audio duration, and latency.
4. Keep translation output as perception/context, not an automatic decision or action.
```

Verification:

```text
Run a short controlled translation session.
Inspect source and translated transcript events.
Confirm translation does not bypass memory promotion or context-budget rules.
```

Expected result:

Translation remains available as a future capability without complicating the first
audio or framework milestones.

## Suggested First Implementation Target

Start with:

```text
Experiment.FrameworkSkeletonMVP
```

If that experiment document does not yet exist, create it from `Experiment.Template.md`.

Then immediately run:

```text
Experiment.AssociativeMemoryToyModel
```

This gives the project both a framework skeleton and a meaningful first concept experiment.

## FrameworkSkeletonMVP Suggested Scope

This first implementation-focused experiment should prove that the framework can run one simple experiment and write useful output.

Minimum scope:

```text
experiment runner
event log
trace log
report file
memory fixture
simple retrieval strategy
context budget
```

Out of scope:

```text
OpenAI API
audio
sleep
tools
persistent database
UI
```

This keeps the first milestone achievable.

## Verification Checklist

Before calling the Framework MVP useful, verify:

```text
- The workspace builds.
- `cargo build -p engine_logging` succeeds.
- `cargo run -p qsf_app -- --help` succeeds.
- Tests pass.
- At least one named experiment can run.
- Each run creates an output directory.
- Each run writes a per-run `engine.log` developer/operator log.
- Each run writes an event log.
- Each run writes a trace log.
- Each run writes a Markdown report.
- Memory retrieval can be inspected.
- Context selection can be inspected.
- Omitted context is visible.
- Tool calls can be traced when tool experiments are added.
- Model calls can be mocked.
- OpenAI-backed model calls are behind a replaceable abstraction.
- API keys are not logged by event logs, traces, reports, or `engine_logging` macros.
```

## Documentation Updates During Implementation

During framework implementation, update:

```text
docs/EngineeringDiary.md
  for chronological notes and discoveries.

docs/Plans/Plan.FrameworkMVP.md
  for implementation plan changes.

docs/Experiments/Experiment.*.md
  for planned and completed experiment results.

docs/Architecture/Architecture.*.md
  when implementation clarifies architecture.

docs/DecisionLog.md
  when a design choice becomes accepted.
```

Do not turn every coding discovery into an architecture decision.

## Decision Candidates

The following are likely decision candidates, but should be reviewed before being recorded as accepted decisions:

```text
Candidate: The MVP uses Rust as the implementation language.

Candidate: The first framework uses a named experiment runner.

Candidate: The first logs are JSON Lines.

Candidate: Each experiment run writes a Markdown report.

Candidate: Memory retrieval traces are required for memory experiments.

Candidate: Context assembly must log selected and omitted fragments.

Candidate: The first model interface supports both mock and OpenAI-backed clients.

Candidate: OpenAI access uses openai_provider_kit through a pinned Git dependency.

Candidate: Local provider-kit development uses an uncommitted Cargo patch override.

Candidate: The framework postpones real-time audio until event logging and traces exist.

Candidate: The first real audio integration is streaming transcription, not a full
speech-to-speech voice agent.

Candidate: OpenAI realtime speech integrations are side-effect adapters that emit QSF
events and do not own runtime state, memory promotion, tool permissions, or decisions.

Candidate: `gpt-realtime-whisper` is the first OpenAI realtime speech target,
`gpt-realtime-2` is reserved for later full voice-session experiments, and
`gpt-realtime-translate` remains a separate translation experiment.

Candidate: The MVP uses a Cargo workspace from day one, with framework code in `crates/qsf_app`.

Candidate: `engine_logging` is adopted as the developer/operator logging facade, while structured event and trace logs remain separate.

Candidate: Per-run diagnostic logs are written to `runs/<run-id>/engine.log` through `engine_logging::initialize_to_path`.

Candidate: Keep the `engine_logging` name for now, but revisit whether it should become `qsf_logging` or `foundry_logging`.

Candidate: Remove or rename `set_sim_tick` / `get_sim_tick` before they become depended on.
```

## Open Questions

### RQ-Framework-ExperimentRunnerShape

Should experiments be registered statically in Rust code, or discovered through configuration?

### RQ-Framework-TraceFormat

Is JSON Lines enough for early traces, or should traces be structured differently?

### RQ-Framework-ReportFormat

Should experiment reports be generated automatically, manually, or both?

### RQ-Framework-ModelAbstraction

How thin should the model abstraction be over `openai_provider_kit`?

### RQ-Framework-ContextTokenEstimate

Is rough token estimation enough for early context budget experiments?

### RQ-Framework-Persistence

When should the project introduce persistent storage beyond files?

### RQ-Framework-AudioTiming

Which timing fields must exist before real audio is implemented?

### RQ-Framework-DependencySplit

When should `openai_provider_kit` move from `web_page_filet_mignon` into a dedicated repository?

### RQ-Framework-LoggingScope

Should each experiment's `engine.log` live only under `runs/<run-id>/`, or should there also be a long-lived process-level log for binary startup and lifecycle events outside any experiment?

### RQ-Framework-LogCrateName

Should the project keep `engine_logging`, or rename it to `qsf_logging` / `foundry_logging` before many call sites accumulate?

### RQ-Framework-SimTick

Does the framework need the per-thread tick API inherited from `engine_logging`, or should it be removed or renamed to experiment-step terminology?

### RQ-Framework-LogLevels

Should the architecture-level observability modes such as Minimal, Normal, Research, Replay, and Debug remain documentation-level modes, or should they become a custom logging/tracing configuration separate from Rust `log` levels?

## Deferred Work

Defer these until after the first experiments produce results:

- full speech-to-speech real-time audio implementation
- TTS playback integration
- realtime translation
- video input
- memory database
- embedding search
- UI for memory graph
- sophisticated prompt templates
- complex model role routing
- full sleep-phase consolidation
- persistent self-model
- write-capable tools
- external communication tools
- plugin system
- cloud deployment
- multi-user support

## Safety Boundaries

The Framework MVP should preserve early safety boundaries:

```text
- Tools are read-only or compute-only by default.
- Calculator has no external side effects.
- Write-capable tools are not part of the MVP.
- API keys are not logged by event logs, traces, reports, or `engine_logging` macros.
- Sleep phase does not silently create accepted decisions.
- Model outputs are proposals unless explicitly promoted.
- Local path overrides are not committed.
- Generated logs should be reviewed before sharing.
```

## Expected MVP Outcome

At the end of the Framework MVP, the project should be able to:

```text
1. Run named experiments.
2. Write event logs and traces.
3. Generate Markdown reports.
4. Compare memory retrieval strategies.
5. Select context under a budget.
6. Use a safe calculator tool as perception.
7. Run a minimal sleep-phase summary.
8. Use mock model calls.
9. Optionally use OpenAI-backed model calls through openai_provider_kit.
10. Prepare for later audio-loop experiments.
11. Treat streaming transcription as the first real audio provider integration.
```

The MVP succeeds if it lets the project learn from small experiments.

It does not need to feel like a simulated consciousness yet.
