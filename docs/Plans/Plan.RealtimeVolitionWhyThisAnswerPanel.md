# Realtime Volition "What This Answer" Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reframe the realtime browser's right-hand "Volition state" panel into a per-turn "What volition did this turn" explanation so a researcher can read, in plain language, whether and how the server's volition shaped the latest assistant reply.

**Architecture:** Pure TypeScript view-model work in the realtime UI. Two new total (never-throwing) selectors derive a plain-English **verdict** and locate the **verbatim injected volition text** — both from data already on the wire (`latestVolitionState` and `latestTurnContext`), correlated by matching `exchangeIndex`. The locator returns a three-state status model (`found` / `none_injected` / `unavailable`) so neither the verdict nor the renderer can claim nothing was injected when the matching capture simply has not arrived yet, and the verdict consumes the locator result so a `decision: null` capture paired with a coherence-only injected packet reads as "context only", not "no decision". The render layer composes three tiers of progressive disclosure. Today's detailed rows are demoted into a collapsed section, unchanged. One small Rust guard test pins the prose prefix the client keys off. No reducer, state-shape, wire-format, or server-behavior change.

**Tech Stack:** TypeScript, Vitest, Biome (realtime UI under `crates/qsf_realtime_server/ui/`); Rust + cargo (guard test in `qsf_realtime_server`).

## Global Constraints

