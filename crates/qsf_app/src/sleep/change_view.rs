use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SleepStateOutcome {
    ConsumedSession,
    AlreadyConsumed,
    NoPersistedSession,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewMemoryChange {
    pub id: String,
    pub title: String,
    pub importance: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NewAssociationChange {
    pub from_id: String,
    pub to_id: String,
    pub weight: f64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrengthenedAssociationChange {
    pub from_id: String,
    pub to_id: String,
    pub old_weight: f64,
    pub new_weight: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SleepChangeRecord {
    pub state_outcome: SleepStateOutcome,
    pub session_id: Option<String>,
    pub state_dir: String,
    pub new_memories: Vec<NewMemoryChange>,
    pub skipped_duplicates: Vec<String>,
    pub new_associations: Vec<NewAssociationChange>,
    pub strengthened_associations: Vec<StrengthenedAssociationChange>,
    pub admitted_goal_id: Option<String>,
    pub declined_goal_candidate_id: Option<String>,
    pub swept_goal_ids: Vec<String>,
    pub open_question_count: usize,
    pub decision_candidate_count: usize,
    pub state_files_written: Vec<String>,
}

pub fn render_change_view(record: &SleepChangeRecord) -> String {
    let mut view = String::new();

    match record.state_outcome {
        SleepStateOutcome::NoPersistedSession => {
            view.push_str("Sleep update - No persisted session to consume; ran the smoke-test summarization only. state unchanged.\n");
            return view;
        }
        SleepStateOutcome::AlreadyConsumed => {
            view.push_str(&format!(
                "Sleep update - session `{}` was already consumed; state unchanged.\n",
                record.session_id.as_deref().unwrap_or("unknown")
            ));
            return view;
        }
        SleepStateOutcome::ConsumedSession => {
            view.push_str(&format!(
                "Sleep update - session `{}`\n",
                record.session_id.as_deref().unwrap_or("unknown")
            ));
        }
    }

    view.push_str(&format!(
        "\nMemories added ({}):\n",
        record.new_memories.len()
    ));
    if record.new_memories.is_empty() {
        view.push_str("  (none)\n");
    }
    for memory in &record.new_memories {
        view.push_str(&format!(
            "  + {}  \"{}\"  (importance {:.2})\n",
            memory.id, memory.title, memory.importance
        ));
    }
    match record.skipped_duplicates.len() {
        0 => {}
        1 => view.push_str(&format!(
            "  = 1 duplicate skipped: \"{}\"\n",
            record.skipped_duplicates[0]
        )),
        count => view.push_str(&format!(
            "  = {count} duplicates skipped: {}\n",
            record
                .skipped_duplicates
                .iter()
                .map(|title| format!("\"{title}\""))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }

    view.push_str("\nAssociations:\n");
    if record.new_associations.is_empty() && record.strengthened_associations.is_empty() {
        view.push_str("  (none)\n");
    }
    for association in &record.new_associations {
        view.push_str(&format!(
            "  + {} -> {}  ({:.2})  {}\n",
            association.from_id, association.to_id, association.weight, association.reason
        ));
    }
    for strengthened in &record.strengthened_associations {
        view.push_str(&format!(
            "  ~ {} -> {}  weight {:.2} -> {:.2}\n",
            strengthened.from_id,
            strengthened.to_id,
            strengthened.old_weight,
            strengthened.new_weight
        ));
    }

    view.push_str("\nGoals:\n");
    let mut goal_lines = Vec::new();
    if let Some(admitted) = &record.admitted_goal_id {
        goal_lines.push(format!("  admitted `{admitted}`"));
    }
    if let Some(declined) = &record.declined_goal_candidate_id {
        goal_lines.push(format!("  declined candidate `{declined}`"));
    }
    if !record.swept_goal_ids.is_empty() {
        goal_lines.push(format!("  swept: {}", record.swept_goal_ids.join(", ")));
    }
    if goal_lines.is_empty() {
        view.push_str("  (no changes)\n");
    } else {
        for line in goal_lines {
            view.push_str(&line);
            view.push('\n');
        }
    }

    view.push_str(&format!(
        "\nOpen questions ({}), decision candidates ({}) - see sleep-report.md\n",
        record.open_question_count, record.decision_candidate_count
    ));

    view.push_str(&format!(
        "\nState files written under {}:\n",
        record.state_dir
    ));
    if record.state_files_written.is_empty() {
        view.push_str("  (none)\n");
    }
    for file in &record.state_files_written {
        view.push_str(&format!("  {file}\n"));
    }

    view
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_record() -> SleepChangeRecord {
        SleepChangeRecord {
            state_outcome: SleepStateOutcome::ConsumedSession,
            session_id: Some("realtime-session-1".to_string()),
            state_dir: "state/realtime".to_string(),
            new_memories: vec![NewMemoryChange {
                id: "memory.sleep.run-1.001".to_string(),
                title: "User prefers itemized views.".to_string(),
                importance: 0.8,
            }],
            skipped_duplicates: vec!["Reducers stay pure.".to_string()],
            new_associations: vec![NewAssociationChange {
                from_id: "memory.sleep.run-1.001".to_string(),
                to_id: "memory.a".to_string(),
                weight: 0.42,
                reason: "Both describe sleep UX.".to_string(),
            }],
            strengthened_associations: vec![StrengthenedAssociationChange {
                from_id: "memory.a".to_string(),
                to_id: "memory.c".to_string(),
                old_weight: 0.40,
                new_weight: 0.45,
            }],
            admitted_goal_id: Some("goal.continuity".to_string()),
            declined_goal_candidate_id: None,
            swept_goal_ids: vec![],
            open_question_count: 1,
            decision_candidate_count: 2,
            state_files_written: vec!["state/realtime/memory-store.json".to_string()],
        }
    }

    #[test]
    fn renders_all_sections_for_a_consumed_session() {
        let view = render_change_view(&full_record());

        assert!(view.contains("session `realtime-session-1`"));
        assert!(view.contains("Memories added (1):"));
        assert!(view.contains("+ memory.sleep.run-1.001"));
        assert!(view.contains("\"User prefers itemized views.\""));
        assert!(view.contains("(importance 0.80)"));
        assert!(view.contains("1 duplicate skipped: \"Reducers stay pure.\""));
        assert!(
            view.contains("+ memory.sleep.run-1.001 -> memory.a  (0.42)  Both describe sleep UX.")
        );
        assert!(view.contains("~ memory.a -> memory.c  weight 0.40 -> 0.45"));
        assert!(view.contains("admitted `goal.continuity`"));
        assert!(view.contains("Open questions (1), decision candidates (2) - see sleep-report.md"));
        assert!(view.contains("state/realtime/memory-store.json"));
    }

    #[test]
    fn renders_no_change_placeholders_when_sections_are_empty() {
        let record = SleepChangeRecord {
            new_memories: vec![],
            skipped_duplicates: vec![],
            new_associations: vec![],
            strengthened_associations: vec![],
            admitted_goal_id: None,
            declined_goal_candidate_id: None,
            state_files_written: vec![],
            ..full_record()
        };

        let view = render_change_view(&record);

        assert!(view.contains("Memories added (0):"));
        assert!(view.contains("(none)"));
        assert!(view.contains("Goals:\n  (no changes)"));
    }

    #[test]
    fn already_consumed_states_that_nothing_changed() {
        let record = SleepChangeRecord {
            state_outcome: SleepStateOutcome::AlreadyConsumed,
            ..full_record()
        };

        let view = render_change_view(&record);

        assert!(view.contains("already consumed"));
        assert!(view.contains("state unchanged"));
    }

    #[test]
    fn no_persisted_session_states_smoke_input() {
        let record = SleepChangeRecord {
            state_outcome: SleepStateOutcome::NoPersistedSession,
            session_id: None,
            ..full_record()
        };

        let view = render_change_view(&record);

        assert!(view.contains("No persisted session"));
        assert!(view.contains("state unchanged"));
    }
}
