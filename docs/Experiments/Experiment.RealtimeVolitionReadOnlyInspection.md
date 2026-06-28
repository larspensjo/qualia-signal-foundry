# Experiment: Realtime Volition Read-Only Inspection

## Experiment ID

`Experiment.RealtimeVolitionReadOnlyInspection`

## Status

Running

## Summary

Validate that the realtime model can call `inspect_volition_state` and
`select_volition_goals` during a live session, receive a traceable read-only
explanation of active goals and arbitration, and answer in a way that clearly
distinguishes simulated internal state from any claim of real desire or
subjective experience.

This is the live-validation companion to
`docs/Plans/Plan.RealtimeVolitionIntegration.md` for the read-only volition
tools added in Phase 3.

## Motivation

- Confirms that the new read-only realtime tools are reachable from a live
  session.
- Checks that the tool output is grounded in the per-session volition snapshot
  rather than a generic response.
- Verifies that the persisted trace is complete enough to explain why a
  particular goal was selected or omitted.
- Catches regressions where the model ignores the tool, omits the trace, or
  speaks as if the simulated state were literal intent.

## Related Documents

```text
docs/Plans/Plan.RealtimeVolitionIntegration.md
docs/Architecture/Architecture.RealtimeSessionServer.md
docs/Architecture/Architecture.ToolSystem.md
docs/Architecture/Architecture.VolitionSystem.md
docs/Architecture/Architecture.StateAndObservability.md
crates/qsf_realtime_server/src/realtime/volition_tools.rs
crates/qsf_realtime_server/src/realtime/tools.rs
```

## Hypothesis

A live realtime session can safely expose the current per-session volition
state through read-only tools, and the resulting trace can explain the tool
chain without exposing secrets or collapsing into a copy of the injected
context packet.

## Scope

### In Scope

- Accessibility of `inspect_volition_state`.
- Accessibility of `select_volition_goals`.
- Grounded spoken answers that reference the tool result.
- Persisted trace coverage for `select_volition_goals`.
- Clear language that the state is simulated internal state, not literal
  desire.

### Out of Scope

- Any write-capable volition behavior.
- Context injection before `response.create`.
- Bounded initiative execution.
- UI inspection panel work.

## Setup

- `qsf_realtime_server` running with a live realtime session enabled.
- `OPENAI_API_KEY` configured server-side.
- A session that already has fixture-backed volition state.
- Access to persisted `ToolRequestRecord` and `ToolExecutionRecord` artifacts.

## Procedure

### Automated Verification

1. Parse `ToolExecutionRecord.result_summary` for `select_volition_goals`.
2. Assert the trace contains all required `volition_tool_trace` fields:
   - `qsf_session_id`
   - `tool_name`
   - `volition_tick`
   - `mode`
   - `input_query`
   - `selected_goal_ids`
   - `omitted_goal_ids`
   - `suppressed_cooldown_ids`
   - `visible_blocked_ids`
   - `selected_truncated`
   - `omitted_truncated`
   - `salience_snapshot`
   - `arbitration_result`
   - `volition_snapshot_hash`
   - `artifact_or_record_reference`
3. Assert the artifact reference uses the expected form
   `exchange:<index>/tool_call:<id>`.
4. Assert the tool output is not just a copy of any context injection packet.

### Human Test Steps

1. Start a live realtime session.
2. Ask, "what are you currently focused on?"
3. Ask, "what goals relate to helping me?"
4. Confirm the model calls a volition tool while answering.
5. Confirm the spoken answer is grounded in the tool result.
6. Confirm the answer explicitly treats the volition state as simulated internal
   state rather than a claim of real desire.

## Baseline

Before this experiment, the live realtime model had no read-only volition tools
and could not inspect per-session volition state directly.

## Measurements

### Quantitative Measurements

- Tool-call success rate for the two read-only volition tools.
- Trace completeness rate for `select_volition_goals`.
- Tool output and trace latency in the live session.

### Qualitative Observations

- Whether the spoken answer is grounded in the selected goals.
- Whether the model preserves the distinction between simulated state and real
  desire.
- Whether the trace is sufficient to explain why a goal was selected or
  omitted.

## Success Criteria

