# Decision log

Purpose: durable project memory for AI-assisted development.

How to use:
- Add an entry when a noteworthy implementation lands.
- Add an entry for every bug fix, including lessons learned and prevention.
- Add an entry for important decisions and tradeoffs.
- Keep entries concise and reference concrete artifacts.
- New entries goes to the end of the file.
- A plan in itself, or change thereof, isn't noteworthy. It is meta information, which doesn't mean anything unless the plan is implemented.

## Entry Template

## YYYY-MM-DD - Short title
Type: Implementation | Bug Fix | Decision
Context: Why this change happened.
Change: What was implemented/changed.
Lessons Learned: (required for Bug Fix)
Prevention: (required for Bug Fix)
Refs: path/to/file.rs, test_name, commit abc1234

## 2026-05-09 - Unidirectional event-reducer-state flow
Type: Decision
Decision: The runtime loop updates state exclusively through pure reducer functions of the
form (State, Event) → State. Side effects are isolated and fed back as events.
Context: Explainable state transitions, pure-function testability, and clean separation
between what happened (events) and what changed (state). Established in Agents.md and
mirrored in Architecture.RuntimeLoop.md.
Consequences: Side-effect-producing operations (model calls, tool invocations, I/O) must
not modify state directly. They must emit events that the reducer then processes.
Refs: docs/Architecture/Architecture.RuntimeLoop.md, Agents.md