- Run all UI commands from `crates/qsf_realtime_server/ui/`. Test: `npm run test`. Lint/typecheck: `npm run check` (runs `tsc --noEmit` + `biome check .`). Format: `npm run fmt`.
- After Rust changes run `cargo clippy --all-targets -- -D warnings` then `cargo fmt` from the repo root.
- No changes to `reduceConversationState`, `ConversationState` shape, wire parsers, or any `qsf_realtime_server` runtime behavior. This is presentation only; the observation plane must stay read-only and non-blocking.
- New selectors MUST be **total**: on any missing/malformed input they return a defined "unavailable" value, never throw — a bad capture must not break the transcript render.
- The injected-text locator prefix is the exact string `Simulated volition context for this turn`. It is defined once in the UI as `VOLITION_INJECTED_TEXT_PREFIX` and pinned by a Rust test. Do not fork this literal.
- End every `git commit` message with the trailer:
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`

---

## File Structure

- `crates/qsf_realtime_server/ui/src/realtime.ts` — **Modify.** Add `VOLITION_INJECTED_TEXT_PREFIX`, the `InjectedVolitionText` and `VolitionVerdict` types, `selectVolitionVerdict`, `selectInjectedVolitionText`, and the private helpers `describeShapingIntensity` / `volitionItemText`. `selectVolitionPanelModel` is left untouched (it becomes the Tier-3 content).
- `crates/qsf_realtime_server/ui/src/realtime.test.ts` — **Modify.** Add two `describe` blocks covering the verdict selector and the injected-text locator.
- `crates/qsf_realtime_server/ui/src/main.ts` — **Modify.** Reorder the two `<details>` (volition first), rename its summary, and replace the panel render with a three-tier composition. Reuses the existing `renderVolitionStatePanel` unchanged for Tier 3.
- `crates/qsf_realtime_server/ui/src/styles.css` — **Modify.** Add classes for the verdict block, the injected-text section, and the collapsed scoring `<details>`.
- `crates/qsf_realtime_server/src/realtime/volition_injection.rs` — **Modify (tests only).** Add `packet_text_starts_with_ui_locator_prefix` guarding the prose prefix.
- `docs/DecisionLog.md` — **Modify.** Record the reframe decision.
- `docs/Architecture/Architecture.RealtimeSessionServer.md` — **Modify.** Note that the browser panel correlates the two existing captures.

---

### Task 1: `selectVolitionVerdict` — the plain-English verdict

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts` (append near the other selectors, after `selectVolitionPanelModel` and its helpers)
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces:**
- Consumes: `ConversationState` (existing), `VolitionInspectionCapture` (existing), `formatLabelValue` (existing private helper in `realtime.ts`).
- Produces:
  - `export const VOLITION_INJECTED_TEXT_PREFIX = "Simulated volition context for this turn"`
  - `export type InjectedVolitionText = { status: "found"; text: string } | { status: "none_injected" } | { status: "unavailable" }` (the locator's return type; the verdict consumes it)
  - `export type VolitionVerdictKind = "not_evaluated" | "no_decision" | "context_only" | "quiet" | "spoke"`
  - `export interface VolitionVerdict { kind: VolitionVerdictKind; line: string; caption: string | null; nudge: string | null }`
  - `export function selectVolitionVerdict(state: ConversationState, injected: InjectedVolitionText): VolitionVerdict`

- [ ] **Step 1: Write the failing tests**

Append to `crates/qsf_realtime_server/ui/src/realtime.test.ts`. Add `selectVolitionVerdict` to the existing import block from `./realtime`, then add this block. It reuses the `sampleCapture` shape already defined in the file, constructed inline here so the block is self-contained:

```ts
describe("volition verdict selector", () => {
  const unavailable = { status: "unavailable" } as const;
  const spokeCapture = {
    qsfSessionId: "session_1",
    exchangeIndex: 4,
    capturedAt: "2026-06-30T12:00:00Z",
    responseCreateEventRef: "hash-abc",
    inspection: {
      mode: "neutral",
      tick: 12,
      activeGoals: [],
      acceptedGoals: [],
      blockedGoals: [],
      cooldownGoals: [],
      retiredGoals: [],
      pendingCandidateCount: 0,
      acceptedCandidateCount: 0,
      lastInitiativeSummaries: [],
    },
    decision: {
      winner: {
        winnerGoalId: "serve-the-present-person",
        winnerGoalTitle: "Serve the present person",
        winnerEffectiveTier: 2,
        winnerBiasedTier: 2,
        protectedTierActive: true,
      },
      qualificationThreshold: 4,
      belowThreshold: [],
      modeBiasOutcomes: [],
      selectedGoalIds: ["serve-the-present-person"],
      omittedOrSuppressedGoalIds: [],
      shapingIntensity: "low",
      lastInitiativeOutputKind: "reflection_requested",
      lastInitiativeSurfaced: true,
      lastInitiativeSuppressionReason: null,
      lastInitiativeRenderedLinePresent: true,
    },
  };

  it("reports awaiting-first-turn when no capture has arrived", () => {
    const verdict = selectVolitionVerdict(INITIAL_STATE, unavailable);
    expect(verdict.kind).toBe("not_evaluated");
    expect(verdict.line).toContain("No evaluated turn yet");
    expect(verdict.caption).toBeNull();
    expect(verdict.nudge).toBeNull();
  });

  it("reports a spoken verdict with the winning goal, intensity word, and added nudge", () => {
    const state = { ...INITIAL_STATE, sessionId: "session_1", latestVolitionState: spokeCapture };
    const verdict = selectVolitionVerdict(state, unavailable);
    expect(verdict.kind).toBe("spoke");
    expect(verdict.line).toContain("Serve the present person");
    expect(verdict.line).toContain("gently");
    expect(verdict.caption).toBe("Latest evaluated turn · exchange 4");
    expect(verdict.nudge).toBe("nudge added");
  });

  it("reports a held-back nudge with the suppression reason when no line was rendered", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: {
        ...spokeCapture,
        decision: {
          ...spokeCapture.decision,
          lastInitiativeSurfaced: false,
          lastInitiativeSuppressionReason: "anti_nag_repeat" as const,
          lastInitiativeRenderedLinePresent: false,
        },
      },
    };
    const verdict = selectVolitionVerdict(state, unavailable);
    expect(verdict.kind).toBe("spoke");
    expect(verdict.nudge).toBe("nudge held back (Anti Nag Repeat)");
  });

  it("reports a quiet verdict with the below-threshold count and bar", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: {
        ...spokeCapture,
        decision: {
          ...spokeCapture.decision,
          winner: null,
          belowThreshold: [
            {
              goalId: "learn-what-drives-this-person",
              goalTitle: "Learn what drives this person",
              matchedKeywords: [{ term: "me", weightClass: "weak" as const }],
              matchStrength: 1,
            },
          ],
          shapingIntensity: "none",
        },
      },
    };
    const verdict = selectVolitionVerdict(state, unavailable);
    expect(verdict.kind).toBe("quiet");
    expect(verdict.line).toContain("No goal qualified to lead this turn");
    expect(verdict.line).toContain("1 goal(s) below the bar (threshold 4)");
    expect(verdict.line).not.toContain("base-model");
    expect(verdict.caption).toBe("Latest evaluated turn · exchange 4");
  });

  it("reports context-only when a no-decision capture pairs with an injected packet", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: { ...spokeCapture, decision: null },
    };
    const verdict = selectVolitionVerdict(state, {
      status: "found",
      text: "Simulated volition context for this turn (internal state only; not a claim of real desire or consciousness).\nDeclined goal candidates (coherence): pursue an unrelated tangent — would derail the current task.",
    });
    expect(verdict.kind).toBe("context_only");
    expect(verdict.line).toContain("still injected context");
  });

  it("reports a no-decision verdict when a capture has no decision and no packet was found", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: { ...spokeCapture, decision: null },
    };
    const verdict = selectVolitionVerdict(state, { status: "none_injected" });
    expect(verdict.kind).toBe("no_decision");
    expect(verdict.line).toContain("no per-turn decision");
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from `crates/qsf_realtime_server/ui/`: `npm run test`
Expected: FAIL — `selectVolitionVerdict is not exported` / not a function.

- [ ] **Step 3: Implement the selector**

Append to `crates/qsf_realtime_server/ui/src/realtime.ts` (after `selectVolitionPanelModel` and its helper functions, before the trailing `isRecord`/`stringField` helpers is fine — TS hoists functions, but keep it with the other selectors for readability):

```ts
/// The exact prefix every volition turn-context packet's rendered text begins with — on the
/// qualified-winner, no-qualifier, and coherence-only paths alike. The realtime server renders it
/// in `crates/qsf_realtime_server/src/realtime/volition_injection.rs`; a Rust guard test
/// (`packet_text_starts_with_ui_locator_prefix`) pins it so a reword there fails CI before this
/// locator silently stops matching.
export const VOLITION_INJECTED_TEXT_PREFIX = "Simulated volition context for this turn";

/// Result of the injected-packet lookup (`selectInjectedVolitionText`, Task 2). Three states so no
/// consumer can claim nothing was injected when the text is merely unavailable: `found` carries the
/// verbatim packet text; `none_injected` means the exchange-matched turn context was inspected and
/// carried no volition packet; `unavailable` means either capture is missing or the two captures
/// describe different turns (the expected non-atomic watch-channel window).
export type InjectedVolitionText =
  | { status: "found"; text: string }
  | { status: "none_injected" }
  | { status: "unavailable" };

export type VolitionVerdictKind =
  | "not_evaluated"
  | "no_decision"
  | "context_only"
  | "quiet"
  | "spoke";

export interface VolitionVerdict {
  /// Machine-readable state, used only to pick a style class in the renderer.
  kind: VolitionVerdictKind;
  /// One plain-English sentence describing volition's role in the latest reply.
  line: string;
  /// Which turn this verdict describes, e.g. "Latest evaluated turn · exchange 4". Null before any
  /// capture arrives. Surfaced so drift between the panel and the visible answer stays honest.
  caption: string | null;
  /// For a spoken turn: whether an extra initiative line was actually injected ("nudge added") or
  /// held back with a reason ("nudge held back (Anti Nag Repeat)"). Null otherwise. A goal can win
  /// and shape framing while its initiative line is suppressed, so this is reported separately.
  nudge: string | null;
}

/// Derive the plain-English verdict for the latest evaluated turn. Takes the exchange-matched
/// injected-packet lookup as an explicit input: the server can inject a coherence-only packet
/// (declined candidates, no arbitration winner) on a turn whose capture has `decision: null`, so
/// the no-decision wording is only safe when no matching packet was found. Total: returns a
/// defined verdict for every input, including before any capture arrives.
export function selectVolitionVerdict(
  state: ConversationState,
  injected: InjectedVolitionText,
): VolitionVerdict {
  const capture = state.latestVolitionState;
  if (capture === null) {
    return {
      kind: "not_evaluated",
      line: "No evaluated turn yet — awaiting the first volition-evaluated turn.",
      caption: null,
      nudge: null,
    };
  }

  const caption = `Latest evaluated turn · exchange ${capture.exchangeIndex}`;
  const decision = capture.decision;
  if (decision === null) {
    if (injected.status === "found") {
      return {
        kind: "context_only",
        line: "No goal led this turn, but volition still injected context (declined-goal coherence packet).",
        caption,
        nudge: null,
      };
    }
    // Deliberately does not claim "nothing was injected": `injected` may be merely unavailable
    // during the non-atomic watch-channel window.
    return {
      kind: "no_decision",
      line: "Volition was watching but recorded no per-turn decision.",
      caption,
      nudge: null,
    };
  }

  if (decision.winner === null) {
    // A no-qualifier turn still injects a packet telling the model volition stays quiet, so this
    // must not read as "base-model reply".
    const count = decision.belowThreshold.length;
    return {
      kind: "quiet",
      line: `No goal qualified to lead this turn — ${count} goal(s) below the bar (threshold ${decision.qualificationThreshold}). No winning goal shaped this reply.`,
      caption,
      nudge: null,
    };
  }

  const nudge = decision.lastInitiativeRenderedLinePresent
    ? "nudge added"
    : decision.lastInitiativeSuppressionReason !== null
      ? `nudge held back (${formatLabelValue(decision.lastInitiativeSuppressionReason)})`
      : null;
  return {
    kind: "spoke",
    line: `Volition spoke: ${decision.winner.winnerGoalTitle} tilted this reply — ${describeShapingIntensity(decision.shapingIntensity)}.`,
    caption,
    nudge,
  };
}

/// Map the wire shaping-intensity string to a plain adverb. `none` still reads as "lightly" (not
/// "not at all") because a winning goal always injects a framing packet — the intensity governs how
/// hard, not whether. Unknown values fall back to a title-cased label rather than throwing.
function describeShapingIntensity(intensity: string): string {
  switch (intensity.toLowerCase()) {
    case "none":
      return "lightly";
    case "low":
      return "gently";
    case "medium":
      return "moderately";
    case "high":
      return "strongly";
    default:
      return formatLabelValue(intensity);
  }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run from `crates/qsf_realtime_server/ui/`: `npm run test`
Expected: PASS (new block green; all existing tests still green).

- [ ] **Step 5: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/realtime.test.ts
git commit -m "feat(realtime-ui): derive a plain-English volition verdict selector

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: `selectInjectedVolitionText` — locate the verbatim injected packet

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/realtime.ts`
- Test: `crates/qsf_realtime_server/ui/src/realtime.test.ts`

**Interfaces:**
- Consumes: `ConversationState` (`latestVolitionState`, `latestTurnContext`), `VOLITION_INJECTED_TEXT_PREFIX` and `InjectedVolitionText` (Task 1), `isRecord` (existing private helper).
- Produces: `export function selectInjectedVolitionText(state: ConversationState): InjectedVolitionText`

- [ ] **Step 1: Write the failing tests**

Add `selectInjectedVolitionText` to the existing import from `./realtime`, then append this block to `realtime.test.ts`:

```ts
describe("injected volition text locator", () => {
  const volitionMessage = {
    type: "conversation.item.create",
    item: {
      type: "message",
      role: "system",
      content: [
        {
          type: "input_text",
          text: "Simulated volition context for this turn (internal state only; not a claim of real desire or consciousness).\nActive goal: Serve the present person (serve-the-present-person) — be useful now.",
        },
      ],
    },
  };
  const memoryMessage = {
    type: "conversation.item.create",
    item: {
      type: "message",
      role: "system",
      content: [{ type: "input_text", text: "Relevant memories: none." }],
    },
  };
  const captureAtExchange = (exchangeIndex: number) => ({
    qsfSessionId: "session_1",
    exchangeIndex,
    capturedAt: "2026-06-30T12:00:00Z",
    responseCreateEventRef: "hash-abc",
    inspection: {
      mode: "neutral",
      tick: 12,
      activeGoals: [],
      acceptedGoals: [],
      blockedGoals: [],
      cooldownGoals: [],
      retiredGoals: [],
      pendingCandidateCount: 0,
      acceptedCandidateCount: 0,
      lastInitiativeSummaries: [],
    },
    decision: null,
  });
  const turnContextAtExchange = (exchangeIndex: number, messages: unknown[]) => ({
    qsfSessionId: "session_1",
    exchangeIndex,
    capturedAt: "2026-06-30T12:00:00Z",
    requestHash: "hash-abc",
    messages,
  });

  const expectFound = (result: ReturnType<typeof selectInjectedVolitionText>): string => {
    if (result.status !== "found") {
      throw new Error(`expected found, got ${result.status}`);
    }
    return result.text;
  };

  it("returns the verbatim volition item text when both captures describe the same turn", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: captureAtExchange(4),
      latestTurnContext: turnContextAtExchange(4, [memoryMessage, volitionMessage]),
    };
    const text = expectFound(selectInjectedVolitionText(state));
    expect(text).toContain("Active goal: Serve the present person");
    expect(text).toMatch(/^Simulated volition context for this turn/);
  });

  it("reports unavailable when the two captures describe different turns", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: captureAtExchange(5),
      latestTurnContext: turnContextAtExchange(4, [volitionMessage]),
    };
    expect(selectInjectedVolitionText(state).status).toBe("unavailable");
  });

  it("reports none injected when the matched turn context has no volition item", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: captureAtExchange(4),
      latestTurnContext: turnContextAtExchange(4, [memoryMessage]),
    };
    expect(selectInjectedVolitionText(state).status).toBe("none_injected");
  });

  it("reports unavailable when either capture is missing", () => {
    expect(selectInjectedVolitionText(INITIAL_STATE).status).toBe("unavailable");
    expect(
      selectInjectedVolitionText({
        ...INITIAL_STATE,
        sessionId: "session_1",
        latestVolitionState: captureAtExchange(4),
      }).status,
    ).toBe("unavailable");
  });

  it("tolerates malformed messages without throwing", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: captureAtExchange(4),
      latestTurnContext: turnContextAtExchange(4, [
        null,
        "a string",
        { type: "conversation.item.create" },
        { type: "conversation.item.create", item: { content: [] } },
        volitionMessage,
      ]),
    };
    expect(expectFound(selectInjectedVolitionText(state))).toContain("Active goal:");
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run from `crates/qsf_realtime_server/ui/`: `npm run test`
Expected: FAIL — `selectInjectedVolitionText is not exported`.

