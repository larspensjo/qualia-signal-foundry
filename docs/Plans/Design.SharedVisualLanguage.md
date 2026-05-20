# Design: Shared Visual Language

## Status

Draft

## Summary

Qualia Signal Foundry UI tools should share a visual language inspired by the
activation-map concept art:

![Concept art for the Live Activation Dashboard](../Assets/LiveActivationDashboard/concept-art-activation-map-2026-05-18.jpg)

The reference should guide mood, hierarchy, color, motion, and relationship
rendering. It should not be copied literally. QSF tools should feel like
research instruments over an abstract cognitive field: dark, luminous, precise,
calm, and grounded in runtime evidence.

This design applies to ordinary UI applications, the Live Activation Dashboard,
and the Memory Association Browser. Each tool can use a different layout density,
but the palette, channel vocabulary, graph styling, activation states, and motion
rules should remain recognizable across the system.

## Design Principles

- Use beauty to clarify activity, focus, and relationships.
- Keep the interface low-glare enough for long sessions.
- Make activations legible without implying biological measurement.
- Prefer stable instrument layouts over cinematic composition for working tools.
- Use color semantically; do not let glow become generic decoration.
- Let dense inspection views be calm, text-capable, and readable.
- Treat ambient visualizations as interpretation layers over source artifacts.
- Keep exact evidence available from any prominent visual state.

## Visual Identity

The shared identity is an abstract activation map. It combines:

- deep blue spatial depth
- electric-cyan live signal flow
- warm gold memory and evidence highlights
- translucent technical overlays
- thin framing lines
- luminous nodes and edge trails
- a clear focal hub for current context or current selection

The result should feel more like a scientific instrument than a game, marketing
site, or fantasy control room.

Avoid:

- literal anatomy, brain diagrams, or face silhouettes
- decorative glow that is not tied to state
- heavy gradients as the main visual idea
- one-note blue-only interfaces
- large text floating over busy graph regions
- opaque panels that disconnect the UI from the activation field
- implying that intensity is true mental importance unless the source signal says so

## Palette

Use a dark blue-black foundation with a small set of high-meaning signal colors.
The palette below is a starting token set, not a final brand system.

```text
Foundation
  page_background        #050812
  field_background       #07162A
  field_depth            #0B2C4A
  panel_background       rgba(7, 18, 32, 0.78)
  panel_elevated         rgba(12, 30, 48, 0.88)
  border_subtle          rgba(172, 215, 255, 0.18)
  border_active          rgba(158, 221, 255, 0.56)

Text
  text_primary           #EAF6FF
  text_secondary         #B8CBE2
  text_muted             #6F89A5
  text_warm              #FFE7A8

Signal channels
  active_context         #7DE3FF
  memory                 #FFD76A
  association            #FFB94A
  speech_input           #71D7FF
  speech_output          #F8FBFF
  tool_call              #5CE6C3
  model_role             #BCA8FF
  sleep_reflection       #8D7BFF
  cost_latency           #F2A65A
  error                  #FF5D73
  success                #7AE28A
```

The default UI should be dark, but the signal colors should not all sit in the
same hue family. Gold, cyan, teal, violet, orange, green, and red each carry a
distinct meaning.

## Color Semantics

Color should answer "what kind of activation is this?" before it answers "how
pretty is this?"

| Color family | Meaning | Typical use |
| --- | --- | --- |
| Cyan | live focus, active context, selected flow | central hub, active path, hover focus |
| Gold | memory, retrieval, evidence | memory nodes, selected evidence, recall pulses |
| Amber/orange | association strength, latency, cost pressure | weighted links, timing bands, budget heat |
| Teal | tools and external calls | tool nodes, outbound requests, returned results |
| Violet | model roles and reflection | role lanes, sleep/reflection, internal review |
| White-blue | speech/output emission | waveform, transcript output, response completion |
| Green | completed or healthy state | successful tool completion, valid load state |
| Red | error, risk, failed operation | failed calls, schema errors, blocked paths |

Use intensity, halo size, line width, and persistence to show magnitude. Do not
invent new colors for every subtype; vary shape and label before expanding the
palette.

## Surfaces And Depth

The interface should feel layered over a dark field.

- The main visual field uses the darkest foundation colors.
- Panels are translucent but readable, with blur only if text remains sharp.
- Borders are thin and low-contrast until active or selected.
- Selection rings and active outlines may glow, but inactive chrome stays quiet.
- Toolbars and inspectors use compact spacing and predictable alignment.
- Cards are reserved for repeated data objects, not for whole page sections.

