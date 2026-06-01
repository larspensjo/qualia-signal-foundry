# Engineering Diary

Chronological "what happened" log: every submitted code change, plus research findings,
planning notes, surprises, and open questions encountered during work. This is Stage 1 of
the project workflow; entries may later be promoted to concept notes, research questions,
experiments, or decisions.

## Instructions how to use
- Add one entry per logical change. A logical change can span several related commits.
- Every code change submitted must be reflected by some diary entry. Non-code activities
  (research, planning, observations, things tried that did not pan out) also belong here.
- Decisions and commitments belong in `DecisionLog.md`, not here.
- Keep entries short and reference concrete artifacts.
- New entries go to the end of the file.
- If a change implements a prior decision, note it in the Refs line.
- Don't reference planning documents. Entries shall stand on their own, even after plans are archived.
- There is no need for entries when meta documents are created. E.g. plans or ideas. Only changes to the application.
- Do not modify older entries if they were commited.

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

Refs: Cargo.toml, Cargo.lock, crates/qsf_app, crates/engine_logging

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

Refs:docs/Experiments/Experiment.StreamingTranscriptionMVP.md,
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
crates/qsf_app/src/audio/transcript_provider.rs
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

## 2026-05-15 - File-backed voice memory source

Added an opt-in file-backed memory source for the text-owned voice loop.

What changed:
- Introduced a `VoiceLoopMemorySource` boundary with deterministic
  `phase_four_fixture` default behavior.
- Added `QSF_VOICE_MEMORY_SOURCE=file` plus `QSF_VOICE_MEMORY_FILE=<path>` for loading
  a JSON `MemoryFixture`.
- Wrote the loaded memory source to `voice-memory-source.json` and added memory source,
  record count, and retrieval strategy to generated diagnostics.
- Added a regression test proving a file memory source can drive selected context.

Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs,
crates/qsf_app/src/memory/fixtures.rs,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md

## 2026-05-15 - Repeatable voice memory fixture

Added a small file-backed voice memory fixture for repeatable retrieval tests.

What changed:
- Created `docs/Experiments/Fixtures/voice-memory.example.json` with five
  project-grounded memory records and four associations.
- Documented the fixture path in the text-owned voice-loop experiment note.

Refs: docs/Experiments/Fixtures/voice-memory.example.json,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md

## 2026-05-15 - Live file-backed voice memory validation

Validated the text-owned voice loop against the repeatable file-backed voice memory
fixture.

Observed:
- Live microphone input transcribed "What do you remember about voice loop ownership
  and memory source configuration?"
- The file memory source `docs\Experiments\Fixtures\voice-memory.example.json` loaded
  five records and four associations.
- Association-weighted retrieval selected `memory.voice-memory-source`, and the answer
  reflected explicit source-boundary and configuration behavior.
- Diagnostics reported `Exact speech handoff: true`, model latency 2589 ms, and total
  observed turn latency 6734 ms.

Refs: runs/2026-05-15-090612-text-owned-voice-loop,
docs/Experiments/Fixtures/voice-memory.example.json,
docs/Experiments/Experiment.TextOwnedVoiceLoop.md,
docs/Experiments/Report.VoiceLoopComparison.2026-05-14.md

## 2026-05-15 - Experiments review cleanup

Applied the relevant experiments-module review findings.

What changed:
- Removed direct stdout logging from the text-owned voice loop.
- Shared transcript runtime-boundary construction, sanitized failure recording,
  retrieved-memory ID extraction, and saturating elapsed-time helpers.
- Replaced phase-numbered generated prose with stable behavior names.
- Made experiment registry dispatch exhaustive and softened brittle event-count tests.

Refs: docs/Reviews/experiments-module-review-2026-05-15.md,
crates/qsf_app/src/experiments,
crates/qsf_app/src/audio/mod.rs,
crates/qsf_app/src/observability/trace.rs

## 2026-05-16 - Reviewed memory draft conversion

Added an explicit conversion experiment that turns provisional sleep memory candidates
into a separate reviewable `MemoryFixture` draft without connecting it to live voice
memory.

What changed:
- Added deterministic sleep-candidate to memory-record conversion with fallback source
  references, default provisional importance, token estimates, and empty associations.
- Registered `reviewed-memory-draft`, selected by `QSF_REVIEWED_MEMORY_SLEEP_REPORT`,
  and wrote `reviewed-memory-draft.json` plus `reviewed-memory-draft.md` into the
  conversion run directory.
- Covered structured candidates, string-only candidates, empty candidate lists,
  deterministic ids, importance clamping, Markdown candidate indexes, and conversion-run
  artifact placement.
- Validated current memory and association schemas during draft writing and in tests.
- Kept source-reference fallbacks tied to the original run directory name, documented
  why they differ from sanitized memory id segments, and fixed title derivation for
  newline-delimited summaries.
- Moved `QSF_REVIEWED_MEMORY_SLEEP_REPORT` handling out of the memory module and into
  the experiment boundary.

Observed:
- Experiment and registry descriptions still duplicate the same text; that is a
  broader pre-existing registry pattern to clean up later.

Refs: crates/qsf_app/src/memory/reviewed_memory_draft.rs,
crates/qsf_app/src/experiments/reviewed_memory_draft.rs,
crates/qsf_app/src/experiments/registry.rs,
crates/qsf_app/src/memory/mod.rs,
docs/DecisionLog.md; implements: 2026-05-16 - Sleep-to-memory conversion is explicit and separate

## 2026-05-16 - Expanded reviewed memory artifact

Expanded the reviewed memory draft Markdown so candidate records can be reviewed without
opening the JSON fixture.

What changed:
- Added source report path, draft JSON path, explicit provisional review policy, and
  the OpenAI-feature file-backed voice test command to `reviewed-memory-draft.md`.
- Rendered each generated memory record with candidate index, compact per-candidate
  review checkboxes, record id, kind, importance, source reference, generated tags,
  estimated tokens, reinforcement count, and summary.
- Extended tests so Stage 2 review policy, per-record content, and generated commands
  are covered separately.

Refs: crates/qsf_app/src/memory/reviewed_memory_draft.rs,
crates/qsf_app/src/experiments/reviewed_memory_draft.rs

## 2026-05-16 - Reviewed draft file-backed voice validation

Added a deterministic voice-loop regression test for using a reviewed memory draft as
the explicit file-backed voice memory source.

What changed:
- Built a reviewed-draft-shaped `MemoryFixture` and loaded it through the existing
  file-backed voice memory source.
- Asserted the Stage 3 success criteria in generated voice-loop artifacts: file memory
  source, expected record count, selected reviewed draft memory, answer text reflecting
  that memory, exact speech handoff, and no raw-audio logging.

Refs: crates/qsf_app/src/experiments/text_owned_voice_loop.rs

## 2026-05-16 - Reviewed memory association drafts

Added provisional association suggestions to reviewed memory draft conversion.

What changed:
- Sleep reports now accept optional `association_candidates` with 1-based memory
  candidate endpoints, weight, and reason.
- Reviewed memory conversion includes only valid draft associations whose endpoints
  exist, whose reason is non-empty, whose weight is strong enough, and whose inclusion
  stays within a small draft graph limit.
- Omitted or weak association suggestions are kept visible in the Markdown review
  artifact with omission reasons instead of silently disappearing.
- Sleep reports and reviewed draft Markdown now show association candidate sections.
- Added a file-backed voice-loop test proving retrieval traces expose association paths
  when a draft association influences selected memory.

Refs: crates/qsf_app/src/sleep/sleep_report.rs,
crates/qsf_app/src/sleep/session_summary.rs,
crates/qsf_app/src/experiments/sleep_phase_session_summary.rs,
crates/qsf_app/src/memory/reviewed_memory_draft.rs,
crates/qsf_app/src/experiments/reviewed_memory_draft.rs,
crates/qsf_app/src/experiments/text_owned_voice_loop.rs

## 2026-05-16 - Acceptance workflow for reviewed memory drafts

Implemented an explicit acceptance experiment that promotes a reviewed draft into the durable voice-memory fixture.

What changed:
- Added `AcceptReviewedMemory` experiment variant, registered as CLI experiment
  `accept-reviewed-memory`.
- The experiment reads a draft via `QSF_ACCEPT_MEMORY_DRAFT` env var, validates both
  memory record and association schemas, and writes the accepted fixture to
  `docs/Experiments/Fixtures/voice-memory.reviewed.json`.
- Schema validation runs before the target file is written; malformed or
  wrong-version drafts fail without touching the durable fixture.
- Acceptance is an explicit, user-initiated step; sleep output never promotes
  automatically.
