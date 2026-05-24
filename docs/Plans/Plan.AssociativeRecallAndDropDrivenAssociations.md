# Associative Recall And Drop-Driven Associations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make associative recall automatic in the live turn via single-hop hint expansion, move the cross-turn co-retrieval pass out of sleep into a token-budget-driven drop event plus session-end flush, and reshape sleep around a pluggable `AssociationProposer` interface.

**Architecture:** Six independently-shippable phases. Phase 1 introduces a new `ContextSourceKind::MemoryHint`, an undirected `expand_neighbors` helper, source-priority assembly, live snapshot refresh after persistence, and switches live retrieval to `KeywordTag`. Phase 2 adds console color and the drop marker. Phase 3 extracts the existing sleep cross-turn co-retrieval into the shared `memory/co_retrieval.rs` module as a pure function. Phase 4 wires the token-budget aging policy, the live-loop cross-turn pass on drop and `:quit`, `processed_ranges` in the store, and the unidirectional event flow. Phase 5 lands the `AssociationProposer` trait, wraps the LLM-candidate path and the safety-net cross-turn pass as proposers, and rewords the sleep prompt. Phase 6 lands the ideas backlog and the durable DecisionLog entry.

**Tech Stack:** Rust workspace (`crates/qsf_app`, `crates/qsf_memory`), `time::OffsetDateTime`, `serde` for persistence, `engine_logging` for runtime logging, ANSI escape codes for console color.

## Review-Pass Adjustments (2026-05-24)

Notable findings from `docs/Plans/Review.AssociativeRecallAndDropDrivenAssociations.md` applied to this plan:

- **A1** (undirected expansion for LLM-proposed edges): tradeoff acknowledged in Task 1.3; follow-up idea (`edge_source` provenance) added to Phase 6 backlog.
- **A2** (no hint-utility feedback loop): added `hint-utility decay` idea to Phase 6 backlog.
- **C1** (token estimator unspecified): Task 4.3 now pins the chars/4 estimator on per-turn verbatim content and explains why `Turn.input_tokens` is **not** used (it would over-count because each turn's value already includes all prior verbatim turns).
- **C2** (proposer conflict resolution): Task 5.1 adds `priority()` to the trait with documented merge ordering; Tasks 5.2 and 5.3 set `LlmCandidate = 100`, `SafetyNet = 30`; Task 5.3 wiring sorts before `merge_and_dedupe`.
- **C3** (`From<&RetrievedMemory>` hardcodes `Memory`): Task 1.7 carries a verification note that hints are constructed directly, not through the `From` impl.
- **C4** (active-verbatim turn identification): Task 4.3 documents the boundary as `state.summarized_turns.len()`.
- **R4** (`ProcessedRange` anchor-vs-target semantics): Task 4.1 module doc now spells out that a range marks anchor coverage; overlap-target turns are not considered processed until they appear as anchors in their own range entry.

R1, R2, R3, D1–D3 are informational only and required no plan change.

## Resolved Open Questions

Resolutions agreed up front (rationale: these decisions block plan structure). Other open questions resolved later via the suggested defaults from the design are called out inline at the relevant task.

| # | Question | Decision |
|---|---|---|
| 6 | Live snapshot refresh strategy | **Reload-on-change.** Cheap, simple, one disk read per persistence. |
| 7 | Source-priority implementation | **Source-priority comparator.** Single `assemble_context` call, sort key extended to `(source_priority, score)`. |
| 8 | Model-context-window source | **Model's documented max tokens for configured `model_id`.** Lookup table keyed by model id. |
| 1 | `QSF_SESSION_WARM_THRESHOLD` composition | **OR — whichever fires first.** Count threshold keeps doing per-turn aging; token-budget threshold can additionally fire a batch drop. |

Suggested defaults adopted from the design (call-outs in plan tasks):
- Q2 (crash recovery) → **sleep safety net only** on next sleep cycle.
- Q3 (cross-turn variant location) → **`crates/qsf_app/src/memory/co_retrieval.rs`** (alongside `generate_deltas`).
- Q4 (`MemoryHint` in diagnostics) → **included in `multi_turn_text_loop` report and trace payloads** alongside `Memory`.
- Q5 (sleep prompt rewording in DecisionLog) → **folded into the single live/sleep split entry** in Phase 6.
- Q9 (cross-turn input scope) → **only `ContextAssembly::retrieved_memory_ids()`** (matches today's sleep behavior; the same source of truth feeds same-turn co-retrieval).
- Q10 (session-end flush failure) → **log and defer to sleep safety net; never block `:quit`.**

## File Structure

### New files

- `crates/qsf_app/src/memory/hint_expansion.rs` — pure `expand_neighbors` function.
- `crates/qsf_app/src/memory/processed_ranges.rs` — `ProcessedRange` data type and helpers (idempotency ledger).
- `crates/qsf_app/src/sleep/proposer.rs` — `AssociationProposer` trait, `ProposedAssociation` struct, registry helpers.
- `crates/qsf_app/src/sleep/proposers/llm_candidate.rs` — wraps existing LLM candidate flow.
- `crates/qsf_app/src/sleep/proposers/safety_net_co_retrieval.rs` — wraps shared cross-turn pass against unprocessed ranges.
- `crates/qsf_app/src/sleep/proposers/mod.rs` — module file.
- `crates/qsf_app/src/console/styling.rs` — ANSI color helpers, TTY/`NO_COLOR`/`--no-color` detection.
- `crates/qsf_app/src/runtime/model_context_window.rs` — `model_max_tokens(model_id) -> usize` lookup table.
- `docs/Plans/Ideas.AssociationProposers.md` — backlog of future proposer strategies (Phase 6).

### Modified files

- `crates/qsf_app/src/context/context_fragment.rs` — add `ContextSourceKind::MemoryHint` and a `source_priority()` helper.
- `crates/qsf_app/src/context/context_assembler.rs` — comparator extended to `(source_priority, score)`.
- `crates/qsf_app/src/memory/co_retrieval.rs` — add `generate_cross_turn_deltas` (Phase 3); reused by live + sleep.
- `crates/qsf_app/src/memory/mod.rs` — re-export new modules.
- `crates/qsf_memory/src/store.rs` — add `processed_ranges: Vec<ProcessedRange>` to `MemoryStoreContents` (`#[serde(default)]`).
- `crates/qsf_app/src/conversation/prompt.rs` — split `retrieved_memory_block` into a two-block formatter that prints directs and hints with clear headers.
- `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` — call `expand_neighbors`, flip strategy to `KeywordTag`, reload snapshot after persist, wire token-budget trigger and session-end flush; print colored blocks and drop marker.
- `crates/qsf_app/src/experiments/text_owned_voice_loop.rs` — flip `VOICE_MEMORY_RETRIEVAL_STRATEGY` to `KeywordTag`.
- `crates/qsf_app/src/sleep/auto_promote.rs` — call the relocated `generate_cross_turn_deltas`; later, route everything through the proposer registry.
- `crates/qsf_app/src/sleep/mod.rs` — proposer registry wiring.
- `docs/EngineeringDiary.md` — one entry per code-touching phase.
- `docs/Architecture/Architecture.MemorySystem.md` — Phase 4 + Phase 6 updates.
- `docs/Architecture/Architecture.RuntimeLoop.md` — Phase 4 update.
- `docs/Architecture/Architecture.SleepPhase.md` — Phase 5 update.
- `docs/DecisionLog.md` — Phase 6 entry.

---

## Conventions Used Throughout

- **Commit style.** Mirror the existing log: short imperative subject (e.g. `feat: expand neighbors as hints during live retrieval`). One commit per task by default.
- **Test discipline.** Inline `#[cfg(test)] mod tests { use super::*; ... }` for new helpers per Agents.md. Extracted test files use explicit imports.
- **Verification at task end.** Every code task ends with `cargo test -p qsf_app` (or `-p qsf_memory` if the change is in that crate) plus the unit test added in the task. Each phase ends with the project-wide command:
  - `cargo build`
  - `cargo clippy --all-targets -- -D warnings`
  - `cargo fmt`
- **`engine_logging` context.** Every new log line includes `session_id`, turn index where applicable, and the bounded counters (counts, ranges).
- **No new constants without DRY.** Reuse `CROSS_TURN_ASSOCIATION_WINDOW = 3` from `sleep/auto_promote.rs`; that constant moves with the function in Phase 3.

---

## Phase 1 — Hint Expansion In Retrieval

Self-contained. Ends with hints appearing in prompts and a snapshot test demonstrating both blocks.

### Task 1.1: Add `ContextSourceKind::MemoryHint` and `source_priority` helper

**Files:**
- Modify: `crates/qsf_app/src/context/context_fragment.rs`

- [ ] **Step 1: Write the failing test**

Append inside the file (no existing `mod tests` in this file — add one):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_outranks_memory_hint_in_priority() {
        assert!(ContextSourceKind::Memory.source_priority()
            > ContextSourceKind::MemoryHint.source_priority());
    }

    #[test]
    fn memory_hint_serializes_in_snake_case() {
        let kind = ContextSourceKind::MemoryHint;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"memory_hint\"");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p qsf_app context::context_fragment::tests
```

Expected: FAIL — `MemoryHint` variant not found / `source_priority` not found.

- [ ] **Step 3: Add the variant and helper**

Edit `ContextSourceKind`:

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextSourceKind {
    Memory,
    MemoryHint,
    ToolObservation,
    RuntimeState,
    ProjectFrame,
}

impl ContextSourceKind {
    /// Higher priority kinds win when the assembler must choose under budget pressure.
    pub fn source_priority(&self) -> u8 {
        match self {
            ContextSourceKind::Memory => 100,
            ContextSourceKind::ToolObservation => 90,
            ContextSourceKind::RuntimeState => 80,
            ContextSourceKind::ProjectFrame => 70,
            ContextSourceKind::MemoryHint => 50,
        }
    }
}
```

Add `use serde_json;` to the test module's `Cargo.toml` dev-dependencies only if not already present — verify with:

```powershell
grep -n '^serde_json' c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/Cargo.toml
```

If absent under `[dev-dependencies]`, add `serde_json = "1"` there.

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p qsf_app context::context_fragment::tests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/context/context_fragment.rs crates/qsf_app/Cargo.toml
git commit -m "feat: add ContextSourceKind::MemoryHint with source_priority helper"
```

### Task 1.2: Source-priority comparator in `assemble_context`

**Files:**
- Modify: `crates/qsf_app/src/context/context_assembler.rs`

- [ ] **Step 1: Write the failing test**

Append to the inline `mod tests`:

```rust
    #[test]
    fn hint_cannot_evict_direct_under_budget_pressure() {
        let direct = fragment_with_kind("direct.a", 5.0, 60, ContextSourceKind::Memory);
        let hint = fragment_with_kind("hint.b", 10.0, 60, ContextSourceKind::MemoryHint);

        // Budget admits exactly one fragment of 60 tokens.
        let assembly = assemble_context(vec![hint.clone(), direct.clone()], ContextBudget::new(2, 60));

        assert_eq!(assembly.selected.len(), 1);
        assert_eq!(assembly.selected[0].fragment.fragment_id, "direct.a");
        assert_eq!(assembly.omitted.len(), 1);
        assert_eq!(assembly.omitted[0].fragment.fragment_id, "hint.b");
    }

    fn fragment_with_kind(
        id: &str,
        score: f64,
        estimated_tokens: usize,
        source_kind: ContextSourceKind,
    ) -> ContextFragment {
        ContextFragment {
            fragment_id: id.to_string(),
            source_kind,
            summary: format!("Fragment {id}"),
            tags: vec![],
            score,
            estimated_tokens,
            source_reference: "test".to_string(),
            selection_reason: "test".to_string(),
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

```powershell
cargo test -p qsf_app context::context_assembler::tests::hint_cannot_evict_direct_under_budget_pressure
```

Expected: FAIL — high-score hint wins under the current score-only sort.

- [ ] **Step 3: Extend the comparator**

Replace the sort block in `assemble_context`:

```rust
    let mut sorted = fragments;
    sorted.sort_by(|left, right| {
        right
            .source_kind
            .source_priority()
            .cmp(&left.source_kind.source_priority())
            .then_with(|| right.score.total_cmp(&left.score))
            .then_with(|| left.estimated_tokens.cmp(&right.estimated_tokens))
            .then_with(|| left.fragment_id.cmp(&right.fragment_id))
    });
```

- [ ] **Step 4: Run test to verify it passes**

```powershell
cargo test -p qsf_app context::context_assembler::tests
```

Expected: PASS (all three tests).

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/context/context_assembler.rs
git commit -m "feat: sort assembly by source_priority before score so hints cannot evict directs"
```

### Task 1.3: `expand_neighbors` pure function

**Files:**
- Create: `crates/qsf_app/src/memory/hint_expansion.rs`
- Modify: `crates/qsf_app/src/memory/mod.rs`

**Direction tradeoff (resolves review A1).** Co-retrieval edges are min/max-canonicalized at creation time, so their direction is an artifact. LLM-proposed associations preserve semantically meaningful direction (the LLM picks `from` → `to` deliberately in `build_sleep_candidate_associations`). Undirected `expand_neighbors` therefore surfaces LLM-proposed edges as hints in both directions, including the reverse the LLM did not endorse. This is accepted noise for Phase 1; the simpler retrieval semantics matter more than directional fidelity for a hint block the model can ignore. If the noise proves measurable in human testing, a follow-up adds `edge_source: CoRetrieval | LlmCandidate | ...` provenance to `Association` and gates direction by source.

- [ ] **Step 1: Write the failing test**

Create the file with the function signature declared but unimplemented; put tests inline:

```rust
//! Single-hop neighbor expansion for retrieved memories.
//! Produces hint candidates from persisted `Association` edges, undirected.

use std::collections::BTreeSet;

use crate::memory::association::Association;
use crate::memory::memory_record::MemoryRecord;

#[derive(Clone, Debug, PartialEq)]
pub struct HintCandidate {
    pub memory: MemoryRecord,
    pub via_direct_id: String,
    pub association_reason: String,
    pub weight: f64,
}

pub const MAX_HINTS_PER_TURN: usize = 8;

/// Undirected single-hop expansion. Returns up to `max_hints` unique hint candidates,
/// ordered by descending association weight, then by `via_direct_id`, then by hint memory id.
/// A memory id already present in `direct_ids` is never returned as a hint.
pub fn expand_neighbors(
    direct_ids: &[String],
    records: &[MemoryRecord],
    associations: &[Association],
    max_hints: usize,
) -> Vec<HintCandidate> {
    let direct_set: BTreeSet<&str> = direct_ids.iter().map(String::as_str).collect();
    let record_by_id: std::collections::HashMap<&str, &MemoryRecord> =
        records.iter().map(|r| (r.id.as_str(), r)).collect();

    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
    let mut candidates: Vec<HintCandidate> = Vec::new();

    for direct in direct_ids {
        for association in associations {
            let neighbor_id = if association.from_memory_id == *direct {
                association.to_memory_id.as_str()
            } else if association.to_memory_id == *direct {
                association.from_memory_id.as_str()
            } else {
                continue;
            };

            if direct_set.contains(neighbor_id) {
                continue;
            }
            if !seen.insert((direct.clone(), neighbor_id.to_string())) {
                continue;
            }
            if candidates.iter().any(|c| c.memory.id == neighbor_id) {
                continue;
            }
            let Some(memory) = record_by_id.get(neighbor_id) else {
                continue; // dangling edge; drop silently
            };

            candidates.push(HintCandidate {
                memory: (*memory).clone(),
                via_direct_id: direct.clone(),
                association_reason: association.reason.clone(),
                weight: association.weight,
            });
        }
    }

    candidates.sort_by(|left, right| {
        right
            .weight
            .total_cmp(&left.weight)
            .then_with(|| left.via_direct_id.cmp(&right.via_direct_id))
            .then_with(|| left.memory.id.cmp(&right.memory.id))
    });
    candidates.truncate(max_hints);
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::memory_record::{MemoryRecord, MemoryRecordKind};
    use time::OffsetDateTime;
    use time::format_description::well_known::Rfc3339;

    fn ts() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-24T00:00:00Z", &Rfc3339).unwrap()
    }

    fn record(id: &str) -> MemoryRecord {
        MemoryRecord::new(
            id,
            MemoryRecordKind::Observation,
            "Title",
            "Summary text.",
            vec!["topic"],
            ts(),
            0.5,
            0,
            "tests",
            10,
        )
    }

    fn edge(from: &str, to: &str, weight: f64, reason: &str) -> Association {
        Association::new(from, to, weight, reason, ts())
    }

    #[test]
    fn outgoing_edge_produces_neighbor() {
        let records = vec![record("a"), record("b")];
        let edges = vec![edge("a", "b", 0.5, "outgoing")];

        let hints = expand_neighbors(&["a".to_string()], &records, &edges, 8);

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].memory.id, "b");
        assert_eq!(hints[0].via_direct_id, "a");
    }

    #[test]
    fn incoming_edge_produces_neighbor() {
        let records = vec![record("a"), record("b")];
        let edges = vec![edge("b", "a", 0.5, "incoming")];

        let hints = expand_neighbors(&["a".to_string()], &records, &edges, 8);

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].memory.id, "b");
    }

    #[test]
    fn reciprocal_pair_yields_single_unique_hint() {
        let records = vec![record("a"), record("b")];
        let edges = vec![
            edge("a", "b", 0.4, "out"),
            edge("b", "a", 0.5, "in"),
        ];

        let hints = expand_neighbors(&["a".to_string()], &records, &edges, 8);

        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].memory.id, "b");
        // Higher-weight edge wins.
        assert!((hints[0].weight - 0.5).abs() < 1e-9);
    }

    #[test]
    fn neighbor_already_in_directs_is_skipped() {
        let records = vec![record("a"), record("b")];
        let edges = vec![edge("a", "b", 0.5, "r")];

        let hints = expand_neighbors(
            &["a".to_string(), "b".to_string()],
            &records,
            &edges,
            8,
        );

        assert!(hints.is_empty());
    }

    #[test]
    fn dangling_edge_is_dropped_silently() {
        let records = vec![record("a")]; // b is missing
        let edges = vec![edge("a", "b", 0.5, "r")];

        let hints = expand_neighbors(&["a".to_string()], &records, &edges, 8);

        assert!(hints.is_empty());
    }

    #[test]
    fn max_hints_cap_is_enforced_and_weight_ordered() {
        let records = (0..10).map(|i| record(&format!("n{i}"))).collect::<Vec<_>>();
        let edges = (0..10)
            .map(|i| edge("a", &format!("n{i}"), 0.1 * i as f64, "r"))
            .collect::<Vec<_>>();
        let mut all_records = records.clone();
        all_records.push(record("a"));

        let hints = expand_neighbors(&["a".to_string()], &all_records, &edges, 3);

        assert_eq!(hints.len(), 3);
        // Top 3 by weight: n9, n8, n7
        assert_eq!(hints[0].memory.id, "n9");
        assert_eq!(hints[1].memory.id, "n8");
        assert_eq!(hints[2].memory.id, "n7");
    }
}
```

Add to `crates/qsf_app/src/memory/mod.rs` (next to the other `pub mod` lines):

```rust
pub mod hint_expansion;
```

- [ ] **Step 2: Run tests to verify they pass**

```powershell
cargo test -p qsf_app memory::hint_expansion::tests
```

Expected: PASS (six tests). The function is implemented in the same patch as the tests because each piece of behavior is small enough that pure-function TDD here means "tests + impl in one commit"; if any sub-test fails, narrow scope before continuing.

- [ ] **Step 3: Commit**

```powershell
git add crates/qsf_app/src/memory/hint_expansion.rs crates/qsf_app/src/memory/mod.rs
git commit -m "feat: add undirected expand_neighbors helper for hint expansion"
```

### Task 1.4: Flip live retrieval strategy to `KeywordTag`

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs:45`
- Modify: `crates/qsf_app/src/experiments/text_owned_voice_loop.rs:38`

