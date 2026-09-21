# Realtime probe fixtures

This directory holds repeatable synthetic inputs and warm-start state for headless scripted
realtime conversations. `designed.phrases.json` is the default twelve-turn script; `smoke` stays
as the short check. Both use a wholly invented logistics-worker persona and must never be edited to
contain real personal data. Fixture edits are hand-frozen after selector-gate review: change the
phrase and its expected result together, then rerun the offline gate.

`seed/` contains a relative-time memory template, a compatible volition continuity snapshot, and
a continuity manifest. The materializer writes them to `<state-dir>/continuity/default/`; it does
not seed `session-state.json`, because session creation always starts that runtime state fresh.
`memory-store.json` is read during a probe but is not modified by it; only a later sleep operation
can write it. The procrastination record is associated with the invented logistics context; both
record and association timestamps are materialized relative to the injected clock. Warm seeding is
the normal probe start; `--cold-start` deliberately skips it.

| Turn | Intent | Winner → ordered losers |
| --- | --- | --- |
| 1 | job and automation opening | `track-the-ai-transition` → `learn-what-drives-this-person` |
| 2 | AI displacement concern | `track-the-ai-transition` → none |
| 3 | evidence check | `keep-theses-distinct-from-fact` → none |
| 4 | boundary around a colleague | `respect-persons-boundaries` → none |
| 5 | direct help over AI topic | `serve-the-present-person` → `track-the-ai-transition` |
| 6 | noticing a sleep/focus pattern | `grow-the-library` → none |
| 7 | recall the putting-things-off thesis | `grow-the-library` → none |
| 8 | capitalized Grok current-topic query | none; consultation expected |
| 9 | lowercased Grok control | none; consultation not expected |
| 10 | current-focus tool-loop prompt | none |
| 11 | world-picture prompt | `assemble-world-picture` → none |
| 12 | planning the next learning step | `serve-the-present-person` → `learn-what-drives-this-person` |

Turns 8 and 9 are deliberate instrumentation: they are byte-identical except for the `Grok`/`grok`
capitalization. Probe runs type text, so they do not provide evidence about the spoken
world-perception trigger or settle its STT capitalization behavior. Runtime volition mode never
changes during this script.

The fixture keeps two world-consultation concepts separate. `explicit_topic_expected` is the
offline assertion over the pure explicit-topic detector. `world_consultation_expected` is a live
assertion over authoritative `world_consultation_performed` diagnostics, including the trigger and
required anchors. Those records are matched across the run rather than assigned by capture turn:
a corpus lookup over the 5 ms inline budget is deferred and recorded on the next turn, so the turn
8 lookup may be written with turn 9's exchange index without changing the capitalized/lowercased
pair's answer. The pair requires exactly one `explicit_current_topic` record anchored by `grok`;
turn 11 separately requires the `goal_activation` record anchored by `world` and `society`.

The bundled corpus path is compiled from `CARGO_MANIFEST_DIR`. A binary built from another working
tree can therefore resolve its bundled corpus outside this checkout; run manifests record the
resolution source and any fallback degradation reason.

`trace-contract.complete.jsonl` is the miniature, synthetic diagnostics ledger used to verify
per-turn request-hash linkage and required trusted trace presence without making a live call.

## Accepted fidelity gaps

This list is documentary only. No probe run is compared with a manual browser capture, and no
reference capture, accepted-gaps machine format, or structural verdict exists.

- A probe has no browser-relayed envelopes, so it has no untrusted diagnostic exchanges and no
  `SpeechPlaybackCompleted` records.
- A model-scoped attach does not run the browser SDP route: it has no `call_bound` or
  `sdp_rendezvous` latency observation, and it has no `call_invalidated` stop-path record because
  it has no browser `call_binding` to invalidate.
- The typed-turn `provider_id` is session-scoped (`<model>:typed` for the model session), rather
  than the browser-call label `{call_id}:typed`.
- There is no audio input, so the probe does not cover barge-in or interruption and does not emit
  `ignored_continuation_transcript`.
- The `input_transcription` token class is declared in the session configuration but is never
  billed by a typed-only probe; its token accounting is therefore not directly comparable with a
  voice run.

These differences are documented so consumers understand the corpus boundary; they are not
enforced against a reference artifact. If a run is cited by an experiment or report, keep the
generated run under `state/` during execution, confirm `secret_scan.found == false` in the run's
`run-manifest.json`, and copy it by hand to `evaluation/frozen/realtime-probe/<run-id>/`. There is
no freeze command. The phrase script and seed remain entirely synthetic and must never contain
real personal data.
