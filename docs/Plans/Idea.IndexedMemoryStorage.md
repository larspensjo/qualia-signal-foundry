# Idea: Indexed Memory Storage

## Status

Brainstorm. Captures a storage direction for substantial future memory growth.
This is not a commitment to migrate all persisted state to SQLite.

## Purpose

Explore how Qualia Signal Foundry can keep memory queryable and responsive as
long-term memory grows from small JSON files into a larger operational store,
while preserving the current advantage that data is easy to inspect from
PowerShell and standard local tools.

The core conclusion is a hybrid direction with two distinct surfaces:

```text
Static/post-hoc inspection, already partly implemented:
  PowerShell / browser UI
    -> qsf_browser_server REST API
      -> one loaded memory-store.json snapshot
      -> optional SQLite sidecar/index for faster inspection queries

Live memory query, proposed:
  realtime tools / live UI / PowerShell
    -> live memory query service
      -> per-session JSON backend today
      -> SQLite-backed live memory later, if growth warrants it
```

The REST/API layer should be the human- and tool-facing contract. SQLite, if
adopted, should initially be an implementation detail behind that contract rather
than a requirement that every operator query the database directly.

## Current Observations

The current repository state does not show a raw performance emergency. Local
artifacts are still small: the realtime diagnostics log is hundreds of KB, the
realtime session-state snapshot is tens of KB, and the checked-in memory-store
fixture is small.

However, the current memory implementation is growth-sensitive:

- `qsf_memory::MemoryStore` loads a whole `memory-store.json` file and rewrites
  the whole file atomically on persist.
- Retrieval scans records and associations in memory, scores candidates, and
  sorts them.
- Realtime memory tools load the session memory store for memory search and
  association inspection.
- Live memory reinforcement and capture mutate the same coarse JSON store,
  producing O(store size) write amplification for every turn that persists
  memory or association changes.
- The JSON-file path relies on atomic rename to avoid torn writes, but it does
  not provide multi-writer coordination if two live writers race on the same
  store.
- The post-hoc browser API currently rebuilds derived in-memory indexes per
  request; caching an owned index in `AppState` may be a cheaper near-term
  improvement than SQLite for static inspection workloads.

This is acceptable for early prototypes and small stores. It will become less
attractive if normal use produces thousands of memory records, many association
edges, frequent realtime tool calls, concurrent live memory writers, or richer
post-hoc diagnostic queries.

## Candidate Direction

Keep sealed run artifacts as files, but make live memory and diagnostics
queryable through stable APIs and optional indexed storage.

### REST inspection surface

Do not invent a parallel REST namespace. `qsf_browser_server` already exposes a
snapshot memory inspection API that returns domain DTOs:

```text
GET /api/store/summary
GET /api/memories?q=<terms>&tag=<tag>&limit=<n>&offset=<n>
GET /api/memories/{id}
GET /api/memories/{id}/raw
GET /api/memories/{id}/neighborhood?limit=<n>
```

The idea should extend that shipped surface rather than fork it. Candidate
additions:

```text
GET /api/memories/tags
GET /api/memories/kinds
GET /api/memories/recent?limit=<n>
GET /api/memories/reinforced?limit=<n>
GET /api/diagnostics?k=<kind>&session=<id>
```

PowerShell remains a first-class inspection path:

```powershell
Invoke-RestMethod "http://localhost:PORT/api/memories?q=volition&limit=10"
Invoke-RestMethod "http://localhost:PORT/api/memories/memory.live.default.turn-003.topic"
Invoke-RestMethod "http://localhost:PORT/api/memories/memory.live.default.turn-003.topic/neighborhood?limit=20"
```

The API should continue returning domain DTOs, not raw internal persistence rows.
That keeps the UI, scripts, and model-callable tools insulated from storage
changes.

### Snapshot inspection versus live query

The existing browser server is a static snapshot inspector. It loads one store
path at startup and serves that immutable `LoadedStore`; it does not reload the
file or observe live writes.

That is a useful post-hoc inspection surface, but it is not the same thing as a
live memory query service. A later live service must define freshness,
concurrency, session selection, and writer coordination separately. The first
SQLite experiment should be explicit about which surface it targets.

Candidate split:

- **Static snapshot inspection:** one store path, immutable loaded snapshot,
  browser/PowerShell inspection, no live freshness guarantee.
- **Live memory query:** current active session id, fresh read-after-write
  expectations, realtime tool compatibility, and explicit single-writer or
  transactional behavior.

### Store topology

Memory is not one global file in current runtime topology. Realtime continuity is
keyed by QSF session id:

```text
state/realtime/continuity/<qsf_session_id>/memory-store.json
```

The browser inspector points at one selected store path. The live realtime server
resolves the active store from the current session id.

Any SQLite-backed design must choose one of these topologies:

- one SQLite database per session, mirroring the current directory layout;
- one SQLite database indexing many sessions, with `session_id` as part of every
  record, association, processed-range, and diagnostic key;
- a hybrid where live memory stays per-session but post-hoc indexes can aggregate
  many sessions or runs.

The topology choice affects backup, deletion, privacy/trust boundaries, query
latency, and whether cross-session association queries are possible.

### DTO ownership

Avoid creating a third parallel schema for query results.

- Browser inspection DTOs currently live in `qsf_browser_server::memory::dto` and
  should remain the source of truth for the shipped browser API unless they are
  deliberately promoted into a shared crate.
- Search/retrieval semantics already live in `qsf_memory::RetrievalResult` and
  `qsf_memory::RetrievedMemory`, which are serializable and should be the first
  candidate shapes for search-result parity.
- If a shared live query service needs a cross-crate wire contract, promote or
  adapt existing DTOs intentionally rather than creating a new, drifting set.

### SQLite sidecar or backend

