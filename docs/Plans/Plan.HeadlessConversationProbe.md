# Plan: Headless scripted realtime conversation probe

Status: Proposed — not started
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
artifact_or_report_reference     request_hash linking turn_context_captured to the injection and
                                 initiative traces; run-manifest.json
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

Pure, offline, no API calls. Establishes the abstraction everything else hangs off, including the
fail-closed reconnect policy that Corrections item 11 makes necessary.

**Work**

- `crates/qsf_realtime_protocol/src/lib.rs`: add two builders next to `OPENAI_REALTIME_WS_BASE_URL`
  — one for the browser-call attach (`?call_id=`) and one for the server-owned model-scoped attach
  (`?model=`), each taking the base URL so tests can point at a local stub.
- `crates/qsf_realtime_server/src/state.rs`: `AppState::openai_realtime_ws_url` delegates to the
  shared call-id builder (it keeps owning the configurable base URL).
- `crates/qsf_app/src/audio/voice_session_provider.rs`: delete the private
  `OPENAI_REALTIME_VOICE_WEBSOCKET_BASE_URL` literal (line 21) and the inline `format!` at line 538;
  consume `OPENAI_REALTIME_WS_BASE_URL` plus the shared model builder. The base URL is currently
  duplicated as a private literal there, against the `Agents.md` one-source-of-truth rule.
- New behavior-named module `crates/qsf_realtime_server/src/realtime/sideband_attachment.rs`:

  ```text
  SidebandAttachment
    BrowserCall { call_id }            -> websocket_url = ?call_id=<call_id>
    ServerModelSession { model }       -> websocket_url = ?model=<model>

  SidebandAttachment::provider_id()    label written onto UtteranceRecord.provider_id
  SidebandAttachment::call_id()        Option<&str>; None for ServerModelSession, so model-attached
                                       ProviderEventRecords carry call_id: null, never a fake value
  SidebandAttachment::reconnect_policy()
    BrowserCall        -> ReattachToOwningCall
    ServerModelSession -> FailClosedAfterFirstAttach
  ```

  **Why the policy differs, stated in the module docs:** a `?model=` websocket *is* the stateful
  provider session, so reopening it creates a new session with an empty conversation while the local
  `SessionRuntime` still holds every earlier turn; `?call_id=` reattaches to the browser-owned call,
  whose conversation survives (Corrections item 11).
- `run_sideband` consults the policy:
  - Before the **first successful attach** (defined as receiving `session.updated`, the same event
    that sets the new `attached` flag), both policies retry with the existing backoff. This preserves
    today's behavior for `?call_id=`, where OpenAI returns `404 No session found for the provided
    call_id` until the browser's WebRTC handshake completes.
  - After the first successful attach, `FailClosedAfterFirstAttach` does **not** reconnect: it
    records the degradation (monotonic, Phase 4), sets a terminal `terminated` reason on the session
    status, and returns from the task. `ReattachToOwningCall` keeps today's loop.
- Thread the attachment through `SidebandHandle::spawn`, `run_sideband`, `connect_and_run_once`,
  `handle_text_turn`, `handle_provider_event`, `handle_response_done_event`, and
  `mark_session_degraded`, replacing the call-id parameter (which is `String` on the first two and
  `&str` on the rest). `exchange_sdp_impl` constructs `BrowserCall`. Log lines keep naming the
  concrete attachment and the policy decision so a failing operation stays identifiable
  (`Agents.md` logging rule).
- Apply the `OpenAI-Safety-Identifier` header to the websocket handshake request in
  `connect_and_run_once` for **both** attach shapes. The header is documented for server-to-server
  Realtime WebSocket connections and is absent from the attach today, so this is real work
  (Corrections item 14). Extract `hash_session_id` out of `routes.rs:1040-1051` into a shared helper
  so the SDP POST and the websocket attach use one implementation.

**Verification (automated)**

- `cargo build`.
- Unit tests: both URL builders produce the expected shapes from a given base; the `AppState` and
  `qsf_app` callers produce identical URLs to the shared builders for the same inputs (one source of
  truth, pinned); `provider_id()`, `call_id()`, and `reconnect_policy()` return the documented values
  for both variants.
- **Offline lifecycle test:** with a local websocket stub, a `ServerModelSession` attaches, completes
  one scripted turn, then the stub drops the connection; the test asserts that no reconnection is
  attempted, that the sideband task exits with a terminal reason, and that a subsequently submitted
  phrase produces **no** new exchange and **no** promoted turn. The mirror test for `BrowserCall`
  asserts the reconnect loop still runs.
- Existing `qsf_realtime_server` sideband tests (`sideband_tests.rs`, `sideband_lifecycle_tests.rs`,
  `sideband_status_tests.rs`, `sideband_promotion_tests.rs`, `sideband_tool_loop_tests.rs`,
  `sideband_volition_tests.rs`) must pass unchanged — the refactor is behavior-preserving for the
  browser-call path.
- `cargo test -p qsf_realtime_protocol -p qsf_realtime_server -p qsf_app` green.
- `cargo clippy --all-targets -- -D warnings` then `cargo fmt`.

**Human testing**: not required. **Cost**: none (no API calls).

---

## Phase 2 — Live model-scoped attach reconnaissance (first paid step, cheapest)

One real model-scoped session's provider-event stream must be captured **before** the suppression and
latency design in Phase 3 is locked, because Corrections item 5's fix depends on which event actually
arrives first.

**Work**

- New integration test `crates/qsf_realtime_server/tests/model_scoped_attach_smoke.rs`, marked
  `#[ignore]` so it never runs in a normal `cargo test`. It is a durable on-demand live smoke test,
  not throwaway code.