Panel opacity should increase in text-heavy tools such as the Memory Association
Browser. Ambient tools such as the Live Activation Dashboard can let more of the
field show through.

## Typography

Use typography as an instrument surface, not as a poster.

- Prefer a modern sans-serif stack such as `Inter`, `Segoe UI`, or system UI.
- Use tabular numbers for timestamps, counters, token counts, and latency.
- Keep labels short in the visual field.
- Use uppercase sparingly for region labels and compact channel tags.
- Avoid hero-scale text except on true first-viewport presentation surfaces.
- Keep letter spacing at `0`; do not use compressed or negative tracking.

Suggested scale:

```text
caption     11px / 16px
label       12px / 16px
body        14px / 20px
body_large  16px / 24px
panel_title 18px / 24px
view_title  24px / 32px
```

## Iconography

Use restrained line icons for controls and object types. Icons should support
fast scanning, not create a decorative icon set.

Suggested mappings:

```text
memory              circle node or archive-like icon
association         linked nodes
active context      ring, target, or aperture
speech input        waveform in
speech output       waveform out
tool call           plug, terminal, or wrench
model role          layered hexagon or route icon
sleep reflection    moon, orbit, or loop
cost latency        gauge or clock
error               alert triangle
```

When a standard icon library is available in a frontend app, prefer it over
custom SVGs. Use tooltips for icon-only controls.

## Graph And Activation Styling

Graph-like UI should use a small vocabulary of nodes, edges, regions, and flows.

### Nodes

- Dormant nodes are small, low-contrast, and low-opacity.
- Recently activated nodes brighten, gain a halo, and may expand slightly.
- Selected nodes get a crisp cyan ring plus their semantic channel color.
- Memory nodes use warm gold as their primary highlight.
- Tool nodes use teal accents and should look more mechanical than memory nodes.
- Error nodes flare red briefly, then settle into a persistent warning marker.

### Edges

- Dormant edges are thin, dark blue-gray lines.
- Active edges carry moving light along the direction of information flow.
- Memory associations use amber/gold edge accents.
- Live active-context flows use cyan trails.
- Tool request/response paths use teal outbound and green or red return states.
- Edge weight affects line opacity or width, within a capped range.

### Regions

Activation-map views may use stable regions:

```text
center      active context or current selection
upper-left  memory and retrieval
lower-left  tools and external calls
upper-right speech input and output
right       sleep, reflection, or background processing
bottom      timeline, cost, latency, and run rhythm
```

These regions are guides, not hard layout law. Dense tools may collapse them
into tabs, sidebars, or lanes while keeping the same channel colors.

## Motion

Motion should communicate activation, decay, flow, and temporal rhythm.

- Activation pulses should be visible for at least 600 ms.
- Normal decay should be smooth and slow enough to follow.
- Repeated activation should reinforce brightness before fading.
- Flow animation should move along edges, not randomly shimmer.
- Idle motion should be subtle and optional.
- Scrubbing or replay should be deterministic for the same source artifacts.
- Respect reduced-motion preferences by disabling nonessential drift and loops.

Suggested timings:

```text
hover/focus response       100-160 ms
panel transition           140-220 ms
activation pulse minimum   600-900 ms
normal decay half-life     1200-2400 ms
background drift cycle     12000 ms or slower
error flare                300-500 ms plus persistent marker
```

## Layout Modes

### Ambient Dashboard Mode

Used by the Live Activation Dashboard.

- Prioritize peripheral awareness and rhythm.
- Keep text sparse in the main field.
- Use a central active-context hub and subsystem regions.
- Let activations glow and decay visibly.
- Put exact evidence in side panels, timelines, and inspectors.
- Make the display understandable at a glance from several feet away.

### Workbench Browser Mode

Used by the Memory Association Browser and other inspection tools.

- Prioritize search, filtering, exact wording, and provenance.
- Use calmer motion and less transparency than ambient mode.
- Keep graph regions available but subordinate to readable lists and inspectors.
- Use the selected memory or selected neighborhood as the focal hub.
- Show predicates, thresholds, schemas, and source file status directly in the UI.
- Make click-to-evidence more important than ambient beauty.

### Standard Application Mode

Used by ordinary QSF tools.

