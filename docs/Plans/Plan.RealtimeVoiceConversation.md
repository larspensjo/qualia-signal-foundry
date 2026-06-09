# Plan: Realtime Voice Conversation (Three-Plane Architecture)

## Status

Candidate implementation plan. Phase 0 (decisions & contracts) is complete and
accepted (2026-06-09); **Phase 1 is the active phase**, expanded below into an
actionable refactor. Phases 2–5 remain intentionally high-level until reached.

> Companion to the design note
> [`Design.RealtimeVoiceConversation.md`](Design.RealtimeVoiceConversation.md), which
> is authoritative for the rationale, trust boundary, and contracts. This document
> is the **phased build plan**: it sequences the work into independently testable
> slices and marks where external human verification is required.
>
> **Intentionally high-level (for future phases).** Each unstarted phase is a
> self-contained slice, not a task-by-task script. Expand a phase into detailed steps
> (file paths, test code, commits) immediately before executing it, surfacing that
> phase's open questions first (per `Agents.md`).

## Goal

Enable a live, browser-based, full-duplex spoken conversation where `gpt-realtime`
owns the voice (media plane) and QSF owns the mind (memory, context, tools,
observability) — built incrementally so each phase ends in a verifiable state.

This is the intended primary operating mode of the project, not merely another
experiment path. The experiment documents named below are validation scaffolds for
building and measuring slices of the mode; the end state is a normal way to run
QSF.

## Phasing Principles

- Each phase builds, passes `cargo test`, and is green under
  `cargo clippy --all-targets -- -D warnings` then `cargo fmt`. UI changes also pass
  `npm run check` then `npm run fmt` in `crates/qsf_browser_server/ui/`.
- Reducers stay pure and unit-tested; side effects live at the edge and feed back as
  actions (`input -> action -> reducer -> state -> render`).
- A phase that adds a flag/threshold must default to exercising the new path.
- "Human testing" marks steps that need external manual verification — automated
  tests cannot cover the live spoken experience.
- The launcher should eventually expose realtime voice conversation as a first-class
  mode. `app -Experiment ...` remains the current harness for experiments and tests,
  but it is not the intended final operator surface for live conversation.

## Phase Overview

| Phase | Slice | Code? | Human test? |
|-------|-------|-------|-------------|
| 0 | Decisions & contracts (accepted) | No | No |
| 1 | Extract `qsf_session` crate (pure refactor) — **active** | Yes | No |
| 2 | Thin media plane — live browser voice | Yes | **Yes** |
| 3 | Authoritative sideband + memory injection | Yes | **Yes** |
| 4 | Model-invoked read-only perception tools | Yes | **Yes** |
| 5 | Live memory extraction + presence refinement | Yes | **Yes** |

---

## Phase 0 — Decisions & contracts (no code) — completed, accepted 2026-06-09

Lock-in pass, no implementation. The provider-event → QSF-event mapping contract
(exchange boundary, id mapping, overlap / out-of-order behavior) is recorded in
[`Design.RealtimeVoiceConversation.md`](Design.RealtimeVoiceConversation.md) and
`docs/DecisionLog.md`. Accepted decisions that constrain later phases:

- `qsf_realtime_server` owns live realtime side effects; `qsf_browser_server` stays a
  read-only inspection server.
- The browser owns the WebRTC media plane. The QSF server owns ephemeral-token
  minting, SDP rendezvous, and the `{ qsf_session_id ↔ provider call_id }` binding.
- Phase-2 browser-relayed provider events are untrusted, diagnostic-only, and
  excluded from sleep and continuity. The Phase-3 server sideband is the
  authoritative source for trusted live exchanges.
- Phase-2 defaults: `gpt-realtime-2`, voice `marin`, `reasoning_effort = medium`,
  `output_modalities = ["audio"]`, and provider `server_vad` with automatic response
  creation and interruption enabled.
- The browser client-secret lifetime follows the provider-returned `expires_at`. The
  `call_id` binding is active-call scoped, invalidated on stop/error/expiry, and
  retained only for a short cleanup grace for diagnostics.
- `qsf_session` should be lean: reducer/state/event contracts, `Exchange`,
  persistence DTOs, continuity manifest, and the event-record/`EventType` contract
  may move with it; `RunContext`, provider clients, memory retrieval, tools, and
  OpenAI/CPAL dependencies stay outside.
