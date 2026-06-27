# Experiment Backlog

## Purpose

This backlog collects candidate experiments for Qualia Signal Foundry.

The project is exploratory, so experiments should stay small, observable, and reversible. An experiment should reduce uncertainty, test a concept, compare candidate architectures, or reveal failure modes.

This backlog is not a commitment to implement everything listed here.

## Backlog Rules

- Keep experiments small enough to produce a useful result.
- Prefer experiments that test one idea at a time.
- Record negative results; they are useful.
- Do not promote experiment outcomes into architecture decisions too quickly.
- Link experiments to concepts, architecture documents, and research questions.
- Use `Experiment.Template.md` when an experiment is selected for planning.

## Status Values

```text
Idea
Proposed
Planned
Running
Completed
Paused
Abandoned
Superseded
```

## Priority Values

```text
High
Medium
Low
Later
```

## Candidate Experiments

| Experiment | Priority | Status | Main Question |
|---|---:|---:|---|
| `Experiment.AssociativeMemoryToyModel` | High | Completed | Can a small weighted memory graph retrieve useful context better than recency-only lookup? |
| `Experiment.FrameworkSkeletonMVP` | High | Completed | What is the smallest runnable framework needed to support future experiments? |
| `Experiment.EventLogAndTraceMVP` | High | Completed | What minimal event log and trace format is useful for understanding system behavior? |
| `Experiment.ContextBudgetRetrievalTest` | High | Completed | How should the system select memories under a small context budget? |
| `Experiment.SleepPhaseSessionSummary` | High | Completed | Does a session-end summary improve continuity in the next session? |
| `Experiment.ProjectDocLiveRegressionAudit` | High | Proposed | Can live project-doc tool turns preserve prompt-prefix continuity and retrieve expected docs? |
| `Experiment.VolitionGoalFixture` | Medium | Completed | Can a static tension/goal fixture deterministically select input-relevant goals and propose candidate initiatives without executing any effect? |
| `Experiment.VolitionTraceBackedInitiative` | Medium | Completed | Can pre-initiative traces explain goal → delta → candidate initiatives → proposed bounded effect before any behavior changes? |
| `Experiment.VolitionSalienceAndSatisfaction` | Medium | Completed | Can a pure, replayable volition state raise/decay salience and satisfy, block, cool down, and retire goals from evidence without executing any effect? |
| `Experiment.VolitionArbitrationConflict` | Medium | Completed | Can a pure arbitrate() function resolve cross-goal conflict by tension tier, record structured provenance for every loser, and produce deterministic replayable output without executing any effect? |
| `Experiment.VolitionReflectionGoalCandidates` | Medium | Completed | Can a pure, model-free proposer map scripted open questions to goal candidates with evidence refs, and can accept/reject events move candidates through a durable pending-review state without influencing any selector? |
| `Experiment.VolitionBoundedInitiativeExecution` | Medium | Running | Can accepted candidates wire into the selector via tension-derived keywords and, once selected, translate the arbitration winner into a bounded InitiativeOutput without a model call or external action? |
| `Experiment.VolitionModeBias` | Medium | Planned | Can a declared, inspectable mode bias arbitration ordering within a biasable band to flip the winner deterministically, while a protected tier floor stays immune by construction and no effect executes? |
| `Experiment.StreamingTranscriptionMVP` | Medium | Completed | Can live speech be represented as observable partial and final transcript events? |
| `Experiment.AudioLoopMVP` | Medium | Superseded | Can a minimal audio loop create a stronger sense of presence than text-only interaction? |
| `Experiment.ToolAsPerceptionCalculator` | Medium | Completed | How should a simple read-only computational tool be represented as perception? |
| `Experiment.MemoryDecayPolicy` | Medium | Proposed | Does memory decay improve relevance or accidentally hide useful older memories? |
| `Experiment.ModelRoleSplitLiveVsSleep` | Medium | Proposed | Is it useful to split live interaction and sleep consolidation into separate model roles? |
| `Experiment.ContextTraceInspection` | Medium | Proposed | Can a researcher understand why specific context was selected? |
| `Experiment.InterruptionHandlingAudio` | Later | Idea | How should the system react when the user interrupts while it is speaking? |
| `Experiment.ExternalInputEventStream` | Later | Idea | How should non-text inputs be normalized into runtime events? |
| `Experiment.MemoryPromotionRules` | Later | Idea | Which events should become durable memories? |
| `Experiment.AssociationReinforcement` | Later | Idea | Which signals should strengthen links between memories? |
| `Experiment.ToolResultMemoryPromotion` | Later | Idea | When should tool observations become long-term memories? |
| `Experiment.CostPerModelRole` | Later | Idea | Does splitting model roles reduce or increase total cost? |
| `Experiment.SleepTraceAudit` | Later | Idea | Are sleep-phase memory changes understandable and appropriate? |
| `Experiment.ReplaySingleRuntimeStep` | Later | Idea | Can a single runtime step be captured well enough for useful replay or inspection? |