- [ ] **Step 1: Write a regression test that locks the new value**

Append to `multi_turn_text_loop.rs` inside the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn live_retrieval_uses_keyword_tag_strategy() {
        assert_eq!(
            super::SESSION_RETRIEVAL_STRATEGY,
            crate::memory::RetrievalStrategy::KeywordTag,
            "Live loop must use KeywordTag so retrieval + hint expansion stay strict single-hop",
        );
    }
```

Append to `text_owned_voice_loop.rs` inside its `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn voice_retrieval_uses_keyword_tag_strategy() {
        assert_eq!(
            super::VOICE_MEMORY_RETRIEVAL_STRATEGY,
            crate::memory::RetrievalStrategy::KeywordTag,
        );
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```powershell
cargo test -p qsf_app live_retrieval_uses_keyword_tag_strategy voice_retrieval_uses_keyword_tag_strategy
```

Expected: FAIL — current value is `AssociationWeighted`.

- [ ] **Step 3: Flip the constants**

In `multi_turn_text_loop.rs:45`:

```rust
const SESSION_RETRIEVAL_STRATEGY: RetrievalStrategy = RetrievalStrategy::KeywordTag;
```

In `text_owned_voice_loop.rs:38`:

```rust
const VOICE_MEMORY_RETRIEVAL_STRATEGY: RetrievalStrategy = RetrievalStrategy::KeywordTag;
```

- [ ] **Step 4: Run the full crate tests; some existing tests may need expectation updates**

```powershell
cargo test -p qsf_app
```

Expected: the two strategy tests PASS. If other tests fail because they assumed `AssociationWeighted`, surface those failures in the next step — DO NOT silently update them; check the test name to confirm whether the expected behavior is "strategy is whatever the live loop chose" (update fixture) or "AssociationWeighted is exercised" (those tests must stay against an explicit `AssociationWeighted` call). The `memory_and_context` experiment is the experiment surface that should keep exercising `AssociationWeighted` per design §"Direct-Retrieval Strategy When Hints Are Active".

- [ ] **Step 5: Update fixtures only where the failure is the strategy-name string**

For each test still failing, read it; if it embeds the literal string `"association-weighted"` or `RetrievalStrategy::AssociationWeighted` because it was asserting *the live loop's strategy*, update to `"keyword-tag"` / `RetrievalStrategy::KeywordTag`. If it was asserting an independent retrieval call's strategy, leave it.

- [ ] **Step 6: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs crates/qsf_app/src/experiments/text_owned_voice_loop.rs
git commit -m "feat: switch live and voice loops to KeywordTag retrieval (hint expansion owns the hop)"
```

### Task 1.5: Two-block prompt formatter

**Files:**
- Modify: `crates/qsf_app/src/conversation/prompt.rs:186-204`

- [ ] **Step 1: Write the failing test**

Append to the inline `mod tests`:

```rust
    #[test]
    fn block_renderer_emits_two_labeled_sections_when_hints_present() {
        let assembly = ContextAssembly {
            budget: ContextBudget::new(8, 1024),
            used_estimated_tokens: 40,
            omitted: vec![],
            selected: vec![
                ContextSelection {
                    fragment: ContextFragment {
                        fragment_id: "memory.foo".to_string(),
                        source_kind: ContextSourceKind::Memory,
                        summary: "Foo summary".to_string(),
                        tags: vec![],
                        score: 1.0,
                        estimated_tokens: 20,
                        source_reference: "x".to_string(),
                        selection_reason: "matched: decay".to_string(),
                    },
                    cumulative_estimated_tokens: 20,
                },
                ContextSelection {
                    fragment: ContextFragment {
                        fragment_id: "memory.baz".to_string(),
                        source_kind: ContextSourceKind::MemoryHint,
                        summary: "Baz summary".to_string(),
                        tags: vec![],
                        score: 0.4,
                        estimated_tokens: 20,
                        source_reference: "x".to_string(),
                        selection_reason: "via memory.foo: co-retrieved".to_string(),
                    },
                    cumulative_estimated_tokens: 40,
                },
            ],
        };

        let block = retrieved_memory_block(&assembly);

        assert!(block.contains("=== Memories retrieved for this turn ==="));
        assert!(block.contains("- memory.foo: Foo summary"));
        assert!(block.contains("=== Associated memories (hints — may or may not be relevant) ==="));
        assert!(block.contains("- memory.baz: Baz summary"));
        // Directs come before hints.
        let direct_pos = block.find("memory.foo").unwrap();
        let hint_pos = block.find("memory.baz").unwrap();
        assert!(direct_pos < hint_pos);
    }

    #[test]
    fn block_renderer_omits_hint_section_when_no_hints() {
        let assembly = ContextAssembly {
            budget: ContextBudget::new(8, 1024),
            used_estimated_tokens: 20,
            omitted: vec![],
            selected: vec![ContextSelection {
                fragment: ContextFragment {
                    fragment_id: "memory.foo".to_string(),
                    source_kind: ContextSourceKind::Memory,
                    summary: "Foo".to_string(),
                    tags: vec![],
                    score: 1.0,
                    estimated_tokens: 20,
                    source_reference: "x".to_string(),
                    selection_reason: "matched".to_string(),
                },
                cumulative_estimated_tokens: 20,
            }],
        };

        let block = retrieved_memory_block(&assembly);

        assert!(block.contains("=== Memories retrieved for this turn ==="));
        assert!(!block.contains("=== Associated memories"));
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```powershell
cargo test -p qsf_app conversation::prompt::tests::block_renderer_emits_two_labeled_sections_when_hints_present
```

Expected: FAIL — current `retrieved_memory_block` returns only a flat list with no headers.

- [ ] **Step 3: Replace `retrieved_memory_block`**

Replace the function body at `crates/qsf_app/src/conversation/prompt.rs:186-204` with:

```rust
pub fn retrieved_memory_block(assembly: &ContextAssembly) -> String {
    let mut directs: Vec<String> = Vec::new();
    let mut hints: Vec<String> = Vec::new();

    for selection in &assembly.selected {
        let line = format!(
            "- {}: {}",
            selection.fragment.fragment_id, selection.fragment.summary
        );
        match selection.fragment.source_kind {
            crate::context::ContextSourceKind::Memory => directs.push(line),
            crate::context::ContextSourceKind::MemoryHint => hints.push(line),
            _ => {}
        }
    }

    let mut out = String::new();
    if !directs.is_empty() {
        out.push_str("=== Memories retrieved for this turn ===\n");
        out.push_str(&directs.join("\n"));
    }
    if !hints.is_empty() {
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("=== Associated memories (hints — may or may not be relevant) ===\n");
        out.push_str(&hints.join("\n"));
    }
    out
}
```

- [ ] **Step 4: Run prompt tests**

```powershell
cargo test -p qsf_app conversation::prompt::tests
```

Expected: PASS. Existing tests that did not have hints still pass because the directs-only output structure is `=== ... ===\n- ...` which previously was `- ...`; if any test pinned the old exact bytes, update its expected string to include the header. The cache-stability tests should still pass because the byte change is uniform within a turn (no message-prefix instability).

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/conversation/prompt.rs
git commit -m "feat: render directs and hints as two labeled prompt blocks"
```

### Task 1.6: Reload-on-change snapshot refresh after `apply_live_memory_reinforcement`

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` (signature of `apply_live_memory_reinforcement`, call site at line 571)

Read this task in full before starting; the change is mechanical but threads `memory_snapshot` through.

- [ ] **Step 1: Inspect the current signatures**

```powershell
grep -n "fn apply_live_memory_reinforcement\|load_session_memory_snapshot\|SessionMemorySourceSnapshot" c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/experiments/multi_turn_text_loop.rs | head -20
```

Note the type used for `memory_snapshot` (look for `SessionMemorySourceSnapshot` or similar) and the function signature of `load_session_memory_snapshot`.

- [ ] **Step 2: Write a failing test**

Append to the inline `mod tests`. The test targets the refresh helper directly — driving `run_one_turn` end-to-end here adds coupling without proving anything the helper test does not:

```rust
    #[test]
    fn reload_snapshot_picks_up_freshly_persisted_associations() {
        use crate::memory::{Association, MemoryStore};
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let store_path = dir.path().join("memory-store.json");

        // Persist a store with no associations.
        let mut store = MemoryStore::load_or_empty(&store_path).unwrap();
        store.persist().unwrap();

        // Take a "stale" snapshot (load_session_memory_snapshot reads disk).
        // After we mutate disk, reload_session_memory_source_snapshot must reflect it.
        let mut store2 = MemoryStore::load_or_empty(&store_path).unwrap();
        store2
            .contents_mut()
            .associations
            .push(Association::new("a", "b", 0.5, "r", time::OffsetDateTime::now_utc()));
        store2.persist().unwrap();

        let refreshed = super::reload_session_memory_source_snapshot(&store_path).unwrap();
        assert_eq!(refreshed.associations.len(), 1);
    }
```

This test will fail because `reload_session_memory_source_snapshot` does not exist yet.

- [ ] **Step 3: Run the failing test**

```powershell
cargo test -p qsf_app reload_snapshot_picks_up_freshly_persisted_associations
```

Expected: FAIL — function not found.

- [ ] **Step 4: Implement the refresh helper and wire it in**

Add to `multi_turn_text_loop.rs` near `load_session_memory_snapshot`:

```rust
/// Reload-on-change snapshot refresh. Called after every persistence event
/// that may have introduced or strengthened associations: same-turn
/// reinforcement, live cross-turn drop, session-end flush.
pub(crate) fn reload_session_memory_source_snapshot(
    memory_store_path: &Path,
) -> anyhow::Result<SessionMemorySourceSnapshot> {
    // Use the same load path that the initial boot uses. If
    // load_session_memory_snapshot has a richer signature, route through it.
    if !memory_store_path.exists() {
        return Ok(SessionMemorySourceSnapshot::default());
    }
    let store = crate::memory::MemoryStore::load_or_empty(memory_store_path)?;
    Ok(SessionMemorySourceSnapshot {
        records: store.contents().records.clone(),
        associations: store.contents().associations.clone(),
        ..SessionMemorySourceSnapshot::default()
    })
}
```

If `SessionMemorySourceSnapshot` does not have a `Default` impl or the fields differ, read the type first and adapt. If the field shape requires additional metadata (e.g. provenance), reuse whatever `load_session_memory_snapshot` constructs — extract its tail logic into a `snapshot_from_store_contents` helper that both call sites share. The acceptance criterion is single-source-of-truth construction, not duplicated build code.

Change `run_one_turn` signature so `memory_snapshot` is a `&mut`. At the original line 571:

```rust
    apply_live_memory_reinforcement(context, state, state_dir, &retrieval)?;
    let store_path = state_dir.join("memory-store.json");
    *memory_snapshot = reload_session_memory_source_snapshot(&store_path)?;
```

Update the call site in the main run loop (around line 340) to pass `&mut memory_snapshot` and adjust `let memory_snapshot = …` to `let mut memory_snapshot = …`.

- [ ] **Step 5: Run the unit test**

```powershell
cargo test -p qsf_app reload_snapshot_picks_up_freshly_persisted_associations
```

Expected: PASS.

- [ ] **Step 6: Run the full crate**

```powershell
cargo test -p qsf_app
```

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat: reload memory snapshot after live persistence so hints see new edges"
```

### Task 1.7: Wire `expand_neighbors` into the live retrieval path

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` — `run_one_turn` around line 414-419

**Verification note (resolves review C3).** The existing `From<&RetrievedMemory> for ContextFragment` impl in [context_fragment.rs:26-67](../../crates/qsf_app/src/context/context_fragment.rs#L26-L67) hardcodes `source_kind: ContextSourceKind::Memory` and cannot produce hint fragments. Hints are constructed directly with the `MemoryHint` source kind in the new code below; do NOT extend the `From` impl, and do NOT route hint candidates through it. Confirm during code review that no hint candidate flows through that path.

- [ ] **Step 1: Write a failing prompt-snapshot test**

Add to the inline `mod tests` in `multi_turn_text_loop.rs`:

```rust
    #[test]
    fn run_one_turn_emits_memory_hints_when_associations_exist() {
        // Drive run_one_turn with:
        //   - Two records "memory.foo" (matches user query) and "memory.baz"
        //     (does not match) plus a persisted Association(foo, baz).
        //   - A fake ModelClient that returns deterministic text.
        // Assert the turn's context_assembly contains at least one MemoryHint
        // selection referencing "memory.baz".
        // Use the existing in-crate test scaffolding for fake clients and
        // temp memory stores; copy from neighboring tests in this module if
        // present.
        // (Implementation must produce a concrete, runnable test — find a
        // similar existing test that exercises run_one_turn and clone its
        // setup. Look for: `cargo test -p qsf_app run_one_turn` to enumerate.)
        let scaffolded = build_run_one_turn_fixture_with_hint();
        let result = scaffolded.run();
        let hint_ids: Vec<String> = result
            .turn
            .context_assembly
            .selected
            .iter()
            .filter(|s| s.fragment.source_kind
                == crate::context::ContextSourceKind::MemoryHint)
            .map(|s| s.fragment.fragment_id.clone())
            .collect();
        assert!(hint_ids.contains(&"memory.baz".to_string()),
                "expected memory.baz as a hint, got: {hint_ids:?}");
    }
```

Discover the existing test scaffolding pattern before writing `build_run_one_turn_fixture_with_hint`. If it does not exist, lift the setup directly from an existing test that calls `run_one_turn` and inline it into the test body — DRY is preferred but a single inlined fixture is fine here if it's the first multi-step harness.

- [ ] **Step 2: Run the test to verify it fails**

```powershell
cargo test -p qsf_app run_one_turn_emits_memory_hints_when_associations_exist
```

Expected: FAIL — no hints in assembly yet.

- [ ] **Step 3: Insert hint expansion in `run_one_turn`**

After the existing `let fragments = retrieval...collect()` and before `assemble_context`, add:

```rust
    use crate::memory::hint_expansion::{expand_neighbors, MAX_HINTS_PER_TURN};
    use crate::context::ContextSourceKind;

    let direct_ids: Vec<String> = retrieval
        .selected
        .iter()
        .map(|memory| memory.memory.id.clone())
        .collect();

    let hint_candidates = expand_neighbors(
        &direct_ids,
        &memory_snapshot.records,
        &memory_snapshot.associations,
        MAX_HINTS_PER_TURN,
    );

    let mut all_fragments = fragments;
    for hint in &hint_candidates {
        all_fragments.push(ContextFragment {
            fragment_id: hint.memory.id.clone(),
            source_kind: ContextSourceKind::MemoryHint,
            summary: hint.memory.summary.clone(),
            tags: hint.memory.tags.clone(),
            score: hint.weight,
            estimated_tokens: hint.memory.estimated_tokens,
            source_reference: hint.memory.source_reference.clone(),
            selection_reason: format!(
                "via {} — {}",
                hint.via_direct_id, hint.association_reason
            ),
        });
    }
```

Then replace the `fragments` argument to `assemble_context` with `all_fragments`.

- [ ] **Step 4: Run the test**

```powershell
cargo test -p qsf_app run_one_turn_emits_memory_hints_when_associations_exist
```

Expected: PASS.

- [ ] **Step 5: Run the full crate**

```powershell
cargo test -p qsf_app
```

Expected: PASS. Any test that snapshotted prompts without hints continues to pass because hint expansion produces zero hints in stores that have no edges.

- [ ] **Step 6: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat: expand neighbors as hints during live retrieval"
```

### Task 1.8: Phase 1 verification and diary entry

**Files:**
- Modify: `docs/EngineeringDiary.md` (read instructions in the header before editing)

- [ ] **Step 1: Run the full project verification**

```powershell
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo test
```

Expected: all green.

- [ ] **Step 2: Read the EngineeringDiary preamble**

```powershell
head -30 c:/Users/larsp/src/qualia-signal-foundry/docs/EngineeringDiary.md
```

- [ ] **Step 3: Append a Phase 1 diary entry**

Follow the format and instructions in the diary header (date, scope, what changed, why, follow-ups). Body content:
- Hints now appear in live turns via the new `ContextSourceKind::MemoryHint`.
- Source-priority comparator protects directs under budget pressure.
- Live and voice loops use `KeywordTag`; `AssociationWeighted` is retained in code for the `memory_and_context` experiment.
- Snapshot reload-on-change closes the in-process drift gap.

- [ ] **Step 4: Commit**

```powershell
git add docs/EngineeringDiary.md
git commit -m "docs: log Phase 1 (hint expansion, source-priority, snapshot refresh)"
```

- [ ] **Step 5: Human testing**

Run a multi-turn session against a fixture with at least one persisted association. Confirm the two prompt blocks appear and that hints reflect existing edges. Report findings in the diary follow-up.

---

## Phase 2 — Console Color And Drop Marker

Ships independently. The drop marker prints with zero counts at this point (Phase 4 wires real drops); the line proves the rendering path is in place.

### Task 2.1: TTY/`NO_COLOR`/`--no-color` detection helper

**Files:**
- Create: `crates/qsf_app/src/console/styling.rs`
- Modify: `crates/qsf_app/src/console/mod.rs` (create if absent)

- [ ] **Step 1: Confirm the `console` module status**

```powershell
ls c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/console 2>$null
```

If the directory does not exist, create `crates/qsf_app/src/console/mod.rs` with `pub mod styling;` and register it from `lib.rs`/`mod.rs`.

- [ ] **Step 2: Write the failing test**

Create the file:

```rust
//! ANSI styling helpers. Emits escape codes only when output is a TTY and
//! the user hasn't disabled color via NO_COLOR or --no-color.

use std::io::IsTerminal;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorMode {
    Enabled,
    Disabled,
}

impl ColorMode {
    pub fn detect(stdout_is_tty: bool, no_color_env: Option<&str>, no_color_flag: bool) -> Self {
        if no_color_flag || no_color_env.is_some() || !stdout_is_tty {
            Self::Disabled
        } else {
            Self::Enabled
        }
    }

    pub fn for_stdout() -> Self {
        let stdout_is_tty = std::io::stdout().is_terminal();
        let no_color_env = std::env::var("NO_COLOR").ok();
        // The --no-color CLI flag is the experiment runner's concern; this helper
        // only consults env + tty. Call sites that parse CLI args should compose
        // by calling detect(...) directly with no_color_flag = parsed value.
        Self::detect(stdout_is_tty, no_color_env.as_deref(), false)
    }

    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Style {
    pub fg_256: Option<u8>,
    pub dim: bool,
    pub italic: bool,
}

impl Style {
    pub const fn fg(code: u8) -> Self {
        Self { fg_256: Some(code), dim: false, italic: false }
    }

    pub const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
}

pub const STYLE_DIRECT_HEADER: Style = Style::fg(44).dim();      // cyan, dim
pub const STYLE_DIRECT_BODY: Style = Style::fg(51);              // cyan, bright
pub const STYLE_HINT_BLOCK: Style = Style::fg(214).dim();        // amber, dim
pub const STYLE_DROP_MARKER: Style = Style::fg(240).dim().italic(); // gray, dim italic
pub const STYLE_ERROR: Style = Style::fg(196);                   // red, bright

pub fn paint(mode: ColorMode, style: Style, text: &str) -> String {
    if !mode.is_enabled() {
        return text.to_string();
    }

    let mut prefix = String::new();
    if style.dim {
        prefix.push_str("\x1b[2m");
    }
    if style.italic {
        prefix.push_str("\x1b[3m");
    }
    if let Some(code) = style.fg_256 {
        prefix.push_str(&format!("\x1b[38;5;{code}m"));
    }
    if prefix.is_empty() {
        return text.to_string();
    }
    format!("{prefix}{text}\x1b[0m")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_disabled_when_no_color_env_set() {
        assert_eq!(
            ColorMode::detect(true, Some(""), false),
            ColorMode::Disabled
        );
    }

    #[test]
    fn detect_disabled_when_flag_set() {
        assert_eq!(
            ColorMode::detect(true, None, true),
            ColorMode::Disabled
        );
    }

    #[test]
    fn detect_disabled_when_not_tty() {
        assert_eq!(
            ColorMode::detect(false, None, false),
            ColorMode::Disabled
        );
    }

    #[test]
    fn detect_enabled_when_tty_and_no_flags() {
        assert_eq!(
            ColorMode::detect(true, None, false),
            ColorMode::Enabled
        );
    }

    #[test]
    fn paint_passes_through_when_disabled() {
        assert_eq!(
            paint(ColorMode::Disabled, STYLE_DIRECT_HEADER, "hello"),
            "hello"
        );
    }

    #[test]
    fn paint_emits_escape_codes_when_enabled() {
        let out = paint(ColorMode::Enabled, STYLE_DIRECT_HEADER, "hello");
        assert!(out.starts_with("\x1b[2m") || out.starts_with("\x1b["));
        assert!(out.contains("hello"));
        assert!(out.ends_with("\x1b[0m"));
    }
}
```

- [ ] **Step 3: Run the tests**

```powershell
cargo test -p qsf_app console::styling::tests
```

Expected: PASS (six tests).

- [ ] **Step 4: Commit**

```powershell
git add crates/qsf_app/src/console/styling.rs crates/qsf_app/src/console/mod.rs crates/qsf_app/src/lib.rs
git commit -m "feat: add ANSI styling helper with TTY/NO_COLOR/--no-color detection"
```

### Task 2.2: Print colored memory blocks before the model response

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` — `run_one_turn`, just before `writeln!(output, "{response}")`

- [ ] **Step 1: Decide where the writer lives**

The `run_one_turn` function returns the response string and the caller writes it. Move the per-turn console printing into `run_one_turn`, taking the writer as a parameter, OR have the caller print both blocks and the response. To minimize signature churn, print from the caller — the response printing is already there, so add block printing immediately before.

- [ ] **Step 2: Write the failing test**

This is hard to unit test against terminal output without color-stripping. The acceptable test is a non-TTY golden: write to a `Vec<u8>` buffer and assert that `NO_COLOR` mode produces the plain block text exactly, while a forced-`Enabled` mode produces escape codes around the headers.

Add to the inline `mod tests` of `multi_turn_text_loop.rs`:

```rust
    #[test]
    fn print_memory_blocks_no_color_mode_emits_plain_headers() {
        use crate::console::styling::ColorMode;
        let assembly = small_assembly_with_one_direct_one_hint();
        let mut buf: Vec<u8> = Vec::new();

        super::print_memory_blocks(&mut buf, &assembly, ColorMode::Disabled).unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("=== Memories retrieved for this turn ==="));
        assert!(text.contains("=== Associated memories (hints — may or may not be relevant) ==="));
        // No ANSI escape codes.
        assert!(!text.contains("\x1b["));
    }

    #[test]
    fn print_memory_blocks_enabled_mode_wraps_headers_in_escapes() {
        use crate::console::styling::ColorMode;
        let assembly = small_assembly_with_one_direct_one_hint();
        let mut buf: Vec<u8> = Vec::new();

        super::print_memory_blocks(&mut buf, &assembly, ColorMode::Enabled).unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("\x1b["), "expected ANSI escape codes");
        assert!(text.ends_with("\x1b[0m\n") || text.contains("\x1b[0m"));
    }
```

Define `small_assembly_with_one_direct_one_hint()` inline in the test module — reuse the structure from the prompt-formatter test.

- [ ] **Step 3: Run the test to verify it fails**

```powershell
cargo test -p qsf_app print_memory_blocks
```

Expected: FAIL — function missing.

- [ ] **Step 4: Implement `print_memory_blocks`**

Add to `multi_turn_text_loop.rs`:

```rust
fn print_memory_blocks<W: std::io::Write>(
    output: &mut W,
    assembly: &ContextAssembly,
    color_mode: crate::console::styling::ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{
        paint, STYLE_DIRECT_BODY, STYLE_DIRECT_HEADER, STYLE_HINT_BLOCK,
    };

    let mut directs: Vec<String> = Vec::new();
    let mut hints: Vec<String> = Vec::new();

    for selection in &assembly.selected {
        let line = format!(
            "- {}: {}",
            selection.fragment.fragment_id, selection.fragment.summary
        );
        match selection.fragment.source_kind {
            crate::context::ContextSourceKind::Memory => directs.push(line),
            crate::context::ContextSourceKind::MemoryHint => hints.push(line),
            _ => {}
        }
    }

    if !directs.is_empty() {
        writeln!(
            output,
            "{}",
            paint(color_mode, STYLE_DIRECT_HEADER,
                  "=== Memories retrieved for this turn ===")
        )?;
        for line in &directs {
            writeln!(output, "{}", paint(color_mode, STYLE_DIRECT_BODY, line))?;
        }
    }

    if !hints.is_empty() {
        writeln!(
            output,
            "{}",
            paint(color_mode, STYLE_HINT_BLOCK,
                  "=== Associated memories (hints — may or may not be relevant) ===")
        )?;
        for line in &hints {
            writeln!(output, "{}", paint(color_mode, STYLE_HINT_BLOCK, line))?;
        }
    }

    Ok(())
}
```

Call it in the main loop just before `writeln!(output, "{response}")`. Resolve `color_mode` once near session boot via `ColorMode::for_stdout()` and pass it through.

- [ ] **Step 5: Run tests**

```powershell
cargo test -p qsf_app
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat: print colored memory blocks to console before the model response"
```

### Task 2.3: Drop-event marker line (zero counts at this phase)

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn print_drop_marker_renders_expected_format() {
        use crate::console::styling::ColorMode;
        let mut buf: Vec<u8> = Vec::new();

        super::print_drop_marker(&mut buf, 3, 2, 5, ColorMode::Disabled).unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("aged 3 turns from prompt"));
        assert!(text.contains("+2 associations"));
        assert!(text.contains("*5 strengthened"));
    }

    #[test]
    fn print_session_end_flush_marker_renders_expected_format() {
        use crate::console::styling::ColorMode;
        let mut buf: Vec<u8> = Vec::new();

        super::print_session_end_flush(&mut buf, 4, 1, ColorMode::Disabled).unwrap();

        let text = String::from_utf8(buf).unwrap();
        assert!(text.contains("session-end flush"));
        assert!(text.contains("+4 associations"));
    }
```

- [ ] **Step 2: Run to verify failure**

```powershell
cargo test -p qsf_app print_drop_marker_renders_expected_format
```

Expected: FAIL.

- [ ] **Step 3: Implement the marker writers**

```rust
fn print_drop_marker<W: std::io::Write>(
    output: &mut W,
    aged_turn_count: usize,
    new_associations: usize,
    strengthened: usize,
    color_mode: crate::console::styling::ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{paint, STYLE_DROP_MARKER};
    let line = format!(
        "--- aged {} turns from prompt; +{} associations, *{} strengthened ---",
        aged_turn_count, new_associations, strengthened
    );
    writeln!(output, "{}", paint(color_mode, STYLE_DROP_MARKER, &line))
}

fn print_session_end_flush<W: std::io::Write>(
    output: &mut W,
    new_associations: usize,
    strengthened: usize,
    color_mode: crate::console::styling::ColorMode,
) -> std::io::Result<()> {
    use crate::console::styling::{paint, STYLE_DROP_MARKER};
    let line = format!(
        "--- session-end flush; +{} associations, *{} strengthened ---",
        new_associations, strengthened
    );
    writeln!(output, "{}", paint(color_mode, STYLE_DROP_MARKER, &line))
}
```

Insert a stub call (zero counts) at end of `run_one_turn`, gated by a flag we'll wire to the actual trigger in Phase 4. To prove the wiring without changing live behavior, call it only when `std::env::var("QSF_DROP_MARKER_DEBUG").is_ok()` so the marker appears in human testing if you opt in.

- [ ] **Step 4: Run tests**

```powershell
cargo test -p qsf_app print_drop_marker_renders_expected_format print_session_end_flush_marker_renders_expected_format
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat: add drop and session-end flush marker writers"
```

### Task 2.4: Phase 2 verification and diary entry

- [ ] **Step 1: Run project verification**

```powershell
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo test
```

- [ ] **Step 2: Append a Phase 2 diary entry**

Content:
- Console color helper landed under `crates/qsf_app/src/console/styling.rs`.
- Memory blocks render in cyan (directs) and amber (hints); drop marker in gray.
- TTY/`NO_COLOR`/`--no-color` disables color.

- [ ] **Step 3: Commit**

```powershell
git add docs/EngineeringDiary.md
git commit -m "docs: log Phase 2 (console color + drop marker stub)"
```

- [ ] **Step 4: Human testing**

Run a session in a real terminal (Windows Terminal and a Linux terminal if available). Confirm legibility on dark and light themes. Confirm `NO_COLOR=1` strips color. Report findings in the diary follow-up.

---

## Phase 3 — Extract Cross-Turn Variant Into Shared Module

No behavior change. Sleep continues to call the cross-turn pass on the whole session; only the function location changes.

### Task 3.1: Move and rename `build_cross_turn_associations`

**Files:**
- Modify: `crates/qsf_app/src/memory/co_retrieval.rs`
- Modify: `crates/qsf_app/src/sleep/auto_promote.rs` (remove function; call relocated version)

- [ ] **Step 1: Add the new public function to `co_retrieval.rs`**

Add (alongside `generate_deltas`):

```rust
use std::collections::HashSet;
use crate::memory::association::Association;

pub const CROSS_TURN_ASSOCIATION_WINDOW: usize = 3;
pub const SLEEP_ASSOCIATION_INITIAL_WEIGHT: f64 = 0.35;
pub const SLEEP_ASSOCIATION_STRENGTHEN_DELTA: f64 = 0.05;

/// Generate cross-turn co-retrieval deltas across a window.
///
/// `retrievals_per_turn[i]` is the set of memory IDs retrieved in turn `i`
/// (call-site provides — usually `ContextAssembly::retrieved_memory_ids()`).
/// `existing_associations` is the current store's association list.
/// `known_record_ids` is the set of memory IDs currently present in the
/// destination store; pairs touching missing endpoints are dropped (per
/// the 2026-05-23 *"Durable associations require present endpoints"*
/// decision).
///
/// Returns deterministically-ordered `CoRetrievalDelta` values. `Create`
/// deltas use `SLEEP_ASSOCIATION_INITIAL_WEIGHT`; `Strengthen` deltas use
/// `SLEEP_ASSOCIATION_STRENGTHEN_DELTA`.
pub fn generate_cross_turn_deltas(
    retrievals_per_turn: &[Vec<String>],
    existing_associations: &[Association],
    known_record_ids: &HashSet<String>,
    window: usize,
    session_id: &str,
    now: OffsetDateTime,
) -> Vec<CoRetrievalDelta> {
    let mut deltas: Vec<CoRetrievalDelta> = Vec::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();

    let n = retrievals_per_turn.len();
    for from_turn in 0..n {
        let last_turn = (from_turn + window).min(n.saturating_sub(1));
        for to_turn in (from_turn + 1)..=last_turn {
            for from_id in &retrievals_per_turn[from_turn] {
                for to_id in &retrievals_per_turn[to_turn] {
                    if from_id == to_id {
                        continue;
                    }
                    let (left, right) = ordered_pair(from_id, to_id);
                    if !known_record_ids.contains(&left) || !known_record_ids.contains(&right) {
                        continue;
                    }
                    if !seen.insert((left.clone(), right.clone())) {
                        continue;
                    }

                    if let Some(existing) = existing_associations.iter().find(|a| {
                        is_same_unordered_pair(&a.from_memory_id, &a.to_memory_id, &left, &right)
                    }) {
                        deltas.push(CoRetrievalDelta::Strengthen {
                            from: left,
                            to: right,
                            new_weight: (existing.weight + SLEEP_ASSOCIATION_STRENGTHEN_DELTA).min(1.0),
                            at: now,
                        });
                    } else {
                        deltas.push(CoRetrievalDelta::Create {
                            from: left,
                            to: right,
                            weight: SLEEP_ASSOCIATION_INITIAL_WEIGHT,
                            reason: format!(
                                "co-retrieved within {window} turns during session {session_id}"
                            ),
                            at: now,
                        });
                    }
                }
            }
        }
    }

    deltas.sort_by(|left, right| match (left, right) {
        (
            CoRetrievalDelta::Create { from: lf, to: lt, .. },
            CoRetrievalDelta::Create { from: rf, to: rt, .. },
        )
        | (
            CoRetrievalDelta::Strengthen { from: lf, to: lt, .. },
            CoRetrievalDelta::Strengthen { from: rf, to: rt, .. },
        ) => lf.cmp(rf).then_with(|| lt.cmp(rt)),
        _ => left
            .ordering_key()
            .cmp(&right.ordering_key()),
    });

    deltas
}

impl CoRetrievalDelta {
    fn ordering_key(&self) -> (u8, &str, &str) {
        match self {
            CoRetrievalDelta::Create { from, to, .. } => (0, from.as_str(), to.as_str()),
            CoRetrievalDelta::Strengthen { from, to, .. } => (1, from.as_str(), to.as_str()),
        }
    }
}
```

If the compiler reports `is_same_unordered_pair` is private or doesn't exist as-is, reuse it from the same file — it's already there.

- [ ] **Step 2: Port the regression tests for endpoint validation**

In `auto_promote.rs` find the existing test:

```powershell
grep -n "cross_turn_retrievals_skip_ids_missing_from_current_store\|fn build_cross_turn_associations" c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/sleep/auto_promote.rs
```

Port the test into `co_retrieval.rs` inline tests as `cross_turn_deltas_skip_missing_endpoints`:

```rust
    #[test]
    fn cross_turn_deltas_skip_missing_endpoints() {
        let retrievals = vec![
            vec!["a".to_string(), "ghost".to_string()],
            vec!["b".to_string()],
        ];
        let mut known = HashSet::new();
        known.insert("a".to_string());
        known.insert("b".to_string());

        let deltas = generate_cross_turn_deltas(
            &retrievals,
            &[],
            &known,
            CROSS_TURN_ASSOCIATION_WINDOW,
            "s",
            now(),
        );

        // Only the (a,b) pair survives; (ghost,b) and (a,ghost) drop.
        assert_eq!(deltas.len(), 1);
        match &deltas[0] {
            CoRetrievalDelta::Create { from, to, .. } => {
                assert_eq!(from, "a");
                assert_eq!(to, "b");
            }
            other => panic!("expected Create, got {other:?}"),
        }
    }
```

Add at least two more tests:

```rust
    #[test]
    fn cross_turn_deltas_strengthen_existing() {
        let retrievals = vec![vec!["a".to_string()], vec!["b".to_string()]];
        let existing = vec![Association::new("a", "b", 0.5, "prior", now())];
        let mut known = HashSet::new();
        known.insert("a".to_string());
        known.insert("b".to_string());

        let deltas = generate_cross_turn_deltas(
            &retrievals, &existing, &known, CROSS_TURN_ASSOCIATION_WINDOW, "s", now(),
        );

        assert_eq!(deltas.len(), 1);
        assert!(matches!(deltas[0], CoRetrievalDelta::Strengthen { ref from, ref to, new_weight, .. }
            if from == "a" && to == "b" && (new_weight - 0.55).abs() < 1e-9));
    }

    #[test]
    fn cross_turn_deltas_respect_window() {
        let retrievals = vec![
            vec!["a".to_string()],
            vec!["b".to_string()],
            vec!["c".to_string()],
            vec!["d".to_string()],
            vec!["e".to_string()],
        ];
        let known: HashSet<String> =
            ["a","b","c","d","e"].iter().map(|s| s.to_string()).collect();

        let deltas = generate_cross_turn_deltas(&retrievals, &[], &known, 2, "s", now());

        // Within window 2, pairs: (a,b)(a,c) (b,c)(b,d) (c,d)(c,e) (d,e)
        let creates = deltas
            .iter()
            .filter(|d| matches!(d, CoRetrievalDelta::Create { .. }))
            .count();
        assert_eq!(creates, 7);
    }
```

- [ ] **Step 3: Run the new tests**

```powershell
cargo test -p qsf_app memory::co_retrieval::tests
```

Expected: PASS.

- [ ] **Step 4: Replace the call in `auto_promote.rs::build_promotion_plan`**

Convert `build_cross_turn_associations` into a thin shim that calls `generate_cross_turn_deltas` and then applies the deltas to produce the `(Vec<Association>, Vec<(String,String,f64)>)` shape currently returned. This keeps `build_promotion_plan` unchanged:

```rust
fn build_cross_turn_associations(
    session: &SessionState,
    current_store: &MemoryStoreContents,
    as_of: OffsetDateTime,
) -> (Vec<Association>, Vec<(String, String, f64)>) {
    use crate::memory::co_retrieval::{
        generate_cross_turn_deltas, CoRetrievalDelta, CROSS_TURN_ASSOCIATION_WINDOW,
    };

    let known: std::collections::HashSet<String> = current_store
        .records
        .iter()
        .map(|r| r.id.clone())
        .collect();
    let retrievals = session
        .turns
        .iter()
        .map(|turn| turn.context_assembly.retrieved_memory_ids())
        .collect::<Vec<_>>();

    let deltas = generate_cross_turn_deltas(
        &retrievals,
        &current_store.associations,
        &known,
        CROSS_TURN_ASSOCIATION_WINDOW,
        &session.session_id,
        as_of,
    );

    let mut new_associations = Vec::new();
    let mut strengthened = Vec::new();
    for delta in deltas {
        match delta {
            CoRetrievalDelta::Create { from, to, weight, reason, at } => {
                new_associations.push(Association::new(from, to, weight, reason, at));
            }
            CoRetrievalDelta::Strengthen { from, to, new_weight, .. } => {
                strengthened.push((from, to, new_weight));
            }
        }
    }

    new_associations.sort_by(|a, b| {
        a.from_memory_id.cmp(&b.from_memory_id).then_with(|| a.to_memory_id.cmp(&b.to_memory_id))
    });
    strengthened.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    (new_associations, strengthened)
}
```

Remove the now-redundant `CROSS_TURN_ASSOCIATION_WINDOW`, `SLEEP_ASSOCIATION_INITIAL_WEIGHT`, `SLEEP_ASSOCIATION_STRENGTHEN_DELTA` constants from `auto_promote.rs` since they live in `co_retrieval.rs`. Re-export if other callers need them: `pub use crate::memory::co_retrieval::{CROSS_TURN_ASSOCIATION_WINDOW, SLEEP_ASSOCIATION_INITIAL_WEIGHT, SLEEP_ASSOCIATION_STRENGTHEN_DELTA};`.

- [ ] **Step 5: Run existing sleep tests to confirm no regression**

```powershell
cargo test -p qsf_app sleep::auto_promote
```

Expected: all existing tests PASS unchanged.

- [ ] **Step 6: Commit**

```powershell
git add crates/qsf_app/src/memory/co_retrieval.rs crates/qsf_app/src/sleep/auto_promote.rs
git commit -m "refactor: relocate cross-turn co-retrieval to memory/co_retrieval as a pure function"
```

### Task 3.2: Phase 3 verification and diary entry

- [ ] **Step 1: Project verification**

```powershell
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo test
```

- [ ] **Step 2: Diary entry**

Content: cross-turn co-retrieval moved to shared module; no behavior change for sleep; ready for the live loop to consume in Phase 4.

- [ ] **Step 3: Commit**

```powershell
git add docs/EngineeringDiary.md
git commit -m "docs: log Phase 3 (cross-turn co-retrieval extracted)"
```

---

## Phase 4 — Aging Policy, Live Cross-Turn Co-Retrieval, Session-End Flush

The largest phase. Implements the event/reducer/effect flow from the design.

### Task 4.1: Add `ProcessedRange` and persist alongside associations

**Files:**
- Create: `crates/qsf_app/src/memory/processed_ranges.rs`
- Modify: `crates/qsf_memory/src/store.rs` — `MemoryStoreContents`
- Modify: `crates/qsf_app/src/memory/mod.rs` — re-export

- [ ] **Step 1: Define the type with serde tests**

Create `crates/qsf_app/src/memory/processed_ranges.rs`:

```rust
//! Idempotency ledger for cross-turn co-retrieval coverage.
//!
//! A `ProcessedRange` records that the cross-turn pass has been run with
//! each turn in `[first_turn_index, last_turn_index]` serving as the
//! window *anchor* (from-turn). Overlap-target turns reached by the
//! window from inside the range are NOT marked processed by this entry —
//! they remain candidates for future ranges where they appear as anchors.
//! This anchor-semantics matters because the cross-turn algorithm forms
//! every pair (anchor_turn, anchor_turn + k) for k in 1..=window, so each
//! turn must serve as an anchor exactly once across the session's lifetime
//! to achieve full coverage.
//!
//! New durable associations are persisted alongside the matching
//! `ProcessedRange` in a single atomic write, so either both are durable
//! or neither is. This lets the live loop and the sleep safety-net
//! proposer skip ranges that have already been covered, making re-runs
//! no-ops even on crash recovery.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessedRangeKind {
    LiveBatch,
    SessionEnd,
    SleepSafetyNet,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProcessedRange {
    pub session_id: String,
    pub first_turn_index: usize,
    pub last_turn_index: usize,
    pub kind: ProcessedRangeKind,
    pub at: OffsetDateTime,
}

impl ProcessedRange {
    pub fn covers(&self, session_id: &str, turn_index: usize) -> bool {
        self.session_id == session_id
            && self.first_turn_index <= turn_index
            && turn_index <= self.last_turn_index
    }
}

/// Return turn indices in [start, end_inclusive] not covered by any range with
/// `kind ∈ {LiveBatch, SessionEnd, SleepSafetyNet}` for `session_id`.
pub fn uncovered_turn_indices(
    ranges: &[ProcessedRange],
    session_id: &str,
    start: usize,
    end_inclusive: usize,
) -> Vec<usize> {
    (start..=end_inclusive)
        .filter(|turn_index| {
            !ranges
                .iter()
                .any(|r| r.covers(session_id, *turn_index))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::format_description::well_known::Rfc3339;

    fn ts() -> OffsetDateTime {
        OffsetDateTime::parse("2026-05-24T00:00:00Z", &Rfc3339).unwrap()
    }

    #[test]
    fn covers_turn_inside_range() {
        let r = ProcessedRange {
            session_id: "s".into(),
            first_turn_index: 2,
            last_turn_index: 5,
            kind: ProcessedRangeKind::LiveBatch,
            at: ts(),
        };
        assert!(r.covers("s", 2));
        assert!(r.covers("s", 5));
        assert!(!r.covers("s", 1));
        assert!(!r.covers("s", 6));
        assert!(!r.covers("other", 3));
    }

    #[test]
    fn uncovered_indices_excludes_covered_turns() {
        let ranges = vec![
            ProcessedRange {
                session_id: "s".into(),
                first_turn_index: 0,
                last_turn_index: 2,
                kind: ProcessedRangeKind::LiveBatch,
                at: ts(),
            },
            ProcessedRange {
                session_id: "s".into(),
                first_turn_index: 5,
                last_turn_index: 5,
                kind: ProcessedRangeKind::SessionEnd,
                at: ts(),
            },
        ];

        assert_eq!(
            uncovered_turn_indices(&ranges, "s", 0, 6),
            vec![3, 4, 6]
        );
    }

    #[test]
    fn serde_roundtrip() {
        let r = ProcessedRange {
            session_id: "session.1".into(),
            first_turn_index: 0,
            last_turn_index: 9,
            kind: ProcessedRangeKind::SleepSafetyNet,
            at: ts(),
        };
        let json = serde_json::to_string(&r).unwrap();
        let parsed: ProcessedRange = serde_json::from_str(&json).unwrap();
        assert_eq!(r, parsed);
        assert!(json.contains("sleep_safety_net"));
    }
}
```

Add `pub mod processed_ranges;` to `crates/qsf_app/src/memory/mod.rs`.

- [ ] **Step 2: Run the tests**

```powershell
cargo test -p qsf_app memory::processed_ranges::tests
```

Expected: PASS.

- [ ] **Step 3: Add the field to `MemoryStoreContents`**

In `crates/qsf_memory/src/store.rs`, extend the struct.

First create a sibling type inside `qsf_memory` that mirrors `ProcessedRange` (avoids a `qsf_memory → qsf_app` dep — `qsf_memory` is the lower-level crate). Add to `crates/qsf_memory/src/lib.rs`:

```rust
pub mod processed_range;
pub use processed_range::{ProcessedRange, ProcessedRangeKind};
```

Create `crates/qsf_memory/src/processed_range.rs` with the same shape as the type above (serialization compatible). Then re-export from `qsf_app::memory::processed_ranges` so call sites that already imported the `qsf_app` location keep working.

```rust
// crates/qsf_memory/src/processed_range.rs
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessedRangeKind {
    LiveBatch,
    SessionEnd,
    SleepSafetyNet,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProcessedRange {
    pub session_id: String,
    pub first_turn_index: usize,
    pub last_turn_index: usize,
    pub kind: ProcessedRangeKind,
    pub at: OffsetDateTime,
}
```

Then in the existing `qsf_app::memory::processed_ranges.rs`, swap the local definitions for re-exports plus helpers:

```rust
pub use qsf_memory::{ProcessedRange, ProcessedRangeKind};

// covers() and uncovered_turn_indices() remain in qsf_app since they are
// not used by qsf_memory itself.
pub fn covers(range: &ProcessedRange, session_id: &str, turn_index: usize) -> bool {
    range.session_id == session_id
        && range.first_turn_index <= turn_index
        && turn_index <= range.last_turn_index
}
```

Update tests in `qsf_app::memory::processed_ranges` accordingly (call the free function `covers` instead of the method).

In `crates/qsf_memory/src/store.rs`, extend:

```rust
use crate::processed_range::ProcessedRange;

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct MemoryStoreContents {
    pub records: Vec<MemoryRecord>,
    pub associations: Vec<Association>,
    #[serde(default)]
    pub processed_ranges: Vec<ProcessedRange>,
}
```

- [ ] **Step 4: Write a roundtrip test**

In `crates/qsf_memory/src/store.rs::tests`, add:

```rust
    #[test]
    fn processed_ranges_roundtrip_through_persist() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        let mut store = MemoryStore::load_or_empty(&path).unwrap();
        store.contents_mut().processed_ranges.push(ProcessedRange {
            session_id: "s".into(),
            first_turn_index: 0,
            last_turn_index: 2,
            kind: crate::processed_range::ProcessedRangeKind::LiveBatch,
            at: ts(),
        });
        store.persist().unwrap();

        let reloaded = MemoryStore::load_or_empty(&path).unwrap();
        assert_eq!(reloaded.contents().processed_ranges.len(), 1);
        assert_eq!(reloaded.contents().processed_ranges[0].session_id, "s");
    }

    #[test]
    fn legacy_store_without_processed_ranges_loads_via_serde_default() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("memory-store.json");
        std::fs::write(
            &path,
            r#"{"records":[],"associations":[]}"#,
        )
        .unwrap();

        let store = MemoryStore::load_or_empty(&path).unwrap();
        assert!(store.contents().processed_ranges.is_empty());
    }
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p qsf_memory
cargo test -p qsf_app memory::processed_ranges
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/qsf_memory/src/processed_range.rs crates/qsf_memory/src/lib.rs crates/qsf_memory/src/store.rs crates/qsf_app/src/memory/processed_ranges.rs crates/qsf_app/src/memory/mod.rs
git commit -m "feat: add ProcessedRange ledger to MemoryStoreContents"
```

### Task 4.2: `model_max_tokens` lookup

**Files:**
- Create: `crates/qsf_app/src/runtime/model_context_window.rs`
- Modify: `crates/qsf_app/src/runtime/mod.rs` — register

- [ ] **Step 1: Inspect existing model id constants**

```powershell
grep -n "model_id\|DEFAULT_SESSION_MODEL\|gpt-5.4-mini" c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/experiments/multi_turn_text_loop.rs | head
```

Note the configured default model id and the names used elsewhere.

- [ ] **Step 2: Write the failing test**

Create the file:

```rust
//! Per-model documented max context window in input tokens.
//!
//! Source: vendor documentation as of plan creation date. Update when models
//! change. Callers consume this for token-budget aging thresholds.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelWindow {
    pub max_input_tokens: usize,
}

const ENTRIES: &[(&str, usize)] = &[
    ("gpt-5.4-mini", 200_000),
    ("gpt-5.4", 400_000),
    ("claude-opus-4-7", 200_000),
    ("claude-sonnet-4-6", 200_000),
    ("claude-haiku-4-5-20251001", 200_000),
];

/// Returns the documented max input-token window for `model_id`, or
/// `None` if the model is not in the table.
pub fn model_max_tokens(model_id: &str) -> Option<usize> {
    ENTRIES
        .iter()
        .find(|(id, _)| *id == model_id)
        .map(|(_, tokens)| *tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_model_returns_window() {
        assert_eq!(model_max_tokens("gpt-5.4-mini"), Some(200_000));
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(model_max_tokens("imaginary-model").is_none());
    }
}
```

Add `pub mod model_context_window;` to `crates/qsf_app/src/runtime/mod.rs`.

- [ ] **Step 3: Run tests**

```powershell
cargo test -p qsf_app runtime::model_context_window::tests
```

Expected: PASS.

- [ ] **Step 4: Commit**

```powershell
git add crates/qsf_app/src/runtime/model_context_window.rs crates/qsf_app/src/runtime/mod.rs
git commit -m "feat: add model_max_tokens lookup for aging thresholds"
```

### Task 4.3: Token-budget threshold detection (pure function)

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` (add helper)
- Modify: `crates/qsf_app/src/sleep/auto_promote.rs` (extract `estimated_tokens` to a shared location)
- Modify: `crates/qsf_app/src/memory/mod.rs` (re-export the shared helper)

**Estimator decision (resolves review C1).** The hot-context size is measured as the sum of each active turn's *own* verbatim contribution using the existing `chars().count().div_ceil(4).max(1)` heuristic from [auto_promote.rs:294-296](../../crates/qsf_app/src/sleep/auto_promote.rs#L294-L296). `Turn.input_tokens` (model-reported) is **not** suitable because it captures the entire prompt at the moment that turn was sent — it already includes all prior turns, so summing across active turns would over-count by an order of magnitude. The chars/4 estimator is additive, conservative (under-counts slightly because it ignores per-message metadata), and consistent with how memory records are sized.

**Active-turn boundary (resolves review C4).** Active verbatim turns are exactly those with index `>= state.summarized_turns.len()`. The reducer guarantees `summarized_turns` only grows, and warm summaries replace their corresponding verbatim turns during prompt assembly (see [assemble_session_prompt](../../crates/qsf_app/src/experiments/multi_turn_text_loop.rs#L1001)). The aging block chooses the oldest contiguous active turns whose removal brings the hot estimate below low-water.

- [ ] **Step 1: Extract the shared estimator**

Move `estimated_tokens` out of `auto_promote.rs` into a shared location. Create or extend a small module (`crates/qsf_app/src/memory/token_estimate.rs` is a clean home):

```rust
//! Conservative character-based token estimator shared by memory-record
//! construction and live-loop hot-context aging.

pub fn estimated_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4).max(1)
}
```

Add `pub mod token_estimate;` to `memory/mod.rs` and `pub use token_estimate::estimated_tokens;`. Replace the private copy in `auto_promote.rs` with the shared import. Run the full test suite to confirm no regression.

- [ ] **Step 2: Write the failing test**

Append to the inline `mod tests` of `multi_turn_text_loop.rs`:

```rust
    #[test]
    fn aging_range_chosen_when_hot_token_estimate_exceeds_high_water() {
        // 6 active turns whose own verbatim content sums to a known size.
        // High-water = 80% of 1000 = 800; low-water = 50% = 500.
        let state = synthetic_state_with_verbatim_sizes(&[200, 200, 200, 200, 200, 200]);
        let plan = super::plan_token_budget_drop(&state, 1000, 0.80, 0.50);

        // Hot estimate = 1200 > 800. Drop until estimate <= 500.
        // Dropping oldest 4 leaves 2 hot turns @ 400 tokens.
        assert_eq!(plan.as_ref().map(|p| p.aged_count), Some(4));
        assert_eq!(plan.as_ref().map(|p| p.first_turn_index), Some(0));
        assert_eq!(plan.as_ref().map(|p| p.last_turn_index), Some(3));
    }

    #[test]
    fn no_drop_below_high_water() {
        let state = synthetic_state_with_verbatim_sizes(&[100, 100, 100]);
        let plan = super::plan_token_budget_drop(&state, 1000, 0.80, 0.50);
        assert!(plan.is_none());
    }
```

`synthetic_state_with_verbatim_sizes(&[usize])` produces a `SessionState` where each `Turn` has verbatim text (user_input + retrieved_memory_block + assistant_response) whose summed chars satisfy `estimated_tokens(...) == requested`. Build by repeating ASCII characters: `"x".repeat(requested * 4)` gives exactly `requested` from the chars/4 estimator. Inline this helper in the test module.

- [ ] **Step 2: Run the test to verify it fails**

```powershell
cargo test -p qsf_app aging_range_chosen_when_hot_token_estimate_exceeds_high_water
```

Expected: FAIL.

- [ ] **Step 3: Implement `plan_token_budget_drop`**

Add to `multi_turn_text_loop.rs`:

```rust
use crate::memory::token_estimate::estimated_tokens;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TokenBudgetDropPlan {
    pub first_turn_index: usize,
    pub last_turn_index: usize,
    pub aged_count: usize,
    pub hot_tokens_before: usize,
    pub hot_tokens_after: usize,
}

/// Per-turn verbatim contribution. Sums the character counts of user input,
/// retrieved-memory block, and assistant response, then applies chars/4.
/// Conservative under-count (ignores per-message metadata); additive across
/// turns, which is the property `plan_token_budget_drop` needs.
fn turn_verbatim_estimated_tokens(turn: &Turn) -> usize {
    let total_chars = turn.user_input.chars().count()
        + turn.retrieved_memory_block.chars().count()
        + turn.assistant_response.chars().count();
    (total_chars / 4).max(1)
}
```

Add a test pinning the formula:

```rust
    #[test]
    fn turn_verbatim_estimated_tokens_is_chars_over_four() {
        let turn = Turn {
            // build minimal Turn with user_input = "x".repeat(40),
            // retrieved_memory_block = "y".repeat(40),
            // assistant_response = "z".repeat(80).
            // total chars = 160; estimate = 40.
            ..synthetic_turn(0)
        };
        assert_eq!(super::turn_verbatim_estimated_tokens(&turn), 40);
    }
```

Continue with the planner:

```rust
/// Pure function. Returns Some(plan) if the hot-context estimate is above
/// `high_water_fraction * model_window`, choosing the oldest contiguous block
/// of unsummarized turns whose removal brings the estimate down to
/// `low_water_fraction * model_window` or below.
///
/// Hot-context estimate uses `turn_verbatim_estimated_tokens` per turn (each
/// turn's own contribution; additive). This is intentionally NOT the
/// model-reported `turn.input_tokens`, which represents the entire prompt at
/// the moment that turn was sent and therefore over-counts when summed.
pub(crate) fn plan_token_budget_drop(
    state: &SessionState,
    model_window: usize,
    high_water_fraction: f64,
    low_water_fraction: f64,
) -> Option<TokenBudgetDropPlan> {
    // Active verbatim turns: those past the summarized boundary. The
    // reducer keeps summarized_turns monotonic.
    let active_start = state.summarized_turns.len();
    let active_turns: Vec<&Turn> = state.turns.iter().skip(active_start).collect();
    if active_turns.is_empty() {
        return None;
    }

    let per_turn: Vec<usize> = active_turns
        .iter()
        .map(|t| turn_verbatim_estimated_tokens(t))
        .collect();
    let hot_tokens_before: usize = per_turn.iter().sum();
    let high_water = (model_window as f64 * high_water_fraction) as usize;
    if hot_tokens_before <= high_water {
        return None;
    }

    let low_water = (model_window as f64 * low_water_fraction) as usize;
    let mut tokens = hot_tokens_before;
    let mut aged_count = 0;
    for size in &per_turn {
        if tokens <= low_water {
            break;
        }
        tokens = tokens.saturating_sub(*size);
        aged_count += 1;
    }

    if aged_count == 0 {
        return None;
    }

    let first_turn_index = active_start;
    let last_turn_index = active_start + aged_count - 1;

    Some(TokenBudgetDropPlan {
        first_turn_index,
        last_turn_index,
        aged_count,
        hot_tokens_before,
        hot_tokens_after: tokens,
    })
}
```

- [ ] **Step 4: Run tests**

```powershell
cargo test -p qsf_app plan_token_budget_drop
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat: add pure plan_token_budget_drop helper for batched aging"
```

### Task 4.4: `TurnsAgedAndCoRetrieved` event + reducer handling

**Files:**
- Modify: `crates/qsf_app/src/session/mod.rs` (or wherever `SessionEvent` lives — confirm with grep)
- Modify: `crates/qsf_app/src/session/reducer.rs` (or equivalent)

- [ ] **Step 1: Locate the event/reducer files**

```powershell
grep -rn "enum SessionEvent\|apply_session_event\|TurnSummarized" c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/session --include="*.rs" -l
```

- [ ] **Step 2: Write the failing test**

Add a reducer test alongside existing reducer tests (find with the grep above). Required behavior:

```rust
    #[test]
    fn turns_aged_and_co_retrieved_extends_summarized_turns_and_keeps_state_turns_append_only() {
        let mut state = state_with_turns(6);
        let turns_before = state.turns.clone();
        let summaries_before = state.summarized_turns.clone();

        let event = SessionEvent::TurnsAgedAndCoRetrieved {
            range: TurnRange { first_index: 0, last_index: 2 },
            new_associations: 3,
            strengthened_associations: 1,
            persisted_at: SystemTime::now(),
            summaries: vec![
                turn_summary(0),
                turn_summary(1),
                turn_summary(2),
            ],
        };

        apply_session_event(&mut state, event).unwrap();

        // state.turns unchanged (append-only).
        assert_eq!(state.turns, turns_before);
        // summarized_turns extended by 3.
        assert_eq!(state.summarized_turns.len(), summaries_before.len() + 3);
    }
```

Helpers `state_with_turns(n)`, `turn_summary(i)` should already exist in the reducer tests if there are similar tests; if not, write them inline.

- [ ] **Step 3: Run the test to verify it fails**

```powershell
cargo test -p qsf_app turns_aged_and_co_retrieved_extends_summarized_turns
```

Expected: FAIL — variant missing.

- [ ] **Step 4: Add the event variant and reducer arm**

In `SessionEvent`:

```rust
TurnsAgedAndCoRetrieved {
    range: TurnRange,
    new_associations: usize,
    strengthened_associations: usize,
    persisted_at: std::time::SystemTime,
    summaries: Vec<TurnSummary>,
},
```

Add a `TurnRange` struct if one does not already exist:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TurnRange {
    pub first_index: usize,
    pub last_index: usize,
}
```

In the reducer (the match in `apply_session_event`):

```rust
SessionEvent::TurnsAgedAndCoRetrieved { range, summaries, .. } => {
    debug_assert!(range.last_index >= range.first_index);
    state.summarized_turns.extend(summaries);
    // state.turns must remain append-only — DO NOT mutate.
}
```

- [ ] **Step 5: Run tests**

```powershell
cargo test -p qsf_app
```

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add crates/qsf_app/src/session
git commit -m "feat: add TurnsAgedAndCoRetrieved event and append-only reducer arm"
```

### Task 4.5: Side-effect chain — co-retrieval + persist + summarize

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

This is the largest task in the plan. Break into sub-steps and commit at each.

- [ ] **Step 1: Write the failing integration-style test**

Add inline:

```rust
    #[test]
    fn token_budget_drop_persists_associations_and_processed_range() {
        // Build a state_dir with a non-empty memory store.
        // Build a SessionState with 6 turns whose retrievals overlap so cross-turn
        // creates at least one new association.
        // Configure model_window so high-water triggers aging.
        // Call run_token_budget_drop_side_effect(...) and assert:
        //   1. New association(s) appear in the persisted store.
        //   2. A ProcessedRange with kind=LiveBatch covering [0,..] appears.
        //   3. The returned event payload has matching counts.
        let scaffold = build_drop_scaffold_six_turns_overlapping();
        let event = super::run_token_budget_drop_side_effect(&scaffold).unwrap();

        let store = crate::memory::MemoryStore::load_or_empty(&scaffold.store_path).unwrap();
        assert!(!store.contents().associations.is_empty(),
                "expected at least one new association");
        let pr = store
            .contents()
            .processed_ranges
            .iter()
            .find(|r| r.kind == qsf_memory::ProcessedRangeKind::LiveBatch);
        assert!(pr.is_some(), "expected a LiveBatch processed_range");
        assert_eq!(pr.unwrap().first_turn_index, 0);
        assert!(matches!(event, SessionEvent::TurnsAgedAndCoRetrieved { .. }));
    }
```

Implementer must supply `build_drop_scaffold_six_turns_overlapping`. Lift from existing tests that build a `SessionState` + temp memory store; if none, write inline using known constructors. Use a stub `ModelClient` for summary calls (look for an existing fake in the test module).

- [ ] **Step 2: Run the test**

```powershell
cargo test -p qsf_app token_budget_drop_persists_associations_and_processed_range
```

Expected: FAIL — function missing.

- [ ] **Step 3: Implement the side effect**

Add to `multi_turn_text_loop.rs`:

```rust
pub(crate) struct DropSideEffectInputs<'a> {
    pub state: &'a SessionState,
    pub store_path: PathBuf,
    pub plan: TokenBudgetDropPlan,
    pub overlap_window: usize,
    pub now: time::OffsetDateTime,
    pub model_client: &'a dyn ModelClient,
    pub context: &'a mut RunContext,
}

pub(crate) fn run_token_budget_drop_side_effect(
    inputs: DropSideEffectInputs<'_>,
) -> anyhow::Result<SessionEvent> {
    use crate::memory::co_retrieval::{
        generate_cross_turn_deltas, CoRetrievalDelta, CROSS_TURN_ASSOCIATION_WINDOW,
    };
    use qsf_memory::{ProcessedRange, ProcessedRangeKind};

    let DropSideEffectInputs {
        state,
        store_path,
        plan,
        overlap_window,
        now,
        model_client,
        context,
    } = inputs;

    // Aging range: [first..=last]. Overlap extends overlap_window turns
    // into still-hot turns so associations form across the boundary.
    let extended_last = (plan.last_turn_index + overlap_window).min(state.turns.len() - 1);
    let retrievals: Vec<Vec<String>> = state
        .turns
        .iter()
        .enumerate()
        .filter(|(i, _)| *i >= plan.first_turn_index && *i <= extended_last)
        .map(|(_, turn)| turn.context_assembly.retrieved_memory_ids())
        .collect();

    let mut store = crate::memory::MemoryStore::load_or_empty(&store_path)?;
    let known_record_ids: std::collections::HashSet<String> = store
        .contents()
        .records
        .iter()
        .map(|r| r.id.clone())
        .collect();

    let deltas = generate_cross_turn_deltas(
        &retrievals,
        &store.contents().associations,
        &known_record_ids,
        CROSS_TURN_ASSOCIATION_WINDOW,
        &state.session_id,
        now,
    );

    let mut new_count = 0usize;
    let mut strengthened_count = 0usize;
    for delta in deltas {
        match delta {
            CoRetrievalDelta::Create { from, to, weight, reason, at } => {
                store.contents_mut().associations.push(
                    crate::memory::Association::new(from, to, weight, reason, at),
                );
                new_count += 1;
            }
            CoRetrievalDelta::Strengthen { from, to, new_weight, at } => {
                if let Some(existing) = store
                    .contents_mut()
                    .associations
                    .iter_mut()
                    .find(|a| {
                        (a.from_memory_id == from && a.to_memory_id == to)
                            || (a.from_memory_id == to && a.to_memory_id == from)
                    })
                {
                    existing.weight = new_weight;
                    existing.last_reinforced_at = at;
                    strengthened_count += 1;
                }
            }
        }
    }

    store.contents_mut().processed_ranges.push(ProcessedRange {
        session_id: state.session_id.clone(),
        first_turn_index: plan.first_turn_index,
        last_turn_index: plan.last_turn_index,
        kind: ProcessedRangeKind::LiveBatch,
        at: now,
    });

    // Atomic single write of associations + ProcessedRange (atomic-replace
    // guaranteed by MemoryStore::persist via NamedTempFile).
    store.persist()?;

    context.record_event(
        EventType::CoRetrievalAssociationsProposed,
        json!({
            "session_id": state.session_id,
            "kind": "live_batch",
            "first_turn_index": plan.first_turn_index,
            "last_turn_index": plan.last_turn_index,
            "new_count": new_count,
            "strengthened_count": strengthened_count,
        }),
        None,
    )?;

    // Now run the existing per-turn summarizer for each turn in the range.
    let mut summaries: Vec<TurnSummary> = Vec::with_capacity(plan.aged_count);
    for index in plan.first_turn_index..=plan.last_turn_index {
        let turn = &state.turns[index];
        let summary = summarize_turn(context, state, model_client, turn)?;
        summaries.push(summary);
    }

    Ok(SessionEvent::TurnsAgedAndCoRetrieved {
        range: TurnRange {
            first_index: plan.first_turn_index,
            last_index: plan.last_turn_index,
        },
        new_associations: new_count,
        strengthened_associations: strengthened_count,
        persisted_at: std::time::SystemTime::now(),
        summaries,
    })
}
```

- [ ] **Step 4: Run the test**

```powershell
cargo test -p qsf_app token_budget_drop_persists_associations_and_processed_range
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat: add side-effect chain for token-budget batch drop"
```

### Task 4.6: Trigger the side effect at end of turn

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` — `run_one_turn` tail

- [ ] **Step 1: Write the integration test**

A black-box test driving the run loop is heavy. Acceptable smaller test: assert that after `run_one_turn` returns and the hot-token estimate exceeds the high-water threshold, the next reducer event applied is `TurnsAgedAndCoRetrieved`. Add a test that drives this directly by stubbing the side-effect chain.

Add inline test that calls the new orchestration function `maybe_run_token_budget_drop` and asserts behavior:

```rust
    #[test]
    fn maybe_run_token_budget_drop_fires_when_above_high_water() {
        let mut scaffold = build_drop_scaffold_six_turns_overlapping();
        let triggered = super::maybe_run_token_budget_drop(&mut scaffold).unwrap();
        assert!(triggered, "expected drop to fire above high water");
    }

    #[test]
    fn maybe_run_token_budget_drop_no_op_when_below_high_water() {
        let mut scaffold = build_drop_scaffold_below_high_water();
        let triggered = super::maybe_run_token_budget_drop(&mut scaffold).unwrap();
        assert!(!triggered);
    }
```

- [ ] **Step 2: Run to verify failure**

```powershell
cargo test -p qsf_app maybe_run_token_budget_drop_fires_when_above_high_water
```

Expected: FAIL.

- [ ] **Step 3: Implement and wire**

```rust
pub(crate) const HOT_HIGH_WATER_FRACTION: f64 = 0.80;
pub(crate) const HOT_LOW_WATER_FRACTION: f64 = 0.50;
pub(crate) const DROP_OVERLAP_WINDOW: usize = CROSS_TURN_ASSOCIATION_WINDOW;

pub(crate) fn maybe_run_token_budget_drop(
    scaffold: &mut DropOrchestrationInputs<'_>,
) -> anyhow::Result<bool> {
    let Some(window) = crate::runtime::model_context_window::model_max_tokens(
        &scaffold.state.config.model_id,
    ) else {
        return Ok(false);
    };

    let Some(plan) = plan_token_budget_drop(
        scaffold.state,
        window,
        HOT_HIGH_WATER_FRACTION,
        HOT_LOW_WATER_FRACTION,
    ) else {
        return Ok(false);
    };

    let event = run_token_budget_drop_side_effect(DropSideEffectInputs {
        state: scaffold.state,
        store_path: scaffold.state_dir.join("memory-store.json"),
        plan,
        overlap_window: DROP_OVERLAP_WINDOW,
        now: time::OffsetDateTime::now_utc(),
        model_client: scaffold.model_client,
        context: scaffold.context,
    })?;

    apply_session_event(scaffold.context, scaffold.state, event)?;

    // Reload-on-change snapshot refresh (Phase 1 helper).
    let snapshot = reload_session_memory_source_snapshot(
        &scaffold.state_dir.join("memory-store.json"),
    )?;
    *scaffold.memory_snapshot = snapshot;

    Ok(true)
}
```

Call from `run_one_turn` tail, replacing the current `age_out_warm_turns` call:

```rust
    // Per-turn count-based aging (existing).
    age_out_warm_turns(context, state, model_client)?;

    // Token-budget batched aging (new). OR semantics: count-based may have
    // already aged turns this loop; token-budget can additionally fire if
    // hot tokens are still above the high-water mark.
    let mut drop_inputs = DropOrchestrationInputs {
        state,
        state_dir,
        memory_snapshot: memory_snapshot_mut,
        model_client,
        context,
    };
    if maybe_run_token_budget_drop(&mut drop_inputs)? {
        // Print the drop marker (counts come from the most recent event the
        // reducer applied — see Task 4.7 for wiring to the printer).
        // For now we re-derive counts by peeking at the last event if the
        // reducer exposes it; otherwise the printer is wired in Task 4.7
        // before this commit lands as user-visible behavior.
    }
```

`DropOrchestrationInputs` is a thin struct of references. Define it next to `DropSideEffectInputs`.

The `memory_snapshot_mut` reference requires plumbing — see Task 1.6 which already made `memory_snapshot` `&mut`. Pass it through `run_one_turn`'s parameters.

- [ ] **Step 4: Run tests**

```powershell
cargo test -p qsf_app
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat: trigger token-budget batch drop at end of turn"
```

### Task 4.7: Drop marker print with real counts

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs`

- [ ] **Step 1: Capture counts and print**

Adjust `maybe_run_token_budget_drop` to return counts so the caller can print:

```rust
pub(crate) struct DropOutcome {
    pub aged_count: usize,
    pub new_associations: usize,
    pub strengthened: usize,
}

// Change return to anyhow::Result<Option<DropOutcome>>.
```

In `run_one_turn` after the call:

```rust
    if let Some(outcome) = maybe_run_token_budget_drop(&mut drop_inputs)? {
        print_drop_marker(
            output, // caller's writer; passed in via run_one_turn signature
            outcome.aged_count,
            outcome.new_associations,
            outcome.strengthened,
            color_mode,
        )?;
    }
```

Threading `output` and `color_mode` into `run_one_turn` is part of this task. Update the calling sites accordingly. Remove the previous `QSF_DROP_MARKER_DEBUG` stub gate added in Task 2.3.

- [ ] **Step 2: Update tests if writer-threading touched test fixtures**

Run:

```powershell
cargo test -p qsf_app
```

Fix breakage by passing `&mut Vec::<u8>::new()` and `ColorMode::Disabled` to test calls of `run_one_turn`.

- [ ] **Step 3: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat: print drop marker with live counts"
```

### Task 4.8: Session-end flush on clean `:quit` (and `EOF`)

**Files:**
- Modify: `crates/qsf_app/src/experiments/multi_turn_text_loop.rs` — `end_session` path

- [ ] **Step 1: Write the failing test**

```rust
    #[test]
    fn session_end_flush_covers_remaining_hot_turns() {
        // Build a state with 4 hot turns and no prior ProcessedRange.
        let mut scaffold = build_session_end_scaffold_with_hot_turns(4);
        let outcome = super::run_session_end_flush(&mut scaffold).unwrap();
        assert!(outcome.is_some());

        let store = crate::memory::MemoryStore::load_or_empty(&scaffold.store_path).unwrap();
        let session_id = &scaffold.state.session_id;
        let pr = store
            .contents()
            .processed_ranges
            .iter()
            .find(|r| r.kind == qsf_memory::ProcessedRangeKind::SessionEnd
                   && &r.session_id == session_id);
        assert!(pr.is_some(), "expected a SessionEnd processed_range");
    }

    #[test]
    fn session_end_flush_is_idempotent() {
        let mut scaffold = build_session_end_scaffold_with_hot_turns(4);
        super::run_session_end_flush(&mut scaffold).unwrap();
        let outcome2 = super::run_session_end_flush(&mut scaffold).unwrap();
        assert!(outcome2.is_none(), "second flush should be a no-op");
    }
```

- [ ] **Step 2: Run to verify failure**

```powershell
cargo test -p qsf_app session_end_flush_covers_remaining_hot_turns
```

Expected: FAIL.

- [ ] **Step 3: Implement the flush**

```rust
pub(crate) fn run_session_end_flush(
    scaffold: &mut DropOrchestrationInputs<'_>,
) -> anyhow::Result<Option<DropOutcome>> {
    use crate::memory::co_retrieval::{
        generate_cross_turn_deltas, CoRetrievalDelta, CROSS_TURN_ASSOCIATION_WINDOW,
    };
    use crate::memory::processed_ranges::uncovered_turn_indices;
    use qsf_memory::{ProcessedRange, ProcessedRangeKind};

    let store_path = scaffold.state_dir.join("memory-store.json");
    if !store_path.exists() {
        return Ok(None);
    }
    let mut store = crate::memory::MemoryStore::load_or_empty(&store_path)?;

    let active_start = scaffold.state.summarized_turns.len();
    let total = scaffold.state.turns.len();
    if total == 0 || active_start >= total {
        return Ok(None);
    }

    let uncovered = uncovered_turn_indices(
        &store.contents().processed_ranges,
        &scaffold.state.session_id,
        active_start,
        total - 1,
    );
    if uncovered.is_empty() {
        return Ok(None);
    }

    let first = *uncovered.first().unwrap();
    let last = *uncovered.last().unwrap();

    let retrievals: Vec<Vec<String>> = scaffold
        .state
        .turns
        .iter()
        .enumerate()
        .filter(|(i, _)| *i >= first && *i <= last)
        .map(|(_, t)| t.context_assembly.retrieved_memory_ids())
        .collect();

    let known: std::collections::HashSet<String> = store
        .contents()
        .records
        .iter()
        .map(|r| r.id.clone())
        .collect();
    let now = time::OffsetDateTime::now_utc();

    let deltas = generate_cross_turn_deltas(
        &retrievals,
        &store.contents().associations,
        &known,
        CROSS_TURN_ASSOCIATION_WINDOW,
        &scaffold.state.session_id,
        now,
    );

    let mut new_count = 0usize;
    let mut strengthened = 0usize;
    for delta in deltas {
        match delta {
            CoRetrievalDelta::Create { from, to, weight, reason, at } => {
                store.contents_mut().associations.push(
                    crate::memory::Association::new(from, to, weight, reason, at),
                );
                new_count += 1;
            }
            CoRetrievalDelta::Strengthen { from, to, new_weight, at } => {
                if let Some(existing) = store
                    .contents_mut()
                    .associations
                    .iter_mut()
                    .find(|a| {
                        (a.from_memory_id == from && a.to_memory_id == to)
                            || (a.from_memory_id == to && a.to_memory_id == from)
                    })
                {
                    existing.weight = new_weight;
                    existing.last_reinforced_at = at;
                    strengthened += 1;
                }
            }
        }
    }

    store.contents_mut().processed_ranges.push(ProcessedRange {
        session_id: scaffold.state.session_id.clone(),
        first_turn_index: first,
        last_turn_index: last,
        kind: ProcessedRangeKind::SessionEnd,
        at: now,
    });

    if let Err(e) = store.persist() {
        // Q10 resolution: log and defer; never block exit.
        engine_logging::engine_error!(
            "session-end flush persist failed; deferring to sleep safety net: \
             session_id={} state_dir={} new={} strengthened={} error={}",
            scaffold.state.session_id,
            scaffold.state_dir.display(),
            new_count,
            strengthened,
            e
        );
        return Ok(None);
    }

    Ok(Some(DropOutcome {
        aged_count: last + 1 - first,
        new_associations: new_count,
        strengthened,
    }))
}
```

Call `run_session_end_flush` from inside `end_session` before sleep handoff. Print the session-end marker using `print_session_end_flush` after a successful flush.

- [ ] **Step 4: Run tests**

```powershell
cargo test -p qsf_app
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/experiments/multi_turn_text_loop.rs
git commit -m "feat: run cross-turn flush on session-end (clean :quit/EOF)"
```

### Task 4.9: Phase 4 verification and architecture doc updates

**Files:**
- Modify: `docs/Architecture/Architecture.MemorySystem.md`
- Modify: `docs/Architecture/Architecture.RuntimeLoop.md`
- Modify: `docs/EngineeringDiary.md`

- [ ] **Step 1: Project verification**

```powershell
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo test
```

- [ ] **Step 2: Update `Architecture.MemorySystem.md` — Implementation Status**

Add a paragraph noting:
- Cross-turn association creation now occurs in the live loop on token-budget batch drop and on session-end flush.
- Sleep retains safety-net coverage via `SafetyNetCoRetrievalProposer` (lands in Phase 5).
- The store carries `processed_ranges: Vec<ProcessedRange>` as the idempotency ledger.

- [ ] **Step 3: Update `Architecture.RuntimeLoop.md` — Implementation Status**

Add a paragraph noting:
- Aging policy now composes the existing `QSF_SESSION_WARM_THRESHOLD` (per-turn) with the new token-budget high-water threshold (batch), OR-fashion.
- Drop emits `TurnsAgedAndCoRetrieved` event; reducer extends `summarized_turns` only; `state.turns` remains append-only.
- Session-end flush runs the same chain on clean `:quit`/EOF.

- [ ] **Step 4: Diary entry for Phase 4**

Cover: token-budget threshold, processed_ranges ledger, session-end flush, drop marker.

- [ ] **Step 5: Commit**

```powershell
git add docs/Architecture/Architecture.MemorySystem.md docs/Architecture/Architecture.RuntimeLoop.md docs/EngineeringDiary.md
git commit -m "docs: log Phase 4 architecture updates"
```

- [ ] **Step 6: Human testing**

Drive a long session via a scripted input (the design says: "force a long session via a script"). Confirm:
- Drop fires at the 80% threshold.
- Console marker prints with non-zero counts.
- `:quit` produces the session-end flush marker.
- `recall_turn` still works on aged turns; reports still see them.

Record observations in the diary follow-up.

---

## Phase 5 — Proposer Interface And Sleep Prompt Rewording

### Task 5.1: `AssociationProposer` trait and `ProposedAssociation`

**Files:**
- Create: `crates/qsf_app/src/sleep/proposer.rs`
- Modify: `crates/qsf_app/src/sleep/mod.rs`

- [ ] **Step 1: Write the contract test first**

Create the file with stub trait and a doc test:

```rust
//! Sleep-time pluggable association proposer interface.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::memory::association::Association;
use crate::memory::store::MemoryStoreContents;
use crate::session::SessionState;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProposedAssociation {
    pub from_id: String,
    pub to_id: String,
    pub weight: f64,
    pub reason: String,
    pub proposer_name: String,
}

pub trait AssociationProposer {
    fn name(&self) -> &str;

    /// Higher priority wins ties when two proposers propose the same
    /// unordered endpoint pair. Defaults to 50. The shipped proposers use:
    /// LlmCandidateProposer = 100 (LLM has semantic intent),
    /// SafetyNetCoRetrievalProposer = 30 (mechanical fallback).
    fn priority(&self) -> u8 { 50 }

    fn propose(
        &self,
        store: &MemoryStoreContents,
        session: &SessionState,
        as_of: OffsetDateTime,
    ) -> Vec<ProposedAssociation>;
}

/// Merge proposed associations across multiple proposers, dedupe by
/// unordered endpoint pair, and drop any pair where either endpoint is not
/// in `known_record_ids` (per the 2026-05-23 endpoint-validation decision).
///
/// Conflict resolution: callers must pass `proposals` already sorted by
/// proposer priority descending — when two proposals collide on the same
/// unordered pair, the first one in the slice wins. The helper
/// `sort_by_priority_descending` below does this; production callers chain
/// `sort_by_priority_descending(...)` immediately before `merge_and_dedupe`.
pub fn merge_and_dedupe(
    proposals: Vec<ProposedAssociation>,
    existing: &[Association],
    known_record_ids: &std::collections::HashSet<String>,
) -> Vec<ProposedAssociation> {
    let mut seen: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    for a in existing {
        seen.insert(ordered_pair(&a.from_memory_id, &a.to_memory_id));
    }

    let mut out = Vec::new();
    for p in proposals {
        if !known_record_ids.contains(&p.from_id) || !known_record_ids.contains(&p.to_id) {
            continue;
        }
        if p.from_id == p.to_id {
            continue;
        }
        let key = ordered_pair(&p.from_id, &p.to_id);
        if seen.insert(key) {
            out.push(p);
        }
    }
    out
}

/// Stable sort by proposer priority descending. The caller annotates each
/// `ProposedAssociation` with `proposer_name`; the priority is supplied
/// out-of-band by the caller (typically constructed via
/// `proposer.priority()`) since `ProposedAssociation` does not carry it.
pub fn sort_by_priority_descending(
    proposals: &mut [(u8, ProposedAssociation)],
) {
    proposals.sort_by(|left, right| right.0.cmp(&left.0));
}

fn ordered_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn dedupe_drops_pair_existing_in_store() {
        let known: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let existing = vec![Association::new(
            "a",
            "b",
            0.5,
            "r",
            time::OffsetDateTime::UNIX_EPOCH,
        )];
        let proposals = vec![ProposedAssociation {
            from_id: "a".into(),
            to_id: "b".into(),
            weight: 0.4,
            reason: "p".into(),
            proposer_name: "x".into(),
        }];

        let merged = merge_and_dedupe(proposals, &existing, &known);

        assert!(merged.is_empty());
    }

    #[test]
    fn dedupe_drops_missing_endpoints() {
        let known: HashSet<String> = ["a"].iter().map(|s| s.to_string()).collect();
        let proposals = vec![ProposedAssociation {
            from_id: "a".into(),
            to_id: "ghost".into(),
            weight: 0.4,
            reason: "p".into(),
            proposer_name: "x".into(),
        }];

        let merged = merge_and_dedupe(proposals, &[], &known);

        assert!(merged.is_empty());
    }

    #[test]
    fn dedupe_dedupes_across_proposers() {
        let known: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let proposals = vec![
            ProposedAssociation {
                from_id: "a".into(), to_id: "b".into(),
                weight: 0.4, reason: "p1".into(), proposer_name: "x".into(),
            },
            ProposedAssociation {
                from_id: "b".into(), to_id: "a".into(),
                weight: 0.5, reason: "p2".into(), proposer_name: "y".into(),
            },
        ];

        let merged = merge_and_dedupe(proposals, &[], &known);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].from_id, "a");
        assert_eq!(merged[0].to_id, "b");
    }

    #[test]
    fn priority_sort_keeps_high_priority_proposer_on_collision() {
        let known: HashSet<String> = ["a", "b"].iter().map(|s| s.to_string()).collect();
        let mut tagged: Vec<(u8, ProposedAssociation)> = vec![
            (30, ProposedAssociation {
                from_id: "a".into(), to_id: "b".into(),
                weight: 0.3, reason: "low".into(),
                proposer_name: "safety-net".into(),
            }),
            (100, ProposedAssociation {
                from_id: "a".into(), to_id: "b".into(),
                weight: 0.4, reason: "high".into(),
                proposer_name: "llm".into(),
            }),
        ];
        sort_by_priority_descending(&mut tagged);

        let proposals: Vec<ProposedAssociation> =
            tagged.into_iter().map(|(_, p)| p).collect();
        let merged = merge_and_dedupe(proposals, &[], &known);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].proposer_name, "llm",
                   "high-priority proposer must win the collision");
    }
}
```

Add `pub mod proposer;` to `crates/qsf_app/src/sleep/mod.rs`.

- [ ] **Step 2: Run tests**

```powershell
cargo test -p qsf_app sleep::proposer::tests
```

Expected: PASS.

- [ ] **Step 3: Commit**

```powershell
git add crates/qsf_app/src/sleep/proposer.rs crates/qsf_app/src/sleep/mod.rs
git commit -m "feat: add AssociationProposer trait and dedupe helper"
```

### Task 5.2: `LlmCandidateProposer`

**Files:**
- Create: `crates/qsf_app/src/sleep/proposers/mod.rs`
- Create: `crates/qsf_app/src/sleep/proposers/llm_candidate.rs`
- Modify: `crates/qsf_app/src/sleep/auto_promote.rs` — remove direct LLM-candidate flow; replace with proposer call

- [ ] **Step 1: Inspect existing call site**

```powershell
grep -n "build_sleep_candidate_associations\|association_candidates" c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/sleep/auto_promote.rs | head
```

- [ ] **Step 2: Write a test**

First inspect the actual constructors for `SleepReport`, `SessionState`, and the `association_candidates` shape. They will determine the exact fixture wiring:

```powershell
grep -n "pub struct SleepReport\|pub struct AssociationCandidate\|impl SleepReport\|impl SessionState" c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/sleep/sleep_report.rs c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/session/mod.rs | head -20
```

Then add to `proposers/llm_candidate.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sleep::sleep_report::{AssociationCandidate, SleepReport};

    // Build the smallest valid SleepReport + matching promoted_candidate_ids
    // such that one association candidate references positions 1 and 2.
    // Adjust field names to the real shape discovered by the grep above.
    fn report_with_one_candidate() -> SleepReport {
        SleepReport {
            memory_candidates: vec![],
            association_candidates: vec![AssociationCandidate {
                from_memory_candidate_index: 1,
                to_memory_candidate_index: 2,
                weight: Some(0.4),
                reason: Some("test".into()),
                // ...fill remaining required fields with minimal valid values
            }],
            // ...other required fields with minimal valid values
        }
    }

    #[test]
    fn llm_candidate_proposer_returns_named_proposals() {
        let report = report_with_one_candidate();
        let promoted: Vec<Option<String>> = vec![
            Some("memory.a".into()),
            Some("memory.b".into()),
        ];
        let store = MemoryStoreContents::default();
        // SessionState must be constructed via its real constructor — typically
        // SessionState::new(config) or similar. Inspect with:
        //   grep -n "impl SessionState\|pub fn new" .../session/mod.rs
        let session = build_minimal_session_for_proposer_tests();
        let proposer = LlmCandidateProposer {
            report: &report,
            promoted_candidate_ids: &promoted,
        };

        let proposals = proposer.propose(&store, &session, time::OffsetDateTime::UNIX_EPOCH);

        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0].proposer_name, "llm-candidate");
        assert_eq!(proposals[0].from_id, "memory.a");
        assert_eq!(proposals[0].to_id, "memory.b");
    }
}
```

`build_minimal_session_for_proposer_tests` is a helper to add in the test module — return a `SessionState` constructed via whichever public constructor the type exposes (likely `SessionState::new(SessionConfig { ... })`). If a similar helper already exists elsewhere in the crate's test code, pull it up; otherwise inline it once here.

- [ ] **Step 3: Implement**

```rust
// crates/qsf_app/src/sleep/proposers/llm_candidate.rs
use time::OffsetDateTime;

