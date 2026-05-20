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
pub struct SleepAssociationCandidate {
    pub from_memory_candidate_index: usize,
    pub to_memory_candidate_index: usize,
    pub weight: Option<f64>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SleepReport {
    pub session_summary: String,
    pub memory_candidates: Vec<SleepMemoryCandidate>,
    pub association_candidates: Vec<SleepAssociationCandidate>,
    pub open_questions: Vec<String>,
    pub decision_candidates: Vec<String>,
    pub future_context_hints: Vec<String>,
    pub review_notes: Vec<String>,
}

impl SleepReport {
    pub fn counts_summary(&self) -> String {
        format!(
            "memory_candidates={} association_candidates={} open_questions={} decision_candidates={} future_context_hints={}",
            self.memory_candidates.len(),
            self.association_candidates.len(),
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
        association_candidates: parse_association_candidates(value, "association_candidates")?,
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

fn parse_association_candidates(
    value: &Value,
    field_name: &'static str,
) -> anyhow::Result<Vec<SleepAssociationCandidate>> {
    let Some(entries) = optional_array(value, field_name)? else {
        return Ok(vec![]);
    };
    let zero_based_indexes = association_candidates_use_zero_based_indexes(entries, field_name)?;

    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| match entry {
            Value::Object(_) => Ok(SleepAssociationCandidate {
                from_memory_candidate_index: required_association_index(
                    entry,
                    "from_memory_candidate_index",
                    zero_based_indexes,
                )
                .with_context(|| {
                    format!("expected `{field_name}[{index}]` to contain a 1-based source index")
                })?,
                to_memory_candidate_index: required_association_index(
                    entry,
                    "to_memory_candidate_index",
                    zero_based_indexes,
                )
                .with_context(|| {
                    format!("expected `{field_name}[{index}]` to contain a 1-based target index")
                })?,
                weight: optional_probability(entry, "weight")?,
                reason: optional_string(entry, "reason"),
            }),
            _ => bail!(
                "expected `{field_name}[{index}]` to be an object, got {}",
                value_type_name(entry)
            ),
        })
        .collect()
}

fn association_candidates_use_zero_based_indexes(
    entries: &[Value],
    field_name: &'static str,
) -> anyhow::Result<bool> {
    let mut zero_based = false;
    for (index, entry) in entries.iter().enumerate() {
        if !entry.is_object() {
            continue;
        }

        let from_index = required_nonnegative_usize(entry, "from_memory_candidate_index")
            .with_context(|| {
                format!("expected `{field_name}[{index}]` to contain a source index")
            })?;
        let to_index = required_nonnegative_usize(entry, "to_memory_candidate_index")
            .with_context(|| {
                format!("expected `{field_name}[{index}]` to contain a target index")
            })?;
        zero_based = zero_based || from_index == 0 || to_index == 0;
    }

    Ok(zero_based)
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

fn required_association_index(
    value: &Value,
    field_name: &'static str,
    zero_based_indexes: bool,
) -> anyhow::Result<usize> {
    let number = required_nonnegative_usize(value, field_name)?;
    if zero_based_indexes {
        return number
            .checked_add(1)
            .ok_or_else(|| anyhow!("field `{field_name}` is too large"));
    }
    if number == 0 {
        bail!("field `{field_name}` must be 1 or greater");
    }

    Ok(number)
}

fn required_nonnegative_usize(value: &Value, field_name: &'static str) -> anyhow::Result<usize> {
    let number = value
        .get(field_name)
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("missing or non-integer required field `{field_name}`"))?;

    usize::try_from(number).with_context(|| format!("field `{field_name}` is too large"))
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

fn optional_array<'a>(
    value: &'a Value,
    field_name: &'static str,
) -> anyhow::Result<Option<&'a [Value]>> {
    match value.get(field_name) {
        Some(Value::Array(entries)) => Ok(Some(entries.as_slice())),
        Some(other) => bail!(
            "expected optional `{field_name}` to be an array when present, got {}",
            value_type_name(other)
        ),
        None => Ok(None),
    }
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
        assert!(report.association_candidates.is_empty());
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
    fn parse_sleep_report_accepts_optional_association_candidates() {
        let report = parse_sleep_report(&json!({
            "session_summary": "Short summary.",
            "memory_candidates": ["Reducers stay pure.", "Events feed state."],
            "association_candidates": [
                {
                    "from_memory_candidate_index": 1,
                    "to_memory_candidate_index": 2,
                    "weight": 1.4,
                    "reason": "Both describe runtime flow."
                }
            ],
            "open_questions": [],
            "decision_candidates": [],
            "future_context_hints": [],
            "review_notes": []
        }))
        .unwrap();

        assert_eq!(report.association_candidates.len(), 1);
        assert_eq!(
            report.association_candidates[0].from_memory_candidate_index,
            1
        );
        assert_eq!(
            report.association_candidates[0].to_memory_candidate_index,
            2
        );
        assert_eq!(report.association_candidates[0].weight, Some(1.0));
        assert_eq!(
            report.association_candidates[0].reason.as_deref(),
            Some("Both describe runtime flow.")
        );
    }

    #[test]
    fn parse_sleep_report_normalizes_zero_based_association_candidates() {
        let report = parse_sleep_report(&json!({
            "session_summary": "Short summary.",
            "memory_candidates": [
                "Typed model roles.",
                "Deterministic mock model.",
                "Explicit event traces.",
                "Reviewable sleep output."
            ],
            "association_candidates": [
                {
                    "from_memory_candidate_index": 0,
                    "to_memory_candidate_index": 1,
                    "weight": 0.88,
                    "reason": "Both were introduced together."
                },
                {
                    "from_memory_candidate_index": 2,
                    "to_memory_candidate_index": 3,
                    "weight": 0.79,
                    "reason": "Traceability supports reviewable sleep output."
                }
            ],
            "open_questions": [],
            "decision_candidates": [],
            "future_context_hints": [],
            "review_notes": []
        }))
        .unwrap();

        assert_eq!(report.association_candidates.len(), 2);
        assert_eq!(
            report.association_candidates[0].from_memory_candidate_index,
            1
        );
        assert_eq!(
            report.association_candidates[0].to_memory_candidate_index,
            2
        );
        assert_eq!(
            report.association_candidates[1].from_memory_candidate_index,
            3
        );
        assert_eq!(
            report.association_candidates[1].to_memory_candidate_index,
            4
        );
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
