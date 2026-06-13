# Experiment.LiveMemoryExtraction

## Status
Completed 2026-06-13. Validation record for phase 5 live memory extraction and presence observability.

## Goal
Extract reviewable memory candidates from the trusted realtime continuity root without mutating the live voice loop, while keeping interruption and latency signals durable for later inspection.

## Setup
- `qsf_app` experiment: `live-memory-extraction`
- Continuity root: `state/realtime/continuity/default`
- Model role: sleep summarizer through the existing `build_client` path
- Realtime diagnostics: `qsf_realtime_server` diagnostics log

## Procedure
- Build the extraction input from trusted promoted `SessionState.turns`.
- Treat matching persisted exchanges as metadata only.
- Run `summarize_session`, apply the existing warm-turn ageing path, then route the report through the existing review/commit path.
- Emit latency observations for the live-loop stages.
- Persist interruption diagnostics for interrupted trusted exchanges.

## Observations
- The canonical extraction transcript is the persisted turn history, not the exchange stream.
- Fallback smoke input keeps the phase runnable when the continuity root is absent or malformed.
- A successful extraction consumes the realtime continuity root as offline consolidation, so later realtime startup resumes from the consolidated brief even when there are no promoted memory records.
- Latency observations now cover final transcript received, memory injection, response creation, and first audio.
- Interrupted exchanges are written to diagnostics as trusted diagnostic records instead of becoming durable continuity.

## Validation
- `cargo test -p qsf_app live_memory`
- `cargo test -p qsf_realtime_server live_loop_latency`
- `cargo test -p qsf_realtime_server trusted_diagnostic`

## Follow-up
- Human review should check whether the extracted memory candidates feel appropriately grounded in the live session transcript.
- Presence review should use the new latency and interruption diagnostics as the primary evidence.