- Realtime voice conversation is the long-term primary QSF operating mode. Phase
  experiment docs validate the path; they do not define the final operator surface.

**Follow-up constraint into Phase 1.** The "may move with it" latitude above is
deliberately loose. The exact `EventType` home, and how far the `ContextAssembly` /
`ContentHash` field types travel, are confirmed at the top of Phase 1 (open
questions 1–2) before any code moves, so the crate boundary lands once. As resolved
in Phase 1 below, the run-log `EventType` taxonomy stays in `qsf_app` and only the
per-`Exchange` provider event records move; the Phase-1 docs task amends the
2026-06-09 decision-log wording accordingly.

---

## Phase 1 — Extract `qsf_session` crate (pure refactor, no behavior change) — active

**Status.** Active / next to implement. Pure refactor with no runtime behavior
change, with the single exception of the reducer-contract change explicitly
sanctioned below (`ExchangeCompleted` identity), which is reducer-local and
unit-tested.

**Outcome.** A new `crates/qsf_session` crate holds the pure session reducer, state,
event, persistence, and continuity contracts and depends only on data/serialization
crates (`serde`, `serde_json`, `time`, `uuid`, `anyhow`, `tempfile`, and `qsf_memory`
for `ProcessedRange`). It pulls in none of `cpal`, `openai_provider_kit`, `reqwest`,
`tokio`/`tokio-tungstenite`, `hound`, `base64`, or `engine_logging`, and references
no `RunContext`, no `qsf_app::memory` retrieval, no `sleep`/`tools` logic, and no
`observability` recorders. `qsf_app` re-exports the moved items so its existing
`crate::session::…` call sites (≈21 files across `experiments/`, `sleep/`, `tools/`,
`models/`) compile with import-only edits, and the future `qsf_realtime_server`
(Phase 2) can depend on `qsf_session` without the heavy graph. `qsf_browser_server`
sees no dependency change (it depends only on `qsf_memory`, never `qsf_app`), but it
**is schema-affected**: `crates/qsf_browser_server/src/session_context.rs` parses
`session-state.json` directly via its own `SessionStateDocument`, so the persisted
serde schema must stay byte-compatible (covered under Acceptance/Verification below).

**Open questions to confirm before coding** (per `Agents.md` — surface before
silently resolving):

1. **`EventType` home.** Phase 0 says the "event-record/`EventType` contract *may*
   move." But `observability::event_log::EventType` is the run-log taxonomy consumed
   only by effectful recorders (`RunContext::record_event`, `record_session_event`,
   `live_memory`, `ageing`) that all stay in `qsf_app`.
   *Resolution (recommended, adopted as the Phase-1 plan):* keep `EventType` and the
   `EventRecord` writer in `qsf_app`; move only the per-`Exchange`
   `ProviderEventRecord` / `ProviderEventKind` (already defined in `exchange.rs`).
   Moving the whole `event_log` taxonomy into `qsf_session` widens the surface for no
   Phase-2 benefit. This **refines/supersedes** the 2026-06-09 decision-log wording
   ("`qsf_session` owns the event-record/`EventType` contract"); a Phase-1 docs task
   (below) amends that entry to record the shipped split. If reviewers reject this,
   treat the `event_log` move as its own isolated sub-step.
2. **Foundational field types.** `Exchange` / `Turn` / `SessionState` structurally
   embed `ContextAssembly` (from `context`) and `ContentHash` (from
   `conversation::prompt`). Moving them verbatim would otherwise drag the `context`
   module — and through `ContextFragment`'s `From<&RetrievedMemory>` impl, the
   `memory` crate — into `qsf_session`, violating the lean boundary.
   *Resolution (recommended, derivable from the repo):* move the plain value types
   **together with their attached pure methods** (`ContentHash`; the `context`
   assembly value types `ContextAssembly` — including `retrieved_memory_ids()` —
   `ContextBudget`, `ContextFragment`, `ContextSourceKind` — including
   `source_priority()` — `ContextSelection`, `ContextOmission`) into `qsf_session`.
   These methods are pure and dependency-free, so they belong with the types rather
   than being stranded as free functions in `qsf_app` (which would break the
   `assemble_context` / sleep-record callers). Leave the *algorithms*
   (`assemble_context` in `context_assembler.rs`, `canonical_hash` /
   `assemble_prompt` in `conversation::prompt`) and the
   `From<&RetrievedMemory> for ContextFragment` conversion in `qsf_app`. The orphan
   rule is satisfied for that conversion: `impl From<&RetrievedMemory> for
   ContextFragment` is legal from `qsf_app` because `RetrievedMemory` (the type
   parameter of `From`) stays local to `qsf_app`, even though `ContextFragment`
   becomes foreign. **Fallback** only if a future change makes both types foreign:
   replace the `From` impl with a free function
   `context_fragment_from_retrieved(&RetrievedMemory) -> ContextFragment`.

