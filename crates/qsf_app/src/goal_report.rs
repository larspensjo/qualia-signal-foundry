use qsf_volition::{GoalDetail, GoalDetailReport};

use crate::console::goal_detail_view::{GoalDetailHeader, render_goal_detail_report};

/// One self-describing line in the goals report stream.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum GoalReportLine<'a> {
    Session(GoalSessionLine),
    Goal(&'a GoalDetail),
}

#[derive(Debug, serde::Serialize)]
struct GoalSessionLine {
    session_id: String,
    seed_fixture_id: String,
    recorded_at: String,
}

/// Renders the goals command's selected output mode without loading state or writing files.
pub fn render_goal_report(
    report: &GoalDetailReport,
    header: GoalDetailHeader<'_>,
    pretty: bool,
) -> anyhow::Result<String> {
    if pretty {
        return Ok(render_goal_detail_report(
            report,
            header,
            crate::console::styling::ColorMode::for_stdout(),
        ));
    }

    render_goal_jsonl(report, header)
}

fn render_goal_jsonl(
    report: &GoalDetailReport,
    header: GoalDetailHeader<'_>,
) -> anyhow::Result<String> {
    let mut output = String::new();
    push_json_line(
        &mut output,
        &GoalReportLine::Session(GoalSessionLine {
            session_id: header.session_id.to_string(),
            seed_fixture_id: header.seed_fixture_id.to_string(),
            recorded_at: header.recorded_at.to_string(),
        }),
    )?;

    for goal in &report.goals {
        push_json_line(&mut output, &GoalReportLine::Goal(goal))?;
    }

    Ok(output)
}

fn push_json_line(output: &mut String, line: &GoalReportLine<'_>) -> anyhow::Result<()> {
    output.push_str(&serde_json::to_string(line)?);
    output.push('\n');
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use qsf_volition::{VolitionState, build_goal_detail_report, realtime_seed_fixture};

    use super::*;

    fn header() -> GoalDetailHeader<'static> {
        GoalDetailHeader {
            session_id: "default",
            seed_fixture_id: "realtime_seed_fixture",
            recorded_at: "2026-07-19T00:00:00Z",
        }
    }

    fn report() -> GoalDetailReport {
        let fixture = realtime_seed_fixture();
        build_goal_detail_report(&VolitionState::from_fixture(&fixture), &fixture)
    }

    #[test]
    fn jsonl_starts_with_a_self_describing_session_header() {
        let rendered = render_goal_jsonl(&report(), header()).expect("render");
        let first = rendered.lines().next().expect("session line");
        let value: serde_json::Value = serde_json::from_str(first).expect("valid JSON");

        assert_eq!(value["kind"], "session");
        assert_eq!(value["session_id"], "default");
        assert_eq!(value["seed_fixture_id"], "realtime_seed_fixture");
        assert_eq!(value["recorded_at"], "2026-07-19T00:00:00Z");
        assert_eq!(value.as_object().unwrap().keys().count(), 4);
    }

    #[test]
    fn jsonl_emits_one_goal_line_with_full_detail_and_stable_keys() {
        let report = report();
        let rendered = render_goal_jsonl(&report, header()).expect("render");
        let lines: Vec<serde_json::Value> = rendered
            .lines()
            .map(|line| serde_json::from_str(line).expect("valid JSON"))
            .collect();

        assert_eq!(lines.len(), report.goals.len() + 1);
        let goal = lines
            .iter()
            .find(|line| line["id"] == "track-the-ai-transition")
            .expect("goal line");
        assert_eq!(goal["id"], "track-the-ai-transition");
        assert_eq!(goal["provenance"], "reconstructed_from_fixture");
        assert!(goal["definition"]["title"].is_string());
        assert!(goal["dynamic"]["salience"].is_number());

        let expected_keys = BTreeSet::from([
            "kind",
            "id",
            "definition",
            "dynamic",
            "parent_tensions",
            "effective_tension_id",
            "effective_tension_title",
            "effective_tier",
            "mode_bias",
            "provenance",
            "missing_tension_assignment",
            "missing_dynamic_state",
        ]);
        let actual_keys: BTreeSet<&str> = goal
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(actual_keys, expected_keys);
    }

    #[test]
    fn pretty_output_uses_the_existing_console_renderer() {
        let report = report();
        let expected = render_goal_detail_report(
            &report,
            header(),
            crate::console::styling::ColorMode::for_stdout(),
        );
        let actual = render_goal_report(&report, header(), true).expect("render");

        assert_eq!(actual, expected);
    }
}