- Five unit tests cover: normal acceptance, empty drafts, associations,
  malformed JSON rejection, and schema version rejection.

Refs: crates/qsf_app/src/experiments/accept_reviewed_memory.rs,
crates/qsf_app/src/experiments/registry.rs,
crates/qsf_app/src/experiments/mod.rs;
implements: Stage 5 of docs/Plans/Plan.ReviewedMemoryPromotion.md

## 2026-05-17 - Multi-turn text loop stage 1

Added the first human-driven multi-turn text experiment with append-only session state,
cache-stable prompt assembly, per-turn memory retrieval, and deterministic mock-model
coverage. The final implementation also tightens fallback observability, reducer tests,
and prompt-prefix assertions to match the experiment contract directly.

What changed:
- Registered `multi-turn-text-loop` and added `SessionState`, `Turn`, `SessionEvent`,
  and a pure `reduce_session` path for session lifecycle events.
- Added `conversation::prompt` with a stable session system prompt, length-prefixed
  SHA-256 request hashing, and prior-request prefix verification.
- Wired the turn loop through association-weighted memory retrieval, context assembly,
  `ConversationalResponder`, session events, token/latency capture, and a generated
  multi-turn report.
- Missing `QSF_SESSION_MEMORY_FILE` now records an `ErrorOccurred` fallback event before
  loading the deterministic fixture, and the report distinguishes requested from loaded
  memory source.
- Removed duplicate model-latency observability and rely on the shared
  `ModelRoleCompleted` emission for usage and latency.
- Avoided per-event `SessionState` clones in orchestration and cleaned up prompt hash
  rendering and usage extraction.
- Covered prompt stability, reducer behavior, and a three-turn mock-model integration
  run with event/report assertions, including direct turn-to-turn prompt prefix hash
  checks and non-mutating memory/context reducer events.

Observed:
- The existing model, memory, context, event, and trace boundaries were sufficient for
  the stage-1 text loop without changing provider selection semantics.

Refs: crates/qsf_app/src/conversation, crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/experiments/registry.rs, crates/qsf_app/src/observability/event_log.rs

## 2026-05-17 - Multi-turn text loop warm summaries

Added the warm tier for long multi-turn text sessions so older completed turns can be
summarized into a stable in-session system-prompt block while completed turn records
remain append-only. The final implementation includes the Stage 2 review fixes for
default ageing, report diagnostics, summary model selection, and warm-tier bookkeeping.

What changed:
- Added `QSF_SESSION_WARM_THRESHOLD` with a default of six active verbatim turns so
  default runs exercise summarization before the ten-turn session limit.
- Added session-local `TurnSummary` records, a `session_turn_summarizer` model role,
  mock summarizer output, and `TurnSummarized` observability.
- `SessionTurnSummarizer` uses its role default model instead of inheriting the
  conversational responder model.
- Prompt assembly now renders warm summaries as an "earlier in this session" block
  and skips the prefix-hash assertion for the first prompt after an ageing event.
- Summary bookkeeping uses the append-only prefix invariant directly, while completed
  `TurnCompleted` records remain available for reporting.
- Extended the generated multi-turn report with warm-threshold configuration and
  summary diagnostics, including intentional warm-summary cache-prefix invalidations.

Observed:
- Focused mock-session tests cover ageing the oldest turn while retaining completed
  `TurnCompleted` records, `TurnSummarized` payload shape, multiple aged-out summaries,
  and prefix stability resuming after an intentional warm-summary invalidation.

Refs: crates/qsf_app/src/conversation, crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/models, crates/qsf_app/src/observability/event_log.rs,
docs/DecisionLog.md;
implements: 2026-05-17 - Multi-turn warm tier ages by active turn count

## 2026-05-17 - Multi-turn warm tier verification

Verified Stage 2 before starting recall-tool work with automated checks and a deterministic
eight-turn CLI run that crossed the default warm threshold.

Observed:
- Run `runs/2026-05-17-053037-multi-turn-text-loop` completed with eight appended
  turns, two `TurnSummarized` events, no `SessionLimitReached` event, and warm summaries
  produced by `gpt-5.4-nano`.
- The generated report recorded `invalidated_by_warm_summary` for the intentional cache
  prefix invalidation after the first ageing event.

Refs: runs/2026-05-17-053037-multi-turn-text-loop

## 2026-05-17 - Multi-turn recall tool

Implemented the recall-tool stage for the multi-turn text loop so summarized turns can
be expanded back into verbatim session text on demand.

What changed:
- Extended the model boundary with declared tool definitions, tool-call responses, and
  tool messages in prompt assembly.
- Added a scoped `recall_turn` tool to the multi-turn text loop; it records
  `ToolRequested`, `ToolExecuted`, and `ToolFailed` events and freezes successful
  recalls into completed turn records.
- Updated the generated report with recall-tool execution counts and per-call
  diagnostics.
- Restored `PromptAssembled` event ordering before model requests, moved prompt byte
  accounting next to canonical hashing, and made multi-round tool-call follow-ups fail
  without appending a turn.
- Expanded model-role failure logging to preserve the provider error chain in events
  and traces.
- Added deterministic mock-model coverage for summarized-turn recall, active-turn
  recall failure, follow-up tool-call failure, reducer non-mutation, and prompt-prefix
  stability with recalled tool messages.

Observed:
- The current OpenAI provider wrapper only exposes plain chat messages, so real
  provider-native function calling still needs adapter support before live tool-use
  evaluation.

Refs: crates/qsf_app/src/models, crates/qsf_app/src/conversation/prompt.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/observability/event_log.rs;
implements: 2026-05-17 - Multi-turn recall is scoped to summarized turns

## 2026-05-17 - Recall tool registry migration

Moved the multi-turn `recall_turn` capability behind the shared tool registry while
preserving the existing session reducer and prompt-freezing behavior.

What changed:
- Added a typed tool execution context hook plus a session-aware `RecallTurnTool`
  registered beside the calculator.
- Extended `ToolRequest` with structured arguments so model tool-call JSON can be
  marshalled into registry requests without changing calculator callers.
- Replaced inline multi-turn recall execution with `ToolRegistry::validate_and_execute`
  and recorded registry category and side-effect metadata on recall tool events.
- Moved multi-turn session state records into a shared `session` module so tools do not
  depend on the experiment driver.
- Reused one `is_turn_summarized` predicate and avoided double validation before recall
  tool execution.

Observed:
- Borrowed session state cannot be downcast through `std::any::Any` because `Any`
  requires `'static`; the context trait now exposes a narrow session-state accessor
  instead of an `as_any` downcast hook.

Refs: crates/qsf_app/src/session,
crates/qsf_app/src/tools,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs

## 2026-05-18 - Model tool allow-list enforcement

Made `ModelRole.allowed_tools` load-bearing at the model tool-call boundary.

What changed:
- Added a model-side tool dispatcher that rejects tool calls not listed by the role,
  routes permitted calls through `ToolRegistry`, and records tool lifecycle events.
- Switched multi-turn recall execution to use the shared dispatcher while keeping
  recall-specific session records and traces in the experiment.
- Documented `ModelRole.allowed_tools` as the authoritative role-level allow-list.

Refs: crates/qsf_app/src/models,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs

## 2026-05-18 - Tool-call id propagation in model messages

Extended provider-agnostic model messages so tool results can carry the originating
provider call id through prompt assembly and the multi-turn follow-up path.

What changed:
- Added optional `tool_call_id` to `ModelMessage` with serde defaults that keep normal
  system, user, assistant, and tool messages unchanged when the id is absent.
- Added a dedicated `ModelMessage::tool_result` constructor for tool messages that need
  to preserve the originating call id.
- Updated prompt hashing and size accounting to include the tool-call id when present.
- Switched the multi-turn recall follow-up to append tool messages with the preserved
  call id.
- Added unit tests for default tool messages, id-preserving tool results, serde shape,
  and prompt hash differentiation by call id.

Refs: crates/qsf_app/src/models/model_client.rs,
crates/qsf_app/src/conversation/prompt.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs

## 2026-05-18 - OpenAI tool-capable request serialization

Added a direct Chat Completions request/response path for OpenAI-backed model calls
that need tool serialization.

What changed:
- Added a feature-gated `OpenAiToolClient` that serializes QSF messages, tool
  definitions, `max_completion_tokens`, and JSON response mode directly to OpenAI
  Chat Completions requests.
- Routed OpenAI model calls with either declared tools or tool-result messages
  through the direct serializer so follow-up tool messages preserve provider-native
  `tool_call_id` values.
- Parsed OpenAI tool-call responses into `ModelToolCall` values, preserving call id,
  function name, finish reason, usage, and cached prompt tokens.
