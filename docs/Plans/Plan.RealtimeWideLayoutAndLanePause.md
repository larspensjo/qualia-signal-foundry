# Realtime Wide Layout And Lane Pause Implementation Plan

> **For agentic workers (advisory):** If available, superpowers:subagent-driven-development or superpowers:executing-plans is the recommended way to execute this plan task-by-task. Workers without those skills implement the tasks in order, following the repo workflow gates in `Agents.md`. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reshape the realtime browser page for a 4K screen — slim toolbar, three full-height panels, a full-width phase-lane strip — and make the phase lane pause during user inactivity by switching its x-axis from wall-clock time to activity time with compressed idle gaps.

**Architecture:** The realtime UI is a pure reducer (`reduceConversationState`) + pure selectors + a dumb `render()` in vanilla TypeScript; the phase lane is a canvas strip that consumes `selectPhaseLaneModel` on a 100 ms clock. This slice has two independent halves. (1) **Layout**: markup/CSS only — the hero card and controls row merge into one toolbar, the diagnostics sidebar splits into an Events panel and a Volition panel, and the phase lane moves to a full-width bottom strip; no reducer/selector logic changes and all `data-role` hooks keep their names. (2) **Lane pause**: `selectPhaseLaneModel` gains a piecewise wall-time → lane-time mapping in which idle stretches longer than a 2 s cap contribute their first 2 s at true scale plus a fixed-width labeled *break band*; the live trailing idle instead freezes at the cap, so the lane visibly pauses ("paused" replaces the "now" gridline label) and resumes without flushing history. Reducer-side pruning switches to the same lane-time metric so state retention matches what the lane can show. All geometry stays in the selector; the canvas renderer only gains "draw the break bands" and stays decision-free.

**Tech Stack:** TypeScript under `crates/qsf_realtime_server/ui/` (Vite, Vitest, Biome). No Rust changes.

**Spec:** Design approved conversationally 2026-07-06 (wide-layout mockups reviewed as Artifact; "Option B" chosen; idle-gap compression chosen over display-only freeze; cap lowered from 5 s to 2 s by user). No `Design.*.md` per user request — this plan is the authoritative description.

**Design decisions resolved during brainstorming (surfaced per repo rule):**
- Pause semantics: **gap compression** (activity-time axis), not a display-only freeze — history must survive a long wait.
- `PHASE_LANE_IDLE_CAP_MS = 2_000` (user-chosen): idle time shown at true scale before compression kicks in; also the live-pause threshold.
- Break band is a **fixed lane-time width** (`PHASE_LANE_BREAK_LANE_MS = 1_500`, 2.5 % of the window) so any gap costs the same bounded lane space; it is labeled with the **full wall-clock gap duration** (e.g. `⫽ 41s`), because "how long was the wait" is the question the band answers.
- The live trailing idle freezes at the cap with **no break band** until the gap closes (the band's width would otherwise pop in while watching); the band appears when the next activity arrives.
- Pause indicator: the selector swaps the `now` gridline label to `paused` — no new renderer state.
- Events that arrive during a frozen trailing idle without changing the phase (e.g. `connection_error`) tick at the lane's right edge; the lane stays paused because pause reflects *phase* inactivity.
- Gridline offsets (`-15s` …) now mean **activity time**; wall-clock durations appear on break-band labels, and wall-clock timestamps remain in the ticker and tick tooltips.

**Documents to update (per `docs/ProjectFrame/ProjectWorkflow.md`):** routine UI engineering with one durable semantic decision → **no Experiment doc**; one `docs/DecisionLog.md` entry for the activity-time axis (Task 3.1). `docs/Handoff.md` is untouched — its recommendations (volition experimentation) are unchanged by this user-driven UI slice. No architecture doc describes the browser card. No trace contract — this slice makes no trace-based behavioral claims. This plan is ephemeral; durable docs and code must not cite its phase numbers.

## Global Constraints

- All code changes live under `crates/qsf_realtime_server/ui/src/`. Run all npm commands from `crates/qsf_realtime_server/ui/`. When launching npm through `Start-Process`, use `npm.cmd` explicitly.
- After each task: `npm run test` must pass. On task completion also run `npm run check` (tsc + Biome) and `npm run fmt`.
- At plan completion run `cargo build`, then `cargo clippy --all-targets -- -D warnings`, then `cargo fmt` from the repo root (no Rust is touched; this is the standing repo gate).
- `npm run test` is Vitest only — it does not type-check. Wherever a checkpoint expects a TypeScript error, the command that surfaces it is `npm run check`.
- Reducers and selectors stay pure: no `Date.now()` inside `realtime.ts`; time enters selectors as the `nowMs` argument and reducers via `atMs` action fields.
- Keep view logic in pure selectors: the canvas code multiplies fractions by pixel width and picks colors — it makes no geometry or formatting decisions. The one renderer-side judgment allowed is pixel-space legibility (skip a break-band label that cannot fit).
- UI testing policy: test reducers and selectors; no canvas-render or DOM-structure tests. Layout tasks therefore carry no unit tests — their gate is `npm run check` plus human verification.
- All `data-role` attributes keep their existing names; `collectRefs` must not change.
- TDD for all logic tasks: write the failing test, watch it fail, implement, watch it pass, commit. One commit per task.
- Float-equality note for selector tests: write expected fractions as the *same arithmetic expression* the selector computes (e.g. `1 - 7_750 / 60_000`), which keeps `toEqual` bit-exact. If an assertion still flakes on float identity, switch that assertion to `toBeCloseTo(value, 12)` — do not loosen the design.

## File Structure

**Modified:**
- `crates/qsf_realtime_server/ui/src/main.ts` — `root.innerHTML` template only (toolbar, three panels, bottom strip, relocated audio element). No logic changes.
- `crates/qsf_realtime_server/ui/src/styles.css` — hero/controls styles removed; toolbar, chips, three-column grid, events/volition panels, phase strip, break-band legend added; narrow fallback updated.
- `crates/qsf_realtime_server/ui/src/realtime.ts` — constants `PHASE_LANE_IDLE_CAP_MS` / `PHASE_LANE_BREAK_LANE_MS`; private helpers `laneSpansOf` / `laneDurationOf` / `formatGapDuration`; `PhaseLaneBreakModel`; `selectPhaseLaneModel` rewritten onto the lane-time mapping; `prunePhaseTimeline` switched to lane-time.
- `crates/qsf_realtime_server/ui/src/realtime.test.ts` — new selector describe block; two reducer-path pruning tests in the existing phase-timeline block.
- `crates/qsf_realtime_server/ui/src/phase-lane.ts` — model literal gains `breaks: []` (Task 2.1); break-band drawing (Task 2.2).
- `docs/DecisionLog.md` — one entry (Task 3.1).

**Created:** none.

---

## Phase 1 — Wide-screen layout (Option B)

Markup and CSS only; no reducer, selector, or ref changes. Verifiable by `npm run check` and the browser. **External human testing is recommended at the end of this phase** (Task 1.2 Step 4) — layout cannot be asserted by unit tests.

### Task 1.1: Slim toolbar replaces hero and controls row

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/main.ts` (template only)
- Modify: `crates/qsf_realtime_server/ui/src/styles.css`

**Interfaces:**
- Consumes: existing `data-role` hooks `connection`, `phase`, `session`, `start`, `stop`, `mute`, `text-form`, `text-input`, `send-text`, `error`, `warning`.
- Produces: a single `<header class="toolbar">` carrying all of them; every `data-role` name unchanged, so `collectRefs` and `render()` need no edits.

- [ ] **Step 1: Replace the hero and controls sections in the template**

In `main.ts` `root.innerHTML`, replace the entire `<section class="hero">…</section>` and `<section class="controls">…</section>` blocks (everything from `<section class="hero">` through the controls section's closing `</section>`) with:

```html
    <header class="toolbar">
      <p class="brand">QSF realtime voice</p>
      <dl class="status-chips">
        <div>
          <dt>Connection</dt>
          <dd data-role="connection">Idle</dd>
        </div>
        <div>
          <dt>Phase</dt>
          <dd data-role="phase">Idle</dd>
        </div>
        <div>
          <dt>Session</dt>
          <dd data-role="session">—</dd>
        </div>
      </dl>
      <button data-role="start" type="button">Start conversation</button>
      <button data-role="stop" type="button" disabled>Stop</button>
      <button data-role="mute" type="button" aria-pressed="false" title="Stop sending your microphone; the assistant stays live">Mute</button>
      <form data-role="text-form" class="text-turn-form">
        <textarea data-role="text-input" rows="1" placeholder="Type a turn for noisy rooms"></textarea>
        <button data-role="send-text" type="submit">Send text</button>
      </form>
      <p data-role="error" class="error" hidden></p>
      <p data-role="warning" class="warning" role="status" hidden></p>
    </header>
```

- [ ] **Step 2: Replace the hero/controls CSS with toolbar styles**

In `styles.css`:

(a) Delete these rule blocks entirely: `.hero`, `.hero::after`, `.hero > *`, `.eyebrow`, `.hero h1`, `.lede`, `.hero-metrics`, `.metric`, `.metric span`, `.metric strong`, `.controls`.

(b) Rename the two banner selectors `.controls .error` / `.controls .warning` to `.toolbar .error` / `.toolbar .warning` (bodies unchanged — `flex-basis: 100%` makes them wrap to a full-width row under the toolbar when visible).

(c) In `.shell`, change `grid-template-rows: auto auto minmax(0, 1fr);` to `grid-template-rows: auto minmax(0, 1fr);` (hero and controls rows merged; the strip row is added in Task 1.2).

(d) Where the deleted `.hero` block was, add:

```css
.toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 0.65rem;
  padding: 0.6rem 0.9rem;
  border: 1px solid var(--panel-border);
  border-radius: var(--radius);
  background: var(--panel);
  backdrop-filter: blur(22px);
  box-shadow: var(--shadow);
}

