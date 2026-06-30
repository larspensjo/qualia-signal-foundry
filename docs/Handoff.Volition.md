# Handoff: Volition Work — Resume Here

**Date:** 2026-06-30 (updated after Phase 7 complete)
**Status:** All volition work is **complete**. Both Plan.VolitionGoalSystem.md (8 phases)
and Plan.RealtimeVolitionIntegration.md (7 phases) are fully implemented and human-tested.
Both plans are ready to delete. The next open track is personality / goal experimentation —
adding new tensions and goals to the fixture to explore a different character shape.

---

## 30-Second Orientation

The volition system is fully built and live:

- **Pure domain crate** (`qsf_volition`): tensions, goals, salience, arbitration, mode bias,
  bounded initiative, continuity snapshots, consolidation extraction.
- **Realtime integration** (`qsf_realtime_server`): per-session `VolitionRuntimeState`, read-only
  tools, layered context injection (stable baseline + per-turn dynamic packet), bounded initiative
  on every trusted turn, cross-session continuity via reviewed-seed, and a live **volition state
  UI panel** in the browser (Phase 7).
- **Mode system**: `Neutral` / `Focused` / `Exploratory` bias arbitration within the biasable band
  (tier ≥ 4). Protected tiers (≤ 3) are always immune.

Framing (unchanged): keep the project's vocabulary (Tension / Goal / Initiative); keep the
evidence-based stance. "Personality" = the configured tension set + Mode. "Emotion" = derived
functional signals. The durable stance lives in DecisionLog (2026-05-15, 2026-06-27, 2026-06-30).

---

## What Is Complete

### Offline volition build (Plan.VolitionGoalSystem.md — all 8 phases)

| Phase | Slice | Status |
|---|---|---|
| 1 | Document the concept | ✅ done |
| 2 | Static tension/goal fixture + selection | ✅ done |
| 3 | Trace-backed initiative proposals | ✅ done |
| 4 | Event-driven salience, satisfaction, blocking, cooldown | ✅ done |
| 5 | Arbitration and conflict resolution | ✅ done |
| 6 | Reflection-generated goal candidates | ✅ done |
| 7 | Bounded internal initiative execution | ✅ done |
| 8 | Inspectable mode/bias state | ✅ done |

→ **Plan.VolitionGoalSystem.md can be deleted.**

### Realtime volition integration (Plan.RealtimeVolitionIntegration.md — all 7 phases)

| Phase | Slice | Status |
|---|---|---|
| 1 | Extract pure `qsf_volition` crate | ✅ done |
| 2 | Per-session `VolitionRuntimeState` seeded per QSF session | ✅ done |
| 3 | Read-only realtime volition tools | ✅ done |
| 4 | Layered context injection (baseline + dynamic goals/intentions + shaping dial) | ✅ done |
| 5 | Trace-backed bounded initiative in the live loop | ✅ done + human-tested |
| 6 | Persist, inspect, and consolidate realtime volition state | ✅ done + human-tested |
| 7 | Surface volition state in the realtime browser UI | ✅ done |

→ **Plan.RealtimeVolitionIntegration.md can be deleted.**

---

## Current Realtime Integration State

| Feature | State |
|---|---|
| `qsf_volition` pure crate | ✅ live |
| Per-session `VolitionRuntimeState` (seeded; protected tiers) | ✅ live |
| Read-only tools `inspect_volition_state` / `select_volition_goals` | ✅ live |
| Layered context injection (stable baseline + per-turn dynamic packet) | ✅ live |
| Bounded internal initiative with `surfaced` / `suppression_reason` trace | ✅ live |
| Cross-session continuity via `volition-state.json` + reviewed seed | ✅ live |
| **Volition state UI panel** (browser, live updates every trusted turn) | ✅ live (Phase 7) |

---

## Active Plan: RealtimeVoiceConversation Phase 5

[Plan.RealtimeVoiceConversation.md](Plans/Plan.RealtimeVoiceConversation.md) Phase 5 — **live
memory extraction + presence / interruption refinement** — is still the active large plan.
Decisions D14–D19 are resolved. The steps are:

1. Pure extraction-input builder (`qsf_app`) — turns the promoted continuity root into a `SleepInputBundle`
2. Wire the extraction pass — run the existing summarizer + proposers + review path over it
3. Live-loop latency observability — extend `DiagnosticRecord::LatencyObservation` per stage
4. Interruption observability (diagnostics-only, D18) — durable interruption diagnostic record
5. Optional UI surface for latency/interruption
6. Gates + docs

This is independent of the volition work and does not need to be done before personality experimentation.

---

## Next Exploration: Personality / Goal Experimentation

The fixture in [crates/qsf_volition/src/fixture.rs](../crates/qsf_volition/src/fixture.rs) defines
the tensions and goals that shape live behavior. Adding a new "personality" means adding tensions and
goals with the character you want, at the right tier.

### Current fixture summary

**Tensions (static_fixture):**
| ID | Tier | Bias |
|---|---|---|
| `boundary-preservation` | 1 | Highest |
| `coherence-maintenance` | 4 | High |
| `continuity-preservation` | 5 | High |
| `research-curiosity` | 7 | Medium |

**Protected tensions (realtime_seed_fixture, tier ≤ 3):**
| ID | Tier | Bias |
|---|---|---|
| `explicit-user-intent` | 2 | Highest |
| `current-task-completion` | 3 | High |

