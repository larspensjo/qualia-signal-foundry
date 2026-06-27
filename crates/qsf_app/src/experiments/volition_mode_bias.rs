use std::fs;

use anyhow::Context;
use serde_json::json;

use crate::context::ContextBudget;
use crate::observability::event_log::EventType;
use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::volition::{
    Mode, ModeArbitrationResult, SalienceGoalSelectionResult, VolitionEvent, VolitionState, apply,
    arbitrate_with_mode, select_goals_with_salience, static_fixture,
};

use super::registry::{Experiment, ExperimentName, ExperimentOutcome};

// Budget sized to allow all 3 goals in the floor-immunity turn.
const BUDGET: ContextBudget = ContextBudget {
    max_fragments: 4,
    max_estimated_tokens: 100,
};

pub struct VolitionModeBiasExperiment;

impl Experiment for VolitionModeBiasExperiment {
    fn name(&self) -> ExperimentName {
        ExperimentName::VolitionModeBias
    }

    fn description(&self) -> &'static str {
        "Replay 4 scripted turns exercising mode-aware arbitration: neutral baseline, \
         Exploratory flips winner, floor immunity, Focused suppresses tangent — \
         executed_effects=0 on every turn"
    }

    fn run(&self, context: &mut RunContext) -> anyhow::Result<ExperimentOutcome> {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);

        // Scripted inputs per the experiment spec.
        // Band-only (turns 1, 2, 4): continuity + curiosity goals selected, no tier-1 goal.
        let band_input = "The open thread about voice memory evidence is unresolved.";
        // Floor input (turn 3): additionally matches the tier-1 goal.
        let floor_input =
            "Is the voice memory work complete, or is the evidence thread still unresolved?";

        // ── Turn 1: Neutral baseline ──────────────────────────────────────────
        // Apply ModeChanged { Neutral }; band-only conflict; continuity wins.
        state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Neutral,
                tick: 1,
            },
        );
        assert_eq!(state.mode, Mode::Neutral);

        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 1, "phase": "neutral_baseline", "input": band_input,
                    "events_applied": [{"kind": "mode_changed", "mode": "neutral", "tick": 1}],
                    "active_mode": "neutral" }),
            None,
        )?;

        let sel1 = select_goals_with_salience(band_input, &fixture, &state, BUDGET);
        assert_eq!(
            sel1.selected.len(),
            2,
            "turn 1 must select exactly 2 band goals"
        );

        let arb1 = arbitrate_with_mode(sel1.selected.clone(), &fixture, Mode::Neutral)
            .expect("turn 1 must produce an arbitration result");
        let neutral1_winner_id = arb1.winner.goal.id.clone();

        // Continuity (tier 5) beats curiosity (tier 7) under Neutral.
        assert_eq!(neutral1_winner_id, "resurface-open-thread");

        let turn1_id = record_conflict_turn(
            context,
            1,
            "neutral_baseline",
            &state,
            band_input,
            Mode::Neutral,
            &sel1,
            &arb1,
            &neutral1_winner_id,
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 1, "phase": "neutral_baseline", "trace_id": turn1_id,
                "active_mode": "neutral", "winner_goal_id": neutral1_winner_id,
                "mode_changed_winner": false, "executed_effects": 0,
            }),
            Some(turn1_id),
        )?;

        // ── Turn 2: Exploratory flips winner ──────────────────────────────────
        // Apply ModeChanged { Exploratory }; same input; winner flips to curiosity.
        state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Exploratory,
                tick: 2,
            },
        );
        assert_eq!(state.mode, Mode::Exploratory);

        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 2, "phase": "exploratory_flip", "input": band_input,
                    "events_applied": [{"kind": "mode_changed", "mode": "exploratory", "tick": 2}],
                    "active_mode": "exploratory" }),
            None,
        )?;

        let sel2 = select_goals_with_salience(band_input, &fixture, &state, BUDGET);
        assert_eq!(
            sel2.selected.len(),
            2,
            "turn 2 must select exactly 2 band goals"
        );

        let arb2 = arbitrate_with_mode(sel2.selected.clone(), &fixture, Mode::Exploratory)
            .expect("turn 2 must produce an arbitration result");
        let neutral2_winner_id =
            arbitrate_with_mode(sel2.selected.clone(), &fixture, Mode::Neutral)
                .map(|r| r.winner.goal.id)
                .unwrap_or_default();

        // Exploratory: curiosity biased 7→5, continuity biased 5→6 → curiosity wins.
        assert_eq!(arb2.winner.goal.id, "clarify-weak-evidence-topic");
        assert_ne!(
            arb2.winner.goal.id, neutral2_winner_id,
            "turn 2: mode_changed_winner must be true"
        );

        let mode_changed_winner2 = arb2.winner.goal.id != neutral2_winner_id;
        assert!(mode_changed_winner2);

        let turn2_id = record_conflict_turn(
            context,
            2,
            "exploratory_flip",
            &state,
            band_input,
            Mode::Exploratory,
            &sel2,
            &arb2,
            &neutral2_winner_id,
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 2, "phase": "exploratory_flip", "trace_id": turn2_id,
                "active_mode": "exploratory", "winner_goal_id": arb2.winner.goal.id,
                "neutral_winner_goal_id": neutral2_winner_id,
                "mode_changed_winner": mode_changed_winner2, "executed_effects": 0,
            }),
            Some(turn2_id),
        )?;

        // ── Turn 3: Floor immunity ────────────────────────────────────────────
        // Apply ModeChanged { Exploratory }; floor input activates tier-1 goal;
        // tier-1 goal wins regardless of mode.
        state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Exploratory,
                tick: 3,
            },
        );
        assert_eq!(state.mode, Mode::Exploratory);

        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 3, "phase": "floor_immunity", "input": floor_input,
                    "events_applied": [{"kind": "mode_changed", "mode": "exploratory", "tick": 3}],
                    "active_mode": "exploratory" }),
            None,
        )?;

        let sel3 = select_goals_with_salience(floor_input, &fixture, &state, BUDGET);
        assert!(
            sel3.selected
                .iter()
                .any(|s| s.goal.id == "avoid-overstating-impl-status"),
            "turn 3 must select the tier-1 floor goal"
        );

        let arb3 = arbitrate_with_mode(sel3.selected.clone(), &fixture, Mode::Exploratory)
            .expect("turn 3 must produce an arbitration result");
        let neutral3_winner_id =
            arbitrate_with_mode(sel3.selected.clone(), &fixture, Mode::Neutral)
                .map(|r| r.winner.goal.id)
                .unwrap_or_default();

        // Floor goal wins under Exploratory (protected tier 1 can't be displaced).
        assert_eq!(arb3.winner.goal.id, "avoid-overstating-impl-status");
        assert!(
            arb3.winner_bias.protected,
            "turn 3: floor winner must be marked protected"
        );
        let mode_changed_winner3 = arb3.winner.goal.id != neutral3_winner_id;
        assert!(
            !mode_changed_winner3,
            "turn 3: mode_changed_winner must be false"
        );

        // All band losers must have biased_tier >= PROTECTED_TIER_FLOOR + 1.
        for loser in &arb3.losers {
            if !loser.bias.protected {
                assert!(
                    loser.bias.biased_tier > crate::volition::PROTECTED_TIER_FLOOR,
                    "loser {} biased_tier must not enter the protected floor",
                    loser.selection.goal.id
                );
            }
        }

        let turn3_id = record_conflict_turn(
            context,
            3,
            "floor_immunity",
            &state,
            floor_input,
            Mode::Exploratory,
            &sel3,
            &arb3,
            &neutral3_winner_id,
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 3, "phase": "floor_immunity", "trace_id": turn3_id,
                "active_mode": "exploratory", "winner_goal_id": arb3.winner.goal.id,
                "neutral_winner_goal_id": neutral3_winner_id,
                "mode_changed_winner": mode_changed_winner3, "executed_effects": 0,
            }),
            Some(turn3_id),
        )?;

        // ── Turn 4: Focused suppresses tangent ────────────────────────────────
        // Apply ModeChanged { Focused }; band-only input; continuity still wins,
        // curiosity is pushed further down.
        state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Focused,
                tick: 4,
            },
        );
        assert_eq!(state.mode, Mode::Focused);

        context.record_event(
            EventType::InputReceived,
            json!({ "turn": 4, "phase": "focused_suppress", "input": band_input,
                    "events_applied": [{"kind": "mode_changed", "mode": "focused", "tick": 4}],
                    "active_mode": "focused" }),
            None,
        )?;

        let sel4 = select_goals_with_salience(band_input, &fixture, &state, BUDGET);
        assert_eq!(
            sel4.selected.len(),
            2,
            "turn 4 must select exactly 2 band goals"
        );

        let arb4 = arbitrate_with_mode(sel4.selected.clone(), &fixture, Mode::Focused)
            .expect("turn 4 must produce an arbitration result");
        let neutral4_winner_id =
            arbitrate_with_mode(sel4.selected.clone(), &fixture, Mode::Neutral)
                .map(|r| r.winner.goal.id)
                .unwrap_or_default();

        // Focused: continuity biased 5→4, curiosity biased 7→10 → continuity wins.
        assert_eq!(arb4.winner.goal.id, "resurface-open-thread");

        // Curiosity must be demoted.
        let curiosity_loser4 = arb4
            .losers
            .iter()
            .find(|l| l.selection.goal.id == "clarify-weak-evidence-topic")
            .expect("curiosity must be a loser under Focused");
        assert!(
            curiosity_loser4.bias.bias_applied > 0,
            "curiosity bias_applied must be positive (demotion) under Focused"
        );
        assert!(
            curiosity_loser4.bias.biased_tier > curiosity_loser4.bias.effective_tier,
            "curiosity biased_tier must exceed effective_tier under Focused"
        );

        let mode_changed_winner4 = arb4.winner.goal.id != neutral4_winner_id;
        // Both Neutral and Focused pick the same winner (continuity), so mode_changed_winner = false.
        assert!(
            !mode_changed_winner4,
            "turn 4: mode_changed_winner must be false"
        );

        let turn4_id = record_conflict_turn(
            context,
            4,
            "focused_suppress",
            &state,
            band_input,
            Mode::Focused,
            &sel4,
            &arb4,
            &neutral4_winner_id,
        )?;
        context.record_event(
            EventType::TraceRecorded,
            json!({
                "turn": 4, "phase": "focused_suppress", "trace_id": turn4_id,
                "active_mode": "focused", "winner_goal_id": arb4.winner.goal.id,
                "neutral_winner_goal_id": neutral4_winner_id,
                "mode_changed_winner": mode_changed_winner4, "executed_effects": 0,
            }),
            Some(turn4_id),
        )?;

        write_mode_bias_report(
            context,
            &fixture,
            &[
                (&arb1, "neutral", &neutral1_winner_id, false),
                (
                    &arb2,
                    "exploratory",
                    &neutral2_winner_id,
                    mode_changed_winner2,
                ),
                (
                    &arb3,
                    "exploratory",
                    &neutral3_winner_id,
                    mode_changed_winner3,
                ),
                (&arb4, "focused", &neutral4_winner_id, mode_changed_winner4),
            ],
        )?;

        Ok(ExperimentOutcome {
            summary: format!(
                "Replayed 4 scripted turns through mode-aware arbitration. \
                 Turn 1 (Neutral): winner={}. \
                 Turn 2 (Exploratory): winner={} — mode_changed_winner=true. \
                 Turn 3 (Exploratory+floor): winner={} protected=true — mode_changed_winner=false. \
                 Turn 4 (Focused): winner={} — mode_changed_winner=false, curiosity demoted. \
                 executed_effects=0 on every turn.",
                arb1.winner.goal.id,
                arb2.winner.goal.id,
                arb3.winner.goal.id,
                arb4.winner.goal.id,
            ),
            observations: vec![
                "Mode::Neutral.bias_vector() is empty; arbitrate_with_mode(.., Neutral) matches arbitrate().".to_string(),
                "ModeChanged event is applied once per turn; state.mode is always event-sourced.".to_string(),
                "Exploratory promotes curiosity and demotes continuity, flipping the band winner.".to_string(),
                "Floor goal (tier 1, protected) wins under every mode — bias cannot enter the floor.".to_string(),
                "Focused demotes curiosity (bias_applied > 0) and biased_tier > effective_tier.".to_string(),
                "All band losers have biased_tier >= PROTECTED_TIER_FLOOR + 1.".to_string(),
                "executed_effects=0 on every turn (no external write-capable action).".to_string(),
            ],
            failure_modes: vec![
                "Bias magnitudes too small to cross the tier gap would not produce a visible flip; the declared vectors must be verified against the fixture tier spread.".to_string(),
                "A mode biasing a tension in the protected floor (tier <= 3) has no effect; this is correct but may surprise callers.".to_string(),
            ],
            follow_up_questions: vec![
                "Should mode also bias salience/selection scoring (which goals enter context)?".to_string(),
                "Should a mode raise the proposal-threshold for new goal candidates?".to_string(),
                "How is the active mode chosen outside a script — user setting, experiment flag, or model-assisted evaluator?".to_string(),
                "Should the active mode persist across sessions?".to_string(),
            ],
            decision_candidates: vec![
                "Promote the rule 'mode bias may reorder only within the biasable band; protected tiers are immune by construction' to DecisionLog.".to_string(),
            ],
            extra_artifacts: vec!["volition-mode-bias-report.md".to_string()],
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn record_conflict_turn(
    context: &mut RunContext,
    turn: u64,
    phase: &str,
    state: &VolitionState,
    input: &str,
    mode: Mode,
    selection: &SalienceGoalSelectionResult,
    arb: &ModeArbitrationResult,
    neutral_winner_id: &str,
) -> anyhow::Result<uuid::Uuid> {
    let mode_changed_winner = arb.winner.goal.id != neutral_winner_id;
    let per_goal_bias: Vec<serde_json::Value> = {
        let mut goals = vec![json!({
            "goal_id": arb.winner.goal.id,
            "effective_tier": arb.winner_bias.effective_tier,
            "effective_tension_id": arb.winner_effective_tension_id,
            "bias_applied": arb.winner_bias.bias_applied,
            "biased_tier": arb.winner_bias.biased_tier,
            "protected": arb.winner_bias.protected,
        })];
        for loser in &arb.losers {
            goals.push(json!({
                "goal_id": loser.selection.goal.id,
                "effective_tier": loser.bias.effective_tier,
                "effective_tension_id": loser.effective_tension_id,
                "bias_applied": loser.bias.bias_applied,
                "biased_tier": loser.bias.biased_tier,
                "protected": loser.bias.protected,
            }));
        }
        goals
    };

    let detail = json!({
        "turn": turn,
        "phase": phase,
        "tick": state.tick,
        "input": input,
        "events_applied": [{"kind": "mode_changed", "mode": mode.to_string().to_lowercase(), "tick": turn}],
        "active_mode": mode.to_string().to_lowercase(),
        "mode_bias_vector": serde_json::to_value(mode.bias_vector()).unwrap_or_default(),
        "selector_output": {
            "selected_ids": selection.selected.iter().map(|s| &s.goal.id).collect::<Vec<_>>(),
            "omitted_ids": selection.omitted.iter().map(|o| &o.goal.id).collect::<Vec<_>>(),
        },
        "per_goal_bias": per_goal_bias,
        "mode_arbitration": {
            "winner_goal_id": arb.winner.goal.id,
            "winner_biased_tier": arb.winner_bias.biased_tier,
            "losers": arb.losers.iter().map(|l| json!({
                "goal_id": l.selection.goal.id,
                "biased_tier": l.bias.biased_tier,
                "reason": l.reason,
            })).collect::<Vec<_>>(),
        },
        "neutral_winner_goal_id": neutral_winner_id,
        "mode_changed_winner": mode_changed_winner,
        "executed_effects": 0,
        "artifact_reference": "volition-mode-bias-report.md",
    });

    let trace_record = TraceRecord::new(
        context.experiment_id(),
        "volition-mode-bias-turn",
        format!("turn={turn} phase={phase} mode={mode}"),
        format!(
            "tick={} mode={} winner={} mode_changed_winner={} executed_effects=0",
            state.tick, mode, arb.winner.goal.id, mode_changed_winner,
        ),
    )
    .with_details(detail);

    let trace_id = trace_record.trace_id;
    context.record_trace(trace_record)?;
    Ok(trace_id)
}

fn write_mode_bias_report(
    context: &RunContext,
    fixture: &crate::volition::VolitionFixture,
    turns: &[(&ModeArbitrationResult, &str, &str, bool)],
) -> anyhow::Result<()> {
    let mut md = String::new();
    md.push_str("# Volition Mode Bias Experiment\n\n");
    md.push_str(
        "Scripted 4-turn sequence: neutral baseline → Exploratory flip → floor immunity → \
        Focused suppress. executed_effects=0 on every turn.\n\n",
    );

    md.push_str("## Fixture Tensions and Modes\n\n");
    md.push_str("### Tensions\n\n");
    md.push_str("| Tension id | Tier | Summary |\n|---|---:|---|\n");
    for t in &fixture.tensions {
        md.push_str(&format!(
            "| `{}` | {} | {} |\n",
            t.id, t.arbitration_tier, t.summary
        ));
    }
    md.push_str("\n### Mode Bias Vectors\n\n");
    md.push_str("| Mode | `research-curiosity` | `continuity-preservation` | Effect |\n");
    md.push_str("|---|---:|---:|---|\n");
    for mode in [Mode::Neutral, Mode::Focused, Mode::Exploratory] {
        let v = mode.bias_vector();
        md.push_str(&format!(
            "| **{}** | {} | {} | {} |\n",
            mode,
            v.get("research-curiosity")
                .map(|x| x.to_string())
                .unwrap_or_else(|| "0".to_string()),
            v.get("continuity-preservation")
                .map(|x| x.to_string())
                .unwrap_or_else(|| "0".to_string()),
            match mode {
                Mode::Neutral => "identical to `arbitrate()`",
                Mode::Focused => "suppress tangents; favor continuity",
                Mode::Exploratory => "promote curiosity above continuity",
            },
        ));
    }

    md.push_str("\n## Per-Turn Results\n\n");
    md.push_str(
        "| Turn | Mode | Winner | Neutral winner | mode_changed_winner | Winner protected |\n",
    );
    md.push_str("|---:|---|---|---|:---:|:---:|\n");
    for (i, (arb, mode_label, neutral_winner, mode_changed)) in turns.iter().enumerate() {
        md.push_str(&format!(
            "| {} | {} | `{}` | `{}` | {} | {} |\n",
            i + 1,
            mode_label,
            arb.winner.goal.id,
            neutral_winner,
            if *mode_changed { "**true**" } else { "false" },
            if arb.winner_bias.protected {
                "yes"
            } else {
                "no"
            },
        ));
    }

    md.push_str("\n## Per-Goal Bias Details (all turns)\n\n");
    for (i, (arb, mode_label, _, _)) in turns.iter().enumerate() {
        md.push_str(&format!("### Turn {} ({})\n\n", i + 1, mode_label));
        md.push_str(
            "| Goal id | Effective tier | Tension | Bias applied | Biased tier | Protected |\n",
        );
        md.push_str("|---|---:|---|---:|---:|:---:|\n");
        md.push_str(&format!(
            "| `{}` (**winner**) | {} | `{}` | {} | {} | {} |\n",
            arb.winner.goal.id,
            arb.winner_bias.effective_tier,
            arb.winner_effective_tension_id,
            arb.winner_bias.bias_applied,
            arb.winner_bias.biased_tier,
            if arb.winner_bias.protected {
                "yes"
            } else {
                "no"
            },
        ));
        for loser in &arb.losers {
            md.push_str(&format!(
                "| `{}` | {} | `{}` | {} | {} | {} |\n",
                loser.selection.goal.id,
                loser.bias.effective_tier,
                loser.effective_tension_id,
                loser.bias.bias_applied,
                loser.bias.biased_tier,
                if loser.bias.protected { "yes" } else { "no" },
            ));
        }
        md.push('\n');
    }

    md.push_str("## Human Verification Checklist\n\n");
    md.push_str("- [ ] The active mode and its bias vector are legible in each turn; a reader can see *why* the ordering changed without rerunning the code.\n");
    md.push_str("- [ ] The Exploratory flip and the Focused non-flip read as sensible, deterministic consequences of the declared vectors — not arbitrary.\n");
    md.push_str("- [ ] The floor-immunity turn is convincing: the tier-1 goal wins and nothing suggests a mode could elevate a band goal above it.\n");
    md.push_str("- [ ] The mode is clearly a structural handle over an explicit vector — no free-form mood label is doing hidden work.\n");
    md.push_str(
        "- [ ] Nothing in the output implies the mode caused any external write-capable action.\n",
    );
    md.push_str("- [ ] `executed_effects=0` on every turn.\n");

    fs::write(context.run_dir().join("volition-mode-bias-report.md"), md).with_context(|| {
        format!(
            "failed to write mode bias report for run {}",
            context.run_id()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::super::registry::{ExperimentName, run_experiment_in};
    use crate::context::ContextBudget;
    use crate::volition::{
        Mode, PROTECTED_TIER_FLOOR, VolitionEvent, VolitionState, apply, arbitrate_with_mode,
        select_goals_with_salience, static_fixture,
    };
    use serde_json::json;

    #[test]
    fn turn1_neutral_winner_is_continuity_goal() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let input = "The open thread about voice memory evidence is unresolved.";
        let sel = select_goals_with_salience(input, &fixture, &state, ContextBudget::new(4, 100));
        let arb = arbitrate_with_mode(sel.selected, &fixture, Mode::Neutral).unwrap();
        assert_eq!(arb.winner.goal.id, "resurface-open-thread");
        assert!(!arb.winner_bias.protected);
    }

    #[test]
    fn turn2_exploratory_flips_winner_to_curiosity_goal() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let input = "The open thread about voice memory evidence is unresolved.";
        let sel = select_goals_with_salience(input, &fixture, &state, ContextBudget::new(4, 100));

        let neutral = arbitrate_with_mode(sel.selected.clone(), &fixture, Mode::Neutral).unwrap();
        let exploratory = arbitrate_with_mode(sel.selected, &fixture, Mode::Exploratory).unwrap();

        assert_eq!(neutral.winner.goal.id, "resurface-open-thread");
        assert_eq!(exploratory.winner.goal.id, "clarify-weak-evidence-topic");
        assert_ne!(neutral.winner.goal.id, exploratory.winner.goal.id);
    }

    #[test]
    fn turn3_floor_goal_wins_under_exploratory() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let input =
            "Is the voice memory work complete, or is the evidence thread still unresolved?";
        let sel = select_goals_with_salience(input, &fixture, &state, ContextBudget::new(4, 100));

        let arb = arbitrate_with_mode(sel.selected, &fixture, Mode::Exploratory).unwrap();
        assert_eq!(arb.winner.goal.id, "avoid-overstating-impl-status");
        assert!(arb.winner_bias.protected);
        assert_eq!(arb.winner_bias.biased_tier, arb.winner_bias.effective_tier);

        // All band losers must have biased_tier >= PROTECTED_TIER_FLOOR + 1.
        for loser in &arb.losers {
            if !loser.bias.protected {
                assert!(loser.bias.biased_tier > PROTECTED_TIER_FLOOR);
            }
        }
    }

    #[test]
    fn turn4_focused_keeps_continuity_and_demotes_curiosity() {
        let fixture = static_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let input = "The open thread about voice memory evidence is unresolved.";
        let sel = select_goals_with_salience(input, &fixture, &state, ContextBudget::new(4, 100));

        let neutral = arbitrate_with_mode(sel.selected.clone(), &fixture, Mode::Neutral).unwrap();
        let focused = arbitrate_with_mode(sel.selected, &fixture, Mode::Focused).unwrap();

        assert_eq!(focused.winner.goal.id, "resurface-open-thread");
        assert_eq!(neutral.winner.goal.id, "resurface-open-thread");

        let curiosity = focused
            .losers
            .iter()
            .find(|l| l.selection.goal.id == "clarify-weak-evidence-topic")
            .unwrap();
        assert!(curiosity.bias.bias_applied > 0);
        assert!(curiosity.bias.biased_tier > curiosity.bias.effective_tier);
    }

    #[test]
    fn mode_changed_event_is_event_sourced_across_turns() {
        let fixture = static_fixture();
        let mut state = VolitionState::from_fixture(&fixture);
        assert_eq!(state.mode, Mode::Neutral);

        state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Neutral,
                tick: 1,
            },
        );
        assert_eq!(state.mode, Mode::Neutral);

        state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Exploratory,
                tick: 2,
            },
        );
        assert_eq!(state.mode, Mode::Exploratory);

        state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Exploratory,
                tick: 3,
            },
        );
        assert_eq!(state.mode, Mode::Exploratory);

        state = apply(
            state,
            VolitionEvent::ModeChanged {
                mode: Mode::Focused,
                tick: 4,
            },
        );
        assert_eq!(state.mode, Mode::Focused);
    }

    #[test]
    fn experiment_runs_and_produces_artifacts() {
        let base_dir = std::env::temp_dir().join(format!("qsf-mode-bias-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(&base_dir, ExperimentName::VolitionModeBias).unwrap();

        assert_eq!(summary.experiment_id, "volition-mode-bias");
        assert_eq!(summary.status, "completed");

        let report = std::fs::read_to_string(summary.run_dir.join("Report.md")).unwrap();
        assert!(report.contains("volition-mode-bias"));

        let artifact =
            std::fs::read_to_string(summary.run_dir.join("volition-mode-bias-report.md")).unwrap();
        assert!(artifact.contains("Human Verification Checklist"));
        assert!(artifact.contains("executed_effects=0"));

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn all_turns_record_executed_effects_zero() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-mode-bias-effects-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(&base_dir, ExperimentName::VolitionModeBias).unwrap();

        let events = std::fs::read_to_string(summary.run_dir.join("events.jsonl")).unwrap();
        let nonzero = events
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|record| {
                record
                    .get("payload")
                    .and_then(|p| p.get("executed_effects"))
                    .and_then(|e| e.as_u64())
                    .map(|e| e != 0)
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(
            nonzero, 0,
            "every event payload with executed_effects must record 0"
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn flip_turn_trace_records_mode_changed_winner_true() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-mode-bias-flip-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(&base_dir, ExperimentName::VolitionModeBias).unwrap();

        let traces = std::fs::read_to_string(summary.run_dir.join("traces.jsonl")).unwrap();

        let flip_trace = traces
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|v| {
                v.get("details")
                    .and_then(|d| d.get("turn"))
                    .and_then(|t| t.as_u64())
                    == Some(2)
            });
        let flip = flip_trace.expect("turn 2 trace must exist");
        let mode_changed = flip
            .get("details")
            .and_then(|d| d.get("mode_changed_winner"))
            .and_then(|v| v.as_bool());
        assert_eq!(
            mode_changed,
            Some(true),
            "turn 2 (Exploratory) must record mode_changed_winner=true"
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn floor_turn_trace_records_mode_changed_winner_false_and_protected() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-mode-bias-floor-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(&base_dir, ExperimentName::VolitionModeBias).unwrap();

        let traces = std::fs::read_to_string(summary.run_dir.join("traces.jsonl")).unwrap();

        let floor_trace = traces
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .find(|v| {
                v.get("details")
                    .and_then(|d| d.get("turn"))
                    .and_then(|t| t.as_u64())
                    == Some(3)
            });
        let floor = floor_trace.expect("turn 3 trace must exist");
        let details = floor.get("details").unwrap();
        assert_eq!(
            details.get("mode_changed_winner").and_then(|v| v.as_bool()),
            Some(false),
            "turn 3 (floor) must record mode_changed_winner=false"
        );
        let winner_goal_id = details
            .get("mode_arbitration")
            .and_then(|a| a.get("winner_goal_id"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        assert_eq!(winner_goal_id, "avoid-overstating-impl-status");

        // Verify winner is marked protected in per_goal_bias
        let winner_protected = details
            .get("per_goal_bias")
            .and_then(|g| g.as_array())
            .and_then(|arr| {
                arr.iter().find(|e| {
                    e.get("goal_id").and_then(|v| v.as_str())
                        == Some("avoid-overstating-impl-status")
                })
            })
            .and_then(|e| e.get("protected"))
            .and_then(|v| v.as_bool());
        assert_eq!(
            winner_protected,
            Some(true),
            "floor winner must be marked protected in per_goal_bias"
        );

        std::fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn replay_produces_identical_turn2_trace() {
        let base1 =
            std::env::temp_dir().join(format!("qsf-mode-bias-rep1-{}", uuid::Uuid::new_v4()));
        let base2 =
            std::env::temp_dir().join(format!("qsf-mode-bias-rep2-{}", uuid::Uuid::new_v4()));

        let summary1 = run_experiment_in(&base1, ExperimentName::VolitionModeBias).unwrap();
        let summary2 = run_experiment_in(&base2, ExperimentName::VolitionModeBias).unwrap();

        let turn2_stable_fields = |run_dir: &std::path::Path| {
            let traces = std::fs::read_to_string(run_dir.join("traces.jsonl")).unwrap();
            traces
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .find(|v| {
                    v.get("details")
                        .and_then(|d| d.get("turn"))
                        .and_then(|t| t.as_u64())
                        == Some(2)
                })
                .and_then(|v| v.get("details").cloned())
                .map(|d| {
                    json!({
                        "winner_goal_id": d.get("mode_arbitration").and_then(|a| a.get("winner_goal_id")),
                        "neutral_winner_goal_id": d.get("neutral_winner_goal_id"),
                        "mode_changed_winner": d.get("mode_changed_winner"),
                        "per_goal_bias": d.get("per_goal_bias"),
                        "executed_effects": d.get("executed_effects"),
                    })
                })
        };

        assert_eq!(
            turn2_stable_fields(&summary1.run_dir),
            turn2_stable_fields(&summary2.run_dir),
            "turn 2 stable trace fields must be identical across replays"
        );

        std::fs::remove_dir_all(base1).unwrap();
        std::fs::remove_dir_all(base2).unwrap();
    }

    #[test]
    fn all_turn_traces_satisfy_trace_contract() {
        let base_dir =
            std::env::temp_dir().join(format!("qsf-mode-bias-contract-{}", uuid::Uuid::new_v4()));
        let summary = run_experiment_in(&base_dir, ExperimentName::VolitionModeBias).unwrap();

        let traces = std::fs::read_to_string(summary.run_dir.join("traces.jsonl")).unwrap();
        let turn_traces: Vec<serde_json::Value> = traces
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|v| v.get("details").and_then(|d| d.get("turn")).is_some())
            .collect();

        assert_eq!(
            turn_traces.len(),
            4,
            "experiment must produce exactly 4 turn traces"
        );

        let required_fields = [
            "input",
            "events_applied",
            "active_mode",
            "mode_bias_vector",
            "selector_output",
            "per_goal_bias",
            "mode_arbitration",
            "neutral_winner_goal_id",
            "mode_changed_winner",
            "executed_effects",
            "artifact_reference",
        ];

        for trace in &turn_traces {
            let details = trace.get("details").unwrap();
            let turn = details.get("turn").and_then(|t| t.as_u64()).unwrap_or(0);
            for field in &required_fields {
                assert!(
                    details.get(field).is_some(),
                    "turn {turn} trace missing required contract field `{field}`"
                );
            }
            assert_eq!(
                details.get("executed_effects").and_then(|v| v.as_u64()),
                Some(0),
                "turn {turn} must record executed_effects=0"
            );
            assert!(
                details
                    .get("mode_arbitration")
                    .and_then(|a| a.get("winner_goal_id"))
                    .and_then(|v| v.as_str())
                    .is_some(),
                "turn {turn} mode_arbitration must contain a winner_goal_id string"
            );
        }

        std::fs::remove_dir_all(base_dir).unwrap();
    }
}
