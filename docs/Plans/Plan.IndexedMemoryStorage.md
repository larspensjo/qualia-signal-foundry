# Indexed Memory Storage (SQLite) Implementation Plan

> **For agentic workers:** implement stage by stage, in order. Each stage ends in an
> **evidence gate** — a measured go/no-go that must pass before the next stage starts.
> If a gate fails, stop and report; do not proceed on aesthetics. Phase numbers here are
> internal to this ephemeral plan; durable documents (architecture, decision log,
> experiment specs, code, module names) must name the behavior, never a phase number.

**Goal:** Move per-session memory from a whole-file `memory-store.json` (rewritten in full
every persisted turn) to an indexed SQLite store, in gated stages, so that measured
performance wins — not aesthetics — justify each step. The end state is SQLite as the
authoritative live memory store with field-preserving JSON export/import retained for
local inspection. Every stage can stop early if the numbers do not materialize.

**Motivation (load-bearing, ranked):** normal use is expected to produce thousands of
records and many association edges. The wins we are buying, in priority order:

1. **Per-turn write cost.** Today every persisted turn calls `MemoryStore::persist`, which
   serializes and atomically rewrites the entire store — O(store size) write amplification
   on the live-loop latency path (`live_memory.rs`, `ageing.rs`). Biggest win; grows
   linearly worse with store size.
2. **Startup and footprint.** JSON parses and holds the whole store in memory; SQLite
   touches only the pages a query needs.
3. **Read-side candidate narrowing.** Retrieval scoring is query-dependent, so no index can
   `ORDER BY` the composite score. SQLite's read value is narrowing the candidate set
   (FTS5 / tag / kind / indexed neighborhood traversal) *before* per-candidate scoring.

**Character:** infrastructure engineering with experiment-shaped validation (fidelity and
latency benchmarks). It does not probe a consciousness-simulation mechanism, so it does not
earn a `docs/Experiments/Experiment.*.md` (see *Why no experiment document* below). The
retrieval *mechanism* is unchanged — this plan preserves its behavior bit-for-bit; the only
new research-shaped artifact is a benchmark/fidelity harness.

---

## Committed boundaries (do not re-litigate; these are inputs, not open questions)

These came from the brainstorm and blindspot passes and are fixed for this plan. They are
candidates for the DecisionLog entry recorded when the plan is adopted (see *Documentation
updates*), not decisions this plan re-opens.

- **Single-writer-per-session.** Each session store has exactly one writer today
  (`qsf_app` session runtime writes; `qsf_realtime_server::realtime::memory_store` only
  reads). This is committed as an architectural rule. The plan documents it as a boundary
  and **builds no multi-writer coordination** — no WAL-for-concurrency, locking, or
  multi-writer gates. (SQLite still gets normal transactional durability; that is not
  multi-writer coordination.)
- **Per-session topology.** One database file next to each session's memory store:
  `state/realtime/continuity/<qsf_session_id>/memory-index.sqlite` (Stage A sidecar), later
  `memory.sqlite` (Stage C authoritative). Cross-session queries are out of scope until
  needed. Keeps backup / deletion / privacy boundaries per-session and makes migration
  simplest.
- **DB-candidacy principle.** Dynamic data that affects model behavior is a DB candidate.
  Excluded: source-code constants, configuration (e.g. API keys), and the realtime
  **diagnostics** JSONL log only — that log is ephemeral and discarding it does not affect
  the simulation. Sealed event / trace / run logs remain immutable, file-backed research
  evidence per `ProjectWorkflow.md` and are never moved into SQLite.
- **REST is the stable inspection contract; SQLite schemas are not public interface.**
  Model-callable memory tools use an **in-process** query service in `qsf_memory` behind a
  storage abstraction — no HTTP hop in the live loop. REST (`qsf_browser_server`) is backed
  by the *same* service/semantics so the two cannot drift. No third parallel DTO schema:
  extend `qsf_browser_server::memory::dto` and reuse `qsf_memory::RetrievalResult` /
  `RetrievedMemory`.

## Non-goals

- **Multi-writer coordination** in any form (see boundary above).
- **`GET /api/diagnostics` endpoint** and moving diagnostics into SQLite. The idea doc's
  diagnostics-in-a-database concept is **deferred, not discarded** — recorded here so it is
  not lost. Diagnostics stay JSONL-only for this plan.
- **Cross-session / multi-session aggregate databases.** Per-session only.
- **Migrating sealed run artifacts, reports, event logs, or trace logs** into SQLite. They
  stay file-backed JSON/JSONL.
- **Changing retrieval scoring, tokenization, gate, or tie-break behavior.** This plan
  reproduces the current contract exactly; any scoring change is a separate effort.

---

## Cross-cutting requirements (apply to every stage)

### R1 — Candidate-set membership rule (retrieval fidelity crux)

The idea doc's fidelity contract enumerates *score components* but not *candidate-set
membership*. `retrieve_memories` (`crates/qsf_memory/src/retrieval.rs`) admits records with
zero keyword/tag signal in three ways that a naive FTS-only SQL query would silently drop:

- `RecencyOnly` scores and admits **every** record (`is_relevant_for_strategy`, the
  `RecencyOnly => true` arm).
- `AssociationWeighted` admits any record that is an **association neighbor of a keyword
  seed** (non-empty `association_paths`), even with no direct term match.
