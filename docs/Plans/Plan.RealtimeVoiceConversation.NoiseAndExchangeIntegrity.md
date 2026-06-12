# Plan: Realtime Voice Noise Filtering and Exchange Integrity

## Status

In progress. Phase 1 is expanded and ready for implementation. Phases 2–4 remain
outlined and will be expanded when their turn comes.

## Context

A live `qsf_realtime_server` run using the stable `default` session id and a seeded
memory store showed that Phase 4 tool calls can work in practice:

- `get_associations` resolved `memory.voice-loop-ownership` and returned the strongest
  associated neighbor.
- `inspect_session_state` executed.
- `search_memory` correctly returned no results for an absent "Pineapple Radar"
  query.

The same run exposed three issues:

1. Browser microphone input accepted likely noise or speaker bleed as short user turns
   (`Cheers.`, `Thank you.`).
2. A cancelled response/tool continuation was able to pair its eventual answer with a
   later short transcript, so a durable trusted turn had `user_input = "Thank you."`
   but an answer for the earlier memory-search request.
3. `inspect_session_state()` overcounted exchanges because it combined promoted durable
   turns with retained live completed exchanges.

This plan fixes those issues without changing the Phase 4 tool scope.

## Goals

- Reduce accidental turn creation from speaker bleed and short noise-like transcripts.
- Preserve trusted exchange integrity when provider VAD detects new speech during an
  assistant response or tool loop.
- Make `inspect_session_state()` report an auditable, non-duplicated exchange count.
- Keep defaults exercising the new protections; random or disabled behavior should be
  opt-in only if added later.

## Non-Goals

- Replacing provider `server_vad` with a custom VAD implementation.
- Adding push-to-talk as the only interaction mode.
- Changing the read-only tool allow-list.
- Solving all acoustic echo cases across all hardware. Human testing is still required.

## Open Questions

- Should short phrases like "thanks" always be filtered while the assistant is speaking,
  or only when they occur inside an active response/tool continuation? The conservative
  default should be continuation-scoped filtering so genuine brief user turns still work
  while idle.
- Should filtered noise transcripts be persisted as diagnostic-only records? Prefer yes:
  they should be observable without becoming trusted turns.
- Should `inspect_session_state.exchange_count` include the active exchange? Prefer yes,
  explicitly as `completed_exchange_count` plus optional `active_exchange_index`, instead
  of a single ambiguous count only.

## Phase Completion and Gates

Each phase is implemented and verified on its own. To keep the repository compliant if a
phase is committed independently of the others:

- Run that phase's listed automated checks (UI and/or Rust) as completion gates.
- Add a concise `docs/EngineeringDiary.md` entry for that phase's implementation, after
  reading the "How to use" instructions at the top of that file.

If multiple phases land together in a single submission, the gates and diary entries can
be consolidated and run once with the Phase 4 final gates. The substantive Rust gate run
is in Phase 4 because that is where the Rust code changes (Phases 2–3) land; phases that
touch no Rust code still run the cargo gates as a workspace-sanity check when submitted
on their own.

## Phase 1: Browser Audio Capture Constraints

This phase is browser/TypeScript only. No Rust code changes are expected; the change is
confined to `crates/qsf_realtime_server/ui/`.

Today `startConversation()` in `crates/qsf_realtime_server/ui/src/main.ts` (around
line 168) requests the microphone with the bare constraint `audio: true`, which leaves
echo cancellation, noise suppression, and auto gain control up to browser defaults.
This phase makes the processing constraints explicit and observable so speaker bleed
is reduced at the capture source.

### Step 1: Add a single source of truth for the constraints

Add an exported constant to `crates/qsf_realtime_server/ui/src/realtime.ts`, next to
`DEFAULT_SESSION_CONFIG` (the module already serves as the pure, unit-tested home for
session defaults; `main.ts` stays a thin imperative shell):

```ts
export const MICROPHONE_AUDIO_CONSTRAINTS: MediaTrackConstraints = {
  echoCancellation: true,
  noiseSuppression: true,
  autoGainControl: true,
};
```

### Step 2: Add a regression test

Add a test to `crates/qsf_realtime_server/ui/src/realtime.test.ts` asserting that
`MICROPHONE_AUDIO_CONSTRAINTS` enables all three of `echoCancellation`,
`noiseSuppression`, and `autoGainControl`. This is a contract test: it guards against
a future refactor silently reverting to `audio: true` or dropping a flag.

### Step 3: Use the constraints in the capture path

In `startConversation()` in `crates/qsf_realtime_server/ui/src/main.ts`, replace:

```ts
microphoneStream = await navigator.mediaDevices.getUserMedia({
  audio: true,
});
```

with:

```ts
microphoneStream = await navigator.mediaDevices.getUserMedia({
  audio: MICROPHONE_AUDIO_CONSTRAINTS,
});
```

