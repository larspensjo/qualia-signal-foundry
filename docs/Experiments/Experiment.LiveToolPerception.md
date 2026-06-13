# Experiment.LiveToolPerception

## Status

Running

## Summary

Validate the realtime tool plane: during a live voice session, the model
can request read-only QSF perception tools, the server sideband executes or denies
them under an allow-list, returns `function_call_output`, and keeps the trusted
exchange open until the eventual spoken response.

## Motivation

This experiment checks whether the realtime sideband can safely extend a spoken
turn with server-owned perception without leaking credentials, breaking trusted
promotion, or leaving provider function calls unanswered.

## Related Documents

- Architecture/Architecture.RealtimeSessionServer.md
- Architecture/Architecture.ToolSystem.md
- Architecture/Architecture.StateAndObservability.md
- DecisionLog.md

## Hypothesis

Read-only memory and session-state tools can improve live spoken answers while
preserving QSF's reducer boundary, tool permission boundary, and continuity
promotion rules.

## Scope

### In Scope

- `search_memory(query)`
- `get_associations(memory_id)`
- `inspect_session_state()`
- Denial recovery for unknown, malformed, over-cap, or over-privileged calls
- Durable request and execution records on promoted turns

### Out of Scope

- Write-capable tools
- Existing `qsf_app` project-document or recall-turn tools in the live model
- Live-memory extraction and presence cues

## Setup

- Start `qsf_realtime_server` with `OPENAI_API_KEY` configured server-side.
- Use the browser realtime UI and a session memory store with at least one known
  fact that will not fit in the normal injected packet.
- Defaults should advertise the three read-only tools through `session.update`.

## Procedure

1. Start a live browser voice session.
2. Ask for a fact present in memory but absent from the injected context.
3. Confirm the model calls a read-only tool and speaks an answer grounded in the
   returned result.
4. Ask for an absent fact and confirm the spoken answer recovers gracefully.
5. Inspect persisted session artifacts for compact `ToolRequestRecord` and
   `ToolExecutionRecord` entries.
6. Record latency introduced by the tool call.

## Automated Checks

- `cargo test` covers reducer persistence, protocol builders/parsers, permission
  decisions, malformed-argument recovery, mixed-output function calls, loop-cap
  recovery, and lock-free sideband execution.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt` are the completion gates.

## Human Observations

Pending live browser verification.