**Scope — what moves to `qsf_session`:**

- Pure reducer and contracts from `session/live_state.rs`: `LiveSessionEvent`,
  `LiveSessionState` (+ `RuntimePhase`, `ResponseStatus`, `PartialTranscript`,
  `ActiveResponseState`, `LiveCaptureContext`, `AgedCoRetrievalRecord`),
  `reduce_live_session`.
- Pure functions from `session/runtime.rs`: `reduce_session`,
  `reduce_session_in_place`, `apply_live_session_event` (the pure wrapper), and
  `resume_breaking_config_changed`.
- State/event DTOs from `session/mod.rs`: `SessionState`, `SessionConfig`,
  `MemorySourceConfig`, `Turn`, `TurnSummary`, `TurnRange`, `RecallRecord`,
  `PromptPrefixInvalidation`, `SessionLimit`, `SessionEndReason`, `SessionEvent`,
  `is_turn_summarized`, and the schema-version constants.
- `session/exchange.rs` in full (`Exchange`, the record/enum types — including
  `ProviderEventRecord` / `ProviderEventKind` per open question 1 —
  `ExchangeTurnConversionError`, and `TryFrom<&Exchange> for Turn`).
- `session/persistence.rs` (`persist_session_state` / `load_session_state`) and
  `session/manifest.rs` (`ContinuityManifest`, `ResumeMode`).
- `session/resume.rs` — **split, not moved verbatim** (see the resume-split note
  below). `qsf_session` receives the pure parts: `ResumeInputs`,
  `classify_resume_mode`, and a pure loader that performs the file load + schema
  upgrade and *returns* schema-upgrade metadata instead of logging it.
- `session/continuation.rs` (pure; session-internal deps only).
- `session/sleep_records.rs` (`SleepRecord`, `SleepRecordKind`).
- The two tool-taxonomy enums `ToolCategory` and `ToolSideEffectLevel` (today in
  `tools/tool_request.rs`) that `RecallRecord` embeds — move to `qsf_session`;
  `ToolPermission` and the rest of `tool_request.rs` stay in `qsf_app::tools`, which
  re-exports the two enums.
- The foundational field types per open question 2 (`ContentHash`; the `context`
  value types and their pure methods).

**Resume-split note (resolves review blocker B1).** `session/resume.rs` is **not**
qsf_session-ready verbatim: `load_resume_inputs` calls
`engine_logging::engine_info!` on schema upgrade (`resume.rs:29`), and the file also
exposes `state_dir_from_env()` (`resume.rs:63`) which calls
`crate::session::resolve_shared_state_directory_from_env()` — both forbidden by the
lean boundary. Split it:

- **`qsf_session`** owns `ResumeInputs`, `classify_resume_mode`, and a pure loader
  (e.g. returning `ResumeInputs` plus an `Option<SchemaUpgrade { session_id, from,
  to }>`). No `engine_logging`, no env access. The existing
  `load_resume_inputs_upgrades_legacy_session_schema_version` test moves with the
  loader (it asserts the upgraded `schema_version`, not logging).
- **`qsf_app`** keeps `state_dir_from_env()` and a thin wrapper around the pure
  loader that emits the `engine_info!` schema-upgrade log from the returned metadata,
  preserving today's observability exactly.

**Scope — what stays in `qsf_app` (effectful edge):**

- The effectful functions in `session/runtime.rs`: `boot_session`,
  `apply_session_event`, `record_session_event`, `persist_continuity_state` /
  `persist_continuity_state_from_dirs`, `copy_forward_memory_store`,
  `merge_memory_store_contents`, `format_boot_brief_for_context`. These need
  `RunContext`, `EventType`, `MemoryStore`, and `sleep::commit::ConsolidatedBrief`,
  and now call the moved `qsf_session` reducers/DTOs.
