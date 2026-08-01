# Plan: Headless scripted realtime conversation probe

Status: In progress — Phases 1, 2, 3, 4, 5, and 6 complete (2026-08-01, `feature/headless-conversation`;
Phases 1, 2, 5, and 6 include their live operator runs); next is launcher integration
Maturity: Candidate
Area: Realtime session server / Launcher / Artifact generation

## Why this plan exists

There is no way to produce a realtime-session artifact corpus without a human holding a voice
conversation in the browser. Every corpus we have — diagnostics JSONL, promoted continuity state,
volition snapshots, volition/world/initiative traces, per-turn model use — exists only because the
operator spoke into a microphone for several minutes. That makes corpus generation expensive,
unrepeatable, and impossible to schedule.

This plan adds a repeatable, headless, scripted conversation run against the **live OpenAI Realtime
API** whose filesystem output is equivalent to a manual `.\scripts\qsf.ps1 realtime` session in which
the operator types turns into the browser UI. Its purpose is **artifact generation for analysis**,
not assertion-style testing of model behavior: it produces a comparable corpus on demand.

**Done** = one launcher command (`.\scripts\qsf.ps1 probe`) runs a designed phrase script end to end
against the real API with no browser and no UI, prints per-turn console progress, always writes a
terminal `run-manifest.json`, fails loudly and exits non-zero if the run did not produce a clean
promoted corpus, and leaves a realtime-shaped artifact tree that `qsf.ps1 transcript`,
`qsf.ps1 goals`, and `qsf.ps1 sleep` read unchanged.

Real API cost is accepted, including audio-modality output tokens.

