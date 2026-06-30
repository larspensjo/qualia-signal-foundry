# Handoff: Volition Work — Resume Here

**Date:** 2026-06-30
**Status:** Layered context injection is complete and live. **Realtime bounded initiative is
implemented and was human-tested today** — the behavior matches the designed gate, but the test
exposed diagnostics/observability gaps to close before the trace contract is self-verifying.
Today's doc + decision-log changes are **uncommitted** (on `main`).
**Read next:** "What changed today (2026-06-30)", then "Next steps".

---

## 30-Second Orientation

The volition system is **not** greenfield. A pure `qsf_volition` crate plus a completed
8-slice offline build exist ([Plan.VolitionGoalSystem.md](Plans/Plan.VolitionGoalSystem.md)),
and the realtime integration ([Plan.RealtimeVolitionIntegration.md](Plans/Plan.RealtimeVolitionIntegration.md))
is now most of the way through: read-only tools, layered context injection, and bounded internal
initiative are all built. What remains is cross-session persistence and a UI inspection surface.

Framing (unchanged): **keep the project's vocabulary** (Tension / Goal / Initiative), **keep the
evidence-based stance** (no free-form emotion or personality layer). "Personality" = the
configured tension set + `Mode`; "emotion" = derived functional signals. The durable stance lives
in the DecisionLog (2026-05-15, 2026-06-27); the temporary external brief is scratch.

---

## What Changed Today (2026-06-30, uncommitted on `main`)

Resolved the two blocking questions in the Phase 5 plan review
([Review.RealtimeVolitionIntegration.phase5.Plan.codex.json](Plans/Review.RealtimeVolitionIntegration.phase5.Plan.codex.json))
and ran the live human test.

| Document / artifact | Change |
|---|---|
| [DecisionLog.md](DecisionLog.md) | New entry **2026-06-30 "Realtime bounded-initiative surfacing, anti-nag cadence, and trace granularity"** capturing the protected-winner surfacing gate, anti-nag alternation, compact-snapshot trace contract, and single-item carrier. |
| [Plans/Plan.RealtimeVolitionIntegration.md](Plans/Plan.RealtimeVolitionIntegration.md) | Steps 5a–5g + Verify rewritten to the resolved decisions; the three open questions moved to a "Resolved decisions" block; corrected the false "shaping dial keeps protected turns quiet" claim. |
| Live human test | Ran 10 phrases through the realtime UI (typed input). Verified against `state/realtime/diagnostics/default.jsonl` correlated with the Phase-4 `volition_context_injected` traces. |

**The two decisions, in one line each:**
- **Protected-winner surfacing:** a protected-tier winner surfaces a line only when the turn has a
  *genuine* opportunity beyond the winner's own topic self-match (expressed uncertainty, introduced
  contradiction, or another goal's topic match). Rationale grounded in `ProjectVision.md` (presence
  over task completion), so full suppression on direct asks was rejected.
- **Anti-nag:** consecutive-turn alternation via `previous_turn_surfaced_goal_id` (set on surfaced
  turns, cleared otherwise). This replaced the proposed `last_initiative_goal_id`, which would have
  suppressed a repeated winner forever.

---

## Realtime Integration — Phase Status

| Slice | State |
|---|---|
| Extract `qsf_volition` crate | ✅ done |
| Per-session `VolitionRuntimeState` (seeded; protected tiers) | ✅ done |
| Read-only tools `inspect_volition_state` / `select_volition_goals` | ✅ done |
| Layered context injection before live responses (baseline + dynamic goals/intentions) | ✅ done — `Experiment.RealtimeVolitionContextInjection` |
| Bounded internal initiative in live loop | ✅ implemented + human-tested 2026-06-30 (observability gaps below) |
| Persist / inspect / consolidate realtime volition state | ⏳ not started (the next phase) |
| Surface volition state in the realtime UI | ⏳ not started |

Offline build ([Plan.VolitionGoalSystem.md](Plans/Plan.VolitionGoalSystem.md)): all slices complete.

---

## Human Test Result (2026-06-30)

All 10 turns matched the designed gate. Evidence is the bounded-initiative + context-injection
records in `state/realtime/diagnostics/default.jsonl` (10 `realtime_bounded_initiative` records,
in test order):

- **Quiet on direct asks** ("How can you help…", "Please show me…"): winner
  `honor-explicit-user-request`, only its own topic signal → suppressed. ✅
- **Surfaces on genuine opportunity** ("I'm confused about…", "…done, but how is that consistent?"):
  the contradiction case won via `avoid-overstating-impl-status` (tier 1) and the spoken reply was a
  textbook avoid-overstating reflection. ✅
- **Non-protected sweet spot** ("…evidence…unsettled", "Comparing two prototype fixtures…"):
  `clarify-weak-evidence-topic` (reflection) and `propose-followup-experiment` (ExperimentProposed). ✅
- **Context-retrieval** ("…revisit the unresolved thread about continuity"): `resurface-open-thread`
  → `ContextRetrievalRequested`, hints `[continuity, thread, revisit, unresolved]` stashed, no spoken
  line (hint-only). ✅
- `external_effect_executed = false` on all 10 — safety boundary held. ✅

**Gaps the test exposed (these are the next steps):**

1. **Surfacing outcome is not persisted.** The trace records the gate *inputs* (winner, signals,
   intensity, output) but not whether a line was actually surfaced or whether anti-nag fired. The
   surfaced/suppressed result had to be *reconstructed*; the anti-nag suppression is only proven by
   the unit test `repeated_surfaceable_winner_alternates… → [true,false,true]`, not by the artifact.
