# Diagnostics Transition Visualization Implementation Plan

> **For agentic workers (advisory):** If available, superpowers:subagent-driven-development or superpowers:executing-plans is the recommended way to execute this plan task-by-task. Workers without those skills implement the tasks in order, following the repo workflow gates in `Agents.md`. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the single overwritten "Last event" field in the realtime Diagnostics card with a collapsed recent-event ticker plus a 60-second runtime-phase swimlane, so state transitions that currently flash by become reviewable.

**Architecture:** The realtime UI is a pure reducer (`reduceConversationState`) + pure selectors + a dumb `render()` in vanilla TypeScript. This slice (1) adds reducer-owned history — `eventLog` (collapsed, capped, newest-first) and `phaseTimeline` (runtime-phase segments, time-pruned) — with wall-clock timestamps carried **on the actions** (stamped at the dispatch sites, the same isolated-side-effect pattern as `applyMicrophoneMute`), keeping the reducer pure and deterministic; (2) adds two pure view-model selectors, `selectEventTickerModel` and `selectPhaseLaneModel`, that own all formatting and geometry (x-positions as fractions in [0, 1]); (3) renders the ticker as a DOM list in `render()` and the swimlane on a `<canvas>` redrawn by a 100 ms interval — the one clock-driven loop, and it stays a dumb consumer of the selector. The `lastEvent` state field is removed; the ticker replaces it.

**Tech Stack:** TypeScript under `crates/qsf_realtime_server/ui/` (Vite, Vitest, Biome). No Rust changes.

**Spec:** Design approved conversationally 2026-07-05 (event ticker + phase swimlane combo; interactive mockups reviewed). No `Design.*.md` per user request — this plan is the authoritative description.

**History policy (approved):** history is *kept* after Stop (so the just-ended session can be reviewed) and *cleared* on `session_allocated` — consistent with how `latestTurnContext` / `latestVolitionState` are cleared today. Lane window is 60 s.

**Documents to update (per `docs/ProjectFrame/ProjectWorkflow.md`):** this is routine engineering with no simulation-mechanism question → **no Experiment doc**. One `docs/DecisionLog.md` entry (Task 3.3). `docs/Architecture/Architecture.StateAndObservability.md` describes *server-side* diagnostics artifacts, not the browser card — not touched. This plan is ephemeral; durable docs and code must not cite its phase numbers.

## Global Constraints

- All code changes live under `crates/qsf_realtime_server/ui/src/`. Run all npm commands from `crates/qsf_realtime_server/ui/`. When launching npm through `Start-Process`, use `npm.cmd` explicitly.
- After each task: `npm run test` must pass. On task completion also run `npm run check` (tsc + Biome) and `npm run fmt`.
- At plan completion run `cargo build`, then `cargo clippy --all-targets -- -D warnings`, then `cargo fmt` from the repo root (no Rust is touched; this is the standing repo gate).
- `npm run test` is Vitest only — it does not type-check. Wherever a checkpoint expects a TypeScript error, the command that surfaces it is `npm run check` (`tsc --noEmit`); `npm run test` covers runtime reducer/selector assertions.
- Reducers stay pure: no `Date.now()` inside `realtime.ts`. Timestamps enter via the `atMs` action field, stamped only in `main.ts` dispatch sites and only as `Date.now()`.
- Keep view logic in pure selectors: the canvas code multiplies fractions by pixel width and picks colors — it makes no geometry or formatting decisions.
- UI testing policy: test reducers and selectors; no canvas-render or DOM-structure tests.
- TDD throughout: write the failing test, watch it fail, implement, watch it pass, commit. One commit per task.

## File Structure

**Modified:**
- `crates/qsf_realtime_server/ui/src/realtime.ts` — state fields `eventLog` / `phaseTimeline`; `atMs` on four actions; helpers `appendEventLog` / `appendPhaseTimeline` / `prunePhaseTimeline`; selectors `selectEventTickerModel` / `selectPhaseLaneModel`; `lastEvent` removed (Task 3.1). Each `EventLogEntry` carries the reducer-derived `phase` so tick coloring reflects the actual transition (no static kind→phase lookup).
- `crates/qsf_realtime_server/ui/src/realtime.test.ts` — new describe blocks per task; existing action literals gain `atMs`.
- `crates/qsf_realtime_server/ui/src/main.ts` — dispatch sites stamp `atMs`; Diagnostics card markup (ticker list + lane canvas); ticker rendering in `render()`; lane attach call.
- `crates/qsf_realtime_server/ui/src/styles.css` — phase color custom properties, ticker and lane styles.
- `docs/DecisionLog.md` — one entry (Task 3.3).

**Created:**
- `crates/qsf_realtime_server/ui/src/phase-lane.ts` — the only clock/canvas module: attaches the redraw interval, mouse hover, and drawing to a canvas; consumes `selectPhaseLaneModel` only.

---

## Phase 1 — Reducer-owned transition history

Pure state only; the UI still renders the old "Last event" field (removed in Phase 3). Verifiable entirely by `npm run test` + `npm run check`.

