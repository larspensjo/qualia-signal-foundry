# Design: Memory Schema Versioning

## Status

Accepted

## Summary

Memory records and associations carry an explicit per-record-type schema
version from the first version. When a record is written, its schema version
is fixed for that record. The live runtime only writes and reads the latest
version. Past memory artifacts are sealed and never migrated in place.
Versioned readers for older artifacts live outside the live runtime, in a
separate compatibility module used for replay, debugging, and analysis.

## Scope

This design covers two record types:

```text
MemoryRecord
Association
```

Persistence is not introduced by this design. Phase 4 keeps memory
in-process. The versioning discipline is established so it is already in
place when persistence appears.

The same per-record-type discipline could later be applied to events,
traces, sleep reports, and tool observations, but that is not committed
here.

## Decisions

### 1. Schema version is per record type

`MemoryRecord` and `Association` each carry their own `schema_version: u16`
field, set from a module-level constant. The two types evolve independently.
Bumping the memory record version does not require bumping the association
version.

### 2. Version is an integer constant in code

Each record type defines a constant:

```text
MEMORY_RECORD_SCHEMA_VERSION
ASSOCIATION_SCHEMA_VERSION
```

The version field is a `u16`. Strings such as `"memory.v1"` are not used.

### 3. Pure additions do not bump the version

Adding an optional field with a serde default is not a version bump. The
version is bumped when:

- a field is removed
- a field is renamed
- a field's semantics change (units, scale, meaning)
- the record is structurally restructured

Adding a new variant to an existing enum field is treated as a semantic
change and bumps the version.

### 4. Run artifacts are sealed and never migrated

When memory is persisted, each run's memory artifacts are immutable, the
same way `events.jsonl` and `traces.jsonl` are immutable today. Old runs
are not rewritten when the schema changes.

This preserves replay fidelity. A replay of an old experiment reads exactly
the records the experiment wrote.

### 5. The live runtime only knows the latest version

The in-process memory store reads and writes only the current schema
version. Encountering a record at any other version in the live path is an
error and is logged with enough context to identify the file and offset.

### 6. Versioned readers live in a separate compatibility module

When the live record type reaches v2, a `memory_compat` module (or
equivalent) gains a v1 reader and a `load_as_current` helper that adapts
v1 records into the current shape for tooling.

The choice between backward-compatible parsing, a one-shot in-memory
adapter, and a tagged-enum dispatch is made at v2 time, when the actual
shape of the change is known. This design does not commit to one of those
in advance.

### 7. A future shared cross-run memory store is out of scope

If a working memory store later accumulates state across runs (for example,
as part of sleep-phase consolidation), that store is a working database,
not a research artifact. A different policy may apply there, including
forward migration of the shared store. That decision is deferred until such
a store is actually proposed.

## Rationale

The runs produced by experiments are evidence. Rewriting old runs to match
a newer schema would distort the historical record and undermine the
project's replay goals, including the question "Would the same input
retrieve the same memories?" recorded in
`Architecture.MemorySystem.md`.

Per-record-type versioning matches how the records will actually evolve.
Memory records and associations are independent enough that yoking them to
a single store-wide version would force unnecessary bumps.

Adding the field at v1 is cheap. Retrofitting it later requires either
rewriting old data (which Decision 4 forbids) or inferring the version
from shape, which is fragile.

## Initial Implementation

When the structs are introduced in Phase 4:

```text
1. Define MEMORY_RECORD_SCHEMA_VERSION = 1.
2. Define ASSOCIATION_SCHEMA_VERSION = 1.
3. Add schema_version: u16 to MemoryRecord and Association, defaulting
   to the module constant on construction.
4. Provide a small load function for each type that errors if the
   record's schema_version does not match the current constant.
5. Document the bump policy from Decision 3 next to the constants.
```

No compatibility module is needed at v1. It is introduced when v2 is.

## Open Questions

- The exact error type used when the live runtime sees an off-version
  record. Likely a typed memory error, decided when the load function is
  written.
- Whether to also stamp a small store-level header (for example, a single
  schema-manifest line) when persistence is introduced. Not required by
  this design and can be added at persistence time without breaking it.

## Refs

- docs/Architecture/Architecture.MemorySystem.md
- docs/DecisionLog.md
