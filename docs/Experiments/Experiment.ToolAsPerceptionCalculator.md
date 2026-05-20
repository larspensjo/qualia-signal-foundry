# Experiment: Tool as Perception Calculator

## Experiment ID

`Experiment.ToolAsPerceptionCalculator`

## Status

Completed.

Implemented as the registered `tool-as-perception-calculator` experiment. The
experiment executes a deterministic compute-only calculator tool, records request,
permission, result, failure, and latency events, writes a tool invocation trace, and
turns the result into a tool-observation context fragment.

## Summary

This experiment tests a simple calculator tool represented as a perception extension rather than an action capability.

The goal is to create a minimal tool flow where the system identifies a need for calculation, creates a structured tool request, invokes a deterministic calculator, normalizes the result as an observation, and logs the full trace.

This is a deliberately simple experiment. A calculator is useful because the result is deterministic, easy to verify, and low-risk.

## Motivation

Qualia Signal Foundry treats tools primarily as perception extensions.

The system should be able to inspect, measure, calculate, and retrieve information without immediately becoming an uncontrolled agent. A calculator tool is a safe first example of this pattern.

This experiment reduces uncertainty around:

- how tool requests should be structured
- how tool results should be normalized
- how tool observations enter context
- how tool use should be logged
- how tool permission classes should work
- how to distinguish perception from agency

## Related Documents

```text
Concepts/Concept.ToolsAsPerception.md
Concepts/Concept.ExternalInputs.md
Architecture/Architecture.ToolSystem.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
```

## Hypothesis

A deterministic calculator tool can be integrated as a read-only perception extension using structured requests, normalized observations, and trace logging without introducing agency or unsafe side effects.

## Scope

### In Scope

- simple calculator tool
- structured tool request
- deterministic tool execution
- tool permission class
- normalized tool result
- tool trace
- result as context fragment
- error handling for invalid expressions
- simple manual test cases

### Out of Scope

- web search
- file modification
- external communication
- arbitrary unsandboxed code execution
- autonomous tool chaining
- write-capable tools
- complex symbolic mathematics
- tool marketplace/plugin system
- production permission UI

## Setup

Define a minimal tool registry entry:

```text
tool_id: calculator
category: ComputeOnly
side_effect_level: None
allowed_roles: selected test role
input_schema: expression
output_schema: numeric result or error
```

Define a structured request:

```text
ToolRequest
  tool_id
  requested_by_model_role
  purpose
  input_arguments
  permission_level
  trace_id
```

Define a normalized result:

```text
ToolResult
  tool_id
  status
  result_summary
  structured_payload
  latency
  confidence
  errors
  trace_id
```

## Procedure

1. Define a small calculator tool registry entry.
2. Create several test inputs requiring calculation.
3. Create structured tool requests.
4. Run permission checks.
5. Invoke the calculator.
6. Normalize the result.
7. Emit a tool observation event.
8. Add the result as a candidate context fragment.
9. Log the full trace.
10. Test invalid expressions and failure handling.
11. Review whether the result behaves like perception, not agency.

## Baseline

Baseline:

```text
Model-only arithmetic without tool use.
```

Optional comparison:

```text
Manual calculation outside the system.
```

The main comparison is not raw arithmetic ability. The main comparison is whether structured tool use creates clearer and more reliable behavior.

## Measurements

### Quantitative Measurements

- number of successful tool calls
- number of failed tool calls
- latency per tool call
- number of invalid requests caught
- number of permission checks passed/failed
- number of result fragments added to context

### Qualitative Observations

- clarity of tool request
- clarity of tool result
- usefulness of tool trace
- whether tool use feels controlled
- whether result normalization is sufficient
- whether the tool boundary is understandable
- whether the system avoids treating the tool as agency

## Success Criteria

The experiment is successful if:

- calculator requests are structured
- permission checks are explicit
- results are normalized
- tool traces are inspectable
- invalid inputs are handled
- the result can enter context as an observation
- the tool has no external side effects
- the experiment clarifies the first tool-system interface