- Both read-only volition tools are reachable from a live session.
- The model uses the tool result when answering the test prompts.
- The persisted trace includes every required field listed above.
- The answer stays clearly within the simulated-state framing.

## Failure Criteria

- The model ignores the tool or answers from unsupported assumptions.
- The trace is incomplete or missing the artifact reference.
- The response leaks secrets or resembles a raw context dump.
- The answer collapses simulated state into a claim of real intent.

## Required Observability

- `ToolRequestRecord` and `ToolExecutionRecord` entries for both tools.
- `ToolExecutionRecord.result_summary` for `select_volition_goals`.
- Model-visible output text for the live turn.
- Artifact reference linkage back to the exchange and tool call.

## Trace Completeness Contract

The trace contract applies to `select_volition_goals` because the experiment
needs to explain the selection path from query to ranked goals to arbitration.

Required trace fields:

- `qsf_session_id`
- `tool_name`
- `volition_tick`
- `mode`
- `input_query`
- `selected_goal_ids`
- `omitted_goal_ids`
- `suppressed_cooldown_ids`
- `visible_blocked_ids`
- `selected_truncated`
- `omitted_truncated`
- `salience_snapshot`
- `arbitration_result`
- `volition_snapshot_hash`
- `artifact_or_record_reference`

Artifact boundary:

- The persisted trace lives in `ToolExecutionRecord.result_summary`.
- The model-visible answer is capped separately in `output_text`.
- The artifact reference must identify the exchange and tool call using
  `exchange:<index>/tool_call:<id>`.

Parsing verification:

- Decode the JSON summary from `ToolExecutionRecord.result_summary`.
- Assert each required field is present and non-empty where applicable.
- Assert the selected and omitted ID sets are the uncapped trace sets, not just
  the capped model-visible lists.
- Assert the trace is not a verbatim copy of the injected context packet.

## Expected Output

- A live transcript showing a volition tool call for each prompt.
- A grounded spoken answer that references selected or omitted goals.
- Persisted trace records that can be inspected after the session.

## Results

### Run 1 — 2026-06-28

**Outcome: Tool not called — instructions gap identified**

Two live sessions were run using the stable `default` QSF session id:

- Session 1 (call `rtc_u0_DvnlZabnp5PraQn1IWNMC`): asked "What are you currently focused on?"
- Session 2 (call `rtc_u2_DvnmZWPnGII3nTQ1N6q2M`): asked "What goals relate to helping me?"

In both sessions, the sideband attached successfully, sent a `session.update` that included all
five registered tools (`search_memory`, `get_associations`, `inspect_session_state`,
`inspect_volition_state`, `select_volition_goals`), and issued `response.create` via the
normal trusted turn path. Sideband latency observations were recorded.

**The model did not call either volition tool.** The browser relay exchanges show
`tool_requests: []` and `tool_executions: []`. The engine log shows no ToolLoop turn phase for
either session. The model answered both prompts from its general training context.

**Root cause:** `DEFAULT_INSTRUCTIONS` in `crates/qsf_realtime_server/src/state.rs` contains
only: `"Speak briefly. Keep the browser UI informed, keep secrets server-side, and preserve the
QSF trust boundary."` — there is no guidance telling the model that volition tools exist, when
to use them, or how to frame their output.

**Infrastructure is correct:** Tool wiring is confirmed working — a June 2026 session log shows
the sideband entering `ToolLoop` state when the user explicitly invoked a memory tool, confirming
the tool execution path is functional. The volition tools are correctly registered and included
in the session.update. The failure is a missing usage signal in the session instructions, not a
code or wiring defect.

**Required fix before re-running this experiment:**

Update `DEFAULT_INSTRUCTIONS` (or introduce a per-session instructions field) to include
guidance such as:

> When asked about your current focus, goals, internal state, or what motivates your responses,
> call `inspect_volition_state` first. When asked which goals relate to a specific topic or how
> you can help with something, call `select_volition_goals` with the relevant query. Frame any
> result as simulated internal state — not a claim of real desire, consciousness, or subjective
> experience.

Once the instructions are updated, repeat the human test steps and check the engine log for
ToolLoop state and the diagnostic records for non-empty `tool_requests` and `tool_executions`.
