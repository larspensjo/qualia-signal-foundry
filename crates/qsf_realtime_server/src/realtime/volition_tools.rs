#![allow(dead_code)]

use std::collections::BTreeMap;

use anyhow::Context;
use qsf_session::ToolCategory;
use qsf_tools::{
    Tool, ToolContext, ToolDefinition, ToolMetadata, ToolRequest, ToolResult, ToolSideEffectLevel,
};
use qsf_volition::{
    GoalSelection, ModeArbitrationResult, arbitrate_with_mode, build_state_inspection,
    select_goals_ranked,
};
use serde_json::Value;
use sha2::Digest;

use crate::realtime::tools::{RealtimeToolContext, VolitionStateSnapshot};

pub const INSPECT_VOLITION_STATE_TOOL_NAME: &str = "inspect_volition_state";
pub const SELECT_VOLITION_GOALS_TOOL_NAME: &str = "select_volition_goals";

const SELECT_MAX_SELECTED: usize = 6;
const SELECT_MAX_OMITTED: usize = 8;

pub struct InspectVolitionStateTool;
pub struct SelectVolitionGoalsTool;

impl Tool for InspectVolitionStateTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: INSPECT_VOLITION_STATE_TOOL_NAME.to_string(),
            description: "Inspect the current simulated volition state: mode, tick, goals by status, and last initiative summaries.".to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            INSPECT_VOLITION_STATE_TOOL_NAME,
            "Inspect the current simulated volition state: mode, tick, goals by status, and last initiative summaries.",
            serde_json::json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        ))
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let ctx = realtime_context(ctx)?;

        let Some(snap) = &ctx.volition else {
            let output = serde_json::json!({ "status": "unavailable" });
            let observation_summary = inspect_observation_summary_unavailable(
                &ctx.qsf_session_id,
                ctx.exchange_index,
                &ctx.call_id,
            );
            return Ok(ToolResult {
                tool_name: request.tool_name.clone(),
                category: ToolCategory::ReadOnly,
                side_effect_level: ToolSideEffectLevel::ReadOnly,
                input: request.input.clone(),
                output_text: output.to_string(),
                numeric_value: None,
                observation_summary,
            });
        };

        let inspection = build_state_inspection(&snap.state, &snap.fixture);
        let output = serde_json::json!({
            "status": "ok",
            "mode": inspection.mode,
            "tick": inspection.tick,
            "active_goals": inspection.active_goals,
            "accepted_goals": inspection.accepted_goals,
            "blocked_goals": inspection.blocked_goals,
            "cooldown_goals": inspection.cooldown_goals,
            "retired_goals": inspection.retired_goals,
            "pending_candidate_count": inspection.pending_candidate_count,
            "accepted_candidate_count": inspection.accepted_candidate_count,
            "last_initiative_summaries": inspection.last_initiative_summaries,
            "note": "This reflects simulated internal state. It is not a claim of real subjective experience or desire."
        });
        let observation_summary = inspect_observation_summary_ok(
            &ctx.qsf_session_id,
            ctx.exchange_index,
            &ctx.call_id,
            &inspection,
        );

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: output.to_string(),
            numeric_value: None,
            observation_summary,
        })
    }
}

