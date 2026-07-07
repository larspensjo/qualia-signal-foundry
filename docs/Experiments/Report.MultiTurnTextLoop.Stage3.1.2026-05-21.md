# Multi-Turn Text Loop Stage 3.1 Verification

Date: 2026-05-21

## Scope

Verified the provider-native OpenAI function-calling follow-up for the
`multi-turn-text-loop` experiment. Stage 3.1 makes OpenAI-backed conversational turns
use the same recall path as the deterministic mock path: advertise `recall_turn`, parse
provider tool calls, execute QSF tools through the registry, and send provider-native
tool-result messages on the follow-up request.

## Provider Surface

Stage 3.1 uses OpenAI Chat Completions rather than migrating the experiment to the
Responses API. Tool-capable requests bypass `openai_provider_kit` inside `qsf_app`
because the pinned kit revision has no tool-definition, assistant tool-call, or tool
result message support.

Implemented request shape:

- `tools`: Chat Completions function tools with `{ "type": "function", "function": ... }`.
- Assistant tool calls: assistant messages with `tool_calls` entries and provider call ids.
- Tool results: tool messages with `role: "tool"`, `tool_call_id`, and textual content.
- Existing non-tool OpenAI requests continue through the kit path.

## Implementation Evidence

Key modules:

- `crates/qsf_app/src/models/model_client.rs` adds `tool_call_id` and assistant
  `tool_calls` to provider-agnostic model messages.
- `crates/qsf_app/src/models/openai_tool_client.rs` serializes tool-capable OpenAI
  requests and parses OpenAI tool-call responses.
- `crates/qsf_app/src/models/openai_provider.rs` routes requests with tool definitions,
  assistant tool calls, or tool-result messages through the direct OpenAI serializer.
- `crates/qsf_app/src/models/tool_dispatch.rs` enforces `ModelRole.allowed_tools` and
  routes model-emitted tool calls through `ToolRegistry`.
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` sends `recall_turn` and
  `calculator` tool definitions on the initial responder request, appends assistant
  tool-call and tool-result messages, and sends the follow-up request without tools.
- `crates/qsf_app/src/conversation/prompt.rs` includes `tool_call_id` and assistant
  tool calls in prompt hashing so recalled turns remain cache-stable once frozen.

## Automated Coverage

Fresh verification on 2026-05-21:

```powershell
cargo test -p qsf_app multi_turn_text_loop --lib
cargo test -p qsf_app openai_tool_client
cargo build
cargo clippy --all-targets -- -D warnings
```

Results:

| Check | Result |
|---|---:|
| Multi-turn focused tests | 22 passed |
| OpenAI tool-client focused tests | 9 passed |
| Workspace build | passed |
| Clippy all targets with warnings denied | passed |

Covered behavior includes:

- serializing text-only OpenAI requests without `tools`
- serializing `recall_turn` function definitions
- serializing assistant tool calls and tool-result messages with provider call ids
- preserving `max_completion_tokens`, temperature, JSON response mode, usage, finish
  reason, and cached-token telemetry
- parsing text responses, single tool calls, multiple tool calls, content-part text,
  missing tool-call ids, malformed arguments, unsupported tool-call types, and cached
  token counts
- verifying the multi-turn OpenAI-style recall path preserves the provider `tool_call_id`,
  places the assistant tool-call message before the matching tool result, hides tools on
  the follow-up request, and emits the second `PromptAssembled` event before the
  follow-up model request
- rejecting disallowed model-emitted tool calls before registry execution

The OpenAI coverage is intentionally split at stable boundaries instead of using a
network-level mocked HTTP harness: request JSON serialization and response parsing are
tested directly, and the runtime exchange is tested through the `ModelClient` boundary
with a capturing OpenAI-style client.

## Live OpenAI Recall

A live OpenAI recall run was recorded in `docs/EngineeringDiary.md` (now deprecated) on 2026-05-18:
`runs/2026-05-18-174421-multi-turn-text-loop` completed with one `recall_turn`
execution and a final verbatim `[Turn 0]` response.

That run directory is not present in the current workspace snapshot, so this report
uses the diary entry as historical observation rather than re-counting events from
`events.jsonl`. The current repository still contains later live OpenAI multi-turn runs
that advertise tools, but they did not force a recall path and therefore are not Stage
3.1 recall smoke-test artifacts.

## Recall Frequency And Latency

The missing live run artifact prevents a precise event-log-derived latency table for the
2026-05-18 smoke test. The implemented report surface records recall counts and
per-call latency when a run artifact is available, and the automated OpenAI-style test
covers the full request sequence without depending on live provider behavior.

Current documented live finding:

| Source | Recall requests | Recall executions | Notes |
|---|---:|---:|---|
| 2026-05-18 diary observation | 1 | 1 | Final response used verbatim `[Turn 0]` content |

## Interpretation

Stage 3.1 is complete for the codebase and automated verification. Provider-native
OpenAI function calling is no longer a blocker for the multi-turn text loop. The one
remaining caveat is archival evidence quality: the live smoke-test run mentioned in the
diary is not present under `runs/` in this workspace snapshot.

The recalled context improved the observed live answer in the narrow smoke-test sense:
the final answer used the verbatim recalled turn instead of relying only on a warm
summary. Broader claims about recall-use frequency or overuse/underuse need additional
live sessions with retained artifacts.

## Follow-Up Questions

- Should future live recall smoke tests be preserved under `runs/` or summarized into a
  stable experiment report immediately, so archive evidence is not lost with ignored run
  artifacts?
- Should the system prompt or tool description be tuned if live models underuse
  `recall_turn` when exact older details are requested?
- Should session-aware retrieval use warm summaries and recent active turns to bias
  memory retrieval before recall is needed?
- Should session-local summaries or recall records become candidates for the reviewed
  durable-memory pipeline?
