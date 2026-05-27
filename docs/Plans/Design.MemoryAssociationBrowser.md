# Design: Memory Association Browser

## Status

Draft

## Summary

The Memory Association Browser (MAB) is a browser-based workbench for
inspecting durable memories and their associations after an interactive
session, after a sleep session, or during general memory-state review.

The MAB is not a live activation surface. It exists to make persisted memory
state legible, searchable, and navigable. The Live Activation Dashboard (LAD)
remains a separate tool focused on runtime activity.

The technical shape is:

```text
Browser + TypeScript + Vite + PixiJS/WebGL + HTML/CSS overlays
  served by a Rust backend (qsf_browser_server)
```

The MAB is read-only. It never writes back to the loaded memory store.

## Relationship To The Idea Document

`Idea.MemoryAssociationBrowser.md` remains the exploratory brainstorm. It
captures motivation, candidate use cases, interaction principles, and future
directions.

This document narrows that idea into an implementable design. Notable shifts
from the idea document:

- The MVP no longer loads `memory-store.json` through a browser file picker.
  A Rust backend reads the file and exposes a visualization API; the frontend
  consumes that API only. This follows the architecture proposal in
  `docs/RustBackendBrowserFrontend.md`.
- The MVP includes a small PixiJS canvas showing the selected memory's focal
  neighborhood. The idea document treated graph rendering as a later layer;
  this design pulls a deliberately minimal version into the MVP so the
  workbench is concrete from day one.
- Memory record, association, schema validation, and store-loading types are
  extracted into a shared crate (`qsf_memory`) before `qsf_browser_server` is
  built, so the post-hoc inspection tool does not pull in the full runtime
  binary. Both `qsf_app` and `qsf_browser_server` depend on the shared crate.
- Dangling associations (associations whose `from_memory_id` or
  `to_memory_id` is not present in the store) are not load errors. They are
  rendered as visible broken edges so the user can investigate them; they
  also contribute to a `broken_associations` count in the store summary.

The idea document's read-only stance, schema-error behavior, association
display strength model, and out-of-scope items (editing, before/after diffs)
all carry over unchanged.

## Design Goals

- Make persisted memory state legible, searchable, and navigable.
- Keep the persisted file format separate from the visualization data contract.
- Keep the MAB read-only; no mutation paths exist.
- Run as a single Rust binary that serves both the API and the built frontend.
- Use the shared QSF visual language in workbench-browser mode (denser,
  calmer, more readable than the ambient dashboard).
- Keep the first PixiJS layer simple: static layout, no force simulation, no
  second-hop expansion.
- Provide a debug escape hatch to raw persisted JSON without making it the
  default view.

## Non-Goals

- No memory editing or curation. Editing remains permanently out of scope; if
  curation is ever needed, it should be a separate tool with its own audit
  rules.
- No before/after diff views, mutation history reconstruction, or
  per-turn retrieval timelines in the MVP.
- No sleep-result or run-result projection consumption in the MVP. The
  browser must not heuristically parse `source_reference` to recover run or
  turn structure.
- No live tailing, file watching, or hot reload in the MVP.
- No force-directed layout, second-hop expansion, full-graph view, pinning,
  comparison groups, or duplicate detection in the MVP.
- No browser-side persistence of memory data. Filter and view state may be
  encoded in the URL; nothing else is stored client-side.

## Architecture

```text
state/text-loop/memory-store.json
  -> qsf_browser_server (Rust, axum)
       reads + validates the store via qsf_memory
       exposes /api/* (visualization DTOs)
       serves the built frontend at /
  -> TypeScript / Vite / PixiJS frontend
       HTML/CSS workbench: list, filters, inspector, association panel
       PixiJS canvas: focal-hub neighborhood for the selected memory

Shared crate:
  qsf_memory  - MemoryRecord, Association, MemoryStoreContents,
                schema validation, load helpers
                (depended on by qsf_app and qsf_browser_server)
```

