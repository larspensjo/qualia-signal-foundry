# Engineering Diary

Chronological "what happened" log: every submitted code change, plus research findings,
planning notes, surprises, and open questions encountered during work. This is Stage 1 of
the project workflow; entries may later be promoted to concept notes, research questions,
experiments, or decisions.

How to use:
- Add one entry per logical change. A logical change can span several related commits.
- Every code change submitted must be reflected by some diary entry. Non-code activities
  (research, planning, observations, things tried that did not pan out) also belong here.
- Decisions and commitments belong in `DecisionLog.md`, not here.
- Keep entries short and reference concrete artifacts.
- New entries go to the end of the file.
- If a change implements a prior decision, note it in the Refs line.
- Don't reference planning documents. Entries shall stand on their own, even after plans are archived.

Entry template (only the topic line and summary are mandatory; add other sections when
they apply):

## YYYY-MM-DD - <topic>

<one or two sentence summary>

What changed:
- <bullet>

Observed:
- <bullet>

Open question:
- <bullet>

Refs: <files, commits>; implements: <decision title> (if applicable)

## 2026-05-09 - Workspace skeleton and placeholder app

Phase 1 of the Framework MVP landed: a buildable Cargo workspace pairing the existing
`engine_logging` crate with a thin new `qsf_app` application crate.

What changed:
- Cargo workspace set up with `engine_logging` and `qsf_app` as members.
- `qsf_app` gained a basic CLI, placeholder experiment registration, and
  `engine_logging` integration.
- `.gitignore` extended to cover generated run and log outputs.

Refs: Cargo.toml, Cargo.lock, crates/qsf_app, crates/engine_logging,
docs/Plans/Plan.FrameworkMVP.md

## 2026-05-09 - Event log and trace MVP

Phase 2 of the Framework MVP landed: placeholder experiments now produce separate
per-run artifacts for developer logs, chronological events, explanatory traces, and a
Markdown report.

What changed:
- `RunContext` introduced to own the per-run output directory and the JSONL writers
  for event and trace logs.
- `engine_logging` initialization redirected to `runs/<run-id>/engine.log` per run,
  keeping it as the developer/operator logging layer.
- Per-run Markdown report artifact generated alongside the JSONL streams.

Refs: crates/qsf_app/src/runtime/run_context.rs,
crates/qsf_app/src/observability/event_log.rs,
crates/qsf_app/src/observability/trace.rs,
crates/qsf_app/src/reports/markdown_report.rs

## 2026-05-09 - Diary and decision-log conventions clarified

Reworked the contracts of `EngineeringDiary.md` and `DecisionLog.md` so they no longer
overlap. Diary is now an activity log + observations (every code change plus non-code
work). Decision log is deliberate commitments only.

What changed:
- Diary header rewritten; entry template reshaped around topic / What changed / Observed
  / Open question / Refs.
- Decision log header rewritten; `Type:` and `Implementation | Bug Fix` removed from the
  template since every entry is a decision by construction.
- `ProjectWorkflow.md` Stage 1, Stage 9, and the Document Responsibilities one-liners
  updated to match.

Refs: docs/EngineeringDiary.md, docs/DecisionLog.md,
docs/ProjectFrame/ProjectWorkflow.md;
implements: 2026-05-09 - Diary and decision-log document contracts

## 2026-05-09 - Experiment runner MVP

Named experiments now dispatch through a
first-class runner abstraction instead of a single placeholder function.

What changed:
- Added an `Experiment` trait, explicit registry, run summary, and placeholder
  experiment implementation.
- Moved CLI experiment execution through the runner and kept output artifacts under
  per-run directories.
- Made report sections data-driven so future experiments can provide their own
  observations, failure modes, follow-up questions, and decision candidates.

Refs: crates/qsf_app/src/experiments,
crates/qsf_app/src/runtime/run_context.rs,
crates/qsf_app/src/reports/markdown_report.rs

## 2026-05-10 - Transcript-first realtime speech planning