SQLite is a good candidate for indexed live memory because memory queries are
naturally database-shaped:

- lookup by memory id
- filter by kind, tag, source, or session
- order by recency, reinforcement, or importance
- traverse association neighborhoods by either endpoint
- query processed ranges and idempotency ledgers
- search titles, summaries, tags, and bounded transcript excerpts with FTS5
- inspect diagnostics by kind, call id, event id, item id, session id, or turn
- apply incremental updates with database locking instead of rewriting the whole
  store for every persisted turn

A cautious progression:

1. Cache per-request browser-derived indexes in `AppState` if static inspection
  gets hot before storage itself is a problem.
2. Build a rebuildable `memory-index.sqlite` sidecar from one
  `memory-store.json`.
3. Compare query latency and retrieval fidelity against the current JSON-backed
  retrieval path.
4. Add a storage abstraction in `qsf_memory` only when the backend boundary is
  proven useful.
5. Decide whether diagnostics belong in the same sidecar, a separate sidecar, or
  remain JSONL-only.
6. Promote SQLite from sidecar index to authoritative live memory storage only
  if normal usage shows meaningful growth, write amplification, concurrent
  writer pressure, or realtime latency pressure.
7. Keep JSON export/import available so the memory remains inspectable outside
  the database.

## What Should Stay File-Backed

Do not treat this as a reason to move every artifact into SQLite.

Run artifacts, reports, event logs, and trace logs are research evidence. JSON
and JSONL remain strong formats for sealed artifacts because they are easy to
archive, diff, copy, grep, review, and attach to experiment reports.

This follows the 2026-05-10 decision that memory record and association schema
versioning is per record type and that past memory artifacts are immutable and
never migrated in place. A future SQLite backend must map those per-record
schema-version rules onto relational tables without rewriting historical
evidence.

Small manifest-style state files may also remain file-backed unless they become
a proven bottleneck. The existing manifest-last and snapshot-file pattern is
simple to inspect and reason about.

## Boundaries

- The REST API is the stable inspection/query contract; SQLite schemas are not
  the public interface.
- SQLite should improve inspectability, not hide state transitions or evidence.
- Reducers remain pure. Storage access stays in side-effect layers and returns
  data through actions, tools, DTOs, or query results.
- Sealed historical run artifacts are not migrated in place.
- A SQLite index must be rebuildable from authoritative inputs until the project
  explicitly decides to make SQLite authoritative for live memory.
- If SQLite becomes authoritative for live memory, export and compatibility
  tooling must preserve the project's local-inspection workflow.

## Open Questions

- What record count and association count should trigger SQLite-backed live
  memory as the default path?
- Is write amplification or concurrent-writer coordination enough reason to add
  a SQLite backend before read latency becomes visible?
- Should the first index target static snapshot inspection, live memory query, or
  retrieval-fidelity benchmarking only?
- Should SQLite use one database per session or one database indexing many
  sessions?
- Should diagnostics share a database with memory, or use a separate diagnostics
  index to preserve trust and lifecycle boundaries?
- Which APIs belong in `qsf_browser_server`, `qsf_realtime_server`, or a shared
  inspection service?
- Should model-callable memory tools use the same REST/DTO layer as PowerShell
  and UI clients, or call an internal query service directly?
- How should per-record memory schema versions map onto a relational schema and
  SQLite migrations?
- What is the minimum useful FTS5 search surface without accidentally promoting
  raw diagnostic material into durable memory?

## Retrieval Fidelity Contract

A SQLite-backed retrieval path must reproduce the current `retrieve_memories`
contract before it can replace or stand beside the JSON-backed in-memory path.

Required fidelity:

- same query tokenization inputs and matched-term behavior;
- same score components: recency, keyword, tag, association, importance, and
  reinforcement;
- same 30-day recency decay half-life and timestamp fallback behavior;
- same association-path contribution and cap;
- same relevance gate and skip reason semantics;
- same `limit` behavior, including selected versus omitted candidates;
- same tie-break order: total score descending, then `created_at` descending,
  then memory id ascending;
- same serialized selected/omitted ids and enough score detail to explain any
  divergence.

## Verification Expectations

A later plan or experiment should verify:

- Current JSON-backed retrieval and SQLite-backed retrieval satisfy the retrieval
  fidelity contract above for representative queries.
- Association-neighborhood queries match the current in-memory graph traversal.
- Existing `/api/store/summary`, `/api/memories`, `/api/memories/{id}`, raw, and
  neighborhood responses remain stable when a backend or cached index changes.
- A deleted sidecar index can be rebuilt from authoritative JSON or JSONL inputs.
- PowerShell inspection remains ergonomic enough for normal local debugging.
- Realtime memory query latency remains within the live-loop budget under a
  synthetic large-memory fixture.
- Incremental write behavior avoids whole-store rewrite cost and has a clear
  concurrent-writer story if SQLite becomes authoritative.

Suggested synthetic fixture sizes:

- 1,000 memory records / 5,000 associations
- 10,000 memory records / 50,000 associations
- 50,000 memory records / 250,000 associations

## Promotion Notes

Promote this idea into a `Plan.*.md` when memory growth becomes part of an
active implementation slice, or when realtime memory latency starts affecting
live conversation quality. A plan should begin with a sidecar/index experiment,
not an immediate full migration.

Smallest useful first experiment: build a rebuildable `memory-index.sqlite`
sidecar from one `memory-store.json`, add a SQLite-backed retrieval shim, and
validate it against the retrieval fidelity contract over synthetic 1k / 10k /
50k memory fixtures. Do not change endpoints, live writes, or authoritative
storage in that first slice.

Record a decision only if the project commits to SQLite as an authoritative live
memory backend, commits to REST as the stable memory inspection contract beyond
the existing browser API, or commits to a specific session/database topology.
