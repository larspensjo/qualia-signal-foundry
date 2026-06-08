# Non-Goals

## Purpose

This document defines what Qualia Signal Foundry is intentionally not trying to become.

The project is exploratory and open-ended, but it still needs boundaries. Clear non-goals help prevent the project from drifting into adjacent but different areas, such as a normal chatbot, productivity assistant, or general agent framework.

Non-goals may change over time, but changes should be deliberate and recorded in the decision log.

## Non-Goal: Build a Productivity Assistant

Qualia Signal Foundry is not primarily a tool for helping users complete tasks.

It may eventually support useful capabilities such as search, calculation, file inspection, summarization, or code execution. However, those capabilities are not the central purpose.

The central purpose is to explore simulated presence, continuity, memory, perception, and consciousness-like behavior.

A useful distinction:

```text
Productivity assistant:
  "How can the system help the user accomplish tasks?"

Qualia Signal Foundry:
  "How can the system behave like a continuous simulated mind?"
```

## Non-Goal: Build a Conventional Chatbot

The project is not intended to be another chat interface around a language model.

A chatbot can answer prompts. This project is interested in what happens around and between prompts:

- memory formation
- attention
- association
- interruption
- reflection
- continuity across sessions
- internal state transitions
- perception through tools
- sleep-like consolidation

A text chat may be useful for debugging or early testing, but it should not define the long-term shape of the project.

## Non-Goal: Optimize for Task Completion

Many AI systems are evaluated by how efficiently they solve user requests.

That is not the primary evaluation model here.

This project may care more about questions such as:

- Does the system seem continuous over time?
- Does it remember in a plausible way?
- Does it retrieve relevant associations without loading all memory?
- Does it react appropriately in real time?
- Does it show coherent internal state?
- Does it maintain a stable but evolving identity model?
- Does it forget, reinforce, or consolidate memories in interesting ways?

Task completion may still matter, but it is secondary.

## Non-Goal: Claim Real Consciousness

This project does not attempt to prove that a software system is conscious.

The project explores consciousness-like structures and behaviors. It is about simulation, experimentation, and observation.

The language of consciousness, memory, attention, and perception should be treated as research vocabulary, not as a claim that the system has subjective experience.

## Non-Goal: Produce a Finished Theory of Consciousness

The project is not a philosophical proof or a complete theory of mind.

It may borrow ideas from cognitive science, neuroscience, philosophy, psychology, AI research, and software architecture. However, the goal is to build an experimental platform, not to settle the nature of consciousness.

The project should allow multiple competing models to be tested.

## Non-Goal: Perfectly Imitate Humans

Human cognition is an inspiration, not a strict blueprint or an upper limit.

The project may explore human-like mechanisms such as:

- memory decay
- associative recall
- limited attention
- sleep-like consolidation
- continuity of identity

But it does not need to reproduce all human limitations.

It is acceptable to explore non-human or super-human forms of simulated cognition, such as broader memory, exact temporal awareness, faster reflection, parallel cognitive roles, or more structured self-observation.

This does not mean bypassing the simulation with unobservable shortcuts. When a
super-human capacity is introduced, it should be represented explicitly enough
to inspect, test, and compare. The project may ask both:

```text
What human-like limitation creates useful continuity or presence?
What non-human capacity makes this simulated mind more coherent, legible, or interesting?
```

The aim is not to copy a human mind exactly. The aim is to build a simulated
mind-like system whose limits and enhancements are deliberate research choices.

## Non-Goal: Give the System Uncontrolled Agency

The early project should not give the simulated system uncontrolled ability to act in the outside world.

Initial tools should be treated mainly as read-only perception extensions.

Examples of acceptable early tools:

- calculator
- file reader
- search
- local code execution in a controlled environment
- sensor input
- audio input
- possibly video input

Examples of capabilities that should be delayed or restricted:

- sending messages
- posting online
- making purchases
- modifying external systems
- controlling user accounts
- running unsandboxed commands
- initiating contact with other people

This boundary may be revisited later, but the early system should prioritize observation over action.

## Non-Goal: Hide Internal State

The project should not treat the simulated system as a black box.

For research purposes, internal state should be inspectable where practical. This may include:

- active context
- retrieved memories
- memory weights
- associations
- current goals or tensions
- attention focus
- recent state transitions
- sleep-phase changes
- tool-use decisions

The system does not need to expose every implementation detail to end users, but researchers and developers should be able to inspect enough state to understand why behavior occurred.

## Non-Goal: Lock Down the Architecture Too Early

The project should avoid premature architectural certainty.

Many ideas are still speculative, including:

- how memory should be represented
- how associations should be weighted
- how sleep-like consolidation should work
- how many AI model roles are needed
- how real-time interaction should be structured
- how identity and self-modeling should be represented
- how tools should be selected and invoked

Early architecture documents should be treated as sketches or candidates until experiments provide evidence.

## Non-Goal: Require Expensive Always-Loaded Context

The system should not depend on loading large amounts of memory, transcript history, or documentation into every live interaction.

A central challenge of the project is to keep the main loop small, responsive, and cost-aware.

This means the project should explore:

- selective retrieval
- associative indexing
- summarization
- salience scoring
- decay and reinforcement
- context budgeting
- sleep-phase preparation

Large context windows may be useful, but they should not become the default solution to every memory problem.

## Non-Goal: Treat Documentation as Final Specification

The documentation should not be interpreted as a fixed specification unless a document clearly says so.

Some documents will be stable project framing. Others will be concept notes, research questions, checklists, diary entries, experiments, or candidate architectures.

The documentation system should support uncertainty.

A useful distinction:

```text
Concept note:
  "This idea may be useful."

Research question:
  "This is unresolved."

Experiment:
  "This is how we test an idea."

Decision record:
  "This is what we have deliberately chosen for now."
```

## Non-Goal: Build a Polished Product First

The early project should not prioritize product polish, onboarding flows, branding, packaging, or a complete user experience.

Those may become relevant later, but the first priority is learning.

Early work should favor:

- simple prototypes
- observable behavior
- clear logs
- small experiments
- documented assumptions
- explicit open questions
- repeatable test scenarios

## Non-Goal: Hide Uncertainty

The project should not pretend that speculative ideas are already solved.

Open questions should remain visible. Failed experiments should be recorded. Weak assumptions should be marked as weak. Design alternatives should be preserved when useful.

A research platform benefits from traceability:

- what was tried
- why it was tried
- what happened
- what changed as a result

## Summary

Qualia Signal Foundry should remain focused on its central purpose:

> Explore consciousness-like behavior through practical software experiments.

The project may use chat, audio, tools, memory, agents, and models, but those are means rather than ends.

The early project should prioritize research value, inspectability, controlled experimentation, and conceptual clarity over productivity, polish, or premature architectural certainty.