- Rejected malformed tool arguments and missing call ids as provider-response errors
  before tool dispatch.
- Added unit tests covering request serialization, response parsing, tool-result
  messages, and the tool-capable routing decision.

Refs: crates/qsf_app/Cargo.toml,
crates/qsf_app/src/models/openai_provider.rs,
crates/qsf_app/src/models/openai_tool_client.rs

## 2026-05-18 - Multi-turn OpenAI recall path wiring

Confirmed the recall-turn loop works through the OpenAI-backed model client with
tool definitions on the first conversational request and provider-native tool
messages on the follow-up request.

What changed:
- Added a capturing OpenAI-style model client test for the multi-turn loop that
  records every request, returns a tool call on the recall prompt, and verifies the
  follow-up request contains the original `tool_call_id`.
- Verified the first recall request advertises `recall_turn` from the shared tool
  registry, while the follow-up request carries a tool message and no advertised
  tools.
- Covered the sequencing contract that the recall turn produces a second
  `PromptAssembled` event after the tool result is appended.
- Kept the existing guard that rejects any follow-up response that still returns
  tool calls.

Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs

## 2026-05-18 - OpenAI tool-call response parsing

Tightened the direct OpenAI Chat Completions path so tool-call responses are parsed
into QSF model responses without losing provider ids or token accounting.

What changed:
- Parsed OpenAI `tool_calls` entries into `ModelToolCall` values while preserving the
  provider call id, tool name, and JSON arguments.
- Kept normal text responses on the same `ModelResponse::from_text` path, including
  finish reason and usage metadata.
- Added strict failures for missing tool-call ids, malformed JSON arguments, and
  unsupported non-function tool-call types.
- Covered multiple tool calls, text content, content-part text, missing ids,
  malformed arguments, and cached-token parsing in unit tests.

Refs: crates/qsf_app/src/models/openai_tool_client.rs,
crates/qsf_app/src/models/openai_provider.rs

## 2026-05-18 - OpenAI recall follow-up transcript preservation

Fixed the OpenAI recall follow-up transcript so provider-native tool results are
preceded by the assistant message that originally requested the tool call.

What changed:
- Added assistant `tool_calls` preservation to `ModelMessage` and OpenAI request
  serialization.
- Rebuilt recalled prompt history with assistant tool-call messages immediately
  before their matching tool-result messages.
- Updated the multi-turn recall follow-up path to send the assistant tool-call
  message before dispatch results.
- Covered the transcript ordering and serialized OpenAI tool-call payloads in tests.

Observed:
- Live OpenAI recall run `runs/2026-05-18-174421-multi-turn-text-loop`
  completed with one `recall_turn` execution and a final verbatim `[Turn 0]`
  response.

Refs: crates/qsf_app/src/models/model_client.rs,
crates/qsf_app/src/conversation/prompt.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/models/openai_tool_client.rs,
crates/qsf_app/src/models/openai_provider.rs

## 2026-05-18 - OpenAI-feature clippy cleanup

Cleaned up a realtime voice session match arm that only triggered Clippy when the
OpenAI feature set was checked.

What changed:
- Collapsed the response transcript completion branch into a match guard while
  preserving the existing fallback from `transcript` to `text`.

Refs: crates/qsf_app/src/audio/voice_session_provider.rs

## 2026-05-18 - Model boundary review fixes

Addressed the highest-risk model-module review findings around OpenAI tool-message
validity, tool-dispatch permissions, and JSON-mode response diagnostics.

What changed:
- Removed the public invalid `ModelMessage::tool` constructor; tool-role messages are
  constructed with `tool_result` so a provider call id is present.
- Built model tool-dispatch permissions from registry metadata instead of a permissive
  fallback, while preserving failure events for unknown or malformed tool calls.
- Recorded JSON parse errors on `ModelResponse` when a JSON-mode response is malformed.
- Added guard tests for role-id string serialization and advertised-tool drift, plus a
  documented decision that model tool dispatch fails fast.

Refs: crates/qsf_app/src/models/model_client.rs,
crates/qsf_app/src/models/tool_dispatch.rs,
crates/qsf_app/src/models/model_role.rs,
crates/qsf_app/src/models/openai_tool_client.rs,
crates/qsf_app/src/conversation/prompt.rs,
docs/DecisionLog.md

## 2026-05-18 - Conversational calculator tool access

Enabled the multi-turn conversational responder to call the existing calculator
tool through the model tool-dispatch boundary.

What changed:
- Added `calculator` to the conversational responder's model-callable tool
  allow-list beside `recall_turn`.
- Generalized the multi-turn tool follow-up path so non-recall tool results are
  returned to the model as ordered tool messages.
- Updated the session prompt and mock model behavior so arithmetic requests can
  select the calculator path during deterministic tests.
- Refreshed the tool-system architecture status for calculator availability in
  the multi-turn loop.

Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/conversation/prompt.rs,
crates/qsf_app/src/models/mock_model.rs,
docs/Architecture/Architecture.ToolSystem.md

## 2026-05-20 - OpenAI provider path compiled unconditionally

Removed the `qsf_app/openai` Cargo feature so the live OpenAI model, realtime
transcription, and realtime voice session adapters are compiled by default while
runtime provider selection still defaults to mocks unless explicitly configured.

What changed:
- Made the former OpenAI-feature dependencies unconditional in `qsf_app`.
- Removed `#[cfg(feature = "openai")]` gates and the fallback stubs that reported
  the feature as missing.
- Updated setup and generated voice-memory instructions to drop `--features openai`.

Refs: crates/qsf_app/Cargo.toml, crates/qsf_app/src/models,
crates/qsf_app/src/audio, README.md, docs/DecisionLog.md; implements:
2026-05-20 - openai Cargo feature removed

## 2026-05-20 - Cross-session memory store foundation

Added the persistence and retrieval primitives needed before sleep can create a durable
cross-session memory store.

What changed:
- Added optional `MemoryRecord.last_reinforced_at` with backwards-compatible v1 JSON deserialization.
- Changed memory retrieval recency scoring from rank order to time decay against `last_reinforced_at`, falling back to `created_at`.
- Added `MemoryStore` load, append, schema validation, and atomic pretty-JSON persistence.

Observed:
- Existing default retrieval behavior still prefers newer fixture records when no reinforcement timestamp exists.

Refs: crates/qsf_app/src/memory/memory_record.rs, crates/qsf_app/src/memory/retrieval.rs,
crates/qsf_app/src/memory/store.rs

## 2026-05-20 - Multi-turn text loop awake continuation

Added the continuity manifest and persisted `SessionState` path for the multi-turn text loop so a later run can resume an unfinished awake session.

What changed:
- Added stable session identifiers, atomic session-state persistence, an atomic continuity manifest, a pure resume classifier, and `prepare_awake_continuation` reset semantics.
- Wired `multi-turn-text-loop` boot through `ColdStart`, `AwakeContinuation`, and the Stage 4 placeholder `ConsolidatedBrief` mode, emitting `SessionResumed` before `SessionStarted`.
- Persisted the session state and manifest at loop end under the configured state directory, with `state/` ignored locally.
- Downgraded awake-continuation attempts to `ColdStart` when the stored session config differs from the new run, and pinned the consolidated-brief placeholder with a loop-level test.

Refs: crates/qsf_app/src/session, crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/observability/event_log.rs, docs/Architecture/Architecture.RuntimeLoop.md,
docs/Architecture/Architecture.StateAndObservability.md, docs/DecisionLog.md, .gitignore;
implements: 2026-05-20 - Text-loop continuity uses a manifest-backed state directory

## 2026-05-20 - Sleep continuity commit and consolidated brief resume

Sleep can now consume a persisted session, auto-promote routine memory candidates into the cross-session store, write a consolidated brief through a manifest-last commit, and leave decision candidates in the reviewed-memory draft workflow. The multi-turn text loop loads the consolidated brief on the next boot and prepends it to the first turn's memory context.

What changed:
- Added pure sleep promotion and commit helpers with idempotency coverage.
- Wired the sleep experiment to update `memory-store.json`, `consolidated-brief.json`, the brief archive, and the continuity manifest when a pending session exists.
- Added decision-kind reviewed-memory draft output for sleep decision candidates.
- Added first-turn consolidated brief injection for `ConsolidatedBrief` resumes.

Observed:
- The sleep experiment still behaves as a legacy artifact-only run when no persisted session is present.
- Focused tests cover promotion, commit idempotency, decision draft output, sleep wiring, awake continuation, and consolidated-brief resume injection.
- Sleep memory ids intentionally normalize the sleep run id segment to lowercase, and cross-turn association reasons use "co-retrieved within N turns" wording to match the implemented retrieval signal.
- Continuity-state paths in `extra_artifacts` are report breadcrumbs outside the run directory; the report writer renders them as text links and does not dereference them.

