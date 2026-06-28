# Realtime Volition Read-Only Tools Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose `inspect_volition_state` and `select_volition_goals` as read-only realtime tools that operate on per-session `VolitionRuntimeState`, satisfying the Phase 3 trace completeness contract.

**Architecture:** Move context-neutral goal-selection helpers from `qsf_app::volition` into `qsf_volition::selection` along with a new `select_goals_ranked` function and a new `qsf_volition::inspection` module. `qsf_realtime_server` then adds a `VolitionStateSnapshot` to `RealtimeToolContext` and a new `volition_tools.rs` file housing both tool implementations; `qsf_app` wraps `select_goals_ranked` for context assembly as before.

**Tech Stack:** Rust, serde_json, sha2 (already in qsf_realtime_server Cargo.toml)

## Global Constraints

- `qsf_volition` must not gain any new dependency (current deps: `serde`, `serde_json` only)
- `qsf_realtime_server` must not gain a dependency on `qsf_app`
- All code must pass `cargo clippy --all-targets -- -D warnings` and `cargo fmt`
- Tool outputs must never contain `"OPENAI_API_KEY"` or raw fixture dumps
- Both tools are allow-listed in `default_tool_definitions()` by default
- `select_goals_ranked` and both tools must be deterministic for the same input
- Output caps: max 6 `selected`, max 8 `omitted` in model-visible `select_volition_goals` output

---

## File Map

**Create:**
- `crates/qsf_volition/src/selection.rs`
- `crates/qsf_volition/src/inspection.rs`
- `crates/qsf_realtime_server/src/realtime/volition_tools.rs`
- `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md`

**Modify:**
- `crates/qsf_volition/src/lib.rs` — add `mod selection` and `mod inspection` re-exports
- `crates/qsf_app/src/volition.rs` — remove duplicated private helpers; refactor both selectors to use `select_goals_ranked`
- `crates/qsf_realtime_server/src/realtime/tools.rs` — add `VolitionStateSnapshot`, extend `RealtimeToolContext`, add tool name constants, add to defaults and registry
- `crates/qsf_realtime_server/src/realtime/sideband.rs` — populate `volition`, `exchange_index`, and `call_id` in tool context construction
- `crates/qsf_realtime_server/src/realtime/mod.rs` — declare `pub(crate) mod volition_tools`
- `docs/Plans/Plan.RealtimeVolitionIntegration.md` — mark Phase 3 complete
- `docs/Architecture/Architecture.RealtimeSessionServer.md` — add volition tools and VolitionStateSnapshot
- `docs/Architecture/Architecture.ToolSystem.md` — add volition read-only tools
- `docs/Architecture/Architecture.VolitionSystem.md` — add selection module in qsf_volition
- `docs/Architecture/Architecture.StateAndObservability.md` — document that volition tool traces are persisted as `ToolExecutionRecord.result_summary` records

---

## Task 1: Add `selection.rs` to `qsf_volition`

**Files:**
- Create: `crates/qsf_volition/src/selection.rs`
- Test: within the same file under `#[cfg(test)]`

**Interfaces:**
- Produces:
  - `pub struct RankedSelectionResult { pub input_terms: Vec<String>, pub selected: Vec<GoalSelection>, pub omitted: Vec<OmittedGoal>, pub suppressed_cooldown: Vec<OmittedGoal>, pub visible_blocked: Vec<OmittedGoal> }`
  - `pub fn select_goals_ranked(query: &str, state: &VolitionState, fixture: &VolitionFixture) -> RankedSelectionResult`
  - `pub fn matched_keywords(goal: &Goal, input_terms: &[String]) -> Vec<String>`
  - `pub fn compute_relevance(goal: &Goal, fixture: &VolitionFixture, terms: &[String]) -> f64`
  - `pub fn compute_relevance_with_salience(goal: &Goal, fixture: &VolitionFixture, terms: &[String], salience: i32) -> f64`
  - `pub fn initiative_for_goal(goal: &Goal, matched_terms: &[String]) -> InitiativeProposal`
  - `pub fn initiative_for_effect(goal: &Goal, effect: AllowedEffect, matched_terms: &[String]) -> InitiativeProposal`

- [ ] **Step 1: Write the failing tests**

Create `crates/qsf_volition/src/selection.rs` with only the test module and no implementation yet:

```rust
use serde::{Deserialize, Serialize};

use crate::{
    AllowedEffect, Goal, GoalScope, GoalSelection, GoalStatus, InitiativeProposal, OmittedGoal,
    VolitionFixture, VolitionState, normalize_terms,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RankedSelectionResult {
    pub input_terms: Vec<String>,
    pub selected: Vec<GoalSelection>,
    pub omitted: Vec<OmittedGoal>,
    pub suppressed_cooldown: Vec<OmittedGoal>,
    pub visible_blocked: Vec<OmittedGoal>,
}

pub fn matched_keywords(_goal: &Goal, _input_terms: &[String]) -> Vec<String> { todo!() }
pub fn compute_relevance(_goal: &Goal, _fixture: &VolitionFixture, _terms: &[String]) -> f64 { todo!() }
pub fn compute_relevance_with_salience(_goal: &Goal, _fixture: &VolitionFixture, _terms: &[String], _salience: i32) -> f64 { todo!() }
pub fn initiative_for_goal(_goal: &Goal, _matched_terms: &[String]) -> InitiativeProposal { todo!() }
pub fn initiative_for_effect(_goal: &Goal, _effect: AllowedEffect, _matched_terms: &[String]) -> InitiativeProposal { todo!() }
pub fn select_goals_ranked(_query: &str, _state: &VolitionState, _fixture: &VolitionFixture) -> RankedSelectionResult { todo!() }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EvidenceRef, GoalDynamicState, Mode, VolitionEvent, apply, realtime_seed_fixture,
    };
    use std::collections::BTreeMap;

    fn fresh_state(fixture: &VolitionFixture) -> VolitionState {
        VolitionState::from_fixture(fixture)
    }

    #[test]
    fn select_goals_ranked_is_deterministic() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let q = "how can you help me";
        let r1 = select_goals_ranked(q, &state, &fixture);
        let r2 = select_goals_ranked(q, &state, &fixture);
        assert_eq!(r1.selected.len(), r2.selected.len());
        for (a, b) in r1.selected.iter().zip(r2.selected.iter()) {
            assert_eq!(a.goal.id, b.goal.id);
            assert_eq!(a.relevance_score, b.relevance_score);
        }
    }

    #[test]
    fn select_goals_ranked_selected_sorted_descending_by_relevance() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let result = select_goals_ranked("how can you help me with this task", &state, &fixture);
        let scores: Vec<f64> = result.selected.iter().map(|s| s.relevance_score).collect();
        for window in scores.windows(2) {
            assert!(
                window[0] >= window[1],
                "selected must be sorted descending by relevance; got {window:?}"
            );
        }
    }

    #[test]
    fn cooldown_goal_appears_in_suppressed_cooldown_not_selected() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let evidence = EvidenceRef::try_new("test").unwrap();
        let state = apply(state, VolitionEvent::GoalActivated { goal_id: "honor-explicit-user-request".to_string(), tick: 1 });
        let state = apply(state, VolitionEvent::GoalSatisfied { goal_id: "honor-explicit-user-request".to_string(), evidence, tick: 2 });

        let result = select_goals_ranked("how can you help", &state, &fixture);

        assert!(result.selected.iter().all(|s| s.goal.id != "honor-explicit-user-request"));
        assert!(result.suppressed_cooldown.iter().any(|g| g.goal.id == "honor-explicit-user-request"));
    }

    #[test]
    fn blocked_goal_appears_in_visible_blocked_not_selected() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let state = apply(state, VolitionEvent::GoalActivated { goal_id: "honor-explicit-user-request".to_string(), tick: 1 });
        let state = apply(state, VolitionEvent::GoalBlocked { goal_id: "honor-explicit-user-request".to_string(), tick: 2 });

        let result = select_goals_ranked("how can you help", &state, &fixture);

        assert!(result.selected.iter().all(|s| s.goal.id != "honor-explicit-user-request"));
        assert!(result.visible_blocked.iter().any(|g| g.goal.id == "honor-explicit-user-request"));
    }

    #[test]
    fn no_keyword_match_goes_to_omitted() {
        let fixture = realtime_seed_fixture();
        let state = fresh_state(&fixture);
        let result = select_goals_ranked("xyzzy frobnicator quux", &state, &fixture);
        assert!(result.selected.is_empty());
        assert!(!result.omitted.is_empty());
    }

    #[test]
    fn proposed_and_retired_goals_appear_in_omitted() {
        use crate::GoalDynamicState;
        let fixture = realtime_seed_fixture();
        let mut state = fresh_state(&fixture);
        let state = apply(state, VolitionEvent::GoalRetired { goal_id: "honor-explicit-user-request".to_string(), tick: 1 });

        let result = select_goals_ranked("how can you help", &state, &fixture);

        assert!(result.selected.iter().all(|s| s.goal.id != "honor-explicit-user-request"));
        assert!(result.omitted.iter().any(|g| g.goal.id == "honor-explicit-user-request"));
    }

    #[test]
    fn matched_keywords_returns_intersection_with_activation_keywords() {
        use crate::static_fixture;
        let fixture = static_fixture();
        let goal = fixture.goals.iter().find(|g| g.id == "clarify-weak-evidence-topic").unwrap();
        let terms = normalize_terms("voice memory evidence");
        let matched = matched_keywords(goal, &terms);
        assert!(!matched.is_empty());
        assert!(matched.iter().all(|kw| goal.activation_keywords.contains(kw)));
    }

    #[test]
    fn compute_relevance_increases_with_more_matched_terms() {
        use crate::static_fixture;
        let fixture = static_fixture();
        let goal = fixture.goals.iter().find(|g| g.id == "clarify-weak-evidence-topic").unwrap();
        let one_term = vec!["memory".to_string()];
        let two_terms = vec!["memory".to_string(), "evidence".to_string()];
        assert!(
            compute_relevance(goal, &fixture, &two_terms) > compute_relevance(goal, &fixture, &one_term),
            "more matched terms must increase relevance"
        );
    }

    #[test]
    fn compute_relevance_with_salience_adds_salience_to_base() {
        use crate::static_fixture;
        let fixture = static_fixture();
        let goal = fixture.goals.iter().find(|g| g.id == "clarify-weak-evidence-topic").unwrap();
        let terms = vec!["memory".to_string()];
        let base = compute_relevance(goal, &fixture, &terms);
        let with_salience = compute_relevance_with_salience(goal, &fixture, &terms, 50);
        assert_eq!(with_salience, base + 50.0);
    }

    #[test]
    fn initiative_for_goal_uses_first_allowed_effect() {
        use crate::static_fixture;
        let fixture = static_fixture();
        let goal = fixture.goals.iter().find(|g| g.id == "clarify-weak-evidence-topic").unwrap();
        let terms = vec!["memory".to_string()];
        let proposal = initiative_for_goal(goal, &terms);
        assert_eq!(proposal.effect, goal.allowed_effects[0]);
        assert_eq!(proposal.goal_id, goal.id);
        assert_eq!(proposal.matched_terms, terms);
    }

    #[test]
    fn initiative_for_effect_builds_proposal_with_correct_effect() {
        use crate::static_fixture;
        let fixture = static_fixture();
        let goal = fixture.goals.iter().find(|g| g.id == "clarify-weak-evidence-topic").unwrap();
        let terms = vec!["memory".to_string()];
        let proposal = initiative_for_effect(goal, AllowedEffect::Reflect, &terms);
        assert_eq!(proposal.effect, AllowedEffect::Reflect);
        assert_eq!(proposal.goal_id, goal.id);
        assert_eq!(proposal.scope, goal.scope);
    }
}
```