.brand {
  margin: 0;
  color: var(--accent-2);
  text-transform: uppercase;
  letter-spacing: 0.18em;
  font-size: 0.72rem;
  font-weight: 700;
  white-space: nowrap;
}

.status-chips {
  display: flex;
  gap: 0.5rem;
  margin: 0;
}

.status-chips div {
  display: flex;
  align-items: baseline;
  gap: 0.45rem;
  padding: 0.35rem 0.7rem;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 999px;
  background: rgba(7, 10, 20, 0.46);
}

.status-chips dt {
  color: var(--muted);
  font-size: 0.66rem;
  text-transform: uppercase;
  letter-spacing: 0.12em;
}

/* Session ids can be long; the chip truncates rather than blowing up the row. */
.status-chips dd {
  margin: 0;
  max-width: 18ch;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.85rem;
  font-weight: 600;
}
```

(e) In `.text-turn-form`, change `flex: 1 1 32rem;` to `flex: 1 1 24rem;` and `grid-template-columns: minmax(18rem, 1fr) auto;` to `grid-template-columns: minmax(14rem, 1fr) auto;`. In `.text-turn-form textarea`, change `min-height: 2.85rem;` to `min-height: 2.4rem;`.

(f) In the `@media (max-width: 900px)` block: delete the two `.hero { … }` rules (grid-template-columns / align-items, and the padding one). Delete the `@media (max-width: 640px)` block (`.hero-metrics` no longer exists).

- [ ] **Step 3: Verify**

Run: `npm run test` — Expected: PASS (no logic touched).
Run: `npm run check` — Expected: clean (proves no dangling class/selector typos in TS; CSS is not type-checked, so eyeball Step 2 once).
Run: `npm run fmt`.

- [ ] **Step 4: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/main.ts crates/qsf_realtime_server/ui/src/styles.css
git commit -m "realtime ui: slim toolbar replaces hero and controls row"
```

