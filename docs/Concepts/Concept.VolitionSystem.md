# Concept: Volition System

## Maturity

Candidate.

The memory/volition boundary in this document is committed via the DecisionLog entry
"2026-07-30 - Memory and volition are distinct subsystems"; treat that section as the
project's rule. The rest of the document is candidate framing.

## Summary

Volition is the conative half of the simulated mind. Where the memory system holds
representations of what is or was the case, the volition system holds what the
simulation cares about: standing tensions, concrete goals derived from them, and
bounded initiatives proposed on their behalf.

The two systems drive the conversation in different ways:

```text
Memory drives the conversation by supplying content:
  what is relevant reappears when it matters, with honest provenance.

Volition drives the conversation by supplying direction:
  what is cared-about competes, one pressure wins the turn,
  and it shapes attention rather than adding facts.
```

This document explains why a volition system exists, what its layers mean, and —
centrally — where the boundary between volition and memory lies. The structural
design and implementation status live in `Architecture.VolitionSystem.md`.

## Core Idea

A consciousness-like simulation needs more than recall. A system that only remembers
is still purely reactive: it answers well but wants nothing, returns to nothing, and
abandons every thread the user drops. Volition gives the simulation persistent
concerns that outlive a turn and can push back on the flow of conversation.

Volition is modeled as a pressure structure with three layers:

- **Tension** — a durable, standing disposition such as curiosity, coherence,
  continuity, or boundary preservation. Tensions are never satisfied and never
  retire. They are the personality end of the system: a persona is its tension set
  plus protected constraints.
- **Goal** — a concrete, satisfiable commitment derived from one or more tensions.
  A goal has a lifecycle: it is admitted, activates, competes, and is eventually
  satisfied, blocked, or retired.
- **Initiative** — a bounded effect proposed by a selected goal for the current
  turn: ask a question, surface an open thread, request context retrieval, propose
  an experiment. Initiatives stay inside the conversation and the QSF trust
  boundary.

An important consequence of this layering: **"frozen personality settings" are not
goals.** They are tensions and protected-tier constraints. The word *goal* is
reserved for commitments that can in principle be completed or given up. When the
volition system seems to span "from frozen personality to ephemeral ideas," that is
the tension layer and the goal layer being described with one word.

## Why It Matters

### Presence and autonomy

Self-directed behavior — returning to unfinished business, gently steering toward an
active interest, noticing that a promise was never kept — is a large part of what
makes an interaction feel like a mind rather than a lookup service.

### Continuity of concern

Memory provides continuity of *knowledge*; volition provides continuity of
*caring*. Both are needed: remembering that a question was left open is a memory
fact, while still wanting to resolve it next session is a volitional state.

### Research value

Because goals are explicit state with a pure reducer, arbitration traces, and
evidence references, motivation becomes inspectable. A researcher can ask why a goal
won a turn, why a candidate was declined, or why the system went quiet — and get a
recorded answer.

### Safety by structure

Volition is where autonomy pressure concentrates, so it is also where the guardrails
live: a protected tier floor that mode bias cannot displace, arbitration that user
intent always survives, and initiatives that are structurally incapable of external
write effects.

## The Boundary With Memory

This is the section that keeps the two systems from collapsing into each other.

### The distinguishing axis is epistemic vs. conative

Memory records are **epistemic**: they represent something as being the case, so
they can be true, false, stale, or superseded. That is why memory — and only
memory — needs provenance, trust tiers, confidence, supersession, and time-sensitive
decay.

Volition records are **conative**: they commit the system to caring about
something, so they can be satisfied, blocked, or given up, and they must compete
when they conflict. That is why volition — and only volition — needs arbitration
tiers, qualification thresholds, satisfaction evidence, cooldowns, and coherence
checks between goals.

A litmus test for where an item belongs:

| Question | If yes, it belongs in |
|---|---|
| Can it be true, false, stale, or superseded? | Memory |
| Can it be satisfied, blocked, or retired? | Volition |
| Does it need provenance and a trust tier? | Memory |
| Does it need to compete for the turn and lose? | Volition |

You cannot satisfy a memory, and you cannot arbitrate between memories. That
asymmetry is the boundary.

### Durability is not the axis

