# Architecture: Tool System

## Maturity

Candidate

## Summary

The tool system defines how Qualia Signal Foundry can access external capabilities and information.

In this project, tools are primarily treated as **perception extensions**, not as general-purpose agency. A tool allows the simulated system to observe, inspect, calculate, search, or sense something outside the current model context. The early project should prioritize read-only tools and controlled execution environments.

The tool system is important because it connects the simulated mind to the world while preserving safety, observability, and research control.

## Purpose

The purpose of the tool system is to let the simulation access information that is not already in active context.

The system may eventually use tools for:

- calculation
- local file inspection
- web search
- document lookup
- code execution in a sandbox
- audio input
- video input
- environment sensing
- memory inspection
- experiment control
- internal state debugging

The early goal is not to create an autonomous agent that acts freely. The early goal is to provide controlled perception channels that can be studied and logged.

## Design Principle

The core principle is:

```text
Tools are perception before they are action.
```

This means the first tool layer should mostly answer questions such as:

- What can the system observe?
- What can the system inspect?
- What can the system measure?
- What can the system calculate?
- What can the system retrieve?

Capabilities that modify the outside world should be delayed, restricted, or placed behind explicit human approval.

## Tool Categories

### Read-Only Information Tools

These tools retrieve or inspect information.

Examples:

- web search
- document search
- file reading
- local project inspection
- log inspection
- calendar or email reading, if explicitly allowed in a future experiment
- memory lookup
- system state inspection

These are the most appropriate early tools.

### Computational Tools

These tools compute or transform information.

Examples:

- calculator
- small algorithm runner
- data summarizer
- local code execution in a sandbox
- parser
- tokenizer
- embedding generator
- graph analyzer

These can support reasoning without turning the simulation into an uncontrolled actor.

### Sensory Tools

These tools provide ongoing or event-based input streams.

Examples:

- microphone input
- speech-to-text
- camera input
- screen observation
- keyboard/mouse activity metadata
- room/environment sensors
- application state signals

These are especially relevant to simulated presence.

### Internal Introspection Tools

These tools expose the system's own state.

Examples:

- active context viewer
- memory graph viewer
- retrieved memory inspector
- attention/focus trace
- current model role trace
- sleep-phase change log
- latency and cost metrics
- prompt/context assembly trace

These are important because the project is a research platform.

### Write-Capable Tools

These tools modify external systems.

Examples:

- sending messages
- editing files
- posting online
- committing code
- purchasing items
- controlling applications
- contacting people
- changing user account state

These should not be part of the early system except in tightly controlled experiments.

## Initial Boundary

The early tool system should use this boundary:

```text
Allowed early:
  Read, inspect, calculate, retrieve, observe, summarize.

Delayed or restricted:
  Send, post, purchase, modify, control, delete, contact.
```

This boundary supports the non-goal of avoiding uncontrolled agency.

## Candidate Tool Flow

A possible tool-use flow is:

```text
Input event
  -> runtime loop detects need for external information
  -> tool request is proposed
  -> permission and policy checks run
  -> tool invocation is executed
  -> result is normalized
  -> result is logged
  -> relevant result enters context
  -> simulation continues
```

For some tools, especially real-time sensory tools, the flow may be event-driven rather than request-driven.

```text
External signal
  -> sensor adapter
  -> event normalization
  -> runtime state update
  -> optional context assembly
  -> model invocation if needed
```

## Tool Request

A tool request should be structured rather than just free text.

Candidate fields:

```text
ToolRequest
  tool_id
  requested_by_model_role
  purpose
  input_arguments
  expected_result_type
  urgency
  permission_level
  context_budget_hint
  trace_id
```

The purpose field is important. It helps the system and researcher understand why a tool was used.

## Tool Result

Tool results should be normalized before entering active context.

Candidate fields:

```text
ToolResult
  tool_id
  status
  result_summary
  structured_payload
  raw_payload_reference
  timestamp
  latency
  cost
  confidence
  errors
  trace_id
```

The active context should usually receive a compact result summary, not necessarily the full raw result.

## Tool Registry

The system should maintain a registry of available tools.