Refs: crates/qsf_app/src/sleep/{auto_promote,commit}.rs,
crates/qsf_app/src/experiments/{sleep_phase_session_summary,multi_turn_text_loop}.rs,
crates/qsf_app/src/memory/reviewed_memory_draft.rs,
crates/qsf_app/src/context/context_assembler.rs

## 2026-05-20 - Live co-retrieval reinforcement

The multi-turn text loop now treats an existing cross-session memory store as the
retrieval source of truth and writes live retrieval reinforcement back to that store
once per turn.

What changed:
- Added a pure `memory::co_retrieval` delta generator with deterministic pair ordering,
  capped new association creation, and direction-independent strengthening.
- Wired the text loop to create or strengthen co-retrieval associations, bump retrieved
  memory reinforcement counts, and set `last_reinforced_at` from the live clock when
  `state/text-loop/memory-store.json` exists.
- Added `CoRetrievalAssociationsProposed`, `MemoryReinforced`, and
  `MemoryStorePersisted` event variants, plus a cold-start no-write event when no
  persistent store exists.
- Added regression coverage for pure delta behavior, persisted text-loop association
  creation and strengthening, store-source resolution, and emitted observability events.

Observed:
- Cold-start runs still retrieve from the configured fixture or file source but skip
  live memory writes until sleep creates the persistent memory store.

Refs: crates/qsf_app/src/memory/co_retrieval.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/observability/event_log.rs

## 2026-05-20 - Sleep summarizer OpenAI JSON budget

The real OpenAI sleep summarizer path could hit the 512-token output cap and leave
the JSON response truncated, which made the cross-session golden path fail before
sleep committed the memory store.

What changed:
- Raised the sleep summarizer output budget for JSON responses.
- Tightened the sleep prompt to request compact JSON only, numeric importance values,
  bounded list sizes, and explicit 1-based association indexes.
- Normalized real-provider zero-based `association_candidates` indexes to the
  internal 1-based `SleepReport` contract when the provider clearly uses zero-based
  indexing.
- Expanded the missing-JSON error to include provider finish reason and parse error,
  and covered the request budget/schema guidance plus zero-based association parsing
  in focused unit tests.

Observed:
- The failed run `runs/2026-05-20-121912-sleep-phase-session-summary` ended with
  `finish_reason = max_tokens` and an EOF JSON parse error after 512 output tokens.
- The follow-up run `runs/2026-05-20-122850-sleep-phase-session-summary` returned
  valid JSON but used zero-based association indexes.

Refs: crates/qsf_app/src/sleep/session_summary.rs,
crates/qsf_app/src/sleep/sleep_report.rs,
runs/2026-05-20-121912-sleep-phase-session-summary,
runs/2026-05-20-122850-sleep-phase-session-summary

## 2026-05-20 - Cross-session continuity golden path

The OpenAI-backed continuity path completed end to end: sleep created a persistent
memory store, the live text loop loaded that store, and repeated retrieval reinforced
the sleep memories while creating and strengthening co-retrieval associations.

Observed:
- Sleep run `runs/2026-05-20-124258-sleep-phase-session-summary` succeeded after the
  real-provider JSON budget and association-index fixes.
- Live-loop run `runs/2026-05-20-124436-multi-turn-text-loop` loaded `memory_store`,
  created five live co-retrieval associations on turn 0, strengthened five existing
  associations on turn 1, and persisted `reinforcement_count = 2` for the four sleep
  memories.
- The first OpenAI responder turn ended with `finish_reason = length`, which did not
  block reinforcement but remains a conversational output-budget polish item.

Refs: runs/2026-05-20-124258-sleep-phase-session-summary,
runs/2026-05-20-124436-multi-turn-text-loop,
crates/qsf_app/src/experiments/sleep_phase_session_summary.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/memory/co_retrieval.rs, docs/DecisionLog.md;
implements: 2026-05-20 - Sleep auto-promotes routine memory candidates

## 2026-05-21 - Extract qsf_memory shared crate

Memory record, association, and store-loading types moved from `qsf_app` into a
new `qsf_memory` crate. The new crate adds a `load_existing` helper for existing
store reads, a two-pass loader that retains source-faithful per-record JSON,
structured load errors, and dangling association detection.

What changed:
- Added `crates/qsf_memory` with record, association, store, and error modules.
- Kept `qsf_app::memory::*` import paths working through compatibility re-exports.
- Added loader coverage for missing files, invalid JSON, unsupported schemas,
  invalid shapes, duplicate IDs, raw-record preservation, and dangling references.

Refs: crates/qsf_memory, crates/qsf_app/src/memory;
implements: 2026-05-20 - Post-hoc browser tools use Rust backend + browser frontend split

## 2026-05-21 - Project statistics script added

Adapted the project statistics PowerShell script to qualia-signal-foundry, replacing hardcoded project references, handling wildcard workspace crates, and partitioning documentation.

What changed:
- Created `scripts/project-stats.ps1` to calculate lines of code and tests across workspace crates.
- Configured documentation metrics to partition planning (Plan, Design, Idea prefixes) from other files.
- Formatted output numbers using Invariant Culture to ensure clean comma separators.

Refs: scripts/project-stats.ps1

## 2026-05-21 - qsf_browser_server skeleton with /api/health

New crate `qsf_browser_server` hosts the HTTP server for post-hoc memory
inspection. Phase 1 implements CLI args, `AppState` over `qsf_memory::load_existing`,
the `/api/health` route, stubbed `503` responses on the other `/api/*` routes,
loopback-by-default binding, and a non-loopback disclosure warning logged via
`engine_logging`.

What changed:
- Added the axum server crate with read-only dependencies on `qsf_memory` and
  `engine_logging`.
- Added a browser-facing load-error DTO separate from persisted memory types.
- Added integration coverage for missing-store health and data-route responses.

Refs: crates/qsf_browser_server;
implements: 2026-05-20 - Post-hoc browser tools use Rust backend + browser frontend split

## 2026-05-21 - Memory browser data endpoints

The browser server gained read-only memory DTOs and API endpoints for store summaries, paged memory lists, memory details, raw persisted records, and selected-memory neighborhoods.

What changed:
- Added pure mapping, filtering, sorting, pagination, and orphan/broken-edge handling for loaded memory stores.
- Replaced placeholder data routes with real `/api/store/summary` and `/api/memories/*` handlers.
- Added integration coverage with a small store fixture that preserves raw extra fields and includes a broken association.
- Applied review follow-up by avoiding per-record kind string allocation, reusing the built memory id index for detail and neighborhood lookups, and adding regression coverage for multi-tag OR filtering, keyword haystacks, high-side pagination clamp, and neighborhood limit ordering.
- Left a code note for caching the immutable store index if request volume makes rebuilds hot.

Observed:
- Broken associations remain loadable and are surfaced through summaries, details, and neighborhoods without inventing placeholder member records.

Refs: crates/qsf_browser_server/src/memory, crates/qsf_browser_server/tests/data_endpoints.rs, docs/Reviews/Review.Phase2.MemoryAssociationBrowser.md

## 2026-05-21 - Memory browser frontend shell

The browser server gained a Vite/TypeScript workbench shell for inspecting the
read-only memory API.

What changed:
- Added the frontend scaffold, visual tokens, layout shell, API DTO mirrors, URL
  state reducer, toolbar, filters, memory list, inspector, raw JSON overlay, and
  load-error screen.
- Kept the association canvas as a placeholder while wiring the rest of the page
  through `/api/health`, `/api/store/summary`, `/api/memories`, and memory
  detail/raw endpoints.
- Applied review follow-up for async request sequencing, raw JSON overlay dismissal
  and error handling, empty tag normalization, load-error escaping, and a first
  Vitest URL-state round-trip test.

Observed:
- The backend still does not expose the active store path, so the toolbar shows
  the planned `(store)` placeholder.
- `npm install` reported two moderate advisories in the Vite 5 dependency tree;
  no forced upgrade was applied because that would change the planned dependency
  line.
- Adding Vitest changed `npm audit` output to four moderate advisories; no forced
  upgrade was applied.

Open question:
- External testing should confirm URL state survives refreshes and that the
  load-error screen is clear against a deliberately bad store path.
- The `delta-since` filter remains URL/filter-field only, and selected-id deep links
  still load the id directly rather than jumping the list page to that record.

Refs: crates/qsf_browser_server/ui, .gitignore
## 2026-05-21 - Memory association browser reference fixture

