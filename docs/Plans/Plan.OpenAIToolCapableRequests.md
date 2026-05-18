# Plan: OpenAI Tool-Capable Requests and Responses

## Status

Draft. Extracted from `Plan.ToolSystemBridge.md` after the bridge plan landed.
This plan owns the OpenAI-specific provider work needed for live tool calls.

## Purpose

Make the OpenAI-backed `ModelClient` exercise the same
`ModelToolDefinition` -> `ModelToolCall` -> `ToolRegistry` -> tool-result-message
path that the mock model already exercises.

The tool bridge is already in place: model-callable tools are registered in
`ToolRegistry`, roles declare permitted tools with `ModelRole.allowed_tools`, and
`dispatch_model_tool_calls` enforces that allow-list before registry execution.
The remaining gap is provider-specific: the current OpenAI adapter records
`ModelRequest.tools` in QSF events but does not send them to OpenAI or parse
provider tool-call responses.

## Evidence

- Deterministic mock verification succeeded in
  `runs/2026-05-18-041754-multi-turn-text-loop`: `recall_turn` was advertised,
  dispatched through `ToolRegistry`, and returned as a tool message for the
  follow-up model call.
- Live OpenAI smoke testing in
  `runs/2026-05-18-043034-multi-turn-text-loop` advertised `recall_turn` on
  conversational requests, but every OpenAI response had `tool_call_count=0`.
  The final assistant response said it could not use tools directly, matching
  the current adapter limitation.
- `crates/qsf_app/src/models/openai_provider.rs` still routes through
  `openai_provider_kit`, whose request and response types do not currently
  expose tool definitions, tool result messages, or provider tool calls.

## Goal

For OpenAI-backed model calls with non-empty `ModelRequest.tools`:

- Serialize QSF tool definitions as OpenAI Chat Completions function tools.
- Parse OpenAI tool-call responses into `ModelToolCall`.
- Preserve provider-native `tool_call_id` so QSF can send tool result messages
  back to OpenAI.
- Keep actual tool execution inside QSF through `dispatch_model_tool_calls` and
  `ToolRegistry`.

## Non-Goals

- Do not move tool execution into the OpenAI provider adapter.
- Do not replace `ToolRegistry`, `ToolPermission`, or `ModelRole.allowed_tools`.
- Do not migrate the text-only OpenAI path away from `openai_provider_kit`
  unless required for this tool-capable slice.
- Do not migrate from Chat Completions to Responses API in this plan. The
  DecisionLog currently keeps tool-capable requests on Chat Completions.
- Do not add new tools beyond making the existing `recall_turn` path live under
  OpenAI.

## Architecture

Create a dedicated OpenAI tool-capable client module inside
`crates/qsf_app/src/models/`, for example:

- `crates/qsf_app/src/models/openai_tool_client.rs`

`OpenAiProviderModelClient::complete()` should route requests with non-empty
`request.tools` through this module. Text-only requests can continue through the
existing `openai_provider_kit` path until the kit grows native tool support or a
later plan replaces it.

Provider parsing only creates `ModelResponse` and `ModelToolCall` values. It
must not execute tools or mutate runtime state. Execution remains:

`ModelToolCall` -> `dispatch_model_tool_calls` -> `ToolRegistry` -> `ToolResult`
-> `ModelMessage::tool(...)` follow-up.

## Phase 1: Preserve tool-call ids in model messages

**Goal:** Make provider-agnostic model messages capable of carrying the OpenAI
`tool_call_id` required for tool-result messages.

**Likely files:**

- `crates/qsf_app/src/models/model_client.rs`
- `crates/qsf_app/src/conversation/prompt.rs`
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

**Steps:**

1. Add the smallest provider-agnostic representation needed for tool-result
   linkage. Options include `ModelMessage { tool_call_id: Option<String> }` or
   a sibling tool-message struct. Prefer the smallest change consistent with
   existing serialization tests.
2. Update `ModelMessage::tool(...)` or add a new constructor so call sites can
   preserve the original `ModelToolCall.call_id`.
3. Update prompt/tool-message construction in the multi-turn loop to include
   the call id when appending tool results.
4. Add tests proving tool messages preserve `tool_call_id` without changing
   normal system/user/assistant messages.

**Verification:**

```powershell
cargo test -p qsf_app models::
cargo test -p qsf_app multi_turn_text_loop
```

## Phase 2: Serialize tool-capable OpenAI requests

**Goal:** Send OpenAI Chat Completions requests directly from `qsf_app` when
`ModelRequest.tools` is non-empty.

**Likely files:**

- `crates/qsf_app/src/models/openai_provider.rs`
- `crates/qsf_app/src/models/openai_tool_client.rs`
- `crates/qsf_app/src/models/mod.rs`

**Steps:**

1. Add request serialization for `messages`, `model`, `temperature`,
   max-output-token setting, response format, and `tools`.
2. Convert each `ModelToolDefinition` into:

   ```json
   {
     "type": "function",
     "function": {
       "name": "...",
       "description": "...",
       "parameters": {}
     }
   }
   ```

3. Serialize tool-result messages as:

   ```json
   {
     "role": "tool",
     "tool_call_id": "...",
     "content": "..."
   }
   ```

4. Keep text-only requests on the existing kit path.

**Tests:**

