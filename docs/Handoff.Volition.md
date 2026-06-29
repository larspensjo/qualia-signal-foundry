# Handoff: Volition Work — Resume Here

**Date:** 2026-06-29
**Status:** External brief reconciled into project docs; realtime integration in progress and blocked at read-only-tool validation. Doc-only changes today, **nothing committed** (on `main`).
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
| [Plans/Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md) | New. The project-side reconciliation: terminology mapping, concept disposition (Built / Adopt / Defer), framing decisions D1–D4, realtime-roadmap impact. |
| [Plans/Plan.RealtimeVolitionIntegration.md](Plans/Plan.RealtimeVolitionIntegration.md) | Context-injection phase hardened: added **opportunity detection** (brief §4.1) + a **shaping-intensity dial** (brief §14) with a protected-tier cap, plus verify steps, trace fields, and an Open-questions block. |

---

## Realtime Integration — Phase Status

| Slice | State |
|---|---|
| Extract `qsf_volition` crate | ✅ done |
| Per-session `VolitionRuntimeState` (seeded; protected tiers) | ✅ done |
| Read-only tools `inspect_volition_state` / `select_volition_goals` | ⚠️ implemented, **human validation BLOCKED** |
| Inject volition context before `response.create` (now w/ opportunity + intensity) | ⏳ not started; design hardened today |
| Bounded initiative in live loop / persistence / UI | ⏳ not started |

Offline build ([Plan.VolitionGoalSystem.md](Plans/Plan.VolitionGoalSystem.md)): all slices complete.

---

## The Active Blocker

[Testing.RealtimeVolitionTools.md](Testing.RealtimeVolitionTools.md): the realtime model
**will not call the volition tools** under `tool_choice: "auto"`, even after the
`DEFAULT_INSTRUCTIONS` fix. Confirmed it is the model's choice, not wiring.

**Key reframing for the decision below:** the testing handoff's own aside — "feeding the
volition snapshot directly into context would also work" — *is* the context-injection phase,
which **does not depend on the model deciding to call a tool**. So context injection sidesteps
this blocker. Question to settle: is read-only tool-calling the right validation gate, or is
ambient context injection the more robust path to "volition influences/explains behavior"?

---

## Next Steps (in priority order)

1. **Decide the strategic fork.** Keep pushing read-only tool-calling, or treat context
   injection as the real influence mechanism and let tools stay best-effort? This determines
   whether step 2 is a hard gate or a nice-to-have.
2. **Unblock read-only-tool validation** (if still gating) — from
   [Testing.RealtimeVolitionTools.md](Testing.RealtimeVolitionTools.md):
   either (a) imperative instruction ("you MUST call `inspect_volition_state` before answering
   questions about focus/goals/state"), or (b) a scoped `tool_choice` nudge for
   introspection-style prompts. Then re-run the two prompts and confirm a `ToolLoop` phase +
   non-empty `tool_requests` / `tool_executions`. Side fix: diagnostics store `output: null`,
   so capture spoken text from the UI or extend diagnostics.
3. **Decide Adaptation A** (continuity ordering) — whether a minimal cross-session continuity
   slice (recurring `Blocked` / open-thread goal ids) should jump ahead of the persistence
   phase. Recorded as a deferred open decision in the realtime plan's context-injection
   Open-questions block.
4. **When building context injection** — expand it into a task-by-task plan and create the
   `Experiment.RealtimeVolitionContextInjection` scaffold. The opportunity-detection +
   shaping-intensity-dial design is already written into the plan.
5. **Housekeeping** — commit today's doc changes (branch first; currently on `main`).

---

## Open Decisions Parked

- **Strategic fork:** tool-calling vs. context injection as the primary influence path (step 1).
- **Adaptation A:** pull cross-session continuity earlier vs. leave it in the persistence phase.
- **Emotion/personality slices, multi-turn Plans, conscious/subconscious, user-vs-simulator
  goals:** classified as new scope in
  [Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md) with
  attachment points; none scheduled yet.

---

## Key Pointers

| Purpose | Path |
|---|---|
| Reconciliation (mapping, disposition, framing) | [docs/Plans/Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md) |
| External brief (annotated, temporary) | [docs/volition_goal_system_design_brief.md](volition_goal_system_design_brief.md) |
| Realtime plan (context-injection design) | [docs/Plans/Plan.RealtimeVolitionIntegration.md](Plans/Plan.RealtimeVolitionIntegration.md) |
| Offline build plan (complete) | [docs/Plans/Plan.VolitionGoalSystem.md](Plans/Plan.VolitionGoalSystem.md) |
| Current architecture | [docs/Architecture/Architecture.VolitionSystem.md](Architecture/Architecture.VolitionSystem.md) |
| Tool-calling blocker investigation | [docs/Testing.RealtimeVolitionTools.md](Testing.RealtimeVolitionTools.md) |
| Crate | `crates/qsf_volition/src/lib.rs` |