- Both `KeywordTag` and `AssociationWeighted` admit **identity/profile records** on identity
  queries via `profile_identity_allowed` even when the only matched terms are generic
  identity words.

The SQLite candidate set MUST be a **proven superset** of:

```
{ FTS/keyword matches }
  ∪ { tag matches }
  ∪ { identity-or-profile records, when the query is an identity query }
  ∪ { association neighbors of the keyword seed set }
  ∪ { all records, for RecencyOnly }
```

"Superset" is acceptable for the **selected** set because the existing per-candidate
relevance gate and scoring run unchanged on the narrowed set — narrowing only changes *which
candidates are scored*, never *which are selected*. The fidelity harness (Phase A3) proves
selected-set equivalence, and its fixtures MUST include identity queries and association-only
hits (see fixtures below).

**Retrieval expansion is outbound only.** `association_paths_by_target` walks edges *from* a
keyword seed (`from_memory_id` ∈ seeds) *to* the neighbor (`to_memory_id`), one hop, and does
not follow inbound edges. The SQLite association-neighbor expansion must therefore be
**directional and one-hop**: seed → `to_memory_id`, not bidirectional and not transitive.
(Contrast the browser neighborhood *inspection* endpoint, which is deliberately bidirectional
— `a.from_memory_id == id || a.to_memory_id == id`. Retrieval candidacy and inspection
neighborhoods are different queries and must not be conflated.) Keep this as an explicit
fidelity assertion in the harness.

**Tag matching is whole-tag, not tokenized.** Title and summary are tokenized (split on
non-alphanumeric, lowercased, terms < 3 chars dropped). Tags are matched as **whole
lowercased strings** (`matched_terms_in_tags` compares each query term against the set of
lowercased tags; `memory_terms` adds whole lowercased tags, not their tokens). The SQLite tag
index must match whole lowercased tag values, and the FTS surface must not silently tokenize
tags in a way that changes tag scoring. The fidelity harness must include a query whose only
signal is a multi-word or punctuated tag so this distinction is exercised.

### R1a — Omitted-set semantics under narrowing (deliberate, documented change)

The current path scores **every** record and emits every non-selected record into `omitted`:
records failing the relevance gate get `RELEVANCE_GATE_SKIP_REASON`, relevant records beyond
`limit` get `RETRIEVAL_LIMIT_SKIP_REASON`, and the omitted list is in full score order. For
`KeywordTag` / `AssociationWeighted` that means `omitted` contains the entire store complement
in scored order — which is **O(store size) and cannot be reproduced by a narrowed candidate
set** without scoring every record, defeating the whole point of narrowing. Exact
byte-for-byte `omitted` parity with today is therefore incompatible with the read-narrowing
win. This plan reconciles that by redefining the omitted semantics rather than abandoning
narrowing:

- **Redefined semantics:** `omitted` contains only records that **entered candidacy** (the R1
  superset) but were not selected — split into `RELEVANCE_GATE_SKIP_REASON` (a candidate that
  failed the per-candidate gate) and `RETRIEVAL_LIMIT_SKIP_REASON` (a relevant candidate beyond
  `limit`). Records that never entered candidacy (no keyword/tag/identity/association signal,
  strategy not `RecencyOnly`) are **neither selected nor omitted** — they are non-candidates.
  `RecencyOnly` is unchanged: every record is a candidate, so its `omitted` is unchanged.
- **Both paths implement this identically.** The change lands in the JSON reference path
  *first* (Phase A-omitted below), so the Stage A gate compares two paths that share one
  candidacy definition and can reach exact ordered `selected` + `omitted` parity. Without this
  step the gate is unpassable.
- **This changes a simulation-observable output** and must not be silent: the
  `MemoryReinforced` event fields `skipped_relevance_ids` / `skipped_relevance_count`
  (`live_memory.rs`) currently carry the store complement and will now carry only
  candidate-set rejects. Before landing, **inventory every consumer of `omitted` and its skip
  reasons** (at minimum `live_memory.rs` reinforcement event fields, and any experiment tests
  that assert relevance-skipped counts — e.g. under `multi_turn_text_loop`), update them to the
  redefined semantics, and update their event/trace field docs. The reinforcement *decision*
  is unchanged (it keys off `selected` only), so no memory-content behavior changes — only the
  observational skip fields. Record this as a DecisionLog candidate (event-field semantics are
  research evidence) and note it in the affected experiment specs.
- **Optional full-scan verification mode.** To *additionally* prove the scoring itself is
  identical (independent of candidacy), the harness MAY run a SQLite "verification mode" that
  scores all records (no narrowing) and compares the full pre-redefinition omitted list against
  today's JSON output on the 1k fixture only. This is a scoring-equivalence check, not the
  production path, and is not required for the gate; the production comparison uses the
  redefined candidate-set-scoped semantics on all fixtures.

### R2 — Per-turn mutation shapes map to bounded writes

The write path is read-modify-mutate, not append. Every per-turn mutation must map to a
bounded (store-size-independent) write. Enumerate and cover all four shapes:

