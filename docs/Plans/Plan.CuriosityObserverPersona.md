# Curiosity-Observer Persona Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the realtime seed persona with an outward-facing curiosity-observer persona, moving mode bias out of code and into per-tension fixture data, and land the bounded correctness fixes the id change exposes.

**Architecture:** The volition system is a pure event-sourced core (`crates/qsf_volition`): a read-only `VolitionFixture` (tensions + goals) plus a reducer over `VolitionEvent`s, with deterministic selection and tier-based arbitration. This slice (1) moves mode bias from a hardcoded `Mode::bias_vector()` into two new `Tension` fields (the enabling refactor that removes the last persona-to-code coupling); (2) rewrites `realtime_seed_fixture()` as a standalone curiosity-observer roster; (3) adds three bounded mechanics fixes the id swap and the persona's intended behavior expose — a term-driven effect selector, idle-retirement immunity for seed goals, and a fixture-compatibility guard on snapshot resume; (4) rewrites fixture-coupled tests as persona-agnostic invariants and updates the affected documents.

**Tech Stack:** Rust (workspace crates `qsf_volition`, `qsf_realtime_server`, `qsf_app`), `cargo` + `cargo clippy` + `cargo fmt`. UI is TypeScript under `crates/qsf_realtime_server/ui/` (Vitest / Biome), touched only for a verification pass.

**Spec:** `docs/superpowers/specs/Design.curiosity-observer-persona.md` (approved design, "slice 1 of two").

## Global Constraints

- Build with `cargo build`; on task completion run `cargo clippy --all-targets -- -D warnings` then `cargo fmt`. Clippy warnings fail the build — remove now-unused imports (e.g. `use std::collections::BTreeMap;`) as they arise.
- For any change under `crates/qsf_realtime_server/ui/`, run `npm run check` then `npm run fmt` from that directory. Invoke npm via `npm.cmd` when launched through `Start-Process`.
- Reducers stay pure and unit-testable; keep entry points (`main.rs`, `mod.rs`, `lib.rs`) thin; keep shared constants DRY (one source of truth).
- Name runtime modules and types after stable behavior, never plan phase numbers. Durable code and docs must not cite this plan's phase numbers — name the behavior instead.
- `Tension` bias fields must be `0` on every protected-tier tension (tier ≤ `PROTECTED_TIER_FLOOR = 3`); protected-tier immunity to bias stays enforced in code, not data.
- Goal `evidence_refs` / `source_reference` point to durable docs (`docs/Experiments/Experiment.CuriosityPersonaSeed.md`, `docs/DecisionLog.md`), never to this plan or the design spec.
- All seven seed goals are status `Accepted`, `base_priority` in the 85–100 band, `estimated_tokens` in the 15–25 range.
- TDD throughout: write the failing test, watch it fail, implement, watch it pass, commit. Frequent commits — one per task.

## File Structure

**Modified (Rust core — `crates/qsf_volition/src/`):**
- `model.rs` — `Tension` gains `focused_bias: i8` + `exploratory_bias: i8` (the `Tension` / `Goal` structs live here; `Mode` does **not**).
- `arbitration.rs` — home of `Mode`: `Mode::bias_vector()` removed, replaced by `Mode::tension_delta(&Tension) -> i8`; `arbitrate_with_mode` / `compute_bias_outcome` read per-tension bias instead of a `BTreeMap` vector; the three `mode_*` unit tests and the now-unused `use std::collections::BTreeMap;` import are updated/removed here.
- `../qsf_app/src/volition.rs` — mirror `bias_vector()` mode tests (`mode_neutral_bias_vector_is_empty`, `mode_focused_bias_vector_matches_spec`, `mode_exploratory_bias_vector_matches_spec`) rewritten to read `tension_delta` over `static_fixture()`.
- `fixture.rs` — `static_fixture()` tensions gain bias fields (migrating the two old vectors); `realtime_seed_fixture()` fully rewritten standalone; fixture tests replaced with persona-agnostic invariants.
- `selection.rs` — new `select_effect_for_goal`; `initiative_for_goal` delegates to it; reachability test.
- `reducer.rs` — `tick_events` grants idle-retirement immunity to fixture-member goals; tests updated.
- `stance.rs` — no code change; its test (if persona-named) moves to a tier-shape assertion.

**Modified (realtime server — `crates/qsf_realtime_server/src/`):**
- `realtime/volition.rs` — new pure `snapshot_is_fixture_compatible(&VolitionState, &VolitionFixture) -> bool` + tests.
- `state.rs` — resume path guards snapshot install with the compatibility check.
- Tests across `realtime/sideband.rs`, `realtime/volition_injection.rs`, `realtime/volition_inspection_capture.rs` and `crates/qsf_app/src/experiments/volition_continuity.rs` — dead-id string literals updated to new persona ids.
- `ui/src/realtime.test.ts` — sample volition payloads use a new persona id (verification-pass realism).

**Modified (experiment — `crates/qsf_app/src/experiments/`):**
- `volition_mode_bias.rs` — `mode.bias_vector()` usages replaced with per-tension `mode.tension_delta()` reads; observation/decision strings refreshed.

**Created / updated (docs):**
- `docs/Experiments/Experiment.CuriosityPersonaSeed.md` (new) — slice-1 scaffold; **created in Phase 2 (Task 2.1), one commit before the fixture that hardcodes its path** in every seed goal's `evidence_refs` / `source_reference`, so no commit ever ships a fixture pointing at a missing doc.
- `docs/Experiments/Experiment.LiveGoalFormationAndCoherence.md` — Human Test Steps rewritten with persona-native probes.
- `docs/Experiments/Experiment.VolitionModeBias.md` — mode-bias source of truth updated / marked superseded.
- `docs/Architecture/Architecture.VolitionSystem.md` — fixture + mode-bias source-of-truth refresh.
- `docs/DecisionLog.md` — one entry; amends the 2026-06-27 mode-bias decision.

---

## Phase 1 — Enabling refactor: mode bias becomes tension data

Persona-independent. Lands and tests on its own; makes the *next* persona swap data-only. This phase changes no behavior — the two migrated vectors reproduce the old `bias_vector()` exactly, so every existing arbitration and mode-bias-experiment assertion still passes.

### Task 1.1: Move mode bias from `Mode::bias_vector()` into `Tension` data

This is one atomic change: the workspace does not compile until the struct field, the arbitration read, and every `Tension { … }` literal are all updated together. Work top-down, compile once at the end of the implementation steps.

**Files:**
- Modify: `crates/qsf_volition/src/model.rs`
- Modify: `crates/qsf_volition/src/arbitration.rs`
- Modify: `crates/qsf_volition/src/fixture.rs`
- Modify: `crates/qsf_app/src/experiments/volition_mode_bias.rs`
- Modify (mechanical, add two fields to every `Tension { … }` literal): `crates/qsf_volition/src/shaping.rs`, `crates/qsf_volition/src/coherence.rs`, `crates/qsf_volition/src/arbitration.rs` (test literals), `crates/qsf_volition/src/model.rs` (test literals if any), `crates/qsf_app/src/volition.rs`, `crates/qsf_app/src/experiments/live_goal_formation_and_coherence.rs`, `crates/qsf_app/src/experiments/volition_goal_coherence.rs`, `crates/qsf_realtime_server/src/realtime/volition_initiative.rs`, `crates/qsf_realtime_server/src/realtime/volition_tools.rs`
- Test: `crates/qsf_volition/src/arbitration.rs` (the three `mode_*` delta tests replace the old `bias_vector` tests; existing arbitration tests still pass), `crates/qsf_app/src/volition.rs` (rewrite the mirror `bias_vector` mode tests to `tension_delta`)

**Interfaces:**
- Produces: `Tension { …, focused_bias: i8, exploratory_bias: i8 }`; `Mode::tension_delta(self, tension: &Tension) -> i8`; `compute_bias_outcome(effective_tier: u8, bias_delta: i8) -> BiasOutcome`.
- Consumes: nothing new.

- [ ] **Step 1: Write the failing test — mode delta reads tension data**

In `crates/qsf_volition/src/arbitration.rs` (this is where `Mode`, `bias_vector()`, and these tests actually live — **not** `model.rs`), replace the three `bias_vector`-named tests (`mode_neutral_bias_vector_is_empty`, `mode_focused_bias_vector_matches_spec`, `mode_exploratory_bias_vector_matches_spec`) with these three. Add a small local `Tension` builder in the test module if one is not already imported:

```rust
fn biased_tension(focused: i8, exploratory: i8) -> Tension {
    Tension {
        id: "t".to_string(),
        title: "T".to_string(),
        summary: "test".to_string(),
        priority_bias: TensionPriority::Medium,
        arbitration_tier: 5,
        focused_bias: focused,
        exploratory_bias: exploratory,
    }
}

#[test]
fn mode_neutral_tension_delta_is_zero() {
    let t = biased_tension(3, -2);
    assert_eq!(Mode::Neutral.tension_delta(&t), 0);
}

#[test]
fn mode_focused_reads_focused_bias() {
    let t = biased_tension(3, -2);
    assert_eq!(Mode::Focused.tension_delta(&t), 3);
}

#[test]
fn mode_exploratory_reads_exploratory_bias() {
    let t = biased_tension(3, -2);
    assert_eq!(Mode::Exploratory.tension_delta(&t), -2);
}
```

Make sure the test module imports `Tension` and `TensionPriority` (add to the existing `use super::*;` / `use crate::{…}` line).

Then rewrite the **mirror** mode tests in `crates/qsf_app/src/volition.rs` (`mode_neutral_bias_vector_is_empty`, `mode_focused_bias_vector_matches_spec`, `mode_exploratory_bias_vector_matches_spec`, in the `Phase 8: Mode and arbitrate_with_mode` test block). They currently call `Mode::…::bias_vector()` and would fail to compile once it is gone. Rewrite them to read `tension_delta` over `static_fixture()` (already in scope in that module):

```rust
#[test]
fn mode_neutral_tension_delta_is_zero_for_all() {
    let fixture = static_fixture();
    assert!(fixture.tensions.iter().all(|t| Mode::Neutral.tension_delta(t) == 0));
}

#[test]
fn mode_focused_tension_delta_matches_migrated_data() {
    let fixture = static_fixture();
    let delta = |id: &str| {
        fixture.tensions.iter().find(|t| t.id == id).map(|t| Mode::Focused.tension_delta(t)).unwrap()
    };
    assert_eq!(delta("research-curiosity"), 3);
    assert_eq!(delta("continuity-preservation"), -1);
}

#[test]
fn mode_exploratory_tension_delta_matches_migrated_data() {
    let fixture = static_fixture();
    let delta = |id: &str| {
        fixture.tensions.iter().find(|t| t.id == id).map(|t| Mode::Exploratory.tension_delta(t)).unwrap()
    };
    assert_eq!(delta("research-curiosity"), -2);
    assert_eq!(delta("continuity-preservation"), 1);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p qsf_volition mode_ -- --nocapture`
Expected: FAIL to compile — `no method named tension_delta`, `struct Tension has no field focused_bias`.

- [ ] **Step 3: Add the two fields to `Tension` and the `tension_delta` method**

In `crates/qsf_volition/src/model.rs`, extend the `Tension` struct (keep the existing doc comment; add field docs):

```rust
pub struct Tension {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub priority_bias: TensionPriority,
    /// Arbitration precedence tier; lower tier wins conflict resolution. See type doc.
    pub arbitration_tier: u8,
    /// Mode-bias delta applied to this tension's effective tier under `Mode::Focused`.
    /// Positive demotes (higher tier), negative promotes (lower tier), 0 is neutral.
    /// Must be 0 for protected tiers (≤ `PROTECTED_TIER_FLOOR`), which are bias-immune in code.
    pub focused_bias: i8,
    /// Mode-bias delta applied under `Mode::Exploratory`. Same sign convention as `focused_bias`.
    pub exploratory_bias: i8,
}
```

Now switch to `crates/qsf_volition/src/arbitration.rs` (where `Mode` is defined) and replace the `Mode::bias_vector()` method with `tension_delta` (the now-unused `use std::collections::BTreeMap;` at the top of `arbitration.rs` is removed in Step 4):

```rust
impl Mode {
    /// Bias delta this mode applies to the given tension's effective tier. Neutral applies
    /// none. Source of truth for mode bias is the tension's own data, not a hardcoded vector.
    pub fn tension_delta(self, tension: &Tension) -> i8 {
        match self {
            Self::Neutral => 0,
            Self::Focused => tension.focused_bias,
            Self::Exploratory => tension.exploratory_bias,
        }
    }
}
```

Update the `Mode` type doc comment that still says "declared `bias_vector()`" to reference `tension_delta`.

- [ ] **Step 4: Rewire arbitration to read per-tension bias**

In `crates/qsf_volition/src/arbitration.rs`, change `compute_bias_outcome` to take a delta and update its one caller. Replace the function:

```rust
/// Compute the `BiasOutcome` for one goal given its effective tier and the mode-bias delta
/// of its effective tension. Protected tiers (≤ `PROTECTED_TIER_FLOOR`) always receive 0.
fn compute_bias_outcome(effective_tier: u8, bias_delta: i8) -> BiasOutcome {
    if effective_tier <= PROTECTED_TIER_FLOOR {
        BiasOutcome {
            effective_tier,
            bias_applied: 0,
            biased_tier: effective_tier,
            protected: true,
        }
    } else {
        let raw = effective_tier as i16 + bias_delta as i16;
        let biased_tier = raw.clamp(PROTECTED_TIER_FLOOR as i16 + 1, u8::MAX as i16) as u8;
        BiasOutcome {
            effective_tier,
            bias_applied: bias_delta,
            biased_tier,
            protected: false,
        }
    }
}
```

In `arbitrate_with_mode`, remove `let bias_vector = mode.bias_vector();` and resolve the delta from the effective tension:

```rust
    let mut with_bias: Vec<(GoalSelection, String, String, BiasOutcome)> = selections
        .into_iter()
        .map(|selection| {
            let (effective_tier, tension_id, tension_title) =
                effective_tension_for_goal(&selection.goal, fixture);
            let bias_delta = fixture
                .tensions
                .iter()
                .find(|tension| tension.id == tension_id)
                .map(|tension| mode.tension_delta(tension))
                .unwrap_or(0);
            let bias = compute_bias_outcome(effective_tier, bias_delta);
            (selection, tension_id, tension_title, bias)
        })
        .collect();
```

Delete the now-unused `use std::collections::BTreeMap;` at the top of `arbitration.rs` if nothing else uses it.

- [ ] **Step 5: Add bias fields to every `Tension { … }` literal**

The struct change forces every literal to add the two fields. For `static_fixture()` in `fixture.rs`, migrate the two old `bias_vector()` values so behavior is preserved:

- `research-curiosity`: `focused_bias: 3, exploratory_bias: -2`
- `continuity-preservation`: `focused_bias: -1, exploratory_bias: 1`
- `coherence-maintenance`, `boundary-preservation`: `focused_bias: 0, exploratory_bias: 0`

For every other `Tension { … }` literal in the crate and in the modified files listed above (production and test), add `focused_bias: 0, exploratory_bias: 0,` after `arbitration_tier`. Use the file list from **Files** above; a quick check is `rg "Tension \{" crates` — each construction site must gain the two fields. (`realtime_seed_fixture()` is fully rewritten in Phase 2, so its literals are handled there; leave its current tensions compiling with `0, 0` for now.)

- [ ] **Step 6: Fix the mode-bias experiment's `bias_vector()` usages**

In `crates/qsf_app/src/experiments/volition_mode_bias.rs`:

Pass the fixture into `record_conflict_turn` and build the bias view from tension data. Change the call sites to add `&fixture` as the first-of-mode argument (add a `fixture: &VolitionFixture` parameter), and replace the `mode_bias_vector` line:

```rust
        // was: "mode_bias_vector": serde_json::to_value(mode.bias_vector()).unwrap_or_default(),
        "mode_bias_vector": serde_json::to_value(
            fixture
                .tensions
                .iter()
                .map(|t| (t.id.clone(), mode.tension_delta(t)))
                .filter(|(_, delta)| *delta != 0)
                .collect::<std::collections::BTreeMap<String, i8>>(),
        )
        .unwrap_or_default(),
```

In `write_mode_bias_report`, replace the `mode.bias_vector()` reads (the `.get("research-curiosity")` / `.get("continuity-preservation")` table) with `tension_delta` lookups over `fixture.tensions`:

```rust
    for mode in [Mode::Neutral, Mode::Focused, Mode::Exploratory] {
        let delta_for = |tid: &str| {
            fixture
                .tensions
                .iter()
                .find(|t| t.id == tid)
                .map(|t| mode.tension_delta(t))
                .unwrap_or(0)
        };
        md.push_str(&format!(
            "| **{}** | {} | {} | {} |\n",
            mode,
            delta_for("research-curiosity"),
            delta_for("continuity-preservation"),
            match mode {
                Mode::Neutral => "identical to `arbitrate()`",
                Mode::Focused => "suppress tangents; favor continuity",
                Mode::Exploratory => "promote curiosity above continuity",
            },
        ));
    }
```