- `session/live_memory.rs` and `session/ageing.rs` (depend on `RunContext`,
  `EventType`, `models`, `console`, and `memory` logic).
- The resume edge: `state_dir_from_env()` and the schema-upgrade-logging wrapper
  (per the resume-split note).
- Env/launcher concerns: `session/config.rs` (`SessionConfig::from_env` /
  `MemorySourceConfig::from_env`) and `session/state_directory.rs`
  (`resolve_shared_state_directory_from_env`). Because inherent impls cannot cross
  crates, convert the two `from_env` constructors to free functions in `qsf_app`
  (consistent with the launcher owning non-secret QSF environment). Keep
  `resolve_shared_state_directory_from_env` in `qsf_app`.
- `observability::event_log::EventType` and the `EventRecord` writer (per open
  question 1).
- `assemble_context`, `conversation::prompt` (`assemble_prompt`, `canonical_hash`, …),
  and the `RetrievedMemory → ContextFragment` conversion.

**Sanctioned reducer change — `ExchangeCompleted` identity (mapping contract):**

- Add `exchange_index: usize` to `LiveSessionEvent::ExchangeCompleted`. In
  `reduce_live_session_in_place`, finalize only when
  `active_exchange.index == exchange_index`; a mismatched index is a no-op so a
  late/duplicate completion cannot close the wrong exchange.
- Update the three construction sites — `experiments/multi_turn_text_loop/turn_runtime.rs`,
  `experiments/realtime_voice_session.rs`, `experiments/text_owned_voice_loop.rs` —
  and existing reducer tests to pass the index.
- New unit tests: matching index completes and pushes to `completed_exchanges`;
  mismatched index leaves `active_exchange` intact (no completion). This is the only
  behavior change in the phase and stays reducer-local (no provider integration yet).

**Incremental, independently reviewable steps** (each ends green; commit per step):

1. Create the `crates/qsf_session` crate skeleton — empty `lib.rs` plus a `Cargo.toml`
   with the lean dependency set above — **and add `qsf_session` to `qsf_app`'s
   `Cargo.toml` dependencies in the same step** (an unused dependency builds clean).
   This establishes the dependency edge *before* the first re-export, so steps 2–4
   stay green (resolves review finding H1). The workspace already globs `crates/*`,
   so no root manifest edit beyond the implicit membership. `cargo build` green.
2. Resolve open question 2: move the leaf foundational value types **and their pure
   methods** (`ContentHash`; the `context` value types incl.
   `ContextSourceKind::source_priority` and `ContextAssembly::retrieved_memory_ids`)
   into `qsf_session`; add explicit, per-item re-exports from `qsf_app::conversation`
   and `qsf_app::context` (a hybrid facade — never a broad glob — to avoid clashing
   with the local algorithm modules); leave `assemble_context`, the prompt
   algorithms, and the `From<&RetrievedMemory>` conversion behind. `cargo build` +
   `cargo test` green.
3. Move `ToolCategory` / `ToolSideEffectLevel` into `qsf_session`; re-export from
   `qsf_app::tools`. Green.
4. Move `exchange.rs`, the `mod.rs` state/event/`Turn` DTOs, `persistence.rs`,
   `manifest.rs`, `continuation.rs`, `sleep_records.rs`, and the pure reducer
   functions into `qsf_session`. **Split `resume.rs`** per the resume-split note:
   `qsf_session` gets `ResumeInputs` / `classify_resume_mode` / the pure loader;
   `qsf_app` keeps `state_dir_from_env()` and the schema-upgrade-logging wrapper.
   Keep the effectful `runtime.rs` functions in `qsf_app`, importing reducers/DTOs
   from `qsf_session`; convert the `from_env` constructors to `qsf_app` free
   functions. Build the **hybrid `qsf_app::session` facade module** explicitly: local
   effectful submodules (`runtime`, `live_memory`, `ageing`, `config`,
   `state_directory`, the resume wrapper) **plus** explicit `pub use qsf_session::{…}`
   re-exports of the moved items, so `crate::session::…` keeps resolving for the ≈21
   dependent files. Do not use a blanket glob re-export that could collide with the
   retained local modules. Green.
