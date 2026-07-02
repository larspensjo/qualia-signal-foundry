# Plan: Volition Motivational Texture

## Maturity

Candidate. **Detail level: Phase 2 detailed** — Phase 1 (goal coherence under a protected
floor) and Phase 2 (live goal formation and off-hot-path coherence) are both implemented and
summarized below. The remaining phases are sequenced and scoped but not yet specified.
Detailing the next phase is the step after Phase 2's human voice testing is complete.

## Purpose

The realtime volition system is fully built and human-tested: tensions, goals, salience,
arbitration, mode bias, opportunity detection, shaping-intensity dial, bounded initiative in
the live loop, cross-session continuity, and a browser volition panel. See
[Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md) and
[Handoff.Volition.md](../Handoff.Volition.md).

This plan gives volition more **inspectable motivational texture** so the system reads as a
*distinct, motivated agent* — without reopening the evidence-based, anti-anthropomorphic
stance (DecisionLog 2026-05-15, 2026-06-27, 2026-06-30).

The spine of that work is **goal coherence**. The imported brief proposed tagging goals by
owner — user / simulator / shared (§12). That ownership model is **declined**
([DecisionLog 2026-06-30](../DecisionLog.md#2026-06-30---adopted-goals-belong-to-the-simulation-coherence-replaces-goal-provenance)):
every goal the simulation adopts belongs to the simulation, whatever its origin. What makes
it read as a separate agent is not a label but its capacity to **own its goals, keep them
mutually consistent, and decline input that would make it incoherent**. Origin survives only
as an optional background memory/association, never a class of goal. The brief's other three
deferred concepts — emotion-like signals (§8), conscious/subconscious visibility (§6), and
multi-turn Plans (§3.5) — follow, each building on the coherent-agent substrate.

## Guardrails (carry into every phase)

- Project vocabulary stays authoritative; nothing is renamed (reconciliation D1).
- No claim of subjective experience; all new state is inspectable and trace-backed (D2).
- "Emotion" is only ever a named, evidence-derived functional signal — never a felt state,
  never used to confabulate narration (D4).
- New goals cannot enter at or below the protected tier floor. Protected goal *definitions
  and core membership* cannot be formed, edited, replaced, or cancelled at runtime (D3); their
  dynamic state (salience, status) still changes through the normal lifecycle. The
  coherence-specific rule: never cancel a protected goal, never admit into the protected floor.
- Contradiction detection is **model judgment isolated in an adapter**; its verdict is
  recorded as a trace artifact and fed back into the pure reducer as events. The model
  *detects*; the pure reducer *resolves* deterministically
  ([DecisionLog 2026-06-30](../DecisionLog.md#2026-06-30---goal-coherence-is-model-judged-off-the-hot-path-and-repaired-during-sleep),
  [2026-05-09](../DecisionLog.md#2026-05-09---unidirectional-event-reducer-state-flow)).
- Per [Agents.md](../../Agents.md): any phase whose behavior is explained by traces needs a
  trace-completeness contract (required fields, artifact boundary, artifact-parsing
  verification) defined before implementation.

## Phases (in order)

Ordered by increasing cost and decreasing certainty. The coherence engine comes first
because every later concept (an honest conflict signal, subconscious bias, multi-turn plans)
is more legible once goals are a consistent, owned set.

### Phase 1 — Goal coherence under a protected floor (offline engine) — done

The reusable, model-judged coherence engine is built and proven offline. A model *detects*
contradictions (`CoherenceVerdict`) and pure functions *resolve* them deterministically into
the **existing** goal-lifecycle events — `GoalCandidateAccepted` (admit),
`GoalCandidateRejected { reason }` (reject; reason names the conflicting goal), `GoalRetired`
(cancel) — with no new event types. Two triggers share one primitive: **admission** judges
`{existing goals + one candidate}`; the **sweep** judges the whole goal set in one round-trip.
A deterministic hard tier-floor gate rejects any candidate at or below the protected floor
before any model call; the sweep never cancels a floor goal and flags a floor-vs-floor
contradiction for human review rather than auto-resolving it.

**Shipped surface:**
- Pure types and resolution in [`qsf_volition::coherence`](../../crates/qsf_volition/src/coherence.rs)
  (`Contradiction`, `CoherenceVerdict`, `AdmissionResolution`, `SweepResolution`,
  `resolve_admission`, `resolve_sweep`, `candidate_hard_tier_floor_rejected`,
  `resolve_protected_floor_rejection`), plus the public
  `reducer::effective_tier_from_tension_ids` that tiers any `tension_ids` slice against
  `fixture.tensions` (replacing the old fixture-goals-only lookup that mis-tiered candidates as
  `u8::MAX`).
- The `CoherenceJudge` adapter seam in `qsf_app`
  ([coherence_judge.rs](../../crates/qsf_app/src/models/coherence_judge.rs)) with a
  deterministic `ScriptedCoherenceJudge` (default) and a `ModelBackedCoherenceJudge` over
  `ModelRoleId::CoherenceJudge` (real-model opt-in), each validating verdicts against the
  queried goal set.
- The offline `volition-goal-coherence` harness
  ([volition_goal_coherence.rs](../../crates/qsf_app/src/experiments/volition_goal_coherence.rs))
  recording each check as a `goal-coherence-check` trace record.

Validated by
[Experiment.GoalCoherenceUnderProtectedFloor.md](../Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md);
the two 2026-06-30 DecisionLog entries record the durable stance. This is the substrate Phase 2
wires into the live loop. Human testing was not required for Phase 1 (offline, deterministic).

### Phase 2 — Live goal formation and off-hot-path coherence — implemented

Wire the Phase 1 engine into the realtime loop so the simulation **forms its own goals from
live discussion** and can **decline input that would make it incoherent** — the felt,
human-testable expression of the coherent-agent stance. Judging stays off the hot path, so turn
latency is unaffected; a decline becomes durable, inspectable session context that the model may
choose to act on.

**Per-turn loop (off the hot path).** On each trusted turn, *after* the response, a single model
call over a cache-structured prompt does **formation and contradiction detection together**:
- **cacheable prefix:** system instructions + the current goal set (core + accepted candidates).
  It is byte-stable *until the goal set changes* — an admission, retirement, or sweep invalidates
  it and a prefix-hash rule re-warms the cache on the next turn. It is session-scoped (accepted
  candidates differ per session), not identical across sessions;
- **variable suffix:** this turn's user transcript and the assistant's response (optionally a
  small rolling window of recent turns).

The call returns an optional proposed candidate and any contradictions it has with the existing
set. The model only *proposes and detects*; pure `resolve_admission` (Phase 1) decides
admit / reject / cancel by tier and emits the existing lifecycle events. **Admit** → the
candidate becomes a real goal that shapes later turns; **reject** → nothing enters the set and a
declined-candidate record is added to durable session state. A freshly formed candidate is
structurally incapable of shaping turns until admission promotes it, because a merely *pending*
candidate does not participate in arbitration.

**Why one call, every trusted turn, off the hot path (no heuristic gate).** Prompt caching makes
a per-turn model call over a large stable prefix cheap — cache reads bill at ~0.1× base input,
and the goal-set prefix is cached across turns (re-warmed whenever the set changes), so only the
new turn is paid at full price.
Because the call runs after the response, its latency never touches turn responsiveness. Given
both, the simplest uniform design (run every turn) beats a deterministic pre-filter gate, and
folding formation and detection into one call maximizes cache reuse and halves round-trips. A
small, fast model role is appropriate. Recorded in
[DecisionLog 2026-07-01](../DecisionLog.md#2026-07-01---live-goal-formation-and-coherence-detection-run-as-one-cache-structured-model-call-per-turn).

**Declined candidates as durable session context (how the decline is felt).** A rejection
produces a `DeclinedCandidate` record — `{ candidate_id, title, conflicting_goal_id, rationale,
tick }` — held in session-scoped volition state and **injected into the realtime context as its
own coherence layer**, present for the rest of the session. It is evidence-backed (the
conflicting goal id + rationale live in the record), so injecting it is honest state, not
confabulated narration (guardrail D4). The model decides whether and how to voice it; no shaping
rule dictates a line, so there is nothing to nag. Accepted candidates need no special surfacing —
they are ordinary goals that shape turns through the existing selection/arbitration path.

**Sleep pass (whole-history formation + the sweep).** The sleep/consolidation pass — where the
model layer and `CoherenceJudge` already live — performs two coherence operations:
- **whole-history formation:** one deliberate call over the full last session's interaction
  history + the full goal set, to catch durable goals that emerged gradually and that the
  per-turn window missed;
- **the whole-set sweep** (`resolve_sweep`): cancel the less-fundamental goal of any contradicting
  pair for drift that accumulated over the session, never cancelling a floor goal.

**Model access (the enabling refactor).** `qsf_realtime_server` has no model-invocation capability
today and deliberately does not depend on `qsf_app`, while the `CoherenceJudge` and its
`ModelClient` live in `qsf_app`. Lift the model layer — `ModelClient`, `ModelRole` /
`ModelRoleId`, `invoke_model_role`, and the `CoherenceJudge` — into a lower shared crate that
*both* `qsf_app` and `qsf_realtime_server` depend on, rather than coupling the realtime server to
all of `qsf_app`. **Requirement / first risk to resolve:** the `ModelClient` boundary must expose
a stable-prefix / cache-breakpoint boundary (Claude `cache_control`) so the cached goal-set prefix
actually caches. Today `ModelRequest` carries no cache-breakpoint field — it only reads back
provider `cached_input_tokens` *after* the call — so adding that seam is the first implementation
step, before wiring the per-turn call.

**New state / types:**
- a `DeclinedCandidate` record and a session-scoped list of them in the realtime volition state;
- a combined formation-and-detection output (proposed candidate + contradictions) — either an
  extension of the `CoherenceJudge` seam or a sibling proposer role sharing the same cached
  goal-set prefix. Detection still returns a `CoherenceVerdict` so Phase 1's pure resolution is
  reused unchanged.

**Attach points:**
- `events_for_trusted_transcript` / `apply_trusted_transcript_to_volition`
  ([volition.rs](../../crates/qsf_realtime_server/src/realtime/volition.rs),
  [sideband.rs](../../crates/qsf_realtime_server/src/realtime/sideband.rs)) stay unchanged — the
  existing deterministic transcript→volition mapping runs **before** the response is built and
  must not carry the model call;
- a **new explicit post-response hook** in the sideband, invoked after `response.create` is
  dispatched (a background/off-turn task), that runs the per-turn formation + admission call. This
  is the guarantee point for "off the hot path," and it is the hook the experiment verifies — not
  the pre-response transcript mapper;
- the volition context-injection builders
  ([volition_injection.rs](../../crates/qsf_realtime_server/src/realtime/volition_injection.rs)) —
  the declined-candidate coherence layer, injected from the next turn onward (a rejection cannot
  reach the same turn's context, which is already sent);
- the sleep/consolidation pass in `qsf_app` (`crates/qsf_app/src/sleep/`) — whole-history
  formation + the sweep;
- the extracted shared model crate — the relocated `ModelClient` / `CoherenceJudge`.

**Open questions:**
- exact placement of the declined-candidate layer (a dedicated session-scoped coherence layer,
  present all session, is the working answer);
- how much rolling-window context the per-turn formation call includes;
- whether declined goals should later persist cross-session via the continuity snapshot as an
  origin-association (a later refinement, out of scope for this phase).

**Verification:** offline
[Experiment.LiveGoalFormationAndCoherence.md](../Experiments/Experiment.LiveGoalFormationAndCoherence.md)
— see its trace-completeness contract. A deterministic scripted judge drives the form → detect →
resolve → inject pipeline and the sleep formation + sweep; the harness parses artifacts and asserts
each decision (admit / reject / cancel / declined-context injection) is reconstructable from the
trace alone. **Human voice testing is recommended** here (it is the point of the phase): the agent
forms a goal from discussion, keeps it when consistent, and declines it when it contradicts the
core, with the decline available as session context it can act on.

**Shipped surface:**
- The model layer — `ModelClient`, `ModelRequest`/`ModelMessage` (with a
  `stable_prefix_message_count` / `stable_prefix_hash` cache-boundary seam), `ModelRole`/
  `ModelRoleId`, `CoherenceJudge`, and `invoke_model` — is extracted from `qsf_app` into a new
  shared [`qsf_models`](../../crates/qsf_models/src/lib.rs) crate that both `qsf_app` and
  `qsf_realtime_server` depend on. A `ModelInvoker` trait decouples model callers from any one
  observability backend: `qsf_app`'s `RunContext` implements it via the existing
  `invoke_model_role` (unchanged behavior for all prior offline callers); the realtime loop uses
  `DirectModelInvoker` and records its own diagnostic.
- The cache-breakpoint requirement (D6) resolves as an **application-level** stable-prefix
  marker, not a provider request field — neither `openai_provider_kit` nor the raw OpenAI Chat
  Completions API expose a `cache_control`-style breakpoint (confirmed by inspection); OpenAI's
  own prompt caching is automatic over a byte-stable prefix. See the 2026-07-01 DecisionLog
  addendum.
- The combined formation-and-detection judge —
  [`LiveGoalFormationJudge`](../../crates/qsf_models/src/live_goal_formation.rs)
  (`ScriptedLiveGoalFormationJudge` default, `ModelBackedLiveGoalFormationJudge` real-model
  opt-in) — proposes an optional `ProposedGoalCandidate` and a `CoherenceVerdict` in one call,
  reusing Phase 1's `resolve_admission` / `candidate_hard_tier_floor_rejected` /
  `resolve_protected_floor_rejection` unchanged.
- The realtime post-response hook
  ([`live_goal_formation.rs`](../../crates/qsf_realtime_server/src/realtime/live_goal_formation.rs))
  fires once per trusted turn after `response.create` is dispatched, via
  `tokio::task::spawn_blocking` (since `ModelClient::complete` is a blocking call), so it never
  delays turn completion; it records a `DiagnosticRecord::LiveGoalFormationPerformed` carrying
  the live analogue of the trace contract below.
- A rejection is recorded as a `DeclinedCandidate` on `VolitionRuntimeState`
  (`crates/qsf_realtime_server/src/realtime/volition.rs`) and injected as a new `coherence` layer
  in the volition turn packet
  (`crates/qsf_realtime_server/src/realtime/volition_injection.rs`), present from the turn after
  the rejection onward — the rejection turn's own context already predates admission.
- The offline harness `live-goal-formation-and-coherence`
  (`crates/qsf_app/src/experiments/live_goal_formation_and_coherence.rs`) exercises admit /
  reject-with-decline / no-goal-formed, the declined-candidate injection-ordering invariant, the
  pending-candidate-not-selectable invariant, sleep whole-history formation, and the sleep sweep,
  satisfying the trace-completeness contract below with a deterministic scripted judge.

**Open item:** human voice testing (the Human Test Steps in
[Experiment.LiveGoalFormationAndCoherence.md](../Experiments/Experiment.LiveGoalFormationAndCoherence.md))
has not yet been run.

### Phase 3 — Emotion-like signals, visualization-first (brief §8)

Derive named functional signals from existing goal/delta state per reconciliation D4
(frustration = repeatedly `Blocked`; satisfaction = `GoalSatisfied` + `EvidenceRef`; tension =
unresolved conflict; etc.). Pure derivations over recorded state — no new mutable emotion
object.

- **Scope discipline:** **visualization only at first** — no arbitration feedback. Gated.
- **Natural source of the `tension` signal:** the coherence engine's detected contradictions,
  rejections, and cancellations (Phases 1–2) — an evidence-backed conflict signal rather than a
  narrated one.
- **Attach point:** derived at the salience/initiative layer; surfaced in the existing browser
  volition panel / brain-state surface
  ([Design.LiveActivationDashboard.md](Design.LiveActivationDashboard.md)).
- **Open question:** which signals earn a place first; whether any later feed arbitration.
- **Verification:** Experiment scaffold asserting each signal derives only from recorded state.

### Phase 4 — Conscious / subconscious visibility (brief §6)

A visibility attribute on goal selection: a "subconscious" goal biases salience/arbitration but
surfaces only on introspection or forced conflict. Partly latent already in the sideband
surfacing gate + anti-nag wiring.

- **Resolution leaning:** treat as an introspection-*surfacing filter*, not a separate runtime
  path (the reconciliation's open question).
- **Attach point:** the selection/inspection layer (`build_state_inspection`) + surfacing gate.
- **Verification:** Experiment scaffold over what surfaces vs what only biases.

### Phase 5 — Multi-turn Plans (brief §3.5, §4.6)

A genuinely new domain structure: a `Plan` sequencing initiatives across turns with
suspend / resume / abandon. The current system is single-turn initiative.

- **Cost note:** largest new structure; most likely to feel mechanical. Deferred last
  deliberately — revisit need after earlier phases add texture, and prove offline before the
  live loop.
- **Verification:** offline Experiment scaffold over the plan lifecycle before any live wiring.

## Documents to update (per ProjectWorkflow.md)

- **Done at Phase 1 detailing/ship:** the coherence commitment (single ownership +
  belief-coherence invariant; model-judged detection off the hot path + sleep sweep) is recorded
  in [DecisionLog.md](../DecisionLog.md) (two 2026-06-30 entries), the coherent-agent stance is in
  [ProjectVision.md](../ProjectFrame/ProjectVision.md), and the offline engine is validated by
  [Experiment.GoalCoherenceUnderProtectedFloor.md](../Experiments/Experiment.GoalCoherenceUnderProtectedFloor.md).
- **Done at Phase 2 detailing:** the per-turn cache-structured formation+detection cadence and the
  model-layer extraction rationale are recorded in [DecisionLog.md](../DecisionLog.md) (2026-07-01
  entry); the live pipeline's `Experiment.*.md` scaffold and trace contract are
  [Experiment.LiveGoalFormationAndCoherence.md](../Experiments/Experiment.LiveGoalFormationAndCoherence.md).
- When a later phase is detailed: write its `Experiment.*.md` scaffold and trace contract.
- On implementing a phase: refresh the Implementation Status of
  [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md). Phase 2 adds
  the shared model crate and `CoherenceJudge` reuse in the realtime server, live formation +
  off-hot-path admission, the declined-candidate injection layer, and the sleep formation + sweep.
- As brief concepts land in project docs, retire the corresponding brief sections: §12 is retired
  as **not-adopted** (ownership declined); §11 is **delivered** through coherence. Delete the brief
  once nothing in it remains unmerged.
