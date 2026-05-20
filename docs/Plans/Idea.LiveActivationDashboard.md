# Idea: Live Activation Dashboard

## Status

Brainstorm

## Summary

Qualia Signal Foundry could include a live visual dashboard that shows which parts of
the simulation are active during a run.

The dashboard would not replace logs, traces, reports, or developer diagnostics. Its
purpose would be different: to give a researcher or user an immediate felt sense of
what the system is currently doing.

Examples:

- memories being retrieved or reinforced
- external tools being requested or completed
- model roles being invoked
- speech input and speech output becoming active
- context assembly happening
- sleep-phase or reflection work running
- future goals, tensions, uncertainty, or self-reflection states becoming visible
- latency pressure, interruption, or error states appearing

The dashboard should be visually pleasing and expressive, but it should remain grounded
in actual runtime events.

The primary goal is activation feedback, not exact inspection. A memory, goal, or tool
may be represented by a node or edge lighting up, but the dashboard does not need to
make every activation identify exactly which item was used. Detailed identity can remain
available through logs, traces, and optional click-to-inspect behavior.

## Core Principle

The dashboard should visualize runtime evidence, not create runtime truth.

A useful shape is:

```text
events.jsonl / traces.jsonl
  -> dashboard signal projector
  -> activation snapshot
  -> visual dashboard
```

This keeps the dashboard aligned with the existing unidirectional architecture:

```text
input -> action -> reducer -> state -> render
```

The dashboard should observe emitted events and traces. It should not become a hidden
source of state transitions, model decisions, memory updates, or tool behavior.

It should also avoid forcing precise semantic labels into the main visual field. The
main display can say "memory was active" or "a goal was touched" without immediately
saying "this exact memory or goal was used." Precision belongs in the linked artifacts
and inspection overlays.

## Why This Matters

The project already treats observability as part of the research method, not merely as
debugging. A consciousness simulator that appears continuous but cannot be inspected is
hard to study.

Logs and traces answer precise questions such as:

- Which event happened?
- Which model role was invoked?
- Which memory was retrieved?
- Which tool was called?
- How long did the step take?

A live activation dashboard answers a softer but still useful question:

```text
What does the system feel like it is doing right now?
```

This could help researchers notice patterns that are hard to see in raw logs:

- the system is spending most of its time in model inference
- memory retrieval happens often but rarely affects output
- speech playback dominates the user-visible experience
- tools are used in bursts
- context assembly and model calls create visible latency gaps
- errors or fallback paths are rare but disruptive when they appear

## Relationship To Existing Project Direction

This idea builds directly on existing architecture rather than adding a separate
product surface.

Relevant existing material:

- `Architecture.StateAndObservability.md` already proposes event logs, trace records,
  state snapshots, timeline views, memory views, tool views, model role views, audio
  latency views, and experiment dashboards.
- `Architecture.RuntimeLoop.md` already frames the runtime as an event-oriented,
  reducer-style loop with side effects isolated and fed back as events.
- `Architecture.AudioLoop.md` already calls for visibility into listening, speech
  detection, thinking, speaking, interruption, transcripts, and latency.
- `Architecture.ToolSystem.md` treats tools as observable perception channels with
  explicit request, result, permission, latency, and trace data.
- `Architecture.MemorySystem.md` requires memory retrieval, association paths, memory
  selection, and omitted candidates to be inspectable.
- `Concept.MultiModelMind.md` names cognitive roles such as live presence, attention
  controller, memory curator, tool interpreter, self-monitor, and sleep consolidation.
- `Idea.SelfReflectionProjectIntrospection.md` distinguishes static project
  introspection from dynamic runtime self-state; this dashboard belongs mainly to the
  dynamic runtime side.
- `Idea.VolitionGoalSystem.md` suggests active goals, tensions, satisfaction evidence,
  and unresolved questions that could become visible dashboard signals if that idea is
  implemented.