The architectural boundary is the visualization DTO layer. The frontend never
sees `MemoryRecord` or `Association` directly; it sees `MemoryListItem`,
`MemoryDetail`, `AssociationDisplay`, and `Neighborhood` DTOs defined by
`qsf_browser_server`. The persisted format can evolve without breaking the
frontend, and the frontend can evolve without leaking storage details.

Memory parsing, persistence, schema validation, and loader helpers live in a
new `qsf_memory` crate. `qsf_app` and `qsf_browser_server` both depend on it.
This keeps the post-hoc browser server from pulling in model providers,
audio providers, experiments, or unrelated `qsf_app` compile units, and
matches the long-term direction implied by the project workflow's "name
runtime modules after stable behavior or domain concepts" guideline.

The Live Activation Dashboard is intentionally not served by this crate. LAD
needs real-time data from the running simulation and will eventually be
served by `qsf_app` itself. `qsf_browser_server` is reserved for post-hoc
inspection tools that read sealed artifacts.

## Crate Shape

```text
crates/qsf_memory/
  Cargo.toml
  src/
    lib.rs               crate root
    record.rs            MemoryRecord, MemoryRecordKind, schema constants
    association.rs       Association, schema constants
    store.rs             MemoryStoreContents, load_or_empty, load_existing,
                         raw_records_index
    schema.rs            schema-version validation helpers
    errors.rs            StoreLoadError taxonomy

crates/qsf_browser_server/
  Cargo.toml
  src/
    main.rs              thin entry point: arg parsing, server start
    lib.rs               crate root
    cli.rs               --store, --host, --port args; defaults
    server.rs            axum server scaffolding, route registration,
                         bind-address warnings
    state.rs             AppState: loaded store, raw record index,
                         load error, derived indexes
    memory/
      mod.rs
      dto.rs             MemoryListItem, MemoryDetail, AssociationDisplay,
                         Neighborhood, StoreSummary, LoadError
      mapping.rs         persisted -> DTO conversions (pure functions)
      filters.rs         query parsing, predicate evaluation, sorting
      routes.rs          /api/memory/* and /api/store/* handlers
    health/
      mod.rs
      routes.rs          /api/health
    assets.rs            embedded frontend assets, behind a feature flag
  ui/
    package.json
    vite.config.ts
    tsconfig.json
    index.html
    src/
      main.ts
      api.ts             typed fetch wrappers over /api/*
      types.ts           DTO type mirrors of the Rust DTOs
      state.ts           reducer-style frontend state
      ui/                HTML/CSS components
        toolbar.ts
        filters.ts
        list.ts
        inspector.ts
        loadError.ts
      canvas/
        focalHub.ts      PixiJS scene for the selected memory
        radial.ts        deterministic radial layout
      tokens.css         shared visual language tokens
```

### Build Workflow

`cargo build` and `cargo clippy --all-targets -- -D warnings` MUST work
without npm or Node installed, including for `qsf_browser_server`. The
frontend build is a separate documented step. There is no `build.rs` that
shells out to npm.

Distribution is a two-step build:

```text
1. cd crates/qsf_browser_server/ui && npm install && npm run build
2. cargo build --release -p qsf_browser_server --features embedded-frontend
```

The `embedded-frontend` Cargo feature enables `rust-embed` and includes
`ui/dist/` in the binary. Without the feature, the server still runs and
serves the API, but `/` returns a small placeholder page directing the user
to either build the frontend or use the Vite dev server. This keeps the
Rust toolchain self-sufficient and the frontend optional from Cargo's
perspective.

In development, the Rust server runs on one port (default `127.0.0.1:3939`)
and the Vite dev server on another, with Vite proxying `/api/*` to the
Rust server. The release binary with `embedded-frontend` serves both the
API and the built assets from one port.

## Data Loading

The backend gets the store path from a CLI arg:

```text
qsf_browser_server --store path/to/memory-store.json
```

The default is `state/text-loop/memory-store.json`, matching the existing
runtime convention. To inspect a different store, restart the binary with a
different `--store`. No runtime file picker exists.