and add `MICROPHONE_AUDIO_CONSTRAINTS` to the existing import block from
`./realtime`. This stays the default and only path, so local testing always exercises
the constraints (no config flag, per the repo rule that defaults must exercise new
code paths).

### Step 4: Log the applied capture settings (diagnostic only)

Immediately after acquiring `microphoneStream`, log what the browser actually applied,
since `getUserMedia` treats these as ideal (not mandatory) constraints and hardware or
browser support can silently differ:

```ts
for (const track of microphoneStream.getAudioTracks()) {
  const s = track.getSettings();
  console.info("microphone capture settings", {
    echoCancellation: s.echoCancellation,
    noiseSuppression: s.noiseSuppression,
    autoGainControl: s.autoGainControl,
  });
}
```

This is browser-side diagnostics, so the browser console is the right surface
(`engine_logging` covers the Rust runtime, not the UI). No reducer or UI-state changes
in this phase; surfacing these settings in the Diagnostics panel can be a later
follow-up if manual testing shows it is needed.

### Step 5: Record the change in the EngineeringDiary

Per the repo workflow, document this implementation in `docs/EngineeringDiary.md`. First
read the "How to use" instructions at the top of that file, then append one concise entry
for the UI microphone-constraint change (the explicit `MICROPHONE_AUDIO_CONSTRAINTS`
capture path and the applied-settings logging). Keep the entry to the implementation
itself; any human-test observations about provider/VAD or hardware constraint behavior
are recorded later in the Phase 4 diary observation entry and `ResearchQuestions.Audio.md`.

This keeps Phase 1 self-consistent with the repo rule even if it is submitted before the
later phases, instead of relying on a single consolidated Phase 4 entry.

### Verification

Automated, from `crates/qsf_realtime_server/ui`:

- `npm run check` (covers `tsc --noEmit` and Biome lint)
- `npm test` (vitest, including the new constraint contract test)
- `npm run fmt`

Repository completion gates (run if Phase 1 is committed independently of later phases):

Phase 1 changes no Rust code, so the cargo gates below are a workspace-sanity check
rather than a test of this phase's changes. The repo rule still requires them on task
completion, so run them before submitting Phase 1 on its own. If Phase 1 lands together
with later phases in a single submission, these run once with the Phase 4 final gates
instead.