5. **Test-support & fixture preservation** (resolves review findings H2 + M3).
   `fake_turn` is `pub(crate)` in `session::tests` (`mod.rs`) and used by
   `experiments/text_owned_voice_loop.rs`, `experiments/sleep_phase_session_summary.rs`,
   `sleep/auto_promote.rs`, and the moving `session/continuation.rs`. Because
   `#[cfg(test)]` items do not cross crate boundaries, provide the helper on both
   sides: a `#[cfg(test)]` `fake_turn` inside `qsf_session` (for `continuation.rs`
   and the moved reducer/persistence tests) and a `#[cfg(test)]` `fake_turn` in
   `qsf_app` (for the three remaining qsf_app test files); optionally consolidate via
   a `qsf_session` `test-support` cargo feature enabled in `qsf_app` dev-deps to keep
   it DRY. Relocate the `pre_migration_session_state.json` fixture — referenced only
   from the two moving files (`session/mod.rs:278`, `session/resume.rs:136`) — into
   `crates/qsf_session/tests/fixtures/` and fix the `include_str!` paths. Add a
   `qsf_session` golden/fixture test covering both the legacy fixture **and** a
   representative current `SessionState` (including `live` and `Exchange` fields) so
   serde field names / `default` / `skip` attributes are guarded after the move.
   Green.
6. Apply the `ExchangeCompleted` identity change and its tests (above). Green.
7. Run the lint/format gates and update the docs (below).

**Acceptance criteria:**

- `cargo build` and full `cargo test` green; `cargo clippy --all-targets -- -D warnings`
  clean; `cargo fmt` applied.
- `qsf_session`'s dependency graph is lean: `cargo tree -p qsf_session` shows none of
  `cpal`, `openai_provider_kit`, `reqwest`, `tokio`, `tokio-tungstenite`, `hound`,
  `base64`, or `engine_logging`; and the crate's source references no `RunContext`,
  no `qsf_app::memory`/`sleep`/`tools` logic, no `observability` recorders, and no env
  access.
- The ≈21 `qsf_app` call sites still resolve through the hybrid re-export facade —
  the diff to experiment/sleep/tools/models modules is import-only, with no logic
  changes. The four `fake_turn` consumers and both fixture references still compile.
- Behavior parity: the persisted `session-state.json` and `continuity-manifest.json`
  schemas are unchanged (serde field names and `serde(default)`/`skip` attributes
  identical), proven by the golden/fixture test in step 5, so the read-only
  `qsf_browser_server` `session_context.rs` parser continues to read state untouched.
- The resume schema-upgrade `engine_info!` log still fires (via the `qsf_app`
  wrapper) — observability behavior is preserved, not silently dropped.

**Verification guidance** (fits a pure refactor; no human testing):

- *Reducer/persistence determinism:* the moved reducer and persistence tests run
  inside `qsf_session`. They already use `SystemTime::UNIX_EPOCH` fixtures and fixed
  ids, giving deterministic parity for the pure surface — these are the primary gate.
- *Schema golden parity (not byte-for-byte run artifacts):* the new `qsf_session`
  fixture/golden test (step 5) covers the legacy fixture plus a representative
  current `SessionState` with live/exchange fields; full `cargo test` keeps the
  `qsf_browser_server` session-search endpoint (its hand-rolled `SessionStateDocument`
  parser) covered against schema drift.
- *Normalized-artifact parity (not byte-for-byte):* run an experiment that exercises
  the bridge (e.g. `realtime-voice-session` or `multi_turn_text_loop`) before and
  after the refactor, scrub volatile fields (timestamps from `SystemTime::now()`,
  the UUID `session_id`), and diff the normalized run-dir artifacts. Byte-for-byte is
  explicitly out of scope because the bridge uses `SystemTime::now()`.
- *Leanness check:* `cargo tree -p qsf_session` as recorded evidence the boundary
  held.
- **No human testing** — there is no live spoken experience in this phase.

**Docs (per `ProjectWorkflow.md`):**

