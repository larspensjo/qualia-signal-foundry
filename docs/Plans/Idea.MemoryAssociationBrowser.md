# Idea: Memory Association Browser

## Status

Brainstorm

## Summary

Qualia Signal Foundry could include an interactive browser for inspecting durable
memories and their associations after an interactive session, after a sleep session,
or during general memory-state review.

The browser would not be a live activation surface. Its purpose is not to show what is
happening right now, but to make persisted memory state legible, searchable, and
navigable.

The first version can be a static local HTML workbench that loads a
`memory-store.json` file through a local file picker. Later versions may load
browser-friendly projections of sleep or run artifacts and may use richer graph
rendering, including WebGL, if the interaction model proves useful.

## Core Distinction From The Live Activation Dashboard

The live activation dashboard and the memory browser should be treated as related but
different tools.

The live activation dashboard is:

- live
- ambient
- low-text or no-text in its primary view
- focused on runtime activity, rhythm, latency, and subsystem engagement
- useful as an instrument panel during a run

The memory browser is:

- interactive
- post-hoc or state-inspection oriented
- text-capable, including exact wording where useful
- focused on records, associations, provenance, sleep results, and local navigation
- useful as an investigation tool after a session or when reviewing memory health

The two tools may share an artifact projection layer, event loading utilities,
timeline components, visual language, and eventually graph-rendering infrastructure.
Their primary user interfaces should probably remain different.

## Shared Visual Guideline

The memory browser should use the shared QSF visual language captured in
`Design.SharedVisualLanguage.md`. That language uses
[concept-art-activation-map-2026-05-18.jpg](../Assets/LiveActivationDashboard/concept-art-activation-map-2026-05-18.jpg)
as a mood reference.

The shared visual language should carry across both tools:

- low-glare dark operating surface for long review sessions
- luminous blue flow lines for navigation, selected paths, and active focus
- warm gold highlights for memory nodes, retrieval emphasis, and selected evidence
- thin framing lines, translucent overlays, and instrument-panel clarity
- a bright focal hub for the current active context or selected memory neighborhood
- restrained glow and motion that make relationships legible without becoming purely
  decorative

The browser should apply this guideline differently from the live dashboard. The live
dashboard can be ambient and low-text; the browser should be denser, calmer, and more
workbench-like. Lists, filters, memory inspectors, and exact wording panels should use
the same palette and signal vocabulary, but they must prioritize readability over
cinematic atmosphere.

The concept art should inspire color, hierarchy, flow, and focus. It should not force a
literal figure, brain diagram, or full-screen scene into the browser. For this tool, the
selected memory and its local associations are the focal field.

## Inputs And Artifact Contracts

The MVP should have a narrow input contract.

### Required MVP Input

`memory-store.json`

- Location convention: usually `state/text-loop/memory-store.json`, or
  `QSF_STATE_DIR/memory-store.json` when the runtime is configured with a custom state
  directory.
- Browser behavior: read directly through a local file picker or explicit path handoff.
- Required shape: the persisted `MemoryStoreContents` object with `records` and
  `associations` arrays.
- Required record fields: `schema_version`, `id`, `kind`, `title`, `summary`, `tags`,
  `created_at`, `importance`, `reinforcement_count`, `source_reference`, and
  `estimated_tokens`; `last_reinforced_at` may be absent or null for older records.
- Required association fields: `schema_version`, `from_memory_id`, `to_memory_id`,
  `weight`, `reason`, and `last_reinforced_at`.
- Schema behavior: unsupported record or association schema versions should be shown as
  a load error rather than silently interpreted.

The browser should be read-only. It should never write back to the loaded source file,
even if later versions add richer inspection, annotation, or export workflows.

### Deferred Inputs

Sleep reports, run event logs, and trace logs should not be assumed as MVP inputs until
they have a stable browser-facing projection. The full reports and logs remain the
source evidence, but the browser should preferably consume compact projection artifacts
with explicit fields for touched memories, created memories, reinforced memories,
association changes, open questions, and review notes.

The browser should not parse `source_reference` strings heuristically to recover run or
turn structure. If richer provenance matters, the runtime should emit structured
provenance fields or a browser projection that carries them explicitly.

## Why This Matters

Manual inspection of memory files is possible, but it is unwieldy once the store grows.
The memory system is intended to support continuity, association, reinforcement, and
sleep-like consolidation. Those properties become difficult to trust or improve unless
they can be inspected directly.

The browser should help answer questions such as:

- What durable memories exist right now?
- Which memories were created by a sleep session?
- Which memories were reinforced by recent interaction?
- What exact wording does a memory contain?
- Which memories are strongly connected?
- What does the local neighborhood around a memory look like?
- Which memories are old, important, heavily reinforced, orphaned, or unusually large?
- How large is the store, and what does it generally contain?

## Primary Use Cases

### Session And Sleep Review

After an interactive session or sleep session, the user may want to inspect the result.
This does not always require a before/after diff. Often the useful question is simply:

```text
What did this session or sleep phase produce, touch, or surface?
```

Candidate review panels:

- session summary
- memories retrieved during the session
- memories reinforced during the session
- memory candidates created or promoted by sleep
- association candidates created or strengthened
- duplicates skipped
- open questions
- decision candidates
- future context hints
- review notes

A comparison mode may be useful later, but it should not be the default assumption for
sleep-session inspection.

For the first browser version, these panels should wait for a defined sleep or run
projection. The store-only MVP can still expose memories that appear to have sleep
sources through their `source_reference` text, but it should not pretend to know full
sleep-session history from that field alone.