### `load_existing` vs `load_or_empty`

The browser server must distinguish "no store at this path" from "empty
store". The existing `qsf_app` loader returns an empty `MemoryStoreContents`
when the file is absent because that is the right runtime behavior on first
boot. For inspection, a missing path is a user error, not a healthy zero
state.

`qsf_memory` exposes two helpers:

```text
load_or_empty(path)  -> Ok(empty) if missing, Ok(parsed) if present
load_existing(path)  -> Err(missing_file) if absent, Ok(parsed) if present
```

`qsf_app` keeps using `load_or_empty`. `qsf_browser_server` uses
`load_existing`. Phase 1 includes a regression test asserting that pointing
`qsf_browser_server` at a missing path yields `missing_file` and a `503`
on data endpoints, not a healthy empty store.

### Two-Pass Load For Diagnostics

`qsf_memory` parses the store in two passes:

1. Parse the file as `serde_json::Value`. This pass captures the set of
   record and association `schema_version` values found, and is used to
   build a `HashMap<MemoryId, serde_json::Value>` index so the raw-JSON
   endpoint can return source-faithful data later (preserving any extra
   fields and original record shape).
2. Deserialize into `MemoryStoreContents` and run structural validation
   (schema versions, duplicate ids, malformed timestamps, dangling
   association references are inspected here).

Validation outcomes:

- Missing file, invalid JSON, unsupported schema versions, duplicate
  memory ids, and structural shape errors are load errors.
- Dangling associations are NOT load errors. They are returned as edges
  with an unresolvable `other_id`, surfaced as broken edges in the
  canvas and in the inspector's association lists, and counted in
  `StoreSummary.broken_associations`.

The server does not watch the file for changes. Reloading requires a
restart.

### Server Binding And Disclosure

The server defaults to listening on `127.0.0.1`. Exposing the API beyond
loopback requires an explicit `--host` (or `--listen <addr>:<port>`)
argument; the CLI does not infer a wider bind from any other flag. When
the bound address is not in the loopback range, the server logs a
startup warning via `engine_logging` indicating that memory contents are
being exposed beyond the local host.

CORS is disabled by default. In development, the Vite dev server proxies
`/api/*` to the Rust server, so cross-origin requests are not needed.
Production builds serve the frontend from the same origin as the API.

The bound address, the store path, and the load result are logged at
startup via `engine_logging`.

## API Surface

All endpoints are `GET`, return JSON, and live under `/api/*`.

```text
GET /api/health
  -> { status: "ok" | "error", load_error?: LoadError }

GET /api/store/summary
  -> record_count, association_count, broken_associations_count,
     total_estimated_tokens,
     records_by_kind, records_by_tag (top N),
     newest[], most_reinforced[], highest_importance[],
     strongest_associations[],
     orphaned_count, missing_last_reinforced_count

GET /api/memories
  query params:
    q                          keyword across title/summary/tags/source_reference
    kind                       filter
    tag                        repeatable
    created_from / _to         ISO 8601
    last_reinforced_from / _to ISO 8601
    delta_since                created-or-reinforced since this timestamp
    min_importance             threshold
    min_reinforcement_count    threshold
    has_associations           true | false
    orphaned                   true | false
    missing_last_reinforced    true | false
    sort                       newest | oldest | most_reinforced |
                               highest_importance | strongest_connected |
                               largest_tokens
    limit, offset              pagination
  -> { total, page, items: MemoryListItem[] }

GET /api/memories/:id
  -> MemoryDetail:
       all persisted fields, plus
       incoming: AssociationDisplay[]
       outgoing: AssociationDisplay[]
     Both lists are sorted by weight descending. Each
     AssociationDisplay carries other_id, other_title (or null if the
     other side is missing), weight, last_reinforced_at, reason.

GET /api/memories/:id/neighborhood?limit=N
  -> Neighborhood:
       center: MemoryListItem,
       edges:  AssociationDisplayEdge[]
                 { from_id, to_id, weight, last_reinforced_at, reason }
       members: MemoryListItem[]  (resolved other-side records;
                                   absent entries indicate broken edges)
     A reciprocal pair of associations appears as two edges, preserving
     potentially different weights, reasons, and timestamps. The canvas
     collapses them visually (bidirectional arrow + summed visual weight)
     but the data remains faithful to storage.

GET /api/memories/:id/raw
  -> the exact persisted JSON for this record, taken from the raw
     `serde_json::Value` index built at load time. Source-faithful:
     extra fields and original record shape are preserved. (Debug
     escape hatch; not the default inspector view.)
```