use crate::memory::association::Association;
use crate::memory::store::MemoryStoreContents;
use crate::session::SessionState;
use crate::sleep::proposer::{AssociationProposer, ProposedAssociation};
use crate::sleep::sleep_report::SleepReport;

pub struct LlmCandidateProposer<'a> {
    pub report: &'a SleepReport,
    pub promoted_candidate_ids: &'a [Option<String>],
}

impl<'a> AssociationProposer for LlmCandidateProposer<'a> {
    fn name(&self) -> &str { "llm-candidate" }

    fn priority(&self) -> u8 { 100 }

    fn propose(
        &self,
        store: &MemoryStoreContents,
        _session: &SessionState,
        as_of: OffsetDateTime,
    ) -> Vec<ProposedAssociation> {
        // Reuse the logic from build_sleep_candidate_associations, but emit
        // ProposedAssociation rather than Association.
        let mut out = Vec::new();
        for candidate in &self.report.association_candidates {
            let Some(from_id) = candidate
                .from_memory_candidate_index
                .checked_sub(1)
                .and_then(|i| self.promoted_candidate_ids.get(i))
                .and_then(Option::as_ref)
            else { continue };
            let Some(to_id) = candidate
                .to_memory_candidate_index
                .checked_sub(1)
                .and_then(|i| self.promoted_candidate_ids.get(i))
                .and_then(Option::as_ref)
            else { continue };
            if from_id == to_id { continue; }
            // Endpoint validation: caller's merge_and_dedupe does the final
            // check against known_record_ids; here we only emit proposals.
            out.push(ProposedAssociation {
                from_id: from_id.clone(),
                to_id: to_id.clone(),
                weight: candidate.weight.unwrap_or(0.35).clamp(0.0, 1.0),
                reason: candidate.reason.clone().unwrap_or_else(|| "llm-candidate".into()),
                proposer_name: "llm-candidate".into(),
            });
        }
        let _ = store; // not needed for this proposer
        let _ = as_of;
        out
    }
}
```

In `auto_promote.rs::build_promotion_plan`, replace the `build_sleep_candidate_associations` extension with a call to `LlmCandidateProposer::propose` and apply `merge_and_dedupe`. Behavior must be identical to today; this is a refactor with a registry shape.

- [ ] **Step 4: Run sleep tests**

```powershell
cargo test -p qsf_app sleep
```

Expected: existing sleep tests PASS unchanged.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/sleep/proposers
git commit -m "feat: wrap LLM-candidate flow behind AssociationProposer"
```