Accepted a transcript-first path for incorporating OpenAI realtime speech models:
streaming transcription before full speech-to-speech voice sessions.

What changed:
- Added `Experiment.StreamingTranscriptionMVP` as the first real audio provider
  experiment.
- Updated framework, audio architecture, realtime presence, audio research, and backlog
  docs to route realtime speech through QSF events.
- Verified the OpenAI realtime model IDs against current OpenAI API documentation and
  tightened the plan after review.
- Recorded the durable rule that realtime providers are side-effect adapters, not owners
  of runtime state or memory/tool decisions.

Refs: docs/Plans/Plan.FrameworkMVP.md,
docs/Experiments/Experiment.StreamingTranscriptionMVP.md,
docs/Architecture/Architecture.AudioLoop.md,
docs/Concepts/Concept.RealtimePresence.md,
docs/Research/ResearchQuestions.Audio.md,
docs/Experiments/Experiment.Backlog.md,
docs/DecisionLog.md;
implements: 2026-05-10 - Transcript-first realtime speech integration

## 2026-05-10 - Memory and context MVP

Phase 4 now has deterministic in-process memory retrieval and context assembly for
the first two framework experiments.

What changed:
- Added schema-versioned memory records and associations, a small Phase 4 fixture,
  and recency, keyword/tag, and association-weighted retrieval strategies.
- Added context fragments, explicit fragment/token budgets, greedy assembly, and
  omitted-fragment reasons.
- Replaced the associative-memory and context-budget placeholders with real runs
  that write memory/context events, traces, fixture snapshots, and comparison reports.
- Follow-up review fixes made Phase 4 experiment descriptions current, linked
  extra run artifacts from reports, added nanosecond latency fields, and documented
  the first scorer rationale.

Observed:
- Both Phase 4 experiments run end-to-end and produce selected/omitted memory and
  context artifacts for manual review.

Refs: crates/qsf_app/src/memory, crates/qsf_app/src/context,
crates/qsf_app/src/experiments/memory_and_context.rs

## 2026-05-11 - Tool-as-perception MVP

A concrete compute-only tool path replaced the placeholder experiment, and the review follow-up tightened failure observability and removed redundant validation.

What changed:
- Added tool request, permission, metadata, registry, result, and calculator modules under `qsf_app::tools`.
- Replaced the tool-as-perception placeholder with a real calculator experiment that records tool request and completion events, writes a tool invocation trace, and converts the result into a tool-observation context fragment.
- Added `ToolRegistry::validate_and_execute()` so the Phase 5 experiment can capture metadata and result without validating the same request twice.
- Recorded `ToolFailed` when tool validation or execution errors out before the experiment bubbles the error to the runner.
- Added focused tests for request validation, calculator parsing, the end-to-end Phase 5 experiment artifact flow, and malformed calculator input that must write a `ToolFailed` event into `events.jsonl`.

Observed:
- The existing event, trace, and context-budget infrastructure was enough to host tools without widening the runner or report shape.

Refs: crates/qsf_app/src/tools, crates/qsf_app/src/experiments/tool_as_perception_calculator.rs,
crates/qsf_app/src/observability/event_log.rs

## 2026-05-11 - Experiment modules renamed to stable domains

Renamed the experiment source files away from MVP phase labels so the runtime code still reads cleanly after the plan document is deleted.

What changed:
- Renamed the shared Phase 4 experiment module to `memory_and_context.rs`.
- Renamed the Phase 5 experiment module to `tool_as_perception_calculator.rs`.
- Updated experiment module wiring and added a short repo instruction to avoid phase-based names for runtime modules.

Refs: crates/qsf_app/src/experiments/mod.rs,
crates/qsf_app/src/experiments/registry.rs,
crates/qsf_app/src/experiments/memory_and_context.rs,
crates/qsf_app/src/experiments/tool_as_perception_calculator.rs,
Agents.md

## 2026-05-11 - Model role and OpenAI client MVP