Notable decisions:

- DTOs are defined by `qsf_browser_server`, not by `qsf_app` or
  `qsf_memory`. Persisted-type changes must pass through the mapping
  layer.
- Filtering, sorting, search, and neighborhood ranking happen in Rust.
  The frontend never receives the full store.
- Association display strength is the raw `Association.weight`. No
  query-aware scoring exists in the MVP.
- Broken edges (`other_id` missing from the store) are returned with
  `other_title: null`. The frontend renders them as visibly broken.
- There is no `POST /api/reload`. Restart the binary to pick up changes.

## Frontend Layout

The workbench uses a two-column shell. The right column is split vertically
into the canvas (top) and the inspector (bottom).

```text
+---------- toolbar (store path, search, sort, filters toggle) ----------+
|                                                                        |
|  memory list   |   focal-hub canvas (PixiJS)                          |
|                |                                                        |
|                |   selected memory at center                            |
|                |   top-N neighbors arranged radially                    |
|                |                                                        |
|                +---------------------------------------------------+    |
|                |   inspector (full width)                          |    |
|                |                                                   |    |
|                |   title, kind/created/reinforced/importance line, |    |
|                |   summary, tags, source,                          |    |
|                |   associations (outgoing, incoming)               |    |
|                |                                                   |    |
+------------------ status bar (counts, load state) -----------------+----+
```

The inspector gets full horizontal width so long summaries and many
associations remain readable. The canvas stays smaller; this matches the
workbench mode's preference for readability over cinematic atmosphere.

### Inspector Sections

```text
Title                                                 [view raw JSON]
Kind  |  Created  |  Last reinforced  |  Reinforcement count  |  Importance

Summary
  exact wording, no truncation

Tags
  pill list

Source
  source_reference text only; no URL synthesis

Associations  (sorted by weight, both directions)
  Outgoing
    target_title         weight    last_reinforced_at
  Incoming
    source_title         weight    last_reinforced_at
```

Clicking any association row makes that memory the new selection: the list
highlight moves, the canvas redraws its focal hub, and the inspector
reloads.

"View raw JSON" opens an overlay over the workbench (not a tab inside the
inspector) showing the response from `/api/memories/:id/raw`.

There is no "History" section in the MVP. Per-turn retrieval, reinforcement,
or creation history requires a structured run or sleep projection artifact
that does not yet exist.

Timestamps display in relative form (`3 days ago`) with the absolute UTC
value on hover.

### Search, Filter, Sort Controls

Toolbar row:

```text
[store path]  [search box: q or id jump]  [sort menu]  [filters toggle]
```

Expandable filter row (collapsed by default):

```text
kind   tags (multi)   created [from][to]   last_reinforced [from][to]
delta_since [datetime]   min importance   min reinforcements
[] orphaned only   [] missing last_reinforced   [] has associations
```

The search field handles both keyword search and id jump: if `q` exactly
matches a memory id, that record is selected; otherwise it is treated as a
keyword query.

All filter, sort, search, and selection state is encoded in the URL. The
current view is shareable and survives reload.

Every thresholded toggle shows its predicate text near the control (per the
idea document: orphaned = no association references this id; missing
reinforcement = `last_reinforced_at` absent or null).

### Canvas Behavior

