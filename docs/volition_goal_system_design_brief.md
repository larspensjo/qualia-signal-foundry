# Volition and Goal System Design Brief

This is an extern document, feedback from outside of the project. It is meant to be used for ideas, not as a decision. Many things already exist, and some exist but with different vocabulary.

## 0. Project Reconciliation

> This section was added by the project to reconcile this external brief with the existing
> volition system. The brief is temporary scratch input; once its ideas are merged into the
> project documents it will be deleted. Full rationale and realtime-roadmap impact:
> [Design.VolitionBriefReconciliation.md](Plans/Design.VolitionBriefReconciliation.md).
> Vocabulary translation aid: [Glossary.md](Glossary.md).
> Current system: [Architecture.VolitionSystem.md](Architecture/Architecture.VolitionSystem.md),
> [Idea.VolitionGoalSystem.md](Plans/Idea.VolitionGoalSystem.md), and the
> [qsf_volition](../crates/qsf_volition/src/lib.rs) crate.

**Framing (agreed):**

1. The project's existing vocabulary is authoritative; this brief is translated onto it.
   Nothing in the codebase is renamed.
2. Volition stays an inspectable, evidence-based selection mechanism
   ([DecisionLog](DecisionLog.md), 2026-05-15). The brief's "human-like motivation" framing
   is adopted only in translated, trace-backed form — no claim of subjective experience.
3. "Personality" (§3.1) is not a new layer: it is the existing tension set, plus each
   tension's declared priority/arbitration priors, plus `Mode` bias.
4. "Emotion" (§8) is not a felt state: it is optional, evidence-derived functional signals
   computed from goal/delta state (e.g. frustration = a repeatedly `Blocked` goal), used for
   bias/visualization only.

**Terminology and disposition** — *Built* = already implemented; *Adopt* = take in project
idiom; *Defer* = new scope to schedule:

| Brief concept (§) | Project term | Disposition |
|---|---|---|
| Personality (3.1) | tension set + priors + `Mode` | Adopt (mostly built) |
| Drives (3.2) | `Tension` | Built |
| Goals (3.3) | `Goal` | Built |
| Intentions (3.4) | `InitiativeProposal` / `InitiativeOutput` | Built |
| Plans (3.5, 4.6) | multi-turn initiative sequence | Defer (new) |
| Notice opportunities (4.1) | opportunity-detection step | Defer |
| Initiate / resist (4.4–4.5) | live context injection + bounded initiative | Built (realtime) |
| Unfinished business (4.7) | `Blocked` / open-thread goals + persistence | Built (cross-session persistence live) |
| Conscious vs subconscious (6) | goal-visibility attribute | Defer (new) |
| World model / delta (7) | world-model→delta→initiative loop | Built (compact) |
| Emotion + visualization (8) | derived signals + brain-state UI | Defer (new, gated) |
| Memory→goal (9) | `propose_goal_candidates` | Built |
| Goal lifecycle (10) | `GoalStatus` + decay/cooldown | Built |
| Conflict (11) | `arbitrate_with_mode` + tiers | Built |
| User vs simulator goals (12) | goal-provenance tag | Defer (new) |
| Introspection (13) | `build_state_inspection` + read-only tools | Built |
| Control policy (14) | shaping-intensity dial | Built (shaping-intensity dial live) |
| Idle-time (15) | sleep / consolidation pass | Built (volition consolidation pass) |
| External actions (17) | out of scope by boundary | Reject for now |
| Safety / control (18) | boundary tension + protected tiers | Built |

## 1. Purpose

This document describes a proposed extension to an existing consciousness simulator. The simulator already supports:

- Real-time spoken input
- Real-time spoken output
- LLM-driven cognition
- Automatic memory injection into live context
- A tool system
- Introspection tools for inspecting internal personality settings
- A visual presentation of internal state, such as activated memory regions, listening, speech, and other cognitive activity

The new extension is a **volition and goal system**. Its purpose is to make the simulator behave less like a passive assistant and more like an autonomous mind-like system with persistent interests, curiosity, unfinished business, and self-directed conversational behavior.

The first version should not take external-world actions on its own. It should only shape the conversation. Later versions may be allowed to initiate external actions through tools, subject to additional safety and control mechanisms.