- `cargo build`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`

External human testing (recommended):

- Start the realtime server, open the UI, and start a conversation with speakers
  enabled. Confirm the browser console logs `microphone capture settings` with all
  three flags `true` (or note any flag the browser reports differently).
- Speak the same prompts as the Phase 4 live tool-perception test and record whether
  fewer stray one-word transcript turns (`Cheers.`, `Thank you.`) appear while the
  assistant is speaking.
- Repeat with headphones as the control case.
- Expectation setting: this phase reduces bleed at the capture source but does not
  guarantee elimination (see Non-Goals); Phase 2 adds the authoritative sideband guard.
  Record any residual ghost turns observed — they are the Phase 2 test fixtures.

### Phase 1 acceptance criteria

- Microphone capture requests echo cancellation, noise suppression, and automatic gain
  control by default, with no opt-out flag.
- The constraint object has one source of truth exported from `realtime.ts`, used by
  `main.ts`, and covered by a unit test.
- The applied (not just requested) settings are observable in the browser console.
- `npm run check`, `npm test`, and `npm run fmt` pass from
  `crates/qsf_realtime_server/ui`.
- If Phase 1 is committed independently, `cargo build`,
  `cargo clippy --all-targets -- -D warnings`, and `cargo fmt` pass as a workspace
  sanity check, and `docs/EngineeringDiary.md` has a concise entry for the change.
- Manual speaker test records the applied capture settings and any residual ghost turns
  for Phase 2. "Fewer stray short turns than the baseline live run" is recommended
  supporting evidence, not a hard automation gate, because the baseline depends on
  informal hardware/browser conditions that may not be reproducible by the implementer.

Documentation: the EngineeringDiary entry for the Phase 1 implementation is added in
Step 5 above. If the manual test reveals notable browser or hardware constraint behavior,
save that observation for the Phase 4 diary observation entry and
`ResearchQuestions.Audio.md` updates rather than expanding the Phase 1 entry.

## Phase 2: Sideband Turn Integrity Guard

Add an authoritative sideband guard in
`crates/qsf_realtime_server/src/realtime/sideband.rs` so a new final transcript that
arrives while an assistant response or tool continuation is in flight cannot steal or
inherit that response's state.

Implementation direction:

- Extend `SidebandRuntimeState` with an explicit turn phase, for example:
  `Idle`, `AwaitingResponse`, `ToolLoop`, `Speaking`.
- Track the exchange index and provider response id for the active response sequence.
- On `conversation.item.input_audio_transcription.completed`:
  - If no response/tool continuation is in flight, start or update the active trusted
    exchange as today.
  - If a response/tool continuation is in flight and the transcript is short or
    noise-like, record a diagnostic-only ignored transcript and do not start a trusted
    exchange.
  - If a response/tool continuation is in flight and the transcript looks like a real
    interruption, mark the current exchange non-promotable, clear the in-flight response
    state, and start a new trusted exchange cleanly. The previous response must not later
    complete against the new exchange.
- On `response.done` with non-`completed` status, mark that exchange non-promotable and
  clear all response/tool-loop state that belongs to the cancelled response. Do not
  preserve accumulated model-use or request hashes for a later exchange.
- Add a small pure helper for "noise-like continuation transcript" classification. Keep
  defaults intentionally narrow, for example:
  - only active while the assistant response/tool continuation is in flight
  - short transcript by token/character count
  - optional allow-list for common courtesy/bleed phrases observed live, such as
    `cheers`, `thank you`, `thanks`

Verification:

- Add mocked sideband tests for this exact failure:
  `memory-search turn -> function/tool response starts -> short "Thank you" transcript ->
  original response cancelled -> eventual completion must not promote a turn pairing
  "Thank you" with the memory-search answer`.
- Add tests for a real interruption transcript while speaking: the old exchange becomes
  non-promotable and the new exchange starts with clean response/tool state.
- Add tests that short transcripts while idle still create a normal turn.
- Assert `session-state.json` promoted turns never contain an answer whose provider
  response belongs to a different exchange index.
- Run `cargo test -p qsf_realtime_server realtime::sideband::tests`.
- Run full `cargo test`.

External human testing:

- Re-run the default-session browser test with the seeded memory store.
- Speak the same four prompts from the Phase 4 live test.
- Leave speakers enabled once, then repeat with headphones.
- Confirm no promoted trusted turn pairs a short noise transcript with the previous
  answer, even if the UI still shows diagnostic browser-relay transcripts.

## Phase 3: Fix Session Inspection Counts

Fix `ToolSessionSnapshot::from_runtime` in
`crates/qsf_realtime_server/src/realtime/tools.rs`.

Implementation direction:

- Replace the ambiguous `exchange_count` computation:
  `live.completed_exchanges.len() + turns.len()`
- Count durable promoted turns once, then optionally include the active exchange:
  - `completed_exchange_count = runtime.session_state.turns.len()`
  - `active_exchange_index = runtime.session_state.live.active_exchange.as_ref().map(...)`
  - If keeping the existing `exchange_count` field, define it as
    `completed_exchange_count + active_exchange_index.is_some() as usize`.
- Consider adding explicit fields to the tool output:
  `completed_exchange_count`, `active_exchange_present`, and `active_exchange_status`.
  This keeps the compact summary auditable without dumping internals.

Verification:

- Add unit tests for:
  - promoted turns plus retained live completed exchanges do not double-count
  - active exchange is counted once
  - no active exchange reports only completed promoted turns
- Add/update `inspect_session_state` tool tests for the JSON output.
- Run `cargo test -p qsf_realtime_server realtime::tools::tests`.
- Include this in the manual browser retest by asking the session inspection prompt
  after two known successful turns and comparing the answer with persisted
  `session-state.json`.

## Phase 4: Documentation and Acceptance

Update documentation after implementation and human verification:

- `docs/EngineeringDiary.md`: per-phase implementation entries are added as each phase is
  completed (see "Phase Completion and Gates"). In this phase, add any consolidated
  closing entry still needed plus one observation entry if the human test reveals
  provider/VAD behavior worth preserving.
- `docs/Experiments/Experiment.LiveToolPerception.md`: add the observed ghost-turn
  failure mode, the retest procedure, and final result once verified.
- `docs/Architecture/Architecture.RealtimeSessionServer.md`: refresh sideband
  exchange-integrity behavior and `Last reviewed`.
- `docs/Architecture/Architecture.StateAndObservability.md`: document ignored
  diagnostic-only transcripts and non-promotable interrupted exchanges if added.
- `docs/Research/ResearchQuestions.Audio.md`: record the provider VAD / acoustic
  bleed observation as evidence for future presence and turn-taking work.
- `docs/DecisionLog.md`: only add an entry if the implementation creates a durable
  policy, such as "short continuation transcripts are diagnostic-only while the
  assistant is speaking."

Final gates:

- `cargo build`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`
- Because `crates/qsf_realtime_server/ui/` is touched:
  - `npm run check`
  - `npm test`
  - `npm run fmt`

## Acceptance Criteria

- Browser microphone capture requests echo cancellation, noise suppression, and
  automatic gain control by default.
- A cancelled response/tool continuation cannot be promoted under a later transcript.
- Short noise-like continuation transcripts are observable but do not become trusted
  turns by default.
- Genuine user interruption during assistant speech starts a clean new trusted exchange
  and leaves the interrupted exchange non-promotable.
- `inspect_session_state()` no longer double-counts promoted turns and retained live
  completed exchanges.
- The seeded-memory default-session manual test can complete without a mismatched
  durable turn, even if the diagnostic UI still shows provider-observed stray input.