### Task 1.1: Event log with burst collapsing

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts`
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces:**
- Consumes: existing `ConversationState`, `ConversationAction`, `RelayEnvelope`.
- Produces (later tasks rely on these exact names):
  - `export interface EventLogEntry { kind: string; phase: RuntimePhase; firstAtMs: number; lastAtMs: number; count: number; }` — `phase` is the runtime phase *after* the reducer applied the event (reducer-derived, not a kind lookup: `response_completed` lands in `speaking` or `idle` depending on `status`).
  - `ConversationState.eventLog: EventLogEntry[]` — newest first, collapsed, capped.
  - `export const EVENT_LOG_LIMIT = 14;`
  - Actions `provider_envelope`, `connection_error`, `stop_requested`, `stopped` each gain required `atMs: number`.
  - Lifecycle marker kinds logged: `"stopping"`, `"stopped"`, `"connection_error"`.

- [ ] **Step 1: Write the failing tests**

Append to `realtime.test.ts`:

```ts
describe("diagnostics event log", () => {
  function envelopeOfKind(kind: RelayEventKind): RelayEnvelope {
    return { qsf_session_id: "session_1", event_id: `evt_${kind}`, kind };
  }
  function withEnvelope(
    state: ConversationState,
    kind: RelayEventKind,
    atMs: number,
  ): ConversationState {
    return reduceConversationState(state, {
      type: "provider_envelope",
      envelope: envelopeOfKind(kind),
      atMs,
    });
  }

  it("appends distinct events newest-first with timestamps", () => {
    const first = withEnvelope(INITIAL_STATE, "user_turn_started", 1_000);
    const second = withEnvelope(first, "final_transcript", 3_500);
    expect(second.eventLog).toEqual([
      { kind: "final_transcript", phase: "thinking", firstAtMs: 3_500, lastAtMs: 3_500, count: 1 },
      { kind: "user_turn_started", phase: "listening", firstAtMs: 1_000, lastAtMs: 1_000, count: 1 },
    ]);
  });

  it("collapses a burst of one kind into a single counted row", () => {
    let state = withEnvelope(INITIAL_STATE, "user_turn_started", 1_000);
    state = withEnvelope(state, "partial_transcript", 1_200);
    state = withEnvelope(state, "partial_transcript", 1_450);
    state = withEnvelope(state, "partial_transcript", 1_700);
    expect(state.eventLog).toEqual([
      { kind: "partial_transcript", phase: "listening", firstAtMs: 1_200, lastAtMs: 1_700, count: 3 },
      { kind: "user_turn_started", phase: "listening", firstAtMs: 1_000, lastAtMs: 1_000, count: 1 },
    ]);
  });

  it("records the reducer-derived phase, not a static kind lookup", () => {
    // response_completed with a non-completed status transitions to idle, not
    // speaking — the log entry must reflect the transition the reducer made.
    const listening = withEnvelope(INITIAL_STATE, "user_turn_started", 1_000);
    const cancelled = reduceConversationState(listening, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_cancelled",
        kind: "response_completed",
        status: "cancelled",
      },
      atMs: 2_000,
    });
    expect(cancelled.eventLog[0]).toMatchObject({ kind: "response_completed", phase: "idle" });
  });

  it("caps the log at EVENT_LOG_LIMIT rows, dropping the oldest", () => {
    let state = INITIAL_STATE;
    // Alternate two kinds so no collapsing happens.
    for (let i = 0; i < EVENT_LOG_LIMIT + 2; i++) {
      const kind = i % 2 === 0 ? "user_turn_started" : "final_transcript";
      state = withEnvelope(state, kind, 1_000 + i);
    }
    expect(state.eventLog).toHaveLength(EVENT_LOG_LIMIT);
    expect(state.eventLog[0].lastAtMs).toBe(1_000 + EVENT_LOG_LIMIT + 1);
    expect(state.eventLog.at(-1)?.lastAtMs).toBe(1_002);
  });

  it("logs lifecycle markers for stop, stopped, and errors", () => {
    let state = reduceConversationState(INITIAL_STATE, { type: "stop_requested", atMs: 5_000 });
    state = reduceConversationState(state, { type: "stopped", atMs: 6_000 });
    state = reduceConversationState(state, {
      type: "connection_error",
      message: "relay socket closed",
      atMs: 7_000,
    });
    expect(state.eventLog.map((entry) => entry.kind)).toEqual([
      "connection_error",
      "stopped",
      "stopping",
    ]);
  });

  it("clears the log when a new session is allocated", () => {
    const seeded = withEnvelope(INITIAL_STATE, "user_turn_started", 1_000);
    const allocated = reduceConversationState(seeded, {
      type: "session_allocated",
      sessionId: "session_2",
    });
    expect(allocated.eventLog).toEqual([]);
  });
});
```

Add to the existing import block from `./realtime`: `EVENT_LOG_LIMIT`, and the types `type ConversationState`, `type RelayEnvelope`, `type RelayEventKind` if not already imported.

- [ ] **Step 2: Run tests to verify they fail**

Run (from `crates/qsf_realtime_server/ui/`): `npm run check`
Expected: FAIL — TypeScript errors (`atMs` not in action type, `eventLog` not on state, `EVENT_LOG_LIMIT` not exported). These are type errors, so `npm run check` is the gate that reports them; Vitest does not type-check.
Run: `npm run test`
Expected: FAIL — the new describe block cannot pass (missing `EVENT_LOG_LIMIT` export breaks the import; `eventLog` assertions see `undefined`).

- [ ] **Step 3: Implement**

In `realtime.ts`:

(a) Below the `TranscriptEntry` interface, add:

```ts
/// One collapsed row of the diagnostics event ticker. Consecutive events of the
/// same kind merge into a single row so a partial_transcript burst stays readable.
export interface EventLogEntry {
  /// Relay event kind, or a lifecycle marker: "stopping", "stopped", "connection_error".
  kind: string;
  /// Runtime phase after the reducer applied this event — the transition the
  /// reducer actually made, not a kind lookup (response_completed lands in
  /// speaking or idle depending on status). For a collapsed burst this is the
  /// phase after the most recent occurrence.
  phase: RuntimePhase;
  /// Wall-clock ms of the first occurrence in this collapsed run.
  firstAtMs: number;
  /// Wall-clock ms of the most recent occurrence in this collapsed run.
  lastAtMs: number;
  count: number;
}

export const EVENT_LOG_LIMIT = 14;
```

(b) In `ConversationState`, after `lastEvent: string | null;` add:

```ts
  /// Newest-first collapsed history of relay/lifecycle events (see EventLogEntry).
  /// Kept after stop for post-hoc review; cleared when a new session is allocated.
  eventLog: EventLogEntry[];
```

and in `INITIAL_STATE` add `eventLog: [],`.

(c) Change the four action variants:

```ts
  | { type: "provider_envelope"; envelope: RelayEnvelope; atMs: number }
  | { type: "connection_error"; message: string; atMs: number }
  | { type: "stop_requested"; atMs: number }
  | { type: "stopped"; atMs: number }