- Amend the 2026-06-09 `docs/DecisionLog.md` entry ("Lean session crate owns pure
  session contracts") to record the shipped split: `qsf_session` owns the
  per-`Exchange` provider event records/kinds, while `qsf_app` retains the run-log
  `EventType` taxonomy and `EventRecord` writer (resolves review finding M1).
- A `docs/DecisionLog.md` entry recording the `qsf_session` crate boundary as
  actually shipped, including the resolutions to open questions 1–2 and the
  resume-split decision.
- One `EngineeringDiary.md` entry (follow the diary's "How to use" header).
- A touch to `Architecture.StateAndObservability` if the module home or its notes
  move. No `Experiment.*` doc (no live behavior in this phase).

---

## Phase 2 — Thin media plane: live browser voice  *(first time you can talk)*

**Scope.**
- **Server (`qsf_realtime_server`, axum):** `POST /api/realtime/session` mints a
  short-lived ephemeral client secret (holds `OPENAI_API_KEY` server-side).
  `POST /api/realtime/sdp` proxies the SDP exchange and stores the
  `{ qsf_session_id ↔ provider call_id }` binding. `WS /api/realtime/events` receives
  browser-relayed events. Default session config: `gpt-realtime-2`, voice `marin`,
  `reasoning_effort = medium`, `output_modalities = ["audio"]`, and `server_vad`
  with automatic response creation and interruption enabled.
- **Browser (new TS in `ui/src/`):** fetch token → `RTCPeerConnection`, send SDP
  offer via the server, attach mic, play remote audio, provider VAD + barge-in.
  Minimal UI: start/stop, live transcript, listening/thinking/speaking status.
- **Server translate:** relayed events → `LiveSessionEvent` (per the Phase-0 mapping
  contract) → reducer (`qsf_session`) → persist exchanges + event/trace logs,
  **marked untrusted / diagnostic-only and excluded from sleep + continuity**.

**Verify (automated).** Token route (mocked OpenAI); SDP-proxy stores `call_id`;
event-translation → persisted-`Exchange` tests **including the reducer overlap /
out-of-order matrix** (a gate this phase); relayed-event validation rejects
malformed/oversized payloads; TS event-mapping unit tests (`npm run check`).

**Human testing (required).** Open the browser, speak, hear a reply, interrupt
mid-reply; confirm diagnostic exchanges appear in artifacts; inspect network traffic
to confirm the API key never reaches the browser.

**Docs.** `Experiment.RealtimeBrowserVoiceMVP` as the validation record; new
`Architecture.RealtimeSessionServer.md` (three-plane server, rendezvous, trust
boundary); refresh `Architecture.AudioLoop.md` Implementation Status; decision-log
entries (realtime-server crate, browser-owns-media, ephemeral tokens,
diagnostic-only relay); diary entry; README "What works today". Add launcher notes
for the preview path and the intended future first-class realtime mode.

---

## Phase 3 — Control/context plane: authoritative sideband + memory injection  *(the "mixture" becomes real)*

**Scope.**
- Extract reusable **protocol helpers** (request builders, event parsing) from
  `voice_session_provider` (small extraction step), then build a **new async sideband
  adapter** (long-lived, concurrent read/write, cancellation/shutdown) that connects
  to the stored `call_id`. Reuse the helpers, **not** the one-shot runner.
- The sideband becomes the **authoritative** event source; its exchanges are trusted
  and sleep/continuity-eligible. Browser relay reverts to UI-only diagnostics.
- Per session start and per user turn, retrieve relevant memory (existing
  association-weighted retrieval) and inject a **small** working-memory packet via
  `conversation.item.create`, plus `session.update` for identity/tone. Relevance over
  volume — never a full memory dump.

**Verify (automated).** Sideband attaches to a stored `call_id` (mocked); given a
memory store + transcript, the server emits the expected (small) injection payloads;
trusted exchanges are sleep-eligible while Phase-2 diagnostic ones are not.

**Human testing (required).** Reference something across turns and across sessions;
confirm continuity surfaces in the spoken conversation.

**Docs.** `Experiment.LiveContextInjection`; update
`Architecture.RealtimeSessionServer.md` and `Architecture.MemorySystem`; decision-log
entry (authoritative sideband); diary entry.

---

## Phase 4 — Tool plane: model-invoked read-only perception tools

**Scope.**
- Expose allow-listed **read-only** tools (search memory, retrieve associations,
  inspect state). On a function call, the server executes via the existing tool
  registry, adds a `function_call_output` item, and re-issues `response.create`.
- **Record execution, not just intent.** Keep `ToolRequested` as the request record;
  add result/observability types (permission decision, status, result summary, error,
  timing, the returning event) linked by `call_id`. Do not overload `auto_executed`
  as execution evidence.

**Verify (automated).** Function-call → permission decision → registry execution →
`function_call_output` returned; a non-allow-listed tool proven to stay **unexecuted
and recorded as denied**.

**Human testing (required).** Ask something requiring memory search; confirm the
model calls the tool and uses the result in its spoken reply.

**Docs.** `Experiment.LiveToolPerception`; update `Architecture.ToolSystem` and
`Architecture.StateAndObservability`; decision-log entry (read-only tools +
permission/result recording); diary entry.

---

## Phase 5 — Live memory extraction + presence / interruption refinement

**Scope.** Lightweight extraction over completed **trusted** turns (reuse the
sleep/memory proposers) feeding the existing review/consolidation path. Refine
interruption representation and end-to-end / per-stage latency reporting for presence
research.

**Verify (automated).** Extraction tests over trusted turns; latency measurements
recorded.

**Human testing (required).** Presence evaluation against the
`Concept.RealtimePresence` open questions; record latency observations.

**Docs.** Experiment doc + report; refresh `ResearchQuestions.Audio.md` (injection
relevance, ASR-vs-model transcript divergence); update `Concept.RealtimeAudio.md` /
cross-link `Concept.RealtimePresence`; diary entry.

---

## Launcher / Operator Surface

Today `scripts/qsf.ps1` mainly launches `qsf_app` experiments, the memory browser,
the UI, and the workbench. That is appropriate for the current state of the repo.

As realtime voice conversation becomes runnable, the launcher should grow a
first-class operator mode for it rather than treating it as only
`app -Experiment <name>`. The exact command name should be decided when the server
and UI entry point exist, but the intended shape is:

```text
qsf.ps1 <realtime-conversation-mode>
  -> start qsf_realtime_server
  -> start/open the browser UI
  -> apply non-secret QSF defaults through the launcher
  -> verify required secrets without printing them
```

The experiment runner should remain available for regression tests, fixture-backed
validation, and phase reports.

---

## Cross-Cutting Verification

- **Lint gates every phase:** Rust → `cargo clippy --all-targets -- -D warnings` then
  `cargo fmt`; UI → `npm run check` then `npm run fmt`.
- **Phase 1 gate:** schema golden/fixture parity (legacy + current `SessionState`)
  plus normalized-artifact parity (volatile fields scrubbed), not byte-for-byte.
- **Phase 2 gate:** the reducer overlap / out-of-order test matrix.
- **Human testing required at Phases 2–5** for, respectively: the live spoken
  experience, cross-session continuity, model-invoked tool use, and presence.

## Remaining Checks Before Each Phase

- **Phase 1:** addressed in the expanded Phase 1 section above — the file-level
  move/stay split (including the `resume.rs` split that keeps `engine_logging` and
  env access in `qsf_app`), the hybrid (non-glob) `qsf_app::session` facade, the
  dependency edge added before the first re-export, the test-support/`fake_turn` and
  fixture preservation, the dependency-leanness acceptance check, and open
  questions 1–2 (the `EventType` home and how far the `ContextAssembly`/`ContentHash`
  field types and their pure methods travel) — all confirmed before the move begins.
- **Phase 2:** verify the accepted model/voice/VAD defaults against the live provider
  at implementation time, then record any API drift explicitly before changing them.
- **All phases:** confirm the provider-event mapping contract still holds against the
  actual event stream observed once live (Phase 2 is the first reality check).

## Documentation Updates (per `ProjectWorkflow.md`)

Summarized per phase above. Aggregate touch-list: `DecisionLog.md` (accepted
decisions as each lands, plus the Phase-1 amendment of the 2026-06-09 lean-session-
crate entry to reflect the `EventType`/provider-event-record split),
`EngineeringDiary.md` (one entry per logical application change), `README.md` and
launcher documentation (as phases land), new
`Architecture.RealtimeSessionServer.md`,
refreshes to
`Architecture.AudioLoop.md` / `Architecture.ToolSystem` / `Architecture.MemorySystem`
/ `Architecture.StateAndObservability`, `ResearchQuestions.Audio.md`,
`Concept.RealtimeAudio.md` (+ `Concept.RealtimePresence` cross-link), and one
`Experiment.*` doc per live phase.