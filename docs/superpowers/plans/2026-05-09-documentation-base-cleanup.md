# Documentation Base Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve folder-naming drift, restore the EngineeringDiary, fix broken cross-references, and surface the reducer-pattern commitment in architecture and decision documents.

**Architecture:** Pure documentation edits — no source code changes. All operations are `git mv` renames, file creations, and text edits. Each task is independently committable. Tasks must be executed in order because later tasks reference post-rename paths.

**Tech Stack:** Git (for `git mv`), PowerShell or Bash for verification commands, any text editor.

**Spec:** `docs/superpowers/specs/2026-05-09-documentation-base-design.md`

---

## Files Touched

| Operation | Path |
|-----------|------|
| Rename folder | `docs/Project-frame/` → `docs/ProjectFrame/` |
| Rename folder | `docs/Experiment/` → `docs/Experiments/` |
| Create | `docs/EngineeringDiary.md` |
| Modify | `README.md` |
| Modify | `docs/Architecture/Architecture.RuntimeLoop.md` |
| Modify | `docs/ProjectFrame/ProjectWorkflow.md` *(post-rename path)* |
| Modify | `docs/DecisionLog.md` |

---

## Task 1: Rename the two misnamed folders

**Files:**
- Rename: `docs/Project-frame/` → `docs/ProjectFrame/`
- Rename: `docs/Experiment/` → `docs/Experiments/`

- [ ] **Step 1: Verify the old names exist**

```powershell
Test-Path "docs\Project-frame"   # expect True
Test-Path "docs\Experiment"      # expect True
Test-Path "docs\ProjectFrame"    # expect False
Test-Path "docs\Experiments"     # expect False
```

- [ ] **Step 2: Rename Project-frame → ProjectFrame**

```bash
git mv docs/Project-frame docs/ProjectFrame
```

- [ ] **Step 3: Rename Experiment → Experiments**

```bash
git mv docs/Experiment docs/Experiments
```

- [ ] **Step 4: Verify new names exist and old names are gone**

```powershell
Test-Path "docs\ProjectFrame"   # expect True
Test-Path "docs\Experiments"    # expect True
Test-Path "docs\Project-frame"  # expect False
Test-Path "docs\Experiment"     # expect False
```

- [ ] **Step 5: Verify git sees renames, not deletions**

```bash
git status
```

Expected output: lines like `renamed: docs/Project-frame/... -> docs/ProjectFrame/...`
If you see `deleted:` without a corresponding `new file:`, stop and investigate before committing.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "docs: rename Project-frame to ProjectFrame and Experiment to Experiments

Standardises folder names to PascalCase plural, matching the convention
used in all cross-references across the project documentation."
```

---

## Task 2: Restore docs/EngineeringDiary.md

**Files:**
- Create: `docs/EngineeringDiary.md`

- [ ] **Step 1: Verify the file does not already exist**

```powershell
Test-Path "docs\EngineeringDiary.md"   # expect False
```

- [ ] **Step 2: Create the file with the diary template**

Create `docs/EngineeringDiary.md` with this exact content:

```markdown
# Engineering Diary

Chronological capture for rough thoughts, surprises, observations, and half-formed ideas.
This is Stage 1 of the project workflow. Entries here may later be promoted to concept
notes, research questions, experiments, or decisions.

Good diary entry pattern:

## YYYY-MM-DD

Brief topic line.

Important idea:
- <one idea>

Open question:
- <one question>

Possible next step:
- <one action>
```

- [ ] **Step 3: Verify the file exists**

```powershell
Test-Path "docs\EngineeringDiary.md"   # expect True
```

- [ ] **Step 4: Commit**

```bash
git add docs/EngineeringDiary.md
git commit -m "docs: restore EngineeringDiary.md for Stage 1 workflow capture

Provides a home for rough notes, surprises, and half-formed ideas before
they are promoted to concept notes, research questions, or decisions."
```

---

## Task 3: Fix README.md Documentation section

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Verify the old numbered folders are present**

```bash
grep -n "00-Project-Frame\|10-Concepts\|20-Research\|30-Architecture\|40-Experiments\|50-Decisions\|60-Checklists\|70-Diary" README.md
```

Expected: several matching lines in the Documentation section. If no output, skip this task.

- [ ] **Step 2: Replace the Documentation section code block**

In `README.md`, find and replace this code block (inside the `## Documentation` section):

Old content:
```
docs/00-Project-Frame/
docs/10-Concepts/
docs/20-Research-Questions/
docs/30-Architecture/
docs/40-Experiments/
docs/50-Decisions/
docs/60-Checklists/
docs/70-Diary/
```