- Use the dark field, channel colors, and compact panels without requiring a graph.
- Treat navigation, forms, tables, and settings as conventional application UI.
- Use activation colors for state and relationships, not for every button.
- Keep destructive, failed, or risky actions visually distinct from normal activation.
- Reserve high glow for live or selected state.

## Components

Shared components should follow these rules:

- Buttons: compact, icon-first when the command is familiar, with text for major actions.
- Toggles: use channel color only when the toggle controls a channel.
- Sliders: show numeric value and units when controlling time, decay, speed, or thresholds.
- Tabs: quiet by default, active tab marked by a thin luminous line.
- Timelines: use channel bands, event ticks, and a visible playhead.
- Inspectors: show summary first, then source event, trace, provenance, and raw payload.
- Search: always visible in browser/workbench tools.
- Empty states: plain and operational, not illustrative.
- Load errors: red marker plus exact file/schema/context details.

## Evidence And Truthfulness

The visual layer must preserve the project's observability discipline.

- The source artifact or runtime event is the evidence.
- The activation map is an interpretation of that evidence.
- Any selected activation should link to its source event, trace, memory, tool call,
  or projection record when available.
- If a visual state is derived from a threshold, show the threshold in the inspector
  or relevant control.
- Do not hide uncertainty behind polish. Unknown, missing, inferred, or unsupported
  data states should be visible.

## Accessibility And Readability

- Maintain sufficient contrast for text on translucent surfaces.
- Do not rely on color alone; pair channel colors with shape, label, icon, or lane.
- Keep minimum interactive targets around 32px for dense desktop tools and 44px for
  touch-oriented views.
- Respect reduced-motion preferences.
- Make keyboard focus visible with a cyan outline that does not depend on glow alone.
- Keep data tables, inspectors, and text panels readable without zooming.

## Implementation Tokens

Frontend implementations should centralize tokens instead of hardcoding colors and
durations per view.

Candidate CSS custom property shape:

```css
:root {
  --qsf-bg-page: #050812;
  --qsf-bg-field: #07162a;
  --qsf-bg-panel: rgba(7, 18, 32, 0.78);
  --qsf-border-subtle: rgba(172, 215, 255, 0.18);
  --qsf-text-primary: #eaf6ff;
  --qsf-text-secondary: #b8cbe2;
  --qsf-signal-context: #7de3ff;
  --qsf-signal-memory: #ffd76a;
  --qsf-signal-association: #ffb94a;
  --qsf-signal-speech-input: #71d7ff;
  --qsf-signal-speech-output: #f8fbff;
  --qsf-signal-tool: #5ce6c3;
  --qsf-signal-model: #bca8ff;
  --qsf-signal-reflection: #8d7bff;
  --qsf-signal-cost: #f2a65a;
  --qsf-signal-error: #ff5d73;
  --qsf-signal-success: #7ae28a;
  --qsf-radius-panel: 8px;
  --qsf-duration-fast: 140ms;
  --qsf-duration-panel: 180ms;
  --qsf-duration-pulse-min: 700ms;
}
```

Canvas, WebGL, SVG, and native renderers should import from the same semantic token
set where practical.

## Verification Checklist

Use this checklist when a UI adopts the shared visual language:

- The main surface is low-glare and usable for a long session.
- Channel colors match the semantics in this document.
- Memory, tool, speech, model, reflection, error, and context states are visually
  distinguishable without reading a legend constantly.
- Dense text remains readable over panels.
- Activations can be traced back to source evidence.
- Motion communicates state and does not prevent inspection.
- Reduced-motion mode remains usable.
- Screenshot review confirms the UI does not look like a literal brain diagram.
- Human review confirms the tool feels related to the concept art while remaining
  credible as working software.

## Open Questions

- Should the shared token set become a checked-in package once the first browser UI
  exists?
- How bright can ambient glow be before it harms long-session readability?
- Should activation intensity use linear, logarithmic, or perceptual scaling by
  default?
- Which channel colors need adjustment after screenshots from real run artifacts?
- Should exported reports include static screenshots that preserve this visual language?

## Refs

- docs/Plans/Design.LiveActivationDashboard.md
- docs/Plans/Idea.LiveActivationDashboard.md
- docs/Plans/Idea.MemoryAssociationBrowser.md
- docs/Assets/LiveActivationDashboard/concept-art-activation-map-2026-05-18.jpg