### Task 5.3: `SafetyNetCoRetrievalProposer`

**Files:**
- Create: `crates/qsf_app/src/sleep/proposers/safety_net_co_retrieval.rs`
- Modify: `crates/qsf_app/src/sleep/auto_promote.rs` — wire safety net into proposer registry

- [ ] **Step 1: Write failing test**

```rust
    #[test]
    fn safety_net_skips_already_processed_ranges() {
        // Build a store with a ProcessedRange covering turns 0..=5 of session_id "s".
        // Build a session "s" with 6 turns whose retrievals overlap.
        // Assert proposer returns empty.
        let scaffold = build_safety_net_scaffold_with_covered_range();
        let proposer = SafetyNetCoRetrievalProposer;
        let proposals = proposer.propose(
            &scaffold.store,
            &scaffold.session,
            time::OffsetDateTime::now_utc(),
        );
        assert!(proposals.is_empty());
    }

    #[test]
    fn safety_net_proposes_for_uncovered_ranges() {
        let scaffold = build_safety_net_scaffold_with_no_processed_ranges();
        let proposer = SafetyNetCoRetrievalProposer;
        let proposals = proposer.propose(
            &scaffold.store,
            &scaffold.session,
            time::OffsetDateTime::now_utc(),
        );
        assert!(!proposals.is_empty());
    }
```