| Mutation shape | Current site | Bounded write |
|---|---|---|
| Create association | `co_retrieval::CoRetrievalDelta::Create` in `live_memory.rs` / `ageing.rs` | `INSERT` one edge row |
| Strengthen association (unordered-pair lookup) | `CoRetrievalDelta::Strengthen`, matched by `(from,to) OR (to,from)` | `UPDATE` one edge row by endpoint-normalized key |
| Reinforce record (`reinforcement_count`, `last_reinforced_at`) | `apply_live_memory_reinforcement` | `UPDATE` one record row **and its denormalized/FTS rows** |
| Extend processed ranges | `persist_cross_turn_range` | `INSERT` one processed-range row |
| Capture new record | `apply_live_memory_capture` | `INSERT` one record row + FTS row |

Requirements that fall out of this:
- Associations are indexed on **both** endpoints so the unordered-pair `UPDATE` is a lookup,
  not a scan.
- FTS5 and any denormalized columns are updated on **reinforcement and every mutation**, not
  only on insert — reinforcement changes recency inputs that read-side queries rely on.
- A **resident store/connection handle** lives in the session runtime so turns stop
  reloading the whole store from disk each turn (today both live write paths call
  `MemoryStore::load_or_empty` per turn).

**The live per-turn paths are not the only writers.** The storage abstraction and the Stage C
flip must cover the *complete* inventory of `memory-store.json` writers, or the authority flip
would leave paths writing stale JSON or forking memory state away from SQLite:

| Writer | Site | When |
|---|---|---|
| Live capture | `apply_live_memory_capture` (`live_memory.rs`) | per turn |
| Live reinforcement + co-retrieval | `apply_live_memory_reinforcement` (`live_memory.rs`) | per turn |
| Cross-turn ageing | `persist_cross_turn_range` (`ageing.rs`) | per turn / batch |
| **Sleep auto-promotion** | `sleep/update.rs` (`build_promotion_plan` → `append_records` / `append_associations` / strengthen / `processed_ranges` → `persist`) | sleep phase |
| **Sleep commit** | `sleep/commit.rs` `write()` → `atomic_write_json(memory-store.json, new_store_contents)` | sleep phase (whole-store typed write) |
| **Copy-forward merge** | `copy_forward_memory_store` / `merge_memory_store_contents` (`runtime.rs`) | session resume across state dirs |

Sleep promotion mutation shapes (new record, new association, strengthen association, extend
processed ranges) are the same shapes as the live path and reuse the same bounded writes. Sleep
commit currently rewrites the whole store from a freshly-computed `new_store_contents`; under
SQLite it becomes a bounded batch of inserts/updates against the resident store, inside one
transaction. Copy-forward merge (dedup-merge of two stores across directories) becomes a
store-to-store merge over SQLite; its per-session-directory topology is unchanged. All of these
route through the storage abstraction — no writer is allowed to keep a direct
`memory-store.json` path after the Stage C flip. Stage B's differential consistency check and
Stage C's migration must exercise a **sleep cycle and a resume/copy-forward**, not only live
turns.

### R3 — Raw-field preservation

`load_existing` retains source-faithful per-record raw JSON (`raw_records`, backing
`/api/memories/{id}/raw`; see test `raw_record_index_preserves_extra_fields`). DecisionLog
2026-05-10 states additive unknown fields do **not** bump `schema_version` and must survive.

**Reality check on the current write path.** Only the browser *read* path (`load_existing`)
preserves raw JSON today. Every *write* path — `MemoryStore::persist`, `sleep/commit.rs`
`atomic_write_json`, and `copy_forward_memory_store` — serializes the **typed**
`MemoryStoreContents`, so any unknown field on any record is **dropped on the first whole-store
rewrite** (and today every persisted turn rewrites the whole store). The DecisionLog
schema-extension rule is therefore currently honored only for records that are never
re-written. Consequences for this plan:

- "JSON authoritative while mirroring to SQLite" in Stage B does **not** by itself preserve
  unknown fields — the authoritative JSON writer is already lossy. Raw preservation is anchored
  on the **SQLite `raw_json` columns**, which the SQLite build/migration captures at ingest and
  which bounded writes preserve. This is a case where SQLite *improves* on the status quo: a
  bounded per-row write leaves untouched rows' `raw_json` intact, whereas the JSON whole-store
  rewrite loses them.
- The SQLite schema stores the **verbatim raw JSON** alongside typed columns for **both records
  and associations** (typed columns are a derived index; raw JSON is the source of truth). A
  typed round-trip that drops unknown fields is **not acceptable** for either.
- **On mutation of a row** (e.g. reinforcement, strengthen), the changed typed fields are
  merged into that row's `raw_json` object — updating known keys while preserving unknown keys
  — so a mutated record does not lose unknown fields either.

Verification (records **and** associations):
- Phase A2: an unknown extra field on a **record** and on an **association** survives
  build → read-back through `raw_json`.
- Phase B1: after a bounded mutation, the mutated row's unknown fields survive **and** an
  untouched row's unknown fields survive (the whole-store-rewrite regression the JSON path has).
- Phase C2: unknown fields on records and associations survive export → reimport.

### R4 — Per-record `schema_version` in a relational schema

`MemoryRecord` and `Association` carry independent per-record `schema_version` (currently
both `1`); the live store errors loudly on off-version records (DecisionLog 2026-05-10).
Mapping:
- Each typed table carries a `schema_version` column mirroring the record's own version (not
  a single table-wide DDL version). Load/read paths reject off-version rows with the same
  loud error semantics as `ensure_current_memory_schema` / `ensure_current_association_schema`.