### Task 1.2: Full-width shell, three columns, phase strip at the bottom

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/main.ts` (template only)
- Modify: `crates/qsf_realtime_server/ui/src/styles.css`

**Interfaces:**
- Consumes: Task 1.1's toolbar markup; existing `data-role` hooks `live-transcript`, `transcript`, `event-ticker`, `remote-audio`, `volition-state-body`, `turn-context-body`, `phase-lane`, `phase-lane-tip`.
- Produces: `.grid` with three panels (`.transcript-panel`, `.events-panel`, `.volition-panel`) and a sibling `.phase-strip` panel; all `data-role` names unchanged.

- [ ] **Step 1: Replace the grid section in the template**

In `main.ts` `root.innerHTML`, replace everything from `<section class="grid">` through the final `</section>` before the closing `</main>` (i.e. the whole transcript + diagnostics block, including the phase-lane markup and audio element inside the old aside) with:

```html
    <section class="grid">
      <article class="panel transcript-panel">
        <div class="panel-header">
          <h2>Transcript</h2>
          <span class="status-pill">Live</span>
        </div>
        <p data-role="live-transcript" class="live-transcript" aria-live="polite">Waiting for the first turn.</p>
        <ol data-role="transcript" class="transcript" aria-label="Conversation transcript"></ol>
      </article>

      <article class="panel events-panel">
        <div class="panel-header">
          <h2>Events</h2>
          <span class="status-pill muted">Browser view</span>
        </div>
        <ol data-role="event-ticker" class="event-ticker" aria-label="Recent relay events"></ol>
        <dl class="details channel-facts">
          <div>
            <dt>Media</dt>
            <dd>Direct browser to OpenAI</dd>
          </div>
          <div>
            <dt>Relay</dt>
            <dd>Typed browser-to-server envelopes</dd>
          </div>
        </dl>
      </article>

      <aside class="panel volition-panel">
        <div class="panel-header">
          <h2>Volition</h2>
        </div>
        <div class="volition-scroll">
          <details class="turn-context-details" open>
            <summary>What volition did this turn</summary>
            <div data-role="volition-state-body" class="volition-state-body"></div>
          </details>
          <details class="turn-context-details">
            <summary>Last turn context</summary>
            <div data-role="turn-context-body" class="turn-context-body"></div>
          </details>
        </div>
      </aside>
    </section>

    <section class="panel phase-strip">
      <div class="phase-strip-header">
        <h2>Phase timeline</h2>
        <ul class="phase-lane-legend" aria-hidden="true">
          <li><i style="background: var(--phase-idle)"></i>idle</li>
          <li><i style="background: var(--phase-listening)"></i>listening</li>
          <li><i style="background: var(--phase-thinking)"></i>thinking</li>
          <li><i style="background: var(--phase-speaking)"></i>speaking</li>
        </ul>
      </div>
      <div class="phase-lane-wrap">
        <canvas data-role="phase-lane" aria-label="Runtime phase timeline, last 60 seconds of activity"></canvas>
        <div data-role="phase-lane-tip" class="phase-lane-tip" hidden></div>
      </div>
    </section>

    <audio data-role="remote-audio" autoplay playsinline></audio>
```

(The audio element is invisible — it has no `controls` attribute — so it can live at the shell's end.)

- [ ] **Step 2: Update the CSS**

In `styles.css`:

(a) `.shell` becomes full-width with a strip row:

```css
.shell {
  width: calc(100vw - 1.5rem);
  height: 100vh;
  margin: 0 auto;
  padding: 0.75rem 0;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
  gap: 0.85rem;
  overflow: hidden;
}
```

(b) `.grid` gets three columns:

```css
.grid {
  display: grid;
  grid-template-columns: minmax(0, 1.5fr) minmax(0, 1fr) minmax(0, 1.1fr);
  gap: 1rem;
  min-height: 0;
  overflow: hidden;
}
```

(c) Delete these rule blocks: `.details-panel`, `.details-panel audio`, `.phase-lane` (only the container block — keep `.phase-lane-legend`, `.phase-lane-legend li`, `.phase-lane-legend i`, `.phase-lane-wrap`, `.phase-lane-wrap canvas`, `.phase-lane-tip`).

(d) Add panel-internal layout rules **after the `.details dd` rule** (placement matters: `.channel-facts` must come later in the file than `.details`, whose `padding: 0.85rem 1rem 0` it partially overrides at equal specificity):

```css
.events-panel {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr) auto;
}

.events-panel .event-ticker {
  min-height: 0;
  overflow-y: auto;
  padding: 0.85rem 1rem;
}

.channel-facts {
  padding-bottom: 0.85rem;
}

.volition-panel {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
}

.volition-scroll {
  min-height: 0;
  overflow-y: auto;
  padding-bottom: 1rem;
}

.phase-strip {
  display: grid;
  gap: 0.5rem;
  padding: 0.65rem 1rem 0.8rem;
}

.phase-strip-header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 1rem;
}

.phase-strip-header h2 {
  margin: 0;
  color: var(--muted);
  font-size: 0.8rem;
  letter-spacing: 0.1em;
  text-transform: uppercase;
}
```

(e) Update the `@media (max-width: 900px)` block to match the new structure (the fallback still stacks; it is functional, not optimized, per the approved 4K-only scope). The block becomes:

```css
@media (max-width: 900px) {
  body {
    overflow: auto;
  }

  .shell {
    width: calc(100vw - 1rem);
    height: auto;
    min-height: 100vh;
    padding-top: 0.5rem;
    overflow: visible;
  }

  .grid {
    grid-template-columns: 1fr;
    overflow: visible;
  }

  .text-turn-form {
    grid-template-columns: 1fr;
    flex-basis: 100%;
  }

  .transcript {
    max-height: min(46vh, 28rem);
  }

  .volition-scroll,
  .events-panel .event-ticker {
    overflow: visible;
  }
}
```

Keep the existing nested `@supports (min-height: 100dvh)` companion block as-is.

- [ ] **Step 3: Verify the gates**

Run: `npm run test` — Expected: PASS.
Run: `npm run check` — Expected: clean.
Run: `npm run fmt`.
Run: `npm run build` — Expected: clean production build.

- [ ] **Step 4: Human verification (external testing recommended)**

1. Run `./qsf.ps1 realtime`, open the page full-screen on the 4K monitor.
2. Toolbar: one row — brand, three chips, Start/Stop/Mute, text form; no hero card; no dead margin at the sides.
3. Three columns fill the viewport height: Transcript | Events (ticker scrolls internally, Media/Relay pinned at bottom) | Volition (verdict open, scroll internal).
4. Phase strip spans the full window width at the bottom; legend in its header row.
5. Trigger an error (e.g. stop the server mid-session): the banner appears as a full-width row under the toolbar without breaking the layout.
6. Shrink the window below 900 px wide: panels stack vertically and the page scrolls; nothing is clipped.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/main.ts crates/qsf_realtime_server/ui/src/styles.css
git commit -m "realtime ui: wide three-column layout with full-width phase strip"
```