New content:
```
docs/ProjectFrame/
docs/Concepts/
docs/Architecture/
docs/Experiments/
docs/Plans/
docs/Research/
```

- [ ] **Step 3: Verify no numbered paths remain in README**

```bash
grep -n "00-\|10-\|20-\|30-\|40-\|50-\|60-\|70-" README.md
```

Expected: no output.

- [ ] **Step 4: Verify the new paths are present**

```bash
grep -n "ProjectFrame\|Experiments\|Plans\|Research" README.md
```

Expected: matching lines in the Documentation section.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "docs: update README Documentation section to match actual folder names

Removes the abandoned numbered-prefix scheme and lists the current
unnumbered PascalCase folders."
```

---

## Task 4: Fix Architecture.RuntimeLoop.md cross-references and add State Update Model section

**Files:**
- Modify: `docs/Architecture/Architecture.RuntimeLoop.md`

This task makes two edits to the same file. Complete both before committing.

### Part A: Fix numbered paths in Related Documents

- [ ] **Step 1: Verify the old numbered paths are present**

```bash
grep -n "30-Architecture\|10-Concepts\|20-Research" docs/Architecture/Architecture.RuntimeLoop.md
```

Expected: eight matching lines in the Related Documents section. If no output, skip Part A.

- [ ] **Step 2: Replace the Related Documents section content**

In `docs/Architecture/Architecture.RuntimeLoop.md`, find the `## Related Documents` section
and replace its bullet list:

Old content:
```
- `docs/30-Architecture/Architecture.Overview.md`
- `docs/30-Architecture/Architecture.AudioLoop.md`
- `docs/10-Concepts/Concept.RealtimePresence.md`
- `docs/10-Concepts/Concept.AssociativeMemory.md`
- `docs/10-Concepts/Concept.ContextBudget.md`
- `docs/10-Concepts/Concept.ToolsAsPerception.md`
- `docs/10-Concepts/Concept.SleepPhase.md`
- `docs/20-Research-Questions/ResearchQuestions.Audio.md`
```

New content:
```
- `docs/Architecture/Architecture.Overview.md`
- `docs/Architecture/Architecture.AudioLoop.md`
- `docs/Concepts/Concept.RealtimePresence.md`
- `docs/Concepts/Concept.AssociativeMemory.md`
- `docs/Concepts/Concept.ContextBudget.md`
- `docs/Concepts/Concept.ToolsAsPerception.md`
- `docs/Concepts/Concept.SleepPhase.md`
- `docs/Research/ResearchQuestions.Audio.md`
```

- [ ] **Step 3: Verify no numbered paths remain in this file**

```bash
grep -n "30-Architecture\|10-Concepts\|20-Research" docs/Architecture/Architecture.RuntimeLoop.md
```

Expected: no output.

### Part B: Add State Update Model section

- [ ] **Step 4: Verify the section does not already exist**

```bash
grep -n "State Update Model" docs/Architecture/Architecture.RuntimeLoop.md
```

Expected: no output.

- [ ] **Step 5: Insert the State Update Model section**

In `docs/Architecture/Architecture.RuntimeLoop.md`, locate `## Candidate Flow` and insert
the following new section immediately before it (with a blank line before and after):

```markdown
## State Update Model

The runtime loop uses a unidirectional, reducer-style state update model:

- State is updated only through pure functions of the form `(State, Event) → State`.
- Side effects (model calls, tool invocations, logging) are isolated from state update
  functions and fed back into the loop as new events.
- Reducers must remain unit-testable without mocks or external dependencies.
- No meaningful state transition should be hidden inside a side effect.

This is a deliberate architectural commitment recorded in `docs/DecisionLog.md`.
See also: `Agents.md`, which carries this as a coding standard.
```

- [ ] **Step 6: Verify the section was inserted**

```bash
grep -n "State Update Model\|Candidate Flow" docs/Architecture/Architecture.RuntimeLoop.md
```

Expected: "State Update Model" appears on a lower line number than "Candidate Flow".

- [ ] **Step 7: Commit**

```bash
git add docs/Architecture/Architecture.RuntimeLoop.md
git commit -m "docs: fix numbered cross-references and add State Update Model section

Updates Related Documents paths from abandoned numbered-folder scheme
to current unnumbered PascalCase paths. Adds a State Update Model section
committing to the unidirectional reducer pattern established in Agents.md."
```

---

## Task 5: Fix ProjectWorkflow.md decision log paths

**Files:**
- Modify: `docs/ProjectFrame/ProjectWorkflow.md`