- `Plan.ReviewedMemoryPromotion.md` suggests a memory-candidate lifecycle that could
  eventually be shown as movement from candidate memory to reviewed durable memory.

The new contribution is not the existence of observability. The new contribution is a
visual, ambient, live presentation layer over observability.

## Candidate Activation Channels

The dashboard could start with a small set of channels derived from existing event
types.

### Input And Speech

Activation sources:

- `AudioInputStarted`
- `AudioPartialTranscript`
- `AudioFinalTranscript`
- `InputReceived`
- `UserInterrupted`

Possible visualization:

- listening region lights up when audio input starts
- partial transcripts create small pulses
- final transcript creates a stronger committed pulse
- interruption creates a distinct visible break or sharp color shift

### Memory

Activation sources:

- `MemoryRetrievalRequested`
- `MemoryRetrieved`
- retrieval traces
- selected context fragments whose source kind is memory

Possible visualization:

- memory nodes glow when retrieved
- association links pulse when traversal paths are available
- stronger retrieval scores produce brighter or longer-lived activation
- repeated access reinforces visible activation for a short window
- omitted memories remain dim or appear as peripheral candidates

Memory activation should have persistence rather than behaving as an instant flash.
When a node or edge is accessed, it should enter an activated visual state that remains
visible for a short time and then decays. If the same node or edge is accessed again
within a reinforcement window, the visible intensity should increase further before
decaying again.

This should be interpreted as recent access intensity, not durable memory importance.
The decay curve belongs to the dashboard signal projection layer unless the runtime
explicitly emits separate memory-strength, salience, or reinforcement data.

### Context And Attention

Activation sources:

- `ContextAssemblyRequested`
- `ContextAssembled`
- context assembly traces
- future attention/focus state events

Possible visualization:

- selected fragments move into an active context band
- budget pressure appears as a fill meter or tightening ring
- omitted fragments fade outward

### Model Roles

Activation sources:

- `ModelRoleRequested`
- `ModelRoleCompleted`
- `ModelRoleFailed`
- role-specific trace operations

Possible visualization:

- model-role regions pulse while active
- different roles have distinct locations or colors
- latency controls pulse duration
- failure produces a visible disruption without hiding the normal event record

Candidate regions:

- live conversational responder
- memory curator or sleep summarizer
- critic/reviewer or self-monitor
- tool interpreter
- deep reflection

### Tools

Activation sources:

- `ToolRequested`
- `ToolCompleted`
- `ToolFailed`
- tool invocation traces

Possible visualization:

- tool requests appear as probes leaving the central runtime
- successful observations return as light or data particles
- failures return as a different color or broken path
- side-effect class and permission class may affect visual shape

### Speech Output

Activation sources:

- `OutputProduced`
- `SpeechPlaybackRequested`
- `SpeechPlaybackStarted`
- `SpeechPlaybackCompleted`

Possible visualization:

- generated output text activates a language center
- speech playback activates an output region
- exact QSF-owned text handoff can be shown as a direct path from output to speech
- speech completion fades the region back to idle

### Sleep And Reflection

Activation sources:

- `SleepPhaseRequested`
- `SleepPhaseCompleted`
- sleep traces
- future reflection or self-monitor events

Possible visualization:

- sleep phase runs as a slower background pattern
- memory candidate creation, consolidation, and review notes appear as lower-frequency
  pulses
- background activity remains visually distinct from live interaction activity

### Latency, Cost, And Errors

Activation sources:

- `LatencyMeasurementRecorded`
- trace latency fields
- model role token usage, if emitted
- model role cost estimates, if emitted
- tool or provider cost estimates, if emitted
- session or experiment cost summaries, if emitted
- `ErrorOccurred`
- failure event variants

Possible visualization:

- latency pressure appears as heat, tension, or ring thickness
- slow stages leave longer trails
- errors flare without being confused with normal activation
- inference cost appears as an optional technical overlay or dedicated cost view
- cost over time appears as a cumulative line, stacked area, or per-role bar timeline
- cost spikes are linked back to the model role, trace, prompt/context size, and output
  that caused them

The dashboard should help assess inference cost, not merely latency. A researcher
should be able to see whether cost is dominated by live response, memory extraction,
sleep-phase consolidation, self-review, tool interpretation, or oversized context
assembly.

Useful cost views:

- cumulative estimated session cost over time
- per-turn cost
- per-model-role cost
- input token, output token, and total token trends
- cost per successful output or experiment run
- comparison between model providers, model roles, memory strategies, or context
  budgets

Cost graphs should remain estimates unless the underlying provider reports exact
billing data. When only token counts and configured prices are available, the dashboard
should label the result as estimated and preserve the pricing assumptions used for the
calculation.

### Goals, Tensions, And Volition

Activation sources:

- future goal-system events
- unresolved question or tension state
- goal progress, blocking, satisfaction, or abandonment events
- sleep or reflection outputs that affect active goals

Possible visualization:

- active goals appear as persistent low-frequency glows rather than short pulses
- blocked goals create visible tension until resolved or abandoned
- goal satisfaction creates a distinct completion pulse
- unresolved questions remain visible as weak background activity that can return to
  attention later

This channel should wait until the volition idea has real runtime artifacts. It should
not invent hidden goals merely for visualization.

### Self-Reflection And Introspection

Activation sources:

- future project-introspection events
- documentation lookup traces
- source or run-artifact inspection traces
- self-monitor or reviewer notes about current behavior

Possible visualization:

- introspection appears as inward-directed activity
- documentation and source lookup should look different from outward tool use
- trace review can appear as the system looking back over its own recent path

This would make self-reflection visible without granting the dashboard authority over
the reflection itself.

### Certainty, Novelty, And Surprise

Activation sources:

- explicit confidence or uncertainty fields, if added to traces
- model-role outputs that mark uncertainty
- memory retrieval misses or weak retrieval scores
- tool observations that contradict selected memories
- novel inputs with no close memory match

Possible visualization:

- grounded responses appear visually calm
- uncertainty appears as tension, jitter, desaturation, or a visible question signal
- novelty creates a short flare when the system appears outside familiar context
- contradictions create a distinct signal that prompts later inspection

This channel should be conservative. It must not claim to measure confidence unless the
underlying runtime actually emits a confidence, uncertainty, novelty, or contradiction
signal.

### Session Continuity And Turn-Taking

Activation sources:

- session start and completion events
- user/system turn boundaries
- listening, processing, speaking, and interruption events
- retrieved memories or unresolved questions carried across sessions

Possible visualization:

- session boundaries appear as breathing points in the display
- cross-session memories briefly glow when they re-enter active context
- turn-taking rhythm shows who currently has the floor
- interruption appears as a break in the expected speech/listen cadence

## Visual Design Directions

The dashboard should be understandable without becoming a literal brain diagram.

## Visual Identity

The reusable UI rules for this identity are promoted into
`Design.SharedVisualLanguage.md`. This section remains the exploratory origin
for the dashboard-specific direction.

The dashboard should have a deliberate visual identity, not merely a functional debug
layout. It should be pleasing to keep open during long sessions while still looking
professional, modern, and research-oriented.

The visual language should balance three qualities:

- ambient enough to support peripheral awareness
- precise enough to remain credible as an observability tool
- restrained enough to avoid looking like decorative fiction

Candidate identity traits:

- clean modern interface with strong alignment, spacing, and readable hierarchy
- dark or low-glare operating mode for long-running observation
- limited, meaningful color system where colors map to channels or states
- subtle motion, glow, and persistence effects that communicate activity without
  visual clutter
- clear typography for overlays, labels, timelines, and inspection panels
- professional instrument-panel polish rather than playful or toy-like styling