**Goals:** `clarify-weak-evidence-topic`, `avoid-overstating-impl-status`,
`resurface-open-thread`, `propose-followup-experiment`, `honor-explicit-user-request`,
`complete-current-task`.

### How to add a new tension/goal

**Immediate (current session only):** edit `static_fixture()` or `realtime_seed_fixture()` in
[fixture.rs](../crates/qsf_volition/src/fixture.rs), add a `Tension` + `Goal`, rebuild, and
run `qsf.ps1 realtime`. The volition UI panel will show it winning or losing arbitration live.

Good tier placement for experimentation:
- Tier 4–6: biasable band — mode bias can reorder these relative to each other
- Tier 7+: lowest priority, easily outranked by curiosity (but still fires when nothing else matches)
- Tier ≤ 3: protected floor — use only for genuine safety/user-intent constraints

**Cross-session persistence:** write a `volition-seed.reviewed.json` (the reviewed-seed format from
`qsf_volition::continuity`) and run `accept-reviewed-volition-seed` to merge it into future sessions.
New goals in the reviewed seed cannot be admitted at tier ≤ 3 (the `apply_reviewed_seed` invariant
enforces this). The `volition-continuity` experiment runs the consolidation pass.

**Watched in the UI:** after `cargo build`, the volition panel shows mode, tick, winning goal,
tier/protection status, shaping intensity, and initiative outcome live on every trusted turn —
so you can see a new goal win or lose arbitration in real time without inspecting JSONL.

### Ideas to try

- **Depth-seeking tension** (tier 5–6): fire when the conversation stays shallow across turns —
  pressure toward longer explanations or example requests.
- **Skepticism tension** (tier 4): activate when claims are made without evidence markers —
  nudge toward "what's the evidence?" reflection.
- **Acknowledgement tension** (tier 6): when the user signals effort or frustration, surface
  a brief acknowledgement line before the answer.
- **Mode exploration**: switch to `Exploratory` via `VolitionEvent::ModeChanged` at session start
  and watch curiosity goals (`research-curiosity`, tier 7) now win over continuity goals (tier 5)
  for `Exploratory`-boosted goals, while protected tiers stay immune.

---

## Key Pointers

| Purpose | Path |
|---|---|
| Fixture (tensions + goals) | `crates/qsf_volition/src/fixture.rs` |
| Model types (Tension, Goal, Mode, AllowedEffect, …) | `crates/qsf_volition/src/model.rs` |
| Arbitration + mode bias | `crates/qsf_volition/src/arbitration.rs` |
| Continuity / reviewed-seed | `crates/qsf_volition/src/continuity.rs` |
| Realtime session volition state | `crates/qsf_realtime_server/src/realtime/volition.rs` |
| Context injection builders | `crates/qsf_realtime_server/src/realtime/volition_injection.rs` |
| Bounded initiative render + trace | `crates/qsf_realtime_server/src/realtime/volition_initiative.rs` |
| UI volition panel | `crates/qsf_realtime_server/ui/src/realtime.ts` (`latestVolitionState`, `selectVolitionPanelModel`) |
| Volition inspection capture | `crates/qsf_realtime_server/src/realtime/volition_inspection_capture.rs` |
| Surfacing gate + anti-nag wiring | `crates/qsf_realtime_server/src/realtime/sideband.rs` |
| Architecture (volition system) | `docs/Architecture/Architecture.VolitionSystem.md` |
| Durable decisions | `docs/DecisionLog.md` (entries 2026-05-15, 2026-06-27, 2026-06-30) |
| Active large plan | `docs/Plans/Plan.RealtimeVoiceConversation.md` Phase 5 |
| Live diagnostics (per session) | `state/realtime/diagnostics/<qsf_session_id>.jsonl` (default: `default`) |
| Reviewed-seed file | `state/realtime/continuity/<session>/volition-seed.reviewed.json` |

---

## Open Decisions Parked

- **Adaptation A:** cross-session continuity continuity for recurring `Blocked` / open-thread goal ids —
  was superseded by Phase 6's full snapshot+reviewed-seed mechanism. No remaining action needed unless
  the reviewed-seed proves insufficient in practice.
- **Initiative derivation:** stay rule-based (`execute_initiative`) or add a later model-assisted
  proposer emitting the same `InitiativeOutput` shape. Default: rule-based only. Revisit if the
  rule-based outputs feel mechanical after more personality experimentation.
- **Selector quality:** broad help-related prompts may not activate `honor-explicit-user-request` /
  `complete-current-task` because those goals require specific keyword matches. Revisit keyword lists
  in the fixture if the protected-tier goals feel invisible in live sessions.
- **Personality scope:** emotion/personality slices, multi-turn Plans, conscious/subconscious,
  user-vs-simulator goals — classified as new scope in
  [Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md), now
  sequenced in [Plan.VolitionMotivationalTexture.md](Plans/Plan.VolitionMotivationalTexture.md)
  (provenance → emotion signals → conscious/subconscious → multi-turn Plans). Skeleton only;
  per-phase detailing pending.
- **Phase 6 reconciliation item:** the reviewed-seed-only vs. snapshot-restore seeding divergence is
  resolved as snapshot-restore (the implemented behavior); the UI panel must not claim "tick resets
  each session" (it does not). No further action unless the behavior changes.