---

## Phase 2 — Lane pause via idle-gap compression

All geometry in pure selectors, TDD. Interim note: after Task 2.1 and until Task 2.2, compressed gaps render as a blank slot in the band (the selector no longer emits the excess as a segment and the renderer cannot draw breaks yet) — visible but harmless for one commit.

### Task 2.1: Activity-time lane model with break bands, pause, and lane-time pruning

Selector geometry and reducer pruning are one semantic contract — both must switch to lane time together. Landing them in one commit avoids an intermediate state where the selector supports resume-without-flushing but reducer-owned pruning has already discarded the history the model needs.

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts`
- Modify: `crates/qsf_realtime_server/ui/src/phase-lane.ts` (one line: model literal)
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces:**
- Consumes: `ConversationState.phaseTimeline`, `ConversationState.eventLog`, `PHASE_LANE_WINDOW_MS`, `PHASE_LANE_GRIDLINE_STEP_MS`, `formatClockTime`.
- Produces (later tasks rely on these exact names):
  - `export const PHASE_LANE_IDLE_CAP_MS = 2_000;`
  - `export const PHASE_LANE_BREAK_LANE_MS = 1_500;`
  - `export interface PhaseLaneBreakModel { startFraction: number; endFraction: number; label: string; }`
  - `PhaseLaneModel` gains `breaks: PhaseLaneBreakModel[];`
  - `selectPhaseLaneModel(state, nowMs)` — same signature; segments/ticks/gridlines now positioned in activity time; the `now` gridline label becomes `"paused"` while the live trailing idle exceeds the cap.
  - Module-private `laneDurationOf(timeline, index, nowMs): number` — shared by the selector and pruning.
  - `prunePhaseTimeline` measures the window in lane time; no exported surface changes.

- [ ] **Step 1: Write the failing tests**

Append to `realtime.test.ts`, in two places.

First, a new describe block. Do **not** import `PHASE_LANE_IDLE_CAP_MS` or `PHASE_LANE_BREAK_LANE_MS`: the tests hardcode cap 2 000 / break 1 500 in the lane-distance arithmetic (which keeps the expected fractions readable), so importing the constants would leave them unused and Biome's unused-import rule would fail `npm run check`.

```ts
describe("selectPhaseLaneModel idle-gap compression", () => {
  // Lane distances are written as explicit arithmetic mirroring the selector's
  // formula (1 - laneMsFromNow / PHASE_LANE_WINDOW_MS) so toEqual stays bit-exact.

  it("keeps an idle gap at or under the cap at true scale with no break", () => {
    const state: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [
        { phase: "speaking", startedAtMs: 0 },
        { phase: "idle", startedAtMs: 10_000 },
        { phase: "listening", startedAtMs: 11_500 },
      ],
    };
    const model = selectPhaseLaneModel(state, 20_000);
    expect(model.breaks).toEqual([]);
    expect(model.segments).toEqual([
      { phase: "idle", startFraction: 0, endFraction: 1 - 20_000 / 60_000 },
      { phase: "speaking", startFraction: 1 - 20_000 / 60_000, endFraction: 1 - 10_000 / 60_000 },
      { phase: "idle", startFraction: 1 - 10_000 / 60_000, endFraction: 1 - 8_500 / 60_000 },
      { phase: "listening", startFraction: 1 - 8_500 / 60_000, endFraction: 1 },
    ]);
  });

  it("compresses a closed idle gap into a 2s head plus a labeled break band", () => {
    // Gap: idle 12_000..53_000 (41 s). Head 12_000..14_000 (2_000 lane-ms);
    // break band 14_000..53_000 squashed to 1_500 lane-ms.
    // Lane distances from now=60_000: listening 7_000; break 8_500; head 10_500;
    // speaking 22_500.
    const state: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [
        { phase: "speaking", startedAtMs: 0 },
        { phase: "idle", startedAtMs: 12_000 },
        { phase: "listening", startedAtMs: 53_000 },
      ],
    };
    const model = selectPhaseLaneModel(state, 60_000);
    expect(model.segments).toEqual([
      { phase: "idle", startFraction: 0, endFraction: 1 - 22_500 / 60_000 },
      { phase: "speaking", startFraction: 1 - 22_500 / 60_000, endFraction: 1 - 10_500 / 60_000 },
      { phase: "idle", startFraction: 1 - 10_500 / 60_000, endFraction: 1 - 8_500 / 60_000 },
      { phase: "listening", startFraction: 1 - 7_000 / 60_000, endFraction: 1 },
    ]);
    expect(model.breaks).toEqual([
      {
        startFraction: 1 - 8_500 / 60_000,
        endFraction: 1 - 7_000 / 60_000,
        label: "⫽ 41s",
      },
    ]);
  });

  it("labels a multi-minute gap in minutes and seconds", () => {
    const state: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [
        { phase: "listening", startedAtMs: 0 },
        { phase: "idle", startedAtMs: 5_000 },
        { phase: "listening", startedAtMs: 166_000 },
      ],
    };
    const model = selectPhaseLaneModel(state, 170_000);
    expect(model.breaks).toHaveLength(1);
    expect(model.breaks[0].label).toBe("⫽ 2m 41s");
  });

  it("positions a tick inside a compressed gap proportionally within its break band", () => {
    // Same geometry as the compression test. Tick at 33_500 = halfway through the
    // squashed 39_000 ms excess -> lane offset 750 into the 1_500 lane-ms band ->
    // lane distance from now = 8_500 - 750 = 7_750.
    const state: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [
        { phase: "speaking", startedAtMs: 0 },
        { phase: "idle", startedAtMs: 12_000 },
        { phase: "listening", startedAtMs: 53_000 },
      ],
      eventLog: [
        { kind: "connection_error", phase: "idle", firstAtMs: 33_500, lastAtMs: 33_500, count: 1 },
      ],
    };
    const ticks = selectPhaseLaneModel(state, 60_000).ticks;
    expect(ticks.map((tick) => [tick.kind, tick.fraction])).toEqual([
      ["connection_error", 1 - 7_750 / 60_000],
    ]);
  });

  it("freezes the lane while the live trailing idle exceeds the cap", () => {
    const state: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [
        { phase: "speaking", startedAtMs: 0 },
        { phase: "idle", startedAtMs: 10_000 },
      ],
    };
    const at30 = selectPhaseLaneModel(state, 30_000);
    // Trailing idle contributes only the cap: speaking spans lane 12_000..2_000
    // from now, trailing idle the last 2_000, and no break band while live.
    expect(at30.segments).toEqual([
      { phase: "idle", startFraction: 0, endFraction: 1 - 12_000 / 60_000 },
      { phase: "speaking", startFraction: 1 - 12_000 / 60_000, endFraction: 1 - 2_000 / 60_000 },
      { phase: "idle", startFraction: 1 - 2_000 / 60_000, endFraction: 1 },
    ]);
    expect(at30.breaks).toEqual([]);
    expect(at30.gridlines[0]).toEqual({ fraction: 1, label: "paused" });
    // A minute later, still waiting: identical geometry — the lane is paused.
    const at90 = selectPhaseLaneModel(state, 90_000);
    expect(at90.segments).toEqual(at30.segments);
    expect(at90.gridlines).toEqual(at30.gridlines);
  });

  it("ticks at the lane's right edge for events during a frozen trailing idle", () => {
    // Pause reflects *phase* inactivity: an event inside the frozen tail does
    // not unpause; its wall time clamps to the capped span, so it lands at 1.
    const state: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [
        { phase: "speaking", startedAtMs: 0 },
        { phase: "idle", startedAtMs: 10_000 },
      ],
      eventLog: [
        { kind: "connection_error", phase: "idle", firstAtMs: 25_000, lastAtMs: 25_000, count: 1 },
      ],
    };
    const model = selectPhaseLaneModel(state, 30_000);
    expect(model.gridlines[0].label).toBe("paused");
    expect(model.ticks.map((tick) => [tick.kind, tick.fraction])).toEqual([
      ["connection_error", 1],
    ]);
  });

  it("does not pause during short waits, active phases, or before any session", () => {
    const shortWait: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [{ phase: "idle", startedAtMs: 10_000 }],
    };
    expect(selectPhaseLaneModel(shortWait, 11_500).gridlines[0].label).toBe("now");
    const active: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [{ phase: "listening", startedAtMs: 0 }],
    };
    expect(selectPhaseLaneModel(active, 90_000).gridlines[0].label).toBe("now");
    expect(selectPhaseLaneModel(INITIAL_STATE, 90_000).gridlines[0].label).toBe("now");
  });

  it("resumes without flushing history when activity closes a long gap", () => {
    // After a 80_000 ms wait, listening resumes at 90_000. The gap closes into
    // head + break (3_500 lane-ms) and speaking remains well inside the window.
    const state: ConversationState = {
      ...INITIAL_STATE,
      phaseTimeline: [
        { phase: "speaking", startedAtMs: 0 },
        { phase: "idle", startedAtMs: 10_000 },
        { phase: "listening", startedAtMs: 90_000 },
      ],
    };
    const model = selectPhaseLaneModel(state, 95_000);
    // Lane distances from now: listening 5_000; break 6_500; head 8_500; speaking 18_500.
    expect(model.segments).toContainEqual({
      phase: "speaking",
      startFraction: 1 - 18_500 / 60_000,
      endFraction: 1 - 8_500 / 60_000,
    });
    expect(model.breaks).toEqual([
      { startFraction: 1 - 6_500 / 60_000, endFraction: 1 - 5_000 / 60_000, label: "⫽ 1m 20s" },
    ]);
  });
});
```

Second, append inside the existing `describe("diagnostics phase timeline", …)` block (it uses the file-scope `withEnvelope` helper), covering the reducer path so resume-without-flushing is pinned on real app state, not just hand-built timelines:

```ts
  it("keeps wall-clock-old history across a compressed idle gap", () => {
    let state = withEnvelope(INITIAL_STATE, "speech_playback_started", 0); // -> speaking
    state = withEnvelope(state, "speech_playback_completed", 10_000); // -> idle
    // Five minutes of silence, then the user speaks. The gap costs only
    // cap + break lane-ms, so the speaking segment must survive.
    state = withEnvelope(state, "user_turn_started", 310_000); // -> listening
    expect(state.phaseTimeline).toEqual([
      { phase: "speaking", startedAtMs: 0 },
      { phase: "idle", startedAtMs: 10_000 },
      { phase: "listening", startedAtMs: 310_000 },
    ]);
  });

  it("drops pre-gap history once post-gap activity exceeds the lane window", () => {
    // Regression guard: compressed gaps must not make retention unbounded.
    // After the gap, listening runs a full lane window (60_000 ms), so the
    // pre-gap speaking and idle segments fall off the lane-time cutoff.
    let state = withEnvelope(INITIAL_STATE, "speech_playback_started", 0); // -> speaking
    state = withEnvelope(state, "speech_playback_completed", 10_000); // -> idle
    state = withEnvelope(state, "user_turn_started", 310_000); // -> listening
    state = withEnvelope(state, "final_transcript", 370_000); // -> thinking
    expect(state.phaseTimeline).toEqual([
      { phase: "listening", startedAtMs: 310_000 },
      { phase: "thinking", startedAtMs: 370_000 },
    ]);
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `npm run check`
Expected: FAIL — `breaks` does not exist on `PhaseLaneModel`.
Run: `npm run test`
Expected: FAIL — the selector assertions cannot pass (`model.breaks` is `undefined`; segments still wall-clock; the live-pause tick lands at `1 - 5_000 / 60_000`, not `1`), and "keeps wall-clock-old history across a compressed idle gap" fails because wall-clock pruning drops the `speaking` segment (cutoff 250_000). Exception: "drops pre-gap history once post-gap activity exceeds the lane window" passes under both metrics — in this scenario lane time and wall time agree on the outcome. It is a regression guard pinning that lane-time pruning still discards old history, not a TDD driver; do not expect it to fail here.

- [ ] **Step 3: Implement**

In `realtime.ts`:

(a) Below `PHASE_LANE_WINDOW_MS`, add:

```ts
/// Idle time shown at true scale before a gap is compressed; also how long the
/// live trailing idle runs before the lane pauses.
export const PHASE_LANE_IDLE_CAP_MS = 2_000;

/// Fixed lane-time width of a compressed gap's break band.
export const PHASE_LANE_BREAK_LANE_MS = 1_500;
```

(b) Next to `PhaseLaneGridlineModel`, add the break model and extend `PhaseLaneModel`:

```ts
export interface PhaseLaneBreakModel {
  startFraction: number;
  endFraction: number;
  /// Wall-clock duration of the whole compressed idle gap, e.g. "⫽ 41s".
  label: string;
}
```

```ts
export interface PhaseLaneModel {
  segments: PhaseLaneSegmentModel[];
  ticks: PhaseLaneTickModel[];
  gridlines: PhaseLaneGridlineModel[];
  breaks: PhaseLaneBreakModel[];
}
```

(c) Above `selectPhaseLaneModel`, add the lane-time machinery:

```ts
/// One wall-time interval annotated with the lane-time width it occupies.
interface LaneSpan {
  phase: RuntimePhase;
  wallStartMs: number;
  wallEndMs: number;
  laneMs: number;
  /// Non-null when this span is a compressed idle gap's break band.
  breakLabel: string | null;
}

/// Lane-time width of timeline segment `index`: non-idle and short-idle segments
/// map 1:1; a closed long idle gap costs cap + break band; the live trailing
/// idle freezes at the cap (the pause). Shared by the selector and pruning so
/// state retention matches what the lane can show.
function laneDurationOf(timeline: PhaseSegment[], index: number, nowMs: number): number {
  const { phase, startedAtMs } = timeline[index];
  const isTrailing = index + 1 === timeline.length;
  const endMs = isTrailing ? nowMs : timeline[index + 1].startedAtMs;
  const durationMs = endMs - startedAtMs;
  if (phase !== "idle" || durationMs <= PHASE_LANE_IDLE_CAP_MS) {
    return durationMs;
  }
  return isTrailing
    ? PHASE_LANE_IDLE_CAP_MS
    : PHASE_LANE_IDLE_CAP_MS + PHASE_LANE_BREAK_LANE_MS;
}

/// Expand the phase timeline into lane spans. A closed idle gap longer than the
/// cap splits into a true-scale head plus a break band; the live trailing idle
/// keeps a single capped span and gains its band only once the gap closes.
function laneSpansOf(timeline: PhaseSegment[], nowMs: number): LaneSpan[] {
  const spans: LaneSpan[] = [];
  for (let i = 0; i < timeline.length; i++) {
    const { phase, startedAtMs } = timeline[i];
    const isTrailing = i + 1 === timeline.length;
    const endMs = isTrailing ? nowMs : timeline[i + 1].startedAtMs;
    const durationMs = endMs - startedAtMs;
    const isCompressed = phase === "idle" && durationMs > PHASE_LANE_IDLE_CAP_MS;
    if (!isCompressed || isTrailing) {
      spans.push({
        phase,
        wallStartMs: startedAtMs,
        wallEndMs: endMs,
        laneMs: laneDurationOf(timeline, i, nowMs),
        breakLabel: null,
      });
      continue;
    }
    spans.push({
      phase,
      wallStartMs: startedAtMs,
      wallEndMs: startedAtMs + PHASE_LANE_IDLE_CAP_MS,
      laneMs: PHASE_LANE_IDLE_CAP_MS,
      breakLabel: null,
    });
    spans.push({
      phase,
      wallStartMs: startedAtMs + PHASE_LANE_IDLE_CAP_MS,
      wallEndMs: endMs,
      laneMs: PHASE_LANE_BREAK_LANE_MS,
      breakLabel: `⫽ ${formatGapDuration(durationMs)}`,
    });
  }
  return spans;
}

function formatGapDuration(ms: number): string {
  const totalSeconds = Math.round(ms / 1000);
  if (totalSeconds < 60) {
    return `${totalSeconds}s`;
  }
  return `${Math.floor(totalSeconds / 60)}m ${totalSeconds % 60}s`;
}
```

(d) Replace the body of `selectPhaseLaneModel` with:

```ts
/// Geometry for the phase swimlane, all x-positions as fractions of the lane
/// width in [0, 1] with `now` at 1. The x-axis is *activity time*: idle gaps
/// longer than PHASE_LANE_IDLE_CAP_MS are compressed into break bands, and the
/// live trailing idle freezes the lane (gridline "now" reads "paused"). The
/// canvas renderer multiplies by pixel width and picks colors; it makes no
/// layout decisions of its own.
export function selectPhaseLaneModel(state: ConversationState, nowMs: number): PhaseLaneModel {
  const clamp01 = (value: number) => Math.min(1, Math.max(0, value));
  const spans = laneSpansOf(state.phaseTimeline, nowMs);

  // Lane-time distance from now back to each span's start.
  const laneStartFromNow: number[] = new Array(spans.length);
  let cumulative = 0;
  for (let i = spans.length - 1; i >= 0; i--) {
    cumulative += spans[i].laneMs;
    laneStartFromNow[i] = cumulative;
  }

  const fractionWithin = (span: LaneSpan, laneStart: number, atMs: number): number => {
    const wallSpanMs = span.wallEndMs - span.wallStartMs;
    // Break bands squash their wall interval linearly; other spans map 1:1 with
    // the offset clamped to laneMs (the frozen tail of a live idle span).
    const offset =
      span.breakLabel !== null && wallSpanMs > 0
        ? (span.laneMs * (atMs - span.wallStartMs)) / wallSpanMs
        : Math.min(atMs - span.wallStartMs, span.laneMs);
    return 1 - (laneStart - offset) / PHASE_LANE_WINDOW_MS;
  };

  /// Lane fraction of an arbitrary wall time. Times before the first span map
  /// 1:1 through the leading implicit idle; with no spans at all the axis is
  /// plain wall clock.
  const fractionOf = (atMs: number): number => {
    if (spans.length === 0) {
      return 1 - (nowMs - atMs) / PHASE_LANE_WINDOW_MS;
    }
    if (atMs < spans[0].wallStartMs) {
      const firstStartFraction = 1 - laneStartFromNow[0] / PHASE_LANE_WINDOW_MS;
      return firstStartFraction - (spans[0].wallStartMs - atMs) / PHASE_LANE_WINDOW_MS;
    }
    let index = spans.length - 1;
    for (let i = 0; i + 1 < spans.length; i++) {
      if (atMs < spans[i + 1].wallStartMs) {
        index = i;
        break;
      }
    }
    return fractionWithin(spans[index], laneStartFromNow[index], atMs);
  };

  const segments: PhaseLaneSegmentModel[] = [];
  const breaks: PhaseLaneBreakModel[] = [];
  // Before the first recorded segment the runtime phase was idle (INITIAL_STATE.phase).
  const firstStartFraction =
    spans.length === 0 ? 1 : clamp01(1 - laneStartFromNow[0] / PHASE_LANE_WINDOW_MS);
  if (firstStartFraction > 0) {
    segments.push({ phase: "idle", startFraction: 0, endFraction: firstStartFraction });
  }
  for (let i = 0; i < spans.length; i++) {
    // Hoisting the element is load-bearing: TypeScript only narrows
    // span.breakLabel to string on a const reference, not on spans[i] with a
    // mutable index.
    const span = spans[i];
    const startFraction = clamp01(1 - laneStartFromNow[i] / PHASE_LANE_WINDOW_MS);
    const endFraction = clamp01(1 - (laneStartFromNow[i] - span.laneMs) / PHASE_LANE_WINDOW_MS);
    if (endFraction <= 0) {
      continue;
    }
    if (span.breakLabel !== null) {
      breaks.push({ startFraction, endFraction, label: span.breakLabel });
    } else {
      segments.push({ phase: span.phase, startFraction, endFraction });
    }
  }

  const ticks: PhaseLaneTickModel[] = [];
  for (const entry of state.eventLog) {
    const atMss = entry.count > 1 ? [entry.firstAtMs, entry.lastAtMs] : [entry.firstAtMs];
    for (const atMs of atMss) {
      const fraction = fractionOf(Math.min(atMs, nowMs));
      if (fraction >= 0 && fraction <= 1) {
        ticks.push({
          fraction,
          kind: entry.kind,
          phase: entry.phase,
          timeLabel: formatClockTime(atMs),
        });
      }
    }
  }
  ticks.sort((a, b) => a.fraction - b.fraction);

  const lastSegment = state.phaseTimeline.at(-1);
  const paused =
    lastSegment !== undefined &&
    lastSegment.phase === "idle" &&
    nowMs - lastSegment.startedAtMs > PHASE_LANE_IDLE_CAP_MS;

  const gridlines: PhaseLaneGridlineModel[] = [];
  for (let backMs = 0; backMs <= PHASE_LANE_WINDOW_MS; backMs += PHASE_LANE_GRIDLINE_STEP_MS) {
    gridlines.push({
      fraction: 1 - backMs / PHASE_LANE_WINDOW_MS,
      label: backMs === 0 ? (paused ? "paused" : "now") : `-${backMs / 1000}s`,
    });
  }

  return { segments, ticks, gridlines, breaks };
}
```

Note: `firstStartFraction` for an empty timeline is `1`, so the whole lane renders as the single idle segment `{0, 1}` — same as before.

(e) In `phase-lane.ts`, the model literal gains the new field:

```ts
  let model: PhaseLaneModel = { segments: [], ticks: [], gridlines: [], breaks: [] };
```

(f) Replace `prunePhaseTimeline` in `realtime.ts`:

```ts
/// Drop segments that ended a full lane window of *activity time* ago, keeping
/// the segment that spans the cutoff so the lane's left edge is still painted.
/// Compressed idle gaps cost almost no lane time, so wall-clock-old history
/// survives a long wait — that is the point of the lane pause.
function prunePhaseTimeline(timeline: PhaseSegment[], nowMs: number): PhaseSegment[] {
  let laneFromNow = 0;
  for (let i = timeline.length - 1; i > 0; i--) {
    // After adding segment i, laneFromNow is the lane distance from now back to
    // segment i's start — which is where segment i-1 ends.
    laneFromNow += laneDurationOf(timeline, i, nowMs);
    if (laneFromNow >= PHASE_LANE_WINDOW_MS) {
      return timeline.slice(i);
    }
  }
  return timeline;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm run test` — Expected: PASS, including all pre-existing `selectPhaseLaneModel` tests (with no idle gaps, or an empty timeline, activity time and wall time coincide, so their expected fractions are unchanged) and the pre-existing pruning test "prunes segments that ended before the lane window, keeping the spanning one" (its timeline has no idle gaps, so lane time equals wall time and the expected result is identical).
Run: `npm run check` then `npm run fmt`.

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/phase-lane.ts crates/qsf_realtime_server/ui/src/realtime.test.ts
git commit -m "realtime ui: phase lane pauses on idle via gap compression and lane-time pruning"
```

### Task 2.2: Render break bands

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/phase-lane.ts`
- Modify: `crates/qsf_realtime_server/ui/src/main.ts` (one legend item)
- Modify: `crates/qsf_realtime_server/ui/src/styles.css` (legend hatch swatch)

**Interfaces:**
- Consumes: `PhaseLaneModel.breaks` (Task 2.1).
- Produces: hatched break bands with duration labels on the canvas; a "skipped idle" legend entry. No new exports.

No unit tests for this task (canvas rendering is excluded by the repo's UI testing policy; all geometry it consumes is tested in Task 2.1).

- [ ] **Step 1: Draw the bands**

In `phase-lane.ts` `draw()`, insert after the segments loop and before the ticks loop:

```ts
    for (const gapBreak of model.breaks) {
      const x1 = gapBreak.startFraction * width;
      const x2 = gapBreak.endFraction * width;
      const bandWidth = Math.max(2, x2 - x1);
      context.fillStyle = "rgba(184, 191, 215, 0.1)";
      context.fillRect(x1, BAND_TOP, bandWidth, BAND_HEIGHT);
      // Diagonal hatching marks the band as compressed time.
      context.save();
      context.beginPath();
      context.rect(x1, BAND_TOP, bandWidth, BAND_HEIGHT);
      context.clip();
      context.strokeStyle = "rgba(184, 191, 215, 0.32)";
      for (let x = x1 - BAND_HEIGHT; x < x2; x += 7) {
        context.beginPath();
        context.moveTo(x, BAND_TOP + BAND_HEIGHT);
        context.lineTo(x + BAND_HEIGHT, BAND_TOP);
        context.stroke();
      }
      context.restore();
      // Pixel-space legibility only: skip the label when the band cannot fit it.
      if (bandWidth > 44) {
        context.fillStyle = "rgba(244, 241, 234, 0.9)";
        context.fillText(gapBreak.label, x1 + bandWidth / 2, BAND_TOP + BAND_HEIGHT / 2 + 3);
      }
    }
```

(`context.font` and `textAlign: "center"` are already set by the gridline block above.)

- [ ] **Step 2: Add the legend entry**

In `main.ts`, append to the `.phase-lane-legend` list:

```html
          <li><i class="legend-gap"></i>skipped idle</li>
```

In `styles.css`, next to `.phase-lane-legend i`:

```css
.phase-lane-legend .legend-gap {
  background: repeating-linear-gradient(
    45deg,
    rgba(184, 191, 215, 0.55) 0 2px,
    rgba(184, 191, 215, 0.12) 2px 4px
  );
}
```

- [ ] **Step 3: Verify the gates**

Run: `npm run test` — Expected: PASS.
Run: `npm run check` — Expected: clean.
Run: `npm run fmt`.
Run: `npm run build` — Expected: clean production build.

- [ ] **Step 4: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/phase-lane.ts crates/qsf_realtime_server/ui/src/main.ts crates/qsf_realtime_server/ui/src/styles.css
git commit -m "realtime ui: render compressed-gap break bands on the phase lane"
```

---

## Phase 3 — Decision log, final gates, human verification

### Task 3.1: Record the axis decision and close the gates

**Files:**
- Modify: `docs/DecisionLog.md`

- [ ] **Step 1: Add the decision entry**

Append to `docs/DecisionLog.md` (matching the file's entry template; adjust the date if implementation lands later):

```markdown
## 2026-07-06 - Phase lane shows activity time with compressed idle gaps
Decision: The realtime diagnostics phase lane's x-axis is activity time, not wall-clock
time. An idle stretch longer than a short cap (2 s) renders as its first 2 s at true
scale plus a fixed-width hatched break band labeled with the real gap duration; while
the live trailing idle exceeds the cap the lane freezes and reads "paused" until the
next activity. History pruning uses the same activity-time window.
Context: On a wall-clock axis, waiting for the user to respond scrolled all activity out
of the 60 s window — the lane was pure idle exactly when a finished exchange should be
reviewable. Chosen over a display-only freeze, which would still have expired history at
the moment of resume.
Consequences: Lane geometry and history retention are bounded by activity, not elapsed
time — history survives arbitrarily long waits. Gridline offsets on the lane read as
activity time; wall-clock durations appear only on break-band labels, and wall-clock
timestamps remain in the event ticker and tick tooltips.
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
git commit -m "docs: record phase-lane activity-time axis decision"
```

- [ ] **Step 4: Human verification (external testing recommended)**

Run `./qsf.ps1 realtime` on the 4K monitor and hold a short voice conversation:

1. **Layout**: toolbar, three full-height columns, full-width phase strip; no wasted side margins; chips update live (Connection/Phase/Session).
2. **Pause**: finish an assistant turn, then stay silent. Within ~2 s the lane stops scrolling and the right-edge gridline label reads `paused`. Wait 30+ s: the lane stays frozen; earlier bands do not drift left.
3. **Resume**: speak again. The lane resumes; the wait appears as a thin hatched band labeled with the real duration (e.g. `⫽ 34s`); the pre-wait turns are still on the lane.
4. **Break bands**: hover ticks near a band — tooltips still show wall-clock times that jump across the gap, while the band label reports the gap length.
5. **Long session**: after several minutes with multiple waits, the lane shows ~60 s of *activity* with each wait as a band; the ticker's wall-clock times corroborate.
6. **Narrow window**: below 900 px everything stacks and scrolls; break-band labels disappear on bands too narrow to fit them (hatching remains).

## Success Criteria

- On a 3840×2160 window the page is a dashboard: slim toolbar, three panels using the full height, and a phase strip using the full width.
- Waiting on the user costs at most 2 s + one break band of lane space; the lane visibly pauses (`paused` label) during the wait and resumes without losing history.
- A compressed gap is visually distinct (hatched) and reports its true wall-clock duration.
- `reduceConversationState` and all selectors remain pure (no clock reads); every geometry decision lives in `selectPhaseLaneModel` and is unit-tested, including compression, tick placement inside a band, pause freezing, right-edge ticks during a live pause, and lane-time pruning (history preserved across a gap, and still dropped once post-gap activity fills the window).
- All `data-role` contracts and existing tests survive unchanged except the documented pruning-semantics test additions.
- All gates pass: `npm run test`, `npm run check`, `npm run fmt`, `npm run build`, `cargo build`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt`.