Both systems span the full durability range: memory runs from protected project
facts down to world observations with a seven-day half-life; volition runs from
permanent tensions down to a live-formed goal retired after a few ticks. Trying to
separate the systems by lifetime ("stable things are personality, fleeting things
are memories") therefore fails immediately. Lifetime is a property *within* each
system, not the difference *between* them.

### No dual residence

A single item never lives in both stores.

- A goal does not carry facts. It carries evidence *references* that point at
  facts (transcript turns, memory records, trace records).
- A memory does not carry an intention. Memory may hold records *about* volition —
  "a goal to X was pursued and satisfied on date Y" is a perfectly good episodic
  fact — but such a record is a description of a commitment, not the commitment
  itself. Deleting it does not retire the goal; retiring the goal does not falsify
  it.

### Two directed interfaces, nothing else

Cross-system flow is allowed only through two explicit, directed interfaces:

```text
memory -> volition:  epistemic material (open questions, recurring themes)
                     may be proposed as a goal candidate. The candidate
                     must pass admission and coherence checks before it
                     becomes volitional state. Admission is the conversion
                     ritual from "idea" to "commitment".

volition -> memory:  a goal may request context retrieval. The request is
                     an internal hint that directs the memory system's
                     attention; it never stores or asserts facts.
```

Anything that bypasses these interfaces — goals used to smuggle facts into context,
memories used to carry standing instructions — is a boundary violation.

### Routing rule for live formation

Live goal formation and live memory capture both watch the same conversation, which
is the one place the systems could silently duplicate each other. The routing rule
is **intent**:

- Mere ideation — a topic discussed, an idea floated, a question raised — becomes a
  memory candidate. An idea is epistemic content until someone commits to pursuing
  it.
- Expressed or inferred pursuit — "let's find out", an unresolved thread the
  simulation wants to return to — becomes a goal candidate, carrying evidence
  references back to its source.

The same conversational moment may legitimately produce both: a memory that the
topic arose, and a goal to pursue it. They are then linked by the goal's evidence
reference, not duplicated as two copies of one item.

### Parallel dynamics stay separate

Both systems have decay-like mechanics, and they must not be unified. Memory's
retrieval strength (decay, reinforcement) measures how *findable* a fact is.
Volition's salience (decay, cooldown, retirement) measures how much *pressure* a
commitment currently exerts. The arithmetic is similar; the meaning is not. The
design brief's open question — whether memory salience and goal salience should
share a scoring model — is answered **no**.

## How Volition Drives a Turn

Memory and volition also differ in how they are allowed to influence the model:

- Memory injection is **passive supply**: a compact "relevant memory" block, with
  the instruction to use it only when relevant. Memory never initiates anything.
- Volition injection is **active pressure**: a stance that weights attention and
  framing, at most one arbitration winner per turn, and optionally a single bounded
  initiative line. When no goal qualifies, volition stays quiet rather than
  promoting a weak winner.

This asymmetry is deliberate. Content should be abundant and ignorable; direction
should be scarce and accountable.

## Relationship To Other Documents

- `Architecture.VolitionSystem.md` — structure, crate boundary, and implementation
  status; authoritative for what exists today.
- `Architecture.MemorySystem.md` — the epistemic counterpart of this document's
  subject.
- `Concept.AssociativeMemory.md` — why memory matters; this document is its
  conative sibling.
- `docs/Plans/Idea.VolitionGoalSystem.md` — the original exploration. Its "Goals,
  Attention, And Memory" section leaned toward goals being "both state and memory
  records"; that leaning is superseded by the boundary above (goals are separate
  state, linked to memory only by evidence references).
- `docs/volition_goal_system_design_brief.md` — the broader design brief, including
  memory-driven goal formation.
- `docs/Glossary.md` — definitions of the volition and memory vocabularies.
- `docs/DecisionLog.md` — "2026-05-16 - Goal systems become inspectable simulation
  state", "2026-07-03 - A persona is fixture data", and "2026-07-30 - Memory and
  volition are distinct subsystems".

## Open Questions

- How should live formation classify ideation versus pursuit reliably, and how
  should misroutings be detected after the fact?
- Should the tension set be documented explicitly as the personality surface, so
  persona work stops being described in terms of "goals"?
- How strong must satisfaction evidence be before a goal retires on it?
- Semantic (non-lexical) goal activation remains future work
  (`docs/Plans/Idea.SemanticGoalActivation.md`); the boundary above is unaffected
  by how activation is scored.