```text
center node      selected memory, gold, larger, labelled
neighbor nodes   top-N by raw weight from /api/memories/:id/neighborhood
                 (default N = 8, configurable; matches the association
                  list cap)
edges            amber, line width scales with weight (capped range);
                 arrow indicates direction (in, out, bidirectional)
broken edges     edge whose other side is not in the store renders with
                 a dashed amber stroke, a desaturated stub node, and the
                 other-side id (truncated) as the label; clicking it
                 shows the missing id and the reason text without
                 navigating away
layout           static radial: evenly spread angles, radius optionally
                 scaled by weight
interactions     hover neighbor  -> tooltip with title, weight,
                                    last_reinforced_at, reason
                 click neighbor  -> that memory becomes the new center
                 click center    -> no-op
motion           none beyond hover/selection highlight
```

The canvas is deliberately dumb. Force-directed layout, second-hop
expansion, pinning, and ambient motion are deferred until the workbench
shape proves useful.

## Visual Language

The MAB follows `Design.SharedVisualLanguage.md` in workbench-browser mode:

- Dark blue-black foundation, higher panel opacity than the ambient
  dashboard.
- Gold for the selected memory and association edges; cyan for active
  selection and hover focus; amber line width for weight magnitude.
- Restrained motion. Activation pulses and decay belong to the dashboard,
  not the workbench.
- Tabular numbers for counts, weights, timestamps, and token estimates.
- Visible thresholds and predicate labels on every health-like toggle.

The visual tokens (palette, type scale, motion durations) are sourced from
the shared visual language document. They live in
`ui/src/tokens.css` and should be migrated to a shared package once a
second tool starts consuming them.

## Load-Error Behavior

`qsf_memory` distinguishes the following load-error kinds. The Rust type is
`StoreLoadError`; the wire DTO returned by `/api/health` and `503` responses
is `LoadError` and is a one-to-one mirror of the Rust enum.

```text
StoreLoadError / LoadError
  kind: missing_file
      | invalid_json
      | unsupported_schema
      | invalid_store_shape
      | duplicate_memory_ids
  path: string
  message: string
  schema_versions_found?:
    records: int[]
    associations: int[]
  schema_versions_supported?:
    records: int[]
    associations: int[]
  duplicate_ids?: string[]
  shape_errors?: [ { field_path, message } ]
```

`invalid_store_shape` covers valid JSON whose contents fail typed parsing
or structural validation: missing required fields, malformed RFC3339
timestamps, unknown enum variants, wrong numeric types. The two-pass
loader (see Data Loading) collects all observed schema versions before
typed deserialization so `schema_versions_found` is populated even when
typed parsing fails.

`duplicate_memory_ids` is its own kind because the user's first
recourse is to fix the duplicate, not to re-parse.

Dangling associations (associations whose `from_memory_id` or
`to_memory_id` is not in the store) are NOT load errors. They are
counted in `StoreSummary.broken_associations` and rendered as visible
broken edges in the canvas and association lists.

The frontend renders a single load-error screen showing all relevant
fields. The list, canvas, and inspector are not rendered when the
store fails to load.

## Out Of MVP

Carried forward from the idea document as later directions:

- Sleep-result and run-result projection artifacts, once a stable artifact
  contract exists.
- Second-hop expansion in the canvas.
- Force-directed or stress-minimized layout.
- "Touched by this run" highlighting once run projections exist.
- Pinning, comparison groups, saved focus sets.
- Duplicate or near-duplicate content detection (distinct from the
  duplicate-id load error above; this is semantic similarity).
- Memory-health diagnostics beyond the MVP signals (orphaned records,
  missing reinforcement timestamps, largest token consumers via sort,
  and broken associations). An "old inactive records" predicate is
  deferred until the project agrees on an honest default threshold.
- Side-by-side comparison between two stores.
- Live reload or file watching.
- Snapshot or screenshot export.

Memory editing and curation remain permanently out of scope.

## Incremental Phases

Phases are sized so each leaves the MAB usable at the previous phase's
scope.

### Phase 0: Extract `qsf_memory`