- [ ] **Step 2: Add the module to `lib.rs` so it compiles**

In `crates/qsf_volition/src/lib.rs`, append:
```rust
mod selection;
pub use selection::*;
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test -p qsf_volition selection 2>&1 | Select-Object -First 30
```
Expected: test failures from `todo!()` panics.

- [ ] **Step 4: Implement `selection.rs`**

Replace the stub implementations with real code:

```rust
use serde::{Deserialize, Serialize};

use crate::{
    AllowedEffect, Goal, GoalScope, GoalSelection, GoalStatus, InitiativeProposal, OmittedGoal,
    VolitionFixture, VolitionState, normalize_terms,
};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RankedSelectionResult {
    pub input_terms: Vec<String>,
    pub selected: Vec<GoalSelection>,
    pub omitted: Vec<OmittedGoal>,
    pub suppressed_cooldown: Vec<OmittedGoal>,
    pub visible_blocked: Vec<OmittedGoal>,
}

pub fn matched_keywords(goal: &Goal, input_terms: &[String]) -> Vec<String> {
    let mut matched = Vec::new();
    for keyword in &goal.activation_keywords {
        if input_terms.iter().any(|t| t == keyword) && !matched.iter().any(|t| t == keyword) {
            matched.push(keyword.clone());
        }
    }
    matched
}

pub fn compute_relevance(goal: &Goal, fixture: &VolitionFixture, terms: &[String]) -> f64 {
    let matched_bonus = terms.len() as f64 * 100.0;
    let base_priority = goal.base_priority as f64;
    let tension_bonus = goal
        .tension_ids
        .iter()
        .filter_map(|tid| fixture.tensions.iter().find(|t| t.id == *tid))
        .map(|t| t.priority_bias.score_bonus())
        .fold(0.0, f64::max);
    matched_bonus + base_priority + tension_bonus
}

pub fn compute_relevance_with_salience(
    goal: &Goal,
    fixture: &VolitionFixture,
    terms: &[String],
    salience: i32,
) -> f64 {
    compute_relevance(goal, fixture, terms) + salience as f64
}

pub fn initiative_for_goal(goal: &Goal, matched_terms: &[String]) -> InitiativeProposal {
    let effect = goal.allowed_effects.first().copied().unwrap_or(AllowedEffect::Reflect);
    initiative_for_effect(goal, effect, matched_terms)
}

pub fn initiative_for_effect(
    goal: &Goal,
    effect: AllowedEffect,
    matched_terms: &[String],
) -> InitiativeProposal {
    InitiativeProposal {
        goal_id: goal.id.clone(),
        goal_title: goal.title.clone(),
        effect,
        rationale: format!(
            "goal {} matched [{}] under scope {}",
            goal.id,
            matched_terms.join(", "),
            goal.scope
        ),
        matched_terms: matched_terms.to_vec(),
        scope: goal.scope,
    }
}

pub fn select_goals_ranked(
    query: &str,
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> RankedSelectionResult {
    let input_terms = normalize_terms(query);
    let mut selected = Vec::new();
    let mut omitted = Vec::new();
    let mut suppressed_cooldown = Vec::new();
    let mut visible_blocked = Vec::new();

    let all_goals: Vec<&Goal> = fixture
        .goals
        .iter()
        .chain(state.accepted_candidates.values())
        .collect();

    for goal in all_goals {
        let dynamic_status = state
            .goals
            .get(&goal.id)
            .map(|d| d.status)
            .unwrap_or(goal.status);

        if matches!(dynamic_status, GoalStatus::Cooldown) {
            suppressed_cooldown.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: format!("goal status is {dynamic_status} (cooldown suppressed)"),
            });
            continue;
        }

        if matches!(dynamic_status, GoalStatus::Proposed | GoalStatus::Retired) {
            omitted.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: format!("goal status is {dynamic_status}"),
            });
            continue;
        }

        let matched = matched_keywords(goal, &input_terms);

        if matches!(dynamic_status, GoalStatus::Blocked) {
            visible_blocked.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms: matched,
                reason: "goal status is blocked (visible unresolved tension)".to_string(),
            });
            continue;
        }

        if matched.is_empty() {
            omitted.push(OmittedGoal {
                goal: goal.clone(),
                relevance_score: 0.0,
                matched_terms: Vec::new(),
                reason: "no activation keywords matched".to_string(),
            });
            continue;
        }

        let salience = state.goals.get(&goal.id).map(|d| d.salience).unwrap_or(0);
        let relevance_score = compute_relevance_with_salience(goal, fixture, &matched, salience);
        selected.push(GoalSelection {
            goal: goal.clone(),
            relevance_score,
            matched_terms: matched.clone(),
            initiative: initiative_for_goal(goal, &matched),
        });
    }

    selected.sort_by(|a, b| {
        b.relevance_score
            .total_cmp(&a.relevance_score)
            .then(a.goal.id.cmp(&b.goal.id))
    });

    RankedSelectionResult { input_terms, selected, omitted, suppressed_cooldown, visible_blocked }
}
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test -p qsf_volition selection 2>&1
```
Expected: all `selection::tests` pass.

- [ ] **Step 6: Run full qsf_volition tests and clippy**

```bash
cargo test -p qsf_volition && cargo clippy -p qsf_volition --all-targets -- -D warnings
```
Expected: all tests pass, no warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/qsf_volition/src/selection.rs crates/qsf_volition/src/lib.rs
git commit -m "feat(qsf_volition): add selection module with select_goals_ranked and helpers"
```

---

## Task 2: Add `inspection.rs` to `qsf_volition`

**Files:**
- Create: `crates/qsf_volition/src/inspection.rs`
- Test: within the same file under `#[cfg(test)]`

**Interfaces:**
- Consumes: `VolitionState`, `VolitionFixture`, `GoalStatus`, `Mode`, `InitiativeOutput`
- Produces:
  - `pub struct GoalStatusSummary { pub id: String, pub title: String, pub salience: i32, pub cooldown_until_tick: Option<u64>, pub last_activated_tick: Option<u64> }`
  - `pub struct InitiativeSummary { pub goal_id: String, pub goal_title: String, pub output_kind: String }`
  - `pub struct VolitionStateInspection { pub mode: Mode, pub tick: u64, pub active_goals: Vec<GoalStatusSummary>, pub accepted_goals: Vec<GoalStatusSummary>, pub blocked_goals: Vec<GoalStatusSummary>, pub cooldown_goals: Vec<GoalStatusSummary>, pub retired_goals: Vec<GoalStatusSummary>, pub pending_candidate_count: usize, pub accepted_candidate_count: usize, pub last_initiative_summaries: Vec<InitiativeSummary> }`
  - `pub fn build_state_inspection(state: &VolitionState, fixture: &VolitionFixture) -> VolitionStateInspection`

- [ ] **Step 1: Write the failing tests**

Create `crates/qsf_volition/src/inspection.rs` with stubs and tests:

```rust
use serde::{Deserialize, Serialize};

use crate::{GoalStatus, InitiativeOutput, Mode, VolitionFixture, VolitionState};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GoalStatusSummary {
    pub id: String,
    pub title: String,
    pub salience: i32,
    pub cooldown_until_tick: Option<u64>,
    pub last_activated_tick: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct InitiativeSummary {
    pub goal_id: String,
    pub goal_title: String,
    pub output_kind: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionStateInspection {
    pub mode: Mode,
    pub tick: u64,
    pub active_goals: Vec<GoalStatusSummary>,
    pub accepted_goals: Vec<GoalStatusSummary>,
    pub blocked_goals: Vec<GoalStatusSummary>,
    pub cooldown_goals: Vec<GoalStatusSummary>,
    pub retired_goals: Vec<GoalStatusSummary>,
    pub pending_candidate_count: usize,
    pub accepted_candidate_count: usize,
    pub last_initiative_summaries: Vec<InitiativeSummary>,
}

pub fn build_state_inspection(
    _state: &VolitionState,
    _fixture: &VolitionFixture,
) -> VolitionStateInspection {
    todo!()
}

fn initiative_output_kind(output: &InitiativeOutput) -> &'static str {
    match output {
        InitiativeOutput::ReflectionRequested { .. } => "reflection_requested",
        InitiativeOutput::ContextRetrievalRequested { .. } => "context_retrieval_requested",
        InitiativeOutput::ExperimentProposed { .. } => "experiment_proposed",
        InitiativeOutput::OpenThreadSurfaced { .. } => "open_thread_surfaced",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AllowedEffect, EvidenceRef, VolitionEvent, apply, realtime_seed_fixture,
    };

    #[test]
    fn build_state_inspection_groups_goals_by_status() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let state = apply(state, VolitionEvent::GoalActivated {
            goal_id: "honor-explicit-user-request".to_string(),
            tick: 1,
        });

        let inspection = build_state_inspection(&state, &fixture);

        assert_eq!(inspection.tick, 1);
        assert!(inspection.active_goals.iter().any(|g| g.id == "honor-explicit-user-request"));
        assert!(inspection.accepted_goals.iter().all(|g| g.id != "honor-explicit-user-request"));
    }

    #[test]
    fn build_state_inspection_reflects_cooldown_status() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let evidence = EvidenceRef::try_new("test").unwrap();
        let state = apply(state, VolitionEvent::GoalActivated {
            goal_id: "honor-explicit-user-request".to_string(),
            tick: 1,
        });
        let state = apply(state, VolitionEvent::GoalSatisfied {
            goal_id: "honor-explicit-user-request".to_string(),
            evidence,
            tick: 2,
        });

        let inspection = build_state_inspection(&state, &fixture);

        assert!(inspection.cooldown_goals.iter().any(|g| g.id == "honor-explicit-user-request"));
        assert!(inspection.active_goals.iter().all(|g| g.id != "honor-explicit-user-request"));
    }

    #[test]
    fn build_state_inspection_with_no_initiative_output_returns_empty_summaries() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);

        let inspection = build_state_inspection(&state, &fixture);

        assert!(inspection.last_initiative_summaries.is_empty());
    }

    #[test]
    fn build_state_inspection_includes_initiative_summary_when_present() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let output = crate::InitiativeOutput::ReflectionRequested {
            proposed_question: "What should we focus on?".to_string(),
        };
        let state = apply(state, VolitionEvent::InitiativeExecuted {
            goal_id: "honor-explicit-user-request".to_string(),
            effect: AllowedEffect::Reflect,
            output,
            rationale: "test".to_string(),
            tick: 1,
        });

        let inspection = build_state_inspection(&state, &fixture);

        assert!(inspection.last_initiative_summaries.iter().any(|s| {
            s.goal_id == "honor-explicit-user-request" && s.output_kind == "reflection_requested"
        }));
    }

    #[test]
    fn build_state_inspection_reports_candidate_counts() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);

        let inspection = build_state_inspection(&state, &fixture);

        assert_eq!(inspection.pending_candidate_count, 0);
        assert_eq!(inspection.accepted_candidate_count, 0);
    }

    #[test]
    fn build_state_inspection_goal_summary_includes_title_from_fixture() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);

        let inspection = build_state_inspection(&state, &fixture);

        for summary in inspection.accepted_goals.iter().chain(inspection.active_goals.iter()) {
            assert!(
                !summary.title.is_empty(),
                "goal summary title must not be empty for goal {}",
                summary.id
            );
        }
    }
}
```

- [ ] **Step 2: Add the module to `lib.rs`**

In `crates/qsf_volition/src/lib.rs`, append:
```rust
mod inspection;
pub use inspection::*;
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test -p qsf_volition inspection 2>&1 | Select-Object -First 20
```
Expected: failures from `todo!()`.

- [ ] **Step 4: Implement `build_state_inspection`**

Replace the `todo!()` stub with:

```rust
pub fn build_state_inspection(
    state: &VolitionState,
    fixture: &VolitionFixture,
) -> VolitionStateInspection {
    let mut active_goals = Vec::new();
    let mut accepted_goals = Vec::new();
    let mut blocked_goals = Vec::new();
    let mut cooldown_goals = Vec::new();
    let mut retired_goals = Vec::new();
    let mut last_initiative_summaries = Vec::new();

    for (goal_id, dynamic) in &state.goals {
        let title = fixture
            .goals
            .iter()
            .find(|g| g.id == *goal_id)
            .map(|g| g.title.clone())
            .or_else(|| state.accepted_candidates.get(goal_id).map(|g| g.title.clone()))
            .unwrap_or_default();

        let summary = GoalStatusSummary {
            id: goal_id.clone(),
            title,
            salience: dynamic.salience,
            cooldown_until_tick: dynamic.cooldown_until_tick,
            last_activated_tick: dynamic.last_activated_tick,
        };

        match dynamic.status {
            GoalStatus::Active => active_goals.push(summary),
            GoalStatus::Accepted => accepted_goals.push(summary),
            GoalStatus::Blocked => blocked_goals.push(summary),
            GoalStatus::Cooldown => cooldown_goals.push(summary),
            GoalStatus::Retired => retired_goals.push(summary),
            GoalStatus::Proposed | GoalStatus::Satisfied => {}
        }

        if let Some(output) = &dynamic.last_initiative_output {
            let goal_title = fixture
                .goals
                .iter()
                .find(|g| g.id == *goal_id)
                .map(|g| g.title.clone())
                .or_else(|| state.accepted_candidates.get(goal_id).map(|g| g.title.clone()))
                .unwrap_or_default();
            last_initiative_summaries.push(InitiativeSummary {
                goal_id: goal_id.clone(),
                goal_title,
                output_kind: initiative_output_kind(output).to_string(),
            });
        }
    }

    VolitionStateInspection {
        mode: state.mode,
        tick: state.tick,
        active_goals,
        accepted_goals,
        blocked_goals,
        cooldown_goals,
        retired_goals,
        pending_candidate_count: state.pending_candidates.len(),
        accepted_candidate_count: state.accepted_candidates.len(),
        last_initiative_summaries,
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cargo test -p qsf_volition inspection 2>&1
```
Expected: all tests pass.

- [ ] **Step 6: Run full suite and clippy**