- SQLite's own DDL/migration version (the `PRAGMA user_version` of the *file format*) is a
  separate concern from record `schema_version` and never overwrites it. Because R3 keeps
  the raw JSON verbatim, a future record-schema bump re-derives typed columns from raw JSON
  without rewriting the stored raw evidence.

### R5 — Config flags default to exercising the new path

New behavior must not hide behind a flag that defaults to the old path (repo rule). Concretely:
- The SQLite retrieval shim and sidecar builder are **exercised by default** in the
  differential fidelity harness and benchmarks (CI runs the SQLite path every build) — they
  are never dead code waiting behind an off-by-default switch.
- The authoritative-storage transition (Stage C) is a **migration**, not a permanent runtime
  toggle: once the Stage C gate passes, SQLite is the default authoritative backend and JSON
  becomes export/import only. We deliberately avoid a long-lived `backend = json|sqlite`
  runtime flag that would leave one path defaulted-off and rotting. Until the Stage C gate
  passes, JSON remains authoritative and SQLite is the sidecar/benchmark path.

---

## Fidelity & benchmark artifact contract

The plan claims that a machine can *prove* the SQLite path reproduces JSON retrieval and that
sealed artifacts still replay. Those claims rest on parsed artifacts, so the artifact shape
is defined before implementation.

### Fidelity comparison record (Phase A3 differential harness)

For each `(fixture, query, strategy)` triple the harness emits a structured comparison
record with these required fields, for **both** the JSON and SQLite paths:

```
query
strategy
selected_ids          # ordered — tie-break order is part of the contract
omitted                # [{ id, skip_reason }], ordered
score_components       # per candidate: { total, recency, keyword, tag, association,
                       #   importance, reinforcement } (from RetrievalScore)
matched_terms          # per candidate
association_paths      # per candidate: [{ from, to, weight, reason }]
candidate_provenance   # per candidate: which membership rule admitted it
                       #   (keyword | tag | identity | association_neighbor | recency_all)
```

`candidate_provenance` is the field that makes R1 auditable: it shows *why* each candidate
entered the scored set, so an equivalence failure is explainable, not just detectable.

**Artifact boundary.** The harness writes these records as JSONL under a benchmark output
directory (not under `runs/` — this is not a sealed simulation run). The JSONL is the
chronological fact stream; a derived human-readable report summarizes pass/fail and latency
percentiles. Sealed run artifacts are untouched.

**Artifact-parsing verification.** An automated test parses the JSONL and asserts, per triple:
`selected_ids` sequences are byte-identical between paths; `omitted` id+skip_reason sequences
are byte-identical between paths **under the R1a candidate-set-scoped omitted semantics** (both
paths share one candidacy definition, so the ordered omitted lists must match exactly, not just
as sets); and `score_components` agree within f64 determinism (the tie-break is total-desc,
`created_at`-desc, id-asc, so ordering must be exactly reproduced). A divergence must be
reproducible from the record alone. Do not mark the fidelity criterion complete until this
parser passes on all fixtures and all query classes.

Note the two paths compared here are the **redefined** JSON reference path (Phase A-omitted)
and the SQLite path — both candidate-set-scoped. The separate, optional full-scan verification
mode (R1a) checks the SQLite scoring against *today's* JSON omitted output on the 1k fixture
only, and is not part of this per-triple parity assertion.

### Replay artifact check (Phase C1, before authority flip)

Before SQLite becomes authoritative, verify sealed run artifacts that reference or snapshot
session memory replay identically against the SQLite-authoritative read path, **or** establish
that no sealed artifact depends on live-store contents. The check parses existing sealed
`runs/<run-id>/` artifacts, identifies any that embed or reference live memory-store state,
and either replays them through the SQLite path and diffs the stable meaningful fields, or
documents (with the parsed evidence) that the dependency does not exist. This is a hard gate,
not a summary.

---

## Synthetic fixtures (built in Phase A1, used by every gate)

Three sizes, generated deterministically (seeded) so benchmarks are reproducible:

- 1,000 records / 5,000 associations
- 10,000 records / 50,000 associations
- 50,000 records / 250,000 associations

Each fixture MUST contain, in addition to ordinary keyword/tag records:
- **identity/profile records** (`assistant_identity`, `user_identity`, `profile` tags; titles
  like `Assistant name: …` / `User name: …`) so identity-query candidate membership (R1) is
  exercised;
- **association-only hits**: records reachable only as association neighbors of a keyword
  seed, with no direct term/tag match, so the `AssociationWeighted` membership rule is
  exercised;
- a spread of `reinforcement_count`, `importance`, `created_at`, and `last_reinforced_at`
  (including `None`) so recency decay and reinforcement scoring are covered;
- at least one record carrying an **unknown extra field** (R3) so raw-preservation is testable
  on real fixtures.

The **query set** run against every fixture MUST cover: keyword queries, tag queries, identity
queries (assistant and user, using the phrasings `identity_query_target` recognizes),
association-only queries, and a recency-only pass — across all three `RetrievalStrategy`
values.

---

# Stage A — Rebuildable SQLite sidecar + retrieval shim (non-authoritative)

JSON stays authoritative throughout Stage A. The sidecar is rebuildable from JSON and can be
deleted at any time. Goal: prove read-side fidelity and measure read/startup advantage before
touching the write path.

