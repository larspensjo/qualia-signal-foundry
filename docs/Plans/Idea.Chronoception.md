# Idea: A Grasp of Time (Chronoception)

## Status

Brainstorm

The objective-temporal-grounding slice of this idea is now in design at
`docs/Plans/Design.Chronoception.md`. The rest of this document is preserved as
the wider idea space, including facets and alternatives the design deliberately
set aside.

## Summary

Qualia Signal Foundry could give the simulated being an explicit **grasp of
time** — a perceived, reasoned-about sense of *when* it is, rather than time
acting on it only as hidden machinery.

Today time exists in the system mechanically but not experientially. Timestamps
drive memory recency decay (`MemoryRecord.last_reinforced_at`), tool
observations expire, there is a `TimerElapsed` event and a session `start_time`.
Time acts *on* the system as decay math, but is never *presented to* the
cognition as a signal it can attend to. The being cannot tell that a session
resumed after three days versus three minutes, that it is the middle of the
night, or how long ago a memory formed.

For a project whose thesis is presence and continuity, that is a meaningful gap:
the felt difference between "we just spoke" and "you have been gone a week" is a
large part of what makes something feel like a continuous entity rather than a
stateless responder.

## Why This Matters

A grasp of time connects directly to several of the project's stated research
themes — session continuity, being situated in time, and presence over task
completion. It could make interaction feel less like prompting a fresh model and
more like resuming with someone who has been *somewhere in time* since you last
spoke.

It is also conceptually load-bearing rather than cosmetic: continuity across
gaps is exactly where a sense of time earns its keep.

## Facets of a Grasp of Time

"A grasp of time" is not one capability. It splits into fairly distinct facets,
each with a different natural home in the architecture:

- **Situatedness (the now)** — knowing and feeling *when* it currently is: time
  of day, date, day of week. The mind has a present moment, not just a context
  window.
- **Gaps between sessions** — sensing how long since the last interaction, and
  experiencing that gap as elapsed time rather than instant resumption.
- **Temporal memory** — ordering and dating the past: how old a memory is, "the
  first time we discussed X". Memories carry a felt distance, not only a decay
  weight.
- **Subjective / felt time** — experienced duration diverging from clock time:
  engagement compresses, waiting stretches, sleep is a discontinuity. Time as
  something the being *feels*, not just measures.

The first three are *objective temporal grounding*: the system knows real facts
about *when* and presents them to the mind. The fourth is more speculative and
is deferred — it is easier to layer felt-time on top of a solid objective
grounding than to start there.

## Design Dimensions and Options

These are the forks the brainstorm explored. They are recorded here as the
option space; `Design.Chronoception.md` commits to one path through them.

### Where "now" comes from

- An **injectable clock** (real wall-clock when live, scriptable when replaying).
- Reading the **real OS clock** directly wherever needed.
- **Fully virtual** experiment time that never touches the OS.

Tension: the project values replayable, deterministic runs, which pulls against
reading a real clock. An injectable clock reconciles the two.

### How time reaches the mind

- **Layered, per facet** — a small always-on temporal frame for situatedness, a
  resume event for gaps, relative-time annotations for temporal memory.
- **Ambient only** — one always-on block carries everything.
- **Pull / time-sense tool** — the mind queries time on demand, fitting
  tools-as-perception.
- **Event-driven only** — time enters purely as salient perception events.

The facets do not all want the same channel, which favours a layered approach.

### What texture the perceived time has

- **Human-fuzzy** — "a while ago", "late evening" (exact timestamps kept
  underneath).
- **Tiered by distance** — precise near the present, fuzzy far away.
- **Exact / super-human** — the being knows precise times and durations
  directly; precision becomes a characterising trait. The project's NonGoals
  explicitly permit exploring super-human cognition.

### How much dedicated machinery time gets

- A **dedicated module** owning the clock and time derivation, with thin
  consumers at the delivery points.
- **Threaded through** existing modules with no new home.
- A **first-class temporal signal** recomputed every turn into observable state,
  with a landmark-event stream — the maximal version.

### Smaller dimensions

- **Timezone** for situatedness: a configured home timezone, host local, or UTC.
- **Gap anchor**: measure from the last exchange, the session's end, or the last
  user input specifically.

## Speculative and Future Directions

Beyond the first objective-grounding cut, a grasp of time could grow toward:

- **Subjective / felt time** — modelling experienced duration, boredom,
  engagement, and sleep-as-discontinuity.
- **Anticipation / future orientation** — expecting return ("you said you would
  be back tomorrow"), scheduling, looking forward.
- **Longer temporal structure** — session counts and cadence ("our 7th session
  across 23 days"), noticing the user's typical rhythms.
- **Temporal landmarks** — salient crossings (midnight, a new day, a long
  silence) emitted as their own perception events.
- **A first-class temporal signal** surfaced in a live inspection dashboard.

These are possibilities, not commitments.

## Open Questions

- Does an exact, ever-present sense of time actually change the being's
  behaviour in interesting ways, or is it mostly cosmetic? (Worth an experiment.)
- How much temporal detail can sit in scarce live context before it is noise?
- Should every resume surface a gap, or only gaps past some threshold?
- How should subjective time, if pursued, relate to the sleep/consolidation
  phase, which already represents a between-sessions discontinuity?
- What is the right module name — `chronoception` (evocative, parallels "tools
  as perception") or the plainer `temporal`?

## Relationship to Existing Concepts

- `Concept.RealtimePresence.md` — already treats timing, silence, and pacing as
  presence signals within a session; a grasp of time extends that *across*
  sessions.
- `Architecture.MemorySystem.md` — recency and decay already use time
  mechanically; this idea makes that passage of time perceptible to the mind.
- `Concept.SleepPhase.md` — the sleep phase is itself a between-sessions
  discontinuity and a natural partner for any future felt-time work.
- `NonGoals.md` — the explicit permission to explore super-human cognition that
  the exact-texture direction draws on.

## Current Cut

The committed first slice — objective temporal grounding via an injectable
clock, a layered per-facet delivery, and an exact/super-human texture — is
specified in `docs/Plans/Design.Chronoception.md`.