2. **`exchange_index` is `0` on every volition trace** (Phase 4 and Phase 5), because the trusted
   turn came through the typed box. The plan's "forward link by `exchange_index`" for hint
   consumption is therefore not usable as-is.
3. **Hint round-trip not completed live.** The retrieval hint was stashed on the last turn, so
   `hint_consumed_by_next_memory_injection` stayed `false`; the stash is per-connection
   (`SidebandRuntimeState`) and would be lost on reconnect (the known `phase5-sideband-stash-lifetime`
   risk).
4. **UI text duplication** (separate UI bug): each answer renders as a lead-in bubble + a detail
   bubble, then a third bubble repeating both concatenated. Voice was not duplicated → it's a
   realtime UI transcript reducer/selector issue, not sideband/volition.

---

## Next Steps (in priority order)

1. **Add `surfaced: bool` + `suppression_reason` to `RealtimeBoundedInitiativeTrace`** so surfacing
   and anti-nag are auditable from the artifact (closes gap #1). Update the experiment's
   trace-completeness contract and parsing verification to assert on them.
2. **Give each trusted turn a reliable unique key** (fix gap #2) so cross-turn correlation and the
   hint-consumption forward link work; confirm whether real voice turns already differ from the
   typed path before choosing the fix.
3. **Re-run the hint round-trip as two consecutive same-session turns** to verify augmented retrieval
   and `hint_consumed_by_next_memory_injection == true` (closes gap #3). Decide whether the stash
   must survive sideband reconnect or stay documented best-effort.
4. **Investigate the UI transcript duplication** (gap #4), independent of volition.
5. **Verify/extend [Experiment.RealtimeVolitionBoundedInitiative.md](Experiments/Experiment.RealtimeVolitionBoundedInitiative.md)**
   covers the resolved gate, the A/A/A alternation, the verbatim per-variant line text, and the new
   `surfaced`/`suppression_reason` fields once added. Update the architecture docs listed in the
   plan's "Documentation To Update" (VolitionSystem, ToolSystem, ContextManagement,
   StateAndObservability) for the bounded-initiative behavior.
6. **Housekeeping:** commit today's doc + decision-log changes (branch first; currently on `main`).

---

## Next Phase (after the steps above)

**Persist, inspect, and consolidate realtime volition state.** Decide what survives across sessions
(full `VolitionState` snapshot vs. compact derived memory vs. diagnostics-only + sleep/consolidation
extraction), keep durable goal/candidate changes behind manual review, and degrade gracefully to the
default fixture on corrupt/missing persistence. The gating open decision is **Adaptation A** — whether
a minimal cross-session continuity slice (recurring `Blocked` / open-thread goal ids) jumps ahead of
the full persistence work. After persistence, the final realtime slice is the **UI inspection panel**
(mode, tick, active/winning goal, selected/suppressed goals, last bounded initiative, trace links).

---

## Open Decisions Parked

- **Adaptation A:** pull cross-session continuity earlier vs. leave it in the persistence phase.
- **Initiative derivation:** stay rule-based (winner → `execute_initiative`) or add a later
  model-assisted proposer emitting the same `InitiativeOutput` shape through the event path. Default:
  rule-based only for now (the one remaining open question in the bounded-initiative plan).
- **Selector quality:** whether broad help-related prompts should select
  `honor-explicit-user-request` / `complete-current-task`, or whether the current omitted-goal trace
  is sufficient for explicit inspection.
- **Emotion/personality slices, multi-turn Plans, conscious/subconscious, user-vs-simulator goals:**
  classified as new scope in
  [Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md); none scheduled.

---

## Key Pointers

| Purpose | Path |
|---|---|
| Phase 5 decisions (durable) | [docs/DecisionLog.md](DecisionLog.md) — 2026-06-30 entry |
| Realtime plan (current) | [docs/Plans/Plan.RealtimeVolitionIntegration.md](Plans/Plan.RealtimeVolitionIntegration.md) |
| Phase 5 review (blockers, now resolved) | [docs/Plans/Review.RealtimeVolitionIntegration.phase5.Plan.codex.json](Plans/Review.RealtimeVolitionIntegration.phase5.Plan.codex.json) |
| Bounded-initiative experiment | [docs/Experiments/Experiment.RealtimeVolitionBoundedInitiative.md](Experiments/Experiment.RealtimeVolitionBoundedInitiative.md) |
| Context-injection experiment (complete) | [docs/Experiments/Experiment.RealtimeVolitionContextInjection.md](Experiments/Experiment.RealtimeVolitionContextInjection.md) |
| Initiative render + trace | `crates/qsf_realtime_server/src/realtime/volition_initiative.rs` |
| Surfacing gate + anti-nag wiring | `crates/qsf_realtime_server/src/realtime/sideband.rs` (`has_genuine_opportunity_signal`, gate near the `RealtimeBoundedInitiative` write) |
| Live diagnostics (per session id) | `state/realtime/diagnostics/<qsf_session_id>.jsonl` (default id: `default`) |
| Reconciliation (mapping, disposition, framing) | [docs/Plans/Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md) |
| Project glossary | [docs/Glossary.md](Glossary.md) |
| Offline build plan (complete) | [docs/Plans/Plan.VolitionGoalSystem.md](Plans/Plan.VolitionGoalSystem.md) |
| Current architecture | [docs/Architecture/Architecture.VolitionSystem.md](Architecture/Architecture.VolitionSystem.md) |
| Crate | `crates/qsf_volition/src/lib.rs` |
