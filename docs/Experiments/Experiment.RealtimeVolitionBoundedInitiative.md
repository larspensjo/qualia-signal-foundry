# Experiment: Realtime Volition Bounded Initiative

## Status

Running. Phase 5 code is implemented and automated verification passed on 2026-06-30.
Live human voice verification is still pending, so this experiment is not yet marked
complete.

## Summary

Validate that the realtime sideband can derive a bounded internal initiative from the
arbitration winner on trusted turns, surface it only under bounded conditions, and record
an inspectable diagnostics trace without introducing any external write-capable effect.

This is the live-validation companion to the "realtime bounded initiative" behavior in
`docs/Plans/Plan.RealtimeVolitionIntegration.md`.

## Decisions Resolved Before Implementation

### D1 - Initiative line rides inside the existing volition packet

The surfaced initiative line is appended to the existing single per-turn volition system
item. It does not become a second system item, so the Phase 4 ordering stays intact:
memory item -> volition item -> `response.create`.

### D2 - Protected winners do not nag on ordinary direct requests

A protected-tier winner surfaces only when the turn contains a genuine opportunity signal
beyond the winner's own topic self-match. Ordinary direct requests stay quiet even if the
winner is protected and active.

### D3 - Anti-nag is consecutive-turn alternation

The same goal is not surfaced on two adjacent trusted turns. The runtime remembers only
the previous surfaced goal id, and it clears that marker on any non-surfaced turn so a
repeated winner can surface again on a later turn.

### D4 - Context-retrieval outputs are hint-only

`ContextRetrievalRequested` never becomes a model-facing initiative line. Its query terms
are stashed for the next turn's memory/context retrieval path and recorded in the trace.

## Related Documents

```text
docs/Plans/Plan.RealtimeVolitionIntegration.md
docs/Architecture/Architecture.RealtimeSessionServer.md
docs/Architecture/Architecture.ContextManagement.md
docs/Architecture/Architecture.StateAndObservability.md
docs/Architecture/Architecture.VolitionSystem.md
docs/DecisionLog.md
crates/qsf_realtime_server/src/realtime/sideband.rs
crates/qsf_realtime_server/src/realtime/volition_initiative.rs
crates/qsf_volition/src/initiative.rs
```

## Hypothesis

A live realtime session can surface a small, bounded internal initiative without
claiming real desire or taking action, while protected tiers remain dominant and the
trace stream can explain exactly why the line did or did not surface.

## Scope

### In Scope

- Trace-backed bounded initiative derived from the arbitration winner.
- Model-facing line appended to the existing volition packet only when the surfacing
  gate allows it.
- Next-turn memory/context hint stashing for `ContextRetrievalRequested`.
- Diagnostics JSONL record for the initiative trace.
- Anti-nag alternation and protected-winner opportunity gating.

### Out of Scope

- Cross-session persistence of volition state.
- UI surfacing of initiative state.
- Any external write-capable effect.

## Initiative Line Contract

The rendered line is asserted verbatim in tests. The exact output depends only on the
`InitiativeOutput` variant and `ShapingIntensity`.

### ReflectionRequested

```text
Bounded initiative: reflect on {proposed_question}. Keep it simulated and internal; do
not take external action.
```

### ExperimentProposed

```text
Bounded initiative: consider experiment {hypothesis} (scope: {scope}). Keep it simulated
and internal; do not take external action.
```

### OpenThreadSurfaced

```text
Bounded initiative: surface open thread {thread_summary}. Keep it simulated and internal;
do not take external action.
```

### ContextRetrievalRequested

```text
None
```

### None intensity

```text
None
```

## Setup

- Realtime sideband enabled.
- Fixture-backed realtime volition state present for the session.
- Persisted diagnostics JSONL available for the session.

## Procedure

### Automated Verification

1. Assert each renderable initiative variant produces the exact bounded line above.
2. Assert `ContextRetrievalRequested` and `ShapingIntensity::None` render no line.
3. Assert a direct protected request is recorded but does not surface the line.
4. Assert a protected winner with an uncertainty/contradiction signal surfaces the line.
5. Assert a `ContextRetrievalRequested` turn stashes hint terms and the next turn consumes
   them, clearing the runtime stash.
6. Assert the same goal surfaces on turns 1 and 3 but not on turn 2 when repeated on three
   consecutive trusted turns.
7. Parse the diagnostics JSONL and verify every `RealtimeBoundedInitiative` record carries
   `external_effect_executed = false`, the surfaced/suppression fields, a matching
   `response_create_event_ref`, and the expected before/after state snapshots.

### Verification Result

The implementation-level verification passed with:

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`

### Human Test Steps

1. Start a realtime voice session.
2. Ask an open-ended research question and confirm a small internal initiative can surface
   without taking action.
3. Ask a direct task question and confirm protected tiers still dominate and the system
   does not nag.
4. Confirm the spoken framing still distinguishes simulated internal state from real
   subjective desire.

## Trace Completeness Contract

`realtime_bounded_initiative_trace` is written as
`DiagnosticRecord::RealtimeBoundedInitiative` in the diagnostics JSONL stream.

Required fields:

- `qsf_session_id`
- `exchange_index`
- `winning_goal_id`
- `initiative_proposal`
- `allowed_effect`
- `initiative_output`
- `bounded_or_external_output` with `external_effect_executed: false`
- `surfaced`
- `suppression_reason` when a surfaced line is suppressed
- `rendered_line_present`
- `context_retrieval_hint_terms` when the output is `ContextRetrievalRequested`
- `hint_consumed_by_next_memory_injection` on the consuming turn
- `rationale`
- `state_snapshot_before`
- `state_snapshot_after`
- `response_create_event_ref`
- `artifact_or_record_reference`

Artifact boundary:

- The persisted diagnostics JSONL stream is the authoritative artifact boundary.
- `response_create_event_ref` reuses the same per-turn request-sequence hash used by the
  `VolitionContextInjected` trace for that turn.

Parsing verification:

- Parse the diagnostics JSONL, not just in-memory structs.
- Assert each initiative trace has `external_effect_executed = false`.
- Assert each `ContextRetrievalRequested` trace is followed by a later turn whose trace has
  `hint_consumed_by_next_memory_injection = true`.
- Assert the winning goal's before/after snapshot transition reflects the initiative event.

## Expected Output

- A realtime turn whose spoken answer can stay focused while still surfacing a bounded
  internal initiative when the conversation invites it.
- Persisted `RealtimeBoundedInitiative` records that explain surfaced and suppressed turns.

## Results

Automated verification passed. Live human voice verification is pending.