- [ ] **Step 3: Implement the locator**

Append to `crates/qsf_realtime_server/ui/src/realtime.ts` (below `selectVolitionVerdict`):

```ts
/// Locate the verbatim volition turn packet the model saw this turn. The text is not carried on the
/// volition capture (kept out by a deliberate privacy guardrail); it rides inside the turn-context
/// messages as a `conversation.item.create` item whose text begins with
/// `VOLITION_INJECTED_TEXT_PREFIX`. Returns a status model rather than `string | null` so consumers
/// can tell "the matched turn context carried no packet" (`none_injected`) apart from "the matching
/// capture has not arrived or describes another turn" (`unavailable`). Total: never throws.
export function selectInjectedVolitionText(state: ConversationState): InjectedVolitionText {
  const capture = state.latestVolitionState;
  const context = state.latestTurnContext;
  if (capture === null || context === null) {
    return { status: "unavailable" };
  }
  // Only correlate when both captures describe the same turn. They are published by two
  // non-atomic watch-channel writes, so for a brief window the browser can hold a verdict for turn
  // N and a context for turn N-1; matching exchangeIndex rejects that mismatch instead of showing
  // last turn's injected text next to this turn's verdict.
  if (capture.exchangeIndex !== context.exchangeIndex) {
    return { status: "unavailable" };
  }
  for (const message of context.messages) {
    const text = volitionItemText(message);
    if (text !== null && text.startsWith(VOLITION_INJECTED_TEXT_PREFIX)) {
      return { status: "found", text };
    }
  }
  return { status: "none_injected" };
}

/// Extract the first content text from a `conversation.item.create` message, or null if the value
/// is not that shape. Defensive at every hop so a malformed capture can never throw.
function volitionItemText(message: unknown): string | null {
  if (!isRecord(message) || message.type !== "conversation.item.create") {
    return null;
  }
  const item = message.item;
  if (!isRecord(item)) {
    return null;
  }
  const content = item.content;
  if (!Array.isArray(content)) {
    return null;
  }
  const first = content[0];
  if (!isRecord(first) || typeof first.text !== "string") {
    return null;
  }
  return first.text;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run from `crates/qsf_realtime_server/ui/`: `npm run test`
Expected: PASS (new block green; existing tests still green).

- [ ] **Step 5: Typecheck and lint**

Run from `crates/qsf_realtime_server/ui/`: `npm run check`
Expected: no type or Biome errors. If Biome reports formatting, run `npm run fmt` and re-run `npm run check`.

- [ ] **Step 6: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/realtime.ts crates/qsf_realtime_server/ui/src/realtime.test.ts
git commit -m "feat(realtime-ui): locate verbatim injected volition text by exchange-matched prefix

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Rust guard test pinning the injected-text prefix

**Files:**
- Modify (tests only): `crates/qsf_realtime_server/src/realtime/volition_injection.rs` (inside the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `fixture_state`, `VolitionStateSnapshot`, `build_volition_turn_context_packet`, `select_goals_ranked`, `arbitrate_with_mode`, `Mode`, `ShapingIntensity`, `detect_opportunities`, `grounded_terms_from_text`, `DeclinedCandidate`, `DeclineReason` — all already in scope in that test module (the last four are used the same way by the existing `coherence_only_turn_injects_declined_candidates_with_no_goal_selected` test).
- Produces: a test that fails if the rendered packet text stops starting with `Simulated volition context for this turn` on any of the three packet-emitting paths: qualified winner, no qualifier, and coherence only.

- [ ] **Step 1: Write the guard test**

Append inside `mod tests` in `crates/qsf_realtime_server/src/realtime/volition_injection.rs`:

```rust
#[test]
fn packet_text_starts_with_ui_locator_prefix() {
    // The realtime browser UI locates the injected volition item by this exact prefix — see
    // `VOLITION_INJECTED_TEXT_PREFIX` and `selectInjectedVolitionText` in
    // crates/qsf_realtime_server/ui/src/realtime.ts. If you reword the rendered packet, update
    // that constant and its tests too; this assertion exists so the reword fails CI here first.
    const UI_LOCATOR_PREFIX: &str = "Simulated volition context for this turn";
    let (fixture, state) = fixture_state();
    let snapshot = VolitionStateSnapshot {
        state: state.clone(),
        fixture: fixture.clone(),
    };

    // Qualified-winner path.
    let ranked = select_goals_ranked("how can you help me", &state, &fixture);
    let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        outcome,
        &[],
        ShapingIntensity::Low,
        "stable-baseline-hash".to_string(),
        None,
        &[],
    )
    .expect("qualified winner emits a packet");
    assert!(
        packet.text.starts_with(UI_LOCATOR_PREFIX),
        "qualified-path packet prefix drifted from the UI locator: {}",
        packet.text
    );

    // No-qualifier path (goals activate but none clear the bar).
    let ranked = select_goals_ranked("for what it's worth, thanks", &state, &fixture);
    let outcome = arbitrate_with_mode(ranked.selected.clone(), &fixture, Mode::Neutral);
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        outcome,
        &[],
        ShapingIntensity::None,
        "stable-baseline-hash".to_string(),
        None,
        &[],
    )
    .expect("no-qualifier turn emits a packet");
    assert!(
        packet.text.starts_with(UI_LOCATOR_PREFIX),
        "no-qualifier-path packet prefix drifted from the UI locator: {}",
        packet.text
    );

    // Coherence-only path (no ranked selection or arbitration winner; declined candidates only).
    let ranked = select_goals_ranked("xyzzy frobnicator quux", &state, &fixture);
    let opportunities = detect_opportunities(&grounded_terms_from_text("xyzzy"), &state, &fixture);
    let declined = vec![DeclinedCandidate {
        candidate_id: "candidate-3".to_string(),
        title: "pursue an unrelated tangent".to_string(),
        conflict: DeclineReason::ConflictingGoal {
            goal_id: "keep-theses-distinct-from-fact".to_string(),
        },
        rationale: "would derail the current task".to_string(),
        tick: 5,
    }];
    let packet = build_volition_turn_context_packet(
        &snapshot,
        &ranked,
        None,
        &opportunities,
        ShapingIntensity::None,
        "stable-baseline-hash".to_string(),
        None,
        &declined,
    )
    .expect("coherence-only turn emits a packet");
    assert!(
        packet.text.starts_with(UI_LOCATOR_PREFIX),
        "coherence-only-path packet prefix drifted from the UI locator: {}",
        packet.text
    );
}
```

- [ ] **Step 2: Run the test to verify it passes**

Run from repo root: `cargo test -p qsf_realtime_server packet_text_starts_with_ui_locator_prefix`
Expected: PASS (1 test run).

- [ ] **Step 3: Clippy and format**

Run from repo root: `cargo clippy --all-targets -- -D warnings` then `cargo fmt`
Expected: no warnings.

- [ ] **Step 4: Commit**

```bash
git add crates/qsf_realtime_server/src/realtime/volition_injection.rs
git commit -m "test(realtime): pin volition packet prefix the browser UI locates by

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Render the three-tier panel

