# Idea: Tool System Follow-Ups

## Status

Idea backlog. These items were preserved from `Plan.ToolSystemBridge.md` when
the bridge plan was closed out and the OpenAI provider work was extracted into
its own plan.

## Purpose

Keep post-bridge tool-system work discoverable without leaving it buried in an
archived implementation plan.

The completed bridge established the core rule: model-callable tools execute
through `ToolRegistry`, role allow-lists are enforced by
`dispatch_model_tool_calls`, and providers only marshal tool definitions and
tool calls. The items below are follow-up applications or extensions of that
boundary.

## Related Plans

- `docs/Plans/Plan.ToolSystemBridge.md`
- `docs/Plans/Plan.OpenAIToolCapableRequests.md`

## Follow-Up Ideas

### Realtime voice tool execution path

Current state:
- `realtime_voice_session.rs` records `ToolRequested` with
  `auto_executed: false`.
- Realtime voice providers still do not execute tools directly, per the
  2026-05-14 DecisionLog entry.

Candidate direction:
- Convert realtime provider tool-call records into `ModelToolCall` or an
  equivalent runtime-owned request shape.
- Route accepted calls through `dispatch_model_tool_calls` and `ToolRegistry`.
- Feed results back into the realtime session as explicit QSF events.

Verification:
- A simulated realtime voice session records `ToolRequested` followed by
  `ToolCompleted` or `ToolFailed`.
- No provider adapter mutates runtime state or executes tools directly.
- Tool events include registry metadata such as category and side-effect level.

### Documentation introspection tools

Current state:
- `Idea.SelfReflectionProjectIntrospection.md` describes project
  introspection, but those capabilities are not yet real `Tool` implementations.

Candidate direction:
- Implement project/documentation introspection as registry-backed tools rather
  than prompt-only context injection.
- Start with read-only tools that inspect bounded documentation surfaces.
- Expose model-facing schemas only for roles that explicitly need the tools.

Verification:
- Tools declare metadata, permissions, model schemas, and deterministic result
  envelopes.
- Roles that use introspection list the tools in `ModelRole.allowed_tools`.
- Disallowed roles cannot call the tools through `dispatch_model_tool_calls`.

### ToolContext memory-store access

Current state:
- `ToolContext` exposes `session_state()` for borrowed session-aware tools.
- No tool currently needs direct memory-store access.

Candidate direction:
- Add a typed `ToolContext` accessor for memory-store access only when a real
  memory-reading tool requires it.
- Keep borrowed-state access explicit rather than using untyped downcasts.
- Preserve reducer purity: tools may observe through context but state changes
  still flow through events and reducers.

Verification:
- Stateless tools continue to work with `EmptyToolContext`.
- Session-aware tools continue to work with `SessionToolContext`.
- New memory-aware tools fail clearly when invoked without the required context.

## Notes

- OpenAI tool-capable request/response handling is tracked separately in
  `docs/Plans/Plan.OpenAIToolCapableRequests.md`.
- These items are not decisions yet. Promote any durable rule that emerges from
  implementation to `docs/DecisionLog.md`.
