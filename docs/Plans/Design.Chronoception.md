# Design: Chronoception (A Grasp of Time)

## Status

Candidate

> Working module name: `chronoception` (perception of time, parallel to the
> project's "tools as perception" framing). Plain alternative: `temporal`. The
> name is the one open naming decision; see *Open decisions*.

The broader brainstorm in `docs/Plans/Idea.Chronoception.md` is preserved as the
wider idea — all four facets (including the deferred subjective/felt time) and
the alternatives weighed for clock source, delivery, texture, and architecture.
This design narrows that idea to the first committed cut: objective temporal
grounding only.

## Summary

Give the simulated being an explicit, first-class **grasp of time**. Today time
exists in Qualia Signal Foundry only *mechanically* — timestamps drive memory
recency decay (`MemoryRecord.last_reinforced_at`), tool observations expire,
there is a `TimerElapsed` event and a session `start_time`. Time acts *on* the
system as decay math but is never *presented to* the cognition as a signal it
can perceive and reason about. The being cannot tell that a session resumed
after three days versus three minutes, that it is 2am, or how long ago a memory
formed.

This design introduces a small dedicated `chronoception` module that owns the
single source of "now" (an injectable `Clock`) and a pure derivation of exact
temporal facts. Three thin delivery points consume it, one per facet of "a grasp
of time":

- **Situatedness** — an always-on *temporal frame* assembled into live context.
- **Gaps** — a `SessionResumedAfterGap` perception event at session start.
- **Temporal memory** — exact age annotations attached to retrieved memories.

The being's *perceived* sense of time is **exact / super-human**: it does not
estimate ("a while ago") but knows ("14h 22m since our last exchange", "this
memory formed 4d 2h ago"). Precision is itself a characterizing trait.

The feature is deterministic despite being about real time: the clock is read
only at the input boundary, the resulting timestamp flows inward as data, and
every clock read is logged so a `FixedClock`-driven replay reproduces the exact
temporal frame.

## Scope

In scope:

- A new `crates/qsf_app/src/chronoception/` module owning the `Clock` trait
  (`SystemClock`, `FixedClock`), a pure `TemporalFacts` deriver, and frame
  rendering.
- An always-on temporal frame delivered as a new high-priority
  `ContextSourceKind::TemporalFrame` fragment through the existing context
  assembler.
- A `SessionResumedAfterGap` perception event computed at session resume,
  reduced into observable `SessionState`, and surfaced once to the cognition.
- Exact age annotations attached to retrieved memory fragments.
- Routing existing ad-hoc clock reads (e.g.
  [run_context.rs:42](../../crates/qsf_app/src/runtime/run_context.rs#L42))
  through the injectable `Clock`.
- A configured "home" timezone (default: host local), recorded per run for
  replay.
- Event-log + report observability for everything the being knows about time.

Not in scope for this design:

- **Subjective / felt time** — experienced duration diverging from clock time
  (engagement compression, waiting dilation, sleep as discontinuity). Explicitly
  deferred; the objective grounding here is the foundation it would layer on.
- A continuously-recomputed per-turn `TemporalPerception` state object and
  landmark-event stream (the "first-class temporal signal" maximal version).
  This design is the seed that can grow into it once there is evidence an exact
  time sense changes behavior usefully.
- Any change to how decay/reinforcement math itself works.
- Anticipation / scheduling / future-orientation ("you said you'd return
  tomorrow").

## Goal

When the being acts, it should know — exactly and correctly — *when* it is,
*how long* it has been since it last interacted, and *how old* the things it
remembers are, and it should be able to weave that into how it responds, the way
a continuous entity situated in time would.

## Architecture

### Module boundary

`chronoception` owns exactly three things and nothing else:

- **`Clock`** — the single source of "now":
  ```rust
  pub trait Clock {
      fn now(&self) -> time::OffsetDateTime; // UTC
  }
  pub struct SystemClock;                    // reads the OS clock (live runs)
  pub struct FixedClock { /* scripted instants */ } // tests & replay
  ```
- **`TemporalFacts`** — a *pure* deriver. Given `(now, home_tz,
  session_started_at, last_exchange_at, optional memory_timestamp)` it computes
  exact facts: local time-of-day, day-of-week, session elapsed, gap since last
  exchange, memory age. No I/O, no clock read inside — fully unit-testable.
- **Rendering** — turning `TemporalFacts` into the strings the cognition sees
  and into structured state for observability.

The three delivery sites are thin consumers. They *call* chronoception; they do
not compute time themselves. This keeps "what does the being know about time" in
one home and the derivation DRY.

### The determinism rule (the one principle everything obeys)

**The clock is read only at the input boundary; the resulting timestamp travels
inward as data.** Reducers never call `clock.now()` — a `now` value arrives *in*
the action/event and the reducer stays pure (honoring the project's "reducers
must stay pure" rule). Every clock read is written to the per-run event log, so a
replay driven by `FixedClock` reproduces the exact same temporal frame. This is
what lets "it is exactly 02:14:33, 14h 22m since we last spoke" be both *real*
when live and *bit-identical* when replayed.

### Data model

```text
Clock (trait)              -> SystemClock | FixedClock
HomeTimezone               // configured; default host local; recorded per run

TemporalFrame {            // situatedness — the always-on ambient block
  now_utc, now_local, time_of_day, day_of_week,
  session_started_at, session_elapsed,
  since_last_exchange,     // exact, e.g. 2d 11h 44m
}

SessionResumedAfterGap {   // gaps — a salient perception event
  previous_exchange_at, gap
}                          // -> reducer -> SessionState.last_gap (observable)

MemoryTemporalAnnotation { // temporal memory — attached at retrieval
  occurred_at, age         // exact delta vs now
}

ContextSourceKind::TemporalFrame  // new, top source_priority; tiny; selected first
```

### Data flow, per facet

- **Situatedness:** the input boundary stamps `now` →
  [chronoception] derives `TemporalFacts` → a `TemporalFrame`
  `ContextFragment` (new top-priority `ContextSourceKind`) is assembled into
  context every turn via
  [context_assembler.rs](../../crates/qsf_app/src/context/context_assembler.rs).
  Always present, cheap, never evicted because it is tiny and ranks first.
- **Gaps:** at session start,
  [resume.rs](../../crates/qsf_app/src/session/resume.rs) reads the previous
  session's **last exchange timestamp**, diffs it against `now`, and emits
  `SessionResumedAfterGap`. The reducer writes it into `SessionState`; the
  context builder surfaces it **once**, as a notification ("resumed after
  2d 11h"), not a standing fact.
- **Temporal memory:**
  [retrieval.rs](../../crates/qsf_app/src/memory/retrieval.rs) annotates each
  retrieved record with exact `occurred_at` + `age` derived from its timestamp
  vs `now`; the annotation rides along on the memory fragment into context.

## Decisions captured by this design

These were settled during brainstorming and should be promoted to `DecisionLog.md`
when the implementation plan lands:

1. **Facets:** build objective temporal grounding — situatedness, session gaps,
   temporal memory. Defer subjective/felt time.
2. **Clock source:** an **injectable `Clock`** (real wall-clock when live,
   scriptable when replaying). No direct OS clock reads outside the boundary.
3. **Delivery:** **layered, per-facet** (frame / event / annotation), not one
   uniform channel.
4. **Perceived texture:** **exact / super-human**. The being knows precise times
   and durations rather than fuzzy approximations.
5. **Architecture:** dedicated `chronoception` module owning Clock + pure
   derivation; delivery sites are thin consumers. Phased.
6. **Timezone:** a **configured "home" timezone** (default: host local),
   recorded per run so situatedness is meaningful and replay-safe. Storage stays
   UTC.
7. **Gap anchor:** measured from the **last exchange timestamp** of the previous
   session.
8. **Determinism rule:** clock read only at the boundary; timestamp flows as
   data; reducers stay pure; clock reads logged for replay.

## Defaults exercise the new path

Per the repo rule that defaults must exercise new code: the temporal frame is
**on by default**, the gap event **fires by default**, and memory annotations are
**on by default**. There is no opt-in flag hiding the feature — a default run
shows temporal behavior. The home timezone defaults to host local so situatedness
works out of the box.

## Phasing & verification

Each phase is an independently shippable, testable end-to-end slice.

### P1 — Clock + temporal frame (situatedness)

Introduce `Clock`, route ad-hoc `now()` reads (starting with
[run_context.rs:42](../../crates/qsf_app/src/runtime/run_context.rs#L42))
through it, derive `TemporalFacts`, add the `TemporalFrame` fragment.

- *Automated:* pure unit tests on `TemporalFacts`; a `FixedClock` assembly test
  asserting the exact frame string; reducer-purity test (no clock read inside).
- *Human (recommended):* run a live text loop and observe whether the being
  references time naturally and correctly.

### P2 — Gap event

Compute the gap at resume from the previous last-exchange timestamp, emit
`SessionResumedAfterGap`, reduce into `SessionState.last_gap`, surface once. If
the persisted previous `SessionState` does not already carry a last-exchange
timestamp, P2 includes persisting it (verify against the session-state schema
before building the gap math).

- *Automated:* gap-math unit tests; reducer test that the event sets `last_gap`;
  a two-session test with `FixedClock` advanced between sessions.
- *Human (recommended):* end a session, resume later, confirm it acknowledges
  the gap plausibly.

### P3 — Memory annotations

Annotate retrieved records with exact age.

- *Automated:* annotation-delta unit tests; a retrieval test asserting fragments
  carry `age`.
- *Human (recommended):* trigger recall of an older memory and see whether it
  situates it ("4d 2h ago").

## Testing strategy

`FixedClock` is the workhorse: every temporal test injects scripted instants, so
the whole feature is deterministic despite being about real time. Derivation and
rendering are pure functions (golden/snapshot tests for frame strings). Reducer
tests confirm purity. This module is the regression-test home for any future time
bug.

## Observability

Time becomes inspectable state, per the "make internal state observable"
non-goal:

- `TemporalFrame` and `SessionState.last_gap` are serializable and land in the
  per-run event log (new `TemporalFrameComputed`, `SessionResumedAfterGap` event
  types).
- The markdown report
  ([markdown_report.rs](../../crates/qsf_app/src/reports/markdown_report.rs))
  gains a short "Temporal" line per turn — a researcher reading a run sees
  exactly what the being knew about time when it acted.
- Because every clock read is logged, "why did it say it was 2am?" is always
  answerable from the trace.

## Open decisions

1. **Module name** — `chronoception` (evocative, parallels "tools as
   perception") vs `temporal` (plain). To settle at review.
2. **Gap notification on tiny gaps** — with the exact/super-human texture the
   default is to always emit `SessionResumedAfterGap` (the being notices every
   gap) and let the cognition decide what is worth mentioning. Revisit if it
   proves noisy; a threshold would be the mitigation.
3. **Timezone representation** — fixed UTC offset vs IANA zone name in config.
   Implementation detail for the plan; IANA is more correct across DST.

## Documentation workflow

Per `docs/ProjectFrame/ProjectWorkflow.md`, the implementation plan should also:

- Add a `DecisionLog.md` entry capturing the decisions above (injectable clock;
  exact/super-human texture; configured home timezone; last-exchange gap anchor;
  clock-at-the-boundary determinism rule).
- Refresh the *Not yet implemented* band of
  [Architecture.Overview.md](../../docs/Architecture/Architecture.Overview.md)
  (a grasp of time / temporal signal moves toward implemented) and, once built,
  add an `Architecture.Chronoception.md` with an *Implementation Status* section.
- Add an `EngineeringDiary.md` entry per logical change.
- Optionally promote the deferred *subjective/felt time* facet into a
  `Concept.SubjectiveTime.md` so the wider idea is preserved.
- Consider an `Experiment.*` that exercises the temporal frame + gap event
  end-to-end and produces an inspectable run.

## Related documents

- `docs/Concepts/Concept.RealtimePresence.md` — timing as a presence signal;
  chronoception extends presence across session gaps.
- `docs/Architecture/Architecture.MemorySystem.md` — recency/decay; the existing
  mechanical use of time this design makes perceptible.
- `docs/Architecture/Architecture.RuntimeLoop.md` — the unidirectional loop and
  `TimerElapsed` event the Clock plugs into.
- `docs/ProjectFrame/NonGoals.md` — the explicit permission to explore
  super-human cognition that the exact-texture choice draws on.
