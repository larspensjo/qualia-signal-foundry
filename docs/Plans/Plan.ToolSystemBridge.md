# Plan: Bridge the Tool Registry and the Model Tool-Call Protocol

## Status

Draft. **OQ1 resolved: single `Tool` trait with typed `ToolContext`
accessors for borrowed runtime state**. All phases are now actionable. Phases 1
and 2 are independently landable; Phase 3 implements the trait change locked in
here; Phases 4 and 5 follow.

### Supersedes prior decision on `allowed_tools`

The 2026-05-17 DecisionLog entry *"allowed_tools on ModelRole is removed as
unenforced"* is **superseded by this plan and must be reversed in Phase 5**.
Reasons:

- The decision was recorded but never executed: `ModelRole::allowed_tools`
  still exists (`crates/qsf_app/src/models/model_role.rs:50`) and is still
  set by `conversational_responder_role_with_recall_tool()`
  (`crates/qsf_app/src/experiments/multi_turn_text_loop.rs:599`). Per the
  DecisionLog's own contract, an unimplemented decision can be reversed by a
  later entry that references it.
- The entry's own Consequences clause explicitly anticipates this plan's
  direction: *"If enforcement is added later, it belongs in `invoke_model_role`
  or the provider adapter, not as a passive annotation."* Phase 4 enforces at
  exactly that boundary (`models/tool_dispatch.rs`), so the original entry's
  guidance is honored, not contradicted, by enforcement.
- The removal reasoning was scoped to the multi-turn loop in isolation, where
  the field genuinely had no reader. The bridge plan finds the field is the
  right place to express *"what this role is permitted to ask for"* — a
  declarative, role-level concern that is distinct from `with_tools()`, which
  is a per-request builder. Removing `allowed_tools` would force every caller
  to duplicate role↔tool knowledge at the call site.

Phase 5 must add a reversal DecisionLog entry referencing the 2026-05-17
removal entry, per the *"Reversals of prior decisions get their own entry
referencing the original"* convention in `docs/DecisionLog.md`.

## Purpose

QSF currently has two parallel "tool" surfaces that do not meet:

- **Registry layer** (`crates/qsf_app/src/tools/`): a `Tool` trait, `ToolRegistry`,
  structured `ToolRequest`/`ToolResult`, and a `ToolPermission` model with
  `ToolCategory` and `ToolSideEffectLevel`. Today this holds exactly one tool
  (`CalculatorTool`) and is consumed only by `tool_as_perception_calculator.rs`.
  Execution is code-initiated: QSF builds a request, the registry validates
  permissions, dispatch happens, the result becomes a `ContextFragment`.
  Lifecycle events: `ToolRequested` → `ToolCompleted` / `ToolFailed`.

- **Model-call protocol layer** (`crates/qsf_app/src/models/model_client.rs`):
  `ModelToolDefinition` (JSON Schema sent to the provider), `ModelToolCall`
  (what the model emits), and `ModelMessageRole::Tool` (how results return as
  chat messages). Today this holds exactly one tool (`recall_turn`) and is
  used only by `multi_turn_text_loop.rs`. Execution is model-initiated:
  the provider returns `tool_calls`, the experiment dispatches them inline,
  results are fed back as `ModelMessage::tool(...)` for a follow-up call.
  Lifecycle events: `ToolRequested` → `ToolExecuted` / `ToolFailed`.

These layers are not redundant — they describe orthogonal concerns:

- Registry = **internal capability boundary**: what can run, with what side
  effects, requested by whom, and how the observation enters context.
- Model-call protocol = **LLM wire shape**: what tools the model sees, how it
  calls them, how the result is serialized as a chat message.

The problem is that they are not bridged. As a result:

1. `recall_turn` bypasses the registry entirely — no `ToolMetadata`, no
   `ToolPermission`, no `ToolCategory`, no `ToolResult`. Its category and
   side-effect class are implicit in code.
2. `CalculatorTool` has no `ModelToolDefinition`, so the model cannot call it,
   and the registry-side abstraction has only one in-process consumer.
3. `ModelRole.allowed_tools` is declared on the role and set by the multi-turn
   loop, but the field is never read anywhere — there is no enforcement site.
4. Event vocabulary collides: `ToolCompleted` (registry success) and
   `ToolExecuted` (model-call success) describe the same lifecycle moment but
   are emitted at different code sites with no documented difference.