## High-Priority Experiments

### Experiment.AssociativeMemoryToyModel

**Priority:** High
**Status:** Completed

Built a small toy version of associative memory using simple text memories, weighted links, recency, and reinforcement.

This experiment compares associative retrieval against simpler baselines such as recency-only lookup and keyword/tag lookup.

Related documents:

```text
Concepts/Concept.AssociativeMemory.md
Concepts/Concept.ContextBudget.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.SleepPhase.md
```

Possible success criteria:

- The system can retrieve relevant memories from a small controlled set.
- Retrieval decisions are inspectable.
- The result shows whether association weights are useful enough to continue.
- Failure modes are clear.

Suggested baseline:

```text
Recency-only retrieval.
```

Useful observations:

- Which memories were selected?
- Which memories were omitted?
- Were selected memories actually relevant?
- Did association links help or distract?
- How much context budget was needed?

### Experiment.FrameworkSkeletonMVP

**Priority:** High
**Status:** Completed

Created the smallest runnable project framework that can host later experiments.

This experiment is less about consciousness simulation and more about making future experiments easy to run.

Related documents:

```text
Architecture/Architecture.Overview.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
Architecture/Architecture.ModelRoles.md
```

Possible scope:

- basic runtime loop skeleton
- event type definitions
- event log
- simple trace output
- placeholder model role abstraction
- placeholder memory store
- command-line experiment entry point

Possible success criteria:

- One experiment can be run through the framework.
- Events and traces are written somewhere inspectable.
- The framework does not overcommit to final architecture.

### Experiment.EventLogAndTraceMVP

**Priority:** High
**Status:** Completed

Defined and tested a minimal event log and trace system.

This experiment should answer what must be recorded to understand a runtime step.

Related documents:

```text
Architecture/Architecture.StateAndObservability.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.ContextManagement.md
```

Possible events:

```text
InputReceived
StateUpdated
MemoryRetrieved
ContextAssembled
ModelRoleInvoked
OutputProduced
TraceRecorded
```

Possible success criteria:

- A short interaction can be inspected after the fact.
- The trace explains why output happened.
- The log is useful without being too verbose.

### Experiment.ContextBudgetRetrievalTest

**Priority:** High
**Status:** Completed

Compared several ways of selecting context under a small budget.

Candidate strategies:

```text
recency only
keyword match
semantic similarity
associative weight
hybrid score
manual ideal selection
```

Related documents:

```text
Concepts/Concept.ContextBudget.md
Concepts/Concept.AssociativeMemory.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.MemorySystem.md
```

Possible success criteria:

- The experiment shows which retrieval strategy is most promising for small memory sets.
- The output includes retrieval scores and omitted candidates.
- The result informs the first memory-system implementation.

### Experiment.SleepPhaseSessionSummary

**Priority:** High
**Status:** Completed

Ran a simple session-end sleep phase that produces a summary, memory candidates, association candidates, open questions, decision candidates, future context hints, and review notes.

Related documents:

```text
Concepts/Concept.SleepPhase.md
Architecture/Architecture.SleepPhase.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.ContextManagement.md
```

Possible success criteria:

- The sleep phase produces a useful session summary.
- It extracts plausible memory candidates.
- It identifies unresolved questions.
- It does not silently create accepted decisions.
- The output is inspectable.

### Experiment.ProjectDocLiveRegressionAudit

**Priority:** High
**Status:** Proposed