The identity should make the dashboard feel like part of the research environment:
expressive, but still grounded in evidence. It can be beautiful, but beauty should help
users notice state, rhythm, cost, latency, and memory activity rather than distract from
them.

The first prototype does not need a complete brand system, but it should establish
early design rules for palette, type scale, spacing, iconography, graph styling,
animation speed, and inspection-panel density. Later dashboard views should reuse those
rules instead of inventing a new style for each visualization.

Possible directions:

### Activation Map

A central map with regions for memory, language, tools, model roles, speech, and
reflection. Regions light up based on recent events and fade over time.

### Memory Constellation

Memory records and associations are shown as a network. Retrieval activates nodes and
links. The active context sits near the center; omitted candidates remain at the edge.

This view does not need to keep the graph visually tidy at all times. If goals,
memories, associations, and external tools eventually form a large graph, the dormant
structure may be complex or chaotic. The useful signal is the temporary lighting of
activation paths, not the ability to read the full graph as a static diagram.

The layout should still use a real dynamic graph-layout algorithm rather than fixed
random positions. A good starting assumption is a force-directed or stress-minimizing
layout where connected nodes attract each other, unrelated nodes repel each other, and
the optimizer runs continuously or incrementally as the graph changes.

Design goals for the layout:

- minimize distance between strongly connected nodes
- preserve temporal stability so nodes do not jump abruptly between frames
- allow slow motion while the optimizer settles
- support new nodes and edges without relaying out the entire graph from scratch
- use edge weights, retrieval scores, or association strengths when available
- keep recently accessed nodes and edges easier to see than dormant structure

This view should treat motion as acceptable and possibly useful. A slowly moving
network can reveal live reorganization, but the motion should be damped enough that
access paths remain trackable.

Candidate algorithms or libraries to investigate later:

- force-directed layout, such as Fruchterman-Reingold or force simulation
- stress majorization for more stable graph drawing
- incremental layout with position reuse between frames and between replay timestamps
- hierarchical or clustered layout for large memory regions
- WebWorker-backed layout for browser prototypes so rendering remains responsive

### Timeline Plus Ambient View

A precise event timeline is shown along the bottom while the main area presents a more
expressive activation field. This keeps visual beauty anchored to the real chronology.

### Latency Flow

Each runtime turn appears as a flow:

```text
input -> memory -> context -> model -> output -> speech
```

Durations are visible as segment lengths, glow persistence, or movement speed.

### Multi-Role Mind View

Each model role appears as a module. Invocations create pulses between modules,
context, memory, and tools. This may fit later multi-model experiments.

### Score Or Instrument View

Each activation channel appears as a track in a visual score. Time flows left to right;
activation is shown through height, color, or density. This is more precise and less
biologically suggestive than a brain-like map, and it naturally supports replay,
scrubbing, and comparison.

### Breathing Room View

The system is treated as a room rather than a brain. Listening opens a window, speaking
lights the room, memory brings objects into view, and sleep dims and reorganizes the
space. This may support an ambient, glanceable display while avoiding literal
neurobiological metaphors.

### Comparative View

Two runs are shown side by side using the same activation channels. This could support
A/B experiments, such as different memory retrieval strategies, context budgets, or
role configurations.

### Cost Trend View

Estimated inference cost is shown over session time, turn number, or replay time. This
view can show cumulative cost, per-turn cost, model-role breakdowns, and token trends.
It should make expensive phases visible without implying that lower cost is always
better than continuity, quality, or responsiveness.

## Detail Philosophy

The dashboard should have two levels of truth:

```text
Ambient activation:
  Something in this subsystem or graph region was active.

Detailed inspection:
  This specific event, trace, memory ID, goal ID, or tool call caused the activation.
```

The ambient view should privilege rhythm, activation, and system feel. It can show nodes
and edges lighting up without requiring labels, exact identifiers, or all neighboring
relationships to be readable.