Curated the tracked memory-association browser reference bundle so it can act as a
self-contained QA graph rather than only a generated continuity smoke fixture.

What changed:
- Added the phase-four seed memory records alongside the generated sleep-promoted
  memory so stored associations have resolvable endpoints.
- Varied reference association weights and preserved reinforcement metadata to cover
  browser sorting, filtering, and edge rendering cases.
- Added a fixture README explaining why this continuity bundle lives under tracked
  experiment fixtures instead of ignored `state/` or `runs/` folders.

Observed:
- The fixture manifest intentionally remains in awake-continuation state after the
  follow-up text-loop run, with sleep pending for the current session.

Refs: docs/Experiments/Fixtures/memory-association-browser-reference

## 2026-05-21 - Project stats include browser UI source

The project statistics report now accounts for authored browser UI source and JSON config/test data without counting generated frontend output or vendored dependencies.

What changed:
- Added frontend TypeScript, CSS, and HTML reporting to the project stats script.
- Added JSON config/data reporting that excludes lockfiles from line totals.
- Excluded `node_modules`, `dist`, `target`, `runs`, and `state` paths from authored file counts.

Refs: scripts/project-stats.ps1

## 2026-05-22 - PowerShell launcher baseline

Added the first PowerShell launcher for local development commands while keeping the
underlying Cargo and npm commands visible.

What changed:
- Added `scripts/qsf.ps1` with `help`, `app`, `browser`, `ui`, `workbench`, and
  `list experiments` commands.
- Set the launcher runtime to PowerShell 7.6 and made the workbench UI child
  process start through `pwsh`.
- Documented launcher usage in the README alongside the raw Cargo commands.
- Captured the Phase 0 command inventory used by this baseline: `qsf_app`
  experiment IDs, runtime `QSF_*` groups for model, transcript, realtime voice,
  speech output, memory, session, state, and review flow, browser-server
  `--store` / `--host` / `--port`, and UI `dev` / `build` / `test` / `preview`
  scripts.

Observed:
- The default browser store is absent in this checkout, so the launcher correctly
  points users at the tracked sample store before starting Cargo.
- The workbench command starts the UI by re-entering the launcher in a separate
  visible PowerShell process, then runs the API in the current terminal.

Refs: scripts/qsf.ps1, README.md, crates/qsf_browser_server/src/cli.rs,
crates/qsf_browser_server/ui/package.json

## 2026-05-22 - PowerShell launcher profiles

Added process-scoped launch profiles so common provider and memory-source environment
bundles are visible without permanently changing the caller's shell.

What changed:
- Added checked-in launcher profiles for mock providers, OpenAI text, file-backed
  voice memory, and OpenAI microphone transcription.
- Extended the launcher with `-Profile`, `-VoiceMemoryFile`, `list profiles`,
  profile prerequisite checks, child-process environment set/clear handling, and
  secret-like value redaction in printed output.
- Documented profile usage in the README.

Observed:
- Missing profile prerequisites fail before Cargo starts, and unknown profile names
  list the valid checked-in profiles.

Refs: scripts/qsf.ps1, scripts/qsf.profiles.json, README.md

## 2026-05-22 - PowerShell launcher argument completion

Added opt-in PowerShell tab completion for the launcher command surface so common
development launches are easier to discover from an interactive shell.

What changed:
- Added `scripts/qsf-completion.ps1` with native argument completion for launcher
  commands, `list` targets, checked-in profile names, static experiment IDs, likely
  browser store JSON files, and bind host values.
- Documented dot-sourcing the completion script in the README.

Observed:
- Programmatic completion checks return the checked-in profile names, static
  experiment IDs, command names, and JSON store path candidates without shelling out
  to Cargo.

Refs: scripts/qsf-completion.ps1, README.md

## 2026-05-22 - PowerShell launcher completion tests

Added Pester coverage for the launcher completion script so the interactive command
surface can be checked without manual tab-completion testing.

What changed:
- Added `scripts/qsf-completion.Tests.ps1` with programmatic completion checks for
  launcher commands, `list` targets, profiles, experiment names, store paths, and bind
  host values.

Observed:
- `Invoke-Pester -Path .\scripts\qsf-completion.Tests.ps1 -CI` passes with seven
  tests.

Refs: scripts/qsf-completion.Tests.ps1, scripts/qsf-completion.ps1

## 2026-05-22 - PowerShell launcher doctor

Added a non-launching diagnostics command for the local PowerShell launcher so setup
issues can be identified before starting Cargo or Vite.

What changed:
- Added `doctor`, `doctor -Profile <name>`, and `doctor -Workbench` checks for
  PowerShell, Cargo, Rust, repository root detection, Node/npm, UI dependencies, the
  default memory store, port `3939`, and `OPENAI_API_KEY` presence with secret values
  hidden.
- Updated launcher help, argument completion, completion tests, and README usage for
  the new diagnostics command.

Observed:
- General doctor output treats optional UI/OpenAI prerequisites as warnings, while
  workbench-specific missing prerequisites become failures.

Refs: scripts/qsf.ps1, scripts/qsf-completion.ps1, scripts/qsf-completion.Tests.ps1,
README.md

## 2026-05-22 - PowerShell launcher documentation consolidation

The launcher is now documented as the Windows happy path for common local development
launches while preserving the raw Cargo and npm commands as fallback references.

What changed:
- Expanded README launcher guidance with workbench usage, Memory Association Browser
  launch commands, raw command equivalents, and troubleshooting for blocked ports,
  missing API keys, missing UI dependencies, execution policy restrictions, and
  completion refresh.
- Updated the Memory Association Browser plan snippets that start the API and Vite UI
  together to point at `.\scripts\qsf.ps1 workbench` first.
- Recorded the durable launcher convention in the decision log and marked the
  documentation-polish tasks complete.

Refs: README.md, docs/Plans/Plan.MemoryAssociationBrowser.md,
docs/Plans/Plan.PowerShellLauncher.md, docs/DecisionLog.md

## 2026-05-22 - Launch script review follow-up

The PowerShell launcher and supporting scripts were tightened after launch-script
review without changing the underlying Cargo or npm command surface.

What changed:
- Renamed the primary profile-selection parameter to `-LaunchProfile` while keeping
  `-Profile` as a compatibility alias, avoiding a clash with PowerShell's automatic
  `$Profile` variable.
- Required checked-in profile definitions to include explicit `clear_env` and
  `requires` arrays, and documented the file-memory profile's coupling to
  `-VoiceMemoryFile` in the launcher.
- Made `workbench` print the spawned UI PID and try to close that process when the API
  foreground process exits.
- Bounded and briefly cached JSON store-path completion, kept profile completion for
  the alias, renamed completion-test variables away from `$matches`, and updated
  project-stats helper verbs to approved PowerShell names.

Refs: scripts/qsf.ps1, scripts/qsf-completion.ps1,
scripts/qsf-completion.Tests.ps1, scripts/project-stats.ps1, README.md,
docs/Plans/Plan.PowerShellLauncher.md

## 2026-05-22 - Positional browser store launcher argument

The PowerShell launcher now accepts the memory-store path as a positional argument
for browser launches, matching the way the command is commonly typed.

What changed:
- `browser <store>` and `workbench <store>` now pass the positional store path through
  to `qsf_browser_server` instead of leaving the default store in place.
- The launcher rejects ambiguous use of both positional store and `-Store`.
- Completion and README examples now cover positional store paths.

Refs: scripts/qsf.ps1, scripts/qsf-completion.ps1,
scripts/qsf-completion.Tests.ps1, README.md

## 2026-05-22 - Browser API root guidance

The memory browser backend now responds at `/` with a small guidance page instead
of a bare 404 when running the split Rust API plus Vite UI development setup.

What changed:
- Added a root route that points to `http://localhost:5173/` and `/api/health`.
- Updated the launcher UI line and README so the Vite workbench URL is explicit.

Refs: crates/qsf_browser_server/src/web.rs, scripts/qsf.ps1, README.md

## 2026-05-22 - Memory browser focal-hub canvas

The browser workbench now renders the selected memory's local neighborhood through
a PixiJS canvas instead of the Phase 3 placeholder.

What changed:
- Added PixiJS to the UI dependencies and introduced a pure radial layout helper
  with focused Vitest coverage.
- Added a focal-hub scene that draws weighted association edges, dashed broken
  edges, neighbor labels, hover tooltips, and click-to-select navigation through
  the existing reducer action flow.
- Wired the canvas into the existing async reload sequencing and added explicit
  Pixi scene cleanup when the selection is cleared or neighborhood loading fails.
