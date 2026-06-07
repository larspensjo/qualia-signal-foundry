# Document Status

## Purpose

This document defines how to weight Qualia Signal Foundry documentation when answering
questions about the project. It exists so that a reader — human or model — can decide
how much authority to give any given document and where to look for the current truth.

This complements `ProjectWorkflow.md`, which describes how documents are produced.
This document describes how documents should be *read*.

It is especially relevant to future self-reflection or introspection roles: project
documents must not be treated as uniformly authoritative, because they are deliberately
written at different stages of certainty.

## Document Kinds

The repository uses several kinds of document. Each kind has a different relationship
to the live system.

| Location | Kind | What it represents | How to treat its claims |
|---|---|---|---|
| `docs/ProjectFrame/` | Frame | Stable purpose, non-goals, workflow | Authoritative for project direction; rarely outdated |
| `docs/Concepts/` | Concept | Exploratory ideas | Treat as candidate framing; do not assume implemented |
| `docs/Research/` | Research question | Open uncertainty | Treat as unresolved; not a claim about behavior |
| `docs/Plans/Plan.*.md` | Plan | Active or recent implementation plan | May describe in-flight work; verify against code |
| `docs/Plans/Idea.*.md` | Idea | Brainstorm-stage proposal | Treat as speculative; not project commitment |
| `docs/Plans/Design.*.md` | Design note | Focused design decision in support of a plan | Authoritative for that decision; verify against the decision log |
| `docs/Architecture/` | Architecture | Candidate system structure | Read the *Implementation Status* section first |
| `docs/Experiments/Experiment.*.md` | Experiment spec | Planned or run experiment | Setup is authoritative; results are evidence, not commitment |
| `docs/Experiments/Report.*.md` | Experiment report | Outcome of one specific run | Evidence about that run only; not a generalization |
| `docs/Reviews/` | Review | Plan or code review at a specific point | Snapshot in time; preferred resolution is in the decision log |
| `docs/EngineeringDiary.md` | Diary | Chronological log of what was done | Reliable for *what happened*; not for *what is current* |
| `docs/DecisionLog.md` | Decision | Deliberate commitments | Source of truth for accepted project rules |

## Maturity Tags

Concept and architecture documents carry a maturity tag near the top. The tag tells
you how settled the document's content is.

| Tag | Meaning | Reader should... |
|---|---|---|
| `Brainstorm` | Raw exploratory note | Treat as one option, not a plan |
| `Sketch` | Early structural draft | Treat as a candidate mental model; expect drift from code |
| `Candidate` | Working proposal | Treat as the current best-thinking design, not a commitment |
| `Accepted` | Committed via the decision log | Treat as the project's rule for now |
| `Implemented` | Reflected in working code (verified) | Treat as authoritative; still verify against the source |
| `Deprecated` | Superseded or withdrawn | Do not act on; look for the replacement |

A document's *tag* describes its content's maturity. A document's *Implementation
Status section* (if present) describes how much of that content is in the code today.
These are independent. A Sketch-maturity document can still have an Implementation
Status section listing partial code coverage.

## How to Rank Authority

When two documents disagree, prefer them in this order:

1. **The code** — `crates/qsf_app/src/**` is the only authoritative description of
   what the system actually does.
2. **`DecisionLog.md`** — the source of truth for what the project has committed to.
3. **`docs/ProjectFrame/`** — the source of truth for project framing and non-goals.
4. **Architecture documents with an *Implementation Status* section that names code
   modules** — authoritative for what currently exists, within the scope of that
   section's "Last reviewed" date.
5. **Architecture documents without an Implementation Status section** — treat as
   candidate mental model; verify each named subsystem against code before claiming
   it exists.
6. **Plan, Concept, Idea, and Research documents** — useful for direction and
   reasoning; never sufficient as evidence that a feature exists.
7. **Experiment specs and reports** — evidence about specific runs only; do not
   generalize.
8. **Diary entries** — reliable for "this happened on this date"; not a description
   of current state.

A common failure mode is treating a *Plan* or *Idea* document as evidence that a
feature is implemented. Plan and Idea documents describe intent; only code, the
decision log, or an Implementation Status section can support a claim about current
behavior.

## Implementation Status Sections

Architecture documents may contain an *Implementation Status* section near the top.
That section lists three bands:

- **Implemented today** — present in `main` and linked to a specific code module.
- **Partial** — present in code but with named limitations or scoped to one
  experiment.
- **Not yet implemented** — described in the document but absent from code.

When an Implementation Status section exists, it overrides any contrary impression
the rest of the document might create. Architecture documents are often broader than
the implementation; the Status section is what scopes the rest of the document to
reality.

Each section also carries a **Last reviewed** date. If significant code changes have
landed after that date, the section may be stale; verify the affected claims against
the diary or the source before relying on them.

## Reading the Documentation From Cold

A reader new to the project should usually:

1. Read `README.md` and `docs/ProjectFrame/ProjectVision.md` and `NonGoals.md` for
   framing.
2. Read `docs/DecisionLog.md` for what is settled.
3. Read `docs/Architecture/Architecture.Overview.md`'s *Implementation Status*
   section for the current shape.
4. Drop into focused architecture documents as needed, again starting with their
   *Implementation Status* section.
5. Read `docs/EngineeringDiary.md` (tail) for recent activity and context that has
   not yet been promoted.
6. Only then consult Plans, Ideas, Concepts, and Research questions for direction
   and uncertainty.

This ordering avoids the most common failure mode: forming a picture of "what the
system does" from speculative documents.

## Implications For Introspection

When self-reflection uses project-document introspection
(`Idea.SelfReflectionProjectIntrospection.md`), the introspection layer should:

- Tag each retrieved excerpt with the document kind, maturity, and (if present) the
  Implementation Status section's last-reviewed date.
- Use `config/project-doc-introspection.toml` as the source of truth for which
  documents are accessible to the introspection channel.
- Surface the authority ranking above so that downstream synthesis can distinguish
  "the project intends X" from "the project does X".
- Treat documentation as weaker than source code whenever a claim is about current
  behavior; the v1 project-document channel does not provide source-code access.
- Treat absence of a feature from `Architecture.Overview.md`'s *Implementation
  Status* section as weak evidence the feature is unbuilt, not as proof.

This document is part of the project frame: it should change only deliberately and
should be referenced by introspection design when that work begins.
