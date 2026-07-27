use crate::transcript::join::TranscriptRun;
use crate::transcript::model::TranscriptLine;

/// Serializes runs as JSONL: one session header line per run, then one line per turn. `pretty`
/// indents each record, which is no longer strict JSONL but stays a valid concatenated JSON
/// stream, so the two forms parse to identical values.
pub fn render_runs(runs: Vec<TranscriptRun>, pretty: bool) -> anyhow::Result<String> {
    let mut out = String::new();
    for run in runs {
        push_line(&mut out, &TranscriptLine::Session(run.header), pretty)?;
        for turn in run.turns {
            push_line(&mut out, &TranscriptLine::Turn(turn), pretty)?;
        }
    }
    Ok(out)
}

fn push_line(out: &mut String, line: &TranscriptLine, pretty: bool) -> anyhow::Result<()> {
    let rendered = if pretty {
        serde_json::to_string_pretty(line)?
    } else {
        serde_json::to_string(line)?
    };
    out.push_str(&rendered);
    out.push('\n');
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::transcript::model::{SessionLine, SourceIntegrity, TurnLine};

    use super::*;

    fn turn(index: usize, user: &str) -> TurnLine {
        TurnLine {
            turn: index,
            at: None,
            user: user.to_string(),
            assistant: Some("reply".to_string()),
            status: "completed".to_string(),
            volition: None,
            initiative: None,
            formation: None,
            world: None,
            undecodable: vec![],
            traces: None,
        }
    }

    fn run_with_two_turns() -> TranscriptRun {
        TranscriptRun {
            header: SessionLine {
                session_id: "s".to_string(),
                ledger: "ledger.jsonl".to_string(),
                run_index: 1,
                run_started_at: None,
                turn_count: 2,
                source: SourceIntegrity {
                    complete: true,
                    skipped_line_count: 0,
                    skipped_lines: vec![],
                    orphans: Default::default(),
                },
            },
            turns: vec![turn(0, "first"), turn(1, "second")],
        }
    }

    #[test]
    fn compact_output_is_one_line_per_record() {
        let rendered = render_runs(vec![run_with_two_turns()], false).expect("render");

        let lines: Vec<&str> = rendered.lines().collect();
        assert_eq!(lines.len(), 3, "one session header plus two turns");
        for line in &lines {
            serde_json::from_str::<serde_json::Value>(line).expect("each line is valid JSON");
        }
    }

    #[test]
    fn pretty_output_parses_to_the_same_values_as_compact() {
        let compact = render_runs(vec![run_with_two_turns()], false).expect("compact");
        let pretty = render_runs(vec![run_with_two_turns()], true).expect("pretty");

        let compact_values: Vec<serde_json::Value> = compact
            .lines()
            .map(|line| serde_json::from_str(line).expect("compact line"))
            .collect();
        let pretty_values: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&pretty)
            .into_iter::<serde_json::Value>()
            .map(|value| value.expect("pretty value"))
            .collect();

        assert_eq!(compact_values, pretty_values);
    }
}