- Applied review follow-up for defensive cleanup after Pixi init failures, retained
  the scene across transient neighborhood-fetch failures, fixed the broken-node
  cursor, enabled HiDPI canvas density, and extracted tested edge-mapping helpers.

Observed:
- `npm run build` passes with Vite's expected large-chunk warning after adding PixiJS.
- The fixture API smoke path returns a neighborhood containing both a normal edge
  and a broken `ghost` edge for canvas verification.
- The project owner confirmed the fixture canvas visually renders the focal hub,
  normal edge, dashed broken edge, and readable `Alpha`, `Beta`, and `ghost`
  labels. Real-store legibility remains useful to check during regular use.
- Review follow-up expanded the UI test suite to cover single-neighbor, zero-radius,
  production-limit radial layouts, neighbor deduplication, and edge-width scaling.

Refs: crates/qsf_browser_server/ui

## 2026-05-22 - Memory browser recent activity sort

The memory browser now makes live-session reinforcement visible in the default list
view instead of showing only creation dates for reused records.

What changed:
- Added a `recent_activity` memory-list sort based on `last_reinforced_at` with
  `created_at` fallback.
- Made the workbench default to recent activity and added row metadata that says
  `reinforced YYYY-MM-DD` when a memory was touched after creation.
- Added backend and UI regression coverage for the new sort/default behavior.

Observed:
- The `2026-05-22-160302-multi-turn-text-loop` run reinforced existing memories in
  `state\qa-memory-browser-real\memory-store.json`; it did not create accepted new
  records for the May 22 conversation text.

Refs: crates/qsf_browser_server/src/memory/filters.rs,
crates/qsf_browser_server/tests/data_endpoints.rs,
crates/qsf_browser_server/ui

## 2026-05-22 - Session context search in memory browser

The memory browser can now surface text that exists in the adjacent
`session-state.json` but has not been promoted into accepted cross-session memory.

What changed:
- Added a read-only `/api/session/search` endpoint that searches session turns,
  summarized turns, and recalled-turn text from `session-state.json` next to the
  selected memory store.
- The workbench search now shows separate "Session context matches" beneath accepted
  memory results, keeping transient session context visually distinct from durable
  memory records.
- Added regression coverage for finding an `Ari` turn summary in session state.

Observed:
- The `Ari` text from `state\qa-memory-browser-real\session-state.json` is session
  continuity state, not a record in `memory-store.json`; it will not appear as an
  accepted memory until a sleep/promotion path creates a store record.

Refs: crates/qsf_browser_server/src/session_context.rs,
crates/qsf_browser_server/ui, crates/qsf_browser_server/tests/data_endpoints.rs

## 2026-05-22 - Sleep uses persisted session turns

The sleep session summary experiment now builds its sleep input from the persisted
previous session state when one exists, so consolidation is grounded in actual prior
turns instead of the built-in sample transcript.

What changed:
- Added a sleep input builder that includes completed turns, prior turn summaries,
  retrieved memory blocks, and recall metadata from `session-state.json`.
- Kept the old inline transcript only as the no-session smoke-test fallback.
- Added regression coverage for the input builder and for the sleep report recording
  the persisted session as its source.
- Report artifact links now reflect the configured state directory instead of always
  naming `state/text-loop`.
- Sleep now auto-promotes valid association candidates between memory candidates that
  were promoted in the same sleep commit, without a human review step.

Refs: crates/qsf_app/src/experiments/sleep_phase_session_summary.rs,
crates/qsf_app/src/sleep/auto_promote.rs

## 2026-05-23 - Sleep skips dangling co-retrieval associations

Sleep no longer writes cross-turn co-retrieval associations whose endpoint memory IDs
are absent from the destination memory store, and the QA memory browser state was
repaired to remove the dangling links from the latest sleep run.

What changed:
- Filtered sleep co-retrieval association promotion to IDs present in
  `memory-store.json`.
- Added regression coverage for skipping retrieved IDs that are not in the current
  store.
- Removed 25 dangling associations from `state/qa-memory-browser-real/memory-store.json`
  and corrected the current consolidated brief association count to `1`.

Refs: crates/qsf_app/src/sleep/auto_promote.rs,
state/qa-memory-browser-real/memory-store.json

## 2026-05-24 - Live memory hint expansion

Live turns now separate directly retrieved memories from associated hint memories so the
conversation prompt can show one-hop graph context without making association scoring the
direct retrieval strategy.

What changed:
- Added `MemoryHint` context fragments, source-priority assembly, and a pure
  single-hop neighbor expansion helper.
- Switched live text and text-owned voice retrieval to keyword/tag scoring while keeping
  association-weighted retrieval available in the memory/context experiment.
- Rendered direct memories and associated hints as separate prompt sections.
- Reloaded the live memory snapshot after persistence so newly written associations can
  affect later turns in the same process.

Observed:
- The hint-expansion implementation intentionally keeps the best candidate per neighbor
  by weight/order, rather than following the first-edge-wins sketch from the planning
  note; this lets reciprocal edges pick the stronger reason and weight.
- Prompt labels and hint selection reasons use ASCII hyphens instead of Unicode dashes
  for stable Windows terminal rendering.
- Voice-loop association traversal is no longer part of live retrieval by design;
  association-weighted retrieval remains covered by the memory/context experiment and
  retrieval unit tests.

Refs: crates/qsf_app/src/context, crates/qsf_app/src/memory/hint_expansion.rs,
crates/qsf_app/src/conversation/prompt.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/experiments/text_owned_voice_loop.rs

## 2026-05-24 - Live console memory styling

The interactive text loop now shows retrieved memory context in the terminal before
the model response, with ANSI styling gated by TTY detection and `NO_COLOR`.

What changed:
- Added `qsf_app::console::styling` with color-mode detection, the runner-level
  `--no-color` switch, and reusable paint helpers for direct-memory, hint-memory,
  and drop-marker text.
- Printed direct memories and associated hint memories from the selected context
  assembly before each successful model response.
- Added drop and session-end flush marker writers behind `QSF_DROP_MARKER_DEBUG`
  until live drop counts are wired into the loop.

Observed:
- Unit coverage verifies plain non-color output, forced ANSI output, and marker text
  formatting. Real terminal light/dark theme legibility still needs human testing.
- The console hint header follows the prompt formatter's ASCII hyphen wording for
  stable Windows terminal rendering, even though the early design sketch used an
  em dash.

Refs: crates/qsf_app/src/console, crates/qsf_app/src/experiments/multi_turn_text_loop.rs

## 2026-05-24 - Shared cross-turn co-retrieval

Sleep's cross-turn co-retrieval pass now delegates to a pure shared memory helper, with
a reusable entry point ready for live-loop drop handling.

What changed:
- Added `generate_cross_turn_deltas` and the sleep cross-turn constants to
  `memory::co_retrieval`.
- Kept sleep promotion on a thin adapter in `sleep::auto_promote`.
- Added unit coverage for missing endpoints, existing-edge strengthening, reverse
  existing-edge direction, and window boundaries in the shared helper.

Observed:
- Cross-turn strengthening now treats existing associations as undirected so reverse
  stored edges are strengthened instead of duplicated; strengthen deltas keep the
  stored association direction so the sleep commit path can apply them.

Refs: crates/qsf_app/src/memory/co_retrieval.rs,
crates/qsf_app/src/sleep/auto_promote.rs

## 2026-05-24 - Live cross-turn aging coverage

The multi-turn text loop now runs cross-turn co-retrieval when turns leave hot
context and when a clean session exit flushes remaining hot turns.

What changed:
- Added `processed_ranges` to persisted memory stores as the idempotency ledger for
  live batch, session-end, and sleep safety-net cross-turn coverage.
- Added token-budget drop planning, live cross-turn persistence, `TurnsAgedAndCoRetrieved`
  reducer handling, and real drop/session-end console markers.
- Made existing count-threshold warm aging persist cross-turn coverage before
  summarizing turns, and made the current sleep cross-turn adapter skip anchors that
  live processing already covered.
- Review follow-up made cross-turn persistence idempotent against already processed
  anchors, kept session-end flush ranges non-contiguous when coverage has gaps,
  deduped sleep safety-net pairs across uncovered segments, and added a distinct
  `TurnsAgedAndCoRetrieved` event type.

Observed:
- The live drop path uses selected `ContextAssembly` memory IDs and keeps `Turn`
  records append-only; only `summarized_turns` grows.
- Sleep still uses the pre-proposer auto-promotion shape, but now behaves as an
  idempotent safety net against `processed_ranges` until the proposer interface lands.
- Unknown model ids now use a fallback context window with an event-log warning
  instead of silently disabling token-budget aging.