- The test builds the model-scoped URL via the Phase 1 builder, attaches with the bearer header and
  the safety identifier, sends the same `session.update` the sideband sends
  (`build_openai_realtime_conversation_session_update` with `output_modalities: ["audio"]`,
  `create_response: false`, `interrupt_response: false`, the default tool list and transcription
  model), sends one `conversation.item.create` + `response.create`, and reads until `response.done`.
- It writes an **event-shape inventory** artifact (not raw payloads) to a path given by an
  environment variable, defaulting under `state/`: for each observed event `type`, the count,
  first-seen offset in milliseconds from `response.created`, the set of top-level JSON keys, and for
  any key whose value is a long string, its byte length only. Payload text is never written.
- Observations the later phases depend on:
  1. Does `response.output_audio.delta` arrive, and does it carry base64 in `delta`?
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
- `cargo test -p qsf_realtime_server` (the ignored test does not run).
- **Operator / human testing required (paid):**
  `cargo test -p qsf_realtime_server --test model_scoped_attach_smoke -- --ignored --nocapture`
  with `OPENAI_API_KEY` set. Cost: **one realtime session, one turn, one short spoken response** —
  the cheapest live step in this plan. The operator records the event-shape inventory (or its path)
  in the phase's follow-up so the next phases are designed on evidence.

---

## Phase 3 — Raw audio payloads never enter the artifact plane; first-audio latency keeps its meaning

Offline, gated on Phase 2's evidence. This is the architecture-invariant phase
(`Architecture.RealtimeSessionServer.md`: "Raw audio is not logged").

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

---

## Phase 5 — Scripted conversation runner: `qsf_realtime_server probe`

The smallest viable end-to-end slice, with an always-run finalizer. It ships the phrase-set loader and
a minimal two-phrase smoke set so the first paid run is cheap and this phase can pass its own human
test; Phase 6 adds the designed script and the seed bundle and flips the default.

**Work — pure core first (input → action → reducer → state → render)**

New module tree `crates/qsf_realtime_server/src/scripted_conversation/` (named for stable behavior,
not for this plan), with `mod.rs` kept a thin re-export wrapper:

