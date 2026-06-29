# Design: Reconciling the External Volition Brief

## Status

Candidate — reconciliation note supporting
[Plan.VolitionGoalSystem.md](Plan.VolitionGoalSystem.md) and
[Plan.RealtimeVolitionIntegration.md](Plan.RealtimeVolitionIntegration.md). D1–D4 below are
translation guidance for the temporary external brief, **not** new project rules: they are
reflected inline in the brief itself and are **not** promoted to the decision log. The durable
evidence-based stance they rest on already lives there
([DecisionLog.md](../DecisionLog.md), 2026-05-15 and 2026-06-27). The durable outputs of this
note are the realtime-roadmap adaptations, to be folded into the plans. Maturity per
[DocumentStatus.md](../ProjectFrame/DocumentStatus.md).

## Context

An external design brief —
[volition_goal_system_design_brief.md](../volition_goal_system_design_brief.md) — was
authored outside the project ("research from elsewhere") and imported. It proposes a
"volition and goal system" for a consciousness simulator.

The project already has a committed, partly-built volition system:

- The pure [qsf_volition](../../crates/qsf_volition/src/lib.rs) crate and
  [Architecture.VolitionSystem.md](../Architecture/Architecture.VolitionSystem.md).
- A completed offline build plan
  ([Plan.VolitionGoalSystem.md](Plan.VolitionGoalSystem.md)): static fixture + selection,
  trace-backed initiative, salience/satisfaction/blocking/cooldown, deterministic
  arbitration, reflection-generated goal candidates, bounded initiative execution, and
  mode/bias.
- An in-flight realtime integration
  ([Plan.RealtimeVolitionIntegration.md](Plan.RealtimeVolitionIntegration.md)): Phases 1–3
  (crate extraction, per-session `VolitionRuntimeState`, read-only inspection tools) done;
  behavioral influence, bounded initiative in the live loop, persistence, and UI not yet
  started.

The brief overlaps heavily in substance but frames volition more anthropomorphically than
the project does. The project committed on **2026-05-15** that volition is "an inspectable
selection mechanism," that "human biological drives… are not the right default," and (in
[NonGoals.md](../ProjectFrame/NonGoals.md) and
[Idea.VolitionGoalSystem.md](Idea.VolitionGoalSystem.md)) that mood-like state, if
introduced, must be "an inspectable bias vector… not free-form simulated emotion." The
brief instead opens with "simulate human-like motivation," adds an explicit Personality
layer, and treats emotions (attachment, boredom, frustration) as first-class signals.

This note fixes the framing decision, maps the brief's vocabulary onto the project's,
classifies each brief concept (already-built / adopt-translated / defer), and records the
impact on the in-flight realtime integration. It does **not** introduce new code.

## Decisions

### D1: Project vocabulary is authoritative; the brief is translated into it

`Tension`, `Goal`, `InitiativeProposal`/`InitiativeOutput`, `Mode`, `arbitration_tier`, and
the salience lifecycle keep their existing names and meanings. The brief's terms are adapted
onto these (see the mapping table); no existing type or concept is renamed.

Rationale: the existing names are on the record (DecisionLog 2026-05-15, 2026-06-26,
2026-06-27) and baked into the crate. Renaming to match an external document would churn
code, tests, and decisions for no behavioral gain. The external brief is the lower-authority
document here (an imported idea, not a project decision) per the
[DocumentStatus.md](../ProjectFrame/DocumentStatus.md) authority ranking.

### D2: Keep the evidence-based stance; do not reopen the anti-anthropomorphic decision

The 2026-05-15 decision and the [NonGoals.md](../ProjectFrame/NonGoals.md) stance stand. The
brief's "human-like motivation," Personality layer, and free-form emotion are adopted only
in **translated, evidence-derived, inspectable** form (D3, D4). The system continues to make
no claim of subjective experience, and narration stays trace-backed rather than confabulated
(per the Idea doc's "Trace-Backed Narration").

Rationale: the brief is a source of structural ideas, not a mandate to change project
framing. Adopting its anthropomorphic surface would contradict committed boundaries and the
project's research identity (study consciousness-*like* structure, not assert inner life).
Translation captures the useful behavior without importing the overreach.

### D3: "Personality" maps to the declared tension set and its priors — not a new invented layer

The brief's Personality layer (§3.1) is expressed by **which tensions exist and their
declared priors**: the fixture's tension set, each tension's `priority_bias` and
`arbitration_tier`, and the `Mode` bias vectors. These are inspectable and immutable at
runtime. There is no separate "personality" object that invents desires; a stable
disposition *is* the configured tensions and their weights.

Rationale: this satisfies the brief's intent (a stable layer above goals that biases what the
system cares about) using mechanisms that already exist and are trace-backed, avoiding the
"personality layer that invents desires with no traceable state" the Idea doc rules out.