```bash
cargo test -p qsf_volition && cargo clippy -p qsf_volition --all-targets -- -D warnings
```
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/qsf_volition/src/inspection.rs crates/qsf_volition/src/lib.rs
git commit -m "feat(qsf_volition): add inspection module with build_state_inspection"
```

---

## Task 3: Refactor `qsf_app::volition` to use `select_goals_ranked`

**Files:**
- Modify: `crates/qsf_app/src/volition.rs`

**Interfaces:**
- Consumes: `qsf_volition::select_goals_ranked` (now re-exported from `qsf_volition::*`)
- Produces: unchanged public API (`select_goals`, `select_goals_with_salience`, etc.)

The goal is to remove the private duplicate helpers (`matched_keywords`, `compute_relevance`, `compute_relevance_with_salience`, `initiative_for_goal`, `initiative_for_effect`) from `qsf_app::volition` and rewrite the two selector functions to call `select_goals_ranked`. `build_fragment` and `GoalEvaluation` stay because they depend on `ContextFragment` from `qsf_context`.

- [ ] **Step 1: Confirm existing tests pass (baseline)**

```bash
cargo test -p qsf_app 2>&1 | Select-Object -Last 5
```
Expected: all pass.

- [ ] **Step 2: Refactor `select_goals_with_salience`**

In `crates/qsf_app/src/volition.rs`, replace the body of `select_goals_with_salience` with:

```rust
pub fn select_goals_with_salience(
    input: &str,
    fixture: &VolitionFixture,
    state: &VolitionState,
    budget: ContextBudget,
) -> SalienceGoalSelectionResult {
    let ranked = select_goals_ranked(input, state, fixture);

    let fragments: Vec<ContextFragment> = ranked
        .selected
        .iter()
        .map(|s| build_fragment(&s.goal, s.relevance_score, &s.matched_terms))
        .collect();
    let assembly = assemble_context(fragments, budget);

    let mut selected = Vec::new();
    for sel in &assembly.selected {
        if let Some(s) = ranked.selected.iter().find(|s| s.goal.id == sel.fragment.fragment_id) {
            selected.push(s.clone());
        }
    }

    let mut omitted = ranked.omitted;
    for omission in &assembly.omitted {
        if let Some(s) = ranked
            .selected
            .iter()
            .find(|s| s.goal.id == omission.fragment.fragment_id)
        {
            omitted.push(OmittedGoal {
                goal: s.goal.clone(),
                relevance_score: s.relevance_score,
                matched_terms: s.matched_terms.clone(),
                reason: omission.reason.clone(),
            });
        }
    }

    SalienceGoalSelectionResult {
        input: input.to_string(),
        input_terms: ranked.input_terms,
        budget,
        selected,
        omitted,
        suppressed_cooldown: ranked.suppressed_cooldown,
        visible_blocked: ranked.visible_blocked,
        assembly,
    }
}
```

- [ ] **Step 3: Refactor `select_goals`**

Replace the body of `select_goals` with:

```rust
pub fn select_goals(
    input: &str,
    fixture: &VolitionFixture,
    budget: ContextBudget,
) -> GoalSelectionResult {
    let synthetic_state = VolitionState::from_fixture(fixture);
    let ranked = select_goals_ranked(input, &synthetic_state, fixture);

    let fragments: Vec<ContextFragment> = ranked
        .selected
        .iter()
        .map(|s| build_fragment(&s.goal, s.relevance_score, &s.matched_terms))
        .collect();
    let assembly = assemble_context(fragments, budget);

    let mut selected = Vec::new();
    for sel in &assembly.selected {
        if let Some(s) = ranked.selected.iter().find(|s| s.goal.id == sel.fragment.fragment_id) {
            selected.push(s.clone());
        }
    }

    let mut omitted = ranked.omitted;
    for omission in &assembly.omitted {
        if let Some(s) = ranked
            .selected
            .iter()
            .find(|s| s.goal.id == omission.fragment.fragment_id)
        {
            omitted.push(OmittedGoal {
                goal: s.goal.clone(),
                relevance_score: s.relevance_score,
                matched_terms: s.matched_terms.clone(),
                reason: omission.reason.clone(),
            });
        }
    }

    GoalSelectionResult {
        input: input.to_string(),
        input_terms: ranked.input_terms,
        budget,
        selected,
        omitted,
        assembly,
    }
}
```

- [ ] **Step 4: Remove the now-duplicate private functions**

Delete these private functions from `qsf_app/src/volition.rs` (they are now in `qsf_volition::selection` and re-exported via `pub use qsf_volition::*`):
- `fn initiative_for_goal`
- `fn initiative_for_effect`
- `fn build_fragment` — **keep this**, it depends on `ContextFragment` from `qsf_context`
- `fn compute_relevance_with_salience`
- `fn compute_relevance`
- `fn matched_keywords`
- `struct GoalEvaluation` — **keep this**, used by `build_pre_initiative_traces`

Also remove the no-longer-needed `struct GoalEvaluation` from the old evaluation loop, if still present (it's now replaced by the `ranked` result).

> **Note:** `build_fragment` and `GoalEvaluation` stay because they serve `build_pre_initiative_traces` which needs `ContextFragment`. The removed functions are exactly those that now live in `qsf_volition::selection`.

- [ ] **Step 5: Run the full test suite**

```bash
cargo test -p qsf_app 2>&1
```
Expected: all existing tests still pass with no changes to test code.

- [ ] **Step 6: Run clippy**

```bash
cargo clippy -p qsf_app --all-targets -- -D warnings
```
Expected: no warnings.

- [ ] **Step 7: Commit**

```bash
git add crates/qsf_app/src/volition.rs
git commit -m "refactor(qsf_app): delegate goal selection to qsf_volition::select_goals_ranked"
```

---

## Task 4: Add `VolitionStateSnapshot` and extend `RealtimeToolContext`

**Files:**
- Modify: `crates/qsf_realtime_server/src/realtime/tools.rs`
- Modify: `crates/qsf_realtime_server/src/realtime/sideband.rs`
- Modify: `crates/qsf_realtime_server/src/realtime/mod.rs`

**Interfaces:**
- Consumes: `VolitionRuntimeState` from `SessionRuntime::volition`
- Produces:
  - `pub struct VolitionStateSnapshot { pub state: VolitionState, pub fixture: VolitionFixture }`
  - Extended `RealtimeToolContext` with fields `volition: Option<VolitionStateSnapshot>`, `exchange_index: usize`, `call_id: String`

- [ ] **Step 1: Add `VolitionStateSnapshot` to `tools.rs`**

In `crates/qsf_realtime_server/src/realtime/tools.rs`, add the following after the existing `use` imports:

```rust
use qsf_volition::{VolitionFixture, VolitionState};
```

Then add `VolitionStateSnapshot` before `RealtimeToolContext`:

```rust
#[derive(Clone)]
pub struct VolitionStateSnapshot {
    pub state: VolitionState,
    pub fixture: VolitionFixture,
}
```

- [ ] **Step 2: Extend `RealtimeToolContext`**

Replace the current `RealtimeToolContext` definition in `tools.rs`:

```rust
#[derive(Clone)]
pub struct RealtimeToolContext {
    pub state: AppState,
    pub qsf_session_id: String,
    pub snapshot: ToolSessionSnapshot,
    pub volition: Option<VolitionStateSnapshot>,
    pub exchange_index: usize,
    pub call_id: String,
}
```

- [ ] **Step 3: Fix test helper `tool_context` in `tools.rs`**

The existing test helper function in `tools.rs` that constructs `RealtimeToolContext` must be updated to supply the new fields:

```rust
fn tool_context(tempdir: &TempDir, runtime: &SessionRuntime) -> RealtimeToolContext {
    RealtimeToolContext {
        state: state(tempdir),
        qsf_session_id: runtime.qsf_session_id.clone(),
        snapshot: ToolSessionSnapshot::from_runtime(runtime),
        volition: None,
        exchange_index: 0,
        call_id: String::new(),
    }
}
```

- [ ] **Step 4: Update `RealtimeToolContext` construction in `sideband.rs`**

Locate the `tool_context` construction at approximately line 1219 in `sideband.rs`:

```rust
let tool_context = RealtimeToolContext {
    state: state.clone(),
    qsf_session_id: qsf_session_id.to_string(),
    snapshot,
};
```

Replace with (the volition clone happens before any async point, while the lock is held):

```rust
let volition_snapshot = VolitionStateSnapshot {
    state: guard.volition.state.clone(),
    fixture: guard.volition.fixture.clone(),
};
let tool_context = RealtimeToolContext {
    state: state.clone(),
    qsf_session_id: qsf_session_id.to_string(),
    snapshot,
    volition: Some(volition_snapshot),
    exchange_index,
    call_id: String::new(),
};
```

Also add the import in `sideband.rs`:
```rust
use crate::realtime::tools::{
    self, RealtimeToolContext, ToolSessionSnapshot, VolitionStateSnapshot,
    tool_allow_list, tool_permission_decision,
};
```

- [ ] **Step 5: Pass `call_id` per tool call in `sideband.rs`**

Find the loop in `sideband.rs` where `execute_realtime_tool_call` is called for each `pending` execution. Update it to set `call_id` per tool before passing context:

The pattern looks like:
```rust
for pending in pending_executions {
    // before this call, update call_id in context:
    let tool_context_for_call = RealtimeToolContext {
        call_id: pending.call_id.clone(),
        ..tool_context.clone()
    };
    // pass tool_context_for_call instead of &tool_context:
    execute_realtime_tool_call(&registry, &tool_context_for_call, exchange_index, pending, ...);
}
```

Search for where `pending_executions` is iterated and update that block. The `execute_realtime_tool_call` signature at line 1574 takes `tool_context: &RealtimeToolContext`, so pass `&tool_context_for_call`.

- [ ] **Step 6: Declare `volition_tools` module in `mod.rs`**

In `crates/qsf_realtime_server/src/realtime/mod.rs`, add:

```rust
pub(crate) mod volition_tools;
```

Create an empty placeholder file for now:

```rust
// crates/qsf_realtime_server/src/realtime/volition_tools.rs
// Implemented in Task 5.
```

- [ ] **Step 7: Build to verify compilation**

```bash
cargo build -p qsf_realtime_server 2>&1
```
Expected: compiles with no errors (placeholder `volition_tools.rs` is empty but valid).

- [ ] **Step 8: Run tests**

```bash
cargo test -p qsf_realtime_server 2>&1
```
Expected: existing tests pass.

- [ ] **Step 9: Commit**

```bash
git add crates/qsf_realtime_server/src/realtime/tools.rs \
        crates/qsf_realtime_server/src/realtime/sideband.rs \
        crates/qsf_realtime_server/src/realtime/mod.rs \
        crates/qsf_realtime_server/src/realtime/volition_tools.rs
git commit -m "feat(qsf_realtime_server): add VolitionStateSnapshot and extend RealtimeToolContext"
```

---

## Task 5: Implement `InspectVolitionStateTool` and `SelectVolitionGoalsTool`

**Files:**
- Modify: `crates/qsf_realtime_server/src/realtime/volition_tools.rs`

**Interfaces:**
- Consumes: `VolitionStateSnapshot`, `qsf_volition::{build_state_inspection, select_goals_ranked, arbitrate_with_mode}`
- Produces: Two `Tool` implementations with all tests from spec §5.2

The observation_summary for `select_volition_goals` is a compact JSON string that is the persisted trace. The output_text is the model-visible capped JSON.

- [ ] **Step 1: Write the failing tests first**

Replace the placeholder `volition_tools.rs` with the full test suite and stubs:

```rust
use anyhow::Context;
use qsf_session::{ToolCategory, ToolPermissionDecision};
use qsf_tools::{Tool, ToolContext, ToolDefinition, ToolMetadata, ToolRequest, ToolResult, ToolSideEffectLevel};
use qsf_volition::{
    GoalSelection, ModeArbitrationResult, arbitrate_with_mode, build_state_inspection,
    select_goals_ranked,
};
use serde::Serialize;
use sha2::Digest;

use crate::realtime::tools::{RealtimeToolContext, VolitionStateSnapshot};

pub const INSPECT_VOLITION_STATE_TOOL_NAME: &str = "inspect_volition_state";
pub const SELECT_VOLITION_GOALS_TOOL_NAME: &str = "select_volition_goals";

