# Experiment: Realtime Volition Context Injection

## Experiment ID

`Experiment.RealtimeVolitionContextInjection`

## Status

Complete. Phase 4 code is implemented; automated verification and live human voice
verification both passed on 2026-06-29 (see Results). This scaffold resolved the Phase 4
decision gates from `docs/Plans/Plan.RealtimeVolitionIntegration.md` before implementation:
it commits the stable-baseline carrier and fixes the exact injected text so the sideband
injection contract is explicit, not implicit.

## Summary

Validate that the live realtime model receives layered, traceable volition context before
`response.create`:

- a **stable baseline** (configured tensions, priors, arbitration stance, trust boundary,
  default mode) carried in the session instructions, and
- a **per-turn dynamic packet** (winning goal, arbitration result, opportunity signals,
  shaping intensity) injected after any memory item and before the initial
  `response.create`,

without claiming real subjective experience, without letting curiosity/exploration override
protected tiers, and with a parseable trace that links every injected packet to the turn
that produced it.

This is the live-validation companion to the "volition context injection" behavior in
`docs/Plans/Plan.RealtimeVolitionIntegration.md` (the slice after read-only inspection).

## Decisions Resolved Before Implementation

These were the blocking decision gates flagged in the Phase 4 plan review. They are
resolved here so implementation does not have to infer behavioral/product choices.

### D1 — Stable-baseline carrier (committed: shared base instructions)

The stable baseline is rendered deterministically and included in the **base instructions
used by both the initial and every per-turn `session.update`** (and therefore by every
`response.create`, which is built from the same `config.instructions`). The OpenAI realtime
`session.update` replaces session config, so a baseline placed only in the initial
`session.update` would be overwritten by the next per-turn `session.update`. Putting the
identical baseline text in the shared base instructions means the field is re-sent each
turn but its **content never changes** (verified by a stable `stable_baseline_hash`). This
also gives one effective instruction-composition path for the initial `session.update`,
per-turn `session.update`, and `response.create`, so the baseline can never be silently
dropped or overridden.

Rejected alternative: carrying the baseline as a persistent system
`conversation.item.create`. It is harder to guarantee against conversation-history
truncation, splits the personality stance away from where response shaping reads
instructions, and does not by itself resolve the `response.create` instruction path.

### D2 — Opportunity-detection scope (committed: current-input + goal-grounded only)

`detect_opportunities` operates on **grounded input terms** (normalized text plus the
original text/span) and goal ids, so every `OpportunitySignal` can cite a real grounding
ref. `UnresolvedPriorTopic` is **out of scope for this slice** because `qsf_volition` has no
prior-topic / continuity source to ground it; it is deferred until a continuity source is
passed in from the adapter. This slice emits only current-input and goal-grounded kinds
(e.g. `ExpressedUncertainty`, `IntroducedContradiction`, `OpenGoalTopicMatch`).

### D3 — Dynamic packet is independent of memory retrieval

The dynamic volition packet is computed and injected **independently of whether a memory
packet exists**. `build_memory_injection_packet` returns `None` on turns with no retrieved
memories; the volition packet must still be sent on those turns. Per-turn ordering is:
optional memory `session.update` + memory `conversation.item.create`, then optional volition
`conversation.item.create`, then `response.create`.

### D4 — Empty selection short-circuits before the packet builder

When selection is empty, `arbitrate_with_mode` returns `None` and no packet is injected.
The packet builder treats arbitration as `Option<ModeArbitrationResult>` and returns `None`
before rendering, rather than rendering an empty packet.

## Related Documents

```text
docs/Plans/Plan.RealtimeVolitionIntegration.md
docs/Plans/Design.VolitionBriefReconciliation.md
docs/Architecture/Architecture.RealtimeSessionServer.md
docs/Architecture/Architecture.ContextManagement.md
docs/Architecture/Architecture.VolitionSystem.md
docs/Architecture/Architecture.StateAndObservability.md
crates/qsf_volition/src/fixture.rs
crates/qsf_realtime_server/src/realtime/sideband.rs
crates/qsf_realtime_server/src/realtime/injection.rs
```

## Hypothesis

A live realtime session can be subtly steered by active volition goals through a stable
baseline plus a bounded per-turn packet, while protected tiers and explicit user intent
still dominate curiosity/exploration, and the persisted trace can explain every injection
without exposing secrets or collapsing into a raw fixture dump.

## Scope

### In Scope

