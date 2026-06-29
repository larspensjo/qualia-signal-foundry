# Handoff: Volition Work — Resume Here

**Date:** 2026-06-29
**Status:** Read-only realtime volition tools are validated enough for this slice; realtime integration should move to context injection next. Doc-only changes today, **nothing committed** (on `main`).
**Read next:** the "Next steps" section below, then the linked docs.

---

## 30-Second Orientation

The volition system is **not** greenfield. A pure `qsf_volition` crate plus a completed
8-slice offline build already exist ([Plan.VolitionGoalSystem.md](Plans/Plan.VolitionGoalSystem.md)).
A realtime integration ([Plan.RealtimeVolitionIntegration.md](Plans/Plan.RealtimeVolitionIntegration.md))
is partway done. Today's session reconciled an **external design brief** ("research from
elsewhere") against that existing system and hardened the design of the next realtime phase.

Framing decided: **keep the project's vocabulary** (Tension / Goal / Initiative), **keep the
evidence-based stance** (no free-form emotion or personality layer). "Personality" = the
configured tension set + `Mode`; "emotion" = derived functional signals. These framing notes
were deliberately **not** added to the DecisionLog — the brief is temporary, and the durable
stance already lives in the DecisionLog (2026-05-15, 2026-06-27).

---

## What Changed Today (doc-only, uncommitted)

| Document | Change |
|---|---|
| [volition_goal_system_design_brief.md](volition_goal_system_design_brief.md) | Added §0 "Project Reconciliation" (framing + full brief→project mapping table) and inline "Project note" callouts on §2/§3.1/§8. Temporary scratch doc — merge into real docs over time, then delete. |
| [Plans/Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md) | New. The project-side reconciliation: terminology mapping, concept disposition (Built / Adopt / Defer), framing decisions D1–D4, realtime-roadmap impact. Later updated with Adaptation C: injection should be layered by lifetime. |
| [Plans/Plan.RealtimeVolitionIntegration.md](Plans/Plan.RealtimeVolitionIntegration.md) | Context-injection phase hardened: added **opportunity detection** (brief §4.1) + a **shaping-intensity dial** (brief §14) with a protected-tier cap, plus verify steps, trace fields, and an Open-questions block. Later updated to mark read-only realtime tools complete, then clarified that Phase 4 should use a **layered injection stack**: stable baseline/personality at session start, dynamic goals/intentions per trusted turn. |
| [Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md](Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md) | Marked complete. Latest trusted sideband diagnostics verify both read-only tools execute, the selector trace contract is complete, and spoken answers preserve simulated-state framing. |
| [Glossary.md](Glossary.md) | New. Project-wide glossary with a volition section translating the external brief's vocabulary into project terms and marking whether concepts are built, designed next, or deferred. Later clarified `StableBaselineLayer` and the dynamic `VolitionContextPacket`. |
| Temporary testing handoff | Retired and deleted. Durable evidence now lives in the experiment file and realtime plan. |

---

## Realtime Integration — Phase Status

| Slice | State |
|---|---|
| Extract `qsf_volition` crate | ✅ done |
| Per-session `VolitionRuntimeState` (seeded; protected tiers) | ✅ done |
| Read-only tools `inspect_volition_state` / `select_volition_goals` | ✅ implemented; human validation accepted for this slice |
| Layered context injection before live responses (baseline + dynamic goals/intentions) | ⏳ not started; design hardened today |
| Bounded initiative in live loop / persistence / UI | ⏳ not started |

Offline build ([Plan.VolitionGoalSystem.md](Plans/Plan.VolitionGoalSystem.md)): all slices complete.

---

## Current Conclusion

The read-only-tool validation gate is closed for now. The latest trusted sideband run
shows successful calls to both `inspect_volition_state` and `select_volition_goals`.
`source: "sideband_trusted"` diagnostics now contain the tool records directly, and the
continuity state agrees with the diagnostics.

Important nuance: `select_volition_goals` still returned `status: no_match` for the broad
"goals related to helping the user" query, with an empty `selected_goal_ids` list and a
complete omitted-goal trace. Treat that as a selector-quality follow-up, not a blocker for
tool reachability or trace observability.