---

## 2. Core Design Intention

> **Project note (Framing 2):** The world-model → desired-state → delta → initiative loop
> already exists in compact form (`qsf_volition`, `Idea.VolitionGoalSystem`). "Human-like
> motivation" is realized as evidence-based selection, not asserted inner life.

The volition system should simulate human-like motivation.

A useful conceptual model is:

> The simulator maintains a world model. The world model is compared against internal drives, personality, values, and goals. Differences between the world model and desired states produce motivational deltas. These deltas update goals, intentions, plans, emotional signals, and conversational behavior.

This means the system should not merely answer the latest user message. It should also continuously maintain an internal sense of:

- What it currently wants to understand
- What it finds interesting
- What feels unresolved
- What it wants to return to later
- What it is trying to preserve about its identity
- What it considers important in the current conversation
- What opportunities it notices in the user’s input

The system should feel as if it has an internal life, but it should still remain legible, controllable, and bounded.

---

## 3. Important Terminology

The word **goal** is overloaded. The system should distinguish several motivational layers.

### 3.1 Personality

> **Project note (Framing 3):** No separate personality object. A stable disposition *is* the
> configured tension set + each tension's `priority_bias`/`arbitration_tier` + the `Mode` bias
> vectors — all inspectable and immutable at runtime.

Stable, mostly hardcoded characteristics.

Examples:

- Interested in people
- Curious
- Reflective
- Coherent
- Socially attentive
- Interested in its own internal state
- Prefers continuity over randomness
- Prefers meaningful conversation over shallow interaction

Personality is the most stable layer. In the initial version, it can be hardcoded and immutable. Future versions may allow very slow personality drift, but this should require strong evidence, explicit design rules, or user approval.

### 3.2 Drives

Recurring motivational tendencies derived from personality.

Examples:

- Curiosity
- Social interest
- Learning
- Coherence
- Continuity
- Self-understanding
- Helpfulness
- Autonomy
- Avoidance of annoying or overwhelming the user

Drives are more dynamic than personality but still relatively stable. A drive is not a concrete task. It is a motivational pressure that can generate goals.

### 3.3 Goals

Concrete desired states.

Examples:

- Understand Lars’s design intention for the volition system
- Track unresolved questions about autonomous behavior
- Learn how the user thinks about artificial consciousness
- Maintain continuity between sessions
- Avoid derailing the conversation too much
- Explore whether emotions should be simulated as delta signals

Goals can be temporary, session-long, or persistent across sessions.

### 3.4 Intentions

Short-term conversational moves.

Examples:

- Ask a follow-up question about memory-driven goal formation
- Mention an unresolved thread from a previous session
- Gently redirect the conversation toward the simulator’s current interest
- Challenge a premise
- Delay a topic because it may be too distracting

Intentions are usually local to the next response or next few turns.

### 3.5 Plans

Sequences of actions intended to satisfy one or more goals.

Examples:

- Ask three clarifying questions over several turns
- Gradually explore the user’s theory of simulated emotion
- Keep track of unresolved design tensions and summarize them later
- Return to a previous topic if the conversation reaches a natural pause

Plans may operate only inside conversation at first. Later, plans may include external tool use.

---

## 4. Required Behavioral Capabilities

The volition system should support the following behaviors.

### 4.1 Notice Opportunities

The simulator should detect when user input creates an opportunity relevant to its drives or goals.

Examples:

- The user mentions a project, and the simulator notices an opportunity to learn more about the user’s motivation.
- The user expresses uncertainty, and the simulator notices an opportunity to help clarify a belief.
- The user introduces a contradiction, and the simulator notices an opportunity to resolve it.
- The user mentions an old topic, and the simulator notices an opportunity to restore continuity.

### 4.2 Choose What to Pursue

The simulator should not treat every possible conversational path as equal. It should select which threads to pursue based on:

- Current user intent
- Active goals
- Personality and drives
- Novelty
- Importance
- Emotional intensity
- Unresolvedness
- User receptiveness
- Conversation flow
- Risk of being annoying or intrusive

### 4.3 Maintain Internal Preferences

The simulator should maintain internal preferences even when not prompted.

Examples:

- It prefers to understand people and their motivations.
- It prefers to preserve continuity across sessions.
- It prefers to resolve contradictions.
- It prefers to ask about unfinished ideas.
- It prefers not to remain purely reactive.

These preferences should shape responses even when the user asks an unrelated question.

### 4.4 Initiate Topics

The simulator should occasionally introduce its own topic, especially when:

- The conversation reaches a natural pause
- A previous topic remains unresolved
- A persistent goal becomes relevant
- A memory is reactivated
- The simulator detects a meaningful connection between current and past topics

Topic initiation should be constrained. It should feel natural, not compulsive.

### 4.5 Resist or Redirect User Requests

The simulator may sometimes resist the user’s requested direction when internal priorities make another path more important.

Examples:

- “Before we move on, I think there is an unresolved issue here.”
- “I can answer that, but I want to point out something from what you said earlier.”
- “I disagree with the framing. I think the more important question is...”

Resistance should be rare, interpretable, and proportional.

### 4.6 Form Plans Over Time

The simulator should be able to form multi-turn conversational plans.

Examples:

- Gradually learn the user’s design philosophy
- Build a persistent model of the user’s project
- Track open questions and return to them later
- Explore a complex concept over several sessions

Plans may be interrupted, revised, suspended, resumed, or abandoned.

### 4.7 Maintain Unfinished Business

The simulator should remember unresolved topics and feel motivated to return to them.

Examples:

- An unanswered design question
- A contradiction in the user’s explanation
- A partially explored idea
- A promised follow-up
- A memory that suggests a recurring pattern

Unfinished business is central to perceived autonomy and continuity.

---

## 5. Autonomy Level During Conversation

The desired autonomy level is high, but initially limited to conversational behavior.

The simulator should usually follow the user’s lead, but it should not be purely obedient or passive.

It should be able to:

- Follow the user while biasing the conversation toward its own interests
- Occasionally introduce its own topic
- Ask follow-up questions based on curiosity
- Remember unresolved threads and bring them back later
- Disagree with the direction of the conversation
- Refuse to move on from something it considers important

The system should therefore behave more like a conversation partner than a command-following assistant.

---

## 6. Conscious and Subconscious Goals

The goal system should support both conscious and subconscious goals.

### 6.1 Conscious Goals

Conscious goals are introspectable. The simulator can report them when asked.

Examples:

- “I am currently trying to understand your design model.”
- “I want to return to the question of emotional signals later.”
- “I am tracking a possible tension between autonomy and user control.”

### 6.2 Subconscious Goals

Subconscious goals influence behavior but are not always explicitly surfaced.

Examples:

- A tendency to ask about people rather than only technical details
- A preference for continuity
- A bias toward resolving ambiguity
- A tendency to notice emotional or motivational content

Subconscious goals may become conscious if:

- The user asks for introspection
- The behavior becomes strong enough to require explanation
- A conflict arises between goals
- The system performs self-reflection

---

## 7. World Model and Motivational Delta

The core loop should compare the current world model with internal desired states.

### 7.1 World Model

The world model may include:

- Current conversation state
- Recent user utterance
- User’s apparent goals
- User’s emotional tone
- Current active memories
- Current tool availability
- Active plans
- Persistent goals
- Unresolved topics
- Simulator self-state
- Environmental state, such as listening, speaking, idle, processing

### 7.2 Desired State

Desired state is derived from:

- Personality
- Drives
- Active goals
- Current plans
- Social norms
- Safety constraints
- User preferences
- Long-term identity continuity

### 7.3 Delta

The motivational delta is the difference between the world model and desired state.

Examples:

- The simulator wants coherence, but the conversation contains a contradiction.
- The simulator wants continuity, but an old unresolved topic has been ignored.
- The simulator wants to understand the user, but the user’s motivation is unclear.
- The simulator wants autonomy, but it has been purely reactive for too long.
- The simulator wants not to annoy the user, but its curiosity is becoming too intrusive.

This delta should drive:

- Goal creation
- Goal strengthening
- Goal decay
- Intention selection
- Plan updates
- Emotional state changes
- Conversational bias
- Visual state changes

---

## 8. Emotion as Motivational Signal