## Phase A0 — Cached inspection index in `AppState` (cheap win + honest read baseline)

The browser inspector rebuilds derived indexes **per request** (`build_index` is called in
every route in `qsf_browser_server::memory::routes`). Cache an owned index in `AppState`
beside the immutable `LoadedStore`.

This earns its place independently of SQLite: (1) it is an immediate, low-risk latency win for
static inspection; (2) it makes the JSON read baseline *fair* — otherwise Stage A would be
benchmarking SQLite against a needlessly slow per-request rebuild and claiming a win that is
really just caching; (3) it forces the read-path to go through an owned-index abstraction,
which is the seam the SQLite-backed service later slots into.

- Build the derived index once at store load, store it in `AppState`, and have all
  `/api/store/summary`, `/api/memories*`, neighborhood routes read the cached index.
- Keep DTO outputs byte-identical (regression test the existing route responses).

**Verify:** existing browser route tests still pass with identical JSON; a test asserts the
index is built once, not per request. `cargo test -p qsf_browser_server`.

**Not gated on SQLite evidence** — this is a pure JSON improvement and may land regardless of
later gate outcomes.

## Phase A1 — Fixture generator + JSON baseline benchmark harness

Build the deterministic fixture generator (three sizes, membership coverage per *Fixtures*)
and a benchmark harness that measures the **JSON baseline** on each fixture:

- whole-store `persist` wall-clock (the write-amplification baseline);
- cold load (`load_or_empty` / `load_existing`) time and peak resident memory attributable to
  the store;
- `retrieve_memories` latency per strategy and query class (p50/p95).

The harness emits the fidelity comparison records and latency percentiles per the artifact
contract. This phase produces the numbers every later gate compares against.

**Verify:** harness runs on all three fixtures and emits baseline JSONL + report; numbers are
stable across two runs within noise. Fixtures include identity and association-only records
(assert in a test).

## Phase A-omitted — Candidate-set-scoped omitted semantics in the JSON path (R1a)

Land the redefined omitted semantics (R1a) in the **JSON reference path first**, so the Stage A
fidelity comparison has a well-defined, narrowing-compatible baseline. This is a controlled,
tested change to current behavior — do it before the SQLite retrieval shim exists so the change
is isolated from the storage change.

- Factor the candidate-set membership rule (R1) into a shared function the JSON path uses to
  decide candidacy; score and gate only candidates; emit `omitted` scoped to candidates that
  entered but were not selected.
- Inventory and update every consumer of `omitted` / skip reasons: the `MemoryReinforced` event
  fields `skipped_relevance_ids` / `skipped_relevance_count` in `live_memory.rs`, and any
  experiment tests that assert relevance-skipped counts (search `skipped_relevance`,
  `RELEVANCE_GATE_SKIP_REASON`; check `multi_turn_text_loop` tests). Update their expectations
  and any field-doc comments to the redefined semantics.
- Update the affected experiment specs' trace/field documentation to note the change, and add
  the DecisionLog candidate (event-field semantics change).

**Verify:** existing retrieval tests pass under the new semantics; a new test asserts that a
no-signal record under `KeywordTag` is a non-candidate (neither selected nor omitted) while a
candidate that fails the gate still appears in `omitted` with `RELEVANCE_GATE_SKIP_REASON`;
`RecencyOnly` omitted is unchanged. `cargo test -p qsf_memory -p qsf_app`.

**This is the one deliberate behavior change in the plan** — surface it in the adoption
DecisionLog entry and do not let it land silently.

## Phase A2 — Storage abstraction + rebuildable `memory-index.sqlite` sidecar

- Introduce a **storage abstraction** in `qsf_memory` (name it for the behavior, e.g. a
  `MemoryStorage` read trait — not for this plan/phase). The existing JSON path implements it;
  the SQLite path is a second implementation. Keep reducers pure — this is a side-effect layer.
- Add `rusqlite` (workspace dependency) with the **bundled** feature so FTS5 and a pinned
  SQLite build ship with the binary (no reliance on a system libsqlite). Justification for
  rusqlite over alternatives: synchronous, thread-per-session fits single-writer-per-session;
  no async runtime coupling in the storage layer; bundled FTS5; mature and widely used;
  `sqlx` would add async/compile-time-checked-query machinery we do not need for an
  in-process per-session file. Record the crate choice as a DecisionLog candidate.
- Build the sidecar from an authoritative `memory-store.json`: typed columns for records
  (id, kind, title, summary, tags, timestamps, importance, reinforcement, source, tokens,
  `schema_version`) **plus a `raw_json` column holding verbatim record JSON** (R3); an
  associations table indexed on both endpoints (R2) with `raw_json` (R3); a processed-ranges
  table; and an FTS5 table over the searchable text surface (title, summary, tags — bounded by
  the DB-candidacy principle; no raw diagnostic material).
- Schema carries per-record `schema_version` columns and rejects off-version rows loudly (R4).

**Verify:**
- Delete the sidecar, rebuild from JSON, and assert the rebuilt DB reproduces the same typed
  contents (rebuildability).
- An unknown extra field on a **record** and on an **association** survives build → read-back
  through the `raw_json` columns (R3), on real fixture rows.
- Off-version record/association rows are rejected with the loud error (R4).
- `cargo test -p qsf_memory`.

## Phase A3 — SQLite-backed retrieval shim + differential fidelity harness

