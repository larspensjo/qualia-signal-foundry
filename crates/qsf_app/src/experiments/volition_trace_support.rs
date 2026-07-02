//! Shared trace helpers for the volition coherence experiments (`volition_goal_coherence` and
//! `live_goal_formation_and_coherence`). Both build the same `goal-coherence-check` trace shape
//! from the same event/goal-status diffs, so the projection lives here once.

use qsf_models::CoherenceJudgeGoalRef;
use serde_json::json;
use uuid::Uuid;

use crate::observability::trace::TraceRecord;
use crate::runtime::run_context::RunContext;
use crate::volition::{VolitionEvent, VolitionFixture, VolitionState, goal_effective_tier};

/// Renders the evaluated goal set as `{ goal_id, effective_tier, status, last_activated_tick }`
/// records for a `goal_set_snapshot` trace field. Shared by both coherence experiments and the
/// sleep goal-maintenance pass so the snapshot shape has a single definition.
pub(crate) fn goal_set_snapshot_json(
    state: &VolitionState,
    fixture: &VolitionFixture,
    goal_set: &[CoherenceJudgeGoalRef],
) -> serde_json::Value {
    json!(
        goal_set
            .iter()
            .map(|goal_ref| {
                let dynamic = state.goals.get(&goal_ref.id);
                json!({
                    "goal_id": goal_ref.id,
                    "effective_tier": goal_effective_tier(&goal_ref.id, state, fixture),
                    "status": dynamic.map(|d| d.status),
                    "last_activated_tick": dynamic.and_then(|d| d.last_activated_tick),
                })
            })
            .collect::<Vec<_>>()
    )
}

/// The goal id a lifecycle-changing coherence event names, or `None` for events that do not
/// change a single goal's status.
pub(crate) fn event_goal_id(event: &VolitionEvent) -> Option<&str> {
    match event {
        VolitionEvent::GoalCandidateAccepted { goal_id, .. }
        | VolitionEvent::GoalCandidateRejected { goal_id, .. }
        | VolitionEvent::GoalRetired { goal_id, .. } => Some(goal_id.as_str()),
        _ => None,
    }
}

/// Builds the `(goal_status_before, goal_status_after)` objects keyed by every goal id the given
/// events affected, so a trace consumer can confirm the recorded resolution against the reducer
/// output.
pub(crate) fn goal_status_diff(
    events: &[VolitionEvent],
    before: &VolitionState,
    after: &VolitionState,
) -> (serde_json::Value, serde_json::Value) {
    let affected: Vec<&str> = events.iter().filter_map(event_goal_id).collect();
    let status_before: serde_json::Map<String, serde_json::Value> = affected
        .iter()
        .map(|id| {
            (
                id.to_string(),
                json!(before.goal(id).map(|dynamic| dynamic.status.to_string())),
            )
        })
        .collect();
    let status_after: serde_json::Map<String, serde_json::Value> = affected
        .iter()
        .map(|id| {
            (
                id.to_string(),
                json!(after.goal(id).map(|dynamic| dynamic.status.to_string())),
            )
        })
        .collect();
    (
        serde_json::Value::Object(status_before),
        serde_json::Value::Object(status_after),
    )
}

/// Writes one volition trace record for the given `operation` (e.g. `"goal-coherence-check"` or
/// `"live-goal-formation"`), keyed by `trigger`+`tick`. The two coherence experiments previously
/// carried near-identical writers differing only in the operation literal.
pub(crate) fn write_volition_trace(
    context: &mut RunContext,
    operation: &str,
    trigger: &str,
    tick: u64,
    events: &[VolitionEvent],
    details: serde_json::Value,
) -> anyhow::Result<Uuid> {
    let trace_record = TraceRecord::new(
        context.experiment_id(),
        operation,
        format!("trigger={trigger} tick={tick}"),
        format!("events_emitted={}", events.len()),
    )
    .with_details(details);
    let trace_id = trace_record.trace_id;
    context.record_trace(trace_record)?;
    Ok(trace_id)
}

/// Writes one `goal-coherence-check` trace record. Shared by both coherence experiments so the
/// documented `goal-coherence-check` record type has a single writer.
pub(crate) fn write_coherence_trace(
    context: &mut RunContext,
    trigger: &str,
    tick: u64,
    events: &[VolitionEvent],
    details: serde_json::Value,
) -> anyhow::Result<Uuid> {
    write_volition_trace(
        context,
        "goal-coherence-check",
        trigger,
        tick,
        events,
        details,
    )
}