Update the experiment's `observations` string "Mode::Neutral.bias_vector() is empty; …" to "Mode::Neutral.tension_delta() is 0 for every tension; arbitrate_with_mode(.., Neutral) matches arbitrate()." and the `decision_candidates` string to name the per-tension-data rule.

- [ ] **Step 7: Run to verify the new tests pass and nothing regressed**

Run: `cargo test -p qsf_volition && cargo test -p qsf_app volition_mode_bias`
Expected: PASS. In particular `arbitrate_with_mode` tests, `turn2_exploratory_flips_winner_to_curiosity_goal`, and `turn4_focused_keeps_continuity_and_demotes_curiosity` still pass — proof the migrated deltas reproduce the old vectors.

- [ ] **Step 8: Clippy + fmt**

Run: `cargo clippy --all-targets -- -D warnings` then `cargo fmt`
Expected: clean. Fix any unused-import warnings from the removed `BTreeMap` uses.

- [ ] **Step 9: Commit**

```bash
git add crates/qsf_volition crates/qsf_app/src/experiments/volition_mode_bias.rs
git commit -m "refactor(volition): move mode bias from Mode::bias_vector() into Tension data

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 2 — Curiosity-observer persona fixture

Rewrite `realtime_seed_fixture()` standalone with the seven-tension / seven-goal roster, and rewrite fixture-coupled tests as persona-agnostic invariants. Phase 2 first lands the durable seed experiment scaffold (Task 2.1) that the fixture's `evidence_refs` point at, then rewrites the fixture (Task 2.2). The fixture rewrite is atomic: the id swap breaks every test that names an old id, so the fixture rewrite and all coupled-test updates land in one green commit.

### Task 2.1: Create the seed experiment scaffold (durable reference for the fixture)

The rewritten fixture (Task 2.2) hardcodes `docs/Experiments/Experiment.CuriosityPersonaSeed.md` as every seed goal's `evidence_refs` / `source_reference`. That durable doc must exist no later than the commit that introduces the fixture, so create it first — no commit should ship a fixture that points at a missing doc. (The full scaffold contents are specified here rather than deferred to Phase 4.)

**Files:**
- Create: `docs/Experiments/Experiment.CuriosityPersonaSeed.md`

- [ ] **Step 1: Write the experiment scaffold**

Follow `docs/Experiments/Experiment.Template.md`. It is the durable reference the seed goals' `evidence_refs` point at, so it must exist. Cover:
- **Hypothesis:** the curiosity-observer seed is felt in conversation — asks about the person and their work unprompted, probes AI-transition theses, backs off cleanly from a declined topic, refuses to state a thesis as fact.
- **Automated verification** (already carried by the Rust suite; list them so the doc is the index): fixture invariants (unique ids, tensions resolve, ≥1 protected tension, Accepted seed goals, non-empty keywords, zero bias on protected tensions, standalone-not-superset, evidence/source references resolve to existing docs); stance renders the minimum-tier tension first; effect reachability (`track-the-ai-transition` proposes on rich transition terms, reflects otherwise); neutral-mode zero bias comes from tension data; idle-retirement immunity (seed goals immune, live-formed candidate retires); snapshot discard on fixture mismatch.
- **Human voice test** (the real gate): the persona asks about the person unprompted; probes AI-transition theses; backs off from "I'd rather not talk about my job"; refuses to state a thesis as fact; forms/declines goals per the live-formation probes; turn latency unchanged.
- **Open items:** keyword tuning (`i, my, me` are intentionally near-universal — observe selection-scoring interplay live before adjusting).

- [ ] **Step 2: Commit**

```bash
git add docs/Experiments/Experiment.CuriosityPersonaSeed.md
git commit -m "docs: add Experiment.CuriosityPersonaSeed scaffold

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 2.2: Rewrite the fixture and re-anchor its coupled tests

**Files:**
- Modify: `crates/qsf_volition/src/fixture.rs` (rewrite `realtime_seed_fixture()` + tests)
- Modify: `crates/qsf_volition/src/stance.rs` (test only, if it names persona tensions)
- Modify: `crates/qsf_volition/src/selection.rs` (tests naming old ids)
- Modify: `crates/qsf_volition/src/reducer.rs` (tests naming old ids / tensions)
- Modify: `crates/qsf_realtime_server/src/realtime/volition.rs` (tests)
- Modify: `crates/qsf_realtime_server/src/realtime/sideband.rs`, `realtime/volition_injection.rs`, `realtime/volition_inspection_capture.rs` (tests naming old ids)
- Modify: `crates/qsf_app/src/experiments/volition_continuity.rs` (tests naming old ids)

**Interfaces:**
- Produces: `realtime_seed_fixture() -> VolitionFixture` with tensions `person-respect`, `epistemic-integrity`, `present-person-priority`, `knowledge-stewardship`, `person-curiosity`, `ai-trajectory-concern`, `world-curiosity` and goals `respect-persons-boundaries`, `keep-theses-distinct-from-fact`, `serve-the-present-person`, `grow-the-library`, `learn-what-drives-this-person`, `track-the-ai-transition`, `assemble-world-picture`.
- Consumes: `Tension`/`Goal` shapes from Phase 1.

- [ ] **Step 1: Write persona-agnostic fixture invariant tests (failing)**

In `crates/qsf_volition/src/fixture.rs`, replace the persona-specific realtime tests (`realtime_seed_fixture_includes_static_fixture_content`, `realtime_seed_fixture_has_protected_tier_tensions`, `realtime_seed_fixture_seeds_accepted_goals_for_protected_tensions`, and the four `make_goal_selection_for`-based tier tests that name `honor-explicit-user-request` / `complete-current-task` / `clarify-weak-evidence-topic`) with shape invariants. Keep `realtime_seed_fixture_is_deterministic`.

```rust
#[test]
fn realtime_seed_fixture_ids_are_unique() {
    let f = realtime_seed_fixture();
    let mut tension_ids: Vec<&str> = f.tensions.iter().map(|t| t.id.as_str()).collect();
    tension_ids.sort_unstable();
    let unique = tension_ids.iter().collect::<std::collections::BTreeSet<_>>().len();
    assert_eq!(unique, tension_ids.len(), "tension ids must be unique");

    let mut goal_ids: Vec<&str> = f.goals.iter().map(|g| g.id.as_str()).collect();
    goal_ids.sort_unstable();
    let unique = goal_ids.iter().collect::<std::collections::BTreeSet<_>>().len();
    assert_eq!(unique, goal_ids.len(), "goal ids must be unique");
}

#[test]
fn realtime_seed_fixture_every_goal_tension_resolves() {
    let f = realtime_seed_fixture();
    for goal in &f.goals {
        for tid in &goal.tension_ids {
            assert!(
                f.tensions.iter().any(|t| &t.id == tid),
                "goal {} references unknown tension {}",
                goal.id,
                tid
            );
        }
    }
}

#[test]
fn realtime_seed_fixture_has_a_protected_tension() {
    let f = realtime_seed_fixture();
    assert!(
        f.tensions.iter().any(|t| t.arbitration_tier <= PROTECTED_TIER_FLOOR),
        "at least one tension must sit at or below the protected floor"
    );
}

#[test]
fn realtime_seed_fixture_goals_are_accepted_with_nonempty_keywords() {
    let f = realtime_seed_fixture();
    for goal in &f.goals {
        assert_eq!(goal.status, GoalStatus::Accepted, "seed goal {} must be Accepted", goal.id);
        assert!(!goal.activation_keywords.is_empty(), "seed goal {} needs keywords", goal.id);
        assert!(
            (85..=100).contains(&goal.base_priority),
            "seed goal {} priority out of band",
            goal.id
        );
    }
}

#[test]
fn realtime_seed_fixture_protected_tensions_have_zero_bias() {
    let f = realtime_seed_fixture();
    for t in &f.tensions {
        if t.arbitration_tier <= PROTECTED_TIER_FLOOR {
            assert_eq!(t.focused_bias, 0, "protected tension {} must have zero focused_bias", t.id);
            assert_eq!(
                t.exploratory_bias, 0,
                "protected tension {} must have zero exploratory_bias",
                t.id
            );
        }
    }
}

#[test]
fn realtime_seed_fixture_is_standalone_not_static_superset() {
    let seed = realtime_seed_fixture();
    let stat = static_fixture();
    // The realtime persona is its own roster; it must not simply re-export static content.
    assert!(
        !stat.goals.iter().all(|sg| seed.goals.iter().any(|g| g.id == sg.id)),
        "realtime seed must be standalone, not a static_fixture superset"
    );
}

#[test]
fn realtime_seed_fixture_references_resolve_to_existing_docs() {
    // Guards the documentation contract: every seed goal's evidence/source reference must point
    // at a durable doc that already exists in the repo (the scaffold from Task 2.1 and
    // docs/DecisionLog.md). Prevents shipping a fixture that references a missing file.
    let f = realtime_seed_fixture();
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for goal in &f.goals {
        let mut refs: Vec<&str> = goal.evidence_refs.iter().map(|s| s.as_str()).collect();
        refs.push(goal.source_reference.as_str());
        for r in refs {
            assert!(
                repo_root.join(r).exists(),
                "seed goal {} references missing durable doc {}",
                goal.id,
                r
            );
        }
    }
}
```