impl Tool for SelectVolitionGoalsTool {
    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            name: SELECT_VOLITION_GOALS_TOOL_NAME.to_string(),
            description: "Given a query, return ranked active goals, omitted goals, and arbitration result without mutating state.".to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        }
    }

    fn definition(&self) -> Option<ToolDefinition> {
        Some(ToolDefinition::new(
            SELECT_VOLITION_GOALS_TOOL_NAME,
            "Given a query, return ranked active goals, omitted goals, and arbitration result without mutating state.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ))
    }

    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let ctx = realtime_context(ctx)?;

        let Some(snap) = &ctx.volition else {
            let output = serde_json::json!({ "status": "unavailable" });
            let observation_summary = select_observation_summary_unavailable(
                &ctx.qsf_session_id,
                ctx.exchange_index,
                &ctx.call_id,
            );
            return Ok(ToolResult {
                tool_name: request.tool_name.clone(),
                category: ToolCategory::ReadOnly,
                side_effect_level: ToolSideEffectLevel::ReadOnly,
                input: request.input.clone(),
                output_text: output.to_string(),
                numeric_value: None,
                observation_summary,
            });
        };

        let query = request
            .structured
            .as_ref()
            .and_then(|value| value.get("query"))
            .and_then(|value| value.as_str())
            .context("select_volition_goals requires `query`")?;

        let ranked = select_goals_ranked(query, &snap.state, &snap.fixture);
        // Task 9 bridge: the select tool still reports only the qualified winner. Reporting the
        // full qualification outcome is wired in by the select-tool outcome work.
        let arbitration =
            arbitrate_with_mode(ranked.selected.clone(), &snap.fixture, snap.state.mode)
                .and_then(|outcome| outcome.qualified);
        let snapshot_hash = volition_snapshot_hash(snap);
        let input_terms = ranked.input_terms.clone();

        let selected_truncated = ranked.selected.len() > SELECT_MAX_SELECTED;
        let omitted_truncated = ranked.omitted.len() > SELECT_MAX_OMITTED;
        let observation_status = if ranked.selected.is_empty() {
            "no_match"
        } else {
            "ok"
        };
        let observation_summary = build_select_observation_summary(
            &ctx.qsf_session_id,
            query,
            snap,
            &ranked,
            &arbitration,
            &snapshot_hash,
            ctx.exchange_index,
            &ctx.call_id,
            selected_truncated,
            omitted_truncated,
            observation_status,
        );

        let selected: Vec<Value> = ranked
            .selected
            .iter()
            .take(SELECT_MAX_SELECTED)
            .map(|selection| select_model_goal_value(selection, snap))
            .collect();
        let omitted: Vec<Value> = ranked
            .omitted
            .iter()
            .take(SELECT_MAX_OMITTED)
            .map(omitted_model_goal_value)
            .collect();

        let output = serde_json::json!({
            "status": observation_status,
            "query_terms": input_terms,
            "mode": snap.state.mode,
            "tick": snap.state.tick,
            "selected": selected,
            "omitted": omitted,
            "suppressed_cooldown_count": ranked.suppressed_cooldown.len(),
            "arbitration": model_arbitration_value(&arbitration),
            "volition_snapshot_hash": snapshot_hash,
            "note": "This reflects simulated internal state. It is not a claim of real subjective experience or desire."
        });

        Ok(ToolResult {
            tool_name: request.tool_name.clone(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
            input: request.input.clone(),
            output_text: output.to_string(),
            numeric_value: None,
            observation_summary,
        })
    }
}

fn realtime_context(ctx: &dyn ToolContext) -> anyhow::Result<&RealtimeToolContext> {
    ctx.as_any()
        .downcast_ref::<RealtimeToolContext>()
        .context("realtime tool context missing")
}

fn inspect_observation_summary_unavailable(
    session_id: &str,
    exchange_index: usize,
    call_id: &str,
) -> String {
    serde_json::json!({
        "qsf_session_id": session_id,
        "tool_name": INSPECT_VOLITION_STATE_TOOL_NAME,
        "status": "unavailable",
        "artifact_or_record_reference": artifact_reference(exchange_index, call_id),
    })
    .to_string()
}

fn inspect_observation_summary_ok(
    session_id: &str,
    exchange_index: usize,
    call_id: &str,
    inspection: &qsf_volition::VolitionStateInspection,
) -> String {
    serde_json::json!({
        "qsf_session_id": session_id,
        "tool_name": INSPECT_VOLITION_STATE_TOOL_NAME,
        "status": "ok",
        "volition_tick": inspection.tick,
        "mode": inspection.mode,
        "active_count": inspection.active_goals.len(),
        "accepted_count": inspection.accepted_goals.len(),
        "blocked_count": inspection.blocked_goals.len(),
        "cooldown_count": inspection.cooldown_goals.len(),
        "retired_count": inspection.retired_goals.len(),
        "artifact_or_record_reference": artifact_reference(exchange_index, call_id),
    })
    .to_string()
}

fn select_observation_summary_unavailable(
    session_id: &str,
    exchange_index: usize,
    call_id: &str,
) -> String {
    serde_json::json!({
        "qsf_session_id": session_id,
        "tool_name": SELECT_VOLITION_GOALS_TOOL_NAME,
        "status": "unavailable",
        "artifact_or_record_reference": artifact_reference(exchange_index, call_id),
    })
    .to_string()
}

#[allow(clippy::too_many_arguments)]
fn build_select_observation_summary(
    session_id: &str,
    query: &str,
    snap: &VolitionStateSnapshot,
    ranked: &qsf_volition::RankedSelectionResult,
    arbitration: &Option<ModeArbitrationResult>,
    snapshot_hash: &str,
    exchange_index: usize,
    call_id: &str,
    selected_truncated: bool,
    omitted_truncated: bool,
    status: &str,
) -> String {
    let salience_snapshot: BTreeMap<String, i32> = ranked
        .selected
        .iter()
        .map(|selection| {
            let salience = snap
                .state
                .goals
                .get(&selection.goal.id)
                .map(|dynamic| dynamic.salience)
                .unwrap_or(0);
            (selection.goal.id.clone(), salience)
        })
        .collect();

    serde_json::json!({
        "qsf_session_id": session_id,
        "tool_name": SELECT_VOLITION_GOALS_TOOL_NAME,
        "status": status,
        "volition_tick": snap.state.tick,
        "mode": snap.state.mode,
        "input_query": query,
        "selected_goal_ids": ranked.selected.iter().map(|selection| selection.goal.id.clone()).collect::<Vec<_>>(),
        "omitted_goal_ids": ranked.omitted.iter().map(|goal| goal.goal.id.clone()).collect::<Vec<_>>(),
        "suppressed_cooldown_ids": ranked.suppressed_cooldown.iter().map(|goal| goal.goal.id.clone()).collect::<Vec<_>>(),
        "visible_blocked_ids": ranked.visible_blocked.iter().map(|goal| goal.goal.id.clone()).collect::<Vec<_>>(),
        "selected_truncated": selected_truncated,
        "omitted_truncated": omitted_truncated,
        "salience_snapshot": salience_snapshot,
        "arbitration_result": trace_arbitration_value(arbitration),
        "volition_snapshot_hash": snapshot_hash,
        "artifact_or_record_reference": artifact_reference(exchange_index, call_id),
    })
    .to_string()
}

fn select_model_goal_value(selection: &GoalSelection, snap: &VolitionStateSnapshot) -> Value {
    let dynamic = snap.state.goals.get(&selection.goal.id);
    let salience = dynamic.map(|entry| entry.salience).unwrap_or(0);
    let status = dynamic
        .map(|entry| entry.status)
        .unwrap_or(selection.goal.status);

    serde_json::json!({
        "id": &selection.goal.id,
        "title": &selection.goal.title,
        "summary": &selection.goal.summary,
        "status": status,
        "salience": salience,
        "relevance_score": selection.relevance_score,
        "matched_terms": selection.matched_terms(),
        "scope": selection.goal.scope,
        "tension_ids": &selection.goal.tension_ids,
    })
}

fn omitted_model_goal_value(goal: &qsf_volition::OmittedGoal) -> Value {
    serde_json::json!({
        "id": &goal.goal.id,
        "title": &goal.goal.title,
        "reason": &goal.reason,
    })
}

fn model_arbitration_value(arbitration: &Option<ModeArbitrationResult>) -> Option<Value> {
    arbitration.as_ref().map(|value| {
        serde_json::json!({
            "winner_id": &value.winner.goal.id,
            "winner_title": &value.winner.goal.title,
            "winner_effective_tier": value.winner_bias.effective_tier,
            "winner_effective_tension_id": &value.winner_effective_tension_id,
            "winner_effective_tension_title": &value.winner_effective_tension_title,
            "loser_count": value.losers.len(),
        })
    })
}

fn trace_arbitration_value(arbitration: &Option<ModeArbitrationResult>) -> Option<Value> {
    arbitration.as_ref().map(|value| {
        serde_json::json!({
            "winner_id": &value.winner.goal.id,
            "winner_effective_tier": value.winner_bias.effective_tier,
        })
    })
}

fn volition_snapshot_hash(snap: &VolitionStateSnapshot) -> String {
    let payload = serde_json::json!({
        "state": &snap.state,
        "fixture": &snap.fixture,
    });
    let hash = sha2::Sha256::digest(payload.to_string().as_bytes());
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn artifact_reference(exchange_index: usize, call_id: &str) -> String {
    format!("exchange:{exchange_index}/tool_call:{call_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsf_session::ToolPermissionDecision;
    use qsf_tools::ToolPermission;
    use qsf_volition::{VolitionState, realtime_seed_fixture};
    use serde_json::Value;
    use tempfile::TempDir;

    use crate::diagnostics::DiagnosticWriter;
    use crate::realtime::tools::{ToolSessionSnapshot, tool_permission_decision};
    use crate::state::{AppState, BrowserSessionConfig, SessionIdMode, SessionRuntime};

    fn state(tempdir: &TempDir) -> AppState {
        AppState::new_with_realtime_ws_base_url(
            "test-api-key",
            "http://127.0.0.1:9999",
            "wss://example.invalid/realtime",
            tempdir.path().to_path_buf(),
            SessionIdMode::Default,
        )
        .expect("state")
    }

    fn runtime(tempdir: &TempDir) -> SessionRuntime {
        let diagnostics = DiagnosticWriter::create(tempdir.path().join("diagnostics.jsonl"))
            .expect("diagnostics");
        SessionRuntime::new(
            "test-session".to_string(),
            BrowserSessionConfig::default(),
            diagnostics,
        )
    }

    fn tool_context_with_volition(
        tempdir: &TempDir,
        runtime: &SessionRuntime,
    ) -> RealtimeToolContext {
        let fixture = realtime_seed_fixture();
        let vol_state = VolitionState::from_fixture(&fixture);
        RealtimeToolContext {
            state: state(tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(runtime),
            volition: Some(VolitionStateSnapshot {
                state: vol_state,
                fixture,
            }),
            exchange_index: 1,
            call_id: "call-abc".to_string(),
        }
    }

    fn tool_context_no_volition(
        tempdir: &TempDir,
        runtime: &SessionRuntime,
    ) -> RealtimeToolContext {
        RealtimeToolContext {
            state: state(tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(runtime),
            volition: None,
            exchange_index: 0,
            call_id: String::new(),
        }
    }

    fn inspect_request() -> ToolRequest {
        ToolRequest::new(
            INSPECT_VOLITION_STATE_TOOL_NAME,
            "{}",
            None,
            ToolPermission::read_only(),
            "tester",
        )
    }

    fn select_request(query: &str) -> ToolRequest {
        let args = serde_json::json!({ "query": query });
        ToolRequest::new(
            SELECT_VOLITION_GOALS_TOOL_NAME,
            args.to_string(),
            Some(args),
            ToolPermission::read_only(),
            "tester",
        )
    }

    #[test]
    fn permission_decision_allows_volition_tools_when_allow_listed() {
        let allow_list = vec![
            INSPECT_VOLITION_STATE_TOOL_NAME.to_string(),
            SELECT_VOLITION_GOALS_TOOL_NAME.to_string(),
        ];
        let read_only_meta = |name: &str| ToolMetadata {
            name: name.to_string(),
            description: "test".to_string(),
            category: ToolCategory::ReadOnly,
            side_effect_level: ToolSideEffectLevel::ReadOnly,
        };

        assert_eq!(
            tool_permission_decision(
                INSPECT_VOLITION_STATE_TOOL_NAME,
                &allow_list,
                Some(&read_only_meta(INSPECT_VOLITION_STATE_TOOL_NAME))
            ),
            ToolPermissionDecision::Allowed
        );
        assert_eq!(
            tool_permission_decision(
                SELECT_VOLITION_GOALS_TOOL_NAME,
                &allow_list,
                Some(&read_only_meta(SELECT_VOLITION_GOALS_TOOL_NAME))
            ),
            ToolPermissionDecision::Allowed
        );
    }

    #[test]
    fn inspect_volition_returns_unavailable_when_volition_is_none() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_no_volition(&tempdir, &runtime);
        let tool = InspectVolitionStateTool;

        let result = tool.execute(&inspect_request(), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        assert_eq!(json["status"], "unavailable");
    }

    #[test]
    fn inspect_volition_output_contains_required_fields() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = InspectVolitionStateTool;

        let result = tool.execute(&inspect_request(), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        assert!(json.get("mode").is_some(), "output must contain mode");
        assert!(json.get("tick").is_some(), "output must contain tick");
        assert!(
            json.get("active_goals").is_some() || json.get("accepted_goals").is_some(),
            "output must contain at least one goal list key"
        );
    }

    #[test]
    fn inspect_volition_observation_summary_contains_key_fields() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = InspectVolitionStateTool;

        let result = tool.execute(&inspect_request(), &ctx).unwrap();
        let summary: Value = serde_json::from_str(&result.observation_summary)
            .expect("observation_summary must be valid JSON");

        assert_eq!(
            summary["tool_name"], INSPECT_VOLITION_STATE_TOOL_NAME,
            "must carry tool_name"
        );
        assert!(
            summary.get("qsf_session_id").is_some(),
            "must carry qsf_session_id"
        );
        assert!(
            summary.get("volition_tick").is_some(),
            "must carry volition_tick"
        );
        assert!(summary.get("mode").is_some(), "must carry mode");
        assert!(
            summary.get("artifact_or_record_reference").is_some(),
            "must carry artifact_or_record_reference"
        );
    }

    #[test]
    fn inspect_volition_output_does_not_contain_api_key() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = InspectVolitionStateTool;

        let result = tool.execute(&inspect_request(), &ctx).unwrap();

        assert!(!result.output_text.contains("OPENAI_API_KEY"));
        assert!(!result.observation_summary.contains("OPENAI_API_KEY"));
    }

    #[test]
    fn select_volition_returns_unavailable_when_volition_is_none() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_no_volition(&tempdir, &runtime);
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("help me"), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        assert_eq!(json["status"], "unavailable");
    }

    #[test]
    fn select_volition_returns_no_match_when_query_has_no_keyword_match() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = SelectVolitionGoalsTool;

        let result = tool
            .execute(&select_request("xyzzy frobnicator quux"), &ctx)
            .unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        assert_eq!(json["status"], "no_match");
        assert_eq!(json["arbitration"], Value::Null);
    }

    #[test]
    fn select_volition_output_is_deterministic_for_same_state_and_query() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let tool = SelectVolitionGoalsTool;

        let result1 = tool
            .execute(
                &select_request("how can you help me"),
                &tool_context_with_volition(&tempdir, &runtime),
            )
            .unwrap();
        let result2 = tool
            .execute(
                &select_request("how can you help me"),
                &tool_context_with_volition(&tempdir, &runtime),
            )
            .unwrap();

        assert_eq!(result1.output_text, result2.output_text);
    }

    #[test]
    fn select_volition_observation_summary_is_parseable_json_with_required_trace_fields() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = SelectVolitionGoalsTool;

        let result = tool
            .execute(&select_request("how can you help me"), &ctx)
            .unwrap();
        let trace: Value = serde_json::from_str(&result.observation_summary)
            .expect("observation_summary must be JSON");

        assert!(
            trace.get("qsf_session_id").is_some(),
            "trace must have qsf_session_id"
        );
        assert!(
            trace.get("tool_name").is_some(),
            "trace must have tool_name"
        );
        assert!(
            trace.get("volition_tick").is_some(),
            "trace must have volition_tick"
        );
        assert!(trace.get("mode").is_some(), "trace must have mode");
        assert!(
            trace.get("input_query").is_some(),
            "trace must have input_query"
        );
        assert!(
            trace.get("selected_goal_ids").is_some(),
            "trace must have selected_goal_ids"
        );
        assert!(
            trace.get("omitted_goal_ids").is_some(),
            "trace must have omitted_goal_ids"
        );
        assert!(trace.get("suppressed_cooldown_ids").is_some());
        assert!(trace.get("visible_blocked_ids").is_some());
        assert!(trace.get("selected_truncated").is_some());
        assert!(trace.get("omitted_truncated").is_some());
        assert!(trace.get("salience_snapshot").is_some());
        assert!(trace.get("arbitration_result").is_some());
        assert!(trace.get("volition_snapshot_hash").is_some());
        assert!(trace.get("artifact_or_record_reference").is_some());
    }

    #[test]
    fn select_volition_output_does_not_contain_api_key() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = tool_context_with_volition(&tempdir, &runtime);
        let tool = SelectVolitionGoalsTool;

        let result = tool
            .execute(&select_request("how can you help"), &ctx)
            .unwrap();

        assert!(!result.output_text.contains("OPENAI_API_KEY"));
        assert!(!result.observation_summary.contains("OPENAI_API_KEY"));
    }

    #[test]
    fn select_volition_caps_model_visible_output_at_6_selected() {
        use qsf_volition::{
            ActivationKeyword, AllowedEffect, DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD, Goal,
            GoalScope, GoalStatus, Tension, TensionPriority, VolitionFixture,
        };

        let tension = Tension {
            id: "t1".to_string(),
            title: "T1".to_string(),
            summary: "test".to_string(),
            priority_bias: TensionPriority::Medium,
            arbitration_tier: 7,
            focused_bias: 0,
            exploratory_bias: 0,
        };
        let goals: Vec<Goal> = (0..20)
            .map(|i| Goal {
                id: format!("goal-{i:02}"),
                title: format!("Goal {i}"),
                summary: "test summary".to_string(),
                tension_ids: vec!["t1".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Session,
                base_priority: 70,
                activation_keywords: vec![ActivationKeyword::normal("test")],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "done".to_string(),
                evidence_refs: vec![],
                estimated_tokens: 10,
                source_reference: "plan".to_string(),
            })
            .collect();
        let fixture = VolitionFixture {
            tensions: vec![tension],
            goals,
            arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
        };
        let vol_state = VolitionState::from_fixture(&fixture);

        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = RealtimeToolContext {
            state: state(&tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot {
                state: vol_state,
                fixture,
            }),
            exchange_index: 1,
            call_id: "call-cap".to_string(),
        };
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("test"), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        let selected_count = json["selected"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(
            selected_count <= SELECT_MAX_SELECTED,
            "selected must be capped at {SELECT_MAX_SELECTED}, got {selected_count}"
        );

        let arbitration = json["arbitration"].as_object().expect("arbitration object");
        assert_eq!(
            arbitration
                .get("loser_count")
                .and_then(|value| value.as_u64()),
            Some(19),
            "compact arbitration should report loser count without re-emitting losers"
        );
        assert!(
            !arbitration.contains_key("winner") && !arbitration.contains_key("losers"),
            "model-visible arbitration must not serialize full goal selections"
        );
    }

    #[test]
    fn select_volition_caps_model_visible_output_at_8_omitted() {
        use qsf_volition::{
            ActivationKeyword, AllowedEffect, DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD, Goal,
            GoalScope, GoalStatus, Tension, TensionPriority, VolitionFixture,
        };

        let tension = Tension {
            id: "t1".to_string(),
            title: "T1".to_string(),
            summary: "test".to_string(),
            priority_bias: TensionPriority::Medium,
            arbitration_tier: 7,
            focused_bias: 0,
            exploratory_bias: 0,
        };
        let goals: Vec<Goal> = (0..12)
            .map(|i| Goal {
                id: format!("goal-{i:02}"),
                title: format!("Goal {i}"),
                summary: "test summary".to_string(),
                tension_ids: vec!["t1".to_string()],
                status: if i < 2 {
                    GoalStatus::Accepted
                } else {
                    GoalStatus::Proposed
                },
                scope: GoalScope::Session,
                base_priority: 70,
                activation_keywords: vec![ActivationKeyword::normal("test")],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "done".to_string(),
                evidence_refs: vec![],
                estimated_tokens: 10,
                source_reference: "plan".to_string(),
            })
            .collect();
        let fixture = VolitionFixture {
            tensions: vec![tension],
            goals,
            arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
        };
        let vol_state = VolitionState::from_fixture(&fixture);

        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = RealtimeToolContext {
            state: state(&tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot {
                state: vol_state,
                fixture,
            }),
            exchange_index: 1,
            call_id: "call-omit-cap".to_string(),
        };
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("test"), &ctx).unwrap();
        let json: Value = serde_json::from_str(&result.output_text).unwrap();

        let omitted_count = json["omitted"].as_array().map(|a| a.len()).unwrap_or(0);
        assert!(
            omitted_count <= SELECT_MAX_OMITTED,
            "omitted must be capped at {SELECT_MAX_OMITTED}, got {omitted_count}"
        );
    }

    #[test]
    fn select_volition_trace_includes_full_list_when_truncated() {
        use qsf_volition::{
            ActivationKeyword, AllowedEffect, DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD, Goal,
            GoalScope, GoalStatus, Tension, TensionPriority, VolitionFixture,
        };

        let tension = Tension {
            id: "t1".to_string(),
            title: "T1".to_string(),
            summary: "test".to_string(),
            priority_bias: TensionPriority::Medium,
            arbitration_tier: 7,
            focused_bias: 0,
            exploratory_bias: 0,
        };
        let goals: Vec<Goal> = (0..10)
            .map(|i| Goal {
                id: format!("goal-{i:02}"),
                title: format!("Goal {i}"),
                summary: "test summary".to_string(),
                tension_ids: vec!["t1".to_string()],
                status: GoalStatus::Accepted,
                scope: GoalScope::Session,
                base_priority: 70,
                activation_keywords: vec![ActivationKeyword::normal("test")],
                allowed_effects: vec![AllowedEffect::Reflect],
                satisfaction_condition_summary: "done".to_string(),
                evidence_refs: vec![],
                estimated_tokens: 10,
                source_reference: "plan".to_string(),
            })
            .collect();
        let fixture = VolitionFixture {
            tensions: vec![tension],
            goals,
            arbitration_qualification_threshold: DEFAULT_ARBITRATION_QUALIFICATION_THRESHOLD,
        };
        let vol_state = VolitionState::from_fixture(&fixture);

        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let ctx = RealtimeToolContext {
            state: state(&tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot {
                state: vol_state,
                fixture,
            }),
            exchange_index: 1,
            call_id: "call-trunc".to_string(),
        };
        let tool = SelectVolitionGoalsTool;

        let result = tool.execute(&select_request("test"), &ctx).unwrap();
        let trace: Value = serde_json::from_str(&result.observation_summary).unwrap();

        let trace_selected = trace["selected_goal_ids"].as_array().unwrap();
        assert_eq!(
            trace_selected.len(),
            10,
            "trace must contain all 10 goal ids"
        );
        assert_eq!(trace["selected_truncated"], Value::Bool(true));
        assert!(
            trace["arbitration_result"].get("winner").is_none()
                && trace["arbitration_result"].get("losers").is_none(),
            "trace arbitration summary must stay compact while selected_goal_ids preserves full ids"
        );
    }

    #[test]
    fn select_volition_snapshot_hash_changes_when_fixture_changes() {
        let tempdir = TempDir::new().unwrap();
        let runtime = runtime(&tempdir);
        let tool = SelectVolitionGoalsTool;

        let fixture1 = realtime_seed_fixture();
        let state1 = VolitionState::from_fixture(&fixture1);

        let mut fixture2 = fixture1.clone();
        fixture2.goals[0].title = "Modified Title".to_string();
        let state2 = VolitionState::from_fixture(&fixture2);

        let ctx1 = RealtimeToolContext {
            state: state(&tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot {
                state: state1,
                fixture: fixture1,
            }),
            exchange_index: 1,
            call_id: "call-1".to_string(),
        };
        let ctx2 = RealtimeToolContext {
            state: state(&tempdir),
            qsf_session_id: runtime.qsf_session_id.clone(),
            snapshot: ToolSessionSnapshot::from_runtime(&runtime),
            volition: Some(VolitionStateSnapshot {
                state: state2,
                fixture: fixture2,
            }),
            exchange_index: 1,
            call_id: "call-2".to_string(),
        };

        let r1 = tool
            .execute(&select_request("how can you help"), &ctx1)
            .unwrap();
        let r2 = tool
            .execute(&select_request("how can you help"), &ctx2)
            .unwrap();

        let t1: Value = serde_json::from_str(&r1.observation_summary).unwrap();
        let t2: Value = serde_json::from_str(&r2.observation_summary).unwrap();

        assert_ne!(
            t1["volition_snapshot_hash"], t2["volition_snapshot_hash"],
            "hash must differ when fixture differs"
        );
    }
}