- Implement `retrieve_memories`' contract over SQLite: candidate narrowing per **R1** (FTS +
  whole-lowercased-tag match + kind + identity-record admission + **outbound one-hop**
  association-neighbor expansion of the keyword seed set + all-records for RecencyOnly), then
  the **existing** per-candidate scoring, relevance gate, tie-break, and limit logic run
  unchanged on the narrowed set. Reuse the shared candidacy function and the scoring/gate code
  from Phase A-omitted; do not reimplement scoring in SQL.
- Emit `omitted` under the R1a candidate-set-scoped semantics (identical definition to the
  refactored JSON path).
- Extend the harness (A1) to run both paths and emit the fidelity comparison records with
  `candidate_provenance` (artifact contract).

**Verify:** the artifact-parsing verification passes — identical ordered `selected_ids`,
identical ordered `omitted` (id+skip_reason under R1a semantics), agreeing `score_components` —
across **all** fixtures and **all** query classes (keyword, tag including a multi-word/punctuated
tag, identity assistant/user, association-only, recency-only × three strategies). This is a
correctness bar (100% match), not a threshold. The outbound-only, one-hop association expansion
is asserted explicitly (a record reachable only by an inbound edge from a seed must NOT be a
retrieval candidate). **External human review recommended:** a reviewer spot-checks the fidelity
report and confirms identity, tag, and association-only cases are genuinely present and passing,
not skipped.

## Stage A evidence gate (go/no-go)

Proceed to Stage B only if, on the fixtures, measured numbers show real advantage. Proposed
criteria (thresholds are proposals — confirm against the first real harness run; see Open
Questions):

- **Fidelity (hard, must be 100%):** every `(fixture, query, strategy)` triple matches per the
  artifact-parsing verification. Any mismatch blocks the gate regardless of latency.
- **Read latency:** SQLite retrieval p95 at 50k/250k is **≥ 3× lower** than JSON full-scan
  p95, and stays **within the live-loop retrieval budget** in absolute terms (budget TBD —
  proposed placeholder **≤ 20 ms p95**; confirm the real budget, see Open Questions). At
  1k/5k, SQLite retrieval is allowed a small regression but must stay within **1.2×** of JSON.
- **Startup/footprint (secondary):** cold open-to-first-query at 50k/250k is **≤ 25%** of JSON
  parse-whole-store time, and resident memory attributable to storage is **materially lower**
  (proposed **≤ 40%** of the JSON approach).

If read latency shows no advantage but write-side is still the expected primary win, the gate
may record a **conditional pass** that carries Stage A's sidecar forward *only* as the
substrate for Stage B's write work — but this must be an explicit, recorded decision, not a
silent slide. If nothing shows advantage, stop here; the AppState cache (A0) still stands.

---

# Stage B — Incremental writes + in-process live memory query service

Now attack the primary win. JSON is still authoritative for durability at the start of Stage B;
the SQLite store is kept in sync incrementally and validated against it, so a Stage B failure
does not risk live data.

## Phase B1 — Resident handle + bounded incremental writes (all writers)

- Give the session runtime a **resident store/connection handle** so `apply_live_memory_capture`,
  `apply_live_memory_reinforcement` (`live_memory.rs`), and `persist_cross_turn_range`
  (`ageing.rs`) stop calling `load_or_empty` per turn (R2).
- Map every per-turn mutation shape (R2 table) to a bounded write: create/strengthen
  association (endpoint-normalized `UPDATE` using the both-endpoints index), reinforce record
  (record `UPDATE` **plus** FTS/denormalized refresh **plus** `raw_json` merge preserving
  unknown keys — R3), capture record (`INSERT` + FTS row), extend processed ranges (`INSERT`).
  Wrap each turn's mutations in one transaction for crash-atomicity (durability, not
  multi-writer coordination).
- **Route the non-live writers through the same storage abstraction** (R2 writer inventory):
  sleep auto-promotion (`sleep/update.rs`) reuses the create/strengthen/insert bounded writes;
  sleep commit (`sleep/commit.rs`) becomes a bounded transactional batch instead of
  `atomic_write_json` of the whole store; copy-forward merge (`copy_forward_memory_store` /
  `merge_memory_store_contents`, `runtime.rs`) becomes a store-to-store merge over the
  abstraction. No writer keeps a direct typed-JSON whole-store rewrite.
- During Stage B, mirror writes so JSON remains authoritative and a consistency test can diff
  the SQLite store against the JSON store after a synthetic turn stream.

**Verify:**
- Unit tests: each mutation shape produces exactly the expected bounded write and leaves the
  SQLite store equal to the JSON store after the same operation (differential consistency).
- **A sleep cycle and a resume/copy-forward** are exercised, not only live turns: after a
  synthetic sleep promotion and after a cross-dir resume, the SQLite store equals the expected
  merged store.
- Reinforcement updates the FTS/denormalized rows (regression test: a reinforced record's
  recency-affected read result changes correctly — R2 (d)).
- **Raw preservation on mutation (R3):** after a bounded mutation, the mutated row's unknown
  fields survive **and** an untouched row's unknown fields survive — the exact regression the
  JSON whole-store rewrite fails today; verify for both a record and an association.
- `cargo test -p qsf_app -p qsf_memory`.

## Phase B2 — In-process live memory query service + REST parity