- [ ] **Step 2: Implement**

```rust
use std::collections::HashSet;

use time::OffsetDateTime;

use crate::memory::co_retrieval::{
    generate_cross_turn_deltas, CoRetrievalDelta, CROSS_TURN_ASSOCIATION_WINDOW,
};
use crate::memory::processed_ranges::uncovered_turn_indices;
use crate::memory::store::MemoryStoreContents;
use crate::session::SessionState;
use crate::sleep::proposer::{AssociationProposer, ProposedAssociation};

pub struct SafetyNetCoRetrievalProposer;

impl AssociationProposer for SafetyNetCoRetrievalProposer {
    fn name(&self) -> &str { "safety-net-co-retrieval" }

    fn priority(&self) -> u8 { 30 }

    fn propose(
        &self,
        store: &MemoryStoreContents,
        session: &SessionState,
        as_of: OffsetDateTime,
    ) -> Vec<ProposedAssociation> {
        if session.turns.is_empty() {
            return vec![];
        }
        let uncovered = uncovered_turn_indices(
            &store.processed_ranges,
            &session.session_id,
            0,
            session.turns.len() - 1,
        );
        if uncovered.is_empty() {
            return vec![];
        }
        // Build retrievals only for the uncovered range. To keep window
        // semantics consistent, we pass the full session retrievals but mask
        // non-uncovered turns to empty so cross-turn pairs only form inside
        // the uncovered span.
        let mask: HashSet<usize> = uncovered.iter().copied().collect();
        let retrievals: Vec<Vec<String>> = session
            .turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                if mask.contains(&i) {
                    turn.context_assembly.retrieved_memory_ids()
                } else {
                    Vec::new()
                }
            })
            .collect();
        let known: HashSet<String> =
            store.records.iter().map(|r| r.id.clone()).collect();

        let deltas = generate_cross_turn_deltas(
            &retrievals,
            &store.associations,
            &known,
            CROSS_TURN_ASSOCIATION_WINDOW,
            &session.session_id,
            as_of,
        );

        deltas
            .into_iter()
            .filter_map(|d| match d {
                CoRetrievalDelta::Create { from, to, weight, reason, .. } => {
                    Some(ProposedAssociation {
                        from_id: from,
                        to_id: to,
                        weight,
                        reason,
                        proposer_name: "safety-net-co-retrieval".into(),
                    })
                }
                // Strengthen is an edge-weight change, not a new proposal —
                // applied separately by the sleep pipeline if it chooses to
                // process Strengthen deltas. For now we drop Strengthen here
                // because the existing sleep flow does not consume
                // strengthened-weight signals from proposers.
                CoRetrievalDelta::Strengthen { .. } => None,
            })
            .collect()
    }
}
```