`qsf_app` now has typed model roles, deterministic mock-model behavior, and an optional OpenAI-backed adapter that flows through the same event and trace artifacts as the earlier subsystems.

What changed:
- Added `qsf_app::models` modules for role definitions, request/response payloads, a synchronous `ModelClient` boundary, a deterministic `MockModelClient`, and an optional `OpenAiProviderModelClient` backed by `openai_provider_kit`.
- Added `ModelRoleRequested`, `ModelRoleCompleted`, and `ModelRoleFailed` event types plus linked model invocation traces.
- Added a `model-role-smoke-test` experiment that defaults to the mock provider, writes `model-invocation.md`, and can target OpenAI explicitly through configuration when the `openai` Cargo feature is enabled.
- Pinned `openai_provider_kit` in `crates/qsf_app/Cargo.toml` and updated `Cargo.lock`.

Observed:
- The existing run context and report shape were sufficient for model-role observability without widening the experiment runner API.

Refs: Cargo.toml, Cargo.lock, crates/qsf_app/src/models,
crates/qsf_app/src/experiments/model_role_smoke.rs,
crates/qsf_app/src/observability/event_log.rs

## 2026-05-11 - Phase 6 review follow-up fixes

Applied the relevant follow-up fixes from the Phase 6 review without widening the model subsystem scope.

What changed:
- Refactored provider selection so tests can exercise the mock path through explicit provider arguments instead of mutating `QSF_MODEL_PROVIDER` with `unsafe` environment writes.
- Added a `debug_assert!` when provider-reported cached input tokens exceed total input tokens, while preserving the existing clamp in release behavior.
- Added an ignored `openai`-feature smoke test that compiles the real OpenAI adapter path and can run manually when `OPENAI_API_KEY` is available.
- Cleaned the OpenAI adapter imports so default builds remain warning-free under `cargo clippy --all-targets -- -D warnings`.

Observed:
- The provider-selection boundary was already narrow enough that the unsafe-test fix only required a small helper split and a test-only experiment entry point.

Refs: crates/qsf_app/src/models/openai_provider.rs,
crates/qsf_app/src/models/model_client.rs,
crates/qsf_app/src/experiments/model_role_smoke.rs,
docs/Reviews/Review.Phase6.ModelRoleAndOpenAIClient.md

## 2026-05-12 - Sleep phase MVP

A real sleep-phase experiment and subsystem that turns a session transcript into explicit reviewable sleep outputs through the existing model-role boundary.

What changed:
- Added `qsf_app::sleep` modules for sleep input bundles, parsed sleep reports, and session summarization through the shared `SleepSummarizer` model role.
- Replaced the sleep-phase placeholder with a real `sleep-phase-session-summary` experiment that records sleep request/completion events, writes a dedicated sleep trace, and emits `sleep-report.json` plus `sleep-report.md` artifacts.
- Extended the mock sleep summarizer fixture so deterministic runs exercise structured memory candidates and review notes.
- Added focused tests for sleep-report parsing, session summarization, and end-to-end Phase 7 artifact generation.

Observed:
- The Phase 6 model-role path was already sufficient for sleep summarization, so Phase 7 only needed sleep-specific types, parsing, and artifacts rather than a second model abstraction.

Refs: crates/qsf_app/src/sleep, crates/qsf_app/src/experiments/sleep_phase_session_summary.rs,
crates/qsf_app/src/models/mock_model.rs,
crates/qsf_app/src/observability/event_log.rs;
implements: 2026-05-11 - Model access uses explicit roles and optional provider adapters

## 2026-05-12 - Audio preparation layer

A concrete simulated audio boundary that shows how transcript input and speech playback will plug into the existing event and trace model before any real audio provider is added, with the review follow-up kept inside the same logical Phase 8 change.