### General Memory State Review

The user may also want to browse the general state of memory without starting from a
particular run.

Useful summary signals:

- record count
- association count
- records by kind
- records by tag
- total estimated tokens
- newest records
- most reinforced records
- highest-importance records
- strongest associations
- orphaned records
- records with stale or missing reinforcement timestamps

Health-like signals should be treated carefully. Objective store-derived signals are
safe for an early browser, such as:

- orphaned record: no association has the record id as `from_memory_id` or
  `to_memory_id`
- missing reinforcement timestamp: `last_reinforced_at` is absent or null
- largest token consumers: records sorted by `estimated_tokens`
- old inactive records: records whose `last_reinforced_at` fallback timestamp is older
  than a documented threshold

Any thresholded signal should show its predicate and default threshold in the UI so it
does not become a mysterious judgment about memory quality.

### Local Association Navigation

The complete memory graph should not be shown as the normal view. Eventually it will
be too large to read meaningfully.

The browser should instead make it easy to start from a specific memory and expand a
local neighborhood:

- selected memory
- strongest direct associations by display strength
- optional second-hop expansion
- incoming and outgoing associations
- filters by weight, date, kind, and tag
- ability to pin a few memories for comparison

This keeps graph navigation intuitive while avoiding the false promise that the full
connection graph can remain readable at scale.

## Interaction Principles

### Pretty Memory Inspection

Clicking a memory should usually open a readable inspector rather than raw JSON.

A candidate memory inspector:

```text
Title
Kind | Created | Last reinforced | Reinforcement count | Importance

Summary / exact wording

Tags

Source
  source reference and any structured provenance that was loaded

Associations
  strongest links
  incoming links
  outgoing links

History
  only facts backed by loaded artifacts
```

Raw JSON can remain available as a secondary debug view, but it should not be the main
interaction.

In the store-only MVP, the inspector can show `created_at`, `last_reinforced_at`,
`reinforcement_count`, and `source_reference`. It should not show per-turn retrieval,
reinforcement, or creation history unless a loaded run or sleep projection supplies
that data explicitly.

### Association Display Strength

When the browser says "strongest association" in a local neighborhood, the MVP should
mean raw `Association.weight`, with `last_reinforced_at` shown beside it. This keeps
the display tied to persisted graph data and avoids hiding query-specific scoring in a
generic edge label.

If the browser later ranks search or retrieval results, that ranking should use the
same retrieval scoring model as the runtime or a named projection of it. Query-aware
retrieval score and local association strength should be labeled separately so the user
can see whether an item is strongly linked, recently reinforced, directly matched, or
important for some other reason.

### Search And Filtering

Keyword search and date filtering are core browser capabilities, not polish.

Candidate controls:

- keyword search across title, summary, tags, and source reference
- deep-link or jump-to-memory by id
- created-date range
- last-reinforced-date range
- delta-since timestamp filter for records created or reinforced after a chosen time
- kind filter
- tag filter
- importance threshold
- reinforcement-count threshold
- association-count filter
- has-associations / orphaned toggle
- sort by newest, oldest, most reinforced, highest importance, strongest connected,
  or largest estimated token count

### Progressive Disclosure

The browser should start from summaries and readable cards, then reveal exact wording,
source references, associations, and raw artifacts as needed.

The user should be able to move from broad overview to specific evidence without
being forced to read storage-format JSON.

## MVP Direction

The first implementation should probably be a static local HTML memory workbench.

Candidate MVP scope:

1. Load a `memory-store.json` chosen by the user or passed through a local file input.
2. Validate record and association schema versions and report load errors clearly.
3. Enforce read-only behavior; the browser must not write back to the source file.
4. Show store summary metrics.
5. Show a searchable, filterable memory list.
6. Show a pretty memory inspector for the selected record.
7. Show incoming and outgoing associations for the selected record, sorted by display
  strength.
8. Support keyword search, memory-id jumps, and basic date filters.
9. Keep raw JSON as a debug escape hatch, not the default inspector.

This phase tests whether the inspection model is useful before investing in WebGL,
sleep/run projections, live streaming, a backend service, or a full graph-layout
system.

## Later Directions

Possible later improvements:

- browser-friendly sleep-result and run-result projection artifacts
- Canvas or WebGL local graph rendering
- force-directed or stress-minimized local association layout
- timeline of retrieval and reinforcement events
- run-aware highlighting of memories touched by a session
- "touched by this run" filter for created, retrieved, or reinforced memories
- negative-space view for asking whether two pinned memories have any connecting path
- sleep-result review workflow
- duplicate or near-duplicate detection surfaces
- memory health diagnostics
- saved focus sets or pinned comparison groups
- side-by-side comparison between two stores or runs
- richer provenance links into event logs, trace logs, reports, and source documents

Editing and curation should remain out of scope for this browser for the foreseeable
future. If the project later needs memory editing, that should be designed as a
separate curation surface with its own safety and audit rules.

## Open Questions

- What exact file names and schemas should browser-friendly sleep-result and run-result
  projection artifacts use?
- Should those projections be emitted by the runtime, generated by an offline tool, or
  both?
- Should the browser live as a checked-in static HTML tool, a generated report, or a
  small local web app after the store-only MVP?
- What structured provenance fields should eventually sit beside `source_reference`?
- When does a local neighborhood graph become more useful than a textual association
  list?
- What mutation evidence would be needed later for precise before/after diff views?
- Which memory-health predicates deserve thresholds, and what defaults are honest?
