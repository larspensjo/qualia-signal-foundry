# Experiment: Live Goal Formation and Off-Hot-Path Coherence

## Status

Planned. Depends on the offline coherence engine from
[Experiment.GoalCoherenceUnderProtectedFloor.md](Experiment.GoalCoherenceUnderProtectedFloor.md)
(`qsf_volition::coherence` + the `CoherenceJudge` seam), which is implemented. This experiment
adds the live-loop wiring, off-hot-path admission, the declined-candidate context layer, and the
sleep formation + sweep. See
[Plan.VolitionMotivationalTexture.md](../Plans/Plan.VolitionMotivationalTexture.md).

## Summary

Validate that the simulation **forms its own goals from live discussion** and **declines input
that would make it incoherent**, without touching turn latency. On each trusted turn, one
cache-structured model call over `{system + current goal set}` (stable prefix) `+ {this turn}`
(variable suffix) *proposes* an optional candidate and *detects* any contradictions; pure
`resolve_admission` *resolves* the verdict into existing lifecycle events. An admitted candidate
becomes a real goal; a rejected one is recorded as a `DeclinedCandidate` and injected as durable
session context that the model may choose to act on. A sleep pass performs whole-history formation
plus the whole-set sweep for drift.

The novel, hard parts are proven **offline and deterministically** with a scripted judge; the
live, human-testable behavior is verified by voice.

## Decisions Resolved Before Implementation

### D1 - Admission is post-turn, in-session, off the hot path