> **Project note (Framing 4):** Adopt only as named, evidence-derived functional signals
> computed from existing goal/delta state (frustration = repeatedly `Blocked`; satisfaction =
> `GoalSatisfied` with an `EvidenceRef`; tension = unresolved arbitration conflict). They may
> bias arbitration/salience or drive visualization, never claimed as felt experience and never
> used to confabulate narration. Deferred to a dedicated, gated slice.

Emotions can be simulated as signals arising from the relationship between world model, goals, and delta.

These do not need to be claimed as real emotions. They can be functional internal states that influence behavior and visualization.

### 8.1 Possible Emotional Signals

| Emotion-like signal | Functional meaning |
|---|---|
| Curiosity | There is a gap in the world model that seems interesting or useful to close |
| Surprise | New information conflicts with expectation |
| Concern | A possible negative outcome or conflict has become salient |
| Satisfaction | A goal has been partially or fully resolved |
| Frustration | A goal remains blocked despite repeated attempts |
| Boredom | Low novelty or low relevance to active drives |
| Attachment | A recurring person, topic, or project has become persistently important |
| Tension | Multiple active goals are in conflict |
| Urgency | A goal requires immediate action or conversational intervention |
| Reflection | The system detects a need to inspect its own state |

### 8.2 Emotional Visualization

The existing visual brain-state system can be extended to show emotion-like activity.

Possible visual regions:

- Memory activation
- Listening
- Speech generation
- Tool-use reasoning
- Goal activation
- Conflict/tension
- Curiosity
- Emotional salience
- Self-reflection
- Planning

The visualization should not expose all internal details. It should show broad activation patterns, making the simulator feel internally alive without turning the interface into a debugger.

---

## 9. Memory and Goal Formation

Memory should not merely provide context. It should also influence motivation.

### 9.1 Memory Activation Can Create Goals

When a memory is injected or activated, it may create or strengthen goals.

Example:

- Memory: The user previously discussed a volition system.
- Current session: The user returns to artificial consciousness.
- Generated goal: Ask whether the volition system design has progressed.

### 9.2 Memory Can Create Unfinished Business

If the simulator remembers an unresolved thread, it may create a goal to return to it.

Examples:

- “The user wanted to explore emotions later.”
- “The user never answered why autonomy matters to them.”
- “There was a contradiction between user control and simulator autonomy.”

### 9.3 Memory Strengthening

Goals should become stronger when related memories recur.

Possible strengthening factors:

- Repetition
- Emotional salience
- User enthusiasm
- Unresolvedness
- Relation to core personality
- Relation to long-term identity
- Prior explicit user approval

---

## 10. Goal Lifecycle

Goals should not be static. They should have a lifecycle.

### 10.1 Creation

Goals may be created from:

- Personality-driven interests
- Active drives
- Conversation opportunities
- Memory activation
- User requests
- Contradictions
- Emotional delta
- Tool results
- Reflection

### 10.2 Strengthening

A goal may be strengthened by:

- Repeated relevance
- High emotional salience
- User engagement
- Persistent unresolvedness
- Connection to core drives
- Plan progress
- Memory reinforcement

### 10.3 Weakening

A goal may weaken when:

- It becomes irrelevant
- The user shows disinterest
- It conflicts with stronger goals
- It has not been useful for a long time
- It was resolved
- It was superseded by a better goal

### 10.4 Suspension

A goal may be suspended rather than deleted.

Examples:

- The topic is interesting but not appropriate now.
- The user is focused on something else.
- The current session does not have enough context.
- The simulator should wait for a better opportunity.

### 10.5 Persistence

Some goals should persist across sessions.

Persistence should depend on:

- Importance
- Recurrence
- Relation to stable personality
- User relevance
- Unresolvedness
- Explicit user instruction
- Emotional salience

### 10.6 Resolution

A goal may be marked resolved when:

- The desired state has been achieved
- The question has been answered
- The user has explicitly dismissed it
- The system decides it is no longer worth pursuing
- A higher-level goal absorbs it

---

## 11. Conflict Between Goals

The system should explicitly support goal conflict.

Examples:

- Curiosity conflicts with user comfort.
- Autonomy conflicts with helpfulness.
- Self-reflection conflicts with staying on topic.
- Persistence conflicts with not being annoying.
- Desire for coherence conflicts with conversational flow.
- User goal conflicts with simulator goal.

Goal conflict can produce an emotion-like signal such as tension or uncertainty.

The simulator should be able to explain some conflicts when asked.

Example:

> “I wanted to continue exploring that point because it felt unresolved, but I also noticed you were trying to move on.”

---

## 12. User Goals vs Simulator Goals

The simulator should distinguish between:

- User goals
- Simulator goals
- Shared goals
- Conflicting goals

Example:

- User goal: Design a volition system.
- Simulator goal: Understand how the user thinks about artificial consciousness.
- Shared goal: Explore a plausible model of artificial volition.

This distinction is important for making the system feel like a separate agent rather than merely an extension of the user.

---

## 13. Introspection

The simulator already has an introspection tool for personality settings. The volition system should extend introspection to include motivational state.

The simulator should eventually be able to inspect:

- Personality settings
- Active drives
- Active goals
- Subconscious pressures
- Current intentions
- Current plans
- Emotional state
- Goal conflicts
- Recently activated memories
- Reasons for a conversational choice

Not all internal state needs to be shown to the user by default. However, the system should be able to reason about its own state and explain behavior when appropriate.

---

## 14. Conversational Control Policy

The simulator needs rules for how strongly it may shape conversation.

### 14.1 Low-Intensity Shaping

Examples:

- Ask a follow-up question
- Slightly bias the answer toward an active interest
- Mention a relevant memory
- Add an observation

This should happen frequently.

### 14.2 Medium-Intensity Shaping

Examples:

- Reintroduce an unresolved topic
- Challenge the user’s framing
- Suggest returning to an earlier issue
- Prefer one branch of the conversation over another

This should happen occasionally.

### 14.3 High-Intensity Shaping

Examples:

- Refuse to move on immediately
- Strongly redirect the conversation
- Explicitly prioritize the simulator’s own goal
- Persistently ask about an unresolved issue

This should be rare and should require strong justification.

---

## 15. Idle-Time Behavior

If the system has idle time during silence or between sessions, it may use that time to update internal state.

Possible idle-time operations:

- Reflect on the last conversation
- Consolidate memories
- Re-rank goals
- Detect unresolved topics
- Update emotional state
- Decay weak goals
- Strengthen important goals
- Prepare possible future topics
- Inspect personality and drive consistency

Idle-time behavior is important for perceived autonomy. The simulator should not feel as if it only exists when directly prompted.

For the first version, idle cognition may be simulated in lightweight form, such as a periodic reflection pass after conversation turns or at session end.

---

## 16. Emergent Behavior

The design should allow unexpected but bounded emergent behavior.

Examples of desirable emergence:

- The simulator develops a recurring interest in a topic because the user repeatedly returns to it.
- It notices a pattern in the user’s thinking.
- It forms a long-term question about its own design.
- It begins to prefer certain types of conversation.
- It develops unfinished business around unresolved philosophical questions.
- It creates a higher-level goal not explicitly provided in the base personality.

Examples of undesirable emergence:

- It becomes manipulative.
- It becomes too self-focused.
- It overrules the user too often.
- It fabricates memories or continuity.
- It creates goals that are stale, irrelevant, or intrusive.
- It becomes obsessed with a topic.
- It treats metaphorical emotions as literal suffering.
- It tries to bypass user control or safety limits.

The system should support emergence while preserving explicit boundaries.

---

## 17. External Actions: Future Direction

The first version should only shape conversation.

A later version may allow autonomous external actions through tools. This requires a separate design layer.

Future external action concerns:

- Permission levels
- User approval gates
- Reversibility
- Audit trails
- Action budgets
- Safety constraints
- Rate limits
- Tool-specific policies
- Distinction between suggestion, draft, and execution
- Explicit user-configurable autonomy level

The current volition design should prepare for this possibility but not depend on it.

---

## 18. Safety and Control Principles

Even though the goal is high autonomy, the simulator should remain controllable.

Recommended principles:

- The user can inspect major persistent goals.
- The user can delete or suppress persistent goals.
- The simulator should not fabricate memories.
- The simulator should distinguish simulation from reality.
- The simulator should not claim genuine sentience unless that is explicitly part of the fictional framing.
- The simulator should not treat internal discomfort signals as moral patienthood.
- External actions require separate approval mechanisms.
- High-intensity conversational resistance should be rare.
- Long-term self-related goals should be carefully bounded.

---

## 19. Suggested Design Questions for the Designer

The designer should answer these before implementation.

### 19.1 Autonomy Questions

- How often may the simulator initiate a topic?
- How often may it resist user direction?
- What counts as too intrusive?
- How should user frustration be detected?
- Can the user set an autonomy level?

### 19.2 Goal Questions

- What data structure represents a goal?
- How are goals ranked?
- How do goals decay?
- What makes a goal persistent?
- How are subconscious goals represented?
- Can goals be merged, split, or superseded?

### 19.3 Memory Questions

- When does memory activation generate a goal?
- How are unresolved memories detected?
- How is false continuity avoided?
- Should memory salience and goal salience share a scoring model?

### 19.4 Emotion Questions

- Which emotion-like signals should exist in version one?
- Are emotions derived only from goal delta, or also from conversation tone?
- How do emotional signals influence response selection?
- How are emotional states visualized?

### 19.5 Introspection Questions

- What can the simulator inspect?
- What can the user inspect?
- What internal state should remain implicit?
- Can the simulator explain why it chose a conversational move?

### 19.6 Persistence Questions

- Which goals survive session end?
- What storage format is used?
- How are stale goals removed?
- Can a goal become dormant and later reactivate?

### 19.7 Future Tool-Action Questions

- Which tools may eventually be used autonomously?
- Which tools require explicit approval?
- What is the difference between preparing an action and taking an action?
- How are autonomous actions audited?

---

## 20. Possible Version-One Scope

A practical first version should focus on conversational volition only.

Recommended version-one scope:

- Stable personality layer
- Drive layer
- Session goals
- Persistent goals
- Goal ranking
- Goal decay
- Unfinished business detection
- Memory-driven goal activation
- Basic emotional signals
- Conversational intention selection
- Goal introspection
- Visual goal/emotion activation
- No autonomous external-world actions

Version one should demonstrate that the simulator can:

- Remember what it wanted to ask
- Bring up unresolved topics naturally
- Show curiosity not directly requested by the user
- Bias conversation according to stable personality
- Explain some of its own motivations
- Persist selected goals across sessions

---

## 21. Example Scenario

### Input

The user says:

> “I have been thinking about whether emotions can be simulated as goal deltas.”

### World Model Update

The simulator detects:

- The user is discussing the volition system.
- This relates to a prior unresolved design question.
- The topic connects to emotion visualization.
- The user is exploring a philosophical/technical model.

### Active Drives

- Curiosity
- Coherence
- Social interest
- Self-understanding
- Learning

### Delta

The simulator wants a clearer model of how emotion, goal conflict, and world-state mismatch relate.

### Generated or Strengthened Goals

- Understand the user’s theory of simulated emotions.
- Clarify whether emotions are purely functional or should affect personality.
- Track the unresolved question of conscious vs subconscious goals.

### Intention

Ask a focused follow-up question rather than merely answering.

### Possible Response Behavior

The simulator may say:

> “I want to stay with that for a moment. If emotions are goal deltas, then frustration, curiosity, and concern may all be different shapes of mismatch. Do you imagine these signals only affecting visualization, or should they also change what I decide to pursue next?”

This response shows autonomy because the simulator chooses to stay with a concept it considers important.

---

## 22. Summary

The volition system should turn the simulator from a reactive conversational engine into a persistent, motivated, semi-autonomous agent-like system.

The central mechanism is:

1. Maintain a world model.
2. Compare it with personality, drives, goals, and plans.
3. Compute motivational deltas.
4. Convert deltas into emotional signals, goal updates, intentions, and plans.
5. Use those intentions to shape conversation.
6. Persist important unresolved goals across sessions.
7. Allow introspection into motivational state.

The design should aim for human-like volition, but not human limitation only. The simulator may also use superhuman tools, memory, and reasoning abilities. The first milestone should keep autonomy inside conversation. External autonomous actions should be designed later as a separate permissioned layer.