**Files:**
- Modify: `crates/qsf_realtime_server/ui/src/main.ts`
- Modify: `crates/qsf_realtime_server/ui/src/styles.css`

**Interfaces:**
- Consumes: `selectVolitionVerdict`, `selectInjectedVolitionText` (Tasks 1–2), `selectVolitionPanelModel` (existing), `renderVolitionStatePanel` (existing, reused unchanged for Tier 3).
- Produces: user-visible panel; no exported API. Verified by typecheck, existing tests, and manual run.

- [ ] **Step 1: Import the new selectors**

In `crates/qsf_realtime_server/ui/src/main.ts`, extend the existing import from `./realtime` to include `selectVolitionVerdict` and `selectInjectedVolitionText` alongside `selectVolitionPanelModel`:

```ts
  reduceConversationState,
  type SdpExchangeResponse,
  type SessionAllocationResponse,
  selectInjectedVolitionText,
  selectMuteButton,
  selectVolitionPanelModel,
  selectVolitionVerdict,
} from "./realtime";
```

- [ ] **Step 2: Reorder the two disclosures and rename the volition summary**

In the `root.innerHTML` template, replace the current order (turn context first, volition second) so the volition disclosure comes first and its summary is renamed. Replace this block:

```html
        <details class="turn-context-details">
          <summary>Last turn context</summary>
          <div data-role="turn-context-body" class="turn-context-body"></div>
        </details>
        <details class="turn-context-details">
          <summary>Volition state</summary>
          <div data-role="volition-state-body" class="volition-state-body"></div>
        </details>
```