- [ ] **Step 3: Wire into the sleep pipeline**

In `auto_promote.rs::build_promotion_plan`, route both proposers through the priority-aware pipeline. Use the LLM-candidate proposer too (replacing the earlier inline call landed in Task 5.2) so all proposals flow through the same merge:

```rust
    use crate::sleep::proposer::{
        merge_and_dedupe, sort_by_priority_descending, AssociationProposer,
    };
    use crate::sleep::proposers::llm_candidate::LlmCandidateProposer;
    use crate::sleep::proposers::safety_net_co_retrieval::SafetyNetCoRetrievalProposer;

    let llm = LlmCandidateProposer {
        report,
        promoted_candidate_ids: &promoted_candidate_ids,
    };
    let safety = SafetyNetCoRetrievalProposer;

    let mut tagged: Vec<(u8, _)> = Vec::new();
    for proposal in llm.propose(current_store, session, as_of) {
        tagged.push((llm.priority(), proposal));
    }
    for proposal in safety.propose(current_store, session, as_of) {
        tagged.push((safety.priority(), proposal));
    }
    sort_by_priority_descending(&mut tagged);

    let known: std::collections::HashSet<String> =
        current_store.records.iter().map(|r| r.id.clone()).collect();
    let merged = merge_and_dedupe(
        tagged.into_iter().map(|(_, p)| p).collect(),
        &current_store.associations,
        &known,
    );

    for p in merged {
        new_associations.push(Association::new(
            p.from_id, p.to_id, p.weight, p.reason, as_of,
        ));
    }
```