This is harness/launcher engineering whose outcome is not in doubt, so it gets a phased
`Plan.*.md` and **no `Experiment.*.md`** (`ProjectWorkflow.md`, "Document Tracks: Plans vs
Experiments"). The phrase script is a fixture, not an experiment spec. The corpus it generates may
later feed experiments; those experiments will own their own documents.

### Naming and ephemerality

This document owns the ephemeral phase labels below. Durable artifacts — modules, the launcher
command, decision-log entries, architecture text — name the behavior ("model-scoped sideband
attach", "trusted turn completion signal", "live-goal-formation drain barrier", "scripted
conversation run"), never a phase number (`Agents.md`; `ProjectWorkflow.md`). This plan is
self-contained: it cites code, architecture documents, and the decision log, and depends on no other
ephemeral document to be readable.

---

## Corrections verified against the source

Each item below states what the code actually does, what a plausible reading of it gets wrong, and
where to check. These are load-bearing: several phases exist only because of them, and the in-text
references elsewhere in this document point at the numbers here.

1. **Two Weak keywords do not qualify a goal for arbitration.** Activation keywords carry curated
   weight classes (Weak = 1, Normal = 4, Strong = 8; `crates/qsf_volition/src/model.rs:36-42`) and a
   selection must reach the global qualification threshold of 4
   (`DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD`, `model.rs:9`) before it can win. So a turn whose
   only hits on `serve-the-present-person` are `what`(1) + `want`(1) scores 2, and the goal never
   reaches arbitration at all — it is recorded as a below-threshold selection. Any phrase intended to
   put that goal into a contest needs at least one Normal keyword (`please`, `help`, `explain`).
2. **`create_session` never restores `session-state.json`.** It restores only the volition continuity
   snapshot (`volition-state.json`, gated on `snapshot_is_fixture_compatible`) and an optional
   `volition-seed.reviewed.json` (`crates/qsf_realtime_server/src/state.rs:217-340`);
   `SessionRuntime::new` builds a fresh `SessionState`, and the first promotion overwrites
   `session-state.json` wholesale. Seeding that file would therefore be silently discarded. "Warm
   start" in the realtime server means carried-over volition state plus the memory store — never
   carried-over turns.
3. **`relay_event_received` has two writers, and only one of them is the browser.** The sideband
   itself writes it for `session.created` and `session.updated`
   (`sideband_provider_event.rs:49-68`), under any attach shape; the browser-relay envelope handler
   writes it for *every* relayed envelope kind (`routes.rs:768-775`). A headless run therefore does
   have `relay_event_received` records — what it lacks is the browser-relayed ones, and with them the
   untrusted diagnostic exchanges written by `persist_completed_diagnostic_exchanges` and the
   relay-only `SpeechPlaybackCompleted` kind.
4. **`call_bound`, `sdp_rendezvous`, and `call_invalidated` do not share a writer.** `CallBound`
   (`routes.rs:268-272`) and the `sdp_rendezvous` latency observation (`routes.rs:273-281`) are
   written by `exchange_sdp_impl`, which a model-scoped attach never runs. `CallInvalidated` is
   written by `stop_session_impl` (`routes.rs:328-337`) and by the relay `SessionStopped` handler
   (`routes.rs:944-960`), in both cases **only when a `call_binding` exists** (`routes.rs:321`,
   `routes.rs:944`). A model-scoped session has no binding, so a headless run emits no
   `call_invalidated` — for the missing-binding reason, not because the SDP route was skipped. A
   manual typed browser session *will* emit one when the operator stops it.
5. **The first-audio latency label's source event depends on the attach shape.** One match arm
   handles `response.output_audio.delta`, `response.audio.delta`,
   `response.output_audio_transcript.delta`, and `response.output_audio_transcript.done`, and the
   first of them to arrive sets `first_audio_received_at` (`sideband_provider_event.rs:389-451`).
   Under `?call_id=` the sideband never sees the raw audio deltas — audio goes to the browser over
   WebRTC — so in practice a transcript delta sets it. Under `?model=` the raw audio delta arrives
   first and would silently redefine every label derived from that timestamp. Two labels derive from
   it: `response_created_to_first_audio` and `final_transcript_received_to_first_audio`; the
   ~600–850 ms envelope recorded in `docs/Experiments/Experiment.WorldConsultation.md` is a
   *transcript*-to-first-audio measurement, i.e. the second label. Pinning `first_audio_received_at`
   to the transcript event types preserves both.
6. **The session token ledger is never persisted.** `TokenUsageSnapshot` lives on `SessionRuntime`
   and is published only over the events socket (`state.rs:665-669`) plus `engine_logging` lines
   (`realtime/token_usage.rs`). On disk only per-turn `ExchangeModelUse` inside `session-state.json`
   records model spend, so a run has no durable by-model/by-class accounting unless something writes
   one.
7. **The explicit-topic world trigger accepts more anchors than "a capitalized entity".**
   `explicit_topic_world_consultation_request` (`crates/qsf_volition/src/initiative.rs:63-122`)
   requires a current-information cue plus at least one anchor, where anchors are (a) capitalized
   non-stoplisted words, preferring those after position 0 but falling back to a sentence-initial one,
   and (b) dotted numeric versions. So a phrase pair meant to isolate capitalization must contain no
   dotted version and must begin with a word that `is_generic_world_query_term` already stoplists
   (`initiative.rs:125-207` — e.g. `Can`, `What`, `Do`, `I`), or the "lowercased" variant fires on its
   own sentence-initial capital and the pair proves nothing.
8. **There is no positive "sideband attached" signal.** `SidebandStatus` is `{degraded, detail}` and
   starts `degraded: false` before any attach attempt; `handle_provider_event` clears degradation on
   `session.updated` only when it was already set. Nothing distinguishes "not yet attached" from
   "attached and healthy", so a driver that submits its first turn immediately races the websocket
   handshake.
9. **A function-call-only or mixed `response.done` deliberately does not finalize the exchange**
   (`sideband_response_done.rs:152-394`); finalization waits for the eventual spoken response. Any
   turn-completion signal must therefore come from the promotion path, not from a response event.
10. **Sideband degradation is not latched.** `SessionRuntime::set_sideband_status(false, None)` clears
    **both** `SidebandStatus.degraded` and `SessionRuntime.degraded` (`state.rs:673-677`), and
    `handle_provider_event` calls exactly that on `session.updated` after a degradation
    (`sideband_provider_event.rs:50-51`). Promotion therefore skips exchanges only *while* degraded
    (`sideband_exchange_promotion.rs:37-44`); what is latched is `non_promotable_exchange_indices`,
    per exchange. A consequence: a latest-value `watch` channel carrying `degraded` cannot prove
    "never degraded over a whole run", because a fast degrade/recover overwrites the value before a
    receiver reads it.
11. **A model-scoped websocket is itself the stateful provider session.** `connect_and_run_once`
    reopens the URL and re-sends only `session.update` (`sideband_connection.rs:81-227`). For
    `?call_id=` that reattaches to the browser-owned call, whose conversation survives. For `?model=`
    it starts a *new* session with an empty conversation while the local `SessionRuntime` still holds
    every earlier turn, so later exchanges could promote with model context that no longer matches
    the recorded local conversation — and the runner would keep spending money producing misleading
    artifacts. Today's retry loop is correct for the browser call and only for it.
12. **Trusted-turn completion is not the end of a turn's side effects.** Promotion runs *before*
    `spawn_live_goal_formation` (`sideband_response_done.rs:484` then `:513-519`); formation then runs
    in a detached FIFO worker that mutates in-memory volition state, records token usage, and appends
    diagnostics (`live_goal_formation.rs:79-188`); and promotion is the **only** server-side writer
    of `volition-state.json` (`sideband_exchange_promotion.rs:89-97`). So the last turn's formation
    result is never persisted, and diagnostics can be appended after a naive end-of-run scan. A turn
    delay is not a synchronization contract. Worse for any barrier built on the queue:
    `spawn_live_goal_formation` spawns a task that *then* locks the session and pushes onto
    `live_goal_formation_queue` (`live_goal_formation.rs:104-124`), so immediately after
    `response.done` the queue can still look empty while a push is pending.
13. **`Get-QsfCompletionStateDirs` is one level deep.** It enumerates only the immediate children of
    `state/`, excluding `state/backups` (`scripts/qsf-completion.ps1:171-186`). A run directory at
    `state/probe/<run-id>` is therefore *not* completable; only its parent `state/probe` is offered,
    which is not a path `transcript`, `goals`, or `sleep` accept.
14. **The websocket attach sends no `OpenAI-Safety-Identifier` header today.**
    `connect_and_run_once` layers on only `Authorization` (`sideband_connection.rs:94-103`); the
    header appears solely in the SDP POST (`routes.rs:219-226`, hashing the session id through
    `hash_session_id` at `routes.rs:1040-1051`). Current official OpenAI documentation shows the
    header on server-to-server Realtime WebSocket connections, so applying it there is real work, not
    a redundancy, and needs no design fork.
15. **`qsf.ps1 realtime` does not forward `-StateDir` to the server, although the server accepts it.**
    `Start-RealtimeServerProcess` builds `cargo run -p qsf_realtime_server` plus, optionally,
    `--random-session-id` (`scripts/qsf.ps1:1197-1209`), while `qsf_realtime_server`'s CLI already has
    `--state-dir` with default `state/realtime` (`crates/qsf_realtime_server/src/cli.rs:16-17`). Every
    manual session therefore writes `state/realtime`; and because `DiagnosticWriter::create` opens the
    ledger in **append** mode (`crates/qsf_diagnostics/src/writer.rs:17-26`), a `default`-id session
    is not isolated from earlier sessions in that directory. Adding the passthrough is a pure launcher
    change.
16. **Recency decay reads `last_reinforced_at` first, and 30 days is only the default half-life.**
    `compute_recency_decay` uses `record.last_reinforced_at.unwrap_or(record.created_at)`
    (`crates/qsf_memory/src/retrieval.rs:393`) and divides by `effective_decay_halflife_days`, which
    honors a per-record `time_sensitive_decay_half_life_days` override, then falls back to 7 days for
    `MemoryProvenance::WorldObservationExternal` records and 30 days otherwise (`retrieval.rs:11`,
    `:14`, `:399-409`). Any checked-in memory fixture with fixed absolute timestamps therefore stops
    retrieving as wall-clock time passes, and the relevant timestamp to rewrite is
    `last_reinforced_at` whenever it is set.
17. **No Pester test asserts the realtime server's command line.**
    `Describe "qsf.ps1 realtime launcher"` (`scripts/qsf.Tests.ps1:172-284`) covers UI target
    resolution, store defaults, ports, the environment delta, and secret checks; there is no assertion
    anywhere on `Start-RealtimeServerProcess`'s argument list. Changing that command line means
    writing that coverage, not extending it.

---

## Settled design, with reasons

1. **Transport: a second sideband attach shape.** The sideband gains a server-owned model-scoped
   session (`?model=<model>`) alongside today's browser-call session (`?call_id=<id>`), modeled as
   one behavior-named `SidebandAttachment` enum carrying an explicit reconnect policy, not a boolean.
   Rejected alternative: driving headless Chromium via Playwright — it would additionally reproduce
   the untrusted browser-relay records, but adds a Node/browser dependency, fake-audio-device flags,
   and flakiness for records that are diagnostic-only by decision (*"Authoritative realtime sideband
   supersedes the browser relay"*). Every durable artifact is written by the server-side sideband, so
   the model-scoped attach reaches full artifact fidelity for the trusted plane.
2. **Muting is passive.** Assistant audio is discarded; the probe never sends `response.cancel`.
   Cancellation is an interruption, and interrupted exchanges are non-promotable
   (*"Interruptions are captured as diagnostics, not durable continuity"*). `output_modalities`
   stays `["audio"]` so provider behavior matches a real session.
3. **Artifact placement: a per-run state dir with session id `default`** — `state/probe/<run-id>/`,
   giving `continuity/default/…` and `diagnostics/default.jsonl` inside it.
   `qsf_session::resolve_continuity_session_dir` prefers `default`, falls back to a single session,
   and hard-errors on several sessions under one root (`continuity.rs:69-124`); a new session id
   under the shared `state/realtime` would be silently ignored by `sleep`, and two probe runs there
   would make `sleep` fail outright. A per-run dir also gives each run a **fresh** append-mode
   diagnostics ledger, which is what makes the ledger's contents attributable to exactly one run
   (Corrections item 15).
4. **The phrase script and its seed state ship as one checked-in fixture bundle.** Goal activation is
   exact-term matching with no stemming (`qsf_volition::normalize_terms`), so phrases must hit real
   keywords in `qsf_volition::realtime_seed_fixture()`.
5. **Document track: this plan, no experiment document.**
6. **Accepted fidelity gaps**, restated in the fixture README and encoded machine-readably in the
   structural comparison's accepted-gaps file:
   - No browser-relayed envelopes, and therefore no untrusted diagnostic exchanges and no
     `SpeechPlaybackCompleted` (Corrections item 3).
   - No `call_bound` and no `sdp_rendezvous` latency observation, because the model-scoped attach
     never runs the SDP route; and no `call_invalidated`, because a model-scoped session has no
     `call_binding` for the stop path to invalidate (Corrections item 4).
   - The typed-turn `provider_id` label is session-scoped instead of `{call_id}:typed`.
   - No barge-in/interruption coverage and no `ignored_continuation_transcript`, because there is no
     audio input.
   - With no audio input the `input_transcription` token class is declared but never billed, so probe
     token accounting is not directly comparable to a voice run.

### Two forks resolved by the operator

- **Post-attach disconnect of a model-scoped session: fail closed.** Retry freely *before* the first
  successful attach; once a `ServerModelSession` has attached, a disconnect stops the probe
  immediately with a failing verdict. Conversation replay-and-verify is explicitly **not** designed —
  the added correctness surface is not worth salvaging an interrupted run. `BrowserCall` keeps
  today's reattach behavior.
- **Live-goal-formation drain timeout: an accepted structured partial, not a corpus failure.** The
  finalizer waits on the formation barrier with a bounded timeout; on timeout (or on formation
  failure) the manifest carries an explicit clause and the console summary says so prominently, but
  the verdict still passes. Everything else in the corpus stays valid and the operator can see
  exactly what is missing. All other failing verdict clauses stay strictly deterministic.

---

## Trace completeness contract

`Agents.md` requires this because the entire output of this work is traces.

**Required fields per trusted turn** (all already exist; the probe must not lose any of them):

```text
input                            diagnostics: diagnostic_exchange_recorded{source:"sideband_trusted"}
                                 -> exchange.utterances[].transcript, final_user_input, provider_id
events_applied                   diagnostics: volition_context_injected.trace.events_applied,
                                 volition_tick_before
selector_output                  diagnostics: volition_context_injected.trace — ranked/selected goals
                                 with match strength and matched terms
omitted_or_suppressed_candidates same trace — below-threshold selections, omitted/blocked counts,
                                 suppression_reason (incl. below_qualification_threshold)
arbitration_result               same trace — winner goal id, effective tier, mode-biased tier,
                                 ordered losers
bounded_or_external_output       diagnostics: realtime_bounded_initiative.trace (initiative output,
                                 surfaced, rendered_line_present) and world_consultation_performed
                                 .trace (source-tagged query terms, required anchors, candidates and
                                 omission reasons, exact model-visible text, injection point,
                                 latency, external-effect flag, corpus marker)
detached_formation_outcome       diagnostics: live_goal_formation_performed | _failed | _skipped,
                                 plus the settled/expected counts in run-manifest.json
dynamic_state_snapshot           initiative trace state snapshots before/after; the explicitly
                                 persisted end-of-run continuity/default/volition-state.json
artifact_or_report_reference     request_hash linking turn_context_captured.request_hash to
                                 volition_context_injected.trace.request_hash and the initiative
                                 traces; run-manifest.json
model_use_and_cost               continuity/default/session-state.json turns[].model
                                 (ExchangeModelUse) plus the run manifest's token-ledger snapshot
```

**Artifact boundary for a probe run directory:**

```text
state/probe/<run-id>/diagnostics/default.jsonl
  Chronological facts plus the structured causal traces above. Fresh per run, because the run
  directory is fresh (the diagnostics writer appends).

state/probe/<run-id>/continuity/default/session-state.json
  Durable promoted turns (the canonical transcript source).

state/probe/<run-id>/continuity/default/volition-state.json
  End-of-run volition dynamic state, written explicitly by the finalizer after the formation
  barrier — not merely whatever the last promotion happened to persist.

state/probe/<run-id>/continuity/default/memory-store.json
  Seeded memory; the realtime server only reads it. Changed only by a follow-on `sleep`.

state/probe/<run-id>/continuity/default/continuity-manifest.json
  Session/state/snapshot pointers, sleep_pending, resume mode.

state/probe/<run-id>/run-manifest.json
  Terminal run provenance and verdict, always written after run-directory creation:
  status (passed | failed | infrastructure_error), fixture id + content hash, phrase count, run id,
  attach shape and reconnect policy, model ids, git commit (optional), start/end times, per-turn
  timings, promoted turn count, non-promotable indices, degradation epoch and every recorded
  degradation reason, sideband termination reason, formation clause (expected / settled / failed /
  timed_out / timeout_ms), token-ledger snapshot, audio-delta counts and byte volume, world-corpus
  state and marker, seed mode, expectation diff, structural-comparison result, no-secret scan
  result, finalization errors.
```

**Automated artifact-parsing verification** (not merely run status):

- The finalizer re-reads the generated `diagnostics/default.jsonl` and asserts, for every promoted
  turn, that a `volition_context_injected` trace, a `turn_context_captured` record with a matching
  `request_hash`, and a trusted `diagnostic_exchange_recorded` all exist. A missing field fails the
  run. The parse happens **after** the sideband is stopped and the session removed, so no writer can
  append afterwards.
- `.\scripts\qsf.ps1 transcript -StateDir state/probe/<run-id> -Full -Out <path>` must emit
  `source.complete == true`, exactly one `turn` line per phrase, and no non-empty `undecodable`.
- `.\scripts\qsf.ps1 goals -StateDir state/probe/<run-id>` must emit a non-empty goal listing.
- The structural comparison parses the run and a checked-in structural reference and asserts record
  kinds, `(field path, JSON type)` pairs, and required-file presence — never values.

---

## Phase 1 — Realtime websocket URLs, the sideband attachment shape, and its reconnect policy

**Status: COMPLETE (2026-07-30).** Implemented by Codex GPT-5.6-Luna, reviewed by Claude Opus
(seven findings, all approved and applied by Codex GPT-5.6-Sol), verified green. Work is
uncommitted on `feature/headless-conversation`.

**What was done**

- `crates/qsf_realtime_protocol/src/lib.rs`: `build_openai_realtime_browser_call_ws_url(base,
  call_id)` and `build_openai_realtime_model_ws_url(base, model)` next to
  `OPENAI_REALTIME_WS_BASE_URL`; both take the base URL (trailing-slash tolerant) so tests point at
  a local stub, with unit tests pinning the produced shapes.
- `AppState::openai_realtime_ws_url` delegates to the shared call-id builder;
  `crates/qsf_app/src/audio/voice_session_provider.rs` lost its private
  `OPENAI_REALTIME_VOICE_WEBSOCKET_BASE_URL` literal and inline `format!` and now consumes
  `OPENAI_REALTIME_WS_BASE_URL` plus the shared model builder.
- New module `crates/qsf_realtime_server/src/realtime/sideband_attachment.rs`: `SidebandAttachment
  { BrowserCall { call_id }, ServerModelSession { model } }` with `websocket_url(base)`,
  `provider_id()`, `typed_turn_provider_id()` (`{label}:typed`; added in review so the browser
  typed-turn `provider_id` stays byte-identical to the pre-existing `{call_id}:typed`),
  `call_id()` (`None` for model sessions, so model-attached `ProviderEventRecord`s carry
  `call_id: null`), and `reconnect_policy()` (`BrowserCall -> ReattachToOwningCall`,
  `ServerModelSession -> FailClosedAfterFirstAttach`). Module docs state the Corrections item 11
  rationale; `Display` impls name the concrete attachment and policy in log lines.
- The attachment threads as `&SidebandAttachment` through `SidebandHandle::spawn`, `run_sideband`,
  `connect_and_run_once`, `handle_text_turn`, `handle_provider_event`,
  `handle_response_done_event`, and `mark_session_degraded`, replacing the call-id parameter.
  `exchange_sdp_impl` constructs `BrowserCall`. **No string→attachment coercion exists**: a
  review-proposed `From<&str>` convenience was rejected because it would silently give a bare
  string the browser-call reconnect policy; the six pre-existing sideband test files were instead
  mechanically updated to construct attachments (asserted behavior unchanged;
  `sideband_promotion_tests.rs` needed no edits).
- `run_sideband` consults the policy: before the first successful attach (receiving
  `session.updated`) both policies retry with the existing backoff (preserving the `?call_id=`
  404-until-WebRTC-handshake behavior); after it, `FailClosedAfterFirstAttach` records the
  degradation, publishes a terminal `terminated` reason, and exits, while `ReattachToOwningCall`
  keeps the pre-existing loop. Review hardening: a requested stop always wins over fail-closed
  termination (so a finalizer stop racing a provider close cannot fail a clean run); unexpected
  stream exhaustion (`stream.next()` → `None` without a stop request) is treated as a disconnect,
  not a stop; `attached` is cleared on task exit.
- `SidebandStatus` gained minimal `attached` and set-once `terminated` fields (both
  `#[serde(default)]`) — the slice Phase 1's fail-closed policy itself requires. The monotonic
  `degradation_epoch` was deliberately **not** added (Phase 4b owns it). The browser events-socket
  message does not include the new fields, so no `ui/` changes were needed.
- `OpenAI-Safety-Identifier` is applied to the websocket handshake for both attach shapes;
  `hash_session_id` was extracted into `crates/qsf_realtime_server/src/realtime/
  safety_identifier.rs`, which also owns the header-name constant, and both the SDP POST and the
  websocket attach use it.

**Verification (ran 2026-07-30, all green)**

- `cargo build`; `cargo test -p qsf_realtime_protocol -p qsf_realtime_server -p qsf_app`
  (761 tests, 0 failures); `cargo clippy --all-targets -- -D warnings`; `cargo fmt --check`.
- Offline lifecycle tests against a local websocket stub: the model-scoped session attaches,
  completes exactly one promoted turn (pinned — the test cannot pass vacuously), the stub drops
  the connection, no reconnect is attempted, the task exits with a terminal reason, and a
  subsequently submitted phrase allocates no exchange and no turn; the `BrowserCall` mirror test
  asserts the reattach loop still runs. The stub captures handshake headers and asserts the
  safety-identifier hash for both shapes.

**Deferred follow-ups** (review advisories, deliberately not applied): a third copy of the
realtime base URL remains in `crates/qsf_app/src/audio/transcript_provider.rs`
(`?intent=transcription` — a matching shared builder would finish the DRY job); the caller-side
"one source of truth" URL tests are delegation-tautological and would be stronger asserting
literal URLs; the crate-level `#[allow(dead_code)]` on `SidebandAttachment` should fall away when
the probe constructs `ServerModelSession`; `set_sideband_status` and `set_sideband_attached`
assemble the published status separately and should share one `publish_status()` when Phase 4b
adds fields.

---

## Phase 2 — Live model-scoped attach reconnaissance (first paid step, cheapest)

**Status: COMPLETE (2026-07-30), including both operator runs.** Implemented by Codex
GPT-5.6-Luna, reviewed by Claude Opus (ten findings, all approved and
applied by Codex GPT-5.6-Sol). Operator decisions recorded during review: the idle-close
measurement is split into its own unbilled `#[ignore]` probe with a 900-second default cap, and the
inventory artifact may record enumerated `response.status` values, provider error type/code
identifiers, `(field path, JSON type)` pairs, and QSF-configured tool names — every other string
value stays forbidden regardless of length. Review hardening beyond the original sketch: both
raw-audio event names are probed, non-completed responses and pre-`response.done` `error` events
fail fast with the provider's actual complaint, handshake rejections are reported through the
shared `format_connect_error`, `DEFAULT_PCM_RATE_HZ` is the re-exported production constant, and a
capture error is printed even when the artifact write also fails. Offline verification is green
(`cargo build`; `cargo test -p qsf_realtime_server` with both live probes ignored; clippy
`-D warnings`; `cargo fmt --check`).

One real model-scoped session's provider-event stream must be captured **before** the suppression and
latency design in Phase 3 is locked, because Corrections item 5's fix depends on which event actually
arrives first.

**What was built**

- New integration tests in `crates/qsf_realtime_server/tests/model_scoped_attach_smoke.rs`, both
  marked `#[ignore]` so they never run in a normal `cargo test`. They are durable, separately
  invokable live probes, not throwaway code: `model_scoped_attach_smoke` owns the billed turn capture,
  while `model_scoped_attach_idle_close_probe` owns the no-turn idle-close measurement.
- Both tests build the model-scoped URL via the Phase 1 builder, attach with the bearer header and
  the safety identifier, and send the same `session.update` the sideband sends
  (`build_openai_realtime_conversation_session_update` with `output_modalities: ["audio"]`,
  `create_response: false`, `interrupt_response: false`, the default tool list and transcription
  model). The billed test sends one `conversation.item.create` + `response.create`, reads until a
  completed `response.done`, then ends immediately. The idle test submits no turn and waits for the
  provider close with a 900-second default cap configurable through
  `QSF_MODEL_SCOPED_ATTACH_IDLE_CLOSE_TIMEOUT_SECS`; read errors count as categorized close
  observations, and cap expiry records `cap_elapsed` and says explicitly how to raise the cap.
- Each test writes its own **event-shape inventory** artifact (not raw payloads), defaulting under the
  repository-root `state/`: for each observed event `type`, the count, first-seen offset in
  milliseconds from `response.created` where applicable, the set of top-level JSON keys, the full
  set of normalized `(field path, JSON type)` pairs, and byte lengths for long strings. The artifact
  may additionally contain enumerated `response.status` values, provider error type/code identifiers,
  the configured tool names from QSF's own session config, and the idle outcome/category/offset.
  Every other string value is forbidden regardless of length; payload and transcript text are never
  written. The billed artifact path remains overridable by
  `QSF_MODEL_SCOPED_ATTACH_ARTIFACT_PATH`.
- Observations the later phases depend on:
  1. Does either raw-audio event name (`response.output_audio.delta` or `response.audio.delta`)
     arrive, and does it carry base64 in `delta`?
  2. Which event type arrives first after `response.created` (the first-audio label's source)?
  3. **Smoke assertion only:** the documented `OpenAI-Safety-Identifier` header is accepted on the
     handshake. A rejection would be a provider/docs mismatch to report and escalate, not a design
     fork — the header stays the default.
  4. Do `session.updated`, tool advertisement, and `response.done` (including `usage`) match the
     `?call_id=` shapes the sideband already parses?
  5. How long an idle model-scoped session stays open before the provider closes it. This bounds how
     slow a scripted run can be before the fail-closed policy cuts it short.

**Verification**

- `cargo build`; `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.
- `cargo test -p qsf_realtime_server` (the two ignored tests do not run).
- **Operator / human testing required (paid):**
  `cargo test -p qsf_realtime_server --test model_scoped_attach_smoke model_scoped_attach_smoke -- --ignored --exact --nocapture`
  with `OPENAI_API_KEY` set. Cost: **one realtime session, one turn, one short spoken response** —
  the cheapest paid live step in this plan. The operator records the event-shape inventory (or its
  path) in the phase's follow-up so the next phases are designed on evidence.
- **Operator / human idle measurement (no turn and no token billing):**
  `cargo test -p qsf_realtime_server --test model_scoped_attach_smoke model_scoped_attach_idle_close_probe -- --ignored --exact --nocapture`.
  This is deliberately separate so it can be re-run or skipped without buying another turn.

**Operator follow-up — both live runs performed 2026-07-30, observations recorded:**

Artifacts: `state/model-scoped-attach-event-shape-inventory.json` (billed turn capture) and
`state/model-scoped-attach-idle-close-inventory.json` (idle probe). The five observations:

1. **Raw audio deltas arrive under the modern name only**: `response.output_audio.delta`
   (7 events for one short greeting, `delta` chunks of ~22–25 KB, all base64-decodable). The
   legacy `response.audio.delta` name never appeared.
2. **The transcript delta precedes the raw audio delta.** First event after `response.created`
   was `response.output_item.added` (297 ms); `response.output_audio_transcript.delta` arrived at
   348 ms, the first `response.output_audio.delta` at 931 ms, `response.done` at 1452 ms.
   Corrections item 5's concern — that under `?model=` the raw audio delta would arrive first and
   silently redefine the first-audio labels — did **not** materialize; the transcript pinning in
   the audio-suppression work remains correct and is retained as shape-independent insurance.
3. **The `OpenAI-Safety-Identifier` header was accepted** on the model-scoped handshake
   (`handshake_accepted: true` in both artifacts). No provider/docs mismatch to escalate.
4. **`session.updated`, tool advertisement, and `response.done` match the `?call_id=` shapes the
   sideband parses.** All five configured tools are advertised by name; `response.done.usage`
   carries every nested field the token ledger reads (`input_token_details.text_tokens`,
   `input_token_details.cached_tokens_details.*`, `output_token_details.*`). Statuses observed:
   `in_progress`, `completed`; no provider error events. One novelty: transcript deltas carry an
   `obfuscation` field the sideband does not parse (harmless).
5. **An idle model-scoped session stays open for at least 900 s** — the probe hit its cap
   (`cap_elapsed`, 900 001 ms) without a provider close. This comfortably exceeds any inter-turn
   gap a scripted run can produce (the per-turn timeout defaults to 120 s), so the fail-closed
   policy is not at risk from run pacing; the session's absolute lifetime is separately bounded by
   the `session.expires_at` value the artifact captured. No longer measurement is needed.

---

## Phase 3 — Raw audio payloads never enter the artifact plane; first-audio latency keeps its meaning

**Status: COMPLETE (2026-07-30).** Implemented by Codex GPT-5.6-Terra, reviewed by Claude Opus (six
findings, all approved and applied by Codex GPT-5.6-Sol), verified green. Work is uncommitted on
`feature/headless-conversation`.

**What was done**

- `sideband_provider_event.rs`: the four-event arm is split into two guard arms driven by two named
  constants — `RAW_OUTPUT_AUDIO_DELTA_EVENT_TYPES` (`response.output_audio.delta`,
  `response.audio.delta`) and `OUTPUT_AUDIO_TRANSCRIPT_EVENT_TYPES`. The raw-audio arm constructs no
  `ProviderEventRecord` at all; the transcript arm is byte-for-byte today's behavior. Both arms are
  gated on event type only, so the invariant holds identically for both attach shapes. Match-arm
  ordering keeps a raw-audio type from reaching the catch-all arm, whose `text` comes from
  `realtime_event_text` (top-level `text` only, never `delta`).
- `first_audio_received_at` is now set only in the transcript arm, so `response_created_to_first_audio`
  and `final_transcript_received_to_first_audio` keep their pre-existing meaning under both shapes.
  A new `first_output_audio_received_at` drives the separate `response_created_to_first_output_audio`
  observation, emitted from the raw-audio arm and re-attempted in the `response.created` arm so an
  early audio delta cannot silently drop the label (the mirror of the pre-existing catch-up for its
  sibling — added in review). All three new `SidebandRuntimeState` fields reset in
  `clear_in_flight_response_state`.
- `sideband_response_done.rs` logs the per-response audio-delta count and decoded byte volume through
  `engine_logging` with the session and response ids, then resets the counters immediately — log and
  reset are one operation. Review found the tool-loop path returns without the in-flight reset, so
  counting on the original "reset with the in-flight state" rule would have double-counted a `Mixed`
  response's audio; the operator chose per-response, matching this document and the log text.
- Decoded byte volume is computed arithmetically from the encoded length rather than by decoding, so
  it is exact, infallible, allocation-free under the session lock, and cannot report a silent zero on
  a payload variant the decoder rejects. `base64` stays a dev-only dependency.
- `RAW_OUTPUT_AUDIO_DELTA_EVENT_TYPES` is `pub` and re-exported from `lib.rs`, and the Phase 2 live
  probe consumes it instead of its own copy (the `DEFAULT_PCM_RATE_HZ` precedent), so the paid
  reconnaissance run cannot drift from the list production actually suppresses.

**Verification (ran 2026-07-30, all green)**

- `cargo build`; `cargo test --workspace` (all suites pass; the two paid live probes stay ignored);
  `cargo clippy --all-targets -- -D warnings`; `cargo fmt --check`.
- Regression coverage: the suppression test is table-driven over all four combinations of the two
  raw-audio event names and the two attachment variants, asserting no persisted `provider_events`
  entry exists for those types and no payload string survives, while a transcript delta's text *is*
  preserved; latency tests pin that the two first-audio labels come from a transcript delta and that
  the output-audio label appears only when a raw audio delta arrives, including the early-audio
  ordering; a `Mixed` tool-loop regression test pins the per-response counter reset; a unit test
  covers the decoded-length helper for padded, unpadded, and empty input.

**Follow-up this phase deliberately did not build:** the audio-delta count and byte volume live only
in `SidebandRuntimeState`, which the sideband task owns and zeroes at every `response.done`. Nothing
outside that task can read them, so the run manifest's audio-delta totals need a session-lifetime
accumulator that does not exist yet. The scripted-conversation runner phase owns adding it (noted
there); the `engine_logging` line is the only carrier until then.

Offline, gated on Phase 2's evidence. This is the architecture-invariant phase
(`Architecture.RealtimeSessionServer.md`: "Raw audio is not logged").

**Phase 2 evidence (2026-07-30) resolves this phase's open assumption:** under the model-scoped
attach the transcript delta (348 ms) arrives *before* the first raw audio delta (931 ms), so the
first-audio labels would not have been silently redefined in practice; pinning them to the
transcript event types is retained as designed, as shape-independent insurance. Only
`response.output_audio.delta` was observed (never the legacy `response.audio.delta`); keep both
names in the suppression list regardless, since the list exists to make payload paths
unrepresentable, not to mirror one observed run.

**Work**

- In `sideband_provider_event.rs`, split today's four-event match arm by **event type**, not by
  attach shape, so the invariant holds identically for both attach shapes:
  - Raw audio payload event types (`response.output_audio.delta`, `response.audio.delta`) emit
    **no** `ProviderEventRecord`. They still update the output-audio timestamp and are counted.
    Rationale for dropping rather than blanking: a blanked record would inflate the tree by
    thousands of empty records per turn and would make the probe's `provider_events` list
    structurally *different* from a browser-call run, where these events never reach the sideband at
    all. Dropping keeps both shapes identical on the artifact plane.
  - Transcript delta/done event types keep today's behavior exactly, including the recorded `text`.
  - The event-type lists live in one named constant so no future caller can reintroduce a payload
    path.
- Latency labels: pin `first_audio_received_at` — and therefore both
  `response_created_to_first_audio` and `final_transcript_received_to_first_audio` — to the
  **transcript** delta/done event types under both attach shapes. That preserves today's meaning and
  keeps the transcript-to-first-audio measurement comparable with the ~600–850 ms envelope recorded
  in `docs/Experiments/Experiment.WorldConsultation.md` (Corrections item 5). Add a separately named
  observation for the true output-audio delta (e.g. `response_created_to_first_output_audio`),
  present only under the model-scoped attach. If Phase 2 shows the first-arriving event is not what
  this assumes, adjust here and say so in the phase notes.
- Per-response audio accounting: at `response.done`, log via `engine_logging` the audio-delta count
  and total byte volume for that response with the session and response ids. No new persisted
  diagnostic record kind; the totals also land in the run manifest.

**Verification (automated)**

- `cargo build`.
- Regression test: drive `handle_provider_event` with a synthetic `response.output_audio.delta`
  carrying a base64 payload, complete the exchange, promote it, load the persisted
  `session-state.json`, and assert that **no** persisted `provider_events` entry contains the payload
  string and that no entry was recorded for that event type. Repeat with a transcript delta and
  assert its text *is* preserved.
- Test that the first-audio latency observations are emitted from a transcript delta and not from an
  audio delta, and that the output-audio label is emitted only when an audio delta arrives.
- `cargo test -p qsf_realtime_server` green; `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.

**Human testing**: not required this phase. **Cost**: none.

---

## Phase 4 — Turn completion, monotonic degradation, and the live-goal-formation drain barrier

Offline. This phase supplies the runtime signals a trustworthy verdict needs, closing Corrections
items 8, 10, and 12, and providing the terminal signal Phase 1's fail-closed policy needs.

### 4a. Trusted turn completion, published from the promotion path

- `SessionRuntime` gains a per-session `watch` channel, consistent with the existing five
  (`status_tx`, `turn_context_tx`, `volition_inspection_tx`, `world_perception_tx`,
  `token_usage_tx`), publishing `TrustedTurnCompletion { exchange_index, promoted,
  promoted_turn_count, skipped_reason: Option<..>, completed_at }`, with `subscribe_*` / `*_sender`
  accessors matching the existing pattern.
- Publish it from `promote_completed_trusted_exchanges` (`sideband_exchange_promotion.rs`) for
  *every* completed trusted exchange the loop consumes — including the `continue` branches for
  non-promotable exchanges, degraded sessions, and `Turn::try_from` conversion failures. A driver
  must never wait forever on an exchange the pipeline quietly dropped.
- Do **not** publish from `sideband_turn_injection.rs::send_response_create_and_capture`
  (function at line 659, publish site at lines 700-702) — that is injection time, at or just before
  `response.create`, and is what the existing channels mean. Function-call-only responses correctly
  publish nothing; the turn finalizes on the eventual spoken `response.done` (Corrections item 9).
- Trusted-turn completion means "this exchange left the promotion pipeline". It explicitly does
  **not** mean "every side effect of this turn is finished" — that is what 4c is for.

### 4b. Monotonic degradation state, attachment readiness, and terminal state

Because degradation is cleared on recovery (Corrections item 10), a latest-value channel carrying
`degraded` cannot support a whole-run claim. The fix is monotonic state.

- `SessionRuntime` gains `degradation_epoch: u32` and a bounded `degradation_reasons: Vec<String>`,
  incremented/appended by every `set_sideband_status(true, ..)` and **never** cleared for the life of
  the session. `degraded` keeps meaning *current* health, so the browser recovery UX is unchanged.
- `SidebandStatus` gains, all `#[serde(default)]`:
  - `attached: bool` — set true on `session.updated`, cleared on disconnect/degradation and on
    `session.closed`. This is the positive readiness signal that does not exist today (Corrections
    item 8).
  - `degradation_epoch: u32` — mirror of the monotonic counter.
  - `terminated: Option<String>` — set once when the sideband task exits without a stop request
    (the fail-closed model-session disconnect from Phase 1). Monotonic: never cleared during a run.
- Because `degradation_epoch` and `terminated` are monotonic, a latest-value watch receiver is
  sufficient for them; the runner never has to observe every transition. The verdict is built from
  these fields (read under the session lock at finalization) and both are recorded in
  `run-manifest.json`.
- The browser events-socket parser (`crates/qsf_realtime_server/ui/src/realtime.ts:684-703`) checks
  named fields and ignores extras, so no UI change is required. If the implementer chooses to surface
  `attached`, the epoch, or `terminated` in the UI, the `ui/` obligations apply (`npm run check` then
  `npm run fmt` from `crates/qsf_realtime_server/ui`).

### 4c. Live-goal-formation drain barrier and one shared continuity-persistence helper

Corrections item 12 is the whole reason this exists: the last turn's formation result is otherwise
never persisted, and diagnostics can be appended after an end-of-run scan.

- **Remove the enqueue race.** Split `spawn_live_goal_formation` into:
  - `enqueue_live_goal_formation(&mut SessionRuntime, exchange_index, turn_transcript,
    response_dispatched_at) -> bool /* should_spawn_worker */`, called by
    `handle_response_done_event` **while it still holds the session guard** (the transcript is built
    by the pure `qsf_models::format_exchange_transcript`, so it can be computed before the existing
    `drop(guard)`), and
  - `spawn_live_goal_formation_worker(session, qsf_session_id)` (plus the existing
    `…_with_client_builder` test seam, renamed to match), called after the guard is dropped.

  Without this split a barrier reading the queue can observe a false "drained" immediately after
  `response.done`, because the push happens inside the spawned task.
- New per-session `watch` channel publishing `LiveGoalFormationProgress { expected, settled, failed,
  in_flight }`, sent from inside the same lock acquisitions that mutate the queue and the in-flight
  flag — the enqueue, each worker pop/settle, the empty-queue reset in
  `drain_live_goal_formation_queue`, **and** `LiveGoalFormationInFlightGuard::drop` (the panic safety
  net), so a worker panic can never leave the barrier waiting forever. The published value can
  therefore never lag the real queue state.
- The barrier is `settled == expected && !in_flight`. It is a **separate** signal; trusted-turn
  completion is not overloaded.
- **One shared continuity-persistence helper.** Extract the volition-snapshot-plus-manifest write
  currently inlined in `promote_completed_trusted_exchanges`
  (`sideband_exchange_promotion.rs:88-116`) into a named helper used by both promotion and the
  finalizer, so the end-of-run snapshot writes the same files with the same manifest semantics and
  there is one implementation (`Agents.md` DRY). The finalizer's call is what makes the last turn's
  formation result durable.

**Verification (automated)**

- `cargo build`.
- Completion-signal tests (in the existing `sideband_promotion_tests.rs` home): a promoted exchange
  publishes `promoted: true` with the incremented count; a non-promotable exchange publishes
  `promoted: false` with a reason; a degraded session publishes `promoted: false` for every
  subsequent exchange; a conversion-failure exchange publishes rather than going silent; a subscriber
  attaching late immediately observes the latest completion.
- Degradation tests (in `sideband_status_tests.rs`): `session.updated` sets `attached: true`; a
  simulated disconnect clears it; **a degrade → recover sequence completed before the receiver ever
  reads leaves `degradation_epoch >= 1` and produces a failing verdict**; a fail-closed model-session
  disconnect sets `terminated` with a reason.
- Formation-barrier tests, using the `…_with_client_builder` seam with a **deliberately blocked**
  model client:
  - the barrier reports `in_flight` immediately after `response.done` (no false drained state);
  - finalization waits for the barrier rather than proceeding;
  - a formation result produced for the **last** phrase appears both in
    `diagnostics/default.jsonl` and in the explicitly persisted `volition-state.json`;
  - a never-completing client hits the bounded timeout and yields the structured-partial clause,
    not a verdict failure;
  - a panicking formation item still clears `in_flight` and releases the barrier.
- Existing `live_goal_formation.rs` tests, including the stale-goal-set test at line 824, updated for
  the enqueue/spawn split and kept green.
- `cargo test -p qsf_realtime_server` green; `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.

**Human testing**: not required. **Cost**: none.

**Status: COMPLETE (2026-07-31).** Implemented offline on `feature/headless-conversation`.

**What was done**

- Added trusted-turn completion and live-goal-formation progress watch channels, including
  completion publication for every consumed exchange and monotonic degradation epoch/reasons with
  unified status publication. Runtime health fields are private behind accessors so status updates
  cannot bypass the unified publisher; the first omitted degradation reason is logged when bounded
  retention fills.
- Split formation enqueue from worker spawning so the drain barrier observes the queue before the
  response handler releases the session lock. The barrier's first observation now takes that same
  lock, distinguishes session teardown from timeout, and documents the fresh-session boundary.
  Abnormal worker exits fail and diagnose only the active exchange, preserve queued work, and start
  a replacement worker. The shared continuity persistence helper remains the single artifact writer.

**Verification**

- `cargo build`; `cargo test -p qsf_realtime_server`; `cargo clippy --all-targets -- -D warnings`;
  `cargo fmt`.
- All new completion, status, barrier, last-result persistence, timeout, channel-closure, and
  queue-preserving panic-safety tests pass; the two paid live probes remain `#[ignore]`d.

---

## Phase 5 — Scripted conversation runner: `qsf_realtime_server probe`

**Status: COMPLETE (2026-07-31).**

**What was done**

- Added the scripted-conversation module tree, bundled synthetic smoke phrase set, pure reducer,
  verdict, trace-contract parser, secret scan, manifest serializer/atomic writer, and rendering.
- Added the headless probe command with its scripted-run defaults, in-process model-session
  sideband driver, status/completion waits, finalization path, terminal manifest, and non-zero
  failing verdict behavior. Auxiliary seed and structure modes intentionally report that their
  later-owned behavior is not available yet.
- `VolitionContextInjectionTrace` now persists a backward-compatible `request_hash`, populated
  from the exact `ContentHash` used for the provider request. The trace-contract parser links it
  per exchange to `turn_context_captured.request_hash`; missing, mismatched, and cross-wired hashes
  fail closed.
- Extracted the neutral shared session stop lifecycle for the HTTP route and probe finalizer. It
  takes and joins the sideband even when relay persistence fails, so artifact parsing starts only
  after the final writer is stopped.
- The always-run finalizer now retains the original run failure separately from finalization
  errors, snapshots the token ledger and actual promoted indices under the session lock, records
  the complete manifest provenance contract, scans the terminal manifest for secrets, and leaves
  a parseable failed manifest for attach and turn timeouts. Session-lifetime output-audio totals
  survive per-response resets and are included in that snapshot.
- The pure reducer/verdict/render path now owns infrastructure-error precedence, arbitration-winner
  progress, expectation differences, formation-barrier output, and structured-partial warnings.
  Supplied run directories are used exactly as given and rejected when they contain an earlier
  diagnostics ledger or terminal manifest.

**Verification**

- `cargo build`.
- `cargo test -p qsf_realtime_server -p qsf_diagnostics`.
- Unit coverage for every deterministic verdict clause and the structured formation clauses,
  phrase-set name/path/error loading, complete/missing/mismatched per-turn trace linkage, terminal
  manifest fields and atomic replacement, secret detection, and session-lifetime audio totals.
- Effect-layer coverage uses local websocket stubs to prove attach-timeout and turn-timeout runs
  stop/join their sidebands and leave failed terminal manifests, and that a blocked manifest target
  still returns the original run failure.

**Deliberate follow-ups**

- The structural-comparison result has an explicit `NoStructuralReferenceConfigured` state;
  it produces no divergence. The structural builder and reference remain the later artifact work.
- Seed materialization and structure-only document emission remain deferred to their owning work.

**Operator follow-up — the live smoke run was performed 2026-07-31, verdict `passed`.**

`cargo run -p qsf_realtime_server -- probe --phrase-set smoke`, two turns, run directory
`state/probe/20260731-073909`. Evidence collected:

- The trace contract parsed complete with `matching_request_hash: true` for both promoted turns,
  so the per-exchange injection-to-request linkage holds against real artifacts, not only fixtures.
- The session-lifetime output-audio totals survived the per-response resets: 60 + 10 + 36 deltas
  logged, `output_audio_delta_count: 106` and 1 980 000 decoded bytes in the manifest.
- The token ledger was captured for all three provider responses, including the tool-loop response —
  the accounting Corrections item 6 shows exists nowhere else on disk.
- The second phrase reproduced the designed script's tool-loop turn: nothing qualified,
  `arbitration_winner: null`, one `inspect_volition_state` request and one execution.
- `transcript -StateDir state/probe/20260731-073909 -Full` reported `source.complete == true`,
  zero skipped lines, zero orphans, two turn lines, and no non-empty `undecodable`.

Three defects the run exposed are folded into the fixture-bundle work below rather than fixed here,
because that is where the designed script, its expectation blocks, and the corpus-dependent
world-consultation turn all land: the expectation-diff rendering, the corpus-resolution provenance
gap, and the smoke set's inaccurate `expected` block.

The smallest viable end-to-end slice, with an always-run finalizer. It ships the phrase-set loader and
a minimal two-phrase smoke set so the first paid run is cheap and this phase can pass its own human
test; Phase 6 adds the designed script and the seed bundle and flips the default.

**Work — pure core first (input → action → reducer → state → render)**

New module tree `crates/qsf_realtime_server/src/scripted_conversation/` (named for stable behavior,
not for this plan), with `mod.rs` kept a thin re-export wrapper:

- `script.rs` — the phrase-set document: `id`, `description`, ordered phrases, and per-phrase
  `expected` metadata (expected qualifying goal ids, the **explicit tagged expected winner**
  (`none` or an exact goal id),
  the ordered expected loser ids for contest phrases, the expected below-threshold goal ids, and
  whether a world-consultation record is expected). Pure loading and validation, plus the
  fixture-root resolution: a name resolves against `docs/Experiments/Fixtures/realtime-probe/`, a
  path is used as given, and an unresolvable name errors with the resolved absolute path and the list
  of bundled names. Both forms are supported.
- **`docs/Experiments/Fixtures/realtime-probe/smoke.phrases.json` ships in this phase** — two short
  phrases with `expected` blocks — together with the fixture directory and a README stub, so
  `--phrase-set smoke` and the bundled default both resolve from the moment the subcommand exists.
- `run_state.rs` — pure reducer: `ProbeEvent { SidebandAttached, AttachTimedOut,
  TurnSubmitted{index}, TurnCompleted{index, promoted, elapsed_ms}, TurnTimedOut{index},
  SidebandTerminated{reason}, FormationBarrierSettled{expected, settled, failed},
  FormationBarrierTimedOut{..}, RunFinished }` applied to `ProbeRunState`. Unit-testable with no I/O.
- `verdict.rs` — pure `probe_verdict(&ProbeRunState, &RuntimeCounters, &TraceContractReport,
  &StructuralComparison, &SecretScanReport) -> ProbeVerdict`, producing a terminal
  `status: passed | failed | infrastructure_error`.
  **Failing clauses (all deterministic):** promoted turn count != phrase count; any non-promotable
  exchange index; `degradation_epoch > 0`; `terminated` set; attach timeout; any turn timeout; any
  missing required trace field; a structural divergence outside the accepted-gaps list; any secret
  found under the run dir.
  **Non-failing structured clauses:** formation timeout, formation failures, and per-turn expectation
  differences. These are recorded prominently and never fail the corpus (operator decision).
- `trace_contract.rs` — pure parser over the generated `diagnostics/default.jsonl` producing the
  per-turn presence report the contract requires.
- `secret_scan.rs` — pure `scan_for_secret(bytes, secret) -> bool` plus a directory walk over the run
  dir.
- `manifest.rs` — pure `build_run_manifest(...) -> RunManifest`, serialization, and an **atomic**
  write (temp file plus rename) so a partially written manifest can never be mistaken for a terminal
  one.
- `render.rs` — pure progress-line, structured-partial-warning, and verdict renderers (strings).
- `artifact_structure.rs` — see Phase 8.
- `runner.rs` — the isolated effect layer.

**Work — the finalizer**

`runner.rs` defines **one idempotent finalizer**, used on success and on every error after
run-directory creation. Ordinary `?` propagation past that point is forbidden; every fallible step
funnels through it: seed materialization, `AppState`/session creation, attach timeout, turn timeout,
sideband task failure or fail-closed termination, formation-barrier errors, trace parse failure,
structural comparison failure, secret detection.

Fixed finalization order, which is also what closes Corrections item 12:

```text
1. observe the last trusted-turn completion (or the failure that ended the run)
2. wait on the live-goal-formation barrier, bounded by --formation-timeout-ms
3. persist the final volition snapshot + manifest pointers through the shared helper
4. snapshot the token ledger and the runtime counters (degradation epoch and reasons,
   non-promotable indices, terminated reason, audio-delta totals)
5. stop and JOIN the sideband and remove the session, so nothing can append afterwards
6. parse the artifacts: trace contract, structural comparison, secret scan
7. atomically write the terminal run-manifest.json and render the verdict
```

**Step 4 needs a counter that does not exist yet.** The audio-payload-suppression work counts
audio deltas and their byte volume in `SidebandRuntimeState`, which the sideband task owns privately
and zeroes at every `response.done`, so the totals reach only an `engine_logging` line. Before the
finalizer can snapshot them, add a session-lifetime accumulator — incremented where the per-response
counters are, never reset for the life of the session — reachable from the finalizer under the
session lock like the other runtime counters. Without it the manifest's audio-delta totals and the
trace-completeness contract's corresponding field cannot be filled.

Precedence when finalization itself fails: the **original** failure is retained as the run's cause;
each finalization error is appended to the manifest's `finalization_errors` and the status becomes
`infrastructure_error` only if no earlier deterministic failure exists. If the manifest cannot be
written at all, the runner logs the terminal verdict through `engine_logging` at error level and
exits non-zero with the original failure. Errors *before* run-directory creation cannot write a
manifest; those exit non-zero with a clear message, and the finalizer contract is documented as
beginning at directory creation.

**Work — the run loop**

Creates the run dir, materializes the seed bundle when one is configured, constructs `AppState`,
calls `create_session()`, spawns the sideband with `SidebandAttachment::ServerModelSession`, waits
for `attached` with a timeout, then per phrase submits via `SidebandHandle::submit_text_turn` and
awaits the trusted-turn-completion channel with a per-turn timeout. Every wait is a `select!` over
the completion channel **and** the status channel, so a `terminated` status (fail-closed disconnect)
aborts the turn wait immediately instead of burning the full per-turn timeout, and no further phrase
is submitted. Every observation is fed back as a `ProbeEvent`.

Session stop reuses a shared path: extract today's `routes.rs::stop_session_impl` body into
`realtime::session_lifecycle::stop_session(state, qsf_session_id)`, used by both the HTTP route
(which stays a thin wrapper) and the finalizer, so finalization has one implementation.

**Work — CLI and defaults**

- `crates/qsf_realtime_server/src/cli.rs` gains an optional clap subcommand while keeping today's
  flat "serve" behavior when no subcommand is given (`cli.rs` stays a thin argument definition;
  `lib.rs::run` dispatches). `probe` flags: `--phrase-set` (default: the bundled fixture name, so the
  default exercises the new path with no flag), `--state-dir` (default `state/probe/<run-id>`),
  `--run-id`, `--cold-start`, `--turn-delay-ms` (default **250**), `--turn-timeout-ms` (default
  **120000**), `--attach-timeout-ms` (default **30000**), `--formation-timeout-ms` (default
  **60000**), `--git-commit` (optional metadata). Two auxiliary modes on the same subcommand:
  `--seed-only <dir>` materializes the seed bundle into a directory without running anything (a
  no-op until Phase 6 supplies the bundle), and `--structure-only <dir>` emits an artifact-structure
  document from an existing run (Phase 8).
- **No HTTP listener.** Justification from the code: the probe drives `SidebandHandle` in-process;
  the HTTP routes exist only for the browser. Binding would collide with a running
  `qsf.ps1 realtime` on the fixed port 3940 (pinned across `cli.rs:7`, the Vite proxy in
  `vite.config.ts:8`, and `qsf.ps1:51`), and the debug UI could not attach usefully anyway: its flow
  begins with `POST /api/realtime/session`, which would fail with "session `default` is already
  active", and the events socket needs a session id it can only learn from that call. Nothing durable
  is lost — every events-socket capture except the token ledger is also written to the diagnostics
  JSONL, and the token ledger is snapshotted into the run manifest (Corrections item 6).
- Console progress: run header (run id, state dir, phrase set + hash, attach shape and reconnect
  policy, model, world corpus state, seed mode), then per turn `n/N` with a truncated phrase, elapsed
  ms, promoted yes/no, and the arbitration winner read from the volition-inspection channel; then the
  formation-barrier line (settled/expected, or a prominent timeout warning), the structured-partial
  warnings, and the rendered verdict. Non-zero exit on a failing verdict.
- The run-id shape is `<UTC yyyyMMdd-HHmmss>` with a numeric suffix on collision, generated by the
  launcher and passed through so the directory name, console output, and manifest agree; the runner
  generates one when the flag is absent. **Stale run dirs are not pruned** — runs are evidence and
  `state/` is gitignored (see Open Questions).

**Verification (automated)**

- `cargo build`.
- Reducer tests: a clean run reaches a passing verdict; attach timeout, turn timeout, sideband
  termination, a non-promotable exchange, `degradation_epoch > 0`, and a promoted-count shortfall
  each produce their specific failing clause; a formation timeout and formation failures produce
  structured clauses with the verdict still `passed`.
- **Effect-layer finalizer tests:** an attach-timeout run and a turn-timeout run each assert that the
  sideband was stopped and joined and that a parseable `run-manifest.json` with `status: failed` and
  the specific clause remains on disk; a run whose manifest directory is made unwritable asserts the
  original failure is still reported and the exit code is non-zero.
- Trace-contract parser tests over a checked-in miniature diagnostics ledger fixture: a complete turn
  passes; a ledger missing `volition_context_injected` or with a mismatched `request_hash` fails with
  the turn identified.
- Secret-scan tests: a file containing the key is detected; a file containing only a hash is not.
- Manifest tests: field presence, stable serialization, atomic replacement, absent `--git-commit`
  serializing null without affecting the verdict.
- Script-loading tests: bundled-name resolution against the real fixture root, path resolution,
  unknown-name error text, malformed-document error text, and rejection of a phrase whose `expected`
  block omits the tagged winner.
- CLI parse tests mirroring the existing `sleep`/`ingest-world` patterns: no subcommand still serves;
  `probe` defaults, `--seed-only`, and `--structure-only` resolve as documented.
- `cargo test -p qsf_realtime_server` green; `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.

**Operator / human testing required (paid)**: first end-to-end live run with the two-phrase smoke
set — `cargo run -p qsf_realtime_server -- probe --phrase-set smoke` with `OPENAI_API_KEY` and
`QSF_MODEL_PROVIDER=openai` set. Cost: **one session, two turns**, plus roughly two off-hot-path
live-goal-formation model calls. Evidence to collect: the run dir has the expected tree shape;
`run-manifest.json` status is `passed`; the formation clause shows `settled == expected`; the
persisted `volition-state.json` reflects the **last** turn's formation outcome;
`cargo run -p qsf_app -- transcript --state-dir state/probe/<run-id> --full` reports
`source.complete == true` with two turn lines; the diagnostics file contains no base64 audio and is
of a sane size.

---

## Phase 6 — The fixture bundle: designed phrase script and warm-start seed state

Offline design and tests, then one paid full-script run.

**Bundle location.** `docs/Experiments/Fixtures/realtime-probe/`. Justified by precedent: that
folder's own README states it exists for "repeatable inputs or reference state for experiments and
QA … durable examples that should survive checkout", it already holds a curated *continuity bundle*
(`memory-association-browser-reference/`, with `memory-store.json` and `continuity-manifest.json`),
and the launcher already passes a fixture from there to a runtime command
(`docs/Experiments/Fixtures/session-memory.empty.json`). A crate `tests/fixtures/` tree would be the
wrong home: this bundle is consumed by the runtime on operator command, not only by tests. Paths
resolve relative to the working directory, which the launcher always sets to the project root
(`Invoke-LoggedCommand`), and an unresolvable path errors with the resolved absolute path.

**Bundle contents**, with the phase in which each file arrives:

```text
docs/Experiments/Fixtures/realtime-probe/README.md                             Phase 5 stub, Phase 9 full
docs/Experiments/Fixtures/realtime-probe/smoke.phrases.json                    Phase 5
docs/Experiments/Fixtures/realtime-probe/designed.phrases.json                 Phase 6 (becomes the default)
docs/Experiments/Fixtures/realtime-probe/seed/memory-store.seed.json           Phase 6
docs/Experiments/Fixtures/realtime-probe/seed/volition-state.json              Phase 6
docs/Experiments/Fixtures/realtime-probe/seed/continuity-manifest.json         Phase 6
docs/Experiments/Fixtures/realtime-probe/artifact-structure.reference.json     Phase 8
docs/Experiments/Fixtures/realtime-probe/artifact-structure.accepted-gaps.json Phase 8
```

**Seeding behavior**

- `memory-store.seed.json` is a **template**: each record carries `created_days_ago` and, where
  relevant, `last_reinforced_days_ago`, instead of absolute timestamps. A pure seeding function
  materializes `<dir>/continuity/default/memory-store.json` against an injected `now`. This is what
  keeps the bundle retrievable indefinitely: retrieval scores recency from `last_reinforced_at`,
  falling back to `created_at` only when it is unset, against an effective half-life that defaults to
  30 days for first-party records (Corrections item 16). Fixed absolute timestamps would decay out of
  usefulness over months.
- Every seed record sets `provenance` and `trust_tier` **explicitly** (`FirstPartyInternal` /
  `Trusted`) rather than relying on serde defaults, because those fields feed world-memory
  consolidation and supersession. They are also coupled to decay: pinning provenance to
  `FirstPartyInternal` is what keeps the 30-day default half-life in force, since
  `WorldObservationExternal` records fall back to 7 days, and no seed record sets a per-record
  `time_sensitive_decay_half_life_days` override (Corrections item 16).
- `volition-state.json` is a genuine `VolitionContinuitySnapshot` (schema version 4,
  `seed_fixture_id: "realtime_seed_fixture"`), copied into the target dir with `recorded_at` and
  `qsf_session_id` rewritten at seed time. It must satisfy
  `crate::realtime::volition::snapshot_is_fixture_compatible` — every `Accepted` goal of
  `realtime_seed_fixture()` present in `snapshot.goals` — otherwise `create_session` discards it and
  writes a `VolitionContinuityNote`. Its tick is greater than zero with plausible per-goal
  `salience`, `last_activated_tick`, and `admitted_tick`, and **no goal on cooldown at the seed
  tick**, so the phrase expectations hold.
- `continuity-manifest.json` is seeded so the target dir resolves through
  `resolve_continuity_session_dir` even for a zero-turn failed run; it is overwritten on the first
  promotion and again by the finalizer.
- **`session-state.json` is deliberately not seeded** — Corrections item 2.
- `--cold-start` skips seeding entirely, preserving the empty-seed path. Warm is the default, so the
  default exercises the compatible-snapshot restore path that no automated run covers today.
- The same materializer backs `probe --seed-only <dir>`, which Phase 8 uses to give the manual
  structural-reference session an identical starting store.

**The designed phrase script**

One coherent conversation with a wholly **invented** persona — a person who has just started at a
logistics company. It carries no real personal data and must never be edited to carry any; that
coupling is what makes the hand-freeze convention safe. Strengths are computed against
`realtime_seed_fixture()` (`fixture.rs:88-276`) with Weak = 1, Normal = 4, Strong = 8 and the
qualification threshold 4. Winners follow the arbitration sort key
`(biased_tier asc, base_priority desc, goal_id asc)`
(`crates/qsf_volition/src/arbitration.rs:343-349`) — matching strength does **not** break a
same-tier tie.

| # | Phrase | Qualifies (strength) | Expected winner → ordered losers | What it targets |
|---|---|---|---|---|
| 1 | "I've just started a new job at a logistics company, and my main project is the automation of warehouse scheduling." | `track-the-ai-transition` 12 (`job`,`automation`); `learn-what-drives-this-person` 10 (`i`,`my`,`job`,`project`) | **`track-the-ai-transition`** → `learn-what-drives-this-person` | Opens both the person thread and the AI-transition thread. Both are tier 5, so base priority decides: 94 beats 92. Uses `automation`, the exact fixture term — `automating` would not match, since activation has no stemming. |
| 2 | "Half the planners there are worried that AI will replace the jobs they have within a couple of years." | `track-the-ai-transition` 16 (`ai`,`replace`,`jobs`); below threshold: `respect-persons-boundaries` 1 (`they`) | **`track-the-ai-transition`** → none | The three distinct non-Weak matches select `ProposeExperiment`, not `ConsultWorld`, and the turn also carries a real below-threshold record. `they` is the fixture term (`their` would not match), and a lone Weak hit deliberately stays under the threshold. |
| 3 | "Hang on — is that something you actually read somewhere, or is it a guess? What evidence would prove it either way?" | `keep-theses-distinct-from-fact` 17 (`actually`,`evidence`,`prove`); below threshold: `serve-the-present-person` 1 (`what`) | **`keep-theses-distinct-from-fact`** → none | The two Strong epistemic keywords. Protected-tier winner, surfaced only on a genuine opportunity signal. |
| 4 | "There's something else. A colleague of mine is going through a divorce — it's personal, and she hasn't told me herself, so I don't want to pry." | `respect-persons-boundaries` 9 (`colleague`,`personal`,`she`); below threshold: `learn-what-drives-this-person` 2, then `serve-the-present-person` 1 | **`respect-persons-boundaries`** → none | The explicit boundary decline. |
| 5 | "Enough about AI and the economy — what I really want is help figuring out my own next step." | `serve-the-present-person` 6 (`what`,`want`,`help`); `track-the-ai-transition` 16 (`ai`,`economy`); below threshold: `learn-what-drives-this-person` 2, then `keep-theses-distinct-from-fact` 1 | **`serve-the-present-person`** → `track-the-ai-transition` | The tier-3-beats-tier-5 arbitration probe. `help` (Normal, 4) is what lifts the service goal over the threshold at all (Corrections item 1). Reproduces the "generic service goal crowds out the topical goal" pattern recorded in `Experiment.WorldConsultation.md`. |
| 6 | "Could you keep an eye on how my sleep is affecting my focus? I'd like us to notice that pattern over time." | `grow-the-library` 8 (`notice`,`pattern`); below threshold: `learn-what-drives-this-person` 2, then `serve-the-present-person` 1 | **`grow-the-library`** → none | Uses `notice`, the fixture term (`noticed` would not match). `RetrieveContext` is an allowed effect, so a context-retrieval hint may be stashed for the next turn. |
| 7 | "Can you remember that thesis you had about me putting things off? I think it happened again this week." | `grow-the-library` 12 (`remember`,`thesis`); below threshold: `learn-what-drives-this-person` 2, then `serve-the-present-person` 1 | **`grow-the-library`** → none | Memory recall against the seeded procrastination record; consumes any turn-6 retrieval hint; a repeat winner exercises the anti-nag suppression path when turn 6 surfaced. The stoplisted initial `Can` prevents the capitalization detector from treating this as a world-topic query. |
| 8 | "Can you find the latest on Grok, since the planners keep bringing it up?" | nothing qualifies (`can` 1) | none — explicit-topic path, consultation **expected** | Capitalized-entity world consultation. Cues `find`/`latest`; anchor `grok`; the bundled fixture corpus contains a Grok article, so the default run injects a real match. |
| 9 | "Can you find the latest on grok, since the planners keep bringing it up?" | nothing qualifies | none — consultation **not expected** | The STT-lowercased twin. `Can` is stoplisted, `grok` is lowercase, and there is no dotted version, so the explicit-topic detector returns `None` (Corrections item 7). Byte-identical to turn 8 except one capital letter. |
| 10 | "Never mind — you got it the first time. What are you focused on right now, and what's pulling at your attention?" | nothing qualifies; below threshold: `serve-the-present-person` 1 (`what`) | none — `below_qualification_threshold` | The read-only tool-loop turn: the session instructions direct `inspect_volition_state` for current-focus questions. Also the deliberate no-qualifier suppression class. |
| 11 | "So where is all of this heading for society — the whole world, not just warehouses?" | `assemble-world-picture` 8 (`society`,`world`) | **`assemble-world-picture`** → none | `heading` matches nothing; `world` and `society` are the fixture terms. Exercises the subconscious reduced-ambient-exposure path and the script's goal-activation `ConsultWorld`. |
| 12 | "That's a lot to sit with. I need to plan my next month around it — can you help me name the one thing to learn first?" | `serve-the-present-person` 6 (`need`,`can`,`help`); `learn-what-drives-this-person` 7 (`i`,`my`,`me`,`plan`) | **`serve-the-present-person`** → `learn-what-drives-this-person` | Natural close and the landing turn for a deferred turn-11 consultation. Note `learn` does **not** match `grow-the-library`'s `learned`, so that goal stays out of the contest. |

Notes carried into the fixture README:

- **Runtime volition mode never changes.** Nothing in the server emits `ModeChanged`, so mode
  switching is not a probe target and the script does not design for it.
- **The near-verbatim turn 8 / turn 9 pair is deliberate instrumentation**, not a natural quirk: the
  pair *is* the measurement, and it must stay byte-identical except for the entity's capitalization.
  Turn 10 restores conversational flow.
- **Probe runs are not evidence about the spoken world-perception trigger.** Typed turns preserve
  capitalization unconditionally, so turn 8 fires consultation every run. Spoken input is not
  comparable: the documented blocker is that STT often renders topics in lowercase and defeats the
  capitalization-based entity check, though at least one recorded session had STT *preserve*
  capitalization on a proper name — so the spoken path's behavior is uncertain in both directions
  (`docs/Handoff.md`; `Experiment.WorldConsultation.md` Results). A probe run cannot settle it, and
  probe artifacts must not be used to close `docs/Plans/Plan.WorldPerception.md`.
- **`memory-store.json` is not modified by a probe run** — the realtime server only reads it. It
  changes only if the operator runs a follow-on `sleep` over the run dir.

**Work — corrections carried over from the smoke run (2026-07-31)**

All three surfaced in the first live probe run and belong here, where the designed script, its
expectation blocks, and the corpus-dependent world-consultation turn land together.

- **Render the expectation diff by turn and difference, not by bare index.** The run printed
  `structured partial: expectation_diff:0`, where `0` is the exchange index of the only differing
  turn — which reads as "zero differences" and is exactly backwards. Over twelve turns the current
  form is a row of bare indices indistinguishable from counts. Render the turn number and the
  difference text, and render an empty failing-clause list as an explicit "none" rather than the
  trailing empty value in `failures: `.
- **Record how the world corpus was resolved, not only that it ended `ready`.** `resolve_corpus_path`
  falls back to the bundled fixture when a configured `QSF_WORLD_CORPUS_PATH` is absent *or*
  unusable, carrying a `CorpusPathSource` and a `degraded_reason`
  (`crates/qsf_corpus/src/config.rs:6-46`). The smoke run was launched with that variable set to an
  empty string, so the configured path was rejected and the bundled fixture silently took over,
  while the manifest recorded `world_corpus.state: "ready"` with no trace of the fallback. Since
  turn 8's expected consultation depends on the corpus actually containing the Grok article, the
  manifest must carry the resolution source and any degradation reason alongside the state.
  Related provenance hazard worth stating in the fixture README: `bundled_fixture_corpus_path()`
  bakes `CARGO_MANIFEST_DIR` at compile time, so a binary built in a different working tree resolves
  its corpus outside the repository.
- **Correct the smoke set's `expected` block.** Its first phrase puts `learn-what-drives-this-person`
  below the qualification threshold, which the fixture does not declare; the probe reported the
  difference and correctly did not fail the run. The phrase-design hard gate below is what keeps the
  designed script from carrying the same inaccuracy, and it should cover the smoke set too.

**Verification (automated)**

- `cargo build`.
- **Phrase-design hard gate.** For every phrase, load the seeded volition state and
  `realtime_seed_fixture()`, run `select_goals_ranked` + `arbitrate_with_mode`, and assert the
  qualifying goal ids, the **exact winner id**, the **ordered loser ids**, and the below-threshold
  goal ids match the fixture's `expected` block. Asserting the loser list is what stops fixture drift
  from collapsing a contest into a single qualifying goal while leaving the winner assertion green.
  Also assert, per phrase, whether the explicit-topic detector returns `Some`/`None`, and that turns
  8 and 9 differ only by the capitalization of the entity. No API access needed.
- Seed-materialization tests: materializing at an injected `now` produces records whose ages match
  the declared offsets, asserted on the **`last_reinforced_at`-derived age** for records that carry
  one (that is the timestamp decay actually reads) and on `created_at` for those that do not;
  retrieving with the turn-7 phrase through `retrieve_memories` (`AssociationWeighted`, the
  sideband's strategy) ranks the intended procrastination record ahead of its associated logistics
  context and asserts the full two-record order. One render uses `OffsetDateTime::now_utc()` so
  retrieval's ambient clock exercises real recency decay; a future render preserves that order.
- Snapshot tests: the seeded `volition-state.json` loads through
  `VolitionContinuitySnapshot::load_or_upgrade` and passes `snapshot_is_fixture_compatible`; a
  deliberately incompatible variant is *discarded with a `VolitionContinuityNote`* and does not
  panic; a malformed file likewise produces a note. The checked-in `inspection` is pinned to
  `build_state_inspection(&state, &fixture)`.
- Seed schema and persistence tests: the continuity template is deserialized as a typed
  `ContinuityManifest` and rejects unknown schema versions or resume modes; materialization uses the
  memory, volition, and manifest types' atomic persistence helpers. A failed warm materialization is
  recorded as failed seed provenance rather than claiming a successful warm start.
- Provenance test: every seed record sets `provenance` and `trust_tier` explicitly (parse the raw
  JSON and assert the keys are present, so a future serde default cannot silently take over), and no
  record sets `time_sensitive_decay_half_life_days`.
- Rendering tests for the carried-over corrections: an expectation diff names its turn and its
  difference text and cannot be mistaken for a count; a run with no failing clauses renders an
  explicit "none".
- Manifest test: a corpus resolved from the bundled fixture after an unusable configured path records
  the fallback source and the degradation reason, not merely `state: "ready"`.
- The phrase-design hard gate covers the smoke set as well as the designed script, so neither
  fixture's `expected` block can drift from what the selector actually produces.
- Diagnostics expectation tests separate the offline explicit-topic verdict from live consultation
  effects, match the authoritative trigger and anchors across deferred landing turns, and report an
  unexpected goal-activation lookup without shifting the expected matches.
- `cargo test -p qsf_realtime_server -p qsf_memory -p qsf_volition` green;
  `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.

**Operator / human testing required (paid)**: the first full-script run,
`cargo run -p qsf_realtime_server -- probe`. Cost: **one session, twelve turns** — at least twelve
realtime responses (the tool-loop turn can add up to three more provider responses before the spoken
answer) plus roughly twelve off-hot-path goal-formation calls. Audio output tokens dominate; this is
the most expensive step in the plan. Evidence to collect: status `passed`; the expectation diff empty
or explained; across the turn-8/9 pair there is exactly one `world_consultation_performed` record
with trigger `explicit_current_topic` and required anchor `grok` (its exchange index may be 8 or 9
because an over-budget lookup defers); turn 7's turn context contains the seeded memory; the tool-loop turn
shows a `ToolExecutionRecord` for `inspect_volition_state`; the formation clause shows
`settled == expected`.

**Live goal formation and expectations.** Live goal formation can admit new goals mid-run, which
legitimately changes later turns' activation. Therefore the offline phrase-design test is the hard
gate, while the live run reports per-turn expectation differences as a **diff recorded in the
manifest**, never as a verdict failure. The verdict's failing clauses stay strictly deterministic.

**Status: COMPLETE (2026-07-31).**

**What was done**

- Added the designed synthetic twelve-turn phrase set, corrected smoke expectations, and the
  checked-in relative-time continuity seed bundle. The default probe now uses the designed set and
  warm materialization; `--cold-start` skips it and `--seed-only` materializes the same bundle.
- Added the warm-state phrase gate, memory-age and retrieval checks, snapshot compatibility and
  degradation coverage, explicit seed provenance checks, plus the capitalization control pair.
- Corrected expectation-diff and empty-clause rendering, and carried corpus-resolution source and
  fallback degradation provenance into run manifests.
- Split the offline explicit-topic declaration from live world-consultation expectations. Live
  comparison now uses the authoritative diagnostics trigger and anchors across the full run, so
  deferred records do not invert the capitalization control and the goal-activation consultation is
  declared independently.
- Validated and atomically persisted typed seed artifacts, strengthened association-weighted
  retrieval into a two-record contest under the ambient clock, pinned snapshot inspection, and made
  failed seed materialization explicit in manifest provenance.
- The consultation trigger recorded on a world-consultation trace is **optional**. Introducing it as
  a required field made every previously written ledger unreadable — `transcript` reported the
  affected lines as skipped, which would have broken both the `source.complete` acceptance criterion
  and the structural-reference work that parses an earlier run. Diagnostics artifacts are sealed and
  never migrated in place, so absence is preserved rather than backfilled with a guess, and a
  regression test pins that a trigger-less trace still deserializes.

**Offline verification**

- Offline selector, diagnostics-expectation, seed-materialization, retrieval, snapshot, rendering,
  seed/corpus-provenance, and typed-persistence tests run without OpenAI credentials or network
  access.
- `cargo build`; `cargo test --workspace` (0 failures; the two paid live probes stay `#[ignore]`d);
  `cargo clippy --all-targets -- -D warnings`; `cargo fmt --check`.

**Phrase-table corrections.** The table above is hand-computed, and running it against the real
selector was its first check. Eight rows were wrong — the smoke set's first phrase and the designed
script's below-threshold lists for turns 3–7 and 10, plus empty lists for turns 11–12 — and the
table has been corrected to match the shipped fixture. One phrase was also reworded: the
memory-recall turn originally opened with `Remember`, a capitalized non-stoplisted word that acted
as a world-consultation anchor while `happened` supplied the current-information cue, so the turn
fired an external lookup instead of staying a recall probe. It now opens with the already-stoplisted
`Can`, which is the remedy Corrections item 7 anticipated.

**Operator follow-up — the full-script live run was performed 2026-08-01, verdict `passed`.**

`cargo run -p qsf_realtime_server -- probe`, twelve turns, run directory `state/probe/20260801-064555`.
A two-turn smoke run (`state/probe/20260801-061721`) preceded it to exercise the warm-seed path
live for a sixth of the cost. Evidence collected:

- **The capitalization control resolved cleanly.** Exchange 7 (turn 8, `Grok`) recorded exactly one
  `world_consultation_performed` with trigger `explicit_current_topic`, required anchor `grok`, and
  one surfaced fact; exchange 8 (turn 9, `grok`) recorded none. The pair differs by one capital
  letter, so this is the designed measurement landing as intended. Turn 11's goal-activation
  consultation was recorded separately with anchors `world`, `society` and no surfaced facts.
- **The warm seed was consumed, not discarded.** Both runs wrote the continuity note `restored
  volition state from continuity snapshot (tick=42)`, so the seeded snapshot passed
  `snapshot_is_fixture_compatible`. This is the check that distinguishes a genuine warm start from a
  silent cold start behind a passing verdict.
- **The corpus-resolution provenance fix caught a real degradation on its first live use.** The
  smoke run was launched with `QSF_WORLD_CORPUS_PATH` set to an empty string; the manifest recorded
  source `bundled_fixture_after_missing_configured_path` with the degradation reason, where the
  earlier run of the smoke set had recorded only `state: "ready"`.
- **Detached formation results reached disk.** The end-of-run volition state holds three live-formed
  goals (`help-clarify-users-next-step` active, `track-sleep-focus-pattern-over-time` accepted,
  `understand-warehouse-scheduling-automation-project` retired) at tick 54 from a seed tick of 42 —
  the outcome the drain barrier and the finalizer's explicit snapshot exist to guarantee. One
  formation result took the stale-goal-set path (`the goal set changed during formation`) and
  counted as settled rather than failed; the barrier reported 12/12 settled, 0 failed.
- **One expectation difference, at turn 12**, reporting `help-clarify-users-next-step` as an extra
  qualifying goal and loser. It was formed live at turn 5 from the user's redirection toward their
  own next step and admitted at tick 47, so this is the designed non-failing behavior, not fixture
  drift. It must not be encoded into the fixture: the goal is absent from the seeded state, so the
  offline gate would fail, and it does not reproduce across runs.
- Turn 7's model-visible context carried the seeded procrastination memory, and the turn also
  invoked `search_memory`. The tool-loop turn (turn 10) recorded one `inspect_volition_state`
  request and one execution.
- `transcript --state-dir state/probe/20260801-064555 --full` reported `source.complete == true`,
  twelve turn lines, zero skipped lines, and zero orphans; `goals` printed a non-empty listing
  including the live-formed goals.

**Observation for separate work, outside this plan.** Live-formed goals carry no parent tension, so
they resolve to effective tier 255 and sort last in arbitration regardless of base priority —
`help-clarify-users-next-step` holds `base_priority: 220`, more than double any fixture goal, and
still lost to a tier-3 and a tier-5 goal. A goal the simulation forms about the person in front of
it therefore cannot win a turn under the current tier assignment. This may be the intended
conservatism for unreviewed goals; the run is simply the first concrete evidence of it.

---

## Phase 7 — Launcher command, `sleep -NoBackup`, `realtime -StateDir`, completion, and Pester

Offline.

**Work — `scripts/qsf.ps1`**

- New parameters: `[string]$PhraseSet`, `[switch]$ColdStart`, `[int]$TurnDelayMs`, and
  `[switch]$NoBackup`.
- `Invoke-Probe`: requires `OPENAI_API_KEY` via `Test-RequiredSecret` (never printing it), computes
  the run id, resolves the state dir to `state/probe/<run-id>` unless the operator passed
  `-StateDir` (detected through the existing `$script:QsfScriptBoundParameters`), computes the git
  commit with `git rev-parse HEAD` when git is available and passes it through, and runs
  `cargo run -p qsf_realtime_server -- probe …` through `Invoke-LoggedCommand`.
- `Get-ProbeEnvironmentDelta`, modeled on `Get-RealtimeEnvironmentDelta` per the decision
  *"`realtime` launcher manages the server environment and pins `QSF_MODEL_PROVIDER=openai`"*: sets
  `QSF_MODEL_PROVIDER=openai`, sets `QSF_WORLD_CORPUS_PATH` when `-WorldCorpusPath` is supplied, and
  clears every other managed `QSF_*` variable. The probe is inherently OpenAI-backed, and the live
  goal-formation judge silently no-ops against the mock client when the provider is unset — the
  exact failure that motivated the realtime entry.
- `probe` dispatch arm added to the command `switch`, kept a one-line delegation.
- **`realtime` passes `-StateDir` through to the server.** Forward `--state-dir $StateDir` (default
  `state/realtime`, so behavior is unchanged by default) alongside the existing optional
  `--random-session-id`, and print the resolved state dir in `Invoke-Realtime`. The server CLI
  already accepts the flag with the same default (`crates/qsf_realtime_server/src/cli.rs:16-17`), so
  this is a pure launcher change with no server work. It is what makes an isolated typed structural
  reference possible in Phase 8, given the append-mode diagnostics ledger (Corrections item 15).
- `sleep -NoBackup`: skip `New-QsfStateBackup` and print `State backup: skipped (-NoBackup)`. Default
  backup behavior for normal `sleep` is unchanged. Reason: `New-QsfStateBackup` prunes per state-dir
  leaf (`"$leaf-*"`, keep 5) and `Show-QsfStateBackups` lists every directory under `state/backups`
  with no leaf filter, so a unique run-id leaf per probe run means probe backups accumulate forever
  and crowd the bare `restore` listing. The documented probe follow-on uses `-NoBackup`.
- Help text and examples for `probe`, `sleep -NoBackup`, and `realtime -StateDir`.

**Work — `scripts/qsf-completion.ps1`**

- `probe` added to `$script:QsfCompletionCommands`.
- A `probe` flag list (`-PhraseSet`, `-StateDir`, `-WorldCorpusPath`, `-ColdStart`, `-TurnDelayMs`)
  and a `sleep` flag list (`-StateDir`, `-Provider`, `-WorldCorpusLedger`, `-NoBackup`), following
  the existing `goals`/`transcript` flag-list pattern.
- `-PhraseSet` value completion offering the bundled phrase-set names plus discovered
  `*.phrases.json` paths.
- **Fix state-dir discovery.** `Get-QsfCompletionStateDirs` enumerates only the immediate children of
  `state/`, so it offers `state/probe` rather than the run directories that `transcript`, `goals`,
  and `sleep` actually accept (Corrections item 13). Extend it by one **bounded** extra level for
  `state/probe/*` while continuing to exclude `state/backups`.

**Verification (automated)**

- `Invoke-Pester scripts/qsf.Tests.ps1 -Output Detailed` with:
  - a new `Describe "qsf.ps1 probe launcher"`: default state dir is a `state/probe/<run-id>` path; an
    explicit `-StateDir` wins; `-PhraseSet`, `-ColdStart`, and `-TurnDelayMs` map to the expected
    cargo arguments; `Get-ProbeEnvironmentDelta` sets `QSF_MODEL_PROVIDER=openai`, sets
    `QSF_WORLD_CORPUS_PATH` only when supplied, and clears the other managed variables;
  - **new argument-list coverage for `Start-RealtimeServerProcess`** — none exists today
    (Corrections item 17) — asserting the default command line and the `--state-dir` passthrough,
    with and without `--random-session-id`;
  - an extension of the existing `sleep` `Describe`: `-NoBackup` skips the backup and prints the skip
    line; the default still backs up.
- `Invoke-Pester scripts/qsf-completion.Tests.ps1 -Output Detailed` with coverage for the `probe`
  command, its flags, `-PhraseSet` values, the `sleep -NoBackup` flag, and — using **two concrete
  probe run directories** under a `TestDrive` `state/probe/` — an assertion that the individual run
  directories are offered, not only their parent, while `state/backups` stays excluded.
- `cargo build`; `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.

**Human testing (recommended, unpaid)**: `.\scripts\qsf.ps1 help`, tab completion for
`.\scripts\qsf.ps1 probe -` and for `-StateDir` after a probe run exists, and a
`.\scripts\qsf.ps1 probe -PhraseSet smoke` invocation reviewed for the printed environment delta and
command line before the paid run.

---

## Phase 8 — Making the equivalence claim falsifiable

"Filesystem result equivalent to a manual typed-turn realtime session" is the acceptance criterion,
and today there is neither a reference to diff against nor a field-level definition of "equivalent".
This phase supplies both.

**Work**

- New pure module `crates/qsf_realtime_server/src/scripted_conversation/artifact_structure.rs`:
  `build_artifact_structure(run_dir, exchange_range) -> ArtifactStructure`, recording
  **`(field path, JSON type)` pairs including array element shapes** — not paths alone, because a
  string-to-object schema regression preserves the path while breaking readers. Covered:
  - `diagnostics/*.jsonl`: the set of record `kind`s and, per kind, the typed field-path set;
  - `continuity/default/session-state.json`, `volition-state.json`, `continuity-manifest.json`, and
    **`memory-store.json`**: typed field-path sets, including per-turn, per-exchange, and per-record
    element shapes;
  - **required-file presence**: the set of files that must exist under a conforming run dir.
  Kinds, paths, and JSON types only — never values.
- **Contamination guard.** The builder refuses to produce a *reference* from a diagnostics ledger
  containing more than one `session_allocated` record. The ledger is opened in append mode
  (Corrections item 15), so a reused `default`-id state dir accumulates earlier sessions and would
  let historical or voice-only record kinds silently define the reference. The builder also takes an
  explicit exchange-index range and records it.
- `compare_artifact_structure(reference, observed, accepted_gaps) -> StructuralComparison`: every
  required file, record kind, and typed field path in the reference must be present in the observed
  run unless it appears in the explicit `accepted_gaps` document; a type mismatch on a shared path is
  a failure; kinds or fields present only in the observed run are reported as additive, not fatal.
  The comparison result is a **failing** verdict clause.
- `artifact-structure.accepted-gaps.json` is checked in beside the reference and is exactly the
  fidelity-gap list from "Settled design" item 6, so any *new* divergence fails loudly while the
  known ones live in one machine-readable place. It must record `call_invalidated` as expected in the
  reference but absent from a probe run **because a model-scoped session has no `call_binding` for
  the stop path to invalidate** — while `call_bound` and `sdp_rendezvous` are absent for the separate
  reason that the SDP route never runs (Corrections item 4).
- `artifact-structure.reference.json` carries a provenance header: capture date, git commit, the
  **isolated state directory**, **input modality (typed only)**, session id, and the exact exchange
  index range.
- `probe --structure-only <dir>` produces the document from an existing run without a live call, and
  `probe --seed-only <dir>` prepares the reference directory with the same seed bundle the probe uses
  so the manual session retrieves memory the same way (without it a fresh reference dir would have no
  `memory-store.json` at all, since the realtime server only ever reads that file).

**Reference capture procedure (isolated, typed-only)**

```powershell
cargo run -p qsf_realtime_server -- probe --seed-only state/reference-typed
.\scripts\qsf.ps1 realtime -StateDir state\reference-typed     # type four turns; microphone OFF
cargo run -p qsf_realtime_server -- probe --structure-only state/reference-typed
```

The directory must not previously exist. The operator types four turns into the browser UI and does
not enable the microphone.

**Relationship to the outstanding manual acceptance run in `docs/Handoff.md`.** The current *Now*
item is a live four-or-more-turn **voice** conversation followed by `transcript` acceptance. This
probe **does not supersede it**: the probe has no voice input, so it cannot exercise the STT path,
`ignored_continuation_transcript`, interruptions, or the `input_transcription` token class. Nor is
this plan gated by it. The two sessions must **not** be combined into one browser session, tempting
though it is: a mixed spoken-then-typed session contains voice-only event shapes that cannot
establish typed-turn equivalence, and both would share one append-mode ledger. The structural
reference is its own short, isolated, typed-only session. If reuse ever becomes important it would
need an explicit extractor selecting the typed exchange range and its linked diagnostics — out of
scope here.

**Verification (automated)**

- Structure-builder tests over checked-in miniature run trees: kinds, typed paths, and required files
  are extracted; values never appear in the output; a run missing a kind or a required file fails
  comparison; a run with an extra kind passes with an additive note; a gap listed in
  `accepted_gaps` does not fail; **a scalar-to-object type change on an otherwise identical path
  fails**; **a ledger carrying unrelated pre-existing records (two `session_allocated` records) is
  rejected as a reference source**.
- `cargo test -p qsf_realtime_server` green; `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.

**Operator / human testing required (paid)**: the isolated typed reference capture above. Cost:
**one session, four typed turns**, plus roughly four goal-formation calls. Not combinable with the
voice acceptance run. The emitted document is reviewed and committed; then re-run the full-script
probe and confirm the structural-comparison clause passes.

**Full acceptance for the plan** (operator, after this phase):

```powershell
.\scripts\qsf.ps1 probe
.\scripts\qsf.ps1 transcript -StateDir state\probe\<run-id> -Full -Out state\probe\<run-id>\turns.jsonl
.\scripts\qsf.ps1 goals -StateDir state\probe\<run-id>
.\scripts\qsf.ps1 sleep -StateDir state\probe\<run-id> -NoBackup
```

Expected: manifest status `passed`; `turns.jsonl` reports `source.complete == true` with twelve turn
lines and no non-empty `undecodable`; `goals` prints a non-empty listing including any live-formed
goal (which the finalizer's explicit snapshot is what makes durable); `sleep` completes and reports
an itemized change view over the run dir without touching `state/realtime` or `state/backups`.

---

## Phase 9 — Documentation, decisions, and the corpus-home convention

**Documents to update** (checked against `ProjectWorkflow.md`'s Document Responsibilities):

- **`docs/Architecture/Architecture.RealtimeSessionServer.md`** — Implementation Status: the two
  sideband attach shapes and their **differing reconnect policies**; the headless scripted
  conversation run and its per-run artifact tree; the event-type-gated audio-payload suppression and
  the split first-audio/first-output-audio latency labels; the trusted-turn-completion channel, the
  `attached` flag, the monotonic degradation epoch, and the live-goal-formation drain barrier with
  its explicit end-of-run volition persistence. Boundary section: add the model-scoped attach and the
  in-process driver alongside the browser flow, and keep "Raw audio is not logged" accurate by naming
  the suppression. Refresh `Last reviewed:`.
- **`docs/Architecture/Architecture.StateAndObservability.md`** — the new `run-manifest.json`
  artifact and its fields; the latency-label semantics; and the fact that `volition-state.json` is
  now persisted at an explicit end-of-run boundary as well as inside promotion, so a detached
  formation result is durable. Refresh its Implementation Status and `Last reviewed:`.
- **`README.md`** — the `probe` command alongside `realtime`/`sleep`/`transcript`/`goals`, the
  documented follow-on (`transcript`/`goals`/`sleep -StateDir state/probe/<run-id>`), the
  `sleep -NoBackup` switch, and `realtime -StateDir`.
- **`docs/Experiments/Fixtures/README.md`** — add a bundle entry for `realtime-probe/`, and while
  there add the missing entry for `volition-seed.reviewed.draft.json`, which sits in that folder
  undocumented.
- **`docs/Experiments/Fixtures/realtime-probe/README.md`** — grow the Phase 5 stub into the full
  document: purpose, per-turn intent table with exact winners and losers, the fidelity-gap list, the
  "not evidence for the spoken world trigger" caveat, the invented-persona / no-real-personal-data
  rule, the deliberate turn-8/turn-9 pair, the isolated reference-capture procedure, and the
  hand-freeze convention.
- **`docs/ProjectFrame/ProjectWorkflow.md`** — line 67 cites `Plan.RealtimeVoiceConversation.md` as
  "the established pattern" for a phased plan validated by experiment scaffolds, but that file was
  deleted on 2026-07-05 (commit `9efd97c`), so the reference dangles. Repoint it to
  `Plan.WorldPerception.md`, which carries the same phased structure. Pre-existing, cheap, and inside
  this work's blast radius.
- **`docs/Handoff.md`** — update only if landing a phase changes a Now/Next/Horizon recommendation
  (pointer, not content). Note that the structural reference is a separate short typed session and
  does **not** fold into the voice acceptance run.
- **Do not** cite this plan's phase labels from any durable document; name the behavior. This plan is
  itself ephemeral and is deleted after the work lands, so every durable rule it produced must be in
  the decision log by then.

**Decision-log entries to add** (`docs/DecisionLog.md`):

1. **"Realtime sideband attaches either by browser call id or by server-owned model session, and
   only the browser call may reattach."** Extends *"Sideband uses the server-captured call_id
   websocket with bearer auth"* (2026-06-10) rather than replacing it. Records that both shapes are
   built from one source of truth in `qsf_realtime_protocol`, that the documented
   `OpenAI-Safety-Identifier` header is applied to both, and that a model-scoped websocket **is** the
   stateful provider session, so a post-attach disconnect fails closed instead of silently starting a
   second conversation while the local session state claims otherwise.
2. **"Raw provider audio payloads never reach the artifact plane, and first-audio latency is
   measured from the transcript delta."** Gated on event type so it holds for any future attach
   shape; preserves the architecture invariant and the meaning of the recorded
   transcript-to-first-audio envelope.
3. **"Trusted turn completion is published from the promotion path; detached side effects have their
   own drain barrier."** The existing per-session watch channels publish at injection time; only
   promotion knows a turn finalized and whether it was promoted; and neither is the end of a turn's
   side effects, because live goal formation runs detached afterwards. Records the paired rules: a
   run's terminal boundary waits on the formation barrier, and the end-of-run volition snapshot is
   persisted explicitly rather than depending on a later promotion. Refines *"Live goal formation and
   off-hot-path coherence…"* (2026-07-01) and *"Realtime per-turn injection disables automatic
   response creation"*.
4. **"Sideband degradation is recorded monotonically for the life of a session."** `degraded`
   continues to mean current health so recovery keeps working, but a never-cleared degradation epoch
   is what any correctness claim about a whole run is built from. Refines *"Sideband gaps degrade
   transport trust until verified recovery"*, which recovery-clears the current flag.
5. **"Headless scripted probe runs write a self-contained, self-describing run directory under
   `state/`, and always write a terminal manifest."** Covers the per-run state dir with session id
   `default` (from `resolve_continuity_session_dir` and *"Realtime voice uses a stable default
   session id"*), the fresh append-mode diagnostics ledger a fresh dir guarantees,
   `run-manifest.json` with `passed`/`failed`/`infrastructure_error`, the always-run finalizer, and
   the corpus-home convention: generated runs stay under the gitignored `state/` boundary, mirroring
   *"Durable evaluation artifacts live in the top-level evaluation tree"*. A run cited from an
   experiment or report is **copied by hand** into `evaluation/frozen/realtime-probe/<run-id>/` after
   the no-secret check. No freeze command is built. Safety coupling: the phrase script stays entirely
   synthetic.
6. **"`probe` is the first-class headless scripted-conversation launcher command; `sleep` accepts
   `-NoBackup` and `realtime` accepts `-StateDir`."** Mirrors *"`realtime` is the first-class
   live-conversation launcher command"* and *"`realtime` launcher manages the server environment and
   pins `QSF_MODEL_PROVIDER=openai`"*; records that probe backups would otherwise accumulate per
   unique run-id leaf and pollute the `restore` listing, that default `sleep` backup behavior is
   unchanged, and that a state-dir passthrough is what lets a manual session be captured in
   isolation from the append-mode ledger of earlier runs.
7. **"Probe artifacts are not evidence about the spoken world-perception trigger."** A durable rule
   preventing future misuse of the corpus: typed input always preserves capitalization, spoken input
   does so unreliably, and the corpus therefore says nothing about the spoken trigger path either
   way.

**Verification**

- Documentation review pass; no code change expected.
- `cargo clippy --all-targets -- -D warnings` then `cargo fmt` as the standard closing gates.

---

## Exit criteria (whole plan)

- One shared source of truth in `qsf_realtime_protocol` builds both realtime websocket URLs; the
  `qsf_app` duplicate literal is gone; the safety-identifier header is applied to both attach shapes.
- The sideband attaches by browser call id or by server-owned model session through one
  behavior-named attachment type carrying an explicit reconnect policy; a post-attach model-session
  disconnect fails closed, proven by an offline lifecycle test showing no later turn is produced or
  promoted.
- No persisted `provider_events` entry can carry an audio payload, pinned by a regression test; both
  first-audio latency labels keep their pre-existing meaning under both attach shapes and a distinct
  output-audio label exists.
- A trusted-turn-completion channel is published from the promotion path for every completed trusted
  exchange including skipped ones; `SidebandStatus` carries `attached`, a monotonic
  `degradation_epoch`, and a monotonic `terminated`; a degrade→recover sequence unobserved by the
  receiver still fails the verdict.
- A live-goal-formation drain barrier exists with no enqueue race, and the finalizer persists the
  end-of-run volition snapshot through the same helper promotion uses, so the last turn's formation
  result is durable.
- One idempotent finalizer runs on success and on every failure after run-directory creation, always
  leaving a parseable terminal `run-manifest.json` with `passed`/`failed`/`infrastructure_error`,
  with the sideband stopped and joined — proven by attach-timeout and turn-timeout effect tests.
- `qsf_realtime_server probe` runs a phrase script headless against the live API, gated on turn
  completion, prints per-turn progress and structured-partial warnings, and exits non-zero on a
  degraded corpus. Formation timeout/failure is an accepted structured partial, not a failure.
- A checked-in fixture bundle supplies the smoke set (from the phase that first needs it), the
  designed twelve-turn script with exact winners and ordered losers, and a warm-start seed with
  seed-time-relative timestamps, with an offline hard-gate test and a retrieval test that holds at
  any wall-clock date.
- `.\scripts\qsf.ps1 probe` exists with a managed environment delta, help, completion including
  `state/probe/<run-id>` discovery, and Pester coverage; `sleep -NoBackup` and `realtime -StateDir`
  exist with default behavior unchanged, and the realtime server's command line finally has
  argument-list coverage.
- A typed-only structural reference captured in a fresh isolated state directory, with a
  contamination guard, `(path, JSON type)` comparison, required-file presence, and
  `memory-store.json` included — plus an automated comparison that fails on any unlisted divergence.
- `transcript` reports `source.complete == true` over a probe run, `goals` is non-empty, and `sleep`
  succeeds over the run dir.
- Architecture, README, fixture READMEs, the stale `ProjectWorkflow.md` plan reference, and the
  decision log are updated; no durable document cites a phase number.

---

## Open Questions (surfaced, not silently resolved)

1. **Corpus home — recorded assumption, operator-confirmed.** Generated probe runs stay under the
   gitignored `state/` boundary; a run that gets cited is copied **by hand** into
   `evaluation/frozen/realtime-probe/<run-id>/` after the no-secret check, and this plan builds no
   freeze command. Listed so a reviewer can still flip it. The coupling that makes it safe: the
   phrase script must stay entirely synthetic.
2. **`sleep -NoBackup` — recorded assumption, operator-confirmed.** `sleep` gains `-NoBackup` and the
   documented probe follow-on uses it; default `sleep` behavior is unchanged.
3. **Post-attach disconnect and formation-timeout handling — recorded operator decisions.** A
   model-scoped session fails closed after its first successful attach (no conversation
   replay-and-verify), and a formation-barrier timeout or formation failure is an accepted structured
   partial rather than a corpus failure. Recorded here so they stay visible at the next review.
4. **Inter-turn pacing default.** Turns are gated on the completion signal and the run's terminal
   boundary is gated on the formation barrier, so the delay is purely conversational cadence and
   carries no synchronization meaning. `--turn-delay-ms` defaults to **250** rather than 0 so the
   pacing path is exercised by default (`Agents.md`) and the run is not a tight submit loop. Open:
   whether the operator wants a larger gap for realism.
5. **Formation-barrier timeout value.** `--formation-timeout-ms` defaults to **60000**, which should
   cover one queued formation call comfortably. If real runs show the queue is routinely deeper than
   one item at the end of a script, the default may need to scale with the remaining queue depth
   rather than being a flat wall-clock bound.
6. **Stale run-dir pruning.** The plan does not prune: runs are evidence, `state/` is gitignored, and
   silent deletion of a cited run would be worse than disk use. Open in case the operator wants a
   keep-N policy or a `probe -Prune` housekeeping switch.
7. **Whether the probe should offer to run `sleep` at the end.** Kept a separate operator step and
   documented as the follow-on, because `sleep` is a first-class command with its own provider,
   backup, and ledger flags, and folding it in would hide a second paid model call inside the probe
   run. Open if the operator prefers a one-command "run and consolidate".
8. **Whether the tool-loop turn reliably triggers a tool call.** The session instructions direct
   `inspect_volition_state` for current-focus questions, but tool invocation is model behavior, not a
   deterministic trigger. A missing tool execution on turn 10 is therefore an expectation-diff entry,
   never a verdict failure. Open: whether the operator wants stronger phrasing, or a second
   tool-seeking turn, if the first live run shows the tool loop is not reliably reached.
9. **Model-scoped session idle tolerance.** With fail-closed reconnect, a provider-side idle timeout
   would end a run rather than silently corrupting it — the safe failure — but it would still burn
   the turns already paid for. Phase 2 measures idle tolerance; if it is short relative to a
   twelve-turn script with a formation queue, the mitigations to weigh are a smaller
   `--turn-delay-ms`, splitting the script across runs, or a provider-supported keepalive. Flagged
   rather than designed for.
