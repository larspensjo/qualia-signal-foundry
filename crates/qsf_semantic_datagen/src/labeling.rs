use std::collections::BTreeSet;

use serde::Deserialize;

use crate::{
    CompletionRequest, LabelInterchange, LabelingInput, ModelTransport, PerGoalLabel, TokenUsage,
};

pub const GOAL_RELEVANCE_GUIDELINE_VERSION: &str = "goalrel-label-v1";
pub const MINI_LABELER_ID: &str = "gpt-5.4-mini";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LabelingResponseContext<'a> {
    pub input: &'a LabelingInput,
    pub labeler_id: &'a str,
    pub labeling_run_id: &'a str,
    pub guideline_version: &'a str,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ModelLabelResponse {
    per_goal: Vec<PerGoalLabel>,
    none_of_roster: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MiniLabelingRun {
    pub labels: Vec<LabelInterchange>,
    pub usage: Option<TokenUsage>,
}

/// Builds the blind label prompt from the deliberately limited labeling interchange only.
pub fn build_goal_relevance_label_prompt(input: &LabelingInput) -> Result<String, String> {
    validate_labeling_input(input)?;
    let roster = input
        .roster
        .iter()
        .map(|goal| {
            format!(
                "- goal_ref: {}\n  title: {}\n  summary: {}\n  tensions: {}",
                goal.goal_ref,
                goal.title,
                goal.summary,
                goal.tension_summaries.join(" | ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "You annotate goal_relevance using guideline {}.\n\
         Label the utterance against every roster goal. Relevant means the utterance bears on a\n\
         goal's tension space, including opposition or countering it; negating a topic does not\n\
         itself make it not relevant. Use not_relevant only when it is genuinely not about that\n\
         goal. Use ambiguous only when the wording lacks enough context for a reliable binary\n\
         judgment. Set none_of_roster true only if no roster goal is relevant; then no per-goal\n\
         label may be relevant.\n\n\
         Utterance ({}):\n{}\n\n\
         Frozen roster:\n{}\n\n\
         Return exactly one JSON object with no markdown:\n\
         {{\"per_goal\":[{{\"goal_ref\":\"...\",\"label\":\"relevant|not_relevant|ambiguous\"}}],\"none_of_roster\":false}}\n\
         Include every supplied goal exactly once.",
        GOAL_RELEVANCE_GUIDELINE_VERSION, input.utterance_id, input.utterance, roster
    ))
}

pub fn parse_goal_relevance_label_response(
    response: &str,
    context: LabelingResponseContext<'_>,
) -> Result<LabelInterchange, String> {
    validate_labeling_input(context.input)?;
    let response: ModelLabelResponse = serde_json::from_str(response.trim())
        .map_err(|error| format!("invalid goal-relevance label response: {error}"))?;
    let label = LabelInterchange {
        interchange_version: crate::INTERCHANGE_VERSION,
        labeler_id: context.labeler_id.to_string(),
        labeling_run_id: context.labeling_run_id.to_string(),
        guideline_version: context.guideline_version.to_string(),
        utterance_id: context.input.utterance_id.clone(),
        per_goal: response.per_goal,
        none_of_roster: response.none_of_roster,
    };
    validate_label_for_input(&label, context.input)?;
    Ok(label)
}

pub fn run_mini_labeling<T: ModelTransport>(
    transport: &T,
    input: &[LabelingInput],
    labeling_run_id: &str,
) -> Result<MiniLabelingRun, String> {
    let mut labels = Vec::with_capacity(input.len());
    let mut usage = TokenUsage {
        input_tokens: 0,
        cached_input_tokens: 0,
        output_tokens: 0,
    };
    let mut has_usage = false;
    for record in input {
        let prompt = build_goal_relevance_label_prompt(record)?;
        let response = transport.complete(&CompletionRequest {
            model_id: MINI_LABELER_ID.to_string(),
            prompt,
            goal_ref: "blind-full-roster".to_string(),
            run_id: labeling_run_id.to_string(),
            utterance_id: Some(record.utterance_id.clone()),
        })?;
        if let Some(call_usage) = response.usage {
            usage.add(call_usage);
            has_usage = true;
        }
        labels.push(parse_goal_relevance_label_response(
            &response.content,
            LabelingResponseContext {
                input: record,
                labeler_id: MINI_LABELER_ID,
                labeling_run_id,
                guideline_version: GOAL_RELEVANCE_GUIDELINE_VERSION,
            },
        )?);
    }
    Ok(MiniLabelingRun {
        labels,
        usage: has_usage.then_some(usage),
    })
}

fn validate_labeling_input(input: &LabelingInput) -> Result<(), String> {
    if input.interchange_version != crate::INTERCHANGE_VERSION {
        return Err(format!(
            "unsupported labeling input interchange_version {}",
            input.interchange_version
        ));
    }
    if input.utterance_id.trim().is_empty() || input.utterance.trim().is_empty() {
        return Err("labeling input has an empty utterance identifier or text".to_string());
    }
    let mut refs = BTreeSet::new();
    if input.roster.is_empty()
        || input
            .roster
            .iter()
            .any(|goal| goal.goal_ref.trim().is_empty() || !refs.insert(&goal.goal_ref))
    {
        return Err("labeling input roster has missing or duplicate goal_ref values".to_string());
    }
    Ok(())
}

fn validate_label_for_input(label: &LabelInterchange, input: &LabelingInput) -> Result<(), String> {
    let expected: BTreeSet<_> = input
        .roster
        .iter()
        .map(|goal| goal.goal_ref.as_str())
        .collect();
    let actual: BTreeSet<_> = label
        .per_goal
        .iter()
        .map(|pair| pair.goal_ref.as_str())
        .collect();
    if actual != expected || label.per_goal.len() != expected.len() {
        return Err(format!(
            "label response for utterance_id {} must cover every roster goal exactly once",
            input.utterance_id
        ));
    }
    if label.none_of_roster && label.per_goal.iter().any(|pair| pair.label.is_relevant()) {
        return Err(format!(
            "none_of_roster record {} has a relevant label",
            label.utterance_id
        ));
    }
    Ok(())
}
