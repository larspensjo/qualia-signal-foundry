# Design: Documentation Base Cleanup

Date: 2026-05-09  
Status: Approved

## Context

The project documentation was drafted in multiple sessions, leaving three inconsistencies
before development has started:

1. Folder naming migrated from a numbered scheme (`00-Project-Frame/`) to unnumbered PascalCase,
   but several files still reference the old numbered paths.
2. `docs/Experiment/` (singular) on disk, but every cross-reference uses `Experiments/` (plural).
3. `docs/Project-frame/` uses a hyphenated lowercase name; all other folders use PascalCase.

Additional gaps:
- `docs/EngineeringDiary.md` (Stage 1 capture in the workflow) does not exist.
- The Decision Log template has no entry shape for architectural commitments
  (only Implementation and Bug Fix).
- The reducer / unidirectional-flow commitment in `Agents.md` has no counterpart in the
  architecture documentation.

## Goals

- Make the folder structure consistent: PascalCase, plural collections.
- Restore the EngineeringDiary so Stage 1 of the workflow has a home.
- Fix all broken cross-references to numbered or misnamed paths.
- Surface the reducer/unidirectional-flow commitment in the architecture documents.
- Extend the DecisionLog template to cover the Decision entry type.

## Non-Goals

- Moving `docs/DecisionLog.md` into a `docs/Decisions/` subfolder (YAGNI — one file).
- Numbered or kebab-case folder naming.
- Restructuring the overall workflow or document types.
- Touching `docs/Plans/Plan.FrameworkMVP.md` (deferred by user).

## Section 1: Folder Renames and New Files

### Renames

```
docs/Project-frame/   →  docs/ProjectFrame/
docs/Experiment/      →  docs/Experiments/
```

Both renames use `git mv` so history is preserved.

### New file: docs/EngineeringDiary.md

Create `docs/EngineeringDiary.md` with the date-headed template described in
`docs/ProjectFrame/ProjectWorkflow.md` (Stage 1: Capture). The file starts empty
except for the header block explaining its purpose.

Template content:

```markdown
# Engineering Diary

Chronological capture for rough thoughts, surprises, observations, and half-formed ideas.
This is Stage 1 of the project workflow. Entries here may later be promoted to concept
notes, research questions, experiments, or decisions.

Good diary entry pattern:

## YYYY-MM-DD

<informal notes, open questions, possible next steps>
```

No entries are added at creation. The file is committed as a starting point.

## Section 2: Cross-Reference Fixes

### README.md — Documentation section

The "Documentation" section lists the old numbered folders. Replace with actual names:

```
docs/ProjectFrame/
docs/Concepts/
docs/Architecture/
docs/Experiments/
docs/Plans/
docs/Research/
```

Remove references to `60-Checklists/` and `70-Diary/` (not yet created; add when real).

### docs/Architecture/Architecture.RuntimeLoop.md — Related Documents section

Eight paths use the old numbered scheme. Replace:

| Old | New |
|-----|-----|
| `docs/30-Architecture/Architecture.Overview.md` | `docs/Architecture/Architecture.Overview.md` |
| `docs/30-Architecture/Architecture.AudioLoop.md` | `docs/Architecture/Architecture.AudioLoop.md` |
| `docs/10-Concepts/Concept.RealtimePresence.md` | `docs/Concepts/Concept.RealtimePresence.md` |
| `docs/10-Concepts/Concept.AssociativeMemory.md` | `docs/Concepts/Concept.AssociativeMemory.md` |
| `docs/10-Concepts/Concept.ContextBudget.md` | `docs/Concepts/Concept.ContextBudget.md` |
| `docs/10-Concepts/Concept.ToolsAsPerception.md` | `docs/Concepts/Concept.ToolsAsPerception.md` |
| `docs/10-Concepts/Concept.SleepPhase.md` | `docs/Concepts/Concept.SleepPhase.md` |
| `docs/20-Research-Questions/ResearchQuestions.Audio.md` | `docs/Research/ResearchQuestions.Audio.md` |

### docs/ProjectFrame/ProjectWorkflow.md — Stage 9 decision log path

Change `docs/Decisions/DecisionLog.md` → `docs/DecisionLog.md`.

All other paths in ProjectWorkflow.md are correct after the folder renames.

## Section 3: Reducer Pattern Commitment

### New section in docs/Architecture/Architecture.RuntimeLoop.md

Add a "State Update Model" section (after "Design Intent", before "Candidate Flow"):

```markdown
## State Update Model

The runtime loop uses a unidirectional, reducer-style state update model:

- State is updated only through pure functions of the form `(State, Event) → State`.
- Side effects (model calls, tool invocations, logging) are isolated from state update
  functions and fed back into the loop as new events.
- Reducers must remain unit-testable without mocks or external dependencies.
- No meaningful state transition should be hidden inside a side effect.

This is a deliberate architectural commitment recorded in `docs/DecisionLog.md`.
```

### New Decision Log entry in docs/DecisionLog.md

Extend the DecisionLog template (see Section 4 below) and add the first Decision-type entry:

```markdown
## 2026-05-09 - Unidirectional event-reducer-state flow
Type: Decision
Decision: The runtime loop updates state exclusively through pure reducer functions of the
form (State, Event) → State. Side effects are isolated and fed back as events.
Context: Explainable state transitions, pure-function testability, and clean separation
between what happened (events) and what changed (state). This pattern is established in
Agents.md and mirrored in Architecture.RuntimeLoop.md.
Consequences: Side-effect-producing operations (model calls, tool invocations, I/O) must
not modify state directly. They must emit events that the reducer then processes.
Refs: docs/Architecture/Architecture.RuntimeLoop.md, Agents.md
```

## Section 4: DecisionLog Template Extension

The current DecisionLog template handles Implementation and Bug Fix entry types well.
It needs a third explicit shape for architectural Decision entries.

Add a second template block below the existing one:

```markdown
## Entry Template: Decision

## YYYY-MM-DD - Short title
Type: Decision
Decision: What was decided.
Context: Why this decision was made.
Consequences: What this commits the project to going forward.
Refs: related docs, experiments, commits
```

The existing Implementation / Bug Fix template is unchanged.

## Verification

After implementation, verify:

- [ ] `docs/ProjectFrame/` exists; `docs/Project-frame/` is gone.
- [ ] `docs/Experiments/` exists; `docs/Experiment/` is gone.
- [ ] `docs/EngineeringDiary.md` exists with the diary template.
- [ ] README.md "Documentation" section lists unnumbered folder names only.
- [ ] `Architecture.RuntimeLoop.md` "Related Documents" contains no numbered paths.
- [ ] `ProjectWorkflow.md` Stage 9 references `docs/DecisionLog.md` (no `Decisions/` subfolder).
- [ ] `Architecture.RuntimeLoop.md` contains a "State Update Model" section.
- [ ] `docs/DecisionLog.md` contains a Decision-type template block and the first Decision entry.
- [ ] `cargo build` still passes (no source changes, but confirms no Cargo.toml path issues).
- [ ] `git status` shows no untracked or renamed files after commit.