What changed:
- Added `qsf_app::audio` with simulated audio session data, explicit transcript and playback runtime boundary definitions, and placeholder audio latency measurements.
- Extended shared observability with typed audio events plus `latency_domain` and `latency_stage` fields on traces so audio timing can stay first-class.
- Added an `audio-preparation-layer` experiment that emits simulated audio events, records linked transcription and playback traces, and writes `audio-preparation.md`.
- Changed `AudioRuntimeBoundary.description` from `&'static str` to `String` and added a JSON round-trip test.
- Documented that `LatencyMeasurementRecorded` intentionally duplicates stage timing already summarized in traces because the event log records chronology while traces record rationale.
- Marked `SpeechPlaybackRequested` and `AudioTranscriptionFailed` as boundary and future-work placeholders in the shared event type enum.

Observed:
- The existing runner, event log, and trace log were already sufficient for audio preparation work; Phase 8 only needed typed boundary definitions and a deterministic simulation path.

Refs: crates/qsf_app/src/audio, crates/qsf_app/src/experiments/audio_preparation_layer.rs,
crates/qsf_app/src/observability/event_log.rs,
crates/qsf_app/src/observability/trace.rs,
docs/Reviews/Review.Phase8.AudioPreparationLayer.md

## 2026-05-12 - Streaming transcription MVP start

Deterministic transcript-provider boundary that emits partial
and final transcript events before bridging finalized text into normal runtime input.

What changed:
- Added a `TranscriptProvider` contract, deterministic simulated provider, transcript
  session data, and an OpenAI realtime provider target constant for
  `gpt-realtime-whisper`.
- Registered the `streaming-transcription-mvp` experiment and report artifact.
- Added traces for provider session timing, transcription latency, and final
  transcript-to-runtime input dispatch.

Observed:
- Phase 8's audio event names and latency trace fields were sufficient for the first
  transcript-first implementation slice.

Refs: crates/qsf_app/src/audio/transcript_provider.rs,
crates/qsf_app/src/experiments/streaming_transcription_mvp.rs,
crates/qsf_app/src/experiments/registry.rs

## 2026-05-12 - OpenAI realtime transcription adapter

Implemented the provider-backed path for OpenAI realtime transcription with
environment-selected simulated, prerecorded WAV, and live microphone input sources.

What changed:
- Added a feature-gated OpenAI Realtime WebSocket transcript provider that streams
  base64 PCM16 chunks and records partial/final transcript revisions as existing audio
  events.
- Added prerecorded WAV validation for 24 kHz mono PCM and a bounded live microphone
  capture path for evaluation runs.
- Routed the streaming transcription experiment through `TranscriptProviderRequest::from_env`
  so real inputs are selectable without code edits.
- Kept provider timings relative to session start so latency traces remain comparable
  with the simulated provider.

Observed:
- The provider boundary stayed compatible with the existing transcript-to-runtime bridge;
  most of the change was isolated to the side-effect adapter.

Refs: crates/qsf_app/src/audio/transcript_provider.rs,
crates/qsf_app/src/audio/mod.rs,
crates/qsf_app/src/experiments/streaming_transcription_mvp.rs

## 2026-05-13 - Phase 9 OpenAI feature compile check

Fixed the feature-gated OpenAI realtime transcription path so Phase 9 can be
compiled before real WAV or microphone evaluation.

What changed:
- Updated CPAL 0.17 microphone configuration and device lookup usage.
- Updated Tungstenite text WebSocket messages to use the current message payload type.

Observed:
- The simulated Phase 9 tests still pass, and the OpenAI feature path now compiles
  through its local-input validation test.

Refs: crates/qsf_app/src/audio/transcript_provider.rs

## 2026-05-13 - Real WAV realtime transcription evaluation

Ran the Phase 9 streaming transcription experiment against a converted prerecorded
WAV file and corrected the OpenAI Realtime transcription adapter to match the GA
WebSocket surface.

What changed:
- Fixed WebSocket request construction so Tungstenite owns the handshake headers and
  the adapter only adds OpenAI authorization.
- Removed the beta realtime header and model query parameter for GA transcription
  sessions.