### D4: "Emotion" maps to named, evidence-derived functional signals — not felt states

The brief's emotion signals (§8) are adopted, if at all, as **optional derived signals
computed from existing goal/delta state**, each with a functional definition and a trace:

| Brief signal | Functional, evidence-derived reading |
|---|---|
| Frustration | a goal repeatedly `Blocked` despite activation |
| Satisfaction | `GoalSatisfied` with a recorded `EvidenceRef` |
| Curiosity | an `Active` research-curiosity goal with an open delta |
| Tension/conflict | an unresolved arbitration conflict among selected goals |
| Boredom | low selected-goal salience across recent ticks |
| Attachment | a goal with a high `reinforcement_count` over many sessions |

These may bias arbitration/salience or drive visualization, but they are never claimed as
subjective experience and never used to invent a motive that the trace does not support. The
brief's own "functional meaning" column (§8.1) already supports this reading. Which signals
(if any) earn a place is deferred to a dedicated slice (see Open Questions).

## Concept Disposition

Mapping each brief concept onto the project. **Built** = already implemented; **Adopt
(translated)** = take the idea in project idiom; **Defer** = genuinely new scope to schedule
later.

| Brief concept (§) | Project term / mechanism | Disposition |
|---|---|---|
| Personality (§3.1) | Tension set + `priority_bias`/`arbitration_tier` + `Mode` (D3) | Adopt (translated) — mostly built |
| Drives (§3.2) | `Tension` | Built |
| Goals (§3.3) | `Goal` | Built |
| Intentions (§3.4) | `InitiativeProposal` / `InitiativeOutput` | Built |
| Plans (§3.5, §4.6) | multi-turn initiative sequence | **Defer** — new |
| Notice opportunities (§4.1) | opportunity-detection step before selection | **Defer** — design into behavioral-influence phase |
| Choose / maintain preferences (§4.2–4.3) | salience scoring + arbitration | Built |
| Initiate topics / resist-redirect (§4.4–4.5) | bounded initiative + context injection in the live loop | **Defer** — realtime behavioral influence (not started) |
| Unfinished business (§4.7) | `Blocked`/open-thread goals + cross-session persistence | Partially built; persistence **deferred** |
| Conscious vs subconscious (§6) | goal-visibility attribute (surfaced only on introspection) | **Defer** — new |
| World model / desired state / delta (§7) | world-model→delta→initiative loop (Idea doc) | Built (compact form) |
| Emotion as signal + visualization (§8) | derived functional signals (D4) + brain-state UI | **Defer** — new, gated |
| Memory→goal formation (§9) | `propose_goal_candidates` from open questions | Built |
| Goal lifecycle (§10) | `GoalStatus` (Proposed…Retired) + decay/cooldown | Built |
| Conflict between goals (§11) | `arbitrate_with_mode` + tiers | Built |
| User vs simulator vs shared goals (§12) | goal-provenance/ownership tag | **Defer** — new |
| Introspection (§13) | `build_state_inspection` + read-only realtime tools | Built |
| Conversational control policy (§14) | shaping-intensity dial on context injection | **Defer** — design into behavioral-influence phase |
| Idle-time behavior (§15) | sleep/consolidation pass | Partially built (SleepPhase); volition re-ranking deferred |
| External actions (§17) | out of scope by committed boundary | Reject for now (matches NonGoals) |
| Safety/control (§18) | `boundary-preservation` tension + protected tiers + read-only | Built |

## Impact on the Realtime Volition Integration

**Verdict: continue it. The in-flight realtime work is the substrate the brief presupposes,
not wasted effort.** The brief validates the trajectory and raises the stakes of finishing
it: today the live system can *inspect* goals but cannot yet *act* on them, and everything
that makes the brief compelling lives on the far side of the behavioral-influence phase.

Phase-by-phase against the brief:

- Per-session `VolitionRuntimeState` (done) — required by the brief's world-model/delta loop
  (§7).
- Read-only inspection tools (done, human-validation pending) — exactly what §13 asks for.
- Inject volition context into live response (not started) — **the brief's core thesis**
  (§4, §5, §14); conversational shaping is impossible until this lands.
- Bounded initiative in the live loop (not started) — needed for "initiate topics" (§4.4)
  and "unfinished business" (§4.7).