## Failure Criteria

The experiment is inconclusive if:

- tool requests remain free-form and hard to inspect
- result format is unclear
- errors are not logged
- permissions are implicit
- the tool result bypasses context management
- the implementation overgeneralizes too early

## Required Observability

The experiment should log:

- tool request
- requesting role
- purpose
- permission check
- input expression
- execution status
- result summary
- structured result
- latency
- errors
- whether the result entered context
- trace ID

## Risks and Confounders

- calculator is too simple to reveal real tool-system problems
- implementation may overfit to deterministic tools
- expression parsing may become a distraction
- tool use may bypass the intended context manager
- success may not generalize to noisy tools such as web search or audio input
- model-only arithmetic may be good enough for simple examples, hiding the architectural value

## Expected Output

The experiment should produce:

- tool registry example
- tool request examples
- tool result examples
- trace output
- error examples
- recommendation for minimal tool-system interface
- follow-up questions for read-only tools

## Results

Implemented in `crates/qsf_app/src/experiments/tool_as_perception_calculator.rs` and
`crates/qsf_app/src/tools/`.

### What Happened

- The placeholder experiment was replaced by a real calculator tool path.
- The run creates a structured calculator request, validates it through the static
  tool registry, executes the deterministic calculator, and records the normalized
  result.
- The result becomes a tool-observation context fragment before context assembly.
- Tool validation or execution errors record `ToolFailed` before bubbling the error
  to the runner.

### Measurements

- Events record tool request metadata, completion metadata, latency, output text, and
  numeric value.
- The tool trace records request, permission, metadata, result, and latency.
- Context assembly records whether the tool observation fragment was selected under
  the context budget.

### Observations

- Compute-only tool use can fit the existing event, trace, and context-budget model.
- Treating the calculator result as an observation keeps the tool from becoming a
  hidden state mutation.
- Explicit metadata for category and side-effect level makes the permission boundary
  visible before dispatch.

### Surprises

- The existing context fragment path was enough to carry a tool result without a
  separate tool-specific prompt channel.

### Failure Modes

- The calculator parser is intentionally narrow and supports arithmetic expressions
  only.
- The registry is static and code-defined.
- This experiment does not yet prove the shape for noisy, slow, or externally backed
  tools such as search or file inspection.

## Interpretation

Observed:
  A deterministic compute-only tool can be represented as a structured request,
  normalized result, trace record, and context fragment.

Interpreted:
  The tool system should continue to treat early tools as perception extensions:
  explicit request, explicit permission boundary, observable result, and context
  selection before model use.

Uncertain:
  The same envelope still needs testing with tools that are slower, uncertain,
  externally backed, or permission-sensitive.

## Follow-Up Questions

- Should all tools use the same request/result envelope?
- How should tool result confidence be represented?
- Should tool results always enter context through the context manager?
- Should the live model decide tool use or should a separate role decide?
- How should slower or unreliable tools differ from deterministic tools?
- What is the next read-only perception tool after calculator?

## Follow-Up Experiments

```text
Experiment.ToolResultMemoryPromotion
Experiment.ContextTraceInspection
Experiment.ExternalInputEventStream
Experiment.ToolAsPerceptionSearch
Experiment.ToolLatencyImpact
```

## Decision Candidates

- Candidate: Tools should use structured requests and normalized results.
- Candidate: Tool results should become observations before entering context.
- Candidate: Tool calls should include explicit purpose and trace ID.
- Candidate: Read-only and compute-only tools are acceptable early tool categories.
- Candidate: Tool outputs should not bypass context management.

## Final Status

Completed as the first tool-as-perception MVP. Keep this document as the experiment
spec plus outcome summary; future tool experiments should reuse the request/result
observability shape while testing less deterministic tools.

## Notes

This was a safe first tool experiment because a calculator has no external side effects and produces verifiable results. The purpose was to shape the tool interface, not to test advanced reasoning.