- Updated the session configuration to send `session.update` with
  `session.type = "transcription"` and `audio.input` transcription settings.
- Made the streaming transcription experiment test independent of ambient
  `QSF_TRANSCRIPT_*` environment variables.

Observed:
- `output.wav` transcribed successfully through `gpt-realtime-whisper` as
  "Hello, this is an example of an audio recording."
- The completed run emitted partial transcript events, an `AudioFinalTranscript`,
  and the expected bridged `InputReceived` event without logging raw audio or secrets.

Refs: crates/qsf_app/src/audio/transcript_provider.rs,
crates/qsf_app/src/experiments/streaming_transcription_mvp.rs,
runs/2026-05-13-184740-streaming-transcription-mvp

## 2026-05-13 - Live microphone realtime transcription milestone

Validated the Phase 9 streaming transcription experiment end to end with live
microphone input through the OpenAI Realtime provider.

What changed:
- Ran `streaming-transcription-mvp` with `QSF_TRANSCRIPT_INPUT_SOURCE=mic` and the
  default microphone device.
- Confirmed the provider emitted partial transcript events, one final transcript,
  latency traces, and the bridged `InputReceived` runtime event.
- Treated this as the Phase 9 live-input validation milestone.

Observed:
- The live run completed successfully with `gpt-realtime-whisper` and produced the
  final transcript: "recording. I don't know how long it's going to".
- Safety markers stayed clean: no raw audio, API key, or authorization data was logged.
- Captured chunk timing suggests microphone capture duration should be revisited later,
  but it does not block closing Phase 9.

Refs: runs/2026-05-13-190700-streaming-transcription-mvp,
crates/qsf_app/src/audio/transcript_provider.rs,
crates/qsf_app/src/experiments/streaming_transcription_mvp.rs

## 2026-05-13 - Audio module review follow-up

Applied relevant follow-ups from the audio module review after Phase 9 live
validation.

What changed:
- Rechecked the realtime transcription model choice against the official OpenAI model
  catalog and kept `gpt-realtime-whisper` as the default because Phase 9 prioritizes
  low-latency live transcript deltas.
- Documented `gpt-4o-transcribe` as an accuracy-oriented comparison target rather
  than the Phase 9 default.
- Reused one OpenAI realtime Tokio runtime and returned a structured error if the
  synchronous provider is called from inside an existing Tokio runtime.
- Made microphone capture handle `i16`, `f32`, and `u16` input sample formats.
- Logged malformed realtime server events instead of silently dropping parse failures.
- Derived the WebSocket connect timeout from the configured realtime timeout and
  strengthened best-effort credential redaction.

Observed:
- The Phase 8 simulator duplication and a fuller async-provider redesign remain
  larger follow-up topics; they were not needed to stabilize the completed Phase 9 path.

Refs: docs/Reviews/Review.AudioModule.md,
crates/qsf_app/src/audio/transcript_provider.rs,
docs/Plans/Plan.FrameworkMVP.md,
docs/Experiments/Experiment.StreamingTranscriptionMVP.md

## 2026-05-14 - Realtime voice session MVP

Added a realtime voice-session provider boundary and a default simulated experiment
path for observing speech-to-speech lifecycle events without handing runtime state or
tool execution to the provider, then applied the relevant Phase 10 review fixes before
commit.

What changed:
- Added `RealtimeSessionProvider` with simulated and feature-gated OpenAI realtime
  implementations targeting `gpt-realtime-2`.
- Registered `realtime-voice-session`, which records provider session lifecycle,
  preambles, response start/completion, interruptions, speech playback metadata,
  provider tool-call requests, and audio latency traces.
- Added a dedicated experiment document and regression tests for event mapping,
  no-secret/no-raw-audio markers, sanitized provider failures, and the OpenAI feature
  compile path.
- Removed the duplicate `RuntimeInputDispatch` latency measurement from voice-session
  provider timing so provider response-start latency is not counted twice.