The 2026-05-14 DecisionLog entry ("Realtime voice providers cannot execute tools
directly") already commits to converting provider tool-call requests into QSF
tool events before any tool can execute. That decision presumes a bridge that
does not yet exist.

## Goal

Make the registry the single execution boundary for tool calls. Make the
model-call protocol types a thin advertisement-and-marshalling layer that
routes through the registry. Make `ModelRole.allowed_tools` enforced. Make the
event vocabulary unambiguous.

## Non-Goals

- No new tools are added beyond `recall_turn` migrating into the registry.
- No changes to the model provider clients or wire-format JSON.
- No changes to memory promotion, retrieval, or the prompt-cache prefix
  invariants from the Multi-Turn Text Loop plan.
- No introspection or self-reflection tools — that work is tracked separately
  in `Idea.SelfReflectionProjectIntrospection.md` and inherits from this plan.

## Architecture

After this plan, every model-callable tool has:

- A `Tool` implementation in `crates/qsf_app/src/tools/` providing metadata,
  permission category, side-effect class, and `ToolResult` construction.
- A `ModelToolDefinition` derived from that same implementation, so the JSON
  Schema and the registered tool name cannot drift.
- Dispatch through `ToolRegistry`, with `ModelRole.allowed_tools` enforced at
  the model-call site before any registry lookup.
- One pair of lifecycle events used consistently across the codebase.

`recall_turn` is the first tool that exercises this bridge end-to-end. The
calculator gains a `ModelToolDefinition` as a free side-effect, but does not
need to be exposed to any real role yet.

## Open Design Questions

### 1. Tool trait shape for tools that need session state (RESOLVED — single trait with typed context accessors)

**Decision:** Single `Tool` trait with a `ctx: &dyn ToolContext` parameter on
`execute`. Stateful tools read borrowed runtime state through typed accessors on
`ToolContext` (currently `session_state()`); stateless tools ignore it. Decided
2026-05-17; amended during Phase 3 review after `std::any::Any` downcasting
proved incompatible with borrowed session contexts because `Any` requires
`'static`.

`CalculatorTool::execute(&self, request: &ToolRequest)` is stateless.
`recall_turn` reads `SessionState` to look up a summarized turn. The options
below remain in the plan as a record of the alternatives considered.

**Option A — Context parameter on `Tool`, typed accessors for borrowed state.**

```rust
pub trait ToolContext {
    fn session_state(&self) -> Option<&SessionState> {
        None
    }
}

pub trait Tool {
    fn metadata(&self) -> ToolMetadata;
    fn execute(&self, request: &ToolRequest, ctx: &dyn ToolContext)
        -> Result<ToolResult>;
}
```

Stateless tools accept any `ctx` and ignore it. Stateful tools use the accessor
for the state they need and bail with a typed error if the context does not
provide it.

- ✅ One trait, one registry, one dispatch path.
- ✅ Stateless tools cost nothing (ignore the parameter).
- ✅ Borrowed runtime state can pass through the context without forcing
      `Arc`/`RwLock` ownership changes.
- ❌ Every call site must construct a `ToolContext`, even when nothing needs it.
- ❌ New runtime state kinds require deliberate `ToolContext` accessor additions.

**Option B — Two traits, two dispatch paths on the registry.**

```rust
pub trait StatelessTool {
    fn metadata(&self) -> ToolMetadata;
    fn execute(&self, request: &ToolRequest) -> Result<ToolResult>;
}

pub trait SessionTool {
    fn metadata(&self) -> ToolMetadata;
    fn execute(&self, request: &ToolRequest, state: &SessionState)
        -> Result<ToolResult>;
}

impl ToolRegistry {
    pub fn execute_stateless(...) -> Result<...>;
    pub fn execute_session(..., state: &SessionState) -> Result<...>;
}
```

- ✅ Type-checked at compile time.
- ✅ Calculator-style tools stay simple.
- ❌ Two registries to keep in sync (or one registry that internally branches
      on tool kind). Tool kind has to be visible in the metadata so a
      `ModelToolCall` can be routed to the right dispatch.
- ❌ Adding a tool that needs `MemoryStore` later means a third trait.

**Option C — Tools own their state via construction.**

The registry is built per-experiment, and stateful tools are constructed with
the references they need (e.g. `RecallTurnTool::new(state_handle)`).

- ✅ Single `Tool` trait stays as-is.
- ❌ `SessionState` mutates turn-by-turn. Sharing it through `Arc<RwLock<_>>`
      breaks the existing single-owner reducer invariant. Sharing a snapshot
      means tools see stale state.
- ❌ Conflicts with the established unidirectional input → action → reducer →
      state flow.

**Option D — Carry state through the request.**

`ToolRequest` gains a typed payload (`enum ToolRequestPayload { Plain(String),
WithSessionState { input: String, state: &SessionState } }` or similar).

- ❌ Couples the request type to every future state shape.
- ❌ Mixes "what the model asked for" with "what the runtime has to offer".

**Chosen:** Option A as amended. The boundary stays single, and extending
`ToolContext` with explicit borrowed-state accessors is the honest shape of
"more subsystems will eventually need tool access" (memory store, goal system,
introspection adapter). The original downcast sketch is rejected for borrowed
contexts because `Any` requires `'static`.

### 2. Where is `allowed_tools` enforced?

Two reasonable sites:

- **At the registry** (`ToolRegistry::validate_and_execute` learns about the
  caller's role). Symmetric with `ToolPermission` but couples the registry
  to model roles.
- **At the model-call site** (a new helper in `crate::models` or in the
  experiment runtime) filters `ModelToolCall`s against `request.role.allowed_tools`
  before handing off to the registry.

**Recommendation:** Enforce at the model-call site. Keep the registry agnostic
of role identity. Roles are a model-call concept; the registry already has its
own permission model that is composable with the role allow-list (both must
permit the call).

### 3. Which event name survives?

`ToolCompleted` and `ToolExecuted` both mean "tool succeeded". Pick one and
remove the other. `ToolCompleted` mirrors `ModelRoleCompleted`,
`SleepPhaseCompleted`, `SessionCompleted`, `RealtimeResponseCompleted` — the
codebase already uses `*Completed` as the canonical success suffix.

**Recommendation:** Keep `ToolCompleted`; remove `ToolExecuted`. The multi-turn
loop switches over in Phase 1.

## Invariants This Plan Must Preserve

- Reducers stay pure. Tool dispatch emits events; `SessionState` mutations
  still flow through `SessionEvent` reduction.
- The prompt-cache-prefix invariants from `Plan.MultiTurnTextLoop.md` continue
  to hold. The tool-call path already emits a second `PromptAssembled`
  after the tool message is appended; that stays.
- `ToolPermission` remains the authoritative side-effect gate. Adding a role
  allow-list is an *additional* requirement, not a replacement.
- Stateless tools (today: `CalculatorTool`) continue to be usable without any
  session or runtime context.

## Phase 1: Reconcile Tool lifecycle events (no behavior change)

**Goal:** Make `ToolCompleted` the canonical success event and remove
`ToolExecuted` from the codebase. This is a mechanical rename with no
behavioral effect — it just clears the naming collision before any logic
moves.

**Files:**

- Modify `crates/qsf_app/src/observability/event_log.rs`: remove
  `EventType::ToolExecuted` variant.
- Modify `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`: change
  the two emission sites (`ToolExecuted` for `recall_turn` success, currently
  at ~line 987 and ~line 1735) and the test assertions (~line 1735, 1781) to
  use `ToolCompleted`.
- Modify any other emission sites Grep surfaces. As of this writing, the only
  emitter outside the multi-turn loop is the loop's own
  `apply_tool_executed_event` helper — rename it to `apply_tool_completed_event`.

**Steps:**

1. Run `Grep` for `ToolExecuted` across the workspace. Confirm the emitter
   list matches the files above.
2. Update the assertion-side test first (red): the existing test at
   `multi_turn_text_loop.rs:1734` asserts `events.contains("ToolExecuted")`.
   Change that assertion to `ToolCompleted`. Run `cargo test`. Expect the
   test to fail with the literal string mismatch (because the emitter still
   writes `ToolExecuted`).
3. Update the emitter to write `EventType::ToolCompleted` at the two sites.
   Rename `apply_tool_executed_event` to `apply_tool_completed_event` and
   update its single call site. Run `cargo test`. Expect pass.
4. Remove `EventType::ToolExecuted` from the enum. Run `cargo build`.
   Expect pass (no remaining references).
5. `cargo clippy --all-targets -- -D warnings && cargo fmt`.
6. Commit: `refactor: collapse ToolExecuted into ToolCompleted`.

**Verification:**

- `cargo test -p qsf_app` passes.
- Grep for `ToolExecuted` returns zero matches.
- No `runs/*` artifacts under the workspace currently depend on the string
  (existing JSONL is historical; the live event log starts fresh per run).

**Human-testing note:** None required. Pure rename.

## Phase 2: Expose `ModelToolDefinition` from registered tools

**Goal:** Give the `Tool` trait an optional method that returns its
model-facing schema. This adds capability without changing dispatch or
moving any tool yet. The calculator gains a schema as proof-of-concept; the
schema is not advertised to any role.

**Files:**

- Modify `crates/qsf_app/src/tools/tool_registry.rs`: add a new method
  `fn model_tool_definition(&self) -> Option<ModelToolDefinition> { None }`
  to the `Tool` trait, with a `None` default so existing impls do not break.
  Add `use crate::models::model_client::ModelToolDefinition;` at the top of
  the file (the registry currently has no `models` dependency).
- Modify `crates/qsf_app/src/tools/calculator_tool.rs`: override
  `model_tool_definition` to return a JSON-Schema definition matching the
  existing parser. Keep the schema strictly arithmetic (one string field
  `expression`). Add the same `ModelToolDefinition` import here.
- Modify `crates/qsf_app/src/tools/mod.rs` if a re-export is needed so the
  registry can hand back definitions by name.
- Add `ToolRegistry::model_tool_definitions_for(&self, names: &[&str]) ->
  Vec<ModelToolDefinition>` that looks up the named tools and collects
  whatever definitions exist. Used in Phase 4 by the model-call site.

**Steps:**

1. Add a failing test in `tool_registry.rs`'s `#[cfg(test)]` block:

   ```rust
   #[test]
   fn calculator_exposes_model_tool_definition() {
       let registry = ToolRegistry::default();
       let definitions = registry.model_tool_definitions_for(
           &[crate::tools::CALCULATOR_TOOL_NAME],
       );
       assert_eq!(definitions.len(), 1);
       assert_eq!(definitions[0].name, crate::tools::CALCULATOR_TOOL_NAME);
       assert!(definitions[0].parameters.get("properties").is_some());
   }
   ```

   Run `cargo test`. Expect compile failure (method does not exist).
2. Add the `Tool::model_tool_definition` method with `None` default and the
   `ToolRegistry::model_tool_definitions_for` helper. Run `cargo test`. Expect
   the new test to fail on `definitions.len()` (calculator still returns
   `None`).
3. Override `model_tool_definition` on `CalculatorTool` with the JSON Schema:

   ```rust
   fn model_tool_definition(&self) -> Option<ModelToolDefinition> {
       Some(ModelToolDefinition::new(
           CALCULATOR_TOOL_NAME,
           "Evaluate a deterministic arithmetic expression and return the numeric result.",
           serde_json::json!({
               "type": "object",
               "properties": {
                   "expression": {
                       "type": "string",
                       "description": "Arithmetic expression with +, -, *, /, parentheses, and decimal numbers."
                   }
               },
               "required": ["expression"],
               "additionalProperties": false
           }),
       ))
   }
   ```

   Run `cargo test`. Expect pass.
4. `cargo clippy --all-targets -- -D warnings && cargo fmt`.
5. Commit: `feat(tools): expose ModelToolDefinition from Tool trait`.

**Verification:**

- New unit test passes.
- `tool_as_perception_calculator.rs` continues to pass (calculator's
  `execute` is unchanged).
- No new public types beyond the trait method and registry helper.

**Human-testing note:** None required. The calculator schema is unreachable
from any model role until Phase 4 enables a role that lists it.

## Phase 3: Migrate `recall_turn` into the registry

**Goal:** Replace the inline `execute_recall_turn` / `execute_recall_tool_calls`
implementation in `multi_turn_text_loop.rs` with a `Tool` impl that lives in
`crates/qsf_app/src/tools/recall_turn_tool.rs`, registered in `ToolRegistry`,
dispatched through `validate_and_execute`, and exposing its `ModelToolDefinition`
via the Phase 2 hook.

Implements Option A as amended from OQ1 (single trait + typed `ToolContext`
accessors).

**Files:**

- Modify `crates/qsf_app/src/tools/tool_registry.rs`: add `ToolContext` trait,
  change `Tool::execute` signature to take
  `ctx: &dyn ToolContext`, update `ToolRegistry::dispatch`,
  `validate_and_execute`, and `execute` to thread the context through. Define
  `EmptyToolContext` in this file as the canonical no-state context for
  stateless tools and tests.
- Modify `crates/qsf_app/src/tools/calculator_tool.rs`: update the signature;
  the implementation ignores `ctx`.
- Create `crates/qsf_app/src/tools/recall_turn_tool.rs`: new `RecallTurnTool`
  with metadata (`category: ComputeOnly`, `side_effect_level: None`), a
  `ModelToolDefinition` mirroring the current schema, and an `execute` that
  reads `SessionState` from `ctx.session_state()`.
- Define `SessionToolContext` in `crates/qsf_app/src/tools/recall_turn_tool.rs`
  (alongside `RecallTurnTool`), not in `multi_turn_text_loop.rs`. The
  experiment imports it; the tool reads session state from it. Keeping the type in the
  tools module avoids a `tools -> experiments` back-dependency. Using a
  shared `SessionToolContext` (rather than a per-tool `RecallTurnContext`)
  means future session-aware tools reuse the same context type instead of
  defining a new one each time.
- Modify `crates/qsf_app/src/tools/mod.rs`: register the new module, re-export
  `RECALL_TURN_TOOL_NAME`, `RecallTurnTool`, and `SessionToolContext`.
- Modify `crates/qsf_app/src/tools/tool_registry.rs`: extend `ToolRegistry`'s
  struct fields to hold a `RecallTurnTool`, update `Default` to construct it,
  and route `RECALL_TURN_TOOL_NAME` from `metadata_for` and `dispatch` to it.
- Modify `crates/qsf_app/src/tools/tool_request.rs`: add a sibling field
  `structured: Option<serde_json::Value>` to `ToolRequest`. **Sibling chosen
  over a wrapper `ToolInput` enum** — the wrapper would force every existing
  construction site (including the unit test in this file and the
  tool-as-perception experiment) to change, and `ToolRequest` already has a
  flat shape that a sibling field reads naturally. The `input: String` field
  keeps its current semantics (human-readable summary or expression string);
  `structured` carries the JSON object the model sent for tools that need
  typed arguments. Add a `ToolRequest::recall_turn(call_id, turn_id,
  requested_by)` constructor that populates both fields. The calculator and
  every other current caller ignore `structured`; `RecallTurnTool::execute`
  reads `turn_id` from it.
- Modify `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`: delete
  `execute_recall_turn` and `recall_turn_tool_definition`; rewrite
  `execute_recall_tool_calls` to:
  1. Build a `ToolRequest` from the `ModelToolCall`.
  2. Build a `RecallTurnContext` wrapping `&SessionState`.
  3. Call `registry.validate_and_execute(&request, &ctx)`.
  4. Convert the returned `ToolResult` into the existing `RecallRecord` and
     `SessionEvent::ToolCompleted(...)` (renamed per Phase 1).

**Steps:**

1. Write the failing integration test first: copy the spirit of the existing
   `multi_turn_text_loop.rs:1734` test (which asserts the `ToolRequested` /
   `ToolCompleted` event pair after a `recall_turn` call), but additionally
   assert that the event payload now includes the registry-recorded
   `category` and `side_effect_level` fields. Run `cargo test`. Expect fail.
2. Add the `ToolContext` trait, change `Tool::execute` signature, update
   `CalculatorTool` to match the new signature. Run `cargo build`. Expect
   build failure in `tool_as_perception_calculator.rs` and in the
   `#[cfg(test)] mod tests` block at the bottom of `tool_registry.rs`
   (`registry.validate_request(&request)` no longer compiles unchanged once
   `validate_and_execute` also threads context — the test that builds a
   request and calls into the registry must pass an `EmptyToolContext`).
3. Update `tool_as_perception_calculator.rs` and the unit test in
   `tool_registry.rs` to pass `EmptyToolContext`. Run `cargo test
   tool_as_perception` and `cargo test -p qsf_app tools::`. Expect pass.
4. Extend `ToolRequest` with a structured-argument field. Add
   `ToolRequest::recall_turn(call_id, turn_id, requested_by)`. Run
   `cargo build`. Expect pass.
5. Create `recall_turn_tool.rs` with the `Tool` impl, metadata,
   `ModelToolDefinition`, and `execute`. Register it in `ToolRegistry`.
   Run `cargo test`. Expect the new test to fail on the event-payload
   assertions because the experiment still dispatches inline.
6. Rewrite `execute_recall_tool_calls` in `multi_turn_text_loop.rs` to go
   through the registry. Delete the inline `execute_recall_turn` and the
   inline `recall_turn_tool_definition`. The model request now sources its
   definition from `registry.model_tool_definitions_for(&role.allowed_tools)`.
   Run `cargo test`. Expect pass.
7. Verify the full multi-turn loop test suite still passes, especially the
   prompt-cache-prefix tests and the recall-replay tests around
   `multi_turn_text_loop.rs:1976`.
8. `cargo clippy --all-targets -- -D warnings && cargo fmt`.
9. Commit: `refactor(tools): route recall_turn through ToolRegistry`.

**Verification:**

- `cargo test -p qsf_app` passes.
- A live `multi-turn-text-loop` run (using the mock model client first, then
  the OpenAI path) still produces an identical sequence of events, with
  `ToolRequested`/`ToolCompleted` payloads now including `category=compute_only`
  and `side_effect_level=none`.
- `recall_turn_tool_definition()` is gone from `multi_turn_text_loop.rs`.

**Human-testing note (recommended):** Run one mock-model multi-turn session
and one live OpenAI multi-turn session, both long enough to trigger warm-tier
summarization and a real `recall_turn` call. Compare the `events.jsonl` and
`traces.jsonl` against a recent baseline run (e.g.
`runs/2026-05-17-061331-multi-turn-text-loop`).

The invariant is scoped to **event type sequence and key payload fields**, not
byte-equality. Allowed differences:

- `ToolExecuted` → `ToolCompleted` (Phase 1).
- New `category` and `side_effect_level` fields in `ToolRequested` and
  `ToolCompleted` payloads (Phase 3).
- Timing fields (`latency_ms`, monotonic timestamps, anything wall-clock):
  the tool now executes inside `validate_and_execute` rather than inline, so
  exact timings will shift even though the work is the same. Compare ranges,
  not equality.

Anything other than the three categories above is a regression.

## Phase 4: Enforce `ModelRole.allowed_tools` at the model-call site

**Goal:** Make the unused `ModelRole.allowed_tools` field load-bearing.
Reject `ModelToolCall`s for tools the role does not list. Reject sending
tool *definitions* the role does not list.

### Relationship between `allowed_tools` and `with_tools()`

After Phase 4, `ModelRole.allowed_tools` is the **authoritative declaration**
of what a role may call. `ModelRequest::with_tools()` remains a public builder
but is **derived state**: every production call site should populate it via
`registry.model_tool_definitions_for(&role.allowed_tools)`, not with a
hand-rolled `Vec<ModelToolDefinition>`.

The plan keeps `with_tools()` (rather than removing the parameter and having
`ModelRequest` reach into the registry on construction) for three reasons:

- It avoids a `models -> tools` dependency edge. Today `models` does not depend
  on `tools`; adding that edge would couple the wire-format layer to the
  execution layer, the opposite of this plan's separation-of-concerns goal.
- Tests and ad-hoc construction sites should be able to build a `ModelRequest`
  without a `ToolRegistry` in hand.
- The realtime voice path (2026-05-14 entry) and any future provider adapter
  will continue to need a way to attach a tool list to a request.

To keep the two surfaces in sync, Phase 4 adds a `debug_assert!` inside
`dispatch_model_tool_calls`: when the dispatcher receives a `ModelRequest`
(or, equivalently, the surrounding context's role and tool list), it asserts
that every name in `role.allowed_tools` appears in `request.tools` and
vice versa. The assertion fires only in debug builds and catches the
"caller passed an inconsistent list" mistake at the boundary; production
builds rely on the call-site discipline above. The reasoning is documented
in the rustdoc for `ModelRole::allowed_tools` so future contributors know the
field is canonical.

**Files:**

- Create `crates/qsf_app/src/models/tool_dispatch.rs`: a single helper
  `dispatch_model_tool_calls(context, registry, role, state_ctx, tool_calls)`
  that
  1. Filters each `ModelToolCall` against `role.allowed_tools`. Rejects
     unknown names with `ToolFailed` + a typed `anyhow` error.
  2. Builds the `ToolRequest` for each accepted call.
  3. Calls `registry.validate_and_execute(&request, state_ctx)`.
  4. Emits `ToolRequested` and `ToolCompleted`/`ToolFailed`.
  5. Returns `Vec<ToolResult>` so the experiment can convert into its own
     domain event (e.g. `SessionEvent::ToolCompleted(RecallRecord)` for the
     multi-turn loop).
- Modify `crates/qsf_app/src/models/mod.rs`: export the new helper.
- Modify `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`: replace
  the body of `execute_recall_tool_calls` with a call to the helper plus the
  `ToolResult → RecallRecord` conversion.
- Modify `crates/qsf_app/src/models/model_role.rs`: keep `allowed_tools` as
  the source of truth. Add a docstring stating that the field is enforced by
  `dispatch_model_tool_calls`.

**Steps:**

1. Add a failing test in `tool_dispatch.rs`: build a `ModelRole` with empty
   `allowed_tools`, feed in a `ModelToolCall` for `recall_turn`, assert that
   the dispatcher returns an error containing "not permitted for role" and
   emits `ToolFailed`. Run `cargo test`. Expect fail (helper not written).
2. Implement the helper. Run `cargo test`. Expect pass.
3. Add a second test: role allows `recall_turn`, registry contains
   `RecallTurnTool`, dispatcher succeeds and returns one `ToolResult`. Run
   `cargo test`. Expect pass.
4. Switch the multi-turn loop's `execute_recall_tool_calls` to call the
   helper. The `conversational_responder_role_with_recall_tool` builder stays
   as-is (it already sets `allowed_tools`). Run `cargo test`. Expect pass.
5. Confirm that the `ModelRequest`'s `tools` field is now populated by
   `registry.model_tool_definitions_for(&role.allowed_tools)` rather than the
   inline `vec![recall_turn_tool_definition()]`. Update the request-builder
   call site in `multi_turn_text_loop.rs:484`.
6. `cargo clippy --all-targets -- -D warnings && cargo fmt`.
7. Commit: `feat(tools): enforce ModelRole.allowed_tools at dispatch`.

**Verification:**

- `cargo test -p qsf_app` passes.
- A unit test confirms that an empty `allowed_tools` blocks every call.
- A unit test confirms that a tool present in `allowed_tools` but absent from
  the registry produces `ToolFailed` with an "unknown tool" message
  (registry-side validation).
- The multi-turn loop's published tool list (sent to OpenAI) matches the
  role's `allowed_tools` exactly.

**Human-testing note (recommended):** One short live OpenAI multi-turn run to
confirm the model still receives the `recall_turn` definition and that the
tool message comes back through the new dispatch path.

## Phase 5: DecisionLog entries

**Goal:** Record the durable rule produced by this plan and reverse the prior
decision that conflicts with it, so future work does not relitigate either.

**Files:**

- Modify `docs/DecisionLog.md`: append **two** entries dated to the day the
  plan lands.

  1. **Reversal entry** for the 2026-05-17 *"allowed_tools on ModelRole is
     removed as unenforced"* decision. Suggested topic: *allowed_tools is
     retained and enforced (reverses 2026-05-17 removal).* Body must
     explicitly reference the original entry and cite the reasons in the
     plan's "Supersedes prior decision on `allowed_tools`" preamble: the
     removal was never executed, its own Consequences clause endorses
     dispatch-boundary enforcement, and the field is the natural
     declarative source for "what this role may call."
  2. **Boundary entry** for this plan as a whole. Suggested topic: *Tool
     execution boundary is the ToolRegistry; model protocol types route
     through it.*
- Suggested body (paraphrased; tighten when writing):

  > Decision: All tool execution flows through `ToolRegistry`.
  > `ModelToolDefinition` and `ModelToolCall` describe the LLM-facing wire
  > shape only and must be marshalled into `ToolRequest` /`ToolResult` before
  > a tool runs. `ModelRole.allowed_tools` is the role-level allow-list; it
  > composes with `ToolPermission` (both must permit a call). Tool lifecycle
  > uses `ToolRequested` → `ToolCompleted` / `ToolFailed`.
  >
  > Context: Before this entry, `recall_turn` and `CalculatorTool` lived in
  > separate parallel surfaces with no bridge, `ModelRole.allowed_tools` was
  > unenforced, and `ToolCompleted`/`ToolExecuted` were synonymous. The
  > 2026-05-14 entry on realtime voice presupposed a bridge that did not
  > exist.
  >
  > Consequences: New tools land as `Tool` impls and gain a
  > `ModelToolDefinition` if they are model-callable. Roles that need a tool
  > list it in `allowed_tools`. Realtime voice providers (per 2026-05-14) can
  > now actually route through the boundary they were promised.
  >
  > Refs: `crates/qsf_app/src/tools/`, `crates/qsf_app/src/models/`,
  > `docs/Plans/Plan.ToolSystemBridge.md`.

**Steps:**

1. Confirm Phases 1–4 are merged.
2. Append the reversal entry first (it must precede or accompany the boundary
   entry so a reader following the log top-to-bottom sees the policy change
   before seeing the rule that depends on it). Then append the boundary
   entry. Commit: `docs: reverse allowed_tools removal and log tool-execution-boundary decision`.

**Verification:** None beyond the commit; the DecisionLog is itself the
verification artifact. Spot-check: a reader who lands on the 2026-05-17
removal entry should find the reversal in the log via a `Refs:` line on the
reversal pointing back to it.

## Out-of-Plan Follow-ups

These belong in later plans, not here:

- OpenAI tool-capable provider path. Phase 3's deterministic mock verification
  proves registry-backed `recall_turn` execution, but live OpenAI runs
  `runs/2026-05-17-113835-multi-turn-text-loop` and
  `runs/2026-05-17-114152-multi-turn-text-loop` show the current OpenAI adapter
  does not forward `ModelRequest.tools` to the provider or parse provider tool
  calls. QSF records the requested tools in `ModelRoleRequested`, but
  `openai_provider.rs` builds an `openai_provider_kit::LlmRequest` that has no
  tool field, maps `ModelMessageRole::Tool` back to `User`, and always returns
  text-only `ModelResponse`s. This needs its own implementation slice before
  live OpenAI recall execution can be verified.
- Realtime voice (2026-05-14 entry) growing a real execution path through the
  bridge. Currently `realtime_voice_session.rs:281–296` only records
  `ToolRequested` with `auto_executed: false`. Once this plan lands, that
  experiment can be updated to actually call `dispatch_model_tool_calls`
  with the realtime session's tool-call records.
- Documentation introspection tools per
  `Idea.SelfReflectionProjectIntrospection.md` — those will become real
  `Tool` impls.
- A `ToolContext` extension for memory-store access, once a memory-reading
  tool exists.

### Follow-up Plan: OpenAI tool-capable requests and responses

**Goal:** Make the OpenAI-backed `ModelClient` exercise the same
`ModelToolDefinition` → `ModelToolCall` → `ToolRegistry` → tool-result-message
path that the mock model already exercises.

**Recommended placement:** create a dedicated OpenAI tool-capable client module
inside `crates/qsf_app/src/models/` (for example
`openai_tool_client.rs`) and have `OpenAiProviderModelClient::complete()` route
requests with non-empty `request.tools` through it. Keep the existing
`openai_provider_kit` path for text-only requests until the kit grows native
tool support. This follows the existing DecisionLog entries that chose a
temporary Chat Completions bypass for tool-capable requests.

**Files likely touched:**

- `crates/qsf_app/src/models/openai_provider.rs`
- `crates/qsf_app/src/models/model_client.rs`
- `crates/qsf_app/src/models/mod.rs`
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`
- `docs/EngineeringDiary.md`
- A new unit-testable module such as
  `crates/qsf_app/src/models/openai_tool_client.rs`

**Implementation steps:**

1. Add provider-native tool-result linkage to the provider-agnostic model
   types. `ModelMessage::tool(...)` currently carries content only; OpenAI
   Chat Completions also requires `tool_call_id`. Add the smallest field or
   sibling constructor needed so QSF can preserve the call id returned by the
   provider and send the tool result back with the correct id.
2. Serialize tool-capable Chat Completions requests directly in `qsf_app`:
   `messages`, `model`, `temperature`, max output token setting, response
   format, and a `tools` array where each `ModelToolDefinition` becomes an
   OpenAI function tool:
   `{"type":"function","function":{"name","description","parameters"}}`.
   Omit the `tools` field for text-only requests.
3. Parse OpenAI tool-call responses into `ModelToolCall`. Preserve
   `finish_reason`, usage telemetry, provider/model names, and text output for
   normal text responses. Malformed function-call arguments should produce a
   sanitized provider-response error instead of silently falling back to text.
4. Serialize follow-up tool messages with provider-native shape:
   `{"role":"tool","tool_call_id":"...","content":"..."}`. Remove the current
   lossy fallback that maps `ModelMessageRole::Tool` to `ChatRole::User` for
   tool-capable requests. The text-only kit path may keep its existing role
   mapping until it is replaced.
5. Keep the registry boundary unchanged. Provider parsing should only create
   `ModelToolCall`s; execution still belongs to the Phase 3/4 dispatch path
   through `ToolRegistry`.
6. Update the multi-turn recall follow-up request to include provider-native
   tool-result messages with the original call id and to avoid advertising
   tools again on the follow-up unless multi-round tool calls are deliberately
   supported.
7. Document the change in `EngineeringDiary.md`. Add or update DecisionLog only
   if the implementation makes a durable new provider-boundary rule.

**Tests to add before live testing:**

1. Request serialization unit tests:
   - text-only OpenAI requests omit `tools`
   - `recall_turn` requests emit the expected function-tool schema
   - tool-result messages include `tool_call_id`
   - existing temperature, max-token, and response-format behavior stays intact
2. Response parsing unit tests with mocked OpenAI JSON:
   - normal text response
   - one `recall_turn` tool call
   - multiple tool calls, even if the experiment later rejects multi-round use
   - malformed JSON arguments fail with a useful sanitized error
   - missing tool-call id fails
   - usage and cached-token fields still parse
3. Multi-turn integration tests using a mocked OpenAI tool response:
   - first model response returns `finish_reason=tool_calls`
   - QSF dispatches through `ToolRegistry`
   - `ToolRequested` and `ToolCompleted` include `category=compute_only` and
     `side_effect_level=none`
   - second `PromptAssembled` happens after the tool message is appended
   - follow-up request sends a provider-native tool message with the same
     `tool_call_id`

**Verification commands:**

```powershell
cargo test -p qsf_app models::
cargo test -p qsf_app multi_turn_text_loop
cargo build -p qsf_app --features openai
cargo clippy --all-targets -- -D warnings
cargo fmt
```

**Live smoke test after implementation:**

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
- The final `TurnCompleted` includes a non-empty `recalled_turns` list with
  verbatim `[Turn 0]` text.
- The generated report records `Recall tool executions: 1`.

If OpenAI still returns plain text with no tool call after the provider path is
implemented, record that as model behavior, not as proof that the provider path
is broken. In that case inspect the raw request/response trace first, then
consider stronger tool descriptions, a tool-choice setting, or a different
model.

## Open Questions Snapshot

- **OQ1 — Tool trait shape.** Resolved 2026-05-17: single `Tool` trait with
  typed `ToolContext` accessors for borrowed runtime state.
- **OQ2 — Allow-list enforcement site.** Resolved 2026-05-17: model-call site
  (`tool_dispatch`). Registry stays role-agnostic. Phase 4 implements this.
- **OQ3 — Event survivor.** Resolved 2026-05-17: `ToolCompleted`. Phase 1
  commits this.
- **OQ4 — `ToolRequest` structured-argument shape.** Resolved 2026-05-17:
  sibling field `structured: Option<serde_json::Value>` on the existing
  flat `ToolRequest`. No `ToolInput` wrapper. Phase 3 implements this.
- **OQ5 — `with_tools()` / `allowed_tools` relationship.** Resolved
  2026-05-17: `allowed_tools` is authoritative; `with_tools()` is a derived
  builder populated at the call site from
  `registry.model_tool_definitions_for(&role.allowed_tools)`. A debug-only
  assertion in `dispatch_model_tool_calls` catches drift. Phase 4 implements
  this.
- **OQ6 — Reversal of the 2026-05-17 `allowed_tools` removal entry.**
  Resolved at plan level (see "Supersedes prior decision" above). Phase 5
  writes the reversal entry.

Mark each decision in `docs/DecisionLog.md` as it lands, with a reference back
to this plan.
