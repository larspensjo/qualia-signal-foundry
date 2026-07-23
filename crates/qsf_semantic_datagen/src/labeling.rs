use std::collections::BTreeSet;

use qsf_semantic_eval::GoldLabel;
use serde::Deserialize;

use crate::{
    CompletionRequest, LabelInterchange, LabelingInput, ModelTransport, PerGoalLabel, TokenUsage,
};

pub const GOAL_RELEVANCE_GUIDELINE_VERSION: &str = "goalrel-label-v1";
pub const MINI_LABELER_ID: &str = "gpt-5.4-mini";
pub const MAX_LABELING_UTTERANCE_ATTEMPTS: usize = 5;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LabelingResponseContext<'a> {
    pub input: &'a LabelingInput,
    pub labeler_id: &'a str,
    pub labeling_run_id: &'a str,
    pub guideline_version: &'a str,
}

/// Model-facing response shape: goals are referenced by their 1-based prompt
/// number, never by echoed `goal_ref` hashes — models corrupt long hex strings
/// (dropped repeats), which made hash-echo protocols fail unrecoverably.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ModelLabelResponse {
    per_goal: Vec<ModelIndexedGoalLabel>,
    none_of_roster: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ModelIndexedGoalLabel {
    goal: usize,
    label: GoldLabel,
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
        .enumerate()
        .map(|(index, goal)| {
            format!(
                "- goal {}\n  title: {}\n  summary: {}\n  tensions: {}",
                index + 1,
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
         goal. A standing goal that any turn could in principle feed (keeping a library,\n\
         assembling a world picture, learning the person) is relevant only when the utterance\n\
         offers or solicits specific content for its tension space, not merely because the goal\n\
         could operate on it. Use ambiguous only when the wording lacks enough context for a\n\
         reliable binary judgment. Set none_of_roster true only if no roster goal is relevant;\n\
         then no per-goal label may be relevant.\n\n\
         Utterance ({}):\n{}\n\n\
         Frozen roster:\n{}\n\n\
         Return exactly one JSON object with no markdown:\n\
         {{\"per_goal\":[{{\"goal\":1,\"label\":\"relevant|not_relevant|ambiguous\"}}],\"none_of_roster\":false}}\n\
         Reference goals by their number; include every goal number from 1 to {} exactly once.",
        GOAL_RELEVANCE_GUIDELINE_VERSION,
        input.utterance_id,
        input.utterance,
        roster,
        input.roster.len()
    ))
}

pub fn parse_goal_relevance_label_response(
    response: &str,
    context: LabelingResponseContext<'_>,
) -> Result<LabelInterchange, String> {
    validate_labeling_input(context.input)?;
    let response: ModelLabelResponse = serde_json::from_str(response.trim())
        .map_err(|error| format!("invalid goal-relevance label response: {error}"))?;
    let roster_len = context.input.roster.len();
    let mut seen = BTreeSet::new();
    let mut per_goal = Vec::with_capacity(roster_len);
    for entry in response.per_goal {
        if entry.goal < 1 || entry.goal > roster_len {
            return Err(format!(
                "label response for utterance_id {} references goal number {} outside 1..={}",
                context.input.utterance_id, entry.goal, roster_len
            ));
        }
        if !seen.insert(entry.goal) {
            return Err(format!(
                "label response for utterance_id {} repeats goal number {}",
                context.input.utterance_id, entry.goal
            ));
        }
        per_goal.push(PerGoalLabel {
            goal_ref: context.input.roster[entry.goal - 1].goal_ref.clone(),
            label: entry.label,
        });
    }
    let label = LabelInterchange {
        interchange_version: crate::INTERCHANGE_VERSION,
        labeler_id: context.labeler_id.to_string(),
        labeling_run_id: context.labeling_run_id.to_string(),
        guideline_version: context.guideline_version.to_string(),
        utterance_id: context.input.utterance_id.clone(),
        per_goal,
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
        let mut attempt = 1usize;
        let label = loop {
            let response = transport.complete(&CompletionRequest {
                model_id: MINI_LABELER_ID.to_string(),
                prompt: prompt.clone(),
                goal_ref: "blind-full-roster".to_string(),
                run_id: labeling_run_id.to_string(),
                utterance_id: Some(record.utterance_id.clone()),
            })?;
            if let Some(call_usage) = response.usage {
                usage.add(call_usage);
                has_usage = true;
            }
            match parse_goal_relevance_label_response(
                &response.content,
                LabelingResponseContext {
                    input: record,
                    labeler_id: MINI_LABELER_ID,
                    labeling_run_id,
                    guideline_version: GOAL_RELEVANCE_GUIDELINE_VERSION,
                },
            ) {
                Ok(label) => break label,
                Err(error) if attempt < MAX_LABELING_UTTERANCE_ATTEMPTS => {
                    engine_logging::engine_warn!(
                        "goal-relevance labeling rejected a response; retrying: run_id={} utterance_id={} model_id={} attempt={}/{} error={} response_snippet={}",
                        labeling_run_id,
                        record.utterance_id,
                        MINI_LABELER_ID,
                        attempt,
                        MAX_LABELING_UTTERANCE_ATTEMPTS,
                        error,
                        response_snippet(&response.content),
                    );
                    attempt += 1;
                }
                Err(error) => {
                    engine_logging::engine_error!(
                        "goal-relevance labeling failed after {} attempts: run_id={} utterance_id={} model_id={} error={} response_snippet={}",
                        MAX_LABELING_UTTERANCE_ATTEMPTS,
                        labeling_run_id,
                        record.utterance_id,
                        MINI_LABELER_ID,
                        error,
                        response_snippet(&response.content),
                    );
                    return Err(error);
                }
            }
        };
        labels.push(label);
    }
    Ok(MiniLabelingRun {
        labels,
        usage: has_usage.then_some(usage),
    })
}

/// Truncated single-line view of a rejected model response, for retry diagnostics.
fn response_snippet(content: &str) -> String {
    let single_line = content.replace(['\r', '\n'], " ");
    let mut snippet: String = single_line.chars().take(240).collect();
    if single_line.chars().count() > 240 {
        snippet.push('…');
    }
    snippet
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
