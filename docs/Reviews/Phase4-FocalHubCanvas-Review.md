# Code Review: Phase 4 Focal-Hub Canvas

**Date:** 2026-05-22
**Reviewer:** Code Reviewer (subagent)
**Scope:** Staged changes for Phase 4 of the Memory Association Browser plan.
**Range:** staged (HEAD = 13f637f)

## Summary

The implementation delivers everything Phase 4 promises and improves on the plan in several material ways: the brittle `setTimeout(50)` re-render queue is replaced by an explicit `pendingRender`/`disposed` state machine, the scene is split into small private helpers, and the dashed-edge algorithm renders the final partial segment. The work is close to commit-ready, but there are a handful of issues worth fixing first: aggressive scene teardown on transient failures, a misleading cursor on broken neighbors, missing retina handling, a quiet leak when `app.init()` rejects, and weaker test coverage than the helper would support.

## Strengths

- **Clean separation of pure layout from rendering.** `radial.ts:12` is small, deterministic, and trivially testable, exactly matching the unidirectional-data-flow rule in `Agents.md` ("Reducers must stay pure and unit-testable"). The scene composes it via `positionNeighbors` (`focalHub.ts:145`), which is itself almost-pure.
- **Better readiness gating than the plan.** The plan's `setTimeout(() => this.render(...), 50)` recursive timer (Plan line 3365) is replaced by a single `pendingRender` slot drained inside `init()` (`focalHub.ts:115-119`). This avoids unbounded retry storms and respects the `disposed` flag.
- **Disposal flag protects async init.** `focalHub.ts:98-109` correctly handles the "destroy was called during `app.init()`" race by destroying the freshly-initialized app inline. The plan's reference code did not handle this at all.
- **Render method extracted from positioning.** `neighborIds()` and `positionNeighbors()` (`focalHub.ts:135-159`) are small, focused, and unit-testable in isolation if extracted — much cleaner than the plan's monolithic `render()`.
- **Sequence counter mirrors the inspector pattern.** `main.ts:25,62,65` follows the same `seq`/`reloadSeq`/`inspectorSeq` cancellation idiom already used for the inspector — consistent with the codebase.
- **Stale closure captured correctly.** `main.ts:50` snapshots `state.selectedId` into a local `selectedId` constant before awaiting, so a concurrent selection change cannot poison the render. Subtle and correct.
- **Layout CSS reset is right.** `layout.css:45-54` adds `min-height: 0` and `overflow: hidden` to the grid item and forces `display:block; width:100%; height:100%` on the canvas — the exact incantation required to make a Pixi `<canvas>` size predictably inside a CSS grid.
- **Plan checkboxes and diary entry both updated** before commit, matching the workflow in `Agents.md`.

## Issues

### Critical (must fix before commit)

None. There are no data-loss, crash, or security issues. The Important items below are real but recoverable.

### Important (should fix before merge to main)

#### 1. Pixi `Application` is leaked when `app.init()` throws

**Where:** `focalHub.ts:91-126`

**What:** The `init()` `catch` block writes an error string into the slot and returns. The half-initialized `this.app` is never destroyed. A subsequent `destroy()` call skips destruction because `this.ready === false` (`focalHub.ts:38`). The Pixi `Application` object (and any partially-allocated WebGL resources) leaks for the lifetime of the page.

**Why it matters:** WebGL contexts are a scarce browser resource (Chrome caps them around 16). A user who hits an init failure once, then clears selection, then selects again will create a second `Application`, and so on. The leak is silent.

**Fix:** In the `catch` branch, attempt to destroy the app defensively:

```ts
} catch (err: unknown) {
  try { this.app.destroy({ removeView: true }, { children: true }); } catch {}
  if (this.disposed) return;
  slot.textContent = `canvas init failed: ${err instanceof Error ? err.message : String(err)}`;
}
```

#### 2. Aggressive scene teardown on transient neighborhood-fetch failures

**Where:** `main.ts:67-73`

**What:** When `api.getMemoryNeighborhood(...)` rejects, the code destroys the entire `FocalHubScene` and nulls it out. The next successful selection has to re-initialize Pixi (new WebGL context, new shaders, new `Application`), which is the most expensive operation in the whole scene lifecycle.

**Why it matters:** A flaky network or a server hiccup will cause the user to see the scene unmount and remount with visible latency on every retry. The plan's reference code (Plan line 3501-3503) just wrote "no neighborhood data" into the slot without tearing down. The scene already handles repeated `render()` calls cheaply via `clearLayer`; there is no reason to destroy on a transient error.