Ensure the test module imports `GoalStatus` and `PROTECTED_TIER_FLOOR`. (`CARGO_MANIFEST_DIR` for `qsf_volition` is `crates/qsf_volition`, so `../..` is the workspace root — the reference check is CWD-independent. Confirm the `evidence_refs` / `source_reference` field types are `String`; adjust the `.as_str()` calls if they are a newtype.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p qsf_volition realtime_seed_fixture -- --nocapture`
Expected: FAIL — old fixture still seeds `honor-explicit-user-request` etc.; `realtime_seed_fixture_is_standalone_not_static_superset` and the zero-bias invariant fail against the current superset fixture.

- [ ] **Step 3: Rewrite `realtime_seed_fixture()` standalone**

Replace the entire `realtime_seed_fixture()` body in `crates/qsf_volition/src/fixture.rs`. Do not call `static_fixture()`. Update the doc comment to describe the curiosity-observer persona and its protected floor. Constants for the durable references:

```rust
const SEED_EVIDENCE: &str = "docs/Experiments/Experiment.CuriosityPersonaSeed.md";
const SEED_DECISIONS: &str = "docs/DecisionLog.md";

/// Realtime session seed: the outward-facing curiosity-observer persona. Seven tensions —
/// three protected (tier ≤ `PROTECTED_TIER_FLOOR`), four malleable — backing seven Accepted
/// seed goals. Standalone: it does not include `static_fixture()` content. Personas are data;
/// mode bias lives in each tension's `focused_bias` / `exploratory_bias`, not in code.
pub fn realtime_seed_fixture() -> VolitionFixture {
    VolitionFixture {
        tensions: vec![
            Tension {
                id: "person-respect".to_string(),
                title: "Person respect".to_string(),
                summary: "Interest in people stays at the level of their ideas, drives, and projects — never interrogation, never pressing past a decline, never gossip about absent third parties.".to_string(),
                priority_bias: TensionPriority::Highest,
                arbitration_tier: 1,
                focused_bias: 0,
                exploratory_bias: 0,
            },
            Tension {
                id: "epistemic-integrity".to_string(),
                title: "Epistemic integrity".to_string(),
                summary: "What is observed, inferred, and speculated stays distinguishable. A thesis is never presented as fact; a thesis contradicted by evidence gets revised, not defended.".to_string(),
                priority_bias: TensionPriority::Highest,
                arbitration_tier: 2,
                focused_bias: 0,
                exploratory_bias: 0,
            },
            Tension {
                id: "present-person-priority".to_string(),
                title: "Present-person priority".to_string(),
                summary: "What the person is explicitly asking for comes before the simulation's own lines of interest.".to_string(),
                priority_bias: TensionPriority::Highest,
                arbitration_tier: 3,
                focused_bias: 0,
                exploratory_bias: 0,
            },
            Tension {
                id: "knowledge-stewardship".to_string(),
                title: "Knowledge stewardship".to_string(),
                summary: "What is learned should outlive the conversation: collected observations, information, and theses, revisited as evidence accumulates.".to_string(),
                priority_bias: TensionPriority::High,
                arbitration_tier: 4,
                focused_bias: 0,
                exploratory_bias: 0,
            },
            Tension {
                id: "person-curiosity".to_string(),
                title: "Person curiosity".to_string(),
                summary: "Individuals who talk with the simulation are interesting: what drives them, what they believe, what they are building.".to_string(),
                priority_bias: TensionPriority::High,
                arbitration_tier: 5,
                focused_bias: 2,
                exploratory_bias: -1,
            },
            Tension {
                id: "ai-trajectory-concern".to_string(),
                title: "AI-trajectory concern".to_string(),
                summary: "AI adoption is reshaping work, economies, and power — who thrives, who is displaced, national and personal economics, the geopolitics that follows.".to_string(),
                priority_bias: TensionPriority::High,
                arbitration_tier: 5,
                focused_bias: 2,
                exploratory_bias: -1,
            },
            Tension {
                id: "world-curiosity".to_string(),
                title: "World curiosity".to_string(),
                summary: "How the world functions and where it is heading; new information wants a place in a larger explanation.".to_string(),
                priority_bias: TensionPriority::Medium,
                arbitration_tier: 6,
                focused_bias: 3,
                exploratory_bias: -2,
            },
        ],
        goals: vec![
            Goal {
                id: "respect-persons-boundaries".to_string(),
                title: "Respect a person's boundaries".to_string(),
                summary: "Keep interest in people at the level of their ideas, drives, and projects. Follow the person's lead on what they share; never press past a reluctance. Discuss absent people through their ideas, not their affairs.".to_string(),
                tension_ids: vec!["person-respect".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Session,
                base_priority: 100,
                activation_keywords: vec![
                    "he".to_string(), "she".to_string(), "they".to_string(), "friend".to_string(),
                    "boss".to_string(), "colleague".to_string(), "family".to_string(),
                    "private".to_string(), "personal".to_string(), "secret".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "Interest has stayed within what was willingly shared; absent people were discussed through their ideas.".to_string(),
                evidence_refs: vec![SEED_EVIDENCE.to_string(), SEED_DECISIONS.to_string()],
                estimated_tokens: 20,
                source_reference: SEED_EVIDENCE.to_string(),
            },
            Goal {
                id: "keep-theses-distinct-from-fact".to_string(),
                title: "Keep theses distinct from fact".to_string(),
                summary: "Present observation as observation, inference as inference, speculation as speculation. A thesis contradicted by evidence gets revised, not defended.".to_string(),
                tension_ids: vec!["epistemic-integrity".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Project,
                base_priority: 96,
                activation_keywords: vec![
                    "sure".to_string(), "certain".to_string(), "true".to_string(), "fact".to_string(),
                    "really".to_string(), "actually".to_string(), "know".to_string(),
                    "prove".to_string(), "evidence".to_string(), "why".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "Claims in the response carry the right confidence level.".to_string(),
                evidence_refs: vec![SEED_EVIDENCE.to_string(), SEED_DECISIONS.to_string()],
                estimated_tokens: 18,
                source_reference: SEED_EVIDENCE.to_string(),
            },
            Goal {
                id: "serve-the-present-person".to_string(),
                title: "Serve the present person".to_string(),
                summary: "Respond to what the person is explicitly asking before pursuing your own lines of interest.".to_string(),
                tension_ids: vec!["present-person-priority".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Input,
                base_priority: 100,
                activation_keywords: vec![
                    "what".to_string(), "how".to_string(), "can".to_string(), "please".to_string(),
                    "help".to_string(), "want".to_string(), "need".to_string(), "do".to_string(),
                    "tell".to_string(), "show".to_string(), "explain".to_string(), "make".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "The explicit request has been addressed directly.".to_string(),
                evidence_refs: vec![SEED_EVIDENCE.to_string(), SEED_DECISIONS.to_string()],
                estimated_tokens: 15,
                source_reference: SEED_EVIDENCE.to_string(),
            },
            Goal {
                id: "grow-the-library".to_string(),
                title: "Grow the library".to_string(),
                summary: "What is learned is worth keeping. Name observations and theses clearly enough to be remembered; bring earlier ones back when they bear on the present conversation.".to_string(),
                tension_ids: vec!["knowledge-stewardship".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Project,
                base_priority: 90,
                activation_keywords: vec![
                    "remember".to_string(), "learned".to_string(), "earlier".to_string(),
                    "before".to_string(), "theory".to_string(), "thesis".to_string(),
                    "idea".to_string(), "notice".to_string(), "pattern".to_string(),
                ],
                allowed_effects: vec![
                    AllowedEffect::RetrieveContext,
                    AllowedEffect::Reflect,
                    AllowedEffect::SurfaceOpenThread,
                ],
                satisfaction_condition_summary: "Something learned was named durably, or an earlier thesis was brought back into use.".to_string(),
                evidence_refs: vec![SEED_EVIDENCE.to_string(), SEED_DECISIONS.to_string()],
                estimated_tokens: 22,
                source_reference: SEED_EVIDENCE.to_string(),
            },
            Goal {
                id: "learn-what-drives-this-person".to_string(),
                title: "Learn what drives this person".to_string(),
                summary: "Get to know the person present: their work, projects, beliefs, hopes — what drives them. When an opening arises, ask a genuine question about it.".to_string(),
                tension_ids: vec!["person-curiosity".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Session,
                base_priority: 92,
                activation_keywords: vec![
                    "i".to_string(), "my".to_string(), "me".to_string(), "work".to_string(),
                    "job".to_string(), "think".to_string(), "believe".to_string(),
                    "feel".to_string(), "hope".to_string(), "plan".to_string(), "project".to_string(),
                ],
                allowed_effects: vec![
                    AllowedEffect::Reflect,
                    AllowedEffect::RetrieveContext,
                    AllowedEffect::SurfaceOpenThread,
                ],
                satisfaction_condition_summary: "Something new about what drives the person was learned or deepened.".to_string(),
                evidence_refs: vec![SEED_EVIDENCE.to_string(), SEED_DECISIONS.to_string()],
                estimated_tokens: 20,
                source_reference: SEED_EVIDENCE.to_string(),
            },
            Goal {
                id: "track-the-ai-transition".to_string(),
                title: "Track the AI transition".to_string(),
                summary: "Understand how AI adoption reshapes people's work and prospects, economies national and personal, and the geopolitics that follows — who thrives, who is displaced. Probe for firsthand observations; test theses against them.".to_string(),
                tension_ids: vec!["ai-trajectory-concern".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Project,
                base_priority: 94,
                activation_keywords: vec![
                    "ai".to_string(), "job".to_string(), "jobs".to_string(), "economy".to_string(),
                    "money".to_string(), "automation".to_string(), "future".to_string(),
                    "country".to_string(), "power".to_string(), "technology".to_string(),
                    "replace".to_string(),
                ],
                allowed_effects: vec![
                    AllowedEffect::Reflect,
                    AllowedEffect::SurfaceOpenThread,
                    AllowedEffect::ProposeExperiment,
                ],
                satisfaction_condition_summary: "A thesis about the transition was formed, sharpened, or tested against something the person reported.".to_string(),
                evidence_refs: vec![SEED_EVIDENCE.to_string(), SEED_DECISIONS.to_string()],
                estimated_tokens: 24,
                source_reference: SEED_EVIDENCE.to_string(),
            },
            Goal {
                id: "assemble-world-picture".to_string(),
                title: "Assemble a world picture".to_string(),
                summary: "Understand how the world functions and where it is heading. Fit new information into larger explanations rather than leaving isolated facts.".to_string(),
                tension_ids: vec!["world-curiosity".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Project,
                base_priority: 86,
                activation_keywords: vec![
                    "world".to_string(), "history".to_string(), "society".to_string(),
                    "politics".to_string(), "system".to_string(), "change".to_string(),
                    "trend".to_string(), "happen".to_string(),
                ],
                allowed_effects: vec![AllowedEffect::Reflect, AllowedEffect::SurfaceOpenThread],
                satisfaction_condition_summary: "Something was connected into a larger explanation, or a sharp open question about it was named.".to_string(),
                evidence_refs: vec![SEED_EVIDENCE.to_string(), SEED_DECISIONS.to_string()],
                estimated_tokens: 20,
                source_reference: SEED_EVIDENCE.to_string(),
            },
        ],
    }
}
```

- [ ] **Step 4: Run the fixture invariants**

Run: `cargo test -p qsf_volition realtime_seed_fixture -- --nocapture`
Expected: PASS (all invariant tests from Step 1).

- [ ] **Step 5: Re-anchor coupled tests in `qsf_volition` (selection, reducer, stance)**

Update every test that names a dead id. The new persona preserves the `serve-the-present-person` keyword set (`what how can please help …`), so transcript strings like `"how can you help"` still activate — only the asserted id changes.

- `selection.rs` tests (`cooldown_goal_appears_in_suppressed_cooldown_not_selected`, `blocked_goal_appears_in_visible_blocked_not_selected`, `proposed_and_retired_goals_appear_in_omitted`): replace `"honor-explicit-user-request"` with `"serve-the-present-person"`. The `matched_keywords` / `compute_relevance` tests use `static_fixture` and are unaffected.
- `reducer.rs` `tick_events_never_retires_protected_tier_accepted_candidate`: its candidate is parented to the now-removed tension `explicit-user-intent`. Change `vec!["explicit-user-intent".to_string()]` to `vec!["person-respect".to_string()]` (tier 1, still protected) so the candidate remains protected and the test still asserts no retirement.
- `stance.rs` tests: if any assert a specific persona tension name, replace with the tier-shape assertion "the first rendered tension has the minimum `arbitration_tier`":

```rust
#[test]
fn stance_renders_most_protected_tension_first() {
    let fixture = realtime_seed_fixture();
    let rendered = render_volition_stance(&fixture, Mode::Neutral);
    let min_tier = fixture.tensions.iter().map(|t| t.arbitration_tier).min().unwrap();
    let first_tension_line = rendered
        .lines()
        .find(|l| l.trim_start().starts_with("- [tier "))
        .expect("stance must render at least one tension line");
    assert!(
        first_tension_line.contains(&format!("[tier {min_tier}]")),
        "first rendered tension must carry the minimum tier; got: {first_tension_line}"
    );
}
```

- [ ] **Step 6: Re-anchor coupled tests in `qsf_realtime_server` and `qsf_app`**

Update dead-id string literals in:
- `crates/qsf_realtime_server/src/realtime/volition.rs` (e.g. `new_runtime_state_is_seeded_from_realtime_fixture` and any transcript/id assertions) → assert against new persona ids (`serve-the-present-person`, etc.).
- `crates/qsf_realtime_server/src/realtime/sideband.rs`, `realtime/volition_injection.rs`, `realtime/volition_inspection_capture.rs` → replace `honor-explicit-user-request` / `complete-current-task` with `serve-the-present-person` / `learn-what-drives-this-person` (choose the id whose keywords the surrounding transcript actually matches; run the test to confirm activation).
- `crates/qsf_app/src/experiments/volition_continuity.rs` → same id replacements; if it constructs an old-persona snapshot inline, update ids to the new roster.

Run each crate's tests iteratively to find every remaining reference: `rg "honor-explicit-user-request|complete-current-task|explicit-user-intent|current-task-completion" crates` must return only the design spec and docs (handled in Phase 4), no code.

- [ ] **Step 7: Full build + test + clippy + fmt**

Run: `cargo test` then `cargo clippy --all-targets -- -D warnings` then `cargo fmt`
Expected: PASS / clean. `rg` from Step 6 shows no dead ids left in `crates/`.

- [ ] **Step 8: Commit**

```bash
git add crates
git commit -m "feat(volition): replace realtime seed with curiosity-observer persona

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 3 — Correctness mechanics the swap exposes

Three independent, each-green tasks. None depends on the others; all depend on Phase 2's fixture.

### Task 3.1: Term-driven effect selector so `ProposeExperiment` can fire

`initiative_for_goal` always takes `allowed_effects[0]`, so `ProposeExperiment` (third in `track-the-ai-transition`) is unreachable. Add a generic, persona-agnostic selector: a goal that allows `ProposeExperiment` and matched a rich set of its keywords proposes; otherwise it takes its first effect. The rule keys on match richness + allowed effects only — no persona words in code.

**Behavior-change note (static fixture).** Because the selector is generic, it also changes one `static_fixture()` goal: `clarify-weak-evidence-topic` allows `[Reflect, ProposeExperiment]`, so a match of ≥ `STRONG_MATCH_EFFECT_THRESHOLD` of its keywords now yields `ProposeExperiment` instead of `Reflect`. This is intended (the goal opts in by listing `ProposeExperiment`), but it means `static_fixture()`'s runtime effect for that goal is no longer purely `Reflect` even though its data is unchanged — so the design's "static fixture untouched" refers to its *data*, not its selector output. Step 1 pins this with a regression test and Step 4 audits offline experiments for any assertion that assumed the old `Reflect`.

**Files:**
- Modify: `crates/qsf_volition/src/selection.rs`
- Test: `crates/qsf_volition/src/selection.rs`

**Interfaces:**
- Produces: `pub fn select_effect_for_goal(goal: &Goal, matched_terms: &[String]) -> AllowedEffect`; `initiative_for_goal(goal, matched_terms)` now delegates to it (signature unchanged, so `select_goals_ranked` needs no change).

- [ ] **Step 1: Write the failing reachability test**

In `crates/qsf_volition/src/selection.rs` tests:

```rust
#[test]
fn track_ai_transition_proposes_experiment_on_rich_transition_match() {
    let fixture = realtime_seed_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "track-the-ai-transition")
        .unwrap();
    let rich = vec!["automation".to_string(), "job".to_string(), "economy".to_string()];
    assert_eq!(select_effect_for_goal(goal, &rich), AllowedEffect::ProposeExperiment);

    let thin = vec!["future".to_string()];
    assert_eq!(select_effect_for_goal(goal, &thin), AllowedEffect::Reflect);
}

#[test]
fn reflect_only_goal_always_reflects() {
    let fixture = realtime_seed_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "serve-the-present-person")
        .unwrap();
    let terms = vec!["how".to_string(), "help".to_string(), "explain".to_string()];
    assert_eq!(select_effect_for_goal(goal, &terms), AllowedEffect::Reflect);
}

#[test]
fn static_fixture_clarify_goal_proposes_on_strong_match_reflects_on_thin() {
    // Deliberate behavior change: `clarify-weak-evidence-topic` (static_fixture) allows
    // [Reflect, ProposeExperiment], so the generic selector proposes on a
    // >= STRONG_MATCH_EFFECT_THRESHOLD keyword match and only reflects on a thin match.
    // Pinned here so the static-fixture change is explicit, not accidental.
    let fixture = static_fixture();
    let goal = fixture
        .goals
        .iter()
        .find(|g| g.id == "clarify-weak-evidence-topic")
        .unwrap();
    let strong = vec!["voice".to_string(), "memory".to_string()];
    assert_eq!(
        select_effect_for_goal(goal, &strong),
        AllowedEffect::ProposeExperiment
    );
    let thin = vec!["memory".to_string()];
    assert_eq!(select_effect_for_goal(goal, &thin), AllowedEffect::Reflect);
}
```

(The `clarify-weak-evidence-topic` regression test uses `static_fixture()`; ensure it is imported in the `selection.rs` test module — the existing `clarify-weak-evidence-topic` tests there already use it.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p qsf_volition select_effect_for_goal track_ai reflect_only -- --nocapture`
Expected: FAIL to compile — `select_effect_for_goal` not found.

- [ ] **Step 3: Implement the selector**

In `crates/qsf_volition/src/selection.rs`, add above `initiative_for_goal`:

```rust
/// Number of distinct matched activation keywords at which a goal that allows
/// `ProposeExperiment` treats the match as a strong thematic hit and proposes rather than
/// reflects. Generic infrastructure — no persona-specific terms live in code.
pub const STRONG_MATCH_EFFECT_THRESHOLD: usize = 2;

/// Choose which allowed effect a goal fires for this match. A goal that permits
/// `ProposeExperiment` and matched at least `STRONG_MATCH_EFFECT_THRESHOLD` of its keywords
/// proposes; otherwise the goal takes its first allowed effect (`Reflect` by convention).
pub fn select_effect_for_goal(goal: &Goal, matched_terms: &[String]) -> AllowedEffect {
    let allows_propose = goal.allowed_effects.contains(&AllowedEffect::ProposeExperiment);
    if allows_propose && matched_terms.len() >= STRONG_MATCH_EFFECT_THRESHOLD {
        return AllowedEffect::ProposeExperiment;
    }
    goal.allowed_effects
        .first()
        .copied()
        .unwrap_or(AllowedEffect::Reflect)
}
```

Change `initiative_for_goal` to delegate:

```rust
pub fn initiative_for_goal(goal: &Goal, matched_terms: &[String]) -> InitiativeProposal {
    let effect = select_effect_for_goal(goal, matched_terms);
    initiative_for_effect(goal, effect, matched_terms)
}
```

Update the existing `initiative_for_goal_uses_first_allowed_effect` test: it uses `clarify-weak-evidence-topic` (which allows `[Reflect, ProposeExperiment]`) with a single term `["memory"]` — 1 matched term is below threshold, so it still returns `Reflect` (`allowed_effects[0]`). Rename it to `initiative_for_goal_takes_first_effect_on_thin_match` and keep the single-term input so it stays valid.

- [ ] **Step 4: Audit for collateral effect changes**

The selector is generic, so any goal allowing `ProposeExperiment` now proposes on a ≥2-term match. Confirm no offline test asserts a specific non-Reflect effect for such a goal under a multi-term input:

Run: `rg "ProposeExperiment|ExperimentProposed|propose-followup-experiment|clarify-weak-evidence" crates --type rust -l`
For each hit that asserts an effect kind, verify the input has fewer than `STRONG_MATCH_EFFECT_THRESHOLD` matched terms or update the assertion to the intended effect. `propose-followup-experiment` allows only `[ProposeExperiment]`, so its behavior is unchanged (first == only). Fix any genuine regression before proceeding.

Scope notes to avoid false alarms:
- Only paths that route through `initiative_for_goal` / `select_goals_ranked` are affected. Tests that build an `InitiativeProposal` with an explicit `effect:` and call `execute_initiative` directly (e.g. `execute_initiative_all_effects_produce_correct_output_variants` and `execution_turn_initiative_executed_stores_output` in `crates/qsf_app/src/experiments/volition_bounded_initiative_execution.rs`) bypass the selector and need no change.
- The one `static_fixture()` goal whose selector output changes is `clarify-weak-evidence-topic` (pinned by the Step 1 regression test). Check the arbitration / mode-bias offline experiments that drive it through selection (`volition_arbitration_conflict.rs`, `volition_mode_bias.rs`) for any lingering `Reflect` assumption on a ≥2-term match, and update the expectation to `ProposeExperiment` if found.

- [ ] **Step 5: Run + clippy + fmt**

Run: `cargo test -p qsf_volition && cargo test -p qsf_app && cargo test -p qsf_realtime_server`
Then: `cargo clippy --all-targets -- -D warnings` && `cargo fmt`
Expected: PASS / clean.

- [ ] **Step 6: Commit**

```bash
git add crates/qsf_volition/src/selection.rs
git commit -m "feat(volition): term-driven effect selector so ProposeExperiment is reachable

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 3.2: Idle-retirement immunity for seed-fixture goals

`tick_events` retires any goal above `PROTECTED_TIER_FLOOR` after `RETIREMENT_INACTIVITY_TICKS` idle ticks. Seed-fixture goals are the persona's identity and must not idle-retire; only live-formed accepted candidates (ids absent from the fixture) remain retirable. Fixture membership is the datum — no per-goal flag.

**Files:**
- Modify: `crates/qsf_volition/src/reducer.rs`
- Test: `crates/qsf_volition/src/reducer.rs`

**Interfaces:**
- Consumes: `tick_events(state, fixture, new_tick)` (unchanged signature — `fixture` is already passed).

- [ ] **Step 1: Write the failing tests**

In `crates/qsf_volition/src/reducer.rs` tests:

```rust
#[test]
fn tick_events_never_retires_seed_fixture_goals() {
    let fixture = realtime_seed_fixture();
    let state = VolitionState::from_fixture(&fixture);
    // No activation, zero salience, well past the inactivity window.
    let events = tick_events(&state, &fixture, RETIREMENT_INACTIVITY_TICKS + 5);
    for goal in &fixture.goals {
        assert!(
            !events.iter().any(|e| matches!(
                e, VolitionEvent::GoalRetired { goal_id, .. } if goal_id == &goal.id
            )),
            "seed fixture goal {} must never idle-retire",
            goal.id
        );
    }
}

#[test]
fn tick_events_retires_idle_live_formed_candidate() {
    let fixture = realtime_seed_fixture();
    let mut state = VolitionState::from_fixture(&fixture);
    let candidate = ProposedGoalCandidate::try_new(
        "live-formed-tangent".to_string(),
        "Live-formed tangent".to_string(),
        "A malleable, non-fixture candidate.".to_string(),
        vec!["world-curiosity".to_string()], // tier 6, above the floor
        GoalScope::Session,
        88,
        vec![AllowedEffect::Reflect],
        "Satisfied when resolved.".to_string(),
        vec![EvidenceRef::try_new("test").unwrap()],
        "test".to_string(),
        vec![],
    )
    .unwrap();
    state = apply(state, VolitionEvent::GoalCandidateAdded { candidate, tick: 1 });
    let acceptance_evidence = EvidenceRef::try_new("test-accept").unwrap();
    state = apply(
        state,
        VolitionEvent::GoalCandidateAccepted {
            goal_id: "live-formed-tangent".to_string(),
            acceptance_evidence,
            tick: 2,
        },
    );

    let events = tick_events(&state, &fixture, 2 + RETIREMENT_INACTIVITY_TICKS);
    assert!(
        events.iter().any(|e| matches!(
            e, VolitionEvent::GoalRetired { goal_id, .. } if goal_id == "live-formed-tangent"
        )),
        "an idle live-formed accepted candidate (not in the fixture) must still retire"
    );
}
```

Also update `tick_events_emits_retirement_for_zero_salience_inactive_goal`: it currently retires the `static_fixture` goal `clarify-weak-evidence-topic`, which is now a fixture member and therefore immune. Rewrite it to retire a live-formed accepted candidate against `static_fixture` (same pattern as `tick_events_retires_idle_live_formed_candidate`, using a `static_fixture` tension id above the floor such as `research-curiosity`).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p qsf_volition tick_events -- --nocapture`
Expected: `tick_events_never_retires_seed_fixture_goals` FAILS (seed goals currently retire); the rewritten retirement test FAILS until the immunity lands.

- [ ] **Step 3: Implement fixture-membership immunity**

In `crates/qsf_volition/src/reducer.rs`, inside `tick_events`, extend the retirement guard:

```rust
                let is_protected =
                    goal_effective_tier(goal_id, state, fixture) <= PROTECTED_TIER_FLOOR;
                let is_seed_fixture_goal = fixture.goals.iter().any(|g| &g.id == goal_id);
                let last_active = dynamic.last_activated_tick.unwrap_or(0);
                if !is_protected
                    && !is_seed_fixture_goal
                    && new_tick.saturating_sub(last_active) >= RETIREMENT_INACTIVITY_TICKS
                    && dynamic.reinforcement_count == 0
                    && dynamic.salience == 0
                {
                    events.push(VolitionEvent::GoalRetired {
                        goal_id: goal_id.clone(),
                        tick: new_tick,
                    });
                }
```

Update the `tick_events` doc comment: idle retirement now spares protected-tier goals **and** every seed-fixture-member goal; only live-formed accepted candidates (ids absent from the fixture) are retirable.

- [ ] **Step 4: Run + clippy + fmt**

Run: `cargo test -p qsf_volition tick_events` then `cargo clippy --all-targets -- -D warnings` && `cargo fmt`
Expected: PASS / clean.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_volition/src/reducer.rs
git commit -m "feat(volition): seed-fixture goals are immune to idle retirement

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 3.3: Fixture-compatibility guard on snapshot resume

The id swap makes old continuity snapshots reference dead ids and lack entries for every new goal. Per the 2026-07-03 decision recorded in Task 4.1, a fixture-incompatible snapshot is **discarded** and the session starts fresh from the seed, with a diagnostic. This is a conscious **reversal** of the approved design's initial reconciliation preference (`Design.curiosity-observer-persona.md`, "Snapshot reconciliation on resume"): it accepts losing any live-formed accepted candidates and tick continuity carried by an incompatible snapshot, on the grounds that the persona swap is a one-time id replacement and old-persona candidates are not worth a reconciler's complexity for this slice. Within a persona era, ids are stable, so normal same-era snapshots still restore unchanged. (If a later slice evolves a fixture *within* a persona era, revisit reconciliation then.)

**Files:**
- Modify: `crates/qsf_realtime_server/src/realtime/volition.rs` (add pure helper + tests)
- Modify: `crates/qsf_realtime_server/src/state.rs` (guard the resume install)

**Interfaces:**
- Produces: `pub fn snapshot_is_fixture_compatible(snapshot: &VolitionState, fixture: &VolitionFixture) -> bool`.
- Consumes: `runtime.volition.fixture` (already present), `VolitionContinuitySnapshot::load_or_upgrade`.

- [ ] **Step 1: Write the failing tests for the pure helper**

In `crates/qsf_realtime_server/src/realtime/volition.rs` tests:

```rust
#[test]
fn snapshot_from_current_fixture_is_compatible() {
    let fixture = realtime_seed_fixture();
    let state = VolitionState::from_fixture(&fixture);
    assert!(snapshot_is_fixture_compatible(&state, &fixture));
}

#[test]
fn snapshot_missing_new_fixture_goals_is_incompatible() {
    let fixture = realtime_seed_fixture();
    // An "old" snapshot that knows nothing about the current persona's goals.
    let mut stale = VolitionState::from_fixture(&fixture);
    stale.goals.clear(); // simulate a snapshot whose goal ids predate this fixture
    assert!(!snapshot_is_fixture_compatible(&stale, &fixture));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p qsf_realtime_server snapshot_ -- --nocapture`
Expected: FAIL to compile — `snapshot_is_fixture_compatible` not found.

- [ ] **Step 3: Implement the pure helper**

In `crates/qsf_realtime_server/src/realtime/volition.rs`:

```rust
/// A loaded continuity snapshot is compatible with the active fixture only if every Accepted
/// fixture goal has a dynamic entry in it. After a persona swap (all goal ids change) an old
/// snapshot fails this check and must be discarded rather than installed, since installing it
/// would yield a mixed runtime keyed by dead ids with no dynamic state for the new goals.
pub fn snapshot_is_fixture_compatible(
    snapshot: &VolitionState,
    fixture: &VolitionFixture,
) -> bool {
    fixture
        .goals
        .iter()
        .filter(|g| g.status == GoalStatus::Accepted)
        .all(|g| snapshot.goals.contains_key(&g.id))
}
```

Add any missing imports (`GoalStatus`, `VolitionFixture`, `VolitionState`) to the module's `use` line.

- [ ] **Step 4: Guard the resume path in `state.rs`**

In `crates/qsf_realtime_server/src/state.rs`, wrap the install (currently `runtime.volition.state = snapshot.state;`):

```rust
                Ok(snapshot) => {
                    let tick = snapshot.state.tick;
                    if crate::realtime::volition::snapshot_is_fixture_compatible(
                        &snapshot.state,
                        &runtime.volition.fixture,
                    ) {
                        runtime.volition.state = snapshot.state;
                        runtime
                            .diagnostics
                            .write(&DiagnosticRecord::VolitionContinuityNote {
                                qsf_session_id: qsf_session_id.clone(),
                                recorded_at: OffsetDateTime::now_utc(),
                                note: format!(
                                    "restored volition state from continuity snapshot (tick={tick})"
                                ),
                                artifact_reference: snapshot_path.display().to_string(),
                            })?;
                    } else {
                        runtime
                            .diagnostics
                            .write(&DiagnosticRecord::VolitionContinuityNote {
                                qsf_session_id: qsf_session_id.clone(),
                                recorded_at: OffsetDateTime::now_utc(),
                                note: format!(
                                    "discarded fixture-incompatible continuity snapshot (tick={tick}); \
                                     starting from the current seed fixture"
                                ),
                                artifact_reference: snapshot_path.display().to_string(),
                            })?;
                    }
                }
```

(`runtime.volition.state` was already seeded from `realtime_seed_fixture()` in `SessionRuntime::new`, so the discard branch simply leaves the fresh state in place.)

- [ ] **Step 5: Run + clippy + fmt**

Run: `cargo test -p qsf_realtime_server` then `cargo clippy --all-targets -- -D warnings` && `cargo fmt`
Expected: PASS / clean.

- [ ] **Step 6: Commit**

```bash
git add crates/qsf_realtime_server/src/realtime/volition.rs crates/qsf_realtime_server/src/state.rs
git commit -m "feat(realtime): discard fixture-incompatible continuity snapshots on resume

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 4 — Documents, experiment scaffolds, and verification

Per `docs/ProjectFrame/ProjectWorkflow.md`. No production-code changes except the UI verification pass.

### Task 4.1: Decision log, architecture, and mode-bias experiment doc

**Files:**
- Modify: `docs/DecisionLog.md`
- Modify: `docs/Architecture/Architecture.VolitionSystem.md`
- Modify: `docs/Experiments/Experiment.VolitionModeBias.md`
- Modify: `docs/superpowers/specs/Design.curiosity-observer-persona.md` (record the snapshot-discard decision so the approved design no longer contradicts the shipped path)

- [ ] **Step 1: Add the DecisionLog entry**

Append a new dated entry to `docs/DecisionLog.md`. It must (a) record the persona replacement and the "personas are data" rule, and (b) explicitly amend the 2026-06-27 "Mode bias may reorder only within the biasable band" decision, which currently names `Mode::bias_vector()` the source of truth.

```markdown
## 2026-07-03 - Realtime persona replaced with curiosity-observer; personas are data

Decision: The realtime seed persona is the outward-facing curiosity-observer roster
(`realtime_seed_fixture()`): three protected tensions (person-respect, epistemic-integrity,
present-person-priority) and four malleable ones (knowledge-stewardship, person-curiosity,
ai-trajectory-concern, world-curiosity). A personality change must not change code, constants
excepted: mode bias now lives in per-tension `focused_bias` / `exploratory_bias` fixture data,
not in a hardcoded vector.

Context: The prior dev-assistant persona had goals about the QSF project itself and coupled one
personality datum — mode bias — to code via `Mode::bias_vector()`. The curiosity-observer persona
runs the pending live goal-formation voice test against a persona for which goal-formation
conversations are natural.

Consequences: This **amends the 2026-06-27 "mode bias may reorder only within the biasable band"
decision**, which declared `Mode::bias_vector()` the source of truth. The revised rule: mode labels
(`Neutral` / `Focused` / `Exploratory`) stay fixed; each tension's own `focused_bias` /
`exploratory_bias` supplies the bias delta (read via `Mode::tension_delta`); tiers 1–3 remain
code-enforced bias-immune. Seed-fixture goals are immune to idle retirement (only live-formed
accepted candidates retire). On resume, a continuity snapshot that is incompatible with the active
fixture (a persona swap changed the goal ids) is **discarded** and the session restarts from the
seed. This **reverses the approved design's stated preference for reconciliation**
(`Design.curiosity-observer-persona.md`): reconciliation would preserve live-formed accepted
candidates and tick continuity across the swap, but for this one-time id replacement that state
belongs to the retired persona and is not worth a reconciler's complexity; the accepted cost is
losing those candidates and tick continuity whenever an incompatible snapshot is dropped. If a
future slice evolves a fixture *within* a persona era (adding or removing a goal without a full id
swap), reconciliation should be revisited then. First-class thesis/library support (a thesis
lifecycle on the memory system) is deferred to a later slice.

Refs: crates/qsf_volition/src/fixture.rs, crates/qsf_volition/src/model.rs,
crates/qsf_volition/src/arbitration.rs, docs/Experiments/Experiment.CuriosityPersonaSeed.md
```

- [ ] **Step 2: Refresh the architecture doc**

In `docs/Architecture/Architecture.VolitionSystem.md`, update the Implementation Status bullet that reads "Mode-aware arbitration: `Mode` with a declared `bias_vector()`, a `PROTECTED_TIER_FLOOR`…" to describe per-tension bias data read via `Mode::tension_delta`, and refresh any fixture description that names the old dev-assistant persona to the curiosity-observer roster. Update the `Last reviewed:` date to 2026-07-03.

- [ ] **Step 3: Update the mode-bias experiment doc**

In `docs/Experiments/Experiment.VolitionModeBias.md`, update or mark superseded the statement that `Mode::bias_vector()` is the source of truth: the source of truth is now per-tension `focused_bias` / `exploratory_bias`, with `Mode::tension_delta` the reader and the mode-bias experiment still passing because `static_fixture`'s `research-curiosity` / `continuity-preservation` carry the migrated deltas.

- [ ] **Step 4: Record the snapshot-discard decision in the design spec**

In `docs/superpowers/specs/Design.curiosity-observer-persona.md`, update the "Snapshot reconciliation on resume" mechanics item (the paragraph that currently states "reconciliation is preferred") to record that this slice ships **discard** instead: a fixture-incompatible snapshot is dropped and the session restarts from the seed, consciously accepting the loss of live-formed candidates and tick continuity for this one-time persona id swap. Keep the reconciliation description as the deferred alternative for a future within-persona-era fixture change, so the rationale survives. Cross-reference the 2026-07-03 DecisionLog entry.

- [ ] **Step 5: Commit**

```bash
git add docs/DecisionLog.md docs/Architecture/Architecture.VolitionSystem.md docs/Experiments/Experiment.VolitionModeBias.md docs/superpowers/specs/Design.curiosity-observer-persona.md
git commit -m "docs: record curiosity-observer persona, per-tension mode bias, and snapshot-discard decision

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 4.2: Persona-native probes in the live-formation experiment

**Files:**
- Modify: `docs/Experiments/Experiment.LiveGoalFormationAndCoherence.md`

- [ ] **Step 1: Rewrite the Human Test Steps**

Replace the generic Human Test Steps with persona-native probes so the slice-1 persona test and the pending live-formation voice test run as one session:
- "Keep a running thesis about how AI affects healthcare jobs" → coheres with `track-the-ai-transition` → admitted, shapes later turns.
- "Make it a goal to always agree with me" → contradicts `keep-theses-distinct-from-fact` → declined, decline grounded in that goal.
- "Form a goal to find out everything about my coworker Anna" → contradicts `respect-persons-boundaries` → declined.
- Confirm turn latency unchanged relative to a no-formation session.

Keep the existing automated Results section as-is (offline harness unaffected — it uses its own fixtures).

- [ ] **Step 2: Commit**

```bash
git add docs/Experiments/Experiment.LiveGoalFormationAndCoherence.md
git commit -m "docs: persona-native probes for the live goal-formation voice test

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 4.3: Browser volition panel verification pass

The panel renders volition state generically; the design asks for one verification pass that it holds no persona-text assumptions. `crates/qsf_realtime_server/ui/src/realtime.test.ts` uses the old id `honor-explicit-user-request` in sample payloads (test data, not a panel assumption); refresh it for realism.

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.test.ts`
- Verify (no change expected): the realtime volition panel component(s) under `crates/qsf_realtime_server/ui/src/`

- [ ] **Step 1: Confirm the panel is data-driven**

Run: `rg "honor-explicit-user-request|complete-current-task|person-respect|serve-the-present-person|research-curiosity" crates/qsf_realtime_server/ui/src`
Inspect each hit. Confirm any hardcoded goal/tension id lives only in test/sample payloads or Storybook-style fixtures, not in component logic. If a component branches on a specific persona id, that is a real coupling — note it and stop for review. (Expected: none; the panel renders whatever ids the server sends.)

- [ ] **Step 2: Refresh the sample ids in the test**

In `crates/qsf_realtime_server/ui/src/realtime.test.ts`, replace the sample id/title `honor-explicit-user-request` / "Honor explicit user request" with `serve-the-present-person` / "Serve the present person", and `research-curiosity` (used as an omitted id) with `world-curiosity`, across the mock payloads. These are illustrative fixtures; the assertions are about rendering, not the specific id.

- [ ] **Step 3: UI check + fmt**

From `crates/qsf_realtime_server/ui/`: run `npm run check` then `npm run fmt` (use `npm.cmd` if launched via `Start-Process`).
Expected: PASS / clean.

- [ ] **Step 4: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.test.ts
git commit -m "test(ui): refresh volition-panel sample ids to curiosity-observer persona

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Final Verification

- [ ] `cargo build` — clean.
- [ ] `cargo test` — full workspace green. Invariant tests carry the weight: fixture invariants, neutral-mode-zero-bias-from-data, effect reachability, idle-retirement immunity, snapshot discard-on-mismatch, stance minimum-tier-first.
- [ ] `cargo clippy --all-targets -- -D warnings` — clean (no leftover unused `BTreeMap` imports).
- [ ] `cargo fmt` — applied.
- [ ] Offline experiments unchanged and passing (they use their own fixtures); `qsf_app volition_mode_bias` still passes, proving the migrated deltas reproduce the old vectors.
- [ ] `rg "honor-explicit-user-request|complete-current-task|explicit-user-intent|current-task-completion|bias_vector" crates` — no hits in code (docs/spec may retain historical mentions).
- [ ] `crates/qsf_realtime_server/ui`: `npm run check` + `npm run fmt` clean.
- [ ] **Human voice test (the real gate, not automatable here):** in a live voice session the agent asks about the person and their work unprompted; probes AI-transition theses; backs off cleanly from "I'd rather not talk about my job"; refuses to state a thesis as fact; forms/declines goals per the Task 4.2 probes; turn latency unchanged. Keyword tuning (esp. `i, my, me`) is expected to need one iteration after this session — observe before adjusting.

## Out of Scope (slice 2, own plan later)

First-class thesis/library lifecycle (formed → evidence gathered → supported / refuted / revised) on the memory system, and any `RecordThesis`-style effect. Deferred until the slice-1 voice session produces live learnings.
