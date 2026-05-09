use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExperimentReport {
    pub experiment_id: String,
    pub run_id: String,
    pub status: String,
    pub summary: String,
    pub configuration: Vec<String>,
    pub event_count: usize,
    pub trace_count: usize,
    pub observations: Vec<String>,
    pub failure_modes: Vec<String>,
    pub follow_up_questions: Vec<String>,
    pub decision_candidates: Vec<String>,
}

pub fn write_report(path: impl AsRef<Path>, report: &ExperimentReport) -> anyhow::Result<()> {
    let mut markdown = String::new();
    markdown.push_str("# Experiment Report\n\n");
    markdown.push_str(&format!("Experiment: `{}`\n\n", report.experiment_id));
    markdown.push_str(&format!("Run: `{}`\n\n", report.run_id));
    markdown.push_str(&format!("Status: `{}`\n\n", report.status));
    markdown.push_str("## Summary\n\n");
    markdown.push_str(&report.summary);
    markdown.push_str("\n\n## Configuration\n\n");
    push_list(&mut markdown, &report.configuration);
    markdown.push_str("\n## Key Measurements\n\n");
    markdown.push_str(&format!(
        "- Structured events written: {}\n",
        report.event_count
    ));
    markdown.push_str(&format!(
        "- Structured traces written: {}\n",
        report.trace_count
    ));
    markdown.push_str("\n## Observations\n\n");
    push_list(&mut markdown, &report.observations);
    markdown.push_str("\n## Failure Modes\n\n");
    push_list(&mut markdown, &report.failure_modes);
    markdown.push_str("\n## Follow-up Questions\n\n");
    push_list(&mut markdown, &report.follow_up_questions);
    markdown.push_str("\n## Decision Candidates\n\n");
    push_list(&mut markdown, &report.decision_candidates);
    markdown.push_str("\n## Artifact Links\n\n");
    markdown.push_str("- `engine.log`\n");
    markdown.push_str("- `events.jsonl`\n");
    markdown.push_str("- `traces.jsonl`\n");

    fs::write(path, markdown)?;
    Ok(())
}

fn push_list(markdown: &mut String, items: &[String]) {
    if items.is_empty() {
        markdown.push_str("- None recorded.\n");
        return;
    }

    for item in items {
        markdown.push_str(&format!("- {item}\n"));
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{ExperimentReport, write_report};

    #[test]
    fn report_contains_core_artifact_links() {
        let base_dir = std::env::temp_dir().join(format!("qsf-report-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&base_dir).unwrap();
        let report_path = base_dir.join("Report.md");

        write_report(
            &report_path,
            &ExperimentReport {
                experiment_id: "test-experiment".to_string(),
                run_id: "test-run".to_string(),
                status: "completed".to_string(),
                summary: "Test report.".to_string(),
                configuration: vec![
                    "Runner: test runner".to_string(),
                    "Event log: `events.jsonl`".to_string(),
                    "Trace log: `traces.jsonl`".to_string(),
                    "Developer log: `engine.log`".to_string(),
                ],
                event_count: 3,
                trace_count: 1,
                observations: vec!["Observed JSONL output.".to_string()],
                failure_modes: vec!["No failure.".to_string()],
                follow_up_questions: vec![],
                decision_candidates: vec!["Keep test report shape.".to_string()],
            },
        )
        .unwrap();

        let markdown = fs::read_to_string(&report_path).unwrap();

        assert!(markdown.contains("events.jsonl"));
        assert!(markdown.contains("traces.jsonl"));
        assert!(markdown.contains("engine.log"));

        fs::remove_dir_all(base_dir).unwrap();
    }
}
