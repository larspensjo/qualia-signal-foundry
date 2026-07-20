use qsf_volition::{GoalDefinitionProvenance, GoalDetailReport, GoalStatus, KeywordWeightClass};

use crate::console::styling::{
    ColorMode, STYLE_DIRECT_BODY, STYLE_DIRECT_HEADER, STYLE_HINT_BLOCK, paint,
};

/// Snapshot fields supplied by the loading adapter for the report header.
pub struct GoalDetailHeader<'a> {
    pub session_id: &'a str,
    pub seed_fixture_id: &'a str,
    pub recorded_at: &'a str,
}

/// Renders a complete goal detail report without doing any loading or terminal inspection.
pub fn render_goal_detail_report(
    report: &GoalDetailReport,
    header: GoalDetailHeader<'_>,
    color_mode: ColorMode,
) -> String {
    let mut output = String::new();
    output.push_str(&paint(color_mode, STYLE_DIRECT_HEADER, "Volition goals"));
    output.push('\n');
    output.push_str(&format!(
        "session: {}\nseed fixture: {}\nrecorded at: {}\nmode: {}\n",
        header.session_id, header.seed_fixture_id, header.recorded_at, report.mode
    ));

    for goal in &report.goals {
        output.push('\n');
        output.push_str(&paint(
            color_mode,
            STYLE_DIRECT_BODY,
            &format!(
                "{} ({})",
                goal.id,
                goal_status(goal.dynamic.as_ref().map(|d| d.status))
            ),
        ));
        output.push('\n');

        match &goal.definition {
            Some(definition) => {
                output.push_str(&format!(
                    "title: {}\nsummary: {}\n",
                    definition.title, definition.summary
                ));
                output.push_str(&format!(
                    "scope: {}\nbase priority: {}\nvisibility: {}\n",
                    definition.scope, definition.base_priority, definition.visibility
                ));
                output.push_str(&format!(
                    "activation keywords: {}\n",
                    definition
                        .activation_keywords
                        .iter()
                        .map(|keyword| format!(
                            "{} ({})",
                            keyword.term,
                            weight_class(keyword.weight_class)
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                output.push_str(&format!(
                    "allowed effects: {}\nsatisfaction: {}\nevidence refs: {}\nsource reference: {}\n",
                    definition
                        .allowed_effects
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                    definition.satisfaction_condition_summary,
                    definition.evidence_refs.join(", "),
                    definition.source_reference
                ));
            }
            None => output.push_str("definition: unavailable (snapshot/code drift)\n"),
        }

        match &goal.dynamic {
            Some(dynamic) => output.push_str(&format!(
                "salience: {}\ncooldown until tick: {}\nlast activated tick: {}\nadmitted tick: {}\n",
                dynamic.salience,
                option_tick(dynamic.cooldown_until_tick),
                option_tick(dynamic.last_activated_tick),
                option_tick(dynamic.admitted_tick)
            )),
            None => output.push_str("dynamic state: unavailable (accepted definition has no lifecycle record)\n"),
        }

        output.push_str(&format!(
            "tensions: {}\n",
            goal.parent_tensions
                .iter()
                .map(|tension| format!("{} (tier {})", tension.title, tension.arbitration_tier))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        output.push_str(&format!(
            "effective tension: {}\n",
            if goal.effective_tension_id.is_empty() {
                "unavailable".to_string()
            } else {
                format!(
                    "{} ({})",
                    goal.effective_tension_title, goal.effective_tension_id
                )
            }
        ));
        output.push_str(&tier_line(
            goal.effective_tier,
            goal.mode_bias.biased_tier,
            report,
        ));
        output.push('\n');
        output.push_str(&format!(
            "provenance: {}\n",
            provenance_label(goal.provenance, header.seed_fixture_id)
        ));
        if goal.missing_tension_assignment {
            output.push_str(&paint(
                color_mode,
                STYLE_HINT_BLOCK,
                "warning: missing tension assignment (effective tier u8::MAX)",
            ));
            output.push('\n');
        }
    }

    output
}

fn goal_status(status: Option<GoalStatus>) -> String {
    status.map_or_else(
        || "missing dynamic state".to_string(),
        |status| status.to_string(),
    )
}

fn option_tick(tick: Option<u64>) -> String {
    tick.map_or_else(|| "none".to_string(), |tick| tick.to_string())
}

fn weight_class(weight: KeywordWeightClass) -> &'static str {
    match weight {
        KeywordWeightClass::Weak => "weak",
        KeywordWeightClass::Normal => "normal",
        KeywordWeightClass::Strong => "strong",
    }
}

fn tier_line(effective_tier: u8, biased_tier: u8, report: &GoalDetailReport) -> String {
    let raw = if effective_tier == u8::MAX {
        "tier u8::MAX (unassigned)".to_string()
    } else {
        format!("tier {effective_tier}")
    };
    if biased_tier == effective_tier {
        raw
    } else {
        format!("{raw} (mode-biased {biased_tier} under {})", report.mode)
    }
}

fn provenance_label(provenance: GoalDefinitionProvenance, seed_fixture_id: &str) -> String {
    match provenance {
        GoalDefinitionProvenance::ReconstructedFromFixture => {
            format!("definition reconstructed from current code (fixture {seed_fixture_id})")
        }
        GoalDefinitionProvenance::RecordedInSnapshot => {
            "definition recorded in snapshot".to_string()
        }
        GoalDefinitionProvenance::DefinitionUnavailable => "definition unavailable".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use qsf_volition::{
        Mode, VolitionEvent, VolitionState, apply, build_goal_detail_report, realtime_seed_fixture,
    };

    use super::*;

    fn header() -> GoalDetailHeader<'static> {
        GoalDetailHeader {
            session_id: "default",
            seed_fixture_id: "realtime_seed_fixture",
            recorded_at: "2026-07-19T00:00:00Z",
        }
    }

    #[test]
    fn disabled_color_output_contains_goal_detail_fields() {
        let fixture = realtime_seed_fixture();
        let report = build_goal_detail_report(&VolitionState::from_fixture(&fixture), &fixture);
        let rendered = render_goal_detail_report(&report, header(), ColorMode::Disabled);

        assert!(rendered.contains("Track the AI transition"));
        assert!(rendered.contains("AI-trajectory concern (tier 5)"));
        assert!(rendered.contains("tier 5"));
        assert!(rendered.contains("definition reconstructed from current code"));
    }

    #[test]
    fn mode_bias_annotation_is_present_only_when_tier_changes() {
        let fixture = realtime_seed_fixture();
        let state = apply(
            VolitionState::from_fixture(&fixture),
            VolitionEvent::ModeChanged {
                mode: Mode::Exploratory,
                tick: 1,
            },
        );
        let report = build_goal_detail_report(&state, &fixture);
        let rendered = render_goal_detail_report(&report, header(), ColorMode::Disabled);

        assert!(rendered.contains("mode-biased 4 under Exploratory"));
        assert!(!rendered.contains("tier 1 (mode-biased"));
    }

    #[test]
    fn enabled_color_output_emits_escape_codes_and_resets() {
        let fixture = realtime_seed_fixture();
        let report = build_goal_detail_report(&VolitionState::from_fixture(&fixture), &fixture);
        let rendered = render_goal_detail_report(&report, header(), ColorMode::Enabled);

        assert!(rendered.contains("\x1b["));
        assert!(rendered.contains("\x1b[0m"));
    }
}