with:

```html
        <details class="turn-context-details" open>
          <summary>What volition did this turn</summary>
          <div data-role="volition-state-body" class="volition-state-body"></div>
        </details>
        <details class="turn-context-details">
          <summary>Last turn context</summary>
          <div data-role="turn-context-body" class="turn-context-body"></div>
        </details>
```

- [ ] **Step 3: Swap the render call**

In `render()`, replace this line:

```ts
  renderVolitionStatePanel(refs.volitionStateBody, selectVolitionPanelModel(state));
```

with:

```ts
  const injectedVolition = selectInjectedVolitionText(state);
  renderWhyThisAnswerPanel(
    refs.volitionStateBody,
    selectVolitionVerdict(state, injectedVolition),
    injectedVolition,
    selectVolitionPanelModel(state),
  );
```

- [ ] **Step 4: Add the composition renderer**

Add this function to `main.ts` (next to the existing `renderVolitionStatePanel`, which stays unchanged and is reused for Tier 3):

```ts
function renderWhyThisAnswerPanel(
  container: HTMLElement,
  verdict: ReturnType<typeof selectVolitionVerdict>,
  injected: ReturnType<typeof selectInjectedVolitionText>,
  model: ReturnType<typeof selectVolitionPanelModel>,
) {
  container.replaceChildren();

  // Tier 1 — verdict.
  const verdictBlock = document.createElement("div");
  verdictBlock.className = `why-verdict why-verdict-${verdict.kind}`;
  const line = document.createElement("p");
  line.className = "why-verdict-line";
  line.textContent = verdict.line;
  verdictBlock.appendChild(line);
  if (verdict.nudge !== null) {
    const nudge = document.createElement("span");
    nudge.className = "why-nudge";
    nudge.textContent = verdict.nudge;
    verdictBlock.appendChild(nudge);
  }
  if (verdict.caption !== null) {
    const caption = document.createElement("p");
    caption.className = "why-verdict-caption";
    caption.textContent = verdict.caption;
    verdictBlock.appendChild(caption);
  }
  container.appendChild(verdictBlock);

  // Tier 2 — what volition told the model. "Nothing was injected" is only claimed when the
  // exchange-matched turn context was actually inspected; a missing/mismatched capture (the
  // non-atomic watch-channel window) gets a neutral "not captured" line instead.
  const injectedSection = document.createElement("section");
  injectedSection.className = "why-injected";
  const injectedHeading = document.createElement("h3");
  injectedHeading.textContent = "What volition told the model";
  injectedSection.appendChild(injectedHeading);
  if (injected.status === "found") {
    const pre = document.createElement("pre");
    pre.className = "why-injected-text";
    pre.textContent = injected.text;
    injectedSection.appendChild(pre);
  } else {
    const placeholder = document.createElement("p");
    placeholder.className = "why-injected-empty";
    placeholder.textContent =
      injected.status === "none_injected"
        ? "Nothing was injected this turn."
        : "No matching injected packet captured for this evaluated turn.";
    injectedSection.appendChild(placeholder);
  }
  container.appendChild(injectedSection);

  // Tier 3 — scoring detail, collapsed. Reuses the existing panel renderer verbatim.
  const scoring = document.createElement("details");
  scoring.className = "why-scoring";
  const scoringSummary = document.createElement("summary");
  scoringSummary.textContent = "Scoring detail";
  scoring.appendChild(scoringSummary);
  const scoringBody = document.createElement("div");
  scoringBody.className = "volition-state-body";
  renderVolitionStatePanel(scoringBody, model);
  scoring.appendChild(scoringBody);
  container.appendChild(scoring);
}
```