- Added prerecorded WAV validation before OpenAI realtime voice WebSocket setup, keeping
  the feature-gated validation test from accidentally making a network connection.
- Moved observed output-byte metadata out of `SpeechPlaybackRequested`, documented the
  new realtime event variants, documented simulated-provider input-source behavior, and
  added parser/serialization tests for realtime session types.
- Fixed the first live OpenAI microphone failure by including
  `session.audio.output.format.rate` in the realtime voice session update and accepting
  both GA `response.output_audio*` and older `response.audio*` server event names.
- Fixed the next live OpenAI validation failure by changing the realtime voice default
  output modality from `["audio", "text"]` to the supported speech-to-speech
  combination `["audio"]`.
- Removed `response.modalities` from realtime voice `response.create`; the current
  WebSocket API takes output modality from session configuration.
- Enabled input transcription metadata for realtime voice sessions and changed the
  non-simulated transcript fallback from the prompt text to `<no-input-transcript>` so
  QSF artifacts do not mislabel the configured prompt as user speech.
- Printed and flushed a live microphone "Speak now" cue after the CPAL input stream
  starts so manual realtime tests have a clear beginning-of-capture signal.
- Fixed realtime voice latency recording for cases where provider response generation
  starts before asynchronous input transcription completes, and added an explicit
  response-start offset field to traces and reports.

Observed:
- The existing event and trace layers were sufficient for voice sessions once
  provider tool calls were represented as QSF `ToolRequested` events with automatic
  execution disabled.
- The first live OpenAI run failed before response generation because output PCM format
  now requires an explicit sample rate, matching the input PCM configuration.
- Realtime voice sessions can request either text or audio output, but not both in the
  same `modalities` list; audio transcript events remain the text observation path for
  voice runs.
- The GA realtime WebSocket response-creation shape is stricter than the session shape:
  session configuration accepts `output_modalities`, while `response.create` rejects
  a `response.modalities` field.
- In the first successful microphone run, the model responded as if it heard the later
  counting, but the artifact's input transcript was a fallback prompt because input
  transcription was not enabled yet.
- Manual microphone tests need an explicit capture-start cue because process startup,
  realtime session setup, and audio stream setup happen before user speech is captured.
- Successful live voice runs showed that response generation can begin before the final
  input transcript event arrives, so the trace needs to represent that ordering instead
  of forcing a final-transcript-before-response timeline.

Refs: crates/qsf_app/src/audio/voice_session_provider.rs,
crates/qsf_app/src/experiments/realtime_voice_session.rs,
crates/qsf_app/src/observability/event_log.rs,
docs/Experiments/Experiment.RealtimeVoiceSessionMVP.md,
docs/Reviews/Review.Phase10.RealtimeVoiceSessionMVP.md

## 2026-05-14 - Text-owned voice loop first pass

Added a deterministic voice-loop experiment where simulated speech input becomes QSF
runtime input, a `ConversationalResponder` model role owns the answer text, and a
simulated speech output provider receives that exact `OutputProduced` text.

What changed:
- Registered `text-owned-voice-loop` and added the end-to-end experiment artifact.
- Added `ConversationalResponder` role defaults and a deterministic mock response.
- Added `SpeechOutputProvider` with a simulated metadata-only implementation reusing
  existing playback timing fixtures.
- Added regression tests for event ordering, final-transcript-only input commit,
  session correlation, exact text handoff, model failure, speech failure sanitization,
  and no raw-audio-like event payloads.

Observed:
- The existing transcript provider, context assembler, model-role helper, and event/trace
  writers were enough to build the deterministic voice loop without adding a parallel
  runtime path.

Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
crates/qsf_app/src/audio/speech_output_provider.rs,
crates/qsf_app/src/models/model_role.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md,
docs/Architecture/Architecture.AudioLoop.md

## 2026-05-14 - Text-owned voice loop review follow-up

Applied the relevant first-pass review findings before moving on to live microphone
input.