```

(d) Add the pure helper near `appendTranscript`:

```ts
function appendEventLog(
  log: EventLogEntry[],
  kind: string,
  atMs: number,
  phase: RuntimePhase,
): EventLogEntry[] {
  const head = log[0];
  if (head !== undefined && head.kind === kind) {
    return [{ ...head, lastAtMs: atMs, count: head.count + 1, phase }, ...log.slice(1)];
  }
  return [{ kind, phase, firstAtMs: atMs, lastAtMs: atMs, count: 1 }, ...log].slice(
    0,
    EVENT_LOG_LIMIT,
  );
}
```

(e) Wire the reducer cases. The log entry needs the phase *after* the envelope is applied, so the per-kind switch is split out now (Task 1.2 reuses this structure):

- `session_allocated`: add `eventLog: [],` to the returned object.
- `provider_envelope`: change to `return applyRelayEnvelope(state, action.envelope, action.atMs);` and restructure `applyRelayEnvelope` into a wrapper that appends history from the resulting state:

```ts
function applyRelayEnvelope(
  state: ConversationState,
  envelope: RelayEnvelope,
  atMs: number,
): ConversationState {
  const base = {
    ...state,
    lastEvent: envelope.kind,
  };
  const next = applyRelayEnvelopeKind(base, envelope);
  return {
    ...next,
    eventLog: appendEventLog(state.eventLog, envelope.kind, atMs, next.phase),
  };
}

/// The pre-existing per-kind switch, unchanged except for the rename; it
/// receives `base` directly (the old `const base = ...` moves to the wrapper).
function applyRelayEnvelopeKind(
  base: ConversationState,
  envelope: RelayEnvelope,
): ConversationState {
  switch (envelope.kind) {
    // ... existing cases verbatim, each returning { ...base, ... } ...
  }
}
```

- `connection_error`: add `eventLog: appendEventLog(state.eventLog, "connection_error", action.atMs, state.phase),` (the runtime phase is unchanged by this case).
- `stop_requested`: add `eventLog: appendEventLog(state.eventLog, "stopping", action.atMs, state.phase),` (only `connection` changes here).
- `stopped`: add `eventLog: appendEventLog(state.eventLog, "stopped", action.atMs, "idle"),` (this case sets `phase: "idle"`).

(`lastEvent` stays for now; it is removed with its renderer in Task 3.1.)

(f) In `main.ts`, stamp every dispatch of the four actions with `atMs: Date.now()`. Sites: the `provider_envelope` dispatch in the data-channel `message` listener; the synthetic `final_transcript` dispatch in `submitTextTurn`; every `connection_error` dispatch (five sites: relay-socket `close` listener, data-channel `message` catch, `startConversation` catch, `submitTextTurn` catch, `stopConversation` catch); `stop_requested` and `stopped` in `stopConversation`.

(g) In `realtime.test.ts`, the compiler now flags every pre-existing literal of these four actions — add `atMs: 0` to each (values are irrelevant to those tests).

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test` — Expected: PASS.
Run: `npm run check` then `npm run fmt`.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/realtime.test.ts crates/qsf_realtime_server/ui/src/main.ts
git commit -m "realtime ui: reducer-owned collapsed event log with action timestamps"
```

### Task 1.2: Runtime-phase timeline with window pruning

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts`
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces:**
- Consumes: Task 1.1's `atMs` action fields.
- Produces:
  - `export interface PhaseSegment { phase: RuntimePhase; startedAtMs: number; }`
  - `ConversationState.phaseTimeline: PhaseSegment[]` — oldest first; the last entry is the current phase since its `startedAtMs`; empty means "idle so far".
  - `export const PHASE_LANE_WINDOW_MS = 60_000;`

- [ ] **Step 1: Write the failing tests**

Append to `realtime.test.ts` (reuse the `envelopeOfKind` / `withEnvelope` helpers by moving them to file scope next to the other top-level helpers, or duplicate them in this describe block — moving to file scope is preferred, DRY):

```ts
describe("diagnostics phase timeline", () => {
  it("appends a segment only when the runtime phase changes", () => {
    let state = withEnvelope(INITIAL_STATE, "user_turn_started", 1_000); // -> listening
    state = withEnvelope(state, "partial_transcript", 1_200); // still listening
    state = withEnvelope(state, "final_transcript", 2_000); // -> thinking
    expect(state.phaseTimeline).toEqual([
      { phase: "listening", startedAtMs: 1_000 },
      { phase: "thinking", startedAtMs: 2_000 },
    ]);
  });

  it("returns to idle when the session stops", () => {
    let state = withEnvelope(INITIAL_STATE, "user_turn_started", 1_000);
    state = reduceConversationState(state, { type: "stopped", atMs: 4_000 });
    expect(state.phaseTimeline.at(-1)).toEqual({ phase: "idle", startedAtMs: 4_000 });
  });

  it("prunes segments that ended before the lane window, keeping the spanning one", () => {
    let state = withEnvelope(INITIAL_STATE, "user_turn_started", 0); // listening @ 0
    state = withEnvelope(state, "final_transcript", 1_000); // thinking @ 1000
    state = withEnvelope(state, "speech_playback_started", 2_000); // speaking @ 2000
    // Same phase much later: no new segment, but pruning runs at atMs.
    state = withEnvelope(state, "speech_playback_started", PHASE_LANE_WINDOW_MS + 1_500);
    // cutoff = 1_500: listening ended at 1_000 (dropped); thinking ended at 2_000 (spans, kept).
    expect(state.phaseTimeline).toEqual([
      { phase: "thinking", startedAtMs: 1_000 },
      { phase: "speaking", startedAtMs: 2_000 },
    ]);
  });

  it("clears the timeline when a new session is allocated", () => {
    const seeded = withEnvelope(INITIAL_STATE, "user_turn_started", 1_000);
    const allocated = reduceConversationState(seeded, {
      type: "session_allocated",
      sessionId: "session_2",
    });
    expect(allocated.phaseTimeline).toEqual([]);
  });
});
```