- [ ] **Step 5: Add styles**

Append to `crates/qsf_realtime_server/ui/src/styles.css` (after the `.volition-state-row dd` rule at the end of the volition block):

```css
.why-verdict {
  padding: 0.8rem 0.9rem;
  border-radius: 12px;
  border: 1px solid rgba(125, 211, 252, 0.24);
  background: rgba(125, 211, 252, 0.12);
}

.why-verdict-not_evaluated,
.why-verdict-no_decision,
.why-verdict-context_only,
.why-verdict-quiet {
  border-color: rgba(184, 191, 215, 0.18);
  background: rgba(184, 191, 215, 0.08);
}

.why-verdict-line {
  margin: 0;
  color: var(--text);
  font-size: 0.95rem;
  line-height: 1.45;
}

.why-nudge {
  display: inline-block;
  margin-top: 0.5rem;
  padding: 0.15rem 0.55rem;
  border-radius: 999px;
  background: rgba(245, 158, 11, 0.16);
  border: 1px solid rgba(245, 158, 11, 0.3);
  color: var(--text);
  font-size: 0.72rem;
  letter-spacing: 0.04em;
}

.why-verdict-caption {
  margin: 0.5rem 0 0;
  color: var(--muted);
  font-size: 0.72rem;
  text-transform: uppercase;
  letter-spacing: 0.12em;
}

.why-injected h3 {
  margin: 0 0 0.45rem;
  color: var(--accent-2);
  font-size: 0.76rem;
  text-transform: uppercase;
  letter-spacing: 0.16em;
}

.why-injected-empty {
  margin: 0;
  color: var(--muted);
  font-size: 0.88rem;
  font-style: italic;
}

.why-injected-text {
  margin: 0;
  padding: 0.75rem;
  border-radius: 10px;
  background: rgba(0, 0, 0, 0.3);
  border: 1px solid rgba(255, 255, 255, 0.08);
  font-size: 0.82rem;
  line-height: 1.5;
  color: var(--text);
  max-height: 320px;
  overflow: auto;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.why-scoring summary {
  cursor: pointer;
  color: var(--muted);
  font-size: 0.74rem;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  user-select: none;
}
```