**Fix:** Drop the `scene?.destroy(); scene = null;` lines and just display an error message in the inspector, or paint an error label inside the existing scene. Keep destroy reserved for the "selection cleared" path.

#### 3. Misleading cursor on broken neighbor nodes

**Where:** `focalHub.ts:194`

**What:** `node.cursor = "pointer"` is set unconditionally, before the `if (!broken)` guard around `pointertap`. Broken neighbors show a clickable pointer cursor but ignore clicks.

**Why it matters:** The inspector's broken-association rows correctly use `cursor: default` (and skip the click handler) — see `inspector.ts:46`. The canvas should mirror that. A user who clicks a broken edge and nothing happens will assume the UI is broken.

**Fix:**

```ts
node.eventMode = "static";
node.cursor = broken ? "default" : "pointer";
if (!broken) {
  node.on("pointertap", () => this.onSelect(id));
}
```

#### 4. No retina/HiDPI handling

**Where:** `focalHub.ts:93-97`

**What:** `app.init({...})` does not pass `resolution` or `autoDensity`. Pixi v8 defaults to `resolution: 1`, so on a 2x display the canvas backbuffer is half the CSS resolution and labels render visibly blurry.

**Why it matters:** The plan calls for "readable" labels; the diary explicitly says the project owner verified legibility on the fixture. On a retina laptop (common in this user base) text will look fuzzy compared to the surrounding HTML.

**Fix:**

```ts
await this.app.init({
  background: BACKGROUND,
  resizeTo: slot,
  antialias: true,
  resolution: window.devicePixelRatio || 1,
  autoDensity: true,
});
```

#### 5. `radial.test.ts` misses the obvious edge cases

**Where:** `radial.test.ts:1-24`