- [ ] **Step 1: Verify the old paths are present**

```bash
grep -n "Decisions/DecisionLog" docs/ProjectFrame/ProjectWorkflow.md
```

Expected: at least two matching lines (Stage 9 "Use:" block and Document Responsibilities section).
If no output, skip this task.

- [ ] **Step 2: Replace both occurrences**

In `docs/ProjectFrame/ProjectWorkflow.md`, replace all occurrences of:

```
docs/Decisions/DecisionLog.md
```

with:

```
docs/DecisionLog.md
```

There should be exactly two occurrences: one in the Stage 9 body and one in the
Document Responsibilities section header.

- [ ] **Step 3: Verify the old path is gone and new path is present**

```bash
grep -n "Decisions/DecisionLog\|DecisionLog" docs/ProjectFrame/ProjectWorkflow.md
```

Expected: lines containing `docs/DecisionLog.md` only — no `Decisions/` prefix.

- [ ] **Step 4: Commit**

```bash
git add docs/ProjectFrame/ProjectWorkflow.md
git commit -m "docs: fix DecisionLog path in ProjectWorkflow.md

The file lives at docs/DecisionLog.md, not docs/Decisions/DecisionLog.md.
Updates both the Stage 9 reference and the Document Responsibilities header."
```

---

## Task 6: Extend DecisionLog.md with Decision template and first entry

**Files:**
- Modify: `docs/DecisionLog.md`

- [ ] **Step 1: Verify the existing template is present and the Decision template is absent**

```bash
grep -n "Entry Template\|Type: Decision" docs/DecisionLog.md
```

Expected: lines containing "Entry Template" but nothing with a standalone "Type: Decision"
entry template block. If "Entry Template: Decision" is already present, skip Steps 2–3.

- [ ] **Step 2: Add the Decision-type template block**

In `docs/DecisionLog.md`, append the following after the existing `## Entry Template` block
(leave a blank line between the existing template and the new block):

```markdown
## Entry Template: Decision

## YYYY-MM-DD - Short title
Type: Decision
Decision: What was decided.
Context: Why this decision was made.
Consequences: What this commits the project to going forward.
Refs: related docs, experiments, commits
```

- [ ] **Step 3: Add the first Decision entry**

Append the following after the Decision template block (this is the first real entry):

```markdown
## 2026-05-09 - Unidirectional event-reducer-state flow
Type: Decision
Decision: The runtime loop updates state exclusively through pure reducer functions of the
form (State, Event) → State. Side effects are isolated and fed back as events.
Context: Explainable state transitions, pure-function testability, and clean separation
between what happened (events) and what changed (state). Established in Agents.md and
mirrored in Architecture.RuntimeLoop.md.
Consequences: Side-effect-producing operations (model calls, tool invocations, I/O) must
not modify state directly. They must emit events that the reducer then processes.
Refs: docs/Architecture/Architecture.RuntimeLoop.md, Agents.md
```

- [ ] **Step 4: Verify the new content is present**

```bash
grep -n "Entry Template: Decision\|2026-05-09" docs/DecisionLog.md
```

Expected: both lines appear.

- [ ] **Step 5: Commit**

```bash
git add docs/DecisionLog.md
git commit -m "docs: add Decision entry template and first architectural decision

Adds a separate template block for Decision-type entries (distinct from the
existing Implementation and Bug Fix templates). Records the unidirectional
event-reducer-state flow as the first formal architectural decision."
```

---

## Final Verification

Run these checks after all six tasks are complete.

- [ ] **Verify folder structure**

```powershell
Get-ChildItem docs | Select-Object Name
```

Expected names: `Architecture`, `Concepts`, `DecisionLog.md`, `EngineeringDiary.md`,
`Experiments`, `Plans`, `ProjectFrame`, `Research`, `superpowers`

- [ ] **Verify no numbered paths remain anywhere in docs**

```bash
grep -r "00-\|10-\|20-\|30-\|40-\|50-\|60-\|70-" docs/
```

Expected: no output (or only matches inside the superpowers spec, which documents the old scheme by name).

- [ ] **Verify no old folder names appear in cross-references**

```bash
grep -r "Project-frame\|/Experiment/" docs/
```

Expected: no output.

- [ ] **Verify cargo build still passes**

```bash
cargo build
```

Expected: compiles without errors. (No source changes were made, but this confirms
no Cargo.toml or path issues were introduced.)

- [ ] **Verify git log shows six clean commits**

```bash
git log --oneline -8
```

Expected: the six commits from this plan appear at the top.