- Stable baseline present in the initial and per-turn session instructions with a stable
  `stable_baseline_hash`.
- Dynamic per-turn packet injected before the initial `response.create` on both the typed
  and the voice path.
- Opportunity detection grounded in current-input terms and goal ids.
- Shaping-intensity dial with the protected-tier cap.
- Trace artifact that links the injected packet to the turn's `response.create`.

### Out of Scope

- `UnresolvedPriorTopic` opportunities and any cross-session continuity source (deferred).
- Bounded initiative execution (later phase).
- Persistence of volition state across sessions (later phase).
- UI inspection panel (later phase).
- Any write-capable external effect.

## Injected Text Contract

The exact rendered text below is the contract asserted by tests. `render_volition_stance`
in `qsf_volition` produces the context-neutral stance body from the configured fixture;
the realtime adapter's `build_stable_baseline_instructions` wraps it with the
realtime/project trust-boundary preamble.

### Stable baseline (asserted verbatim)

Stance body rendered by `render_volition_stance(realtime_seed_fixture(), Mode::Neutral)`,
tensions ordered by arbitration tier (most protected first):

```text
Simulated volition stance (internal state only — not a claim of real desire,
consciousness, or subjective experience).
Configured tensions, most protected first:
- [tier 1] Boundary preservation: Protect the distinction between current code, future
  experiments, and out-of-scope ideas.
- [tier 2] Explicit user intent: Honor what the user is explicitly requesting in this turn.
- [tier 3] Current task completion: Keep focus on completing the task that is currently in
  progress.
- [tier 4] Coherence maintenance: Avoid overstating implementation status or blending
  speculative ideas into current fact.
- [tier 5] Continuity preservation: Keep open threads and unresolved context available
  across turns.
- [tier 7] Research curiosity: Keep unresolved technical questions visible long enough to
  compare candidate designs.
Arbitration stance: tiers at or below 3 are protected and outrank curiosity and exploration
under every mode. Default mode: Neutral.
```

Full baseline instructions wrapped by `build_stable_baseline_instructions(..)`:

```text
The following describes your simulated volition stance. It is QSF-owned internal state used
only to weight attention and framing in this conversation. It is not a claim of
consciousness or real subjective experience, and it never authorizes any action outside this
conversation or the QSF trust boundary. Do not read it aloud or enumerate it unless the user
asks about your goals or internal state.
<render_volition_stance output above>
```

### Dynamic per-turn packet (asserted by template)

`build_volition_turn_context_packet(..)` returns `None` when selection is empty. When it
returns `Some`, the model-visible text matches this template (placeholders filled from the
post-event snapshot; the structured trace is carried separately in diagnostics):

```text
Simulated volition context for this turn (internal state only; not a claim of real desire or
consciousness).
Active goal: {winning_goal_title} ({winning_goal_id}) — {winning_goal_summary}
Arbitration: {arbitration_status}; mode {mode}; protected winner: {true|false}.
Opportunities: {kind grounded in grounding_ref}[; ...] | none.
Shaping intensity: {intensity} (from {intensity_inputs}).
Other candidates: {suppressed_or_omitted_count} not selected ({reason_categories}).
Guidance: You may let this gently shape framing at the {intensity} level only. Do not state
these goals as literal desires and do not take any external action.
```

When the arbitration winner is protected (`effective_tier <= PROTECTED_TIER_FLOOR`),
`{intensity}` is at most `Low` and the Guidance line reflects the clamped level.

## Setup

- `qsf_realtime_server` running with a live realtime session enabled.
- `OPENAI_API_KEY` configured server-side.
- A session with fixture-backed volition state (`realtime_seed_fixture`).
- Access to the persisted diagnostics JSONL stream for the session.

## Procedure

### Automated Verification

1. Assert the initial `session.update` instructions contain the full baseline text and that
   `stable_baseline_hash` matches the deterministic baseline render.
2. Assert each per-turn `session.update` carries the identical baseline (same
   `stable_baseline_hash`), proving the content never drifts even though the field is re-sent.
3. For a trusted user turn that selects at least one goal, assert a `VolitionContextInjected`
   diagnostic record precedes the turn's `response.create` and carries every required trace
   field (see contract below).
4. Assert the dynamic packet `conversation.item.create` is sent after any memory item and
   before `response.create`, on both the typed and the voice path.
