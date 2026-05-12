use anyhow::{Context, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SleepInputBundle {
    pub source_kind: String,
    pub source_label: String,
    pub session_text: String,
    pub review_notes: Vec<String>,
}

impl SleepInputBundle {
    pub fn new(
        source_kind: impl Into<String>,
        source_label: impl Into<String>,
        session_text: impl Into<String>,
    ) -> Self {
        Self {
            source_kind: source_kind.into(),
            source_label: source_label.into(),
            session_text: session_text.into(),
            review_notes: vec![],
        }
    }

    pub fn with_review_notes(mut self, review_notes: Vec<String>) -> Self {
        self.review_notes = review_notes;
        self
    }

    pub fn source_excerpt(&self, max_chars: usize) -> String {
        summarize_text(&self.session_text, max_chars)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SleepMemoryCandidate {
    pub summary: String,
    pub importance: Option<f64>,
    pub source_reference: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SleepReport {
    pub session_summary: String,
    pub memory_candidates: Vec<SleepMemoryCandidate>,
    pub open_questions: Vec<String>,
    pub decision_candidates: Vec<String>,
    pub future_context_hints: Vec<String>,
    pub review_notes: Vec<String>,
}

impl SleepReport {
    pub fn counts_summary(&self) -> String {
        format!(
            "memory_candidates={} open_questions={} decision_candidates={} future_context_hints={}",
            self.memory_candidates.len(),
            self.open_questions.len(),
            self.decision_candidates.len(),
            self.future_context_hints.len()
        )
    }
}

pub fn parse_sleep_report(value: &Value) -> anyhow::Result<SleepReport> {
    Ok(SleepReport {
        session_summary: required_string(value, "session_summary")?,
        memory_candidates: parse_memory_candidates(value, "memory_candidates")?,
        open_questions: parse_summary_list(value, "open_questions")?,
        decision_candidates: parse_summary_list(value, "decision_candidates")?,
        future_context_hints: parse_summary_list(value, "future_context_hints")?,
        review_notes: parse_summary_list(value, "review_notes")?,
    })
}

fn parse_memory_candidates(
    value: &Value,
    field_name: &'static str,
) -> anyhow::Result<Vec<SleepMemoryCandidate>> {
    let entries = required_array(value, field_name)?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| match entry {
            Value::String(summary) => Ok(SleepMemoryCandidate {
                summary: summary.clone(),
                importance: None,
                source_reference: None,
            }),
            Value::Object(_) => Ok(SleepMemoryCandidate {
                summary: required_string(entry, "summary")?,
                importance: optional_probability(entry, "importance")?,
                source_reference: optional_string(entry, "source_reference"),
            }),
            _ => bail!(
                "expected `{field_name}[{index}]` to be a string or object, got {}",
                value_type_name(entry)
            ),
        })
        .collect()
}

fn parse_summary_list(value: &Value, field_name: &'static str) -> anyhow::Result<Vec<String>> {
    let entries = required_array(value, field_name)?;
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| match entry {
            Value::String(item) => Ok(item.clone()),
            Value::Object(_) => required_string(entry, "summary").with_context(|| {
                format!("expected `{field_name}[{index}]` object to contain `summary`")
            }),
            _ => bail!(
                "expected `{field_name}[{index}]` to be a string or summary object, got {}",
                value_type_name(entry)
            ),
        })
        .collect()
}

fn required_string(value: &Value, field_name: &'static str) -> anyhow::Result<String> {
    value
        .get(field_name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("missing or non-string required field `{field_name}`"))
}

fn optional_string(value: &Value, field_name: &'static str) -> Option<String> {
    value
        .get(field_name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn optional_probability(value: &Value, field_name: &'static str) -> anyhow::Result<Option<f64>> {
    match value.get(field_name).and_then(Value::as_f64) {
        Some(number) => Ok(Some(number.clamp(0.0, 1.0))),
        None if value.get(field_name).is_none() => Ok(None),
        None => Err(anyhow!("field `{field_name}` must be numeric when present")),
    }
}

fn required_array<'a>(value: &'a Value, field_name: &'static str) -> anyhow::Result<&'a [Value]> {
    value
        .get(field_name)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| anyhow!("missing or non-array required field `{field_name}`"))
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn summarize_text(text: &str, max_chars: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let head: String = text.chars().take(max_chars).collect();
    format!("{head}...")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{SleepInputBundle, parse_sleep_report};

    #[test]
    fn parse_sleep_report_accepts_structured_memory_candidates() {
        let report = parse_sleep_report(&json!({
            "session_summary": "Short summary.",
            "memory_candidates": [
                {
                    "summary": "Reducers stay pure.",
                    "importance": 1.4,
                    "source_reference": "session:1"
                }
            ],
            "open_questions": ["What should be consolidated?"],
            "decision_candidates": ["Keep review required."],
            "future_context_hints": ["Bring the summary next time."],
            "review_notes": ["Pending review."]
        }))
        .unwrap();

        assert_eq!(report.memory_candidates.len(), 1);
        assert_eq!(report.memory_candidates[0].importance, Some(1.0));
        assert_eq!(report.review_notes, vec!["Pending review."]);
    }

    #[test]
    fn parse_sleep_report_accepts_string_memory_candidates() {
        let report = parse_sleep_report(&json!({
            "session_summary": "Short summary.",
            "memory_candidates": ["Reducers stay pure."],
            "open_questions": [],
            "decision_candidates": [],
            "future_context_hints": [],
            "review_notes": []
        }))
        .unwrap();

        assert_eq!(report.memory_candidates[0].summary, "Reducers stay pure.");
        assert_eq!(report.memory_candidates[0].importance, None);
    }

    #[test]
    fn input_bundle_provides_source_excerpt() {
        let bundle = SleepInputBundle::new(
            "session_transcript",
            "phase-7-test",
            "This is a deliberately long excerpt that should be shortened for traces.",
        );

        assert!(bundle.source_excerpt(24).ends_with("..."));
    }
}