Strategic fork for now: context injection is the primary path for volition to influence the
live loop. Read-only tools remain useful for explicit inspection and explanation, but the
next integration step should not depend on the model choosing to call a tool.

Latest conclusion: context injection should not be treated as one flat "volition packet".
There are multiple layers with different lifetimes:

- **Stable baseline / personality rendering:** constant across sessions; injected once at
   conversation start through the initial realtime `session.update` instructions before any
   response. This is a rendering of the configured tension set, priors, project stance, and
   default `Mode`; it is **not** a new mutable personality object.
- **Drives / tensions:** stable or slow-changing; may be summarized with the baseline or
   refreshed only when the configured state changes.
- **Active goals:** session/turn-specific; selected and arbitrated after a trusted user turn
   and injected before the initial `response.create` for that turn.
- **Intentions / shaping intensity:** next-response or few-turn local steering; derived from
   arbitration, opportunity signals, and the protected-tier cap, then injected with the dynamic
   turn context.
- **Plans:** deferred multi-turn layer; injected only when an active conversational plan exists.
- **Memory / retrieved context:** already has a separate per-turn injection path and should
   remain distinct from volition layers.

The intended first implementation point is therefore: initial stable baseline in the sideband's
first `session.update`, and dynamic goal/intention context after trusted transcript handling and
before the first `response.create` for that user turn. Tool-loop continuations should not receive
fresh volition packets unless a later slice explicitly adds that behavior.

---

## Next Steps (in priority order)

1. **Expand layered context injection into implementation tasks.** Create the
   `Experiment.RealtimeVolitionContextInjection` scaffold and break the plan into small,
   testable slices: stable baseline/personality rendering in initial session instructions,
   opportunity detection, selection/arbitration turn packet, shaping-intensity dial,
   sideband injection ordering, and diagnostics.
2. **Decide Adaptation A** (continuity ordering) — whether a minimal cross-session continuity
   slice (recurring `Blocked` / open-thread goal ids) should jump ahead of the persistence
   phase. Recorded as a deferred open decision in the realtime plan's context-injection
   Open-questions block.
3. **Keep selector quality separate.** If non-empty `selected_goal_ids` becomes important,
   refine selector vocabulary, fixture goal terms, or prompt/query normalization in a focused
   follow-up. Do not let that delay context injection.
4. **Housekeeping** — commit today's doc changes (branch first; currently on `main`).

---

## Open Decisions Parked

- **Adaptation A:** pull cross-session continuity earlier vs. leave it in the persistence phase.
- **Selector quality:** decide whether broad help-related prompts should select
   `honor-explicit-user-request` / `complete-current-task`, or whether the current omitted-goal
   trace is sufficient for explicit inspection.
- **Emotion/personality slices, multi-turn Plans, conscious/subconscious, user-vs-simulator
  goals:** classified as new scope in
  [Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md) with
  attachment points; none scheduled yet.
- **Exact injected text:** not yet specified. The next scaffold should define the stable
   baseline instruction text and the dynamic turn-context template explicitly, then test that
   the rendered text is bounded and appears at the intended injection point.

---

## Key Pointers

| Purpose | Path |
|---|---|
| Reconciliation (mapping, disposition, framing) | [docs/Plans/Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md) |
| Project glossary | [docs/Glossary.md](Glossary.md) |
| External brief (annotated, temporary) | [docs/volition_goal_system_design_brief.md](volition_goal_system_design_brief.md) |
| Realtime plan (context-injection design) | [docs/Plans/Plan.RealtimeVolitionIntegration.md](Plans/Plan.RealtimeVolitionIntegration.md) |
| Offline build plan (complete) | [docs/Plans/Plan.VolitionGoalSystem.md](Plans/Plan.VolitionGoalSystem.md) |
| Current architecture | [docs/Architecture/Architecture.VolitionSystem.md](Architecture/Architecture.VolitionSystem.md) |
| Read-only tool validation result | [docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md](Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md) |
| Crate | `crates/qsf_volition/src/lib.rs` |