The detailed view should remain available when needed, but it should not dominate the
first impression. This preserves the dashboard's role as visual feedback rather than a
replacement for logs, traces, reports, or memory inspectors.

## Rendering Technology Options

The first version should probably not start with a heavy render engine.

Candidate progression:

### Phase 1: Static Run Replay

Read a completed `events.jsonl` and `traces.jsonl`, then render an animated replay in
a local web page.

Possible stack:

- plain TypeScript or JavaScript
- Canvas 2D or SVG
- no backend required if run artifacts are loaded from file or served locally

This phase tests event-to-activation mapping before real-time streaming.

### Phase 2: Live Tail Dashboard

Tail the current run's event stream and update activation channels as events arrive.

Possible stack:

- small local HTTP/WebSocket server
- browser UI
- event projector that converts raw QSF events into dashboard signals

This phase tests usefulness during actual experiments.

### Phase 3: Rich Render Layer

If the simple UI proves useful, move to a richer renderer.

Possible options:

- PixiJS for performant 2D effects
- Three.js for 3D or particle-style activation maps
- Bevy for a native Rust visualizer
- egui/wgpu for a Rust-native diagnostics app

The render engine should be chosen after the signal model is proven.

## Candidate Signal Model

The dashboard should not directly expose every raw event as a visual primitive.
Instead, it can project events into compact activation signals.

Candidate structure:

```text
DashboardSignal
  timestamp
  channel
  intensity
  duration_ms
  label
  source_event_id
  trace_id
  metadata
```

Some visual effects need accumulated display state in addition to individual signals.
For example, a memory node may receive several access signals, increase activation, and
then decay gradually. This state should be derived deterministically from the signal
stream and projector parameters rather than stored as runtime truth.

Candidate derived state:

```text
ActivationState
  target_id
  channel
  current_intensity
  last_accessed_at
  decay_half_life_ms
  reinforcement_window_ms
  source_signal_ids
```

For memory graph displays, the visual target may be a memory node, association edge,
cluster, or abstract region. Repeated access inside the reinforcement window can add to
`current_intensity`, while time decay reduces it between signals.

Candidate channels:

```text
speech_input
speech_output
memory
context
model_role
tool
sleep
attention
latency
cost
error
```

The projector can be deterministic and unit-testable. Given a sequence of event
records and trace records, it should produce the same dashboard signals.

## Interaction Modes

The dashboard can begin as a passive display, but some interaction would make it more
useful as a research instrument.

### Scrubbing And Replay

A researcher should eventually be able to pause, rewind, scrub, and change playback
speed. This matters for sleep-phase and reflection runs, where important activity may
take minutes but can be usefully reviewed in seconds.

### Channel Filtering

Researchers should be able to mute channels, adjust thresholds, and change decay curves
when reviewing a run. Memory-focused review may hide speech channels. Latency-focused
review may lower thresholds for slow traces.

These controls should change only the visualization, not the underlying run artifacts.

### Click-To-Inspect

Clicking an activation should reveal the source event, trace, memory ID, tool call, or
model role invocation that caused it. This keeps the dashboard connected to ground
truth without cluttering the ambient display.

### Snapshot Export

Dashboard snapshots or short replay clips could be saved as run artifacts or embedded
in experiment reports. This would let future reviewers see the same visual state that
prompted a research observation.

## Sonification And Accessibility

The dashboard should not assume that visual display is the only useful output.

Possible additions:

- event-driven sounds for important channels such as memory retrieval, tool use,
  speech output, errors, or latency spikes
- ambient state audio that becomes busier or tenser as system activity increases
- a text activity summary such as `Memory: 3 retrievals; Model: live response active;
  Speech: playback requested`
- future haptic cues for urgent latency or error states

Sonification should be optional. It may help peripheral monitoring, but it can also
become distracting during long experiments.

## Activation Semantics

Activation should be intentionally modest in meaning.