**What:** The test covers `count=4` and `count=0`/`count=-1`. It does not exercise:
- `count=1` (the single-neighbor case — should produce one point straight up at `(0, -radius)`)
- `radius=0` (everything collapses to the origin)
- The `angle` field of `NeighborLayout` (currently untested, so a regression that swapped axes wouldn't fail the test)
- The length of the output for non-trivial counts (covered for count=4 but not for, say, 8 — the maximum the scene actually requests)

**Why it matters:** `Agents.md` says "Prefer tests of reducer behavior, emitted effects, and public contracts over internal details." `radialPositions` is a public contract; its tests should pin down the contract.

**Fix:** Add tests for `count=1`, the `angle` values, and `count=8` (matching the production limit).

#### 6. Edge-mapping helpers in `focalHub.ts` are pure but neither extracted nor tested

**Where:** `focalHub.ts:135-159` (`neighborIds`, `positionNeighbors`), plus the `1 + (weight/maxWeight) * 3` formula at `focalHub.ts:168`.

**What:** These are deterministic and Pixi-free. They could live next to `radial.ts` and be tested in jsdom without Pixi.

**Why it matters:** The plan explicitly calls out "Edge width scaled by weight (`1 + (weight/maxWeight) * 3`)" as a numeric contract. If someone later swaps it for a log scale or changes the `0.001` sentinel, nothing fails. The neighbor-ID deduplication (which silently drops self-loops and merges duplicate edges) is also a behavioral contract worth pinning.

**Fix:** Extract three pure helpers into `canvas/scene.ts` (or similar):
- `computeNeighborIds(centerId, edges): string[]` (dedupes, filters center)
- `edgeWidth(weight, maxWeight): number`
- `maxEdgeWeight(edges): number` (encapsulating the `0.001` sentinel)

Test them. The scene then composes them.

#### 7. `app.screen` size when the slot is initially 0×0

**Where:** `focalHub.ts:62-64`

**What:** If the slot is hidden, has `display:none`, or simply hasn't laid out when `render()` is first called (e.g., the bootstrap path races layout), `app.screen.width/height` are 0 and the scene renders at coords `(0,0)` with `radius = max(56, 0) = 56`. The drawing is correct mathematically but invisible until Pixi's `resizeTo` observer fires.

**Why it matters:** `resizeTo: slot` uses a `ResizeObserver`; if the slot resizes after the first `render()`, the existing graphics are positioned at the old coordinates and only repaint on the next `render()` call. There is no internal "re-layout on resize" hook.

**Fix (lightweight):** Listen for `ResizeObserver` directly on the slot (or hook `app.renderer.on("resize", ...)`) and re-run the last render. Track the last `(centerId, neighborhood)` rendered for this purpose. Alternatively, document this as a known limitation if the slot is guaranteed laid-out before first render (the current `getSlots(root)` call after `renderShell` does happen synchronously, but the canvas is constructed lazily — so this is a real risk on first selection).

### Minor (nice to have)

#### 8. `clearLayer` `removeChildren()` is redundant after `destroy({ children: true })`

**Where:** `focalHub.ts:128-133`

**What:** In Pixi v8, `displayObject.destroy()` removes the object from its parent. Iterating and destroying every child, then calling `removeChildren()`, double-removes. Harmless but noisy.

**Fix:** Drop the `layer.removeChildren()` line, or — cleaner — use `layer.removeChildren().forEach(c => c.destroy({ children: true }))`. The latter avoids mutating the children array mid-iteration (the current spread `[...layer.children]` guards against that, which is fine, just verbose).

#### 9. `pointerout` clears the hover layer unconditionally

**Where:** `focalHub.ts:201`

**What:** When the pointer moves from neighbor A to neighbor B in a single frame, `A.pointerout` fires, clearing the tooltip; `B.pointerover` fires, drawing a new one. Net effect is fine but there's a one-frame flicker. Also, if the pointer leaves a node while the tooltip belongs to another node (theoretically possible with overlapping hit areas), the wrong tooltip is dismissed.

**Fix:** Optional. Track the currently-tooltipped id and only clear when it matches the leaving node.

#### 10. Inline `truncateId` versus the plan's `id.slice(0, 10) + "…"`

**Where:** `focalHub.ts:284-286`

**What:** The plan used an inline `id.slice(0, 10) + "…"`. The implementation factored that into a module-private helper that also avoids truncating strings shorter than 11 chars and uses ASCII `...` instead of the ellipsis `…`. Functionally improved (no spurious ellipsis for short ids).

**Fix:** Consider exporting `truncateId` for reuse (the inspector or future canvas affordances will want the same logic). Also consider the real ellipsis character `…` for visual density.

#### 11. Bundle size: PixiJS adds ~250 KB to the chunk

**Where:** `package.json:21`, diary entry.

**What:** The diary acknowledges Vite's large-chunk warning. No `manualChunks` config, no lazy import. For an internal workbench MVP this is fine, but it is worth a `Plan.Phase5` note since Phase 5 ("Packaging") will embed this frontend.

**Fix:** Add a small Phase-5 entry: investigate `vite.config.ts` `build.rollupOptions.output.manualChunks: { pixi: ['pixi.js'] }` to split it into its own chunk, and consider `import('./canvas/focalHub')` for lazy loading once the user actually selects a memory.

#### 12. A11y / keyboard navigation

**Where:** `focalHub.ts` entire file.

**What:** The canvas is purely visual. There is no keyboard navigation, no announcement of the current center, no aria-live region for the canvas region.

**Fix:** Either document explicitly that keyboard navigation is out of scope for Phase 4 (the inspector and list are keyboard-reachable, which is enough for the MVP), or add an `aria-label` to the canvas slot describing the current center. Adding a note in `Plan.MemoryAssociationBrowser.md` about this being a deferred concern would close the loop.

#### 13. Background color duplicated as `0x07162a` and `"#07162a"`

**Where:** `focalHub.ts:10` (`BACKGROUND = "#07162a"`) and `focalHub.ts:250` (`0x07162a` in tooltip fill).

**What:** The same color appears as both a hex string (for Pixi `Application.init`) and as a numeric literal (for `Graphics.fill`). DRY violation; if the design token changes, this drifts.

**Fix:** `const COLOR_BG_NUM = 0x07162a;` then `BACKGROUND = "#07162a"` derived from a comment, or just both pointing at the same numeric. Better still: read from a CSS custom property if Pixi can consume it (it cannot directly, but a runtime read of `getComputedStyle(slot).getPropertyValue("--qsf-bg-canvas")` would unify with the tokens system).

#### 14. `0.001` sentinel for `maxWeight`

**Where:** `focalHub.ts:68-71`

**What:** Using `0.001` as the initial value of `reduce` means an empty edge list yields `maxWeight = 0.001` and an edge list with all-zero weights also yields `0.001`. For empty edges the loop body never runs anyway. For all-zero weights, `lineWidth = 1` for every edge — defensible. But the sentinel is opaque without a comment.

**Fix:** `const maxWeight = Math.max(0.001, ...neighborhood.edges.map(e => e.weight));` plus a one-line comment explaining the sentinel prevents `lineWidth = NaN`.

## Plan Deviations

| Plan code | Implementation | Verdict |
|---|---|---|
| `setTimeout(() => this.render(...), 50)` queue (Plan line 3365) | `pendingRender` slot drained from `init()` (`focalHub.ts:54-57, 115-119`) | **Improvement.** No timer, no recursive retries, plays well with `disposed`. |
| No `disposed` flag, no `destroy()` method | Full `disposed`/`destroy()` lifecycle (`focalHub.ts:35-50, 98-109`) | **Improvement.** Required by the `main.ts` integration (which now destroys on selection clear). |
| `id.slice(0, 10) + "…"` inline (Plan line 3416) | `truncateId(id)` helper using `...` and a length guard (`focalHub.ts:284-286`) | **Improvement** (no spurious ellipsis for short ids). Minor regression on ellipsis character. |
| `while (drawn + dashLen < dist)` — final partial dash dropped (Plan line 3471) | `while (drawn < dist)` with `Math.min(drawn + dashLen, dist)` for the final dash (`focalHub.ts:275-280`) | **Improvement** for short broken edges. |
| `dist === 0` not guarded (would divide by zero) (Plan line 3464) | Explicit `if (dist === 0) return;` (`focalHub.ts:267`) | **Improvement.** Bug fix. |
| Render writes to `n.center.title` directly (Plan line 3434) | Same path, via `drawCenter` (`focalHub.ts:218-234`) | Neutral. Extracted helper is cleaner. |
| `slots.canvasSlot.textContent = "no neighborhood data"` on fetch failure, scene retained (Plan line 3502) | Scene is destroyed and nulled on fetch failure (`main.ts:69-71`) | **Regression.** See Important issue #2. |
| `renderInspector(slots.inspector, state.selectedId, dispatch)` — no sequence guard (Plan line 3496) | Sequence-gated `renderInspector` plus a new `canvasSeq` mirroring the pattern (`main.ts:48-66`) | **Improvement.** The plan's reference code had a known race the implementation fixes. |
| `drawDashed(g, x1, y1, x2, y2, ...)` — six positional args (Plan line 3461) | `drawDashed(g, from, to, ...)` with `Point` interface (`focalHub.ts:257-263`) | **Improvement.** Fewer mis-orderable args. |
| No resolution/autoDensity (Plan line 3350) | Still no resolution/autoDensity (`focalHub.ts:93-97`) | **Carried-over gap.** See Important issue #4. The plan was wrong; the implementation inherited that. |
| Plan promised "Standard closing steps" run | `npm run check` and `npm run fmt` not yet shown as run in this review session | Cannot verify from staged state alone. The diary claims `npm run build`, `npm run check`, and `npm test` all pass. |

## Recommendations

- **Address the Important items in a follow-up commit** rather than amending. Issues 1 and 2 are not technically blockers but together they materially harm the UX/robustness of the new code path.
- **Extract and test the edge-mapping helpers** (Important #6) before Phase 5. Phase 5 packages this UI into the binary; pinning the contract now is cheap.
- **Add a regression test for the scene/main integration** if practical — even a smoke test that asserts `FocalHubScene` is constructed exactly once across selection changes and destroyed exactly once on clear. Pixi is hard to test in jsdom but the construction count can be asserted by injecting a factory.
- **Update `Plan.MemoryAssociationBrowser.md`** to note the resolution/autoDensity and aggressive-teardown deviations from the reference code; future phases reading the plan should not re-introduce them.
- **Run `npm run check` and `npm run fmt` from `crates/qsf_browser_server/ui/`** before committing, per `Agents.md`. The plan claims this was done; reconfirm in the commit message body.
- **Consider whether `FocalHubScene` lifecycle should live in `main.ts` at all.** Right now `main.ts` owns scene instantiation, destruction, sequence counters, and slot management. A small `canvas/sceneController.ts` that exposes `{ ensureScene, renderSelection, clear }` would keep `main.ts` thin (per the "entry points are thin wrappers" rule in `Agents.md`) and would also make Important issue #2 a one-line change rather than spread across three branches.

## Assessment

**Ready to commit?** With fixes.

**Reasoning:** None of the issues are crashes or data loss, so a commit *could* land as-is. However, two of them (#1 leak on init failure, #2 destroy-on-fetch-failure) actively undermine the lifecycle the implementation otherwise gets right, and #3 (broken-node cursor) is a UX inconsistency with the inspector. These are 30-minute fixes; folding them in before the commit produces a notably better Phase 4 milestone. The Minor items can wait.