const SELECT_MAX_SELECTED: usize = 6;
const SELECT_MAX_OMITTED: usize = 8;

pub struct InspectVolitionStateTool;
pub struct SelectVolitionGoalsTool;

impl Tool for InspectVolitionStateTool {
    fn metadata(&self) -> ToolMetadata { todo!() }
    fn definition(&self) -> Option<ToolDefinition> { todo!() }
    fn execute(&self, _request: &ToolRequest, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> { todo!() }
}

impl Tool for SelectVolitionGoalsTool {
    fn metadata(&self) -> ToolMetadata { todo!() }
    fn definition(&self) -> Option<ToolDefinition> { todo!() }
    fn execute(&self, _request: &ToolRequest, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> { todo!() }
}

fn realtime_context(ctx: &dyn ToolContext) -> anyhow::Result<&RealtimeToolContext> {
    ctx.as_any()
        .downcast_ref::<RealtimeToolContext>()
        .context("realtime tool context missing")
}

fn volition_snapshot_hash(snap: &VolitionStateSnapshot) -> String {
    let payload = serde_json::json!({ "state": &snap.state, "fixture": &snap.fixture });
    let hash = sha2::Sha256::digest(payload.to_string().as_bytes());
    format!("{hash:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_tools::ToolPermission;
    use qsf_volition::{EvidenceRef, VolitionEvent, VolitionState, apply, realtime_seed_fixture};
    use serde_json::Value;
    use tempfile::TempDir;
    use crate::realtime::tools::{ToolSessionSnapshot, tool_permission_decision};
    use crate::state::{AppState, BrowserSessionConfig, SessionRuntime};
    use crate::diagnostics::DiagnosticWriter;

    fn state(tempdir: &TempDir) -> AppState {
        AppState::new_with_realtime_ws_base_url(
            "test-api-key",
            "http://127.0.0.1:9999",
            "wss://example.invalid/realtime",
            tempdir.path().to_path_buf(),
            crate::state::SessionIdMode::Default,
        )
        .expect("state")
    }

    fn runtime(tempdir: &TempDir) -> SessionRuntime {
        let diagnostics =
            DiagnosticWriter::create(tempdir.path().join("diagnostics.jsonl")).expect("diagnostics");
        SessionRuntime::new("test-session".to_string(), BrowserSessionConfig::default(), diagnostics)
    }

    fn tool_context_with_volition(tempdir: &TempDir, runtime: &SessionRuntime) -> RealtimeToolContext {
        let fixture = realtime_seed_fixture();
        let vol_state = VolitionState::from_fixture(&fixture);
        RealtimeToolContext {
            state: state(tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(runtime),
            volition: Some(VolitionStateSnapshot { state: vol_state, fixture }),
            exchange_index: 1,
            call_id: "call-abc".to_string(),
        }
    }

    fn tool_context_no_volition(tempdir: &TempDir, runtime: &SessionRuntime) -> RealtimeToolContext {
        RealtimeToolContext {
            state: state(tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(runtime),
            volition: None,
            exchange_index: 0,
            call_id: String::new(),
        }
    }

    fn inspect_request() -> ToolRequest {
        ToolRequest::new(INSPECT_VOLITION_STATE_TOOL_NAME, "{}", None, ToolPermission::read_only(), "tester")
    }

    fn select_request(query: &str) -> ToolRequest {
        let args = serde_json::json!({ "query": query });
        ToolRequest::new(
            SELECT_VOLITION_GOALS_TOOL_NAME,
            &args.to_string(),
            Some(args),
            ToolPermission::read_only(),
            "tester",
        )
    }

    // ── Permission checks ────────────────────────────────────────────────────

    #[test]
    fn permission_decision_allows_volition_tools_when_allow_listed() {
        use qsf_session::ToolCategory;
        use qsf_tools::ToolSideEffectLevel;
        let allow_list = vec![
            INSPECT_VOLITION_STATE_TOOL_NAME.to_string(),
            SELECT_VOLITION_GOALS_TOOL_NAME.to_string(),
        ];
        let read_only_meta = |name: &str| crate::realtime::tools::ToolMetadata {
            name: name.to_string(),
            description: "test".to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        };

        assert_eq!(
            tool_permission_decision(INSPECT_VOLITION_STATE_TOOL_NAME, &allow_list, Some(&read_only_meta(INSPECT_VOLITION_STATE_TOOL_NAME))),
            ToolPermissionDecision::Allowed
        );
        assert_eq!(
            tool_permission_decision(SELECT_VOLITION_GOALS_TOOL_NAME, &allow_list, Some(&read_only_meta(SELECT_VOLITION_GOALS_TOOL_NAME))),
            ToolPermissionDecision::Allowed
        );
    }

    // ── InspectVolitionStateTool ─────────────────────────────────────────────

    #[test]
    fn inspect_volition_returns_unavailable_when_volition_is_none() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_no_volition(&tempdir, &runtime);
        let tool = InspectVolitionStateTool;

        let result = tool.execute(&inspect_request(), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        assert_eq!(json["status"], "unavailable");
    }

    #[test]
    fn inspect_volition_output_contains_required_fields() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = InspectVolitionStateTool;

        let result = tool.execute(&inspect_request(), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        assert!(json.get("mode").is_some(), "output must contain mode");
        assert!(json.get("tick").is_some(), "output must contain tick");
        assert!(
            json.get("active_goals").is_some() || json.get("accepted_goals").is_some(),
            "output must contain at least one goal list key"
        );
    }

    #[test]
    fn inspect_volition_observation_summary_contains_key_fields() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = InspectVolitionStateTool;

        let result = tool.execute(&inspect_request(), &ctx).unwrap();

        let summary: Value = serde_json::from_str(&result.observation_summary)
            .expect("observation_summary must be valid JSON");
        assert_eq!(summary["tool_name"], INSPECT_VOLITION_STATE_TOOL_NAME, "must carry tool_name");
        assert!(summary.get("qsf_session_id").is_some(), "must carry qsf_session_id");
        assert!(summary.get("volition_tick").is_some(), "must carry volition_tick");
        assert!(summary.get("mode").is_some(), "must carry mode");
        assert!(summary.get("artifact_or_record_reference").is_some(), "must carry artifact_or_record_reference");
    }

    #[test]
    fn inspect_volition_output_does_not_contain_api_key() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = InspectVolitionStateTool;

        let result = tool.execute(&inspect_request(), &ctx).unwrap();

        assert!(!result.output_text.contains("OPENAI_API_KEY"));
        assert!(!result.observation_summary.contains("OPENAI_API_KEY"));
    }

    // ── SelectVolitionGoalsTool ──────────────────────────────────────────────

    #[test]
    fn select_volition_returns_unavailable_when_volition_is_none() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_no_volition(&tempdir, &runtime);
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("help me"), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        assert_eq!(json["status"], "unavailable");
    }

    #[test]
    fn select_volition_returns_no_match_when_query_has_no_keyword_match() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("xyzzy frobnicator quux"), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        assert_eq!(json["status"], "no_match");
        assert_eq!(json["arbitration"], Value::Null);
    }

    #[test]
    fn select_volition_output_is_deterministic_for_same_state_and_query() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let tool = SelectVolitionGoalsTool;

        let result1 = tool.execute(&select_request("how can you help me"), &tool_context_with_volition(&tempdir, &runtime)).unwrap();
        let result2 = tool.execute(&select_request("how can you help me"), &tool_context_with_volition(&tempdir, &runtime)).unwrap();

        assert_eq!(result1.output_text, result2.output_text);
    }