Candidate registry fields:

```text
ToolDefinition
  tool_id
  name
  description
  category
  input_schema
  output_schema
  permission_class
  latency_expectation
  cost_expectation
  side_effect_level
  sandbox_requirements
  allowed_model_roles
```

The tool registry should be inspectable.

The simulation should not need to infer tool behavior from vague text alone. Tool descriptions should be explicit enough to support safe and predictable selection.

## Permission Model

A simple permission model may be enough for early work.

Candidate permission classes:

```text
ObserveOnly
  Can read or inspect information.

ComputeOnly
  Can compute or transform information without external side effects.

SandboxedLocal
  Can execute code or inspect files inside a controlled local boundary.

HumanApprovedWrite
  Can modify external state only after explicit human approval.

Disallowed
  Not available to the simulation.
```

The early system should default to `ObserveOnly`, `ComputeOnly`, and carefully restricted `SandboxedLocal`.

## Side Effect Levels

Tools should declare their side effect level.

```text
None
  No external side effects.

LocalEphemeral
  Temporary local state only.

LocalPersistent
  Writes local files or persistent state.

ExternalRead
  Reads from an external service.

ExternalWrite
  Changes an external system.

HumanContact
  Sends messages or communicates with people.
```

This helps prevent accidental expansion from perception into agency.

## Model Role Access

Not every model role should have access to every tool.

Examples:

```text
Live interaction model
  May use fast read-only tools, memory lookup, and selected sensory events.

Memory extraction model
  May inspect recent transcript and memory state.

Sleep/consolidation model
  May use memory inspection, association tools, and summarization tools.

Research/planning model
  May inspect documentation and experiment logs.

Critic/reviewer model
  May inspect proposals, requirements, and traces.

Audio transcription model
  Should not need general tools.
```

Tool access should be role-specific and explicit.

## Real-Time Tool Constraints

The real-time loop has stricter constraints than offline processes.

A live interaction may need:

- predictable latency
- interrupt handling
- partial results
- cancellation
- small result summaries
- no long-running blocking calls
- fallback behavior if the tool fails

A sleep-phase or research process can use slower and deeper tools.

A useful distinction:

```text
Live tools:
  fast, small, interruptible, low-latency.

Offline tools:
  deeper, slower, broader, more analytical.
```

## Tool Results as Memory

Tool use can create memories.

Examples:

- the system searched for something important
- a file inspection revealed a project decision
- an audio event indicated user interruption
- a calculation corrected a mistaken assumption
- repeated tool failures indicate a design problem

The system should decide which tool results are worth storing.

Not every tool result should become long-term memory. Some results are transient observations.

## Observability

Tool use should be heavily logged.

The system should record:

- which tool was used
- why it was used
- which model role requested it
- input arguments
- permission checks
- latency
- cost
- result summary
- errors
- whether the result entered active context
- whether the result was stored as memory
- whether the result influenced an output

Tool traces are essential for debugging, safety, and research.

## Safety Boundaries

The tool system should support conservative defaults.

Important boundaries:

- tools should be opt-in
- write-capable tools should be disabled early
- external communication should require explicit approval
- local code execution should be sandboxed
- raw tool results should not automatically become trusted facts
- tool output should be treated as potentially incomplete or misleading
- tool failures should be visible
- tools should be cancellable when possible
- permissions should be explicit and inspectable

The system should avoid creating the illusion that a tool result is equivalent to certain knowledge.

## Candidate Implementation Shape

A possible implementation could include:

```text
ToolRegistry
  stores available tool definitions.

ToolPolicy
  checks whether a model role may use a tool.

ToolRequest
  structured request for tool invocation.

ToolExecutor
  invokes the tool through an adapter.

ToolAdapter
  tool-specific implementation boundary.

ToolResultNormalizer
  converts raw output into structured result and context summary.

ToolTrace
  records the full lifecycle of tool use.

ToolObservationEvent
  emits normalized events back into the runtime loop.
```

This is a candidate design, not a final implementation requirement.

## Tool Adapters

Each tool should be wrapped by an adapter.

The adapter should handle:

- input validation
- execution
- timeout
- cancellation
- raw result capture
- error normalization
- result summarization
- trace recording

Adapters prevent the rest of the system from depending on tool-specific details.

## Tool Output Trust

Tool results should carry a trust or confidence signal when possible.

Examples:

```text
High confidence:
  deterministic calculator result.

Medium confidence:
  local file inspection.

Variable confidence:
  web search result.

Low confidence:
  noisy speech transcription.

Experimental confidence:
  video or sensor interpretation.
```

The system should distinguish between direct observations, model interpretations, and externally sourced claims.

## Relationship to Context Management

The tool system does not decide by itself what enters the model context.

A tool may return a large result, but the context manager should decide what part is included.

Typical flow:

```text
ToolResult
  -> result summary
  -> candidate context fragment
  -> context ranking
  -> active context inclusion or omission
```

This prevents tool output from overwhelming the live context.

## Relationship to Memory

The memory system may store selected tool observations.

Possible memory records:

- observed fact
- retrieved document
- tool failure
- environment event
- user correction
- repeated signal
- experiment result

Memory storage should be deliberate. Raw tool output should usually be referenced, summarized, or discarded rather than blindly stored.

## Relationship to Audio and External Inputs

Audio can be understood as a special case of the tool system, but it may need dedicated architecture because it is continuous and latency-sensitive.

For example:

```text
Microphone input
  -> audio capture adapter
  -> speech-to-text tool
  -> normalized speech event
  -> runtime loop
```

Video, screen input, and other sensors may follow similar patterns.

## Relationship to Sleep Phase

The sleep phase may use tools differently from the live loop.

Examples:

- inspect session logs
- review memory changes
- run association analysis
- summarize events
- detect unresolved questions
- prepare context packs
- evaluate tool failure patterns

Sleep-phase tool use can be slower and broader because it does not need immediate response.

## Risks and Failure Modes

### Tool Overuse

The system may use tools too often, increasing cost and latency.

### Tool Underuse

The system may fail to inspect relevant external information and rely too much on stale memory.

### Perception-Agency Drift

Read-only tools may gradually expand into action tools without deliberate decisions.

### Context Flooding

Large tool outputs may overwhelm active context.

### False Authority

The simulation may treat tool results as more reliable than they are.

### Latency Breaks Presence

Slow tools may damage the feeling of real-time presence.

### Hidden Side Effects

A tool may appear read-only but have external logging, network, or account effects.

### Unsafe Local Execution

Code execution tools may accidentally access or modify more than intended.

### Ambiguous Tool Purpose

If the reason for a tool call is not logged, researchers may not understand system behavior.

## Open Questions

### RQ-Tool-Selection

How should the system decide when to use a tool rather than rely on memory or model reasoning?

### RQ-Tool-Permissions

What is the smallest useful permission model for early experiments?

### RQ-Tool-Latency

Which tools are acceptable in the live loop, and which should only be used offline?

### RQ-Tool-ResultTrust

How should tool result confidence be represented?

### RQ-Tool-MemoryPromotion

When should a tool result become a memory?

### RQ-Tool-PerceptionBoundary

Where is the boundary between perception and agency?

### RQ-Tool-HumanApproval

What kinds of tool actions should require explicit human approval?

## Possible Experiments

### Experiment: Read-Only Tool Loop

Give the simulation access to a small set of read-only tools and observe whether this improves continuity and relevance.

### Experiment: Tool Latency Impact

Measure how tool latency affects perceived real-time presence in audio interaction.

### Experiment: Tool Result Summarization

Compare raw tool output, compact summaries, and structured observations as context inputs.

### Experiment: Tool Trace Review

Review tool traces after an interaction to understand whether tool use was helpful, excessive, or misleading.

### Experiment: Memory Promotion from Tool Results

Test different rules for deciding which tool observations become long-term memories.

## Current Status

The tool system is considered a central architecture concern.

The current working assumption is that early tools should be read-only, explicit, logged, and treated as perception extensions. Write-capable tools and external communication should be delayed until the project has stronger safety boundaries and clearer research reasons for adding them.