What changed:
- Extracted shared transcript event emission for streaming transcription and the
  text-owned voice loop.
- Added shared audio test helpers for event parsing, safety-marker assertions, and
  raw-audio-like payload checks.
- Added optional `session_id` metadata to model requests so model role events can
  correlate with a voice-loop turn.
- Added speech playback safety markers, default speech-output model materialization,
  stronger event-order tests, and a consistent deterministic latency timeline.
- Documented the voice-turn runtime shape in the runtime-loop architecture note.

Observed:
- The review fixes removed duplication without changing the public experiment command
  or starting Slice 3 live microphone work.

Refs: crates/qsf_app/src/audio/transcript_event_emitter.rs,
crates/qsf_app/src/audio/test_support.rs,
crates/qsf_app/src/models/model_client.rs,
crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
crates/qsf_app/src/experiments/streaming_transcription_mvp.rs,
docs/Architecture/Architecture.RuntimeLoop.md

## 2026-05-14 - Text-owned voice loop live microphone input attempt

Started Slice 3 by running the text-owned voice loop with OpenAI realtime microphone
transcription while keeping the responder mock-backed and speech output simulated.

What changed:
- Verified the OpenAI transcript provider still compiles with the text-owned loop
  refactor.
- Ran live microphone input through `text-owned-voice-loop` with
  `QSF_TRANSCRIPT_PROVIDER=openai` and `QSF_TRANSCRIPT_INPUT_SOURCE=mic`.
- Added an empty-final-transcript guard so silence or unusable live transcription
  records `AudioTranscriptionFailed` and stops before `InputReceived`.
- Added a regression test for the empty transcript guard.
- Documented the live microphone command and observed failure mode in the experiment
  document.

Observed:
- The live provider path connected successfully, but local microphone evaluations
  returned empty final transcripts rather than useful speech text.
- The guarded failure run correctly emitted `AudioTranscriptionFailed` and did not
  emit `InputReceived`, model role, output, or speech playback events.

Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md,
runs/2026-05-14-112211-text-owned-voice-loop

## 2026-05-14 - Text-owned voice loop live microphone success

Validated Slice 3 with a live microphone run through the OpenAI realtime transcript
provider while keeping the QSF responder mock-backed and speech output simulated.

Observed:
- The run transcribed "Tell me something about yourself." as the final transcript.
- Six partial transcript revisions were recorded before `AudioFinalTranscript`.
- The final transcript became `InputReceived`, context was assembled, the
  `ConversationalResponder` mock role produced QSF-owned text, and the simulated
  speech output provider received exactly that text.
- First partial transcript latency was 1648 ms, final transcript latency was 2923 ms,
  and total text-owned voice-loop latency was 3069 ms with simulated speech output.

Refs: runs/2026-05-14-113329-text-owned-voice-loop,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md

## 2026-05-14 - Text-owned voice loop stdout response

Printed the QSF-owned `OutputProduced` text to stdout for successful text-owned voice
loop runs.

What changed:
- Added a concise console line after `OutputProduced` is recorded so simulated speech
  output runs still show the answer without opening `text-owned-voice-loop.md`.
- Documented the stdout behavior in the experiment note.

Observed:
- The speech output provider remains metadata-only; stdout only mirrors the QSF-owned
  text response for manual live-test ergonomics.

Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md

## 2026-05-14 - Text-owned voice loop OpenAI responder run

Validated the text-owned voice loop with live microphone transcription and an
OpenAI-backed `ConversationalResponder`, while keeping speech output simulated.

Observed:
- The live run transcribed "Tell me something funny and unexpected about yourself."
- `ModelRoleRequested` and `ModelRoleCompleted` used the `openai` provider, resolving
  to `gpt-5.4-nano-2026-03-17`.
- The OpenAI model response was emitted as QSF-owned `OutputProduced` text before the
  simulated speech provider received the exact same text.
- Model latency was 1937 ms with 89 input tokens and 45 output tokens.
- Total loop latency was 4997 ms with simulated speech output metadata.