- Create `crates/qsf_memory` and move `MemoryRecord`, `MemoryRecordKind`,
  `Association`, `MemoryStoreContents`, schema-version constants, and
  store load/save helpers out of `qsf_app` into it.
- Add `load_existing` alongside `load_or_empty`.
- Implement the two-pass loader: parse to `serde_json::Value` to capture
  observed schema versions and build the raw-record index, then
  deserialize and run structural validation.
- Define the `StoreLoadError` taxonomy.
- `qsf_app` now depends on `qsf_memory` and re-exports through its
  existing module paths to keep its consumers unchanged.

Verify:

- Existing `qsf_app` tests still pass after the move.
- New `qsf_memory` tests cover: missing-file error, invalid-json error,
  unsupported-schema error (with versions found vs supported),
  duplicate-id error, invalid-shape error, dangling-association
  detection (returned as a count, not an error), raw-record index
  preserves extra fields.
- `cargo clippy --all-targets -- -D warnings` and `cargo fmt` are clean.

### Phase 1: Backend skeleton

- Create `crates/qsf_browser_server` with the directory shape above.
- Implement `--store`, `--host`, `--port` CLI args with documented defaults.
- Implement `/api/health` and a `LoadError` DTO that mirrors
  `StoreLoadError`.
- Implement startup load via `qsf_memory::load_existing`.
- Implement loopback-by-default binding, the non-loopback warning, and
  startup logging via `engine_logging`.

Verify:

- `cargo run -p qsf_browser_server -- --store path/to/store` reports
  `status: ok` against a real store.
- A missing path yields `missing_file` on `/api/health` and `503` on
  data endpoints (data endpoints exist as stubs after this phase).
- A malformed or schema-mismatched store surfaces the matching
  `LoadError` kind.
- Default binding is `127.0.0.1`; non-loopback binding logs a startup
  warning.
- External human verification: open `/api/health` in a browser and
  confirm ok and each error path.

### Phase 2: Memory list, summary, detail

- Implement `MemoryListItem`, `MemoryDetail` (with `incoming` and
  `outgoing` `AssociationDisplay` arrays), `StoreSummary` DTOs and their
  mapping functions in `memory/mapping.rs`.
- Implement `/api/store/summary`, `/api/memories`, `/api/memories/:id`,
  `/api/memories/:id/raw`.
- Implement filter, sort, and pagination predicates as pure functions
  over an in-memory store snapshot. The new filters `orphaned` and
  `missing_last_reinforced` are included.
- The raw endpoint serves the raw `serde_json::Value` from the load-time
  index.

Verify:

- Filter and sort logic has unit tests covering each predicate and sort
  key against fixture stores.
- DTO mapping has unit tests; persisted-type fields appear in DTO
  exactly once.
- A regression test asserts the raw endpoint preserves extra fields
  that are absent from the typed `MemoryRecord`.
- Integration test drives the axum router with a fixture store and
  asserts the broken-edge case: a `MemoryDetail` whose `incoming` or
  `outgoing` contains an association with `other_title: null`.
- External human verification: hit the endpoints with curl and a real
  store; check counts, ordering, and broken-edge surfacing.

### Phase 3: Frontend shell (HTML/CSS, no canvas)

- Stand up the Vite project under `crates/qsf_browser_server/ui/`.
- Implement the layout-C shell with toolbar, filter row, list, inspector,
  status bar.
- Implement typed API wrappers in `api.ts` and DTO type mirrors in
  `types.ts`.
- Implement load-error screen handling all `LoadError` kinds.
- Implement URL-encoded filter, sort, and selection state.
- Apply shared visual language tokens.

Verify:

- The frontend loads against a running backend; selecting a memory
  updates the inspector.
- Filters and sort options are reflected in the URL and restored on
  reload.
- Load-error screen renders correctly for each `LoadError` kind.
- External human verification: run the workbench against a real store
  and confirm the workflow feels right.

### Phase 4: Focal-hub canvas with broken edges

- Implement `/api/memories/:id/neighborhood` returning `edges` and
  `members` per the API contract, including unresolved `other_id`
  entries.