- [ ] **Step 6: Typecheck, lint, and run existing tests**

Run from `crates/qsf_realtime_server/ui/`: `npm run check` then `npm run test`
Expected: no type/Biome errors; all tests pass. If Biome flags formatting, run `npm run fmt` then re-run `npm run check`.

- [ ] **Step 7: Manual verification (human testing recommended)**

Run `qsf.ps1 realtime`, open the browser UI, and start a conversation. Confirm:
- The right panel's first disclosure now reads **"What volition did this turn"** and is open by default.
- After the first reply, Tier 1 shows a plain verdict sentence with a **"Latest evaluated turn · exchange N"** caption.
- Tier 2 shows the injected packet text; "Nothing was injected this turn." appears only on a turn whose matched context truly carried no packet, and "No matching injected packet captured…" appears transiently before the matching turn context arrives.
- Tier 3 **"Scoring detail"** is collapsed and, when expanded, shows the same rows as before.
- Send a stopword-only turn (e.g. "thanks") and confirm the verdict reads "No goal qualified to lead this turn…" while Tier 2 still shows the injected no-qualifier packet ("Volition stays quiet this turn").

- [ ] **Step 8: Commit**

```bash
git add crates/qsf_realtime_server/ui/src/main.ts crates/qsf_realtime_server/ui/src/styles.css
git commit -m "feat(realtime-ui): reframe volition panel as a per-turn 'what this answer' view

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: Documentation

**Files:**
- Modify: `docs/DecisionLog.md`
- Modify: `docs/Architecture/Architecture.RealtimeSessionServer.md`

**Interfaces:** none (prose only).

- [ ] **Step 1: Add a DecisionLog entry**

Add an entry to `docs/DecisionLog.md` using the file's `Decision:` / `Context:` / `Consequences:` template, dated `2026-07-05`, with this content:

```markdown
## 2026-07-05 - Realtime browser explains volition's per-turn effect via presentation-only selectors
Decision: The realtime browser explains volition's effect on the latest reply purely in the
TypeScript view-model layer, from the existing `volition_state` and `turn_context` captures
correlated by `exchange_index` — no new wire fields, no reducer changes, no server-behavior
changes.
Context: The right-hand panel was a flat dump of volition-domain fields (tick, tiers, salience,
thresholds) that assumed familiarity with the volition model and never connected a decision to
the answer it shaped.
Consequences: Per-turn explanation stays on the read-only, non-blocking observation plane;
richer explanations must come from correlating existing captures (or extending a capture), not
from coupling the UI to server internals. The injected text is located client-side by the
packet's prose prefix, pinned by a Rust guard test across every packet-emitting path. Deferred
follow-ups (revisit after using the prototype): binding each capture to the specific transcript
answer that produced it, surfacing the standing persona/stable-baseline stance, and captioning
interrupted/failed turns.
```

- [ ] **Step 2: Update the Architecture document's authoritative status**

In `docs/Architecture/Architecture.RealtimeSessionServer.md`, the `Implementation Status` section (not the appended note below) is what future readers trust for implemented behavior, per `ProjectWorkflow.md` / `DocumentStatus.md`. Make both changes:

1. In **Implemented today**, after the `volition_inspection_tx` bullet, add:

```markdown
- The browser UI renders a per-turn "What volition did this turn" panel derived
  entirely in the view-model layer from the `turn_context` and `volition_state`
  captures correlated by `exchange_index`: a plain-English verdict, the verbatim
  injected volition packet located by its prose prefix, and the previous detailed
  rows collapsed into a "Scoring detail" section.