Audit the live multi-turn project-doc introspection path after manual verification found
that a project-doc tool turn could make the next prompt fail the prefix-continuity guard,
and that lexical project-doc search returned no hits for an expected sleep-phase question.

Related documents:

```text
Architecture/Architecture.ToolSystem.md
Architecture/Architecture.ContextManagement.md
Plans/Design.ProjectDocIntrospection.md
Experiments/Experiment.Backlog.md
```

Possible success criteria:

- A live turn after a `search_project_docs` tool round preserves the prompt-prefix
  invariant or records an intentional invalidation.
- Expected project-self prompts can find relevant allowlisted project documents.
- The console error distinguishes local prompt assembly failures from provider
  unavailability.
- The trace is sufficient to explain any remaining retrieval miss.

## Medium-Priority Experiments

### Experiment.VolitionGoalFixture

**Priority:** Medium
**Status:** Completed

First build slice of the volition/goal system: a small, static, read-only fixture of
tensions and goals tested with deterministic, budget-bounded goal selection against
scripted inputs. Sequenced in `Plans/Plan.VolitionGoalSystem.md`; rationale and
candidate state shapes in `Plans/Idea.VolitionGoalSystem.md`.

The experiment established the `tension → goal → initiative` distinction in code,
showed that goal selection can be deterministic and replayable, and emitted traces
linking input → active goal → candidate initiative — without any model call and
without executing any effect. The result did not show tension priority materially
affecting selection at this scale, so later experiments should treat tensions as
provenance until further evidence exists.

Completed in [Experiment.VolitionGoalFixture.md](Experiment.VolitionGoalFixture.md).

Related documents:

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.StateAndObservability.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

Possible scope:

- pure `Tension` / `Goal` / `InitiativeProposal` types and a static fixture
- a deterministic goal selector reusing `ContextBudget` / `assemble_context`
- selection traces (selected + omitted goals, with rationale)
- candidate initiative proposals (proposed only, never executed)
- a perturbation run showing selection changes predictably

Possible success criteria:

- Selection is deterministic and replayable.
- A direct-task baseline input selects no goals.
- Traces connect input → active goal → candidate initiative, including omissions.
- Changing the fixture changes selection in the expected direction.

### Experiment.VolitionTraceBackedInitiative

**Priority:** Medium
**Status:** Completed

Next validation slice for the volition/goal system: record serialized
pre-initiative traces before any proposed initiative can affect behavior. It reuses
the static fixture and selector from `Experiment.VolitionGoalFixture`, but treats
active tensions as provenance rather than proven priority architecture.

The experiment tests whether a trace can connect selected goal → active tension
provenance → detected input delta or no-delta reason → candidate initiative effects
→ proposed bounded effect and losing candidates, while still executing no effect.

Completed in [Experiment.VolitionTraceBackedInitiative.md](Experiment.VolitionTraceBackedInitiative.md).

Related documents:

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionGoalFixture.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.StateAndObservability.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

Possible scope:

- pure pre-initiative trace shape
- deterministic detected-delta or no-delta reasons for scripted inputs
- candidate initiative generation from selected goals' allowed effects
- losing candidates with rejection reasons
- explicit no-execution marker in every trace

Possible success criteria:

- Every proposed initiative has a preceding trace.
- The trace connects goal, delta, proposed effect, and losing candidates.
- The direct-task baseline records no selected goal, no delta, and no candidate.
- No initiative effect is emitted or executed.

### Experiment.VolitionSalienceAndSatisfaction

**Priority:** Medium
**Status:** Completed

Extends the stateless fixture and trace slices by adding the first durable-within-a-run
volition state. A pure `VolitionState` updated via a `VolitionEvent` reducer tracks
per-goal salience, lifecycle status, cooldown, and evidence-backed progress across a
scripted multi-turn sequence.

Related documents:

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionGoalFixture.md
Experiments/Experiment.VolitionTraceBackedInitiative.md
Experiments/Experiment.VolitionSalienceAndSatisfaction.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

Possible success criteria:

- Salience rises on activation and evidence-backed progress, decays per tick.
- Goals satisfy and enter cooldown only when evidence refs are provided.
- Blocked goals stay visible with a distinct blocked reason.
- Replay produces identical per-turn state snapshots.
- No effect is executed.