- Persist/consolidate across sessions (not started) — needed for persistence (§10.5) and
  idle-time (§15).
- Volition UI (not started) — extends to emotional visualization (§8.2).

Two adaptations the brief motivates:

- **Adaptation A — pull a minimal cross-session continuity slice earlier.** The brief's most
  emphasized behavior is *persistent unfinished business* ("I remember what I wanted to
  ask," §4.7/§9.2/§10.5). Per-session-only volition cannot produce it. Decide deliberately
  whether a thin continuity slice should precede the later persistence phase rather than
  defaulting to last.
- **Adaptation B — design the behavioral-influence phase around the brief's control policy.**
  Before that phase is expanded into steps, fold in **opportunity detection** (§4.1) and a
  **conversational-intensity dial** (§14, low/medium/high shaping) so "how strongly may I
  shape this turn" is a first-class, traceable input — not a retrofit.

These are refinements to the existing plan, not a redirection. No part of Phases 1–3 is
unwound.

## Where the Genuinely-New Concepts Attach

- **Multi-turn Plans (§3.5):** a new offline slice in
  [Plan.VolitionGoalSystem.md](Plan.VolitionGoalSystem.md) — a `Plan` that sequences
  initiatives across turns with suspend/resume/abandon — proven offline before reaching the
  live loop.
- **Conscious vs subconscious (§6):** a visibility attribute on `Goal`/selection; a
  "subconscious" goal influences salience/bias but is surfaced only when introspection asks
  or a conflict forces it. Attaches to the selection/inspection layer.
- **User vs simulator vs shared goals (§12):** a provenance/ownership tag on `Goal`. The
  realtime sideband already maps trusted transcripts to volition events; that mapping is
  where a user-originated goal would be tagged. Attaches to the behavioral-influence phase.
- **Emotion-like signals + visualization (§8):** derived signals (D4) computed at the
  salience/initiative layer; visualization reuses the existing brain-state surface
  ([Design.LiveActivationDashboard.md](Design.LiveActivationDashboard.md)) at the volition-UI
  phase.

## Open Questions

- Does "personality" ever need a representation beyond "the configured tension set + Mode,"
  or is that sufficient indefinitely? (Leaning: sufficient; revisit only if a stable
  disposition must vary independently of tensions.)
- How early should cross-session continuity come (Adaptation A)? What is the smallest durable
  slice — recurring `Blocked`/open-thread goal ids only, or fuller `VolitionState`?
- Which emotion-like signals (if any) earn a place in a first slice, given they must be
  evidence-derived bias/visualization, not free-form? Should any of them feed arbitration, or
  visualization only at first?
- Is conscious/subconscious a runtime distinction (affects selection) or purely an
  introspection-surfacing filter?
- Do user-originated goals share the `GoalStatus` lifecycle, or only a subset?

## Documents To Update

Per [ProjectWorkflow.md](../ProjectFrame/ProjectWorkflow.md):

- **[volition_goal_system_design_brief.md](../volition_goal_system_design_brief.md):**
  annotate the brief inline with the framing (D1–D4) and the brief→project mapping so the
  temporary brief stays self-consistent with these decisions while it exists. *(Done — see the
  brief's "Project Reconciliation" section.)* No decision-log entry is added: D1/D3/D4 are
  translation guidance for a document that will be deleted, and the durable evidence-based
  stance (D2) already lives in the decision log (2026-05-15, 2026-06-27).
- **[Plan.RealtimeVolitionIntegration.md](Plan.RealtimeVolitionIntegration.md):** Adaptation
  B (opportunity detection + shaping-intensity dial) is folded into the behavioral-influence
  phase. *(Done.)* Adaptation A (continuity re-weighting) is recorded there as a deferred open
  decision, not yet committed.
- **[Plan.VolitionGoalSystem.md](Plan.VolitionGoalSystem.md):** add new offline slices for
  multi-turn Plans and (if accepted) emotion-like signals when promoted from idea to planned.
- **[Idea.VolitionGoalSystem.md](Idea.VolitionGoalSystem.md):** add a pointer to this
  reconciliation so the brief's concepts are findable in project terms.
- The imported brief
  ([volition_goal_system_design_brief.md](../volition_goal_system_design_brief.md)) is
  temporary scratch input, now annotated with the reconciliation. As its ideas are merged into
  the project documents (architecture, plans, experiments), the corresponding brief sections
  are retired; the file is deleted once nothing in it remains unmerged.

This note is ephemeral support for the plans above; once its adaptations are absorbed into the
plans (and the brief is fully merged), it can be archived.