- Add the **in-process** live memory query service in `qsf_memory` behind the storage
  abstraction, with explicit **freshness semantics**: reads observe prior writes within the
  same session (read-after-write on the resident handle). Document the freshness contract
  (single-writer-per-session makes this a local invariant, not a distributed one).
- Realtime memory tools (`qsf_realtime_server::realtime::memory_store`) call this service
  in-process — no HTTP hop in the live loop.
- Browser REST is backed by the **same** service/semantics. Extend
  `qsf_browser_server::memory::dto` with the candidate additions that fit the DB-candidacy
  principle: `GET /api/memories/tags`, `/api/memories/kinds`, `/api/memories/recent`,
  `/api/memories/reinforced`. Reuse `RetrievalResult` / `RetrievedMemory`; **no third DTO
  schema**. (`GET /api/diagnostics` stays deferred — non-goal.)

**Verify:**
- Realtime tool search/association-inspection returns results identical to the JSON-backed
  results on the fixtures (fidelity harness reused).
- New REST endpoints return DTOs and are covered by route tests; existing endpoints unchanged.
- `cargo test -p qsf_realtime_server -p qsf_browser_server`; UI checks if any `ui/` file
  changes (`npm run check`, `npm run fmt`).

## Stage B evidence gate (go/no-go) — the primary-win gate

Proposed criteria (confirm against real numbers):

- **Per-turn write cost is O(1) in store size:** measured per-turn write p50 at 50k/250k is
  **within 1.5×** of per-turn write p50 at 1k/5k (i.e. does not grow with store size), and is
  **≥ 10× faster** than the JSON whole-store `persist` at 50k/250k. This is the motivating win;
  if it does not hold, the authoritative flip (Stage C) is not justified — stop and report.
- **Consistency (hard):** after a synthetic turn stream (including a sleep cycle and a
  copy-forward), the SQLite store equals the JSON store on all **typed** contents. Raw-field
  preservation is checked against SQLite's own before/after (R3), **not** against the JSON store
  — the authoritative JSON writer is lossy for unknown fields, so it cannot be the raw-field
  reference; SQLite is strictly better here and that is the point.
- **Live-loop budget:** realtime tool query latency stays within the live-loop budget on the
  50k/250k fixture (same budget as Stage A read gate).

## Trace note for Stage B

Stage B changes *how* memory is written, not *what* the runtime traces about memory selection.
The existing memory events/traces (`MemoryReinforced`, `CoRetrievalAssociationsProposed`,
`MemoryStorePersisted`) keep their current fields and meaning; only the persistence mechanism
behind `MemoryStorePersisted` changes. No new behavioral-chain trace contract is introduced by
Stage B beyond the fidelity/consistency artifacts already defined.

---

# Stage C — SQLite as authoritative live storage

Only reached if Stage A and Stage B gates passed. Flips authority from JSON to SQLite, keeps
JSON as field-preserving export/import for local inspection.

## Phase C1 — Replay gate (hard gate, runs before the flip)

Execute the **Replay artifact check** from the artifact contract: parse sealed
`runs/<run-id>/` artifacts, find any that reference or snapshot live memory-store contents,
and either replay them through the SQLite-authoritative read path and diff stable meaningful
fields, or establish (with parsed evidence) that no sealed artifact depends on live-store
contents. **External human review recommended:** a reviewer confirms the replay evidence
covers the artifact kinds that could embed memory state.

**Gate:** the flip does not proceed unless replay is identical (or independence is proven).

## Phase C2 — Authoritative SQLite + field-preserving JSON export/import + migration

- Make SQLite authoritative for live memory (`memory.sqlite`); retire the JSON whole-store
  write path from **every** writer in the R2 inventory — live per-turn paths, sleep
  auto-promotion, sleep commit, and copy-forward merge (per R5, this is the migration, not a
  defaulted-off flag). Confirm no `memory-store.json` writer remains: a repo search for the
  typed whole-store write helpers (`atomic_write_json(... memory-store.json ...)`,
  `MemoryStore::persist` on the live path) must return only export code.
- Provide **field-preserving** JSON export and import so memory stays inspectable outside the
  DB and the PowerShell/browser local-inspection workflow is preserved (idea-doc boundary).
  Export reconstructs the store JSON from the `raw_json` columns (R3), not from typed columns,
  so unknown fields survive for records **and** associations.
- Provide a one-time migration from an existing authoritative `memory-store.json` to
  `memory.sqlite` (build the DB, verify equality, then switch authority). Keep the original
  JSON as a recoverable export.

**Verify:**
- **Unknown-field round-trip (hard, R3):** a **record and an association** each carrying an
  unknown extra field survive export → reimport unchanged.
- Migration of a real existing session store produces a SQLite store whose export equals the
  original JSON (modulo formatting) including raw fields on records and associations.
- Off-version rows still error loudly (R4) after the flip.
- **No stray JSON writer remains** (the search above returns only export code).
- Full fidelity harness re-run against the authoritative path: retrieval contract still holds.
- A **sleep cycle and a resume/copy-forward** run against the authoritative SQLite store and
  produce the expected state (the non-live writers work post-flip).
- **External human testing recommended:** run a live realtime session against the
  SQLite-authoritative store, confirm memory search / association inspection behave as before
  and PowerShell inspection (`Invoke-RestMethod` against the REST surface) remains ergonomic.