### Experiment.VolitionArbitrationConflict

**Priority:** Medium
**Status:** Completed

Adds deterministic cross-goal arbitration as a pure additive layer over the
salience-aware selector. A new `arbitrate()` function resolves conflicts by tension
tier, records every losing goal with structured tension provenance, and labels each
turn with an explicit `arbitration_status`.

Related documents:

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Design.VolitionArbitration.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionSalienceAndSatisfaction.md
Experiments/Experiment.VolitionArbitrationConflict.md
DecisionLog.md  (2026-06-26 "Arbitration tier is separate from priority bias")
```

Possible success criteria:

- `arbitration_status` is recorded on every turn: `no_selection`, `single_selection`,
  and `conflict_resolved` each appear at least once.
- Conflict turns produce non-empty `losers` with structured tension provenance.
- Loser ordering is deterministic.
- Replay produces identical output.
- No effect is executed.

### Experiment.VolitionReflectionGoalCandidates

**Priority:** Medium
**Status:** Completed

Let a reflection/sleep step propose goal candidates with evidence references. Proposals
stay in pending review until an explicit accept or reject event moves them; nothing is
silently promoted. The proposer function is pure and model-free — it maps scripted open
questions to candidates deterministically.

Related documents:

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionArbitrationConflict.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

Possible success criteria:

- `propose_goal_candidates` is deterministic: same input produces identical output.
- Questions that match no fixture tension appear in `unmatched_questions` and produce no candidate.
- `GoalCandidateAdded` appends to `pending_candidates`; does not auto-accept.
- `GoalCandidateAccepted` moves the candidate to `accepted_candidates` and removes it from pending.
- `GoalCandidateRejected` removes the candidate from `pending_candidates`; reason is in the event log.
- A remaining (neither accepted nor rejected) candidate stays in `pending_candidates` across ticks.
- `accepted_candidates` is distinct from fixture-seeded `goals`; no accepted candidate influences any selector in this phase.
- Replay produces identical state and event logs.
- No effect is executed.

### Experiment.VolitionBoundedInitiativeExecution

**Priority:** Medium
**Status:** Running

Wire accepted goal candidates into `select_goals_with_salience` via activation keywords
derived from matched tension id parts, then translate the arbitration winner into a
bounded `InitiativeOutput` — a purely structural record — via a new
`execute_initiative` pure function and `VolitionEvent::InitiativeExecuted`. The chain
from proposal to execution is traced and replayable. No write-capable external action;
`executed_effects = 0` on every turn.

Related documents:

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionReflectionGoalCandidates.md
Experiments/Experiment.VolitionArbitrationConflict.md
DecisionLog.md  (2026-05-15 "Volition is an explicit research surface")
```

Possible success criteria:

- Accepted candidate's `activation_keywords` are non-empty and derived from matched tension id parts.
- Accepted candidate appears in `select_goals_with_salience` output when input matches derived keywords.
- Accepted candidate competes in `arbitrate` alongside fixture goals; tier ordering is respected.
- `execute_initiative` is deterministic: same input → same `InitiativeOutput`.
- `InitiativeExecuted` stores output in `GoalDynamicState::last_initiative_output`.
- The accepted goal's lifecycle (salience, cooldown, retirement) uses the same reducer branches as fixture goals.
- All prior tests pass; existing selector and reducer behaviour is unchanged.
- Replay produces identical state and event logs.
- No effect is executed (`executed_effects = 0`).

### Experiment.VolitionModeBias

**Priority:** Medium
**Status:** Planned

Add an inspectable `Mode` — a named, declared bias over arbitration ordering — and show that it
deterministically shifts which goal wins a conflict without being able to override the
safety/boundary floor. Bias reorders goals only within a biasable band (effective tier ≥ 4); a
protected floor (tiers 1–3) is immune, and a biased band goal is clamped so it can never enter the
floor (safety invariant by construction). A mode's meaning is its declared bias vector, not a
free-form label. Bias applies to arbitration only; salience/selection and proposal-threshold bias
are follow-ups. Mode is event-driven `VolitionState` (`ModeChanged`); `arbitrate` delegates to a
new `arbitrate_with_mode(.., Neutral)`. No model call; `executed_effects = 0`.