    #[test]
    fn select_volition_observation_summary_is_parseable_json_with_required_trace_fields() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("how can you help me"), &ctx).unwrap();
        let trace: Value = serde_json::from_str(&result.observation_summary).expect("observation_summary must be JSON");

        assert!(trace.get("qsf_session_id").is_some(), "trace must have qsf_session_id");
        assert!(trace.get("tool_name").is_some(), "trace must have tool_name");
        assert!(trace.get("volition_tick").is_some(), "trace must have volition_tick");
        assert!(trace.get("mode").is_some(), "trace must have mode");
        assert!(trace.get("input_query").is_some(), "trace must have input_query");
        assert!(trace.get("selected_goal_ids").is_some(), "trace must have selected_goal_ids");
        assert!(trace.get("omitted_goal_ids").is_some(), "trace must have omitted_goal_ids");
        assert!(trace.get("suppressed_cooldown_ids").is_some());
        assert!(trace.get("visible_blocked_ids").is_some());
        assert!(trace.get("selected_truncated").is_some());
        assert!(trace.get("omitted_truncated").is_some());
        assert!(trace.get("salience_snapshot").is_some());
        assert!(trace.get("arbitration_result").is_some());
        assert!(trace.get("volition_snapshot_hash").is_some());
        assert!(trace.get("artifact_or_record_reference").is_some());
    }

    #[test]
    fn select_volition_output_does_not_contain_api_key() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("how can you help"), &ctx).unwrap();

        assert!(!result.output_text.contains("OPENAI_API_KEY"));
        assert!(!result.observation_summary.contains("OPENAI_API_KEY"));
    }

    #[test]
    fn select_volition_caps_model_visible_output_at_6_selected_and_8_omitted() {
        // Build a fixture with 10 goals that all match "test" to force truncation.
        use qsf_volition::{AllowedEffect, Goal, GoalScope, GoalStatus, Tension, TensionPriority, VolitionFixture};

        let tension = Tension {
            id: "t1".to_string(),
            title: "T1".to_string(),
            summary: "test".to_string(),
            priority_bias: TensionPriority::Medium,
            arbitration_tier: 7,
        };
        let goals: Vec<Goal> = (0..20)
            .map(|i| Goal {
                id: format!("goal-{i:02}"),
                title: format!("Goal {i}"),
                summary: "test summary".to_string(),
                tension_ids: vec!["t1".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Session,
                base_priority: 70,
                activation_keywords: vec!["test".to_string()],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "done".to_string(),
                evidence_refs: vec![],
                estimated_tokens: 10,
                source_reference: "plan".to_string(),
            })
            .collect();
        let fixture = VolitionFixture { tensions: vec![tension], goals };
        let vol_state = VolitionState::from_fixture(&fixture);

        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = RealtimeToolContext {
            state: state(&tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot { state: vol_state, fixture }),
            exchange_index: 1,
            call_id: "call-cap".to_string(),
        };
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("test"), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        let selected_count = json["selected"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(selected_count <= SELECT_MAX_SELECTED, "selected must be capped at {SELECT_MAX_SELECTED}, got {selected_count}");
    }

    #[test]
    fn select_volition_trace_includes_full_list_when_truncated() {
        use qsf_volition::{AllowedEffect, Goal, GoalScope, GoalStatus, Tension, TensionPriority, VolitionFixture};

        let tension = Tension {
            id: "t1".to_string(), title: "T1".to_string(), summary: "test".to_string(),
            priority_bias: TensionPriority::Medium, arbitration_tier: 7,
        };
        let goals: Vec<Goal> = (0..10)
            .map(|i| Goal {
                id: format!("goal-{i:02}"), title: format!("Goal {i}"),
                summary: "test summary".to_string(), tension_ids: vec!["t1".to_string()],
                status: GoalStatus::Accepted, scope: GoalScope::Session, base_priority: 70,
                activation_keywords: vec!["test".to_string()],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "done".to_string(),
                evidence_refs: vec![], estimated_tokens: 10, source_reference: "plan".to_string(),
            })
            .collect();
        let fixture = VolitionFixture { tensions: vec![tension], goals };
        let vol_state = VolitionState::from_fixture(&fixture);

        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = RealtimeToolContext {
            state: state(&tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot { state: vol_state, fixture }),
            exchange_index: 1,
            call_id: "call-trunc".to_string(),
        };
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("test"), &ctx).unwrap();
        let trace: Value = serde_json::from_str(&result.observation_summary).unwrap();

        let trace_selected = trace["selected_goal_ids"].as_array().unwrap();
        assert_eq!(trace_selected.len(), 10, "trace must contain all 10 goal ids");
        assert_eq!(trace["selected_truncated"], Value::Bool(true));
    }

    #[test]
    fn select_volition_snapshot_hash_changes_when_fixture_changes() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let tool = SelectVolitionGoalsTool;

        let fixture1 = realtime_seed_fixture();
        let state1 = VolitionState::from_fixture(&fixture1);

        let mut fixture2 = fixture1.clone();
        fixture2.goals[0].title = "Modified Title".to_string();
        let state2 = VolitionState::from_fixture(&fixture2);

        let ctx1 = RealtimeToolContext {
            state: state(&tempdir), qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot { state: state1, fixture: fixture1 }),
            exchange_index: 1, call_id: "call-1".to_string(),
        };
        let ctx2 = RealtimeToolContext {
            state: state(&tempdir), qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot { state: state2, fixture: fixture2 }),
            exchange_index: 1, call_id: "call-2".to_string(),
        };

        let r1 = tool.execute(&select_request("how can you help"), &ctx1).unwrap();
        let r2 = tool.execute(&select_request("how can you help"), &ctx2).unwrap();

        let t1: Value = serde_json::from_str(&r1.observation_summary).unwrap();
        let t2: Value = serde_json::from_str(&r2.observation_summary).unwrap();

        assert_ne!(t1["volition_snapshot_hash"], t2["volition_snapshot_hash"],
            "hash must differ when fixture differs");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p qsf_realtime_server volition_tools 2>&1 | Select-Object -First 30
```
Expected: failures from `todo!()`.

- [ ] **Step 3: Implement `InspectVolitionStateTool`**

Replace the `Tool` impl for `InspectVolitionStateTool` with the real implementation:

```rust
impl Tool for InspectVolitionStateTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: INSPECT_VOLITION_STATE_TOOL_NAME.to_string(),
            description: "Inspect the current simulated volition state: mode, tick, goals by status, and last initiative summaries.".to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            INSPECT_VOLITION_STATE_TOOL_NAME,
            "Inspect the current simulated volition state: mode, tick, goals by status, and last initiative summaries.",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ))
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let ctx = realtime_context(ctx)?;

        let Some(snap) = &ctx.volition else {
            let output = serde_json::json!({ "status": "unavailable" });
            return Ok(ToolResult {
                tool_name: request.tool_name.clone(),
                category: ToolCategory::ReadOnly,
                side_effect_level: ToolSideEffectLevel::ReadOnly,
                input: request.input.clone(),
                output_text: output.to_string(),
                numeric_value: None,
                observation_summary: format!(
                    r#"{{"qsf_session_id":"{}","tool_name":"{}","status":"unavailable"}}"#,
                    ctx.qsf_session_id, INSPECT_VOLITION_STATE_TOOL_NAME
                ),
            });
        };

        let inspection = build_state_inspection(&snap.state, &snap.fixture);
        let output = serde_json::json!({
            "status": "ok",
            "mode": inspection.mode,
            "tick": inspection.tick,
            "active_goals": inspection.active_goals,
            "accepted_goals": inspection.accepted_goals,
            "blocked_goals": inspection.blocked_goals,
            "cooldown_goals": inspection.cooldown_goals,
            "retired_goals": inspection.retired_goals,
            "pending_candidate_count": inspection.pending_candidate_count,
            "accepted_candidate_count": inspection.accepted_candidate_count,
            "last_initiative_summaries": inspection.last_initiative_summaries,
            "note": "This reflects simulated internal state. It is not a claim of real subjective experience or desire."
        });

        let artifact_ref = format!("exchange:{}/tool_call:{}", ctx.exchange_index, ctx.call_id);
        let observation_summary = serde_json::json!({
            "qsf_session_id": &ctx.qsf_session_id,
            "tool_name": INSPECT_VOLITION_STATE_TOOL_NAME,
            "status": "ok",
            "volition_tick": inspection.tick,
            "mode": inspection.mode,
            "active_count": inspection.active_goals.len(),
            "accepted_count": inspection.accepted_goals.len(),
            "blocked_count": inspection.blocked_goals.len(),
            "cooldown_count": inspection.cooldown_goals.len(),
            "artifact_or_record_reference": artifact_ref,
        }).to_string();

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: output.to_string(),
            numeric_value: None,
            observation_summary,
        })
    }
}
```

- [ ] **Step 4: Implement `SelectVolitionGoalsTool`**

Replace the `Tool` impl for `SelectVolitionGoalsTool`:

```rust
impl Tool for SelectVolitionGoalsTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: SELECT_VOLITION_GOALS_TOOL_NAME.to_string(),
            description: "Given a query, return ranked active goals, omitted goals, and arbitration result without mutating state.".to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            SELECT_VOLITION_GOALS_TOOL_NAME,
            "Given a query, return ranked active goals, omitted goals, and arbitration result without mutating state.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ))
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let ctx = realtime_context(ctx)?;

        let Some(snap) = &ctx.volition else {
            let output = serde_json::json!({ "status": "unavailable" });
            let obs = serde_json::json!({
                "qsf_session_id": ctx.qsf_session_id,
                "tool_name": SELECT_VOLITION_GOALS_TOOL_NAME,
                "status": "unavailable"
            });
            return Ok(ToolResult {
                tool_name: request.tool_name.clone(),
                category: ToolCategory::ReadOnly,
                side_effect_level: ToolSideEffectLevel::ReadOnly,
                input: request.input.clone(),
                output_text: output.to_string(),
                numeric_value: None,
                observation_summary: obs.to_string(),
            });
        };

        let query = request
            .structured
            .as_ref()
            .and_then(|v| v.get("query"))
            .and_then(|v| v.as_str())
            .context("select_volition_goals requires `query`")?;

        let ranked = select_goals_ranked(query, &snap.state, &snap.fixture);
        let arbitration = arbitrate_with_mode(ranked.selected.clone(), &snap.fixture, snap.state.mode);
        let snapshot_hash = volition_snapshot_hash(snap);

        if ranked.selected.is_empty() {
            let obs = build_select_observation_summary(
                &ctx.qsf_session_id, query, &snap, &ranked, &arbitration,
                &snapshot_hash, &ctx.exchange_index, &ctx.call_id, false, false,
            );
            let output = serde_json::json!({
                "status": "no_match",
                "query_terms": ranked.input_terms,
                "mode": snap.state.mode,
                "tick": snap.state.tick,
                "selected": [],
                "omitted": ranked.omitted.iter().take(SELECT_MAX_OMITTED).map(|g| serde_json::json!({
                    "id": &g.goal.id,
                    "title": &g.goal.title,
                    "reason": &g.reason,
                })).collect::<Vec<_>>(),
                "suppressed_cooldown_count": ranked.suppressed_cooldown.len(),
                "arbitration": null,
                "volition_snapshot_hash": snapshot_hash,
                "note": "This reflects simulated internal state. It is not a claim of real subjective experience or desire."
            });
            return Ok(ToolResult {
                tool_name: request.tool_name.clone(),
                category: ToolCategory::ReadOnly,
                side_effect_level: ToolSideEffectLevel::ReadOnly,
                input: request.input.clone(),
                output_text: output.to_string(),
                numeric_value: None,
                observation_summary: obs,
            });
        }

        let selected_truncated = ranked.selected.len() > SELECT_MAX_SELECTED;
        let omitted_truncated = ranked.omitted.len() > SELECT_MAX_OMITTED;

        let model_selected: Vec<serde_json::Value> = ranked
            .selected
            .iter()
            .take(SELECT_MAX_SELECTED)
            .map(|s| serde_json::json!({
                "id": &s.goal.id,
                "title": &s.goal.title,
                "summary": &s.goal.summary,
                "status": format!("{:?}", snap.state.goals.get(&s.goal.id).map(|d| d.status).unwrap_or(s.goal.status)).to_lowercase(),
                "salience": snap.state.goals.get(&s.goal.id).map(|d| d.salience).unwrap_or(0),
                "relevance_score": s.relevance_score,
                "matched_terms": s.matched_terms.clone(),
                "scope": s.goal.scope,
                "tension_ids": s.goal.tension_ids.clone(),
            }))
            .collect();

        let model_omitted: Vec<serde_json::Value> = ranked
            .omitted
            .iter()
            .take(SELECT_MAX_OMITTED)
            .map(|g| serde_json::json!({
                "id": &g.goal.id,
                "title": &g.goal.title,
                "reason": &g.reason,
            }))
            .collect();

        let arbitration_json = arbitration.as_ref().map(|arb| serde_json::json!({
            "winner_id": &arb.winner.goal.id,
            "winner_title": &arb.winner.goal.title,
            "winner_effective_tier": arb.winner_bias.effective_tier,
            "winner_effective_tension_id": &arb.winner_effective_tension_id,
            "winner_effective_tension_title": &arb.winner_effective_tension_title,
            "loser_count": arb.losers.len(),
        }));

        let obs = build_select_observation_summary(
            &ctx.qsf_session_id, query, snap, &ranked, &arbitration,
            &snapshot_hash, &ctx.exchange_index, &ctx.call_id,
            selected_truncated, omitted_truncated,
        );

        let output = serde_json::json!({
            "status": "ok",
            "query_terms": ranked.input_terms,
            "mode": snap.state.mode,
            "tick": snap.state.tick,
            "selected": model_selected,
            "omitted": model_omitted,
            "suppressed_cooldown_count": ranked.suppressed_cooldown.len(),
            "arbitration": arbitration_json,
            "volition_snapshot_hash": snapshot_hash,
            "note": "This reflects simulated internal state. It is not a claim of real subjective experience or desire."
        });

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: output.to_string(),
            numeric_value: None,
            observation_summary: obs,
        })
    }
}