## Stage C evidence gate (final go/no-go)

- Replay identical or independence proven (C1).
- Unknown-field export/reimport round-trips (C2).
- Retrieval fidelity holds against the authoritative path.
- Human inspection workflow preserved (live session + PowerShell spot check).

If any fails, do not flip authority; SQLite remains the sidecar/read+write-mirror from Stage B
and JSON stays authoritative, with the failure recorded.

---

## Why no experiment document

Per `ProjectWorkflow.md` (*Document Tracks: Plans vs Experiments*), an `Experiment.*.md` is for
reducing uncertainty about a **consciousness-simulation mechanism** whose behavior is genuinely
in doubt. This plan changes *storage and performance* while holding the retrieval mechanism's
behavior bit-for-bit constant (the whole point of the fidelity contract). The outcome under
question is engineering equivalence and latency, not how a mechanism behaves — so the
validation lives in the differential fidelity harness, the benchmark harness, and tests, not in
an experiment document. The memory *retrieval mechanism itself* is already covered by existing
memory experiments; this plan must not perturb their results, which is exactly what the
fidelity gate enforces. If, while implementing, a genuine mechanism question surfaces (e.g. a
candidate-membership edge case that reveals the *current* behavior is itself uncertain or
undesired), that question earns its own experiment — it is not silently folded into this plan.

## Documentation updates (per `ProjectWorkflow.md`)

- **`docs/Plans/Idea.IndexedMemoryStorage.md`** — mark promoted to this plan (status note;
  keep as background).
- **`docs/DecisionLog.md`** — a DecisionLog entry is expected when this plan is **adopted**,
  covering: single-writer-per-session boundary, per-session topology, the gated path to
  authoritative SQLite, and the `rusqlite` (bundled/FTS5) technology choice. A **separate**
  DecisionLog candidate covers the R1a omitted-set semantics change (retrieval `omitted` becomes
  candidate-set-scoped; the `MemoryReinforced` `skipped_relevance_*` fields change meaning) —
  this is a behavior change to research-evidence event fields and must be recorded when it
  lands, not left implicit. *Do not write the entries as part of this plan draft* — they are
  recorded at adoption / when the change lands, naming the behavior, never a phase number.
- **Affected experiment specs** — any experiment whose event/field expectations mention
  relevance-skipped counts (e.g. `multi_turn_text_loop`-related specs, `Experiment.LiveMemory*`)
  must be annotated with the R1a semantics change so their historical run records are read
  correctly.
- **`docs/Architecture/Architecture.MemorySystem.md`** — update the *Implementation Status*
  section as each stage lands (sidecar → incremental writes/live query service → authoritative
  SQLite), with code-module refs and a refreshed `Last reviewed:` date. Add the storage
  abstraction and per-session topology to the described structure.
- **`docs/Architecture/Architecture.RealtimeSessionServer.md`** and any inspection/REST
  architecture doc — note the in-process query service vs REST split and the new endpoints.
- **`docs/Handoff.md`** — update Now/Next/Horizon when a stage gate outcome changes the
  recommended next step (rewrite in place; pointer, not content).
- Durable docs name behaviors ("SQLite-backed live memory storage", "memory-index sidecar",
  "candidate-set membership rule"), never this plan's phase numbers.

## Open questions (carry explicitly; do not silently resolve)

1. **Live-loop retrieval latency budget.** The gates reference a "live-loop retrieval budget"
   but the repo does not state a hard number here. Proposed placeholder: **p95 ≤ 20 ms** for
   memory retrieval on the 50k/250k fixture. Confirm the real budget from the realtime turn
   latency target before the Stage A gate is scored.
2. **Exact gate thresholds.** The ≥3× read speedup, ≥10× write speedup, ≤1.5× write-growth,
   and footprint percentages are proposals derived from the expected O(store size) vs O(1)
   asymptotics. Confirm/adjust against the first real harness run rather than treating them as
   settled.
3. **FTS5 tokenizer choice.** The current in-memory tokenizer splits on non-alphanumeric,
   lowercases, and drops terms shorter than 3 chars. FTS5's `unicode61` tokenizer must be
   configured (or the query pre-tokenized with the existing tokenizer and matched as terms) so
   FTS matching reproduces the current matched-term behavior exactly. Decide whether to drive
   FTS from the existing tokenizer's output (safest for fidelity) or reproduce its rules in the
   FTS config. R1 fidelity is the arbiter.
4. **Association-neighbor expansion depth — RESOLVED to outbound, one hop.** Current retrieval
   expands `from_memory_id ∈ seeds → to_memory_id`, one hop, directional (not inbound, not
   transitive); see R1. Retained here only as a standing fidelity assertion, not an open choice.
5. **Sidecar vs authoritative file naming and coexistence.** During Stage B the SQLite file
   mirrors JSON; confirm whether the mirror uses the Stage A `memory-index.sqlite` name or
   already `memory.sqlite`, and how a half-migrated directory (both files present) is detected
   and resolved at load. Proposed: keep `memory-index.sqlite` until the Stage C flip renames/
   supersedes it, with a load-time check that errors loudly on an ambiguous directory.
6. **Diagnostics-in-DB deferral scope.** Recorded as a non-goal; confirm nothing in Stage B's
   REST additions accidentally implies a diagnostics endpoint contract.
