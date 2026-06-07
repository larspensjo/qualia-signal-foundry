# Idea: Associative Project-Doc Context Pointers

## Status

Brainstorm. Preserved from the completed project-doc introspection plan as a
follow-on idea, not as part of the v1 tool channel.

## Purpose

Explore an automatic, association-driven context source for project documents.
Unlike `search_project_docs` and `read_project_doc`, this mechanism would not be
activated by a model tool call. It would be driven by the same memory and
context-selection path that retrieves relevant memories for the current input.

The output should be compact project-doc pointers, not full document bodies.

## Related Work

- `docs/Plans/Design.ProjectDocIntrospection.md`
- `docs/Plans/Plan.ProjectDocIntrospection.md`
- `docs/Plans/Idea.SelfReflectionProjectIntrospection.md`
- `docs/Architecture/Architecture.ContextManagement.md`
- `docs/Architecture/Architecture.MemorySystem.md`
- `docs/Architecture/Architecture.ToolSystem.md`

## Candidate Shape

```text
current input / active focus
  -> associative retrieval cues
  -> project-doc pointer candidates
  -> context budget and authority ranking
  -> selected ProjectDocContextPointer fragments enter active context
  -> model may answer from the pointer metadata, or call read_project_doc
     when body content is needed
```

A `ProjectDocContextPointer` should include only enough material to orient the
model and preserve provenance:

```text
ProjectDocContextPointer
  path
  title
  kind
  maturity_tag
  last_reviewed
  section_hint
  reason_selected
  association_path_or_score
  header_excerpt
  authority_note
  suggested_followup_tool_call
```

`header_excerpt` should be limited to the document title and document
status/header material, such as `## Status`, `## Maturity`, and the
`## Implementation Status` summary when present. It should not include arbitrary
body sections by default. If the model needs actual body text, the expected path
is an explicit `read_project_doc` call with a focused section or token budget.

## Boundaries

- This idea must not make project documents always-present prompt material.
- This idea must not inject complete documents, long plan bodies, or broad search
  excerpts into live context.
- The context assembler, not the project-doc service alone, decides whether a
  pointer enters active context.
- Pointer fragments must carry kind/maturity metadata so brainstorm, plan,
  decision, architecture, and implementation-status material are not flattened
  into one authority level.
- Stable project anchors and accepted decisions may be protected from ordinary
  memory decay, but speculative plans and ideas should remain weaker and clearly
  labeled.
- Full-text reads remain observable tool calls; automatic context injection
  should not hide document inspection that materially affects a reply.

## Planning Work To Flesh Out Later

- Decide whether pointers are generated from the same allowlisted corpus as
  `search_project_docs`, from post-hoc `project_doc_*` traces including the
  `project_doc_influence` signal, from curated stable project facts, or from a
  combination.
- Define the `ContextSource` / `ContextFragment` boundary that turns project-doc
  pointer candidates into active context.
- Define ranking signals: query similarity, association strength, document
  authority, recency, prior successful influence, and diversity.
- Set initial live budgets, for example maximum pointer count and maximum total
  pointer tokens per turn.
- Define trace records for selected and omitted project-doc pointers, including
  why each pointer was selected or rejected.
- Decide whether pointer selection can request asynchronous follow-up reflection
  when the relevant material is too large for the live turn.
- Update `Architecture.ContextManagement.md`, `Architecture.MemorySystem.md`,
  and possibly `Architecture.ToolSystem.md` if the mechanism becomes implemented
  architecture.
- Record a decision only if the project commits to automatic project-doc pointer
  injection as a standing context mechanism.

## Verification Expectations

A later detailed plan should include tests or review checks for these cases:

- A project-self question about a stable boundary selects a compact pointer to a
  frame or decision document.
- A question related to a brainstorm or plan selects a pointer with explicit
  low-authority voicing metadata.
- An unrelated ordinary question selects no project-doc pointers.
- Pointer context contains path/title/status/header metadata, but not full
  document bodies.
- When body content is needed, the responder still uses `read_project_doc` and
  the read remains visible in tool traces.
- Selected and omitted pointer candidates are observable enough for a reviewer to
  understand context influence.

**External testing recommended:** after implementation, run live side-by-side
sessions with associative pointers disabled and enabled. Check whether the
pointers improve grounding without causing prompt bloat, false authority, or
over-eager project self-reference.

## Promotion Notes

Promote this idea into a `Plan.*.md` only after the v1 project-doc tool channel
has produced enough live trace evidence to judge how project-doc lookup behaves
in dialogue. A future plan should include incremental, testable phases and
surface open questions before silently resolving them.