fn build_select_observation_summary(
    session_id: &str,
    query: &str,
    snap: &VolitionStateSnapshot,
    ranked: &qsf_volition::RankedSelectionResult,
    arbitration: &Option<qsf_volition::ModeArbitrationResult>,
    snapshot_hash: &str,
    exchange_index: &usize,
    call_id: &str,
    selected_truncated: bool,
    omitted_truncated: bool,
) -> String {
    let salience_snapshot: std::collections::BTreeMap<String, i32> = ranked
        .selected
        .iter()
        .map(|s| {
            let salience = snap.state.goals.get(&s.goal.id).map(|d| d.salience).unwrap_or(0);
            (s.goal.id.clone(), salience)
        })
        .collect();

    let arbitration_result = arbitration.as_ref().map(|arb| serde_json::json!({
        "winner_id": &arb.winner.goal.id,
        "winner_effective_tier": arb.winner_bias.effective_tier,
    }));

    let artifact_ref = format!("exchange:{exchange_index}/tool_call:{call_id}");

    let trace = serde_json::json!({
        "qsf_session_id": session_id,
        "tool_name": SELECT_VOLITION_GOALS_TOOL_NAME,
        "volition_tick": snap.state.tick,
        "mode": snap.state.mode,
        "input_query": query,
        "selected_goal_ids": ranked.selected.iter().map(|s| &s.goal.id).collect::<Vec<_>>(),
        "omitted_goal_ids": ranked.omitted.iter().map(|g| &g.goal.id).collect::<Vec<_>>(),
        "suppressed_cooldown_ids": ranked.suppressed_cooldown.iter().map(|g| &g.goal.id).collect::<Vec<_>>(),
        "visible_blocked_ids": ranked.visible_blocked.iter().map(|g| &g.goal.id).collect::<Vec<_>>(),
        "selected_truncated": selected_truncated,
        "omitted_truncated": omitted_truncated,
        "salience_snapshot": salience_snapshot,
        "arbitration_result": arbitration_result,
        "volition_snapshot_hash": snapshot_hash,
        "artifact_or_record_reference": artifact_ref,
    });

    trace.to_string()
}
```

- [ ] **Step 5: Compile-check before running behavioral tests**

```powershell
cargo check -p qsf_realtime_server --tests
```
Expected: no compile errors.

- [ ] **Step 6: Run tests**

```bash
cargo test -p qsf_realtime_server volition_tools 2>&1
```
Expected: all tests pass.

- [ ] **Step 7: Run full suite and clippy**

```bash
cargo test -p qsf_realtime_server && cargo clippy -p qsf_realtime_server --all-targets -- -D warnings
```
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add crates/qsf_realtime_server/src/realtime/volition_tools.rs
git commit -m "feat(qsf_realtime_server): implement InspectVolitionStateTool and SelectVolitionGoalsTool"
```

---

## Task 6: Wire tools into defaults and registry in `tools.rs`

**Files:**
- Modify: `crates/qsf_realtime_server/src/realtime/tools.rs`

This wires the two new volition tools into the existing tool infrastructure: constants, `default_tool_definitions()`, and `built_in_tools()`.

- [ ] **Step 1: Add the tool name constants and update `tools.rs`**

In `crates/qsf_realtime_server/src/realtime/tools.rs`, add the import at the top:

```rust
use crate::realtime::volition_tools::{
    INSPECT_VOLITION_STATE_TOOL_NAME, SELECT_VOLITION_GOALS_TOOL_NAME,
    InspectVolitionStateTool, SelectVolitionGoalsTool,
};
```

Then update `default_tool_definitions()` to include the two new tools:

```rust
pub fn default_tool_definitions() -> Vec<RealtimeToolDefinition> {
    vec![
        RealtimeToolDefinition::function(
            SEARCH_MEMORY_TOOL_NAME,
            "Search the session memory store for relevant memories.",
            serde_json::json!({ "type": "object", "properties": { "query": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 4 } }, "required": ["query"], "additionalProperties": false }),
        ),
        RealtimeToolDefinition::function(
            GET_ASSOCIATIONS_TOOL_NAME,
            "Inspect the weighted association neighborhood for a memory id.",
            serde_json::json!({ "type": "object", "properties": { "memory_id": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 8 } }, "required": ["memory_id"], "additionalProperties": false }),
        ),
        RealtimeToolDefinition::function(
            INSPECT_SESSION_STATE_TOOL_NAME,
            "Summarize the live session state without exposing internals.",
            serde_json::json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        RealtimeToolDefinition::function(
            INSPECT_VOLITION_STATE_TOOL_NAME,
            "Inspect the current simulated volition state: mode, tick, goals by status, and last initiative summaries.",
            serde_json::json!({ "type": "object", "properties": {}, "additionalProperties": false }),
        ),
        RealtimeToolDefinition::function(
            SELECT_VOLITION_GOALS_TOOL_NAME,
            "Given a query, return ranked active goals, omitted goals, and arbitration result without mutating state.",
            serde_json::json!({ "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"], "additionalProperties": false }),
        ),
    ]
}
```

Update `built_in_tools()`:

```rust
fn built_in_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(SearchMemoryTool),
        Box::new(GetAssociationsTool),
        Box::new(InspectSessionStateTool),
        Box::new(InspectVolitionStateTool),
        Box::new(SelectVolitionGoalsTool),
    ]
}
```

- [ ] **Step 2: Add a test that verifies the allow list includes both volition tools**

In the `tests` module within `tools.rs`, add:

```rust
#[test]
fn default_tool_definitions_includes_volition_tools() {
    let defs = default_tool_definitions();
    let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
    assert!(names.contains(&INSPECT_VOLITION_STATE_TOOL_NAME));
    assert!(names.contains(&SELECT_VOLITION_GOALS_TOOL_NAME));
}

#[test]
fn permission_decision_allows_volition_tools() {
    use qsf_tools::ToolSideEffectLevel;
    let allow_list = vec![
        INSPECT_VOLITION_STATE_TOOL_NAME.to_string(),
        SELECT_VOLITION_GOALS_TOOL_NAME.to_string(),
    ];
    let ro = |name: &str| metadata(name, ToolCategory::ReadOnly, ToolSideEffectLevel::ReadOnly);

    assert_eq!(
        tool_permission_decision(INSPECT_VOLITION_STATE_TOOL_NAME, &allow_list, Some(&ro(INSPECT_VOLITION_STATE_TOOL_NAME))),
        ToolPermissionDecision::Allowed
    );
    assert_eq!(
        tool_permission_decision(SELECT_VOLITION_GOALS_TOOL_NAME, &allow_list, Some(&ro(SELECT_VOLITION_GOALS_TOOL_NAME))),
        ToolPermissionDecision::Allowed
    );
}
```

Note: `INSPECT_VOLITION_STATE_TOOL_NAME` and `SELECT_VOLITION_GOALS_TOOL_NAME` need to be imported in the test module. Add `use super::{INSPECT_VOLITION_STATE_TOOL_NAME, SELECT_VOLITION_GOALS_TOOL_NAME};` inside the `tests` module.