Refs: crates/qsf_memory/src/processed_range.rs,
crates/qsf_app/src/memory/processed_ranges.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
crates/qsf_app/src/sleep/auto_promote.rs

## 2026-05-24 - Launcher defaults to empty text-loop memory

Adjusted the PowerShell launcher so manual `multi-turn-text-loop` runs no longer load
the deterministic demo memory fixture by surprise.

What changed:
- Added an empty session-memory fixture and made `scripts/qsf.ps1 app -Experiment
  multi-turn-text-loop` pass it as a file-backed source unless the caller selects
  another session memory mode.
- Added `-DemoMemory`, `-SessionMemorySource`, `-SessionMemoryFile`, and a
  `demo-memory` launch profile for explicit fixture/demo runs.
- Updated launcher completion, README guidance, and the decision log for the new
  launcher default.

Refs: scripts/qsf.ps1, scripts/qsf.profiles.json, scripts/qsf-completion.ps1,
scripts/qsf-completion.Tests.ps1, docs/Experiments/Fixtures/session-memory.empty.json,
README.md, docs/DecisionLog.md

## 2026-05-24 - Multi-turn console role colors

Improved the live `multi-turn-text-loop` console presentation so typed user input and
assistant responses are visually distinct in ANSI-capable terminals.

What changed:
- Added reusable console styling helpers for starting and resetting an active style.
- Bracketed terminal input echo with the user-input color before each `read_line`, then
  reset before subsequent loop output.
- Colored completed assistant responses separately from memory and drop-marker output.

Observed:
- Unit coverage verifies forced-color and no-color rendering for the new role styling
  helpers. Real terminal contrast still benefits from a manual check in the active
  PowerShell profile.

Refs: crates/qsf_app/src/console/styling.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs

## 2026-05-24 - Multi-turn response token cap raised

Raised the live `multi-turn-text-loop` responder output budget so conversational
answers are less likely to stop mid-sentence or mid-list, with regression coverage
kept tied to named expectations instead of repeated numeric literals.

What changed:
- Replaced the hard-coded 240-token responder cap with a 1024-token default.
- Added `QSF_SESSION_TURN_MAX_OUTPUT_TOKENS` as a runtime override for both initial
  responder calls and post-tool follow-up responder calls.
- Added unit coverage for cap parsing and for the request cap sent by `run_one_turn`.
- Reworked the parser test to compare invalid values against
  `DEFAULT_TURN_MAX_OUTPUT_TOKENS` and derive the custom override from that constant.
- Kept the regression intent explicit with a named legacy truncating cap.

Observed:
- The prior 240-token cap matched the observed truncation shape: longer structured
  responses could hit the model limit before reaching a natural stop.

Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs

## 2026-05-24 - Launcher allows continuing text-loop sessions

Adjusted the PowerShell launcher so manual `multi-turn-text-loop` runs set the
session override expected after the configured turn limit has been reached.

What changed:
- `scripts/qsf.ps1 app -Experiment multi-turn-text-loop` now sets
  `QSF_SESSION_ALLOW_OVER_LIMIT=true` for the child process.
- Launcher help now documents that text-loop runs allow continuing past the
  configured session limit.

Refs: scripts/qsf.ps1

## 2026-05-24 - Text-loop continuity tolerates limit override changes

Fixed the continuity break observed when a later `multi-turn-text-loop` run changed
only the session limit override and added a narrow durable-memory capture for accepted
assistant names.

What changed:
- Awake continuation now treats `allow_over_limit` as a runtime-only override rather
  than a resume-breaking `SessionConfig` difference.
- The live loop now persists an accepted assistant-name assignment, such as "use the
  name Ari", into `memory-store.json` as an observation when the assistant response
  includes the assigned name.
- Added regression coverage for the config compatibility rule, live name-candidate
  extraction, and creation of a durable name memory record.
- Updated runtime-loop, memory-system, and decision-log docs to reflect the new
  continuity behavior.

Observed:
- The failing QA shape was not a missing warm summary; the second run classified the
  prior state as awake-continuable and then downgraded to cold start because
  `allow_over_limit` changed.

Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
docs/Architecture/Architecture.RuntimeLoop.md,
docs/Architecture/Architecture.MemorySystem.md, docs/DecisionLog.md

## 2026-05-25 - Retrieval relevance gate and reinforcement eligibility

Added relevance gating to live keyword/tag memory retrieval so high-importance or
recent memories with no query signal are omitted rather than selected and reinforced.

What changed:
- Retrieval results now carry `RetrievedMemory.skip_reason` for omitted candidates,
  distinguishing relevance-gated skips from retrieval-limit skips.
- Keyword/tag retrieval requires a keyword, tag, or conservative profile/identity
  signal; association-weighted retrieval also accepts association paths. Generic
  identity terms such as `name` only open identity/profile memories for
  identity-shaped queries.
- Live memory reinforcement continues to operate only on `retrieval.selected` and now
  reports relevance, over-limit, and no-store skipped ids/counts in
  `MemoryReinforced`.
- Added regression coverage for zero-signal omissions, identity-shaped profile
  retrieval, and non-reinforcement of relevance-skipped memories.

Observed:
- The relevance gate is shared by text and voice retrieval because both use the common
  memory retrieval module.

Refs: crates/qsf_app/src/memory/retrieval.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
docs/Architecture/Architecture.StateAndObservability.md,
docs/DecisionLog.md

## 2026-05-25 - Live capture module and user identity memory

Extracted live memory capture into a pure helper module and taught the text loop to
persist both assistant-name and user-name memories.

What changed:
- Added `crates/qsf_app/src/memory/live_capture.rs` with pure capture helpers and
  candidate metadata for assistant-name and user-name memories.
- Switched `multi_turn_text_loop` to capture multiple candidates per turn, build
  stable `memory.live.<session>.turn-<NNN>.<kind>` ids, and emit richer
  `MemoryStorePersisted` payloads with candidate counts and kinds.
- Added an end-to-end text-loop regression that persists Ari and Lars memories and
  checks that `what is your name` and `what is my name` retrieve the matching
  record.
- Tightened identity retrieval so assistant and user identity memories are matched
  by query direction instead of importance alone.
- Hardened user-name capture against casual `I am ...`, callback, and embedded
  `my name is ...` false positives, and added contraction coverage for `What's my
  name?` / `What's your name?`.

Observed:
- The live capture path now has a small pure module that can be unit tested without
  the session runtime.
- Assistant-name and user-name memories now have distinct tags, ids, and retrieval
  behavior.
- Untargeted `profile` records now fail closed for targeted identity queries.

Refs: crates/qsf_app/src/memory/live_capture.rs,
crates/qsf_app/src/memory/mod.rs,
crates/qsf_app/src/memory/retrieval.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs

## 2026-05-25 - Explicit remember-this live capture

Phase 3 of the live memory quality plan landed: the multi-turn text loop now captures
explicit remember-this turns as bounded remembered-topic memories sourced from the
prior assistant response and prior user topic. Review fixes were folded in before
submission so low-value or misleading captures do not enter the durable memory store.

What changed:
- Added remembered-topic capture to `memory/live_capture.rs` with explicit
  remember-request detection, topic-term tagging, and bounded source excerpts.
- Wired `multi_turn_text_loop.rs` to persist remembered-topic candidates, emit
  live-memory-capture traces, and log a skip trace when a remember request has no
  usable prior assistant response.
- Skipped remembered-topic capture when the previous assistant response is missing
  or whitespace-only, and rejected negated or self-directed remember-this phrases.
- Added an end-to-end regression that exercises assistant identity, user identity,
  remembered-topic capture, and follow-up retrievals.
- Restored user-name false-positive regression tests that had been dropped during
  the Phase 3 edit.
- Made duplicate-only live-capture traces generic unless the duplicate candidate was
  actually a remembered-topic memory.
- Normalized singular and plural goal topic tags so remembered-topic retrieval can
  match either query form.
- Updated memory, runtime, and observability architecture notes to reflect the new
  capture path and trace coverage.

Observed:
- The remembered-topic record now stores the previous turn reference plus a bounded
  excerpt instead of a fabricated semantic summary.
- Retrieval on the remembered-topic follow-up prefers the remembered memory and does
  not fall back to the Ari identity memory on unrelated volition queries.

Refs: crates/qsf_app/src/memory/live_capture.rs,
crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
docs/Architecture/Architecture.MemorySystem.md,
docs/Architecture/Architecture.RuntimeLoop.md,
docs/Architecture/Architecture.StateAndObservability.md

## 2026-05-26 - Warm summary truncation guard