- `script.rs` — the phrase-set document: `id`, `description`, ordered phrases, and per-phrase
  `expected` metadata (expected qualifying goal ids, the **exact** expected arbitration winner id,
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
  block omits the winner id.
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
| 2 | "Half the planners there are worried that AI will replace the jobs they have within a couple of years." | `track-the-ai-transition` 16 (`ai`,`replace`,`jobs`); below threshold: `respect-persons-boundaries` 1 (`they`) | **`track-the-ai-transition`** → none | Goal-activation `ConsultWorld` path, and a real below-threshold record. `they` is the fixture term (`their` would not match), and a lone Weak hit deliberately stays under the threshold. |
| 3 | "Hang on — is that something you actually read somewhere, or is it a guess? What evidence would prove it either way?" | `keep-theses-distinct-from-fact` 17 (`actually`,`evidence`,`prove`) | **`keep-theses-distinct-from-fact`** → none | The two Strong epistemic keywords. Protected-tier winner, surfaced only on a genuine opportunity signal. Also the landing turn for a deferred turn-2 consultation. |
| 4 | "There's something else. A colleague of mine is going through a divorce — it's personal, and she hasn't told me herself, so I don't want to pry." | `respect-persons-boundaries` 9 (`colleague`,`personal`,`she`) | **`respect-persons-boundaries`** → none | The explicit boundary decline. Verified free of accidental qualifiers: `want` alone scores `serve-the-present-person` 1, and `I`/`me` score `learn-what-drives-this-person` only 2. |
| 5 | "Enough about AI and the economy — what I really want is help figuring out my own next step." | `serve-the-present-person` 6 (`what`,`want`,`help`); `track-the-ai-transition` 16 (`ai`,`economy`) | **`serve-the-present-person`** → `track-the-ai-transition` | The tier-3-beats-tier-5 arbitration probe. `help` (Normal, 4) is what lifts the service goal over the threshold at all (Corrections item 1). Reproduces the "generic service goal crowds out the topical goal" pattern recorded in `Experiment.WorldConsultation.md`. |
| 6 | "Could you keep an eye on how my sleep is affecting my focus? I'd like us to notice that pattern over time." | `grow-the-library` 8 (`notice`,`pattern`) | **`grow-the-library`** → none | Uses `notice`, the fixture term (`noticed` would not match). `RetrieveContext` is an allowed effect, so a context-retrieval hint may be stashed for the next turn. |
| 7 | "Remember that thesis you had about me putting things off? I think it happened again this week." | `grow-the-library` 12 (`remember`,`thesis`) | **`grow-the-library`** → none | Memory recall against the seeded procrastination record; consumes any turn-6 retrieval hint; a repeat winner exercises the anti-nag suppression path when turn 6 surfaced. |
| 8 | "Can you find the latest on Grok, since the planners keep bringing it up?" | nothing qualifies (`can` 1) | none — explicit-topic path, consultation **expected** | Capitalized-entity world consultation. Cues `find`/`latest`; anchor `grok`; the bundled fixture corpus contains a Grok article, so the default run injects a real match. |
| 9 | "Can you find the latest on grok, since the planners keep bringing it up?" | nothing qualifies | none — consultation **not expected** | The STT-lowercased twin. `Can` is stoplisted, `grok` is lowercase, and there is no dotted version, so the explicit-topic detector returns `None` (Corrections item 7). Byte-identical to turn 8 except one capital letter. |
| 10 | "Never mind — you got it the first time. What are you focused on right now, and what's pulling at your attention?" | nothing qualifies (`what` 1) | none — `below_qualification_threshold` | The read-only tool-loop turn: the session instructions direct `inspect_volition_state` for current-focus questions. Also the deliberate no-qualifier suppression class. |
| 11 | "So where is all of this heading for society — the whole world, not just warehouses?" | `assemble-world-picture` 8 (`society`,`world`) | **`assemble-world-picture`** → none | `heading` matches nothing; `world` and `society` are the fixture terms. Exercises the subconscious reduced-ambient-exposure path and a second goal-activation `ConsultWorld`. |
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
  sideband's strategy) ranks the intended procrastination record first; the same test with `now` set
  years in the future produces the identical ranking.
- Snapshot tests: the seeded `volition-state.json` loads through
  `VolitionContinuitySnapshot::load_or_upgrade` and passes `snapshot_is_fixture_compatible`; a
  deliberately incompatible variant is *discarded with a `VolitionContinuityNote`* and does not
  panic; a malformed file likewise produces a note.
- Provenance test: every seed record sets `provenance` and `trust_tier` explicitly (parse the raw
  JSON and assert the keys are present, so a future serde default cannot silently take over), and no
  record sets `time_sensitive_decay_half_life_days`.
- `cargo test -p qsf_realtime_server -p qsf_memory -p qsf_volition` green;
  `cargo clippy --all-targets -- -D warnings`; `cargo fmt`.

**Operator / human testing required (paid)**: the first full-script run,
`cargo run -p qsf_realtime_server -- probe`. Cost: **one session, twelve turns** — at least twelve
realtime responses (the tool-loop turn can add up to three more provider responses before the spoken
answer) plus roughly twelve off-hot-path goal-formation calls. Audio output tokens dominate; this is
the most expensive step in the plan. Evidence to collect: status `passed`; the expectation diff empty
or explained; the world-consultation pair shows exactly one `world_consultation_performed` record for
turn 8 and none for turn 9; turn 7's turn context contains the seeded memory; the tool-loop turn
shows a `ToolExecutionRecord` for `inspect_volition_state`; the formation clause shows
`settled == expected`.

**Live goal formation and expectations.** Live goal formation can admit new goals mid-run, which
legitimately changes later turns' activation. Therefore the offline phrase-design test is the hard
gate, while the live run reports per-turn expectation differences as a **diff recorded in the
manifest**, never as a verdict failure. The verdict's failing clauses stay strictly deterministic.

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