Add `PHASE_LANE_WINDOW_MS` to the import block.

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run check`
Expected: FAIL — TypeScript errors (`phaseTimeline` not on state, `PHASE_LANE_WINDOW_MS` not exported).
Run: `npm run test`
Expected: FAIL — the new assertions cannot pass (missing export breaks the import; `phaseTimeline` is `undefined`).

- [ ] **Step 3: Implement**

In `realtime.ts`:

(a) Below `EVENT_LOG_LIMIT`, add:

```ts
/// One segment of the runtime-phase swimlane: `phase` holds from `startedAtMs`
/// until the next segment starts (or now, for the last segment).
export interface PhaseSegment {
  phase: RuntimePhase;
  startedAtMs: number;
}

/// Width of the phase-lane display window; also the reducer's pruning horizon.
export const PHASE_LANE_WINDOW_MS = 60_000;
```

(b) In `ConversationState` add:

```ts
  /// Oldest-first runtime-phase history, pruned to the lane window. Empty means
  /// "idle so far". Kept after stop; cleared when a new session is allocated.
  phaseTimeline: PhaseSegment[];
```

and in `INITIAL_STATE` add `phaseTimeline: [],`.

(c) Add the helpers next to `appendEventLog`:

```ts
function appendPhaseTimeline(
  timeline: PhaseSegment[],
  phase: RuntimePhase,
  atMs: number,
): PhaseSegment[] {
  const last = timeline.at(-1);
  const appended =
    last !== undefined && last.phase === phase
      ? timeline
      : [...timeline, { phase, startedAtMs: atMs }];
  return prunePhaseTimeline(appended, atMs);
}

/// Drop segments that ended before the window start, but keep the segment that
/// spans the cutoff so the lane's left edge is still painted.
function prunePhaseTimeline(timeline: PhaseSegment[], nowMs: number): PhaseSegment[] {
  const cutoff = nowMs - PHASE_LANE_WINDOW_MS;
  let firstVisible = 0;
  for (let i = 0; i + 1 < timeline.length; i++) {
    if (timeline[i + 1].startedAtMs <= cutoff) {
      firstVisible = i + 1;
    }
  }
  return firstVisible === 0 ? timeline : timeline.slice(firstVisible);
}
```

(d) Wire the reducer:

- `session_allocated`: add `phaseTimeline: [],`.
- `stopped`: add `phaseTimeline: appendPhaseTimeline(state.phaseTimeline, "idle", action.atMs),`.
- `applyRelayEnvelope` (the wrapper from Task 1.1 already computes `next`): extend its return with the timeline, derived from the same resulting phase the event log records:

```ts
  return {
    ...next,
    eventLog: appendEventLog(state.eventLog, envelope.kind, atMs, next.phase),
    phaseTimeline: appendPhaseTimeline(state.phaseTimeline, next.phase, atMs),
  };
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test` — Expected: PASS.
Run: `npm run check` then `npm run fmt`.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/realtime.test.ts
git commit -m "realtime ui: reducer-owned runtime-phase timeline pruned to lane window"
```

---

## Phase 2 — Pure view-model selectors

All formatting and geometry decisions, unit-tested. Still no visible UI change.