- Text-only OpenAI requests omit `tools`.
- `recall_turn` requests emit the expected function-tool schema.
- Tool-result messages include `tool_call_id`.
- Existing temperature, max-token, and response-format behavior is preserved.

**Verification:**

```powershell
cargo test -p qsf_app models::
cargo build -p qsf_app --features openai
```

## Phase 3: Parse OpenAI tool-call responses

**Goal:** Convert OpenAI Chat Completions responses into `ModelResponse` values
that preserve tool calls, finish reason, usage, and provider/model metadata.

**Likely files:**

- `crates/qsf_app/src/models/openai_tool_client.rs`
- `crates/qsf_app/src/models/model_client.rs`

**Steps:**

1. Parse normal text responses into the existing `ModelResponse::from_text`
   shape with usage and finish reason.
2. Parse provider function calls into `ModelToolCall`, preserving:
   - provider call id
   - function name
   - JSON arguments
3. Treat malformed function-call arguments as provider-response errors with
   sanitized messages. Do not silently fall back to text.
4. Fail clearly when OpenAI omits the tool-call id needed for follow-up tool
   messages.

**Tests:**

- Normal text response.
- One `recall_turn` tool call.
- Multiple tool calls, even if the experiment later rejects multi-round use.
- Malformed JSON arguments fail with a useful sanitized error.
- Missing tool-call id fails.
- Usage and cached-token fields still parse.

**Verification:**

```powershell
cargo test -p qsf_app models::
cargo build -p qsf_app --features openai
```

## Phase 4: Wire the multi-turn OpenAI recall path

**Goal:** Make live OpenAI recall follow the same two-call loop as the mock
model: first response requests `recall_turn`, QSF dispatches through the
registry, and the follow-up request sends the tool result with the original
call id.

**Likely files:**

- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
- `crates/qsf_app/src/models/model_client.rs`
- `crates/qsf_app/src/models/openai_tool_client.rs`

**Steps:**

1. Ensure the first conversational request advertises tools from
   `registry.model_tool_definitions_for(&role.allowed_tools)`.
2. Preserve the original `ModelToolCall.call_id` when creating follow-up tool
   messages.
3. Do not advertise tools again on the follow-up unless multi-round tool calls
   are deliberately supported.
4. Keep the existing guard that fails if the follow-up returns additional tool
   calls.

**Tests:**

- First mocked OpenAI response returns `finish_reason=tool_calls`.
- QSF dispatches through `ToolRegistry`.
- `ToolRequested` and `ToolCompleted` include `category=compute_only` and
  `side_effect_level=none`.
- A second `PromptAssembled` happens after the tool message is appended.
- Follow-up request sends a provider-native tool message with the same
  `tool_call_id`.

**Verification:**

```powershell
cargo test -p qsf_app multi_turn_text_loop
cargo test -p qsf_app models::
```

## Phase 5: Documentation and final verification

**Goal:** Record the implementation and run automated plus live checks.

**Files:**

- `docs/EngineeringDiary.md`
- `docs/DecisionLog.md` only if the implementation creates a durable new rule
  beyond the existing Chat Completions and registry-boundary decisions.

**Verification commands:**

```powershell
cargo test -p qsf_app models::
cargo test -p qsf_app multi_turn_text_loop
cargo build -p qsf_app --features openai
cargo clippy --all-targets -- -D warnings
cargo fmt
```

## Live Smoke Test

```powershell
$env:OPENAI_API_KEY = "<key>"
$env:QSF_MODEL_PROVIDER = "openai"
$env:QSF_CONVERSATION_MODEL = "gpt-5.4-mini"
$env:QSF_SESSION_WARM_THRESHOLD = "2"
cargo run -p qsf_app --features openai -- experiment multi-turn-text-loop
```

Suggested prompts:

```text
one
two
three
Use the recall_turn tool with turn_id 0. I need the exact verbatim original user and assistant text, not the summary.
:quit
```

Live success criteria:

- `ModelRoleRequested` records the `recall_turn` tool definition.
- OpenAI returns `finish_reason=tool_calls` and a non-empty `tool_calls` array.
- `ToolRequested` and `ToolCompleted` appear for `recall_turn`.
- Both tool events include `category=compute_only` and
  `side_effect_level=none`.
- A second `PromptAssembled` appears after the tool result message is added.
- The follow-up OpenAI request contains a provider-native tool message with the
  same `tool_call_id`.
- The final `TurnCompleted` includes a non-empty `recalled_turns` list with
  verbatim `[Turn 0]` text.
- The generated report records `Recall tool executions: 1`.

If OpenAI still returns plain text with no tool call after the provider path is
implemented, record that as model behavior, not as proof that the provider path
is broken. Inspect the raw request/response trace first, then consider stronger
tool descriptions, a tool-choice setting, or a different model.

## Open Questions

- Should the OpenAI tool-capable path force `tool_choice` for the recall smoke
  test, or should it initially rely on model choice?
- Should malformed provider tool-call arguments fail the whole model call or be
  surfaced as a `ToolFailed` event? Current recommendation: fail provider
  parsing before dispatch, because no valid `ToolRequest` exists yet.
- Should the text-only OpenAI kit path eventually be replaced by the new direct
  serializer for consistency? Out of scope for this plan unless duplication
  becomes unmanageable.