It may mean:

- an event of this type recently happened
- a subsystem is currently active or recently active
- a graph region, node, edge, memory class, goal class, or tool category was touched
- a trace reported latency or failure in this subsystem
- a memory, tool, or role participated in the current turn

It should not mean:

- the system is literally conscious
- the visualized region is a biological brain area
- an activation intensity is a scientifically valid mental measurement
- the main view must reveal the exact item identity for every activation
- visual prominence implies importance unless explicitly mapped from runtime data

This distinction matters because a beautiful dashboard can easily become misleading.

## Incremental Phases

### Phase 1: Document And Classify Signals

Define the first activation channels and map them to existing QSF event types.

Test:

- use existing run artifacts to verify that memory, tool, model, speech, and latency
  activity can be derived without changing runtime behavior

### Phase 2: Offline Replay Prototype

Create a small visual replay for one completed run.

Test:

- load `events.jsonl`
- animate event-derived activation over time
- compare replay against the generated report and trace log

### Phase 3: Live Experiment Dashboard

Stream events from an active run into the dashboard.

Test:

- run a text-owned voice-loop experiment
- observe transcript, memory retrieval, context assembly, model role, output, and
  speech playback activation live
- verify that dashboard failure does not affect the experiment

### Phase 4: Memory Constellation View

Add a memory-specific visualization using retrieval results, selected context, and
association data.

Test:

- compare Phase 4 memory/context runs with text-owned voice-loop memory retrieval
- verify that retrieved and omitted memories are visually distinct
- verify that repeated access reinforces node and edge activation before decay
- verify that dynamic layout motion stays slow enough to follow activation paths

### Phase 4B: Cost Trend View

Add inference-cost graphs derived from model role traces, token usage, and configured
pricing assumptions.

Test:

- compare estimated cost by model role across several completed runs
- verify that cost spikes can be traced back to source events and traces
- verify that missing or estimated pricing data is labeled clearly

### Phase 5: Research Review

Evaluate whether the dashboard helps researchers understand runs faster than reports
alone.

Test:

- review the same run with only logs/reports and then with dashboard replay
- note which questions the dashboard answers well and where it misleads

### Phase 6: Comparative And Cross-Session Views

If single-run replay proves useful, add views that compare runs or summarize trends
across sessions.

Test:

- compare runs with different memory sources, context budgets, or model providers
- review whether activation patterns reveal changes that reports alone make hard to
  notice
- keep trend views separate from the live ambient display so they do not overload the
  first dashboard

## Open Questions

- Which activation channels are useful enough for the first prototype?
- Should the dashboard show only current activation, or also short-term history?
- How should it distinguish factual runtime events from interpreted visual effects?
- How should memory association paths be visualized without overwhelming the screen?
- What decay curve and reinforcement window make repeated memory access legible
  without implying durable memory strength?
- Which dynamic graph-layout algorithm gives useful memory topology while preserving
  enough visual stability during live changes?
- Should the first prototype replay completed runs or stream live runs?
- What visual language best fits this project: technical instrument, abstract mind map,
  oscilloscope-like signal view, or cinematic ambient display?
- What visual identity should make the dashboard feel modern, professional, and
  pleasant during long observation sessions?
- Which palette, typography, spacing, and motion rules should be shared across all
  dashboard views?
- Should latency and cost be always visible, optional overlays, or separate dedicated
  views?
- What cost estimate fields are needed in model, tool, and sleep traces to support
  useful graphs over time?
- How should privacy-sensitive event payloads be handled in a dashboard?
- Could the dashboard eventually feed user-visible introspection, or should it remain a
  researcher-only tool?
- What is the minimum signal schema that can support multiple renderers?
- Should the dashboard be a separate process, a browser tab, or an optional native
  window?
