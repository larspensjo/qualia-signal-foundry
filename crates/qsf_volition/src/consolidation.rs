use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{ReviewedVolitionSeed, VolitionSuppressionReason, VolitionTurnOutcome};

pub const VOLITION_CONSOLIDATION_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VolitionConsolidationPatternKind {
    RecurringSelected,
    OftenBlocked,
    AcceptedOrRejectedCandidate,
    ModeChange,
    UnactedInitiative,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VolitionPromotionStatus {
    Proposed,
    HumanPromoted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VolitionCandidateDecision {
    Accepted,
    Rejected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct VolitionTickRange {
    pub first_tick: u64,
    pub last_tick: u64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VolitionConsolidationItem {
    pub kind: VolitionConsolidationPatternKind,
    pub goal_or_candidate_id: String,
    pub count: usize,
    pub tick_range: Option<VolitionTickRange>,
    pub artifact_reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_status: Option<VolitionPromotionStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_decision: Option<VolitionCandidateDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppression_reason: Option<VolitionSuppressionReason>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VolitionConsolidationReport {
    pub schema_version: u16,
    pub qsf_session_id: String,
    pub seed_fixture_id: String,
    pub items: Vec<VolitionConsolidationItem>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct VolitionConsolidationSnapshotRecord {
    pub artifact_reference: String,
    pub snapshot: crate::VolitionContinuitySnapshot,
}

pub fn build_volition_consolidation_report(
    qsf_session_id: &str,
    seed_fixture_id: &str,
    snapshots: &[VolitionConsolidationSnapshotRecord],
    initiative_outcomes: &[VolitionTurnOutcome],
    reviewed_seed: Option<&ReviewedVolitionSeed>,
) -> VolitionConsolidationReport {
    let reviewed_goal_ids = reviewed_seed
        .map(|seed| seed.accepted_goals.keys().cloned().collect::<BTreeSet<_>>())
        .unwrap_or_default();

    let mut items = Vec::new();
    items.extend(recurring_selected_items(snapshots));
    items.extend(often_blocked_items(snapshots));
    items.extend(candidate_transition_items(snapshots, &reviewed_goal_ids));
    items.extend(mode_change_items(snapshots));
    items.extend(unacted_initiative_items(initiative_outcomes));
    items.sort_by(|left, right| {
        (
            left.kind,
            left.goal_or_candidate_id.as_str(),
            left.artifact_reference.as_str(),
            left.count,
        )
            .cmp(&(
                right.kind,
                right.goal_or_candidate_id.as_str(),
                right.artifact_reference.as_str(),
                right.count,
            ))
    });

    VolitionConsolidationReport {
        schema_version: VOLITION_CONSOLIDATION_SCHEMA_VERSION,
        qsf_session_id: qsf_session_id.to_string(),
        seed_fixture_id: seed_fixture_id.to_string(),
        items,
    }
}

fn recurring_selected_items(
    snapshots: &[VolitionConsolidationSnapshotRecord],
) -> Vec<VolitionConsolidationItem> {
    let mut counts: BTreeMap<String, GoalAccumulator> = BTreeMap::new();
    for record in snapshots {
        for summary in &record.snapshot.inspection.last_initiative_summaries {
            counts
                .entry(summary.goal_id.clone())
                .or_insert_with(|| {
                    GoalAccumulator::new(
                        VolitionConsolidationPatternKind::RecurringSelected,
                        &record.artifact_reference,
                    )
                })
                .record(record.snapshot.inspection.tick);
        }
    }
    counts
        .into_iter()
        .filter_map(|(goal_id, accumulator)| accumulator.into_item(goal_id, 2))
        .collect()
}

fn often_blocked_items(
    snapshots: &[VolitionConsolidationSnapshotRecord],
) -> Vec<VolitionConsolidationItem> {
    let mut counts: BTreeMap<String, GoalAccumulator> = BTreeMap::new();
    for record in snapshots {
        for summary in &record.snapshot.inspection.blocked_goals {
            counts
                .entry(summary.id.clone())
                .or_insert_with(|| {
                    GoalAccumulator::new(
                        VolitionConsolidationPatternKind::OftenBlocked,
                        &record.artifact_reference,
                    )
                })
                .record(record.snapshot.inspection.tick);
        }
    }
    counts
        .into_iter()
        .filter_map(|(goal_id, accumulator)| accumulator.into_item(goal_id, 2))
        .collect()
}

fn candidate_transition_items(
    snapshots: &[VolitionConsolidationSnapshotRecord],
    reviewed_goal_ids: &BTreeSet<String>,
) -> Vec<VolitionConsolidationItem> {
    let mut accepted: BTreeMap<String, GoalAccumulator> = BTreeMap::new();
    let mut rejected: BTreeMap<String, GoalAccumulator> = BTreeMap::new();

    for window in snapshots.windows(2) {
        let current = &window[0];
        let next = &window[1];
        let current_pending: BTreeMap<_, _> = current
            .snapshot
            .state
            .pending_candidates
            .iter()
            .map(|candidate| (candidate.id().to_string(), candidate))
            .collect();
        let next_pending: BTreeMap<_, _> = next
            .snapshot
            .state
            .pending_candidates
            .iter()
            .map(|candidate| (candidate.id().to_string(), candidate))
            .collect();
        let next_accepted: BTreeMap<_, _> = next
            .snapshot
            .state
            .accepted_candidates
            .iter()
            .map(|(id, goal)| (id.clone(), goal))
            .collect();

        for candidate_id in current_pending.keys() {
            if next_accepted.contains_key(candidate_id) {
                accepted
                    .entry(candidate_id.clone())
                    .or_insert_with(|| {
                        GoalAccumulator::new(
                            VolitionConsolidationPatternKind::AcceptedOrRejectedCandidate,
                            &next.artifact_reference,
                        )
                    })
                    .record(next.snapshot.inspection.tick);
            } else if !next_pending.contains_key(candidate_id) {
                rejected
                    .entry(candidate_id.clone())
                    .or_insert_with(|| {
                        GoalAccumulator::new(
                            VolitionConsolidationPatternKind::AcceptedOrRejectedCandidate,
                            &next.artifact_reference,
                        )
                    })
                    .record(next.snapshot.inspection.tick);
            }
        }
    }

    if let Some(first) = snapshots.first() {
        for goal_id in first.snapshot.state.accepted_candidates.keys() {
            accepted
                .entry(goal_id.clone())
                .or_insert_with(|| {
                    GoalAccumulator::new(
                        VolitionConsolidationPatternKind::AcceptedOrRejectedCandidate,
                        &first.artifact_reference,
                    )
                })
                .record(first.snapshot.inspection.tick);
        }
    }

    let mut items = Vec::new();
    for (goal_id, accumulator) in accepted {
        let human_promoted = reviewed_goal_ids.contains(&goal_id);
        if let Some(item) = accumulator.into_transition_item(
            goal_id,
            VolitionCandidateDecision::Accepted,
            human_promoted,
        ) {
            items.push(item);
        }
    }
    for (goal_id, accumulator) in rejected {
        if let Some(item) =
            accumulator.into_transition_item(goal_id, VolitionCandidateDecision::Rejected, false)
        {
            items.push(item);
        }
    }
    items
}

fn mode_change_items(
    snapshots: &[VolitionConsolidationSnapshotRecord],
) -> Vec<VolitionConsolidationItem> {
    let mut counts: BTreeMap<String, GoalAccumulator> = BTreeMap::new();
    for window in snapshots.windows(2) {
        let current = &window[0];
        let next = &window[1];
        if current.snapshot.state.mode != next.snapshot.state.mode {
            let key = format!(
                "mode:{}->{}",
                current.snapshot.state.mode.to_string().to_lowercase(),
                next.snapshot.state.mode.to_string().to_lowercase()
            );
            counts
                .entry(key)
                .or_insert_with(|| {
                    GoalAccumulator::new(
                        VolitionConsolidationPatternKind::ModeChange,
                        &next.artifact_reference,
                    )
                })
                .record(next.snapshot.inspection.tick);
        }
    }

    counts
        .into_iter()
        .filter_map(|(goal_id, accumulator)| accumulator.into_item(goal_id, 1))
        .collect()
}

fn unacted_initiative_items(
    initiative_outcomes: &[VolitionTurnOutcome],
) -> Vec<VolitionConsolidationItem> {
    let mut counts: BTreeMap<(String, Option<VolitionSuppressionReason>), GoalAccumulator> =
        BTreeMap::new();

    for outcome in initiative_outcomes {
        if outcome.surfaced {
            continue;
        }

        let reason = outcome
            .suppression_reason
            .or(Some(VolitionSuppressionReason::NonRenderableOutput));
        counts
            .entry((outcome.winning_goal_id.clone(), reason))
            .or_insert_with(|| {
                GoalAccumulator::new(
                    VolitionConsolidationPatternKind::UnactedInitiative,
                    &outcome.artifact_reference,
                )
            })
            .record(outcome.exchange_index as u64);
    }

    counts
        .into_iter()
        .filter_map(|((goal_id, reason), accumulator)| {
            accumulator.into_initiative_item(goal_id, reason)
        })
        .collect()
}

#[derive(Clone, Debug)]
struct GoalAccumulator {
    kind: VolitionConsolidationPatternKind,
    artifact_reference: String,
    count: usize,
    first_tick: Option<u64>,
    last_tick: Option<u64>,
}

impl GoalAccumulator {
    fn new(kind: VolitionConsolidationPatternKind, artifact_reference: &str) -> Self {
        Self {
            kind,
            artifact_reference: artifact_reference.to_string(),
            count: 0,
            first_tick: None,
            last_tick: None,
        }
    }

    fn record(&mut self, tick: u64) {
        self.count += 1;
        self.first_tick = Some(self.first_tick.map_or(tick, |current| current.min(tick)));
        self.last_tick = Some(self.last_tick.map_or(tick, |current| current.max(tick)));
    }

    fn into_item(
        self,
        goal_or_candidate_id: String,
        minimum_count: usize,
    ) -> Option<VolitionConsolidationItem> {
        if self.count < minimum_count {
            return None;
        }

        Some(VolitionConsolidationItem {
            kind: self.kind,
            goal_or_candidate_id,
            count: self.count,
            tick_range: match (self.first_tick, self.last_tick) {
                (Some(first_tick), Some(last_tick)) => Some(VolitionTickRange {
                    first_tick,
                    last_tick,
                }),
                _ => None,
            },
            artifact_reference: self.artifact_reference,
            promotion_status: None,
            candidate_decision: None,
            suppression_reason: None,
        })
    }

    fn into_transition_item(
        self,
        goal_or_candidate_id: String,
        decision: VolitionCandidateDecision,
        human_promoted: bool,
    ) -> Option<VolitionConsolidationItem> {
        if self.count == 0 {
            return None;
        }

        Some(VolitionConsolidationItem {
            kind: self.kind,
            goal_or_candidate_id,
            count: self.count,
            tick_range: match (self.first_tick, self.last_tick) {
                (Some(first_tick), Some(last_tick)) => Some(VolitionTickRange {
                    first_tick,
                    last_tick,
                }),
                _ => None,
            },
            artifact_reference: self.artifact_reference,
            promotion_status: decision.eq(&VolitionCandidateDecision::Accepted).then_some(
                if human_promoted {
                    VolitionPromotionStatus::HumanPromoted
                } else {
                    VolitionPromotionStatus::Proposed
                },
            ),
            candidate_decision: Some(decision),
            suppression_reason: None,
        })
    }

    fn into_initiative_item(
        self,
        goal_or_candidate_id: String,
        suppression_reason: Option<VolitionSuppressionReason>,
    ) -> Option<VolitionConsolidationItem> {
        if self.count == 0 {
            return None;
        }

        Some(VolitionConsolidationItem {
            kind: self.kind,
            goal_or_candidate_id,
            count: self.count,
            tick_range: None,
            artifact_reference: self.artifact_reference,
            promotion_status: None,
            candidate_decision: None,
            suppression_reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActivationKeyword, AllowedEffect, EvidenceRef, Goal, GoalScope, GoalStatus,
        InitiativeOutput, Mode, ProposedGoalCandidate, VolitionContinuitySnapshot, VolitionFixture,
        VolitionState, build_state_inspection, realtime_seed_fixture,
    };

    fn snapshot_record(
        fixture: &VolitionFixture,
        state: VolitionState,
        artifact_reference: &str,
    ) -> VolitionConsolidationSnapshotRecord {
        let inspection = build_state_inspection(&state, fixture);
        VolitionConsolidationSnapshotRecord {
            artifact_reference: artifact_reference.to_string(),
            snapshot: VolitionContinuitySnapshot::new(
                "session-1",
                "2026-06-30T12:00:00Z",
                crate::REALTIME_SEED_FIXTURE_ID,
                state,
                inspection,
            ),
        }
    }

    #[test]
    fn consolidation_report_separates_recurring_selected_and_unacted_initiatives() {
        let fixture = realtime_seed_fixture();
        let state = VolitionState::from_fixture(&fixture);
        let first = snapshot_record(&fixture, state.clone(), "snapshot:1");
        let mut next_state = state;
        next_state.mode = Mode::Focused;
        let second = snapshot_record(&fixture, next_state, "snapshot:2");
        let outcome = VolitionTurnOutcome {
            qsf_session_id: "session-1".to_string(),
            exchange_index: 1,
            recorded_at: "2026-06-30T12:00:00Z".to_string(),
            response_create_event_ref: "ref-1".to_string(),
            winning_goal_id: "serve-the-present-person".to_string(),
            initiative_output: InitiativeOutput::ContextRetrievalRequested {
                query_terms: vec!["thread".to_string()],
            },
            surfaced: false,
            suppression_reason: Some(VolitionSuppressionReason::NonRenderableOutput),
            rendered_line_present: false,
            artifact_reference: "diagnostic:1".to_string(),
        };

        let report = build_volition_consolidation_report(
            "session-1",
            crate::REALTIME_SEED_FIXTURE_ID,
            &[first, second],
            &[outcome],
            None,
        );

        assert_eq!(report.schema_version, VOLITION_CONSOLIDATION_SCHEMA_VERSION);
        assert!(report.items.iter().any(|item| {
            item.kind == VolitionConsolidationPatternKind::ModeChange
                && item.goal_or_candidate_id == "mode:neutral->focused"
        }));
        assert!(report.items.iter().any(|item| {
            item.kind == VolitionConsolidationPatternKind::UnactedInitiative
                && item.goal_or_candidate_id == "serve-the-present-person"
                && item.suppression_reason == Some(VolitionSuppressionReason::NonRenderableOutput)
        }));
    }

    #[test]
    fn accepted_candidate_items_mark_reviewed_seed_promotions() {
        let fixture = realtime_seed_fixture();
        let reviewed_goal = Goal {
            id: "reviewed-continuity-check".to_string(),
            title: "Reviewed continuity check".to_string(),
            summary: "Reviewed".to_string(),
            tension_ids: vec!["coherence-maintenance".to_string()],
            status: GoalStatus::Accepted,
            scope: GoalScope::Session,
            base_priority: 75,
            activation_keywords: vec![ActivationKeyword::normal("continuity")],
            allowed_effects: vec![AllowedEffect::Reflect],
            satisfaction_condition_summary: "done".to_string(),
            evidence_refs: vec!["tests".to_string()],
            estimated_tokens: 10,
            source_reference: "tests".to_string(),
        };
        let reviewed_seed = ReviewedVolitionSeed {
            schema_version: crate::REVIEWED_VOLITION_SEED_SCHEMA_VERSION,
            promoted_by: "human".to_string(),
            promoted_at: "2026-06-30T12:00:00Z".to_string(),
            source_artifact_reference: "seed-review.json".to_string(),
            accepted_goals: BTreeMap::from([(reviewed_goal.id.clone(), reviewed_goal)]),
        };
        let mut state = VolitionState::from_fixture(&fixture);
        state.pending_candidates.push(
            ProposedGoalCandidate::try_new(
                "reviewed-continuity-check".to_string(),
                "Reviewed continuity check".to_string(),
                "Reviewed".to_string(),
                vec!["coherence-maintenance".to_string()],
                GoalScope::Session,
                75,
                vec![AllowedEffect::Reflect],
                "done".to_string(),
                vec![EvidenceRef::try_new("tests").unwrap()],
                "tests".to_string(),
                vec![],
            )
            .unwrap(),
        );
        let pending = snapshot_record(&fixture, state.clone(), "snapshot:1");
        let mut accepted_state = state;
        accepted_state.pending_candidates.clear();
        accepted_state.accepted_candidates.insert(
            "reviewed-continuity-check".to_string(),
            reviewed_seed.accepted_goals["reviewed-continuity-check"].clone(),
        );
        let accepted = snapshot_record(&fixture, accepted_state, "snapshot:2");

        let report = build_volition_consolidation_report(
            "session-1",
            crate::REALTIME_SEED_FIXTURE_ID,
            &[pending, accepted],
            &[],
            Some(&reviewed_seed),
        );

        assert!(report.items.iter().any(|item| {
            item.kind == VolitionConsolidationPatternKind::AcceptedOrRejectedCandidate
                && item.goal_or_candidate_id == "reviewed-continuity-check"
                && item.promotion_status == Some(VolitionPromotionStatus::HumanPromoted)
                && item.candidate_decision == Some(VolitionCandidateDecision::Accepted)
        }));
    }
}
