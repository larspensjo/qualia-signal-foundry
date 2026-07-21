use std::collections::BTreeMap;

use qsf_semantic_eval::GoldLabel;

use crate::{
    LabelInterchange, LabelingInput, ReconciliationRecord, ReviewDecision, ReviewField, ReviewValue,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewQueueEntry {
    pub utterance_id: String,
    pub goal_ref: String,
    pub mini_label: GoldLabel,
    pub fable_label: GoldLabel,
    pub agree: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgreementRate {
    pub agreed_pairs: usize,
    pub total_pairs: usize,
    pub rate: f64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewGoalViewModel {
    pub goal_ref: String,
    pub title: String,
    pub label: GoldLabel,
    pub mini_label: GoldLabel,
    pub fable_label: GoldLabel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewViewModel {
    pub utterance_id: String,
    pub utterance: String,
    pub none_of_roster: bool,
    pub per_goal: Vec<ReviewGoalViewModel>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlindQaGoalViewModel {
    pub goal_ref: String,
    pub title: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlindQaReviewViewModel {
    pub utterance_id: String,
    pub utterance: String,
    pub per_goal: Vec<BlindQaGoalViewModel>,
    pub asks_for_none_of_roster: bool,
}

pub fn priority_review_queue(reconciliation: &[ReconciliationRecord]) -> Vec<ReviewQueueEntry> {
    let mut queue = reconciliation
        .iter()
        .map(|record| ReviewQueueEntry {
            utterance_id: record.utterance_id.clone(),
            goal_ref: record.goal_ref.clone(),
            mini_label: record.mini_label.clone(),
            fable_label: record.fable_label.clone(),
            agree: record.agree,
        })
        .collect::<Vec<_>>();
    queue.sort_by(|left, right| {
        left.agree
            .cmp(&right.agree)
            .then_with(|| left.utterance_id.cmp(&right.utterance_id))
            .then_with(|| left.goal_ref.cmp(&right.goal_ref))
    });
    queue
}

pub fn mini_fable_agreement_rate(reconciliation: &[ReconciliationRecord]) -> AgreementRate {
    let agreed_pairs = reconciliation.iter().filter(|record| record.agree).count();
    let total_pairs = reconciliation.len();
    AgreementRate {
        agreed_pairs,
        total_pairs,
        rate: if total_pairs == 0 {
            0.0
        } else {
            agreed_pairs as f64 / total_pairs as f64
        },
    }
}

pub fn build_review_view_model(
    utterance_id: &str,
    input: &[LabelingInput],
    mini: &[LabelInterchange],
    fable: &[LabelInterchange],
    decisions: &[ReviewDecision],
) -> Result<ReviewViewModel, String> {
    let (input, mini, fable) = lookup_utterance(utterance_id, input, mini, fable)?;
    let decision_labels = resolved_decision_labels(utterance_id, decisions);
    let none_of_roster = resolved_none_of_roster(utterance_id, mini.none_of_roster, decisions);
    let mini_by_goal: BTreeMap<_, _> = mini
        .per_goal
        .iter()
        .map(|pair| (pair.goal_ref.as_str(), &pair.label))
        .collect();
    let fable_by_goal: BTreeMap<_, _> = fable
        .per_goal
        .iter()
        .map(|pair| (pair.goal_ref.as_str(), &pair.label))
        .collect();
    let mut per_goal = Vec::with_capacity(input.roster.len());
    for goal in &input.roster {
        let mini_label = mini_by_goal
            .get(goal.goal_ref.as_str())
            .ok_or_else(|| format!("mini label misses goal_ref {}", goal.goal_ref))?
            .to_owned()
            .clone();
        let fable_label = fable_by_goal
            .get(goal.goal_ref.as_str())
            .ok_or_else(|| format!("Fable label misses goal_ref {}", goal.goal_ref))?
            .to_owned()
            .clone();
        let label = decision_labels
            .get(goal.goal_ref.as_str())
            .cloned()
            .unwrap_or_else(|| mini_label.clone());
        per_goal.push(ReviewGoalViewModel {
            goal_ref: goal.goal_ref.clone(),
            title: goal.title.clone(),
            label,
            mini_label,
            fable_label,
        });
    }
    Ok(ReviewViewModel {
        utterance_id: input.utterance_id.clone(),
        utterance: input.utterance.clone(),
        none_of_roster,
        per_goal,
    })
}

pub fn build_blind_qa_review_view_model(
    utterance_id: &str,
    input: &[LabelingInput],
) -> Result<BlindQaReviewViewModel, String> {
    let input = input
        .iter()
        .find(|record| record.utterance_id == utterance_id)
        .ok_or_else(|| format!("unknown utterance_id {utterance_id}"))?;
    Ok(BlindQaReviewViewModel {
        utterance_id: input.utterance_id.clone(),
        utterance: input.utterance.clone(),
        per_goal: input
            .roster
            .iter()
            .map(|goal| BlindQaGoalViewModel {
                goal_ref: goal.goal_ref.clone(),
                title: goal.title.clone(),
            })
            .collect(),
        asks_for_none_of_roster: true,
    })
}

pub fn render_review_view(view: &ReviewViewModel) -> String {
    let mut result = format!(
        "Utterance {}\n{}\nnone_of_roster: {}\n",
        view.utterance_id, view.utterance, view.none_of_roster
    );
    for (index, goal) in view.per_goal.iter().enumerate() {
        result.push_str(&format!(
            "{}. {}\n   label: {:?}; mini: {:?}; fable: {:?}\n",
            index + 1,
            goal.title,
            goal.label,
            goal.mini_label,
            goal.fable_label
        ));
    }
    result
}

pub fn render_blind_qa_review_view(view: &BlindQaReviewViewModel) -> String {
    let mut result = format!("Utterance {}\n{}\n", view.utterance_id, view.utterance);
    for (index, goal) in view.per_goal.iter().enumerate() {
        result.push_str(&format!("{}. {}\n", index + 1, goal.title));
    }
    if view.asks_for_none_of_roster {
        result.push_str("none_of_roster: [decide cold]\n");
    }
    result.push_str("Make a cold decision for every goal and the roster-wide annotation.\n");
    result
}

pub fn priority_review_utterance_ids(reconciliation: &[ReconciliationRecord]) -> Vec<String> {
    let mut ids = Vec::new();
    for entry in priority_review_queue(reconciliation) {
        if !ids.contains(&entry.utterance_id) {
            ids.push(entry.utterance_id);
        }
    }
    ids
}

fn lookup_utterance<'a>(
    utterance_id: &str,
    input: &'a [LabelingInput],
    mini: &'a [LabelInterchange],
    fable: &'a [LabelInterchange],
) -> Result<
    (
        &'a LabelingInput,
        &'a LabelInterchange,
        &'a LabelInterchange,
    ),
    String,
> {
    let input = input
        .iter()
        .find(|record| record.utterance_id == utterance_id)
        .ok_or_else(|| format!("unknown utterance_id {utterance_id}"))?;
    let mini = mini
        .iter()
        .find(|record| record.utterance_id == utterance_id)
        .ok_or_else(|| format!("mini labels miss utterance_id {utterance_id}"))?;
    let fable = fable
        .iter()
        .find(|record| record.utterance_id == utterance_id)
        .ok_or_else(|| format!("Fable labels miss utterance_id {utterance_id}"))?;
    Ok((input, mini, fable))
}

fn resolved_decision_labels<'a>(
    utterance_id: &str,
    decisions: &'a [ReviewDecision],
) -> BTreeMap<&'a str, GoldLabel> {
    let mut labels = BTreeMap::new();
    for decision in decisions {
        if decision.utterance_id == utterance_id
            && decision.field == ReviewField::GoldLabel
            && let (Some(goal_ref), ReviewValue::GoldLabel(label)) =
                (&decision.goal_ref, &decision.value)
        {
            labels.insert(goal_ref.as_str(), label.clone());
        }
    }
    labels
}

fn resolved_none_of_roster(utterance_id: &str, draft: bool, decisions: &[ReviewDecision]) -> bool {
    decisions.iter().fold(draft, |current, decision| {
        if decision.utterance_id == utterance_id
            && decision.field == ReviewField::NoneOfRoster
            && let ReviewValue::NoneOfRoster(value) = &decision.value
        {
            *value
        } else {
            current
        }
    })
}