Refs: runs/2026-05-14-113743-text-owned-voice-loop,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md

## 2026-05-14 - Voice loop comparison report

Created the Slice 5 comparison report using one streaming transcription run, one
provider-owned realtime voice run, and one text-owned voice loop run.

What changed:
- Compared transcript latency, response ownership, context participation, speech output,
  tool boundary, and safety boundary across the three run artifacts.
- Documented that text-owned voice now proves live speech can enter QSF-owned context
  and model-role response generation before speech output receives exact text.
- Kept OpenAI TTS deferred; the report recommends improving comparison baselines and
  adding richer memory/context participation first.

Refs: docs/Experiments/Report.VoiceLoopComparison.2026-05-14.md,
runs/2026-05-14-133230-streaming-transcription-mvp,
runs/2026-05-14-075918-realtime-voice-session,
runs/2026-05-14-113743-text-owned-voice-loop

## 2026-05-14 - Same-prompt realtime voice comparison

Updated the voice-loop comparison with a same-prompt realtime voice-session run.

What changed:
- Replaced the older realtime baseline in the comparison report with
  `runs/2026-05-14-133853-realtime-voice-session`.
- Compared provider-owned realtime voice, streaming transcription, and text-owned voice
  on the spoken prompt "Tell me something funny and unexpected about yourself."
- Recorded that the realtime provider began response generation 246 ms before final
  transcript completion and observed 480000 provider audio bytes.

Refs: docs/Experiments/Report.VoiceLoopComparison.2026-05-14.md,
runs/2026-05-14-133853-realtime-voice-session

## 2026-05-14 - Text-owned voice loop memory context

Wired the text-owned voice loop to retrieve one association-weighted memory candidate
after `InputReceived` and before context assembly.

What changed:
- Reused the Phase 4 memory fixture and retrieval scorer in the voice-loop answer path.
- Added `MemoryRetrievalRequested` and `MemoryRetrieved` events plus a
  `voice-memory-retrieval` trace to the run artifacts.
- Included the selected memory context id in `text-owned-voice-loop.md` and kept the
  exact `OutputProduced` to speech-output handoff unchanged.

Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md,
docs/Experiments/Report.VoiceLoopComparison.2026-05-14.md

## 2026-05-14 - Text-owned voice loop latency accounting

Fixed the generated text-owned voice-loop latency summary to include model-role
runtime.

What changed:
- Measured the `ConversationalResponder` invocation inside the voice-loop experiment.
- Added memory retrieval, context assembly, model role, speech output, and total
  observed turn latency fields to the latency event and generated markdown report.
- Added a delayed mock-model regression test so total turn latency cannot undercount
  model runtime again.

Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md,
docs/Experiments/Report.VoiceLoopComparison.2026-05-14.md

## 2026-05-14 - Corrected voice-loop comparison baseline

Updated the voice-loop comparison report to use the corrected memory-context
text-owned run as the current QSF-owned baseline.

What changed:
- Replaced the current text-owned baseline with
  `runs/2026-05-14-140617-text-owned-voice-loop`.
- Kept the earlier same-prompt text-owned run as historical context for transcript
  timing comparisons.
- Documented selected memory context, corrected model latency, and total observed turn
  latency from the live run.

Refs: docs/Experiments/Report.VoiceLoopComparison.2026-05-14.md,
runs/2026-05-14-140617-text-owned-voice-loop

## 2026-05-15 - Text-owned voice loop report diagnostics

Added a generated diagnostics section to each text-owned voice-loop run report.

What changed:
- `text-owned-voice-loop.md` now reports response owner, selected memory context,
  model provider/model latency, exact speech handoff, total observed turn latency, and
  raw-audio logging status.
- Covered the diagnostics section in the deterministic voice-loop regression test.
- Updated the experiment and comparison docs so the next step is moving beyond the
  Phase 4 memory fixture.

Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md,
docs/Experiments/Report.VoiceLoopComparison.2026-05-14.md