Related documents:

```text
Plans/Plan.VolitionGoalSystem.md
Plans/Design.VolitionModeBias.md
Plans/Idea.VolitionGoalSystem.md
Experiments/Experiment.VolitionArbitrationConflict.md
Experiments/Experiment.VolitionBoundedInitiativeExecution.md
DecisionLog.md  (2026-06-26 "Arbitration tier is separate from priority bias")
```

Possible success criteria:

- `arbitrate_with_mode(.., Neutral)` matches `arbitrate` on the same selection.
- A biasing mode flips the winner among band goals (`mode_changed_winner == true`).
- A present tier-1 goal wins under every mode (floor immunity; `mode_changed_winner == false`).
- No band goal's biased tier drops below `PROTECTED_TIER_FLOOR + 1`.
- Bias is attributed to each goal's effective tension and recorded per goal.
- `ModeChanged` updates `state.mode`; results are deterministic and replay-identical.
- All prior tests pass; existing `arbitrate`/selector/reducer behaviour is unchanged.
- No effect is executed (`executed_effects = 0`).

### Experiment.StreamingTranscriptionMVP

**Priority:** Medium
**Status:** Completed

Built the first provider-backed audio boundary by representing streaming speech-to-text
as partial and final transcript events.

This came before broader voice-loop work because it tested real-time input, event
ordering, transcript finalization, and latency tracing without requiring TTS, playback,
or interruption handling.

Related documents:

```text
Concepts/Concept.RealtimePresence.md
Concepts/Concept.ExternalInputs.md
Architecture/Architecture.AudioLoop.md
Architecture/Architecture.RuntimeLoop.md
Research/ResearchQuestions.Audio.md
```

Possible scope:

- simulated transcript provider
- OpenAI `gpt-realtime-whisper` provider adapter
- partial transcript events
- final transcript events
- transcript latency traces
- final transcript bridge into runtime input

Possible success criteria:

- Partial and final transcript events are distinguishable.
- Final transcripts can enter the runtime loop as normal input.
- Partial transcripts are logged without mutating committed state by default.
- Provider failures and latency are visible in run artifacts.

### Experiment.AudioLoopMVP

**Priority:** Medium
**Status:** Superseded

This broad audio-loop proposal was split into narrower experiments after streaming transcription events worked.

Use `Experiment.StreamingTranscriptionMVP`, `Experiment.RealtimeVoiceSessionMVP`, and `Experiment.TextOwnedVoiceLoop` for the active audio implementation record.

Related documents:

```text
Concepts/Concept.RealtimePresence.md
Concepts/Concept.ExternalInputs.md
Architecture/Architecture.AudioLoop.md
Architecture/Architecture.RuntimeLoop.md
Research/ResearchQuestions.Audio.md
```

Possible scope:

- microphone capture
- speech-to-text
- model input
- text-to-speech
- speaker output
- latency logging

Possible success criteria:

- The loop works end-to-end.
- End-to-end latency is measured.
- The system can handle at least simple turn-taking.
- Failure modes are logged.

### Experiment.ToolAsPerceptionCalculator

**Priority:** Medium
**Status:** Completed

Gave the system access to a simple calculator-like tool and represented the result as an observation rather than an action.

Related documents:

```text
Concepts/Concept.ToolsAsPerception.md
Architecture/Architecture.ToolSystem.md
Architecture/Architecture.ContextManagement.md
```

Possible success criteria:

- Tool requests are structured.
- Tool results are normalized.
- Tool use is logged.
- The result enters context only when relevant.

### Experiment.MemoryDecayPolicy

**Priority:** Medium
**Status:** Proposed

Compare simple memory decay strategies.

Candidate strategies:

```text
no decay
time-based decay
retrieval reinforcement
manual importance only
hybrid decay and reinforcement
```

Related documents:

```text
Concepts/Concept.AssociativeMemory.md
Concepts/Concept.SleepPhase.md
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.SleepPhase.md
```

Possible success criteria:

- The experiment shows how decay affects retrieval quality.
- It reveals whether important but old memories are lost too easily.
- It produces clear follow-up questions.