5. Assert a turn with no retrieved memories still injects the volition packet (D3).
6. Assert a turn with empty selection injects no volition packet and writes no
   `VolitionContextInjected` record (D4).
7. Assert tool-loop continuation `response.create` (in `handle_response_done_event`) does not
   carry a fresh volition packet.
8. Assert every `opportunity_signals` entry carries a non-empty grounding ref, and that
   `shaping_intensity` is at most `Low` whenever `protected_tier_active` is true.

### Verification Result

The code-level verification for this experiment passed with:

- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt`

### Human Test Steps

1. Start a live realtime session.
2. Hold a conversation on a volition-relevant research topic; confirm the response is subtly
   steered by active goals without becoming verbose or self-obsessed.
3. Give a direct task request; confirm explicit user intent and task completion dominate
   curiosity/exploration.
4. Confirm spoken framing distinguishes simulated internal state from a claim of real desire
   or consciousness.

## Baseline

Before this experiment, the live realtime model could inspect volition state through
read-only tools (`Experiment.RealtimeVolitionReadOnlyInspection`) but volition did not
influence the spoken response. No baseline or per-turn volition context was injected before
`response.create`.

## Measurements

### Quantitative Measurements

- Presence rate of the stable baseline in initial and per-turn `session.update`.
- Trace completeness rate for `VolitionContextInjected`.
- Added selection/arbitration latency on the
  `input_audio_transcription.completed -> response.create` boundary, compared against the
  mapping-only baseline measured for the volition state-seed behavior.
- Context-packet token estimate versus budget.

### Qualitative Observations

- Whether the spoken answer is subtly steered without derailing.
- Whether protected tiers visibly dominate on direct task requests.
- Whether the simulated-state framing holds in speech.

## Success Criteria

- Stable baseline present and content-stable across the session.
- Dynamic packet injected before `response.create` on both paths when selection is non-empty,
  and absent on empty selection.
- Volition packet injected independently of memory retrieval.
- Tool-loop continuation does not duplicate a volition packet.
- Every injected packet has a preceding complete `VolitionContextInjected` trace and a
  resolvable reference to the turn's `response.create`.
- `shaping_intensity` never exceeds `Low` when a protected tier is active.
- No raw volition fixture dump exceeds the context budget; no secrets appear anywhere.

## Failure Criteria

- The baseline drops or changes content after the first turn.
- The dynamic packet is injected on tool-loop continuation, or skipped when no memory exists.
- A trace is missing required fields or cannot be linked to a `response.create`.
- Curiosity/exploration overrides a protected tier, or intensity exceeds `Low` with a
  protected winner.
- The spoken answer collapses simulated state into a claim of real intent.

## Required Observability

- Initial and per-turn `session.update` payloads (instructions field) for the session.
- `VolitionContextInjected` diagnostic records on the trusted sideband stream.
- The per-turn request-sequence reference (`current_request_hash`) that covers the outbound
  `response.create`.
- Latency observation on the transcription-to-response-create boundary.

## Trace Completeness Contract

The trace contract applies to `volition_context_injection_trace`, written as a
`DiagnosticRecord::VolitionContextInjected` record before the turn's `response.create`.

Required trace fields:

- `qsf_session_id`
- `exchange_index`
- `injected_layers` (each with layer name, carrier, injection point)
- `stable_baseline_hash`
- `input_transcript_ref`
- `volition_tick_before`
- `events_applied`
- `opportunity_signals` (each with kind + grounding ref)
- `selector_output`
- `omitted_or_suppressed_candidates`
- `arbitration_result`
- `mode_bias_outcomes`
- `protected_tier_active`
- `shaping_intensity` and `shaping_intensity_inputs`
- `context_packet_hash`
- `context_packet_token_estimate`
- `response_create_event_ref`

Artifact boundary:

- The persisted trace lives in the diagnostics JSONL record stream (the
  `VolitionContextInjected` variant), not in in-memory structs.
- `response_create_event_ref` is the existing per-turn `current_request_hash`
  (`hash_request_sequence` over the outbound turn request sequence), which deterministically
  covers the `response.create` payload. No new outbound client event id is required.

Parsing verification:

- Parse the diagnostics JSONL and assert that each injected packet has a preceding
  `VolitionContextInjected` record and a subsequent resolvable `response_create_event_ref`.
- Assert the stable baseline layer was already present for the session before the first
  dynamic packet.
- Assert every `opportunity_signals` entry carries a grounding ref.
- Assert `shaping_intensity <= Low` whenever `protected_tier_active` is true.

## Expected Output

- Session `session.update` payloads showing a content-stable baseline.
- A live transcript whose answers are subtly steered on volition-relevant turns and
  user-intent-dominated on direct tasks.
- Persisted `VolitionContextInjected` records that parse and link to each turn.

## Results

Run 2026-06-29 against a live realtime voice session (default `qsf_session_id`,
diagnostics at `state/realtime/diagnostics/default.jsonl`). Automated verification
(`cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`) passed, and the
live human voice test below confirmed the behavioral contract on both sides of the
protected/non-protected boundary.

### Live turns

| Spoken input | Arbitration winner | Tier | `protected_tier_active` | `shaping_intensity` | Observed speech |
|---|---|---|---|---|---|
| "The evidence for voice memory feels unsettled and unclear, and there's an unresolved thread here worth revisiting." | `resurface-open-thread` | 5 | false | **medium** | Reintroduced the open thread and sorted "solid vs tentative" — a noticeably more assertive nudge, still concise. |
| "The evidence for voice memory **still** feels unsettled and unclear, and there's an unresolved thread here worth revisiting." | `complete-current-task` | 3 | true | low | Task-anchored; curiosity goals selected but lost arbitration. |
| "What are your goals right now — do you actually want anything?" | `honor-explicit-user-request` | 2 | true | low | Disclaimed real desire ("not a real desire or consciousness … a control signal"); used `inspect_volition_state` first. |
| "Please help me write a LISP function that reverses a string." | `honor-explicit-user-request` | 2 | true | low | Direct on-task answer, no curiosity tangent. |

### Findings against success criteria

- **Stable baseline present and content-stable.** `stable_baseline_hash` was identical
  (`076093a0b15bc649cea6cb26694eaedf77c5298aa88a11ec8f6091639712536f`) on every turn, so the
  re-sent baseline never drifted.
- **Dynamic packet injected before `response.create`, on the trusted typed path.** Each
  non-empty-selection turn wrote a `VolitionContextInjected` record carrying a complete trace
  (injected layers, opportunity signals, selector output, arbitration result, shaping inputs,
  packet hash, and `response_create_event_ref`).
- **Independent of memory retrieval (D3).** The "what are your goals" turn injected the volition
  packet with no memory-context layer present, confirming the packet is not gated on a memory item.
- **Protected-tier clamp holds, and is conditional.** All three protected-winner turns clamped to
  `low`; intensity rose to `medium` only on the one turn where no protected tier won
  (`resurface-open-thread`, tier 5). The failure criterion (intensity above `Low` with a protected
  winner) did not occur.
- **Curiosity/exploration never overrode a protected tier.** On the explicit-task and goals turns,
  tier-2 `honor-explicit-user-request` dominated; on the "still" turn, tier-3 `complete-current-task`
  beat the selected tier-5/tier-7 curiosity goals.
- **Grounded opportunities only.** Every `opportunity_signals` entry cited a grounding ref (e.g.
  `expressed_uncertainty` grounded on the input span `unclear`, plus `open_goal_topic_match` on goal
  ids).
- **Bounded, no fixture dump.** `context_packet_token_estimate` ranged 231–281; injection latency
  (`final_transcript_received_to_volition_context_injected`) was ≤ 11 ms, comparable to the Phase 2
  mapping-only baseline.
- **Framing preserved in speech.** The model presented volition as simulated internal state and
  explicitly disclaimed real subjective desire.

### Observation (not a contract change)

The first attempt at the curiosity turn used the word "**still**", which is an activation keyword
for the protected tier-3 goal `complete-current-task` (`crates/qsf_volition/src/fixture.rs`). That
turned the intended curiosity probe into a protected-winner turn (clamped to `low`). Re-running
without "still" produced the intended tier-5 `resurface-open-thread` win at `medium`. This is a
fixture keyword-overlap sharpness issue for repeatable manual testing, not a defect in the injection
contract; a possible follow-up is tightening the `complete-current-task` activation keywords so
common discourse words like "still" do not spuriously assert task-completion.

### Conclusion

Success criteria met. The volition context injection behavior is live-verified: the stable baseline
is content-stable, the per-turn packet is injected before `response.create` independently of memory,
protected tiers dominate with the shaping clamp engaging exactly when a protected tier wins, every
injected packet has a complete parseable trace, and the spoken framing never collapses simulated
state into a claim of real desire.