- Should activation channels be fixed globally, or configured per experiment?
- Should dashboard snapshots become run artifacts?
- What minimum activation duration is needed for very short events to be perceptible?
- Should activation intensity use linear, logarithmic, or perceptual scaling?
- How should simultaneous activation across many channels be handled?
- Should the dashboard ever be visible to the simulation itself as a form of
  self-perception, or would that create confusing feedback loops?

## Experiment Ideas

### Experiment: Dashboard Versus Logs For Run Comprehension

Give reviewers the same completed run with different review materials:

- logs and reports only
- dashboard replay plus logs and reports

Measure whether dashboard replay improves speed, accuracy, and number of surprising
behaviors noticed.

### Experiment: Channel Usefulness Ranking

Run a prototype with all candidate channels available, then ask reviewers which channels
they used, ignored, or found misleading. Use this to reduce channel count before
investing in richer rendering.

### Experiment: Ambient Monitoring

Let a researcher keep the dashboard visible while doing other work. Test whether they
notice anomalies such as errors, latency spikes, or memory storms without actively
watching the display.

### Experiment: Visual Language Comparison

Render the same run in two styles, such as activation map and score view. Compare which
style is more informative, more intuitive, and less likely to invite over-interpretation.

### Experiment: Sonification Supplement

Compare dashboard replay with and without optional sound. Measure whether sonification
helps researchers notice events they would miss visually, and whether it becomes
annoying over time.

## Risks And Failure Modes

### Misleading Beauty

A polished visualization may make the system look more coherent or conscious than it
is.

Mitigation:

- derive visuals from explicit events and traces
- keep source event links available
- label the dashboard as an interpretation layer
- preserve logs and reports as the source of truth

### Runtime Coupling

The dashboard may accidentally become part of the runtime path.

Mitigation:

- consume append-only event and trace streams
- keep signal projection deterministic
- ensure dashboard failure cannot alter runtime behavior

### Visual Noise

Too many events may make the display unreadable.

Mitigation:

- aggregate into channels
- use decay and filtering
- keep detailed event inspection separate from ambient activation
- allow large graphs to remain visually complex while only active nodes and edges are
  emphasized
- avoid requiring every visible node or edge to carry a readable label in the ambient
  view

### Privacy Leakage

Live visual displays may expose transcripts, tool outputs, or memory contents.

Mitigation:

- support metadata-only display modes
- avoid showing raw payloads by default
- expose labels and references instead of full text unless explicitly requested

### Premature Render Complexity

A sophisticated engine may consume effort before the useful signal model is known.

Mitigation:

- start with replay and simple rendering
- prove the event-to-signal mapping first
- upgrade rendering only after the dashboard helps actual research

### Dashboard-Induced Experiment Bias

If researchers see the dashboard during a live experiment, they may unconsciously adjust
their behavior in response to it. For example, they might speak differently when memory
retrieval looks low.

Mitigation:

- support replay-only review for experiments that need blinding
- record whether the dashboard was visible during a run
- compare live-dashboard and post-run-dashboard review modes

### Over-Interpretation Of Patterns

Researchers may see meaning in visual patterns that are only artifacts of rendering,
aggregation, or decay curves.

Mitigation:

- require important claims to be checked against source events and traces
- expose signal-projector parameters
- compare alternative projections of the same event stream

### Dashboard As Crutch

The dashboard may become so convenient that researchers stop reading logs and traces.

Mitigation:

- make click-to-inspect a core interaction
- keep logs, traces, and reports as the source of truth
- periodically review important runs without the dashboard to preserve log literacy

## Current Leaning

Start with an offline replay prototype that reads existing `events.jsonl` and
`traces.jsonl` artifacts, projects them into a small set of activation channels, and
renders a simple animated dashboard in a browser.

The first target run should probably be a text-owned voice-loop run because it already
contains speech input, memory retrieval, context assembly, model role invocation,
output production, speech playback, and latency events. That gives the dashboard enough
variety to test whether the visual activation idea is actually useful.