### Task 2.1: Event ticker view-model

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts`
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces:**
- Consumes: `ConversationState.eventLog` (Task 1.1).
- Produces:
  - `export interface TickerRowModel { kind: string; countLabel: string | null; timeLabel: string; deltaLabel: string | null; }`
  - `export function selectEventTickerModel(state: ConversationState): TickerRowModel[]` — newest first, same order as `eventLog`.
  - Module-private `formatClockTime(atMs: number): string` → `"HH:MM:SS.d"` (local time, deciseconds).

- [ ] **Step 1: Write the failing tests**

```ts
describe("selectEventTickerModel", () => {
  // Local-time constructor keeps the expected labels timezone-independent.
  const at = (h: number, m: number, s: number, ms: number) =>
    new Date(2026, 6, 5, h, m, s, ms).getTime();

  it("formats rows with clock time, burst count, and inter-event gap", () => {
    const state: ConversationState = {
      ...INITIAL_STATE,
      eventLog: [
        { kind: "final_transcript", phase: "thinking", firstAtMs: at(12, 0, 5, 200), lastAtMs: at(12, 0, 5, 200), count: 1 },
        { kind: "partial_transcript", phase: "listening", firstAtMs: at(12, 0, 1, 100), lastAtMs: at(12, 0, 3, 100), count: 14 },
      ],
    };
    expect(selectEventTickerModel(state)).toEqual([
      {
        kind: "final_transcript",
        countLabel: null,
        timeLabel: "12:00:05.2",
        // Gap measured against the previous row's *last* occurrence: 5.2 - 3.1 = 2.1 s.
        deltaLabel: "+2.1s",
      },
      {
        kind: "partial_transcript",
        countLabel: "×14",
        timeLabel: "12:00:01.1",
        deltaLabel: null,
      },
    ]);
  });

  it("returns an empty list before any event", () => {
    expect(selectEventTickerModel(INITIAL_STATE)).toEqual([]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test`
Expected: FAIL — `selectEventTickerModel` not exported.

- [ ] **Step 3: Implement**

In `realtime.ts`, near the other selectors:

```ts
export interface TickerRowModel {
  kind: string;
  /// "×N" for a collapsed burst, null for a single occurrence.
  countLabel: string | null;
  /// Local wall-clock "HH:MM:SS.d" of the row's first occurrence.
  timeLabel: string;
  /// "+X.Ys" gap since the previous (older) row's last occurrence; null on the oldest row.
  deltaLabel: string | null;
}

export function selectEventTickerModel(state: ConversationState): TickerRowModel[] {
  return state.eventLog.map((entry, index) => {
    const older = state.eventLog[index + 1];
    return {
      kind: entry.kind,
      countLabel: entry.count > 1 ? `×${entry.count}` : null,
      timeLabel: formatClockTime(entry.firstAtMs),
      deltaLabel:
        older === undefined
          ? null
          : `+${((entry.firstAtMs - older.lastAtMs) / 1000).toFixed(1)}s`,
    };
  });
}

function formatClockTime(atMs: number): string {
  const date = new Date(atMs);
  const pad = (value: number) => String(value).padStart(2, "0");
  const deciseconds = Math.floor(date.getMilliseconds() / 100);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${deciseconds}`;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test` — Expected: PASS. Then `npm run check`, `npm run fmt`.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/realtime.test.ts
git commit -m "realtime ui: event ticker view-model selector"
```

### Task 2.2: Phase-lane view-model

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts`
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces:**
- Consumes: `phaseTimeline`, `eventLog`, `PHASE_LANE_WINDOW_MS`.
- Produces (Task 3.2 relies on these exact names):

```ts
export interface PhaseLaneSegmentModel { phase: RuntimePhase; startFraction: number; endFraction: number; }
export interface PhaseLaneTickModel { fraction: number; kind: string; phase: RuntimePhase; timeLabel: string; }
export interface PhaseLaneGridlineModel { fraction: number; label: string; }
export interface PhaseLaneModel {
  segments: PhaseLaneSegmentModel[];
  ticks: PhaseLaneTickModel[];
  gridlines: PhaseLaneGridlineModel[];
}
export const PHASE_LANE_GRIDLINE_STEP_MS = 15_000;
export function selectPhaseLaneModel(state: ConversationState, nowMs: number): PhaseLaneModel;
```

Tick semantics: one tick per collapsed log entry at `firstAtMs`, plus one at `lastAtMs` when `count > 1` (a burst renders as a start/end pair under its phase band — the band itself conveys the burst).

Tick color semantics (decided): a tick is colored by the **reducer-derived phase after the event** — `EventLogEntry.phase`, recorded when the reducer handled the action (Task 1.1) — not by a static kind→phase mapping. A cancelled/failed `response_completed` therefore shows an `idle` tick, matching the band transition next to it. The selector copies `entry.phase` onto each tick so the canvas needs no event-kind knowledge.

- [ ] **Step 1: Write the failing tests**

```ts
describe("selectPhaseLaneModel", () => {
  const NOW = 100_000; // window start = 40_000 with the 60 s window

  it("maps segments to window fractions and fills the leading gap with idle", () => {
    const state: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [
        { phase: "listening", startedAtMs: 70_000 },
        { phase: "thinking", startedAtMs: 85_000 },
      ],
    };
    expect(selectPhaseLaneModel(state, NOW).segments).toEqual([
      { phase: "idle", startFraction: 0, endFraction: 0.5 },
      { phase: "listening", startFraction: 0.5, endFraction: 0.75 },
      { phase: "thinking", startFraction: 0.75, endFraction: 1 },
    ]);
  });

  it("renders a fully idle lane when the timeline is empty", () => {
    expect(selectPhaseLaneModel(INITIAL_STATE, NOW).segments).toEqual([
      { phase: "idle", startFraction: 0, endFraction: 1 },
    ]);
  });

  it("clamps a segment that started before the window", () => {
    const state: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [{ phase: "speaking", startedAtMs: 10_000 }],
    };
    expect(selectPhaseLaneModel(state, NOW).segments).toEqual([
      { phase: "speaking", startFraction: 0, endFraction: 1 },
    ]);
  });

  it("emits ticks inside the window carrying the reducer-derived phase; a burst row becomes a start/end pair", () => {
    const state: ConversationState = {
      ...INITIAL_STATE,
      eventLog: [
        { kind: "final_transcript", phase: "thinking", firstAtMs: 85_000, lastAtMs: 85_000, count: 1 },
        { kind: "partial_transcript", phase: "listening", firstAtMs: 70_000, lastAtMs: 76_000, count: 9 },
        { kind: "stopped", phase: "idle", firstAtMs: 10_000, lastAtMs: 10_000, count: 1 }, // outside window
      ],
    };
    const ticks = selectPhaseLaneModel(state, NOW).ticks;
    expect(ticks.map((tick) => [tick.kind, tick.phase, tick.fraction])).toEqual([
      ["partial_transcript", "listening", 0.5],
      ["partial_transcript", "listening", 0.6],
      ["final_transcript", "thinking", 0.75],
    ]);
  });

  it("labels gridlines every 15 s back from now", () => {
    expect(selectPhaseLaneModel(INITIAL_STATE, NOW).gridlines).toEqual([
      { fraction: 1, label: "now" },
      { fraction: 0.75, label: "-15s" },
      { fraction: 0.5, label: "-30s" },
      { fraction: 0.25, label: "-45s" },
      { fraction: 0, label: "-60s" },
    ]);
  });
});
```

(No kind→phase mapping test: tick phase comes from `EventLogEntry.phase`, which Task 1.1 already covers — including the cancelled `response_completed` → `idle` regression test.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run test`
Expected: FAIL — `selectPhaseLaneModel` not exported.

- [ ] **Step 3: Implement**

In `realtime.ts`:

```ts
export interface PhaseLaneSegmentModel {
  phase: RuntimePhase;
  startFraction: number;
  endFraction: number;
}

export interface PhaseLaneTickModel {
  fraction: number;
  kind: string;
  /// Reducer-derived phase after the event (copied from EventLogEntry.phase);
  /// the canvas colors the tick with this and needs no event-kind knowledge.
  phase: RuntimePhase;
  timeLabel: string;
}

export interface PhaseLaneGridlineModel {
  fraction: number;
  label: string;
}

export interface PhaseLaneModel {
  segments: PhaseLaneSegmentModel[];
  ticks: PhaseLaneTickModel[];
  gridlines: PhaseLaneGridlineModel[];
}

export const PHASE_LANE_GRIDLINE_STEP_MS = 15_000;

/// Geometry for the phase swimlane, all x-positions as fractions of the lane
/// width in [0, 1] with `now` at 1. The canvas renderer multiplies by pixel
/// width and picks colors; it makes no layout decisions of its own.
export function selectPhaseLaneModel(state: ConversationState, nowMs: number): PhaseLaneModel {
  const windowStartMs = nowMs - PHASE_LANE_WINDOW_MS;
  const fractionOf = (atMs: number) => (atMs - windowStartMs) / PHASE_LANE_WINDOW_MS;
  const clamp01 = (value: number) => Math.min(1, Math.max(0, value));

  const timeline = state.phaseTimeline;
  const segments: PhaseLaneSegmentModel[] = [];
  // Before the first recorded segment the runtime phase was idle (INITIAL_STATE.phase).
  const firstStartMs = timeline.length > 0 ? timeline[0].startedAtMs : nowMs;
  if (firstStartMs > windowStartMs) {
    segments.push({ phase: "idle", startFraction: 0, endFraction: clamp01(fractionOf(firstStartMs)) });
  }
  for (let i = 0; i < timeline.length; i++) {
    const endMs = i + 1 < timeline.length ? timeline[i + 1].startedAtMs : nowMs;
    if (endMs <= windowStartMs) {
      continue;
    }
    segments.push({
      phase: timeline[i].phase,
      startFraction: clamp01(fractionOf(timeline[i].startedAtMs)),
      endFraction: clamp01(fractionOf(endMs)),
    });
  }

  const ticks: PhaseLaneTickModel[] = [];
  for (const entry of state.eventLog) {
    const atMss = entry.count > 1 ? [entry.firstAtMs, entry.lastAtMs] : [entry.firstAtMs];
    for (const atMs of atMss) {
      if (atMs >= windowStartMs && atMs <= nowMs) {
        ticks.push({
          fraction: fractionOf(atMs),
          kind: entry.kind,
          phase: entry.phase,
          timeLabel: formatClockTime(atMs),
        });
      }
    }
  }
  ticks.sort((a, b) => a.fraction - b.fraction);

  const gridlines: PhaseLaneGridlineModel[] = [];
  for (let backMs = 0; backMs <= PHASE_LANE_WINDOW_MS; backMs += PHASE_LANE_GRIDLINE_STEP_MS) {
    gridlines.push({
      fraction: fractionOf(nowMs - backMs),
      label: backMs === 0 ? "now" : `-${backMs / 1000}s`,
    });
  }

  return { segments, ticks, gridlines };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test` — Expected: PASS. Then `npm run check`, `npm run fmt`.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/realtime.test.ts
git commit -m "realtime ui: phase-lane view-model selector with fractional geometry"
```

---

## Phase 3 — Render, styles, docs

Visible UI change. Ends with the human verification pass.

### Task 3.1: Ticker DOM replaces the "Last event" field

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/main.ts`
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts` (remove `lastEvent`)
- Modify: `crates/qsf_realtime_server/ui/src/styles.css`

**Interfaces:**
- Consumes: `selectEventTickerModel` (Task 2.1).
- Produces: Diagnostics card markup with `data-role="event-ticker"`; `ConversationState.lastEvent` no longer exists.

- [ ] **Step 1: Remove `lastEvent` from the state layer**

In `realtime.ts` delete: the `lastEvent: string | null;` field, `lastEvent: null,` in `INITIAL_STATE`, `lastEvent: action.message,` in `connection_error`, `lastEvent: "stopping",` in `stop_requested`, `lastEvent: "stopped",` in `stopped`, and `lastEvent: envelope.kind,` in `applyRelayEnvelope`'s `base`.

- [ ] **Step 2: Swap the markup and renderer in `main.ts`**

In the `root.innerHTML` template, replace:

```html
          <div>
            <dt>Last event</dt>
            <dd data-role="last-event">None yet</dd>
          </div>
```

with:

```html
          <div>
            <dt>Recent events</dt>
            <dd>
              <ol data-role="event-ticker" class="event-ticker" aria-label="Recent relay events"></ol>
            </dd>
          </div>
```

In `UiRefs`, replace `lastEvent: HTMLElement;` with `eventTicker: HTMLOListElement;`, and in `collectRefs` replace the `lastEvent` line with:

```ts
    eventTicker: query<HTMLOListElement>('[data-role="event-ticker"]'),
```

In `render()`, replace `refs.lastEvent.textContent = state.lastEvent ?? "None yet";` with:

```ts
  const tickerRows = selectEventTickerModel(state);
  if (tickerRows.length === 0) {
    const empty = document.createElement("li");
    empty.className = "event-ticker-empty";
    empty.textContent = "None yet";
    refs.eventTicker.replaceChildren(empty);
  } else {
    refs.eventTicker.replaceChildren(
      ...tickerRows.map((row) => {
        const item = document.createElement("li");
        const time = document.createElement("span");
        time.className = "event-ticker-time";
        time.textContent = row.timeLabel;
        const kind = document.createElement("span");
        kind.className = "event-ticker-kind";
        kind.textContent = row.countLabel === null ? row.kind : `${row.kind} ${row.countLabel}`;
        const delta = document.createElement("span");
        delta.className = "event-ticker-delta";
        delta.textContent = row.deltaLabel ?? "";
        item.append(time, kind, delta);
        return item;
      }),
    );
  }
```

Add `selectEventTickerModel` to the `./realtime` import list.

- [ ] **Step 3: Style the ticker**

Append to `styles.css` (after the `.details dd` rule):

```css
.event-ticker {
  margin: 0;
  padding: 0;
  list-style: none;
  display: grid;
  gap: 0.1rem;
  font-family: Consolas, "Cascadia Mono", ui-monospace, monospace;
  font-size: 0.78rem;
  font-variant-numeric: tabular-nums;
}

.event-ticker li {
  display: grid;
  grid-template-columns: 11ch minmax(0, 1fr) auto;
  gap: 0.6rem;
  align-items: baseline;
}

.event-ticker li:first-child .event-ticker-kind {
  color: var(--text);
}

.event-ticker-time,
.event-ticker-delta {
  color: var(--muted);
}

.event-ticker-kind {
  color: var(--accent-2);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.event-ticker li:nth-child(n + 9) {
  opacity: 0.55;
}

.event-ticker-empty {
  color: var(--muted);
  font-style: italic;
}
```

- [ ] **Step 4: Verify**

Run: `npm run test` — Expected: PASS (no test references `lastEvent`).
Run: `npm run check` — Expected: clean (this proves no dangling `lastEvent` reference).
Run: `npm run fmt`.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/main.ts crates/qsf_realtime_server/ui/src/styles.css
git commit -m "realtime ui: recent-events ticker replaces single last-event field"
```

### Task 3.2: Phase swimlane canvas strip

**Files:**
- Create: `crates/qsf_realtime_server/ui/src/phase-lane.ts`
- Modify: `crates/qsf_realtime_server/ui/src/main.ts`
- Modify: `crates/qsf_realtime_server/ui/src/styles.css`

**Interfaces:**
- Consumes: `selectPhaseLaneModel`, `PhaseLaneModel`, `PhaseLaneTickModel`, `ConversationState`, `RuntimePhase` from `./realtime`.
- Produces: `export function attachPhaseLane(canvas: HTMLCanvasElement, tip: HTMLElement, getState: () => ConversationState): void` — attaches a 100 ms redraw interval and hover handling; lives for the page lifetime (no teardown needed; the page is a single long-lived document).

No unit tests for this module (canvas rendering is excluded by the repo's UI testing policy; all logic it consumes is tested in Task 2.2).

- [ ] **Step 1: Add phase color tokens**

In `styles.css`, extend the `:root` block:

```css
  --phase-idle: #64748b;
  --phase-listening: #7dd3fc;
  --phase-thinking: #f59e0b;
  --phase-speaking: #4ade80;
```

- [ ] **Step 2: Create `phase-lane.ts`**

```ts
import {
  type ConversationState,
  type PhaseLaneModel,
  type PhaseLaneTickModel,
  type RuntimePhase,
  selectPhaseLaneModel,
} from "./realtime";

export const PHASE_LANE_REDRAW_INTERVAL_MS = 100;

const BAND_TOP = 14;
const BAND_HEIGHT = 52;
const TICK_TOP = BAND_TOP + BAND_HEIGHT + 8;
const TICK_HEIGHT = 12;
/// Hover match radius for event ticks, in CSS pixels.
const HOVER_RADIUS_PX = 6;

/// Attach the swimlane renderer to its canvas: a 100 ms interval re-derives the
/// lane model (time advances even without actions) and repaints. The module is
/// a dumb consumer of selectPhaseLaneModel — geometry and formatting live there.
export function attachPhaseLane(
  canvas: HTMLCanvasElement,
  tip: HTMLElement,
  getState: () => ConversationState,
): void {
  const context = canvas.getContext("2d");
  if (context === null) {
    return;
  }
  const colors = readPhaseColors(canvas);
  let pointerX: number | null = null;
  let model: PhaseLaneModel = { segments: [], ticks: [], gridlines: [] };

  canvas.addEventListener("mousemove", (event) => {
    pointerX = event.offsetX;
    draw();
  });
  canvas.addEventListener("mouseleave", () => {
    pointerX = null;
    tip.hidden = true;
    draw();
  });

  function draw() {
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (width === 0 || height === 0 || context === null) {
      return;
    }
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * dpr) || canvas.height !== Math.round(height * dpr)) {
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
    }
    context.setTransform(dpr, 0, 0, dpr, 0, 0);
    model = selectPhaseLaneModel(getState(), Date.now());

    context.clearRect(0, 0, width, height);
    context.fillStyle = "rgba(7, 10, 20, 0.55)";
    context.fillRect(0, 0, width, height);

    context.font = '10px Consolas, "Cascadia Mono", ui-monospace, monospace';
    context.textAlign = "center";
    for (const gridline of model.gridlines) {
      const x = gridline.fraction * width;
      context.strokeStyle = "rgba(255, 255, 255, 0.07)";
      context.beginPath();
      context.moveTo(x, BAND_TOP - 6);
      context.lineTo(x, TICK_TOP + TICK_HEIGHT);
      context.stroke();
      context.fillStyle = "rgba(184, 191, 215, 0.65)";
      context.fillText(gridline.label, Math.min(Math.max(x, 16), width - 16), height - 4);
    }

    for (const segment of model.segments) {
      const x1 = segment.startFraction * width;
      const x2 = segment.endFraction * width;
      context.globalAlpha = segment.phase === "idle" ? 0.28 : 0.75;
      context.fillStyle = colors[segment.phase];
      context.fillRect(x1, BAND_TOP, Math.max(1, x2 - x1), BAND_HEIGHT);
      context.globalAlpha = 1;
    }

    for (const tick of model.ticks) {
      const x = tick.fraction * width;
      context.strokeStyle = colors[tick.phase];
      context.lineWidth = 2;
      context.beginPath();
      context.moveTo(x, TICK_TOP);
      context.lineTo(x, TICK_TOP + TICK_HEIGHT);
      context.stroke();
      context.lineWidth = 1;
    }

    updateTip(width);
  }

  function updateTip(width: number) {
    if (pointerX === null) {
      tip.hidden = true;
      return;
    }
    let nearest: PhaseLaneTickModel | null = null;
    let nearestDistance = HOVER_RADIUS_PX;
    for (const tick of model.ticks) {
      const distance = Math.abs(tick.fraction * width - pointerX);
      if (distance < nearestDistance) {
        nearest = tick;
        nearestDistance = distance;
      }
    }
    if (nearest === null) {
      tip.hidden = true;
      return;
    }
    tip.hidden = false;
    tip.style.left = `${nearest.fraction * width}px`;
    tip.style.top = `${TICK_TOP}px`;
    tip.textContent = `${nearest.kind} · ${nearest.timeLabel}`;
  }

  window.setInterval(draw, PHASE_LANE_REDRAW_INTERVAL_MS);
  draw();
}

function readPhaseColors(element: Element): Record<RuntimePhase, string> {
  const style = getComputedStyle(element);
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    idle: read("--phase-idle", "#64748b"),
    listening: read("--phase-listening", "#7dd3fc"),
    thinking: read("--phase-thinking", "#f59e0b"),
    speaking: read("--phase-speaking", "#4ade80"),
  };
}
```

- [ ] **Step 3: Wire it into `main.ts`**

In the `root.innerHTML` template, insert directly after the closing `</dl>` of the details list:

```html
        <div class="phase-lane">
          <ul class="phase-lane-legend" aria-hidden="true">
            <li><i style="background: var(--phase-idle)"></i>idle</li>
            <li><i style="background: var(--phase-listening)"></i>listening</li>
            <li><i style="background: var(--phase-thinking)"></i>thinking</li>
            <li><i style="background: var(--phase-speaking)"></i>speaking</li>
          </ul>
          <div class="phase-lane-wrap">
            <canvas data-role="phase-lane" aria-label="Runtime phase timeline, last 60 seconds"></canvas>
            <div data-role="phase-lane-tip" class="phase-lane-tip" hidden></div>
          </div>
        </div>
```

Add to `UiRefs`:

```ts
  phaseLaneCanvas: HTMLCanvasElement;
  phaseLaneTip: HTMLElement;
```

and to `collectRefs`:

```ts
    phaseLaneCanvas: query<HTMLCanvasElement>('[data-role="phase-lane"]'),
    phaseLaneTip: query<HTMLElement>('[data-role="phase-lane-tip"]'),
```

After the `render();` call at module top level (line ~169), add:

```ts
attachPhaseLane(refs.phaseLaneCanvas, refs.phaseLaneTip, () => state);
```

with the import `import { attachPhaseLane } from "./phase-lane";`.

- [ ] **Step 4: Style the lane**

Append to `styles.css`:

```css
.phase-lane {
  display: grid;
  gap: 0.45rem;
  padding: 0.85rem 1rem 0;
}

.phase-lane-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 0.8rem;
  margin: 0;
  padding: 0;
  list-style: none;
  color: var(--muted);
  font-size: 0.68rem;
  letter-spacing: 0.12em;
  text-transform: uppercase;
}

.phase-lane-legend li {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
}

.phase-lane-legend i {
  width: 0.65rem;
  height: 0.65rem;
  border-radius: 3px;
}

.phase-lane-wrap {
  position: relative;
}

.phase-lane-wrap canvas {
  display: block;
  width: 100%;
  height: 120px;
  border-radius: 10px;
}

.phase-lane-tip {
  position: absolute;
  padding: 0.28rem 0.55rem;
  border: 1px solid rgba(255, 255, 255, 0.14);
  border-radius: 8px;
  background: rgba(7, 10, 20, 0.95);
  font-family: Consolas, "Cascadia Mono", ui-monospace, monospace;
  font-size: 0.72rem;
  white-space: nowrap;
  pointer-events: none;
  transform: translate(-50%, -130%);
  z-index: 2;
}
```

- [ ] **Step 5: Verify**

Run: `npm run test` — Expected: PASS.
Run: `npm run check` — Expected: clean.
Run: `npm run fmt`.
Run: `npm run build` — Expected: clean production build.

- [ ] **Step 6: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/phase-lane.ts crates/qsf_realtime_server/ui/src/main.ts crates/qsf_realtime_server/ui/src/styles.css
git commit -m "realtime ui: runtime-phase swimlane strip in the diagnostics card"
```

### Task 3.3: Decision log entry and final gates

**Files:**
- Modify: `docs/DecisionLog.md`

- [ ] **Step 1: Add the decision entry**

Read the "How to use" section at the top of `docs/DecisionLog.md` and match the existing entry format exactly. Content to record (adapt wording to the file's format — name the behavior, never this plan's phase numbers):

```text
Decision:
  The realtime browser Diagnostics card shows transition history — a collapsed
  recent-event ticker and a 60 s runtime-phase swimlane — instead of a single
  overwritten last-event field.

Context:
  Relay events (especially partial_transcript bursts) overwrote the single field
  faster than a human could read; transitions were invisible. History lives in
  reducer state (eventLog, phaseTimeline) with wall-clock timestamps carried on
  actions so reducers stay pure; all geometry/formatting is in pure selectors;
  the canvas strip is a dumb consumer redrawn on a clock interval.

Consequences:
  Diagnostics history survives Stop for post-hoc review and clears when a new
  session is allocated. Any future action that should appear in the ticker must
  carry an atMs timestamp stamped at its dispatch site.
```

- [ ] **Step 2: Final repo gates**

From the repo root:

Run: `cargo build` — Expected: clean (no Rust touched; this is the repo's documented build gate).
Run: `cargo clippy --all-targets -- -D warnings` — Expected: clean.
Run: `cargo fmt`
From `crates/qsf_realtime_server/ui/`: `npm run check`, `npm run fmt`, `npm run test` — Expected: all clean/pass.

- [ ] **Step 3: Commit**

```bash
git add docs/DecisionLog.md
git commit -m "docs: record diagnostics transition-history decision"
```

- [ ] **Step 4: Human verification (external testing recommended)**

This is the phase where human testing is recommended — the visual behavior cannot be asserted by unit tests:

1. Run `./qsf.ps1 realtime` and open the web page.
2. Start a conversation and speak a few turns. Verify: the ticker shows newest-first rows, `partial_transcript` collapses into one `×N` row, gaps (`+X.Ys`) look plausible.
3. Watch the swimlane: bands change color at phase transitions (idle → listening → thinking → speaking → idle), band widths match perceived durations, tick marks appear under the band, and the lane scrolls left as time passes.
4. Hover a tick: tooltip shows the event kind and timestamp.
5. Press Stop: ticker shows `stopping` / `stopped`, lane returns to idle, and the history stays visible.
6. Start a new conversation: ticker and lane reset.
7. Resize the window: the lane redraws crisply (no blur/stretch).

## Success Criteria

- The Diagnostics card shows a readable event history where a `partial_transcript` burst is one row, not a blur.
- Phase transitions and their durations are visible as colored bands over the last 60 s.
- `reduceConversationState` remains pure (no clock reads); all new display logic is in tested pure selectors.
- All gates pass: `npm run test`, `npm run check`, `npm run fmt`, `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`.
- Lane ticks and phase bands agree: both derive from the reducer-made transition (`EventLogEntry.phase` / `phaseTimeline`), so a cancelled `response_completed` shows as `idle`, never `speaking`.