Remove the inline `LlmCandidateProposer` call previously landed in Task 5.2 — this Task 5.3 wiring supersedes it.

After persistence, write a `SleepSafetyNet` `ProcessedRange` covering `[0, session.turns.len() - 1]` (the safety net's logical anchor span — every turn that was an uncovered anchor at sleep time is now considered processed for safety-net purposes).

- [ ] **Step 4: Run tests**

```powershell
cargo test -p qsf_app sleep
```

Expected: PASS. Adjust existing sleep tests if they pinned the pre-proposer association count and the safety net now adds zero new edges for the covered case.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/sleep
git commit -m "feat: add safety-net co-retrieval proposer wired into sleep pipeline"
```

### Task 5.4: Sleep prompt rewording

**Files:**
- Modify: the sleep prompt template — locate first with `grep -rn "co-retrieved\|build_sleep_candidate\|sleep_summarizer" c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/sleep --include="*.rs"`

- [ ] **Step 1: Locate the prompt**

```powershell
grep -rn "association_candidates\|sleep prompt\|Find\|Identify associations" c:/Users/larsp/src/qualia-signal-foundry/crates/qsf_app/src/sleep --include="*.rs" | head
```

- [ ] **Step 2: Write or update a test that pins the new prompt language**

Find an existing prompt-text test (likely an inline test in `sleep/session_summary.rs` or a sibling). If absent, add one that asserts the new prompt contains the phrase "non-obvious connections" and does NOT contain "co-retrieved" or "mechanically link".

- [ ] **Step 3: Reword**

Find the prompt string and replace mechanical-co-retrieval phrasing with text encouraging the model to surface non-obvious cross-memory connections.

- [ ] **Step 4: Run tests**

```powershell
cargo test -p qsf_app sleep
```

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add crates/qsf_app/src/sleep
git commit -m "feat: reword sleep prompt to emphasize non-obvious connections"
```

### Task 5.5: Phase 5 verification and architecture doc update

**Files:**
- Modify: `docs/Architecture/Architecture.SleepPhase.md`
- Modify: `docs/EngineeringDiary.md`

- [ ] **Step 1: Project verification**

```powershell
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo test
```

- [ ] **Step 2: Update `Architecture.SleepPhase.md` Implementation Status**

Cover:
- Sleep no longer runs the cross-turn co-retrieval window directly.
- Two proposers ship: `LlmCandidateProposer` and `SafetyNetCoRetrievalProposer`.
- Sleep prompt now targets non-obvious connections.

- [ ] **Step 3: Diary entry for Phase 5**

- [ ] **Step 4: Commit**

```powershell
git add docs/Architecture/Architecture.SleepPhase.md docs/EngineeringDiary.md
git commit -m "docs: log Phase 5 (proposer interface + sleep prompt)"
```

- [ ] **Step 5: Human testing**

- Run sleep on a session with no live drops (short session). Confirm the safety net fires and persists associations.
- Run sleep on a session with multiple drops. Confirm the safety net is a no-op (no new associations beyond what proposers like LLM-candidate added).
- Inspect the sleep prompt to confirm the rewording landed.

---

## Phase 6 — Ideas Backlog And Decision Entry

No code.

### Task 6.1: Create `Ideas.AssociationProposers.md`

**Files:**
- Create: `docs/Plans/Ideas.AssociationProposers.md`

- [ ] **Step 1: Draft the backlog**

Content outline:
- Header: scope, why this lives outside the design.
- Each idea is a short section: name, signal, risk of noise, evaluation criteria.
- Initial entries:
  - **Two-hop bridge.** Memories X and Z where no X↔Z edge exists but both connect to Y. Signal: bridge potential. Risk: combinatorial blowup; noise from low-weight Y.
  - **Common-substring / n-gram.** Co-occurring rare n-grams across record summaries. Signal: shared vocabulary. Risk: false positives on common terms.
  - **Cross-session co-retrieval.** Pairs co-retrieved across separate sessions, not just within one. Signal: durable association across contexts. Risk: requires per-session retrieval history (not currently persisted).
  - **Tag-overlap-rarity.** Memories sharing rare tags. Signal: same topic, narrow scope. Risk: tags drift in granularity.
  - **Hint-utility decay (resolves review A2).** Track whether the model response references a hint memory (substring match on `hint.memory.title` or `hint.memory.id`). Decay edge weight for hints that go unused N turns running; strengthen for hints actually used. Signal: closes the live feedback loop the design explicitly scoped out. Risk: substring matching is brittle for hints whose titles overlap with common words; experiment requires deciding how a hint "counts as used."
  - **Edge-direction provenance (follow-up to review A1).** Add `edge_source: CoRetrieval | LlmCandidate | ...` to `Association` so `expand_neighbors` can keep undirected behavior for co-retrieval edges while honoring LLM-asserted direction. Signal: less noise on LLM-proposed edges. Risk: schema bump on `Association`.
- Closing note: evaluation requires a corpus of memories (real session or fixture).

Use the existing `Idea.*.md` files in `docs/Plans/` as the format reference.

- [ ] **Step 2: Commit**

```powershell
git add docs/Plans/Ideas.AssociationProposers.md
git commit -m "docs: open Ideas.AssociationProposers backlog"
```

### Task 6.2: DecisionLog entry

**Files:**
- Modify: `docs/DecisionLog.md`

- [ ] **Step 1: Read the DecisionLog instructions**

```powershell
head -30 c:/Users/larsp/src/qualia-signal-foundry/docs/DecisionLog.md
```

- [ ] **Step 2: Append a dated decision**

Content (per the design):
*Mechanical association work runs in the live loop on drop and session-end; sleep hosts pluggable proposers for non-obvious associations. The sleep prompt is reworded accordingly.*

Fold the prompt rewording into this single entry per Q5 resolution.

- [ ] **Step 3: Commit**

```powershell
git add docs/DecisionLog.md
git commit -m "docs: record live/sleep split in DecisionLog"
```

### Task 6.3: Final architecture wording pass

**Files:**
- Possibly modify: `docs/Architecture/Architecture.MemorySystem.md`
- Possibly modify: `docs/Architecture/Architecture.RuntimeLoop.md`
- Possibly modify: `docs/Architecture/Architecture.SleepPhase.md`

- [ ] **Step 1: Read each architecture doc end-to-end**

Read the three docs and verify their text reflects the final landed shape after Phases 4 and 5. Edit any drift (terminology, removed mechanisms still referenced as present, etc.) inline.

- [ ] **Step 2: Final verification across the workspace**

```powershell
cargo build
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo test
```

For changes under `crates/qsf_browser_server/ui/`:

```powershell
cd crates/qsf_browser_server/ui
npm run check
npm run fmt
```

(Skip if no UI changes were made — this design does not touch the UI.)

- [ ] **Step 3: Commit any wording fixes**

```powershell
git add docs/Architecture
git commit -m "docs: tighten architecture wording after live/sleep split"
```

---

## Documents Updated (Phase Summary)

Per `docs/ProjectFrame/ProjectWorkflow.md`:

| Doc | Phase touched |
|---|---|
| `docs/EngineeringDiary.md` | Phases 1, 2, 3, 4, 5 |
| `docs/Plans/Ideas.AssociationProposers.md` | Phase 6 (created) |
| `docs/Architecture/Architecture.MemorySystem.md` | Phase 4, Phase 6 wording pass |
| `docs/Architecture/Architecture.RuntimeLoop.md` | Phase 4, Phase 6 wording pass |
| `docs/Architecture/Architecture.SleepPhase.md` | Phase 5, Phase 6 wording pass |
| `docs/DecisionLog.md` | Phase 6 |

## Final Verification

```powershell
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

Plus a human-tested long session in a real terminal that exhibits: directs and hints in the prompt, a batched drop with the colored marker, a `:quit` flush, and a sleep run whose proposer pipeline produces a non-empty trace.

## Cross-Cutting Acceptance Criteria

- Reducers stay pure: `plan_token_budget_drop`, `expand_neighbors`, `generate_cross_turn_deltas`, `uncovered_turn_indices`, `merge_and_dedupe` are pure functions.
- `engine_logging` records carry: `session_id`, turn index, dropping range, association counts (new and strengthened), proposer name when applicable, reason for any skipped pass.
- Every code-touching phase ends with a `docs/EngineeringDiary.md` entry.
- `Association` persistence schema stays at `ASSOCIATION_SCHEMA_VERSION = 1`.
- `MemoryStoreContents::processed_ranges` is purely additive via `#[serde(default)]`; legacy stores load unchanged (Task 4.1 includes the test).
- `Turn` / `Exchange` records are not extended; append-only `state.turns` invariant preserved.

## Risks Carried Forward From The Design

- Hint noise from low-weight edges. Mitigation already in: `MAX_HINTS_PER_TURN = 8`, weight-ordered selection.
- Cache-miss thrashing if high-water set too low. Tune defaults during Phase 4 human testing.
- Proposer proliferation. New proposers must enter through `Ideas.AssociationProposers.md` with a measurable signal first.
- OS-level atomic-replace assumption (already used by `MemoryStore::persist` via `NamedTempFile`); verify behavior holds on Windows during Phase 4 human testing.

---

**End of plan.**