### Experiment.ModelRoleSplitLiveVsSleep

**Priority:** Medium
**Status:** Proposed

Compare a single-model flow with a split between live interaction and sleep consolidation.

Related documents:

```text
Concepts/Concept.MultiModelMind.md
Architecture/Architecture.ModelRoles.md
Architecture/Architecture.SleepPhase.md
```

Possible success criteria:

- The split produces better summaries or memory candidates.
- The cost and complexity are understood.
- The trace shows which role affected which output.

### Experiment.ContextTraceInspection

**Priority:** Medium
**Status:** Proposed

Inspect context assembly traces after interactions and evaluate whether they explain the system's behavior.

Related documents:

```text
Architecture/Architecture.ContextManagement.md
Architecture/Architecture.StateAndObservability.md
```

Possible success criteria:

- A researcher can understand why context was selected.
- Omitted context is visible.
- The trace helps diagnose failures.

## Later Experiments

### Experiment.InterruptionHandlingAudio

**Priority:** Later
**Status:** Idea

Explore how the system should react when the user interrupts while it is speaking.

Related documents:

```text
Architecture/Architecture.AudioLoop.md
Concepts/Concept.RealtimePresence.md
Research/ResearchQuestions.Audio.md
```

### Experiment.ExternalInputEventStream

**Priority:** Later
**Status:** Idea

Normalize external inputs such as audio, file changes, tool observations, or future video signals into runtime events.

Related documents:

```text
Concepts/Concept.ExternalInputs.md
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
```

### Experiment.MemoryPromotionRules

**Priority:** Later
**Status:** Idea

Test rules for deciding when an event should become durable memory.

Related documents:

```text
Architecture/Architecture.MemorySystem.md
Architecture/Architecture.SleepPhase.md
```

### Experiment.AssociationReinforcement

**Priority:** Later
**Status:** Idea

Test which signals should strengthen links between memories.

Related documents:

```text
Concepts/Concept.AssociativeMemory.md
Architecture/Architecture.MemorySystem.md
```

### Experiment.ToolResultMemoryPromotion

**Priority:** Later
**Status:** Idea

Test whether tool observations should become memories and under what conditions.

Related documents:

```text
Architecture/Architecture.ToolSystem.md
Architecture/Architecture.MemorySystem.md
```

### Experiment.CostPerModelRole

**Priority:** Later
**Status:** Idea

Measure whether splitting model roles increases cost too much or reduces cost by allowing cheaper specialized models.

Related documents:

```text
Architecture/Architecture.ModelRoles.md
Architecture/Architecture.ContextManagement.md
```

### Experiment.SleepTraceAudit

**Priority:** Later
**Status:** Idea

Review sleep-phase traces to determine whether consolidation changes are understandable and appropriate.

Related documents:

```text
Architecture/Architecture.SleepPhase.md
Architecture/Architecture.StateAndObservability.md
```

### Experiment.ReplaySingleRuntimeStep

**Priority:** Later
**Status:** Idea

Capture enough information to inspect or rerun a single runtime step.

Related documents:

```text
Architecture/Architecture.RuntimeLoop.md
Architecture/Architecture.StateAndObservability.md
```

## Historical First Experiment

Completed first experiment:

```text
Experiment.AssociativeMemoryToyModel
```

Reason:

- It does not require audio devices.
- It does not require real-time infrastructure.
- It tests a central idea.
- It helps design memory, context management, sleep phase, and observability.
- It can be implemented with simple data structures.
- It can produce useful traces early.

Completed first framework-support experiment:

```text
Experiment.FrameworkSkeletonMVP
```

Reason:

- It creates the minimum structure needed to run future experiments consistently.
- It keeps the project from becoming a collection of disconnected prototypes.

## Parking Lot

Ideas that may become experiments later:

- simulated attention model
- self-model state
- identity continuity across sessions
- emotional or motivational model
- video input
- screen observation
- tool permission escalation
- controlled write-capable tools
- memory graph visualization
- sleep-phase comparison between different models
- replayable conversation sessions
- user-perceived presence scoring
- synthetic benchmark conversations
- local versus remote model roles
- prompt-injection resistance for tool observations