- Implement the PixiJS scene with static radial layout, hover tooltip,
  click-to-navigate, and dashed broken-edge rendering.
- Wire the canvas selection into the same URL state as the list.

Verify:

- Selecting a memory from the list and from the canvas produces
  identical state, including URL.
- Neighbors render with weight-scaled line widths and direction arrows;
  broken edges render dashed with the truncated `other_id`.
- Layout is deterministic for a given selection.
- External human verification: navigate through a real store by
  clicking neighbors and confirm the focal hub stays legible, including
  for memories with broken edges.

### Phase 5: Packaging

- Add `rust-embed` behind the `embedded-frontend` Cargo feature so the
  release binary is self-contained without making `cargo build` depend
  on npm.
- Document the two-step build (`npm run build`, then
  `cargo build --release --features embedded-frontend`).
- Add a README usage section for the MAB.

Verify:

- `cargo build` and `cargo clippy --all-targets -- -D warnings` succeed
  without Node or npm installed.
- `cargo build --release -p qsf_browser_server --features
  embedded-frontend` (after `npm run build`) produces a single binary
  that serves both API and frontend from one port.
- The README usage section is enough for a new contributor to launch
  the workbench.

## Testing Strategy

- **Rust unit tests**: filter predicates, sort comparators, DTO mappings,
  schema-error handling. These tests should be the strongest layer because
  filtering and ranking define the MAB's correctness.
- **Rust integration tests**: drive the axum router with fixture stores and
  assert endpoint responses, especially for load-error behavior.
- **Frontend unit tests** (light): URL state encoding/decoding, typed API
  wrappers, neighborhood layout function. Vitest is acceptable if it
  doesn't bloat the toolchain.
- **Frontend visual verification** is manual for now. Automated visual
  regression can be added once the visual language stabilizes.

The project workflow's bias toward reducer-style purity applies: filter,
sort, and mapping logic should be pure functions that take a store snapshot
and a query, and return a result. The axum handlers should be thin wrappers
around those pure functions.

## Documents To Update

Per `docs/ProjectFrame/ProjectWorkflow.md`:

- This document is the design surface; it lives in `docs/Plans/` per the
  existing `Design.*.md` convention.
- `docs/Plans/Idea.MemoryAssociationBrowser.md` should be updated with a
  short note pointing to this design document for items that have been
  decided.
- `docs/EngineeringDiary.md` should receive entries as phases land, per the
  workflow rule that every submitted code change gets a diary entry.
- `docs/DecisionLog.md` records the architecture commitment to the
  Rust-backend / browser-frontend split for post-hoc inspection tools
  (separate from LAD, which will be served by `qsf_app`) as a precondition
  for this design. The entry is added with this design.
- `docs/Architecture/` may gain a new entry once the MAB has shipping code,
  describing the boundary between `qsf_app`, `qsf_memory`, and
  `qsf_browser_server`.

## Open Questions

- What exact file names and schemas should browser-friendly sleep-result
  and run-result projection artifacts use? Carried over from the idea
  document; out of MVP scope.
- Should the shared visual tokens become a checked-in package now, or wait
  until the LAD frontend starts? The MAB will need them either way.
- Should the API support a basic `Accept-Encoding: gzip` path for large
  stores, or is page-size limit enough? Defer until a real store exceeds
  comfort.
- Should the duplicate-id load-error policy be relaxed if a future runtime
  needs to record colliding ids deliberately? Treat as a load error for
  now; revisit if the runtime requires it.

## Refs

- docs/Plans/Idea.MemoryAssociationBrowser.md
- docs/Plans/Design.LiveActivationDashboard.md
- docs/Plans/Idea.LiveActivationDashboard.md
- docs/Plans/Design.SharedVisualLanguage.md
- docs/RustBackendBrowserFrontend.md
- docs/ProjectFrame/ProjectWorkflow.md
- docs/DecisionLog.md
- docs/Assets/LiveActivationDashboard/concept-art-activation-map-2026-05-18.jpg