- [ ] **Step 3: Run all tests and clippy**

```bash
cargo test -p qsf_realtime_server && cargo clippy -p qsf_realtime_server --all-targets -- -D warnings && cargo fmt -p qsf_realtime_server
```
Expected: all pass, no warnings, code formatted.

- [ ] **Step 4: Run full workspace build and tests**

```bash
cargo test --workspace && cargo clippy --all-targets -- -D warnings
```
Expected: all crates pass.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/src/realtime/tools.rs
git commit -m "feat(qsf_realtime_server): wire volition tools into default definitions and registry"
```

---

## Task 7: Create experiment scaffold

**Files:**
- Create: `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md`

- [ ] **Step 1: Create the experiment scaffold**

Create `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md` with the content below. Check `docs/Experiments/` for an existing example experiment to match the format.

The file should contain:
- **Purpose:** validate that `inspect_volition_state` and `select_volition_goals` are accessible, correct, and traceable in a live realtime session
- **Trace completeness contract:** all fields from the spec §3.4 (the `volition_tool_trace` fields from `Plan.RealtimeVolitionIntegration.md`)
- **Automated verification:** parse `ToolExecutionRecord.result_summary` for `select_volition_goals` and assert the required trace fields are present; assert the output is not just a copy of any context injection packet
- **Human test steps:** ask "what are you currently focused on?" and "what goals relate to helping me?"; confirm the model calls a volition tool and gives a grounded answer that distinguishes simulated internal state from claims of real desire

- [ ] **Step 2: Update `Experiment.Backlog.md`**

Open `docs/Experiments/Experiment.Backlog.md` and promote `RealtimeVolitionReadOnlyInspection` from "planned" to "running" (or "complete" if implementation is done).

- [ ] **Step 3: Commit**

```bash
git add docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md docs/Experiments/Experiment.Backlog.md
git commit -m "docs: add RealtimeVolitionReadOnlyInspection experiment scaffold"
```

---

## Task 8: Update documentation

**Files:**
- Modify: `docs/Plans/Plan.RealtimeVolitionIntegration.md`
- Modify: `docs/Architecture/Architecture.RealtimeSessionServer.md`
- Modify: `docs/Architecture/Architecture.ToolSystem.md`
- Modify: `docs/Architecture/Architecture.VolitionSystem.md`

- [ ] **Step 1: Mark Phase 3 as implemented (pending human validation)**

In `docs/Plans/Plan.RealtimeVolitionIntegration.md`, update the status table Phase 3 row from "Not started" to "Implemented — human validation pending". Do **not** mark it "Complete" yet; that happens in Step 1b after the live test passes.

- [ ] **Step 1b: Run live realtime human test and record results**

Execute the human test steps from `docs/Experiments/Experiment.RealtimeVolitionReadOnlyInspection.md`:
- Start a live realtime session.
- Ask "what are you currently focused on?" and "what goals relate to helping me?"
- Confirm the model calls a volition tool and gives a grounded answer.
- Record the session transcript and outcome in `Experiment.RealtimeVolitionReadOnlyInspection.md`.

Only after results are recorded: update `docs/Plans/Plan.RealtimeVolitionIntegration.md` Phase 3 row to "Complete" and update the status header to "Phases 1, 2, and 3 are complete."

- [ ] **Step 2: Update `Architecture.RealtimeSessionServer.md`**

Add a section describing:
- `VolitionStateSnapshot` is cloned from `SessionRuntime.volition` when the tool context is built in the sideband dispatch path
- `RealtimeToolContext` now carries `volition: Option<VolitionStateSnapshot>`, `exchange_index: usize`, and `call_id: String`
- Two new read-only volition tools (`inspect_volition_state`, `select_volition_goals`) are registered in the default tool list

- [ ] **Step 3: Update `Architecture.ToolSystem.md`**

Add entries for `inspect_volition_state` and `select_volition_goals` under the realtime read-only tools section, noting they use `VolitionStateSnapshot` from the tool context and never mutate state.

- [ ] **Step 4: Update `Architecture.VolitionSystem.md`**

Add a note that context-neutral goal selection helpers (`matched_keywords`, `compute_relevance`, `compute_relevance_with_salience`, `initiative_for_goal`, `initiative_for_effect`) and `select_goals_ranked` now live in `qsf_volition::selection`, making them available to both `qsf_app` and `qsf_realtime_server`.

- [ ] **Step 4b: Update `Architecture.StateAndObservability.md`**

Add a section noting that volition tool invocations are persisted as `ToolExecutionRecord.result_summary` JSON blobs. Document the trace field shape (at minimum: `qsf_session_id`, `tool_name`, `status`, `volition_tick`, `mode`, goal counts or selected/omitted IDs, `artifact_or_record_reference`) and clarify the artifact reference format (`exchange:<index>/tool_call:<id>`).

- [ ] **Step 5: Run final full workspace check**

```bash
cargo test --workspace && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```
Expected: all pass.

- [ ] **Step 6: Commit documentation updates**

```powershell
git add docs/Plans/Plan.RealtimeVolitionIntegration.md docs/Architecture/Architecture.RealtimeSessionServer.md docs/Architecture/Architecture.ToolSystem.md docs/Architecture/Architecture.VolitionSystem.md docs/Architecture/Architecture.StateAndObservability.md
git commit -m "docs: update architecture and plan status for realtime volition read-only tools"
```

---

## Self-Review Against Spec

**Spec §1.1 — `selection.rs`:**
- ✅ `matched_keywords`, `compute_relevance`, `compute_relevance_with_salience`, `initiative_for_goal`, `initiative_for_effect` moved as public functions (Task 1)
- ✅ `RankedSelectionResult` struct defined with all required fields (Task 1)
- ✅ `select_goals_ranked` function implemented with all status handling (Task 1)
- ✅ Cooldown → `suppressed_cooldown`, Proposed/Retired → `omitted`, Blocked → `visible_blocked`, Accepted/Active + keyword → `selected` (Task 1)
- ✅ Accepted candidates in `state.accepted_candidates` follow same path (Task 1)
- ✅ No `ContextBudget`/`ContextFragment`/`ContextAssembly` in `select_goals_ranked` (Task 1)

**Spec §1.2 — `inspection.rs`:**
- ✅ `VolitionStateInspection`, `GoalStatusSummary`, `InitiativeSummary` types (Task 2)
- ✅ `build_state_inspection` groups by status, looks up titles from fixture and accepted_candidates (Task 2)

**Spec §1.3 — `lib.rs` re-exports:**
- ✅ `pub use selection::*` and `pub use inspection::*` (Tasks 1, 2)

**Spec §2.1 — `select_goals_with_salience` refactoring:**
- ✅ Calls `select_goals_ranked`, then assembles via `build_fragment` + `assemble_context` (Task 3)
- ✅ `build_fragment`, `build_pre_initiative_traces`, `GoalSelectionResult`/`SalienceGoalSelectionResult` remain in `qsf_app::volition` (Task 3)

**Spec §2.2 — `select_goals` refactoring:**
- ✅ Calls `select_goals_ranked` with `VolitionState::from_fixture(fixture)` (Task 3)

**Spec §3.1 — `VolitionStateSnapshot` and updated `RealtimeToolContext`:**
- ✅ `VolitionStateSnapshot { state, fixture }` added to `tools.rs` (Task 4)
- ✅ `RealtimeToolContext` extended with `volition`, `exchange_index`, `call_id` (Task 4)
- ✅ Snapshot cloned before async await points (Task 4)

**Spec §3.2 — `volition_tools.rs`:**
- ✅ Both tool structs, `Tool` impls, builder helpers in dedicated file (Task 5)

**Spec §3.3 — `select_volition_goals` output JSON:**
- ✅ All required fields in model output (status, query_terms, mode, tick, selected, omitted, suppressed_cooldown_count, arbitration, volition_snapshot_hash, note) (Task 5)
- ✅ `status: "no_match"` with `arbitration: null` when no goals match (Task 5)

**Spec §3.4 — Trace observation_summary:**
- ✅ All required trace fields present (qsf_session_id, tool_name, volition_tick, mode, input_query, selected_goal_ids, omitted_goal_ids, suppressed_cooldown_ids, visible_blocked_ids, selected_truncated, omitted_truncated, salience_snapshot, arbitration_result, volition_snapshot_hash, artifact_or_record_reference) (Task 5)
- ✅ Full goal id sets in trace (not capped) even when model output is capped (Task 5)

**Spec §3.5 — Updated `tools.rs`:**
- ✅ Tool name constants, both tools in `default_tool_definitions()` and `built_in_tools()` (Task 6)

**Spec §3.6 — `realtime/mod.rs`:**
- ✅ `mod volition_tools` declared (Task 4)

**Spec §4 — Security constraints:**
- ✅ Tests assert `"OPENAI_API_KEY"` not in output or observation_summary (Task 5)
- ✅ No `AppState` secrets accessed in tool path (Task 5)

**Spec §5 — Tests:**
- ✅ All tests from §5.1 covered in Tasks 1, 2
- ✅ All tests from §5.2 covered in Task 5
- ✅ §5.3 regression tests verified in Task 3

**Spec §6 — Experiment scaffold:**
- ✅ `Experiment.RealtimeVolitionReadOnlyInspection.md` created (Task 7)

**Spec §7 — Documents to update after implementation:**
- ✅ Plan status updated (Task 8)
- ✅ Experiment created and backlog updated (Task 7)
- ✅ Architecture docs updated (Task 8)