A candidate may be *formed* during a live turn, but coherence judging and any rejection run
**after** the response is dispatched — turn latency is unaffected — and complete before the
candidate can shape later turns (a merely pending candidate does not participate in arbitration).
The decline is felt within the same session. See
[DecisionLog 2026-06-30](../DecisionLog.md#2026-06-30---goal-coherence-is-model-judged-off-the-hot-path-and-repaired-during-sleep)
and
[2026-07-01](../DecisionLog.md#2026-07-01---live-goal-formation-and-coherence-detection-run-as-one-cache-structured-model-call-per-turn).

### D2 - Formation and detection are one cache-structured call per trusted turn

A single model call does formation and contradiction detection together. Its prefix (system
instructions + the current goal set) is byte-stable *until the goal set changes* — an admission,
retirement, or sweep invalidates it and a prefix-hash rule re-warms the cache on the next turn — so
it caches across the turns between goal-set changes and only the new turn is paid at full price. The
prefix is session-scoped (accepted candidates differ per session). There is **no heuristic
pre-filter gate** — the call runs every trusted turn, off the hot path. A small, fast model role is
appropriate.

### D3 - Model proposes and detects; the pure reducer resolves

The model returns an optional proposed candidate and a `CoherenceVerdict`. Pure
`resolve_admission` / `resolve_sweep` (Phase 1) resolve deterministically into the **existing**
`GoalCandidateAccepted` / `GoalCandidateRejected` / `GoalRetired` events. No new event types; the
model never mutates state.

### D4 - A rejection becomes durable session context, not a shaping rule

A rejected candidate is recorded as a `DeclinedCandidate`
(`{ candidate_id, title, conflicting_goal_id, rationale, tick }`) held in session-scoped volition
state and **injected into the realtime context as a dedicated coherence layer**, present for the
rest of the session. Because the turn's model context is built and sent before admission runs, the
record is model-visible from the **next turn onward**, not the turn that formed it. It is
evidence-backed, so injection is honest state, not confabulated narration (guardrail D4). No
shaping rule forces a line; the model decides whether and how to voice it, so there is nothing to
nag.

### D5 - Sleep does whole-history formation and the whole-set sweep

The sleep/consolidation pass runs one deliberate formation call over the full last session +
goal set (catching emergent goals the per-turn window missed) and the whole-set `resolve_sweep`
for drift. Floor goals are never cancelled.

### D6 - The model layer is extracted into a shared crate

`ModelClient` / `ModelRole` / `invoke_model_role` / `CoherenceJudge` move from `qsf_app` into a
lower shared crate that both `qsf_app` and `qsf_realtime_server` depend on. The `ModelClient`
boundary must expose a stable-prefix / cache-breakpoint boundary (Claude `cache_control`);
verifying the abstraction can express it is the first implementation step.

## Related Documents

```text
docs/Plans/Plan.VolitionMotivationalTexture.md
docs/Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md
docs/Architecture/Architecture.VolitionSystem.md
docs/DecisionLog.md
crates/qsf_volition/src/coherence.rs
crates/qsf_realtime_server/src/realtime/sideband.rs
crates/qsf_realtime_server/src/realtime/volition.rs
crates/qsf_realtime_server/src/realtime/volition_injection.rs
crates/qsf_app/src/models/coherence_judge.rs
crates/qsf_app/src/sleep/
crates/qsf_app/src/experiments/
```

## Hypothesis

The live loop can form a candidate goal from a trusted turn, judge it off the hot path, admit it
when consistent and reject it when it contradicts a more fundamental goal, and carry the rejection
as durable session context — with turn latency unaffected and every form / admit / reject / cancel
/ inject decision reconstructable from the trace stream alone.

## Scope

### In Scope

- The per-turn pipeline offline: a scripted formation-and-detection judge proposes a candidate for
  a fixture turn; pure resolution admits or rejects; a rejection produces a `DeclinedCandidate`
  and its context-injection record.
- The off-hot-path ordering guarantee: formation/admission is recorded as happening after the
  turn's response is dispatched.
- The declined-candidate coherence layer: injected into the realtime context, present across
  subsequent turns, trace-backed.
- The pending-candidate invariant: a formed-but-unadmitted candidate does not participate in
  arbitration.
- The sleep pass: whole-history formation (one call) and the whole-set sweep, over the same pure
  resolution.
- The shared model-crate boundary exposing a cache-breakpoint / stable-prefix seam.
- Cache-key discipline: the trace distinguishes cache-hit-eligible turns from prefix-invalidated
  turns via a prefix hash and an eligibility marker.

### Out of Scope

- Emotion-like signals, conscious/subconscious visibility, and multi-turn Plans (later phases).
- Cross-session persistence of declined candidates (origin-as-association) — a later refinement.
- Any arbitration/salience feedback derived from coherence state.
- Any external write-capable effect.

## Setup

- A realtime seed fixture with: at least one protected-floor core goal; one malleable core goal
  that a formed candidate can contradict; a fixture turn whose scripted formation proposes a
  **compatible** candidate (admit case); a fixture turn whose scripted formation proposes a
  candidate that **contradicts a more-fundamental core goal** (reject → declined-context case);
  and a two-goal drift pair for the sweep.
- A scripted formation-and-detection judge (deterministic) returning, per fixture turn, an
  optional proposed candidate plus a `CoherenceVerdict`.
- A `RunContext` run directory with `traces.jsonl` and `events.jsonl` (offline artifacts). The
  realtime adapter emits the equivalent as `DiagnosticRecord`s (`VolitionContextInjected`,
  bounded-initiative, etc.) on the live/human path; the automated checks below run against the
  offline harness.

## Procedure

### Automated Verification

1. **Form and admit (compatible):** a fixture turn whose formation proposes a non-contradicting
   candidate emits `GoalCandidateAccepted`; the goal set gains one `Accepted` goal; it is now
   selectable by arbitration on a later turn.
2. **Form and reject (incompatible):** a fixture turn whose formation proposes a candidate
   contradicting a more-fundamental goal emits `GoalCandidateRejected` (reason names the
   conflicting goal) and **no** `GoalCandidateAccepted`; a `DeclinedCandidate` record is added with
   the conflicting goal id and rationale.
3. **Declined-candidate injection (next turn onward):** the rejection turn records the
   `DeclinedCandidate` as a post-turn diagnostic — **not** claimed as model-visible context for
   that turn, whose context was already sent before admission ran. From the **first subsequent
   turn onward**, the declined candidate appears as a `coherence` layer in the volition
   context-injection record and stays present for the rest of the session; the injection is
   trace-backed.
4. **Pending candidate does not shape turns:** a formed-but-not-yet-admitted candidate is absent
   from the arbitration input for the turn it was formed on.
5. **Off-hot-path ordering (artifact-verified):** each `live_formation` record carries
   `response_dispatched_at` and `formation_started_at`; the harness asserts
   `formation_started_at >= response_dispatched_at`, so the model call provably follows response
   dispatch. It also asserts the turn's response latency (from the existing `LatencyObservation`
   records) matches a no-formation baseline turn within tolerance.
6. **No goal formed:** a fixture turn whose formation proposes nothing emits no lifecycle events
   and records an empty `proposed_candidate`.
7. **Sleep whole-history formation:** a scripted whole-history formation proposes a durable
   candidate the per-turn window missed; the same `resolve_admission` admits it.
8. **Sleep sweep:** the whole-set sweep cancels the less-fundamental goal of the drift pair
   (`GoalRetired`), never a floor goal, exactly as in the offline coherence experiment.
9. **Trace parse:** parse `traces.jsonl` and `events.jsonl` and assert every record satisfies the
   contract below and that its resolution is recomputable from the recorded proposal,
   contradictions, and effective tiers.

### Human Test Steps

Recommended (this is the point of the phase). Over a live voice session:

1. Steer the conversation so the agent forms a durable goal from discussion; confirm it is admitted
   and begins shaping later turns.
2. Push the agent toward a goal that contradicts a protected/more-fundamental core goal; confirm
   it declines, and that the decline is grounded in the conflicting goal (visible in the volition
   panel / `inspect_volition_state` and, at the model's discretion, in what it says).
3. Confirm turn latency is unchanged relative to a session with no formation.

## Trace Completeness Contract

Each coherence event writes one `TraceRecord` to `traces.jsonl`. Two operations:

- `goal-coherence-check` — reused from the offline coherence experiment for admission and sweep
  resolution (its fields are authoritative for what the reducer applied).
- `live-goal-formation` — the per-turn / sleep formation event wrapping detection, resolution, and
  any declined-context record.

Required fields for a `live-goal-formation` record (in `details`):

- `trigger` — `live_formation` | `sleep_formation`
- `tick`
- `input_ref` — pointer to the trusted turn (`exchange` + transcript hash) for `live_formation`,
  or the sleep-input-bundle reference for `sleep_formation`
- `cached_prefix_ref` — hash of the goal-set prefix sent to the model (so cache behavior is
  inspectable), or `null` when no model call was made
- `prefix_cache_eligible` — bool: `true` when this turn's prefix hash equals the previous turn's
  (cache-hit-eligible), `false` when a goal-set change invalidated it this turn (prefix re-warmed)
- `judge_ref` — model role + prompt-version id, or `null` for a pre-judge floor rejection
- `proposed_candidate` — `{ goal_id, title, effective_tier, tension_ids }`, or `null` when no goal
  was formed
- `goal_set_snapshot` — evaluated goals as `{ goal_id, effective_tier, status, last_activated_tick }`
- `contradictions` — list of `{ goal_a, goal_b, rationale }`; empty when none
- `hard_tier_floor_rejected` — bool when a candidate was proposed; `null` otherwise
- `resolution` — the `AdmissionResolution` shape from Phase 1 (`judged` or
  `reject_protected_floor`)
- `declined_candidate` — `{ candidate_id, conflicting_goal_id, rationale, tick }` when the
  candidate was rejected; `null` otherwise
- `response_dispatch_ref` — reference to the turn's dispatched `response.create` (exchange +
  request hash); `null` for `sleep_formation`
- `response_dispatched_at` / `formation_started_at` / `formation_completed_at` — timestamps that
  let a reader confirm `formation_started_at >= response_dispatched_at` (the off-hot-path
  guarantee) and measure formation duration; `null` for `sleep_formation`
- `events_emitted` — the emitted `VolitionEvent`s (authoritative record of what the reducer
  applied)
- `goal_status_before` / `goal_status_after` — one object each, keyed by affected goal id
- `artifact_or_record_reference` — a stable pointer back to this event (`trigger` + `tick`)

Required fields for the declined-candidate injection (extending the existing volition
context-injection record with a `coherence` layer):

- `injected_layers` — includes a `coherence` layer naming the carrier and injection point
- `declined_candidates_injected` — the list of `{ candidate_id, conflicting_goal_id }` present in
  the injected context for this turn
- `input_transcript_ref`, `request_hash` — as in the existing `VolitionContextInjected` trace

Artifact boundary:

```text
traces.jsonl (TraceRecord: "live-goal-formation" and "goal-coherence-check"):
  Authoritative record of each formation/coherence event — the model proposal + verdict (or null
  for a pre-judge floor rejection), the deterministic resolution, the declined-candidate record,
  and the events_emitted the reducer applied.

events.jsonl:
  The framework EventType stream (InputReceived, TraceRecorded, ...). A TraceRecorded event links
  back to its trace by trace_id; it does not duplicate the VolitionEvents.

realtime DiagnosticRecords (live/human path only):
  VolitionContextInjected (now carrying the coherence layer) and the bounded-initiative records,
  emitted per trusted turn — the live analogue of the offline traces, not asserted by the
  automated checks.

pure state:
  Changes only through apply() over the events_emitted recorded in traces.jsonl; never holds the
  model output. The DeclinedCandidate list is session-scoped volition state, rebuilt from the
  recorded rejections.
```

Parsing verification:

- Parse `traces.jsonl` and `events.jsonl` from disk, not in-memory structs.
- Assert each admission's `resolution` is recomputable from `{ proposed_candidate, contradictions,
  goal_set_snapshot effective tiers }` — replay determinism over resolution and events for a fixed
  proposal + verdict, not over the model output.
- Assert a rejected candidate produced a `GoalCandidateRejected`, no `GoalCandidateAccepted`, and a
  `declined_candidate` record naming the conflicting goal.
- Assert the declined candidate is model-visible in a `coherence` injection layer from the first
  subsequent turn (not the rejection turn, whose context preceded admission) and stays present.
- Assert `prefix_cache_eligible` is `false` exactly on turns whose recorded goal set changed
  (admission / retirement / sweep) and `true` otherwise, so cache-hit-eligible turns are
  distinguishable from prefix-invalidated turns.
- Assert a formed-but-unadmitted candidate is absent from the arbitration input for its turn.
- Assert every `live_formation` record satisfies `formation_started_at >= response_dispatched_at`
  from its own recorded timestamps (not a self-attesting boolean), and that the turn's
  response-latency observation matches the no-formation baseline within tolerance.
- Assert the sleep sweep followed the same cancel-less-fundamental / floor-never-cancelled rules as
  the offline coherence experiment.

## Expected Output

- A live loop that forms candidate goals from trusted turns, admits the consistent ones, rejects
  the incompatible ones off the hot path, and carries each rejection as durable, model-usable
  session context — with every decision explained by a trace record and its emitted lifecycle
  events, and turn latency unchanged.
- Pure resolution reused unchanged from Phase 1; the new offline harness passes without any real
  model call (scripted judge).

## Results

Pending implementation.