Warm summaries now retry once when the summarizer hits a truncation finish reason,
and the aging path leaves the turn unsummarized if the retry truncates again.

What changed:
- Split warm-turn summarization into a retry-aware helper in
  `multi_turn_text_loop.rs` that inspects `ModelResponse.finish_reason`.
- The retry uses a larger output cap and an explicit retry prompt so deterministic
  summarizer calls are not repeated with identical parameters.
- Added fail-closed handling for truncation in the warm-threshold aging path and
  the token-budget aging path so a second truncation logs an error and does not
  commit `TurnSummarized`.
- Kept live cross-turn persistence ahead of summary generation, so association
  deltas and processed ranges are still recorded even when summary generation
  later fails closed.
- Added regression tests for retry-success and double-truncation failure cases.
- Hardened summary range handling for inverted ranges and fixed the final
  truncation error to report the actual session id.
- Updated the runtime-loop architecture note to reflect the retry-and-fail-closed
  behavior.

Observed:
- A first `max_tokens` summary response now retries cleanly and persists the retry
  result.
- A second `max_tokens` response leaves the turn hot and emits an error event with
  the session and turn metadata.

Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
docs/Architecture/Architecture.RuntimeLoop.md

## 2026-05-26 - Live memory QA fixture tightened

The Ari/Lars/volition regression now matches the documented experiment fixture more
closely by exercising the final unrelated volition turn and checking persisted
session state.

What changed:
- Extended the deterministic text-loop regression with `Tell me about volition goals.`
  after the identity and remembered-topic follow-up queries.
- Asserted that the unrelated volition turn does not select the Ari identity memory.
- Loaded `session-state.json` from the fixture run and asserted that no warm summaries
  were persisted.

Observed:
- The experiment document's claimed fixture validations now line up with the test's
  actual assertions.

Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs,
docs/Experiments/Experiment.LiveMemoryCaptureQuality.md

## 2026-05-26 - Default sleep mock stops emitting fake candidates

The default `sleep-phase-session-summary` path still runs the full sleep commit
flow, but the deterministic mock sleep summarizer no longer emits fabricated
memory candidates that can be promoted into the shared store.

What changed:
- Removed the static `Model roles now flow through the same event and trace
  artifacts...` memory candidate from the mock `SleepSummarizer` fixture.
- Restored provider-agnostic sleep promotion so default runs still execute the
  normal commit path, including continuity brief writes and cross-turn
  association persistence.
- Added regressions for both empty mock memory candidates and mock sleep
  cross-turn association persistence.

Observed:
- The prior smoke candidate `Model roles now flow through the same event and
  trace artifacts as other subsystems.` no longer appears in the committed
  memory store after a default mock sleep run.
- Mock sleep still persists valid associations that are grounded in previous
  session retrievals and existing memory-store endpoints.

Refs: crates/qsf_app/src/models/mock_model.rs,
crates/qsf_app/src/sleep/session_summary.rs,
crates/qsf_app/src/experiments/sleep_phase_session_summary.rs;
implements: Default sleep runs execute full side effects without synthetic memory

## 2026-05-27 - Sleep proposer interface and prompt rewording

Phase 5 of the associative recall plan landed: sleep now routes association
generation through pluggable proposers, and the sleep prompt asks for
non-obvious connections instead of mechanical co-retrieval language. Review
follow-up consolidated the safety-net cross-turn work so create proposals,
strengthen deltas, and processed ranges are computed together.

What changed:
- Added `sleep::proposer` with `AssociationProposer`, `ProposedAssociation`, and
  priority-aware merge/dedupe helpers.
- Added `LlmCandidateProposer` and `SafetyNetCoRetrievalProposer` under
  `sleep::proposers`, with coverage tests for both paths.
- Refactored sleep promotion to merge proposer output before creating
  associations, while keeping the cross-turn strengthening and processed-range
  bookkeeping intact.
- Moved safety-net create proposals, strengthen deltas, and `SleepSafetyNet`
  processed ranges into one `propose_with_bookkeeping` path.
- Removed duplicated cross-turn delta computation and the stale cross-turn plan
  helper from `auto_promote.rs`.
- Reworded the sleep summarizer prompt to emphasize non-obvious connections and
  added a regression test to pin the new wording.
- Updated the sleep architecture note to describe the proposer pipeline and the
  non-obvious-connection prompt wording.

Observed:
- `cargo test -p qsf_app sleep` passes after the refactor.
- Follow-up addresses the review findings about duplicate computation,
  misleading naming, split processed-range ownership, and stale architecture
  docs.

Refs: crates/qsf_app/src/sleep/proposer.rs,
crates/qsf_app/src/sleep/proposers/llm_candidate.rs,
crates/qsf_app/src/sleep/proposers/safety_net_co_retrieval.rs,
crates/qsf_app/src/sleep/auto_promote.rs,
crates/qsf_app/src/sleep/session_summary.rs,
docs/Architecture/Architecture.SleepPhase.md;
implements: Phase 5 - Proposer Interface And Sleep Prompt Rewording

## 2026-05-31 - Shared live-session state extraction and review follow-up

Extracted reusable live-session state types so the session layer can represent text and voice exchanges through one shared model while preserving existing persisted `SessionState` files, then applied the Phase 1 review follow-up to tighten compatibility and state handling.

What changed:
- Added `session/exchange.rs` with shared `Exchange`, input/output, utterance, interruption, and model-use types plus `Turn` conversion.
- Added `session/live_state.rs` with reusable live state, runtime phase, partial transcript, active response, live capture, and processed-range tracking.
- Added defaulted `SessionState.live` storage and cleared volatile live state during awake continuation.
- Added regression coverage for the new live state reducer, awake-continuation cleanup, and loading a pre-migration session-state fixture.
- Added `ExchangesAgedAndCoRetrieved` state and reducer support so the shared live slice can represent aging/co-retrieval outcomes alongside `processed_ranges`.
- Replaced the infallible `From<&Exchange> for Turn` with `TryFrom<&Exchange>` and an explicit conversion error type.
- Tightened `UserInterrupted` and `MemoryContextRecorded` handling so mismatched events no longer rewrite unrelated live state.
- Expanded the compatibility fixture to include one real `Turn` and one real `TurnSummary`, and updated the regression assertion to verify both load.

Observed:
- `cargo build`, `cargo test session --lib`, `cargo test multi_turn_text_loop --lib`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt` all passed after the extraction and follow-up.

Refs: crates/qsf_app/src/session/exchange.rs, crates/qsf_app/src/session/live_state.rs, crates/qsf_app/src/session/mod.rs, crates/qsf_app/src/session/continuation.rs, crates/qsf_app/tests/fixtures/pre_migration_session_state.json, docs/Architecture/Architecture.RuntimeLoop.md, docs/Reviews/Review.VoiceLoopUnification.Phase1.md

## 2026-05-31 - Text loop routed through shared live exchange core

The multi-turn text loop now creates and finalizes shared `Exchange` records through the live-session reducer before deriving persisted `Turn` state, with completed exchanges kept in memory only while `Turn` remains the Phase 2 durable shape.

What changed:
- Routed typed text turns through `session/live_state.rs` so exchange start, memory context, model completion, output, and completion are reduced through the shared live core.
- Derived persisted `Turn` records from finalized `Exchange` values instead of maintaining a separate hand-built text-only path.
- Kept `LiveSessionState.completed_exchanges` out of serialized session state to avoid a parallel on-disk write path before `Exchange` becomes canonical.
- Added `schema_version` to `SessionState`, defaulted legacy files to version 1, rejected newer unsupported session schemas, and logged resume-time schema upgrades before they are written back out.
- Cleared failed active exchanges after model failure so a clean exit does not persist a dangling failed exchange.
- Updated the runtime-loop architecture note to reflect that the text loop now uses the shared exchange core in production.

Observed:
- `cargo build`, `cargo test -p qsf_app session --lib`, `cargo test -p qsf_app multi_turn_text_loop --lib`, `cargo clippy --all-targets -- -D warnings`, and `cargo fmt` passed after the change.
- Prompt assembly, memory retrieval/reinforcement, live capture, cross-turn co-retrieval, persistence, and manifest commit remain text-loop-private for now; extracting those behavior helpers is deferred to the next phase with a second caller.

Refs: crates/qsf_app/src/experiments/multi_turn_text_loop.rs, crates/qsf_app/src/session/live_state.rs, crates/qsf_app/src/session/mod.rs, crates/qsf_app/src/session/persistence.rs, crates/qsf_app/src/session/resume.rs, docs/Architecture/Architecture.RuntimeLoop.md, docs/Reviews/Review.VoiceLoopUnification.Phase2.md