```

2. Refresh the `Last reviewed:` line at the end of the section to `2026-07-05` and mention the browser panel reframe alongside what it already lists.

- [ ] **Step 3: Add an Architecture note**

Append this subsection to the end of `docs/Architecture/Architecture.RealtimeSessionServer.md`:

```markdown
## Browser "What volition did this turn" panel

The realtime browser UI derives a per-turn explanation of the latest reply purely
in the TypeScript view-model layer, from two server-pushed captures it already
receives: the `volition_state` inspection capture (mode, tick, decision, scoring)
and the `turn_context` capture (the verbatim messages sent to the provider). The
panel correlates them by matching `exchange_index`, renders a plain-English
verdict plus the injected volition packet located from the turn-context messages,
and demotes the detailed scoring rows into a collapsed section. The volition
capture deliberately excludes the injected instruction text (privacy guardrail);
the panel reads that text from the turn-context capture instead. This keeps the
observation plane read-only and non-blocking: the selectors are total and a
malformed capture cannot break the transcript render.
```

- [ ] **Step 4: Commit**

```bash
git add docs/DecisionLog.md docs/Architecture/Architecture.RealtimeSessionServer.md
git commit -m "docs: record realtime volition panel reframe

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**Spec coverage** (against the agreed design and the 2026-07-05 review findings):
- Verdict tier (5 states + separate nudge clause + honest caption); "base-model reply" wording removed everywhere a packet may still have been injected → Task 1. ✓
- `decision: null` + coherence-only packet distinguished from a true no-packet turn via the `context_only` kind, with a dedicated selector test → Task 1. ✓
- Injected-text tier, exchange-guarded, prefix-located, returning a `found`/`none_injected`/`unavailable` status model so unavailable captures are never rendered as "nothing injected" → Tasks 2, 4. ✓
- Prefix brittleness mitigated by a CI guard test covering all three packet-emitting paths (qualified winner, no qualifier, coherence only) → Task 3. ✓
- Scoring tier demoted into collapsed disclosure, existing rows unchanged → Task 4 (reuses `renderVolitionStatePanel`). ✓
- Panel renamed / reordered; over-attribution avoided via naming + caption → Task 4. ✓
- Totality / read-only observation plane → Tasks 1–2 selectors are total; Task 4 render is pure DOM construction. ✓
- Deferred blindspots (per-answer binding, persona stance, interrupted-turn captioning) recorded, not silently dropped → Task 5. ✓
- Docs per ProjectWorkflow (DecisionLog entry in the `Decision:`/`Context:`/`Consequences:` template; Architecture `Implementation Status` bullets and `Last reviewed` refreshed, plus the explanatory subsection) → Task 5. ✓

**Placeholder scan:** No TBD/TODO; every code and test step carries complete content. ✓

**Type consistency:** `VolitionVerdict` fields (`kind`, `line`, `caption`, `nudge`) are produced in Task 1 and consumed by name in Task 4. `InjectedVolitionText` is defined in Task 1, returned by `selectInjectedVolitionText` in Task 2, consumed as the verdict selector's second argument (Task 1) and by the renderer's status switch (Task 4). `VOLITION_INJECTED_TEXT_PREFIX` defined in Task 1, used in Task 2, mirrored by the literal in Task 3. Wire field names (`lastInitiativeRenderedLinePresent`, `lastInitiativeSuppressionReason`, `shapingIntensity`, `belowThreshold`, `qualificationThreshold`, `winnerGoalTitle`, `exchangeIndex`) match the existing `realtime.ts` interfaces. ✓
