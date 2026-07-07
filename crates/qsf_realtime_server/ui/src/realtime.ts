export type RuntimePhase = "idle" | "listening" | "thinking" | "speaking";

export type ConnectionPhase =
  | "idle"
  | "requesting_session"
  | "connecting_media"
  | "ready"
  | "stopping"
  | "error";

export interface SessionAllocationResponse {
  qsf_session_id: string;
  session: SessionConfig;
}

export interface SessionConfig {
  type: "realtime";
  model: string;
  voice: string;
  reasoning_effort: string;
  output_modalities: string[];
  instructions: string;
  audio: {
    output: {
      voice: string;
    };
    input: {
      turn_detection: {
        type: string;
        create_response: boolean;
        interrupt_response: boolean;
      };
    };
  };
}

export interface SdpExchangeRequest {
  qsf_session_id: string;
  offer_sdp: string;
}

export interface SdpExchangeResponse {
  qsf_session_id: string;
  answer_sdp: string;
}

export type RelayEventKind =
  | "user_turn_started"
  | "partial_transcript"
  | "final_transcript"
  | "response_started"
  | "response_completed"
  | "speech_playback_started"
  | "speech_playback_completed"
  | "session_stopped";

export interface RelayEnvelope {
  qsf_session_id: string;
  event_id: string;
  kind: RelayEventKind;
  item_id?: string;
  previous_item_id?: string;
  response_id?: string;
  transcript?: string;
  text?: string;
  status?: string;
  audio_marker?: string;
  payload?: unknown;
}

export interface ProviderDataChannelMessage {
  event_id: string;
  type: string;
  item_id?: string;
  previous_item_id?: string;
  response_id?: string;
  delta?: string;
  transcript?: string;
  text?: string;
  status?: string;
  audio_marker?: string;
  payload?: unknown;
}

export interface TranscriptEntry {
  role: "user" | "assistant" | "system";
  text: string;
}

/// One collapsed row of the diagnostics event ticker. Consecutive events of the
/// same kind merge into a single row so a partial_transcript burst stays readable.
export interface EventLogEntry {
  /// Relay event kind, or a lifecycle marker: "stopping", "stopped", "connection_error".
  kind: string;
  /// Runtime phase after the reducer applied this event — the transition the
  /// reducer actually made, not a kind lookup (response_completed lands in
  /// speaking or idle depending on status). For a collapsed burst this is the
  /// phase after the most recent occurrence.
  phase: RuntimePhase;
  /// Wall-clock ms of the first occurrence in this collapsed run.
  firstAtMs: number;
  /// Wall-clock ms of the most recent occurrence in this collapsed run.
  lastAtMs: number;
  count: number;
}

export const EVENT_LOG_LIMIT = 14;

/// One segment of the runtime-phase swimlane: `phase` holds from `startedAtMs`
/// until the next segment starts (or now, for the last segment).
export interface PhaseSegment {
  phase: RuntimePhase;
  startedAtMs: number;
}

/// Width of the phase-lane display window; also the reducer's pruning horizon.
export const PHASE_LANE_WINDOW_MS = 60_000;

/// Idle time shown at true scale before a gap is compressed; also how long the
/// live trailing idle runs before the lane pauses.
export const PHASE_LANE_IDLE_CAP_MS = 3_000;

/// Fixed lane-time width of a compressed gap's break band.
export const PHASE_LANE_BREAK_LANE_MS = 1_500;

export type VolitionSuppressionReason =
  | "intensity"
  | "protected_no_opportunity"
  | "anti_nag_repeat"
  | "non_renderable_output"
  | "below_qualification_threshold";

/// Narration visibility of a goal. Mirrors `qsf_volition::GoalVisibility` (snake_case). A
/// `subconscious` goal biases selection and arbitration identically to a `conscious` one but is a
/// background disposition surfaced only on introspection or when forced.
export type VolitionGoalVisibility = "conscious" | "subconscious";

export interface VolitionGoalStatusSummary {
  id: string;
  title: string;
  salience: number;
  cooldownUntilTick: number | null;
  lastActivatedTick: number | null;
  visibility: VolitionGoalVisibility;
}

/// How the arbitration winner was exposed in this turn's model-visible text. Mirrors
/// `AmbientExposure` (snake_case).
export type VolitionAmbientExposure =
  | "ordinary"
  | "reduced_subconscious"
  | "forced_surfaced_subconscious";

/// Which recorded fact forces a subconscious goal to surface. Mirrors
/// `qsf_volition::ForcingCondition`, which serde-tags internally on `kind`.
export type VolitionForcingCondition =
  | { kind: "rendered_initiative"; tick: number; renderedRef: string | null }
  | { kind: "coherence_conflict"; candidateId: string; candidateTitle: string; tick: number };

/// One subconscious goal forced to surface, with the condition forcing it. Mirrors
/// `qsf_volition::ForcedSurfacing`.
export interface VolitionForcedSurfacing {
  goalId: string;
  condition: VolitionForcingCondition;
}

export interface VolitionInitiativeSummary {
  goalId: string;
  goalTitle: string;
  outputKind: string;
}

export interface VolitionStateInspectionCapture {
  mode: string;
  tick: number;
  activeGoals: VolitionGoalStatusSummary[];
  acceptedGoals: VolitionGoalStatusSummary[];
  blockedGoals: VolitionGoalStatusSummary[];
  cooldownGoals: VolitionGoalStatusSummary[];
  retiredGoals: VolitionGoalStatusSummary[];
  pendingCandidateCount: number;
  acceptedCandidateCount: number;
  lastInitiativeSummaries: VolitionInitiativeSummary[];
}

export interface VolitionModeBiasOutcomeCapture {
  goalId: string;
  goalTitle: string;
  effectiveTier: number;
  biasedTier: number;
  protected: boolean;
}

export interface VolitionTurnWinnerSummary {
  winnerGoalId: string;
  winnerGoalTitle: string;
  winnerEffectiveTier: number;
  winnerBiasedTier: number;
  protectedTierActive: boolean;
  winnerVisibility: VolitionGoalVisibility;
}

export type KeywordWeightClass = "weak" | "normal" | "strong";

export interface VolitionBelowThresholdSummary {
  goalId: string;
  goalTitle: string;
  matchedKeywords: Array<{ term: string; weightClass: KeywordWeightClass }>;
  matchStrength: number;
}

export interface VolitionTurnDecisionSummary {
  winner: VolitionTurnWinnerSummary | null;
  qualificationThreshold: number;
  belowThreshold: VolitionBelowThresholdSummary[];
  modeBiasOutcomes: VolitionModeBiasOutcomeCapture[];
  selectedGoalIds: string[];
  omittedOrSuppressedGoalIds: string[];
  shapingIntensity: string;
  lastInitiativeOutputKind: string | null;
  lastInitiativeSurfaced: boolean;
  lastInitiativeSuppressionReason: VolitionSuppressionReason | null;
  lastInitiativeRenderedLinePresent: boolean;
  ambientExposure: VolitionAmbientExposure;
  subconsciousSelectedCount: number;
}

/// The four functional-signal kinds. Display-only instrument readouts derived from recorded
/// volition state; never felt states. Mirrors `qsf_volition::signals::SignalKind` (snake_case).
export type VolitionSignalKind = "coherence_decline" | "frustration" | "satisfaction" | "boredom";

/// Which guard admitted a `boredom` signal past the cold-start check. Mirrors
/// `qsf_volition::signals::BoredomGuard` (snake_case).
export type VolitionBoredomGuard = "prior_activation" | "elapsed_ticks";

/// Why a candidate was declined by the coherence engine. Mirrors `qsf_volition::DeclineReason`,
/// which serde-tags internally on `kind`.
export type VolitionDeclineReason =
  | { kind: "conflicting_goal"; goalId: string }
  | { kind: "protected_floor" };

/// A `(goal id, salience)` pair recorded in boredom evidence.
export interface VolitionGoalSalience {
  goalId: string;
  salience: number;
}

/// Structured, per-kind evidence that justifies a functional signal. The wire form is serde
/// externally tagged (`{ "frustration": { … } }`); this discriminated union flattens each
/// variant's fields alongside a `kind` tag so consumers can switch on `kind` directly.
export type VolitionSignalEvidence =
  | {
    kind: "coherence_decline";
    candidateTitle: string;
    conflict: VolitionDeclineReason;
    rationale: string;
    tick: number;
  }
  | {
    kind: "frustration";
    goalId: string;
    blockedCount: number;
    lastBlockedTick: number;
    lastActivatedTick: number;
  }
  | {
    kind: "satisfaction";
    goalId: string;
    lastSatisfiedTick: number;
    evidenceRef: string;
  }
  | {
    kind: "boredom";
    inspected: VolitionGoalSalience[];
    threshold: number;
    guard: VolitionBoredomGuard;
  };

/// One derived functional signal: its `kind`, a display `intensity` in `[0, 1]`, and the
/// structured `evidence` that justifies it. Mirrors `qsf_volition::signals::FunctionalSignal`.
export interface VolitionFunctionalSignal {
  kind: VolitionSignalKind;
  intensity: number;
  evidence: VolitionSignalEvidence;
}

export interface VolitionInspectionCapture {
  qsfSessionId: string;
  exchangeIndex: number;
  capturedAt: string;
  responseCreateEventRef: string;
  inspection: VolitionStateInspectionCapture;
  decision: VolitionTurnDecisionSummary | null;
  /// Display-only functional signals riding the capture. Empty for older captures that
  /// predate the field (the parser defaults a missing `signals` key to an empty list).
  signals: VolitionFunctionalSignal[];
  /// Subconscious goals forced to surface this run, with the condition forcing each. Empty for
  /// older captures that predate the field. Operator-panel only — used to badge which
  /// subconscious goals surfaced and why.
  forcedSurfaced: VolitionForcedSurfacing[];
}

export interface VolitionPanelRow {
  label: string;
  value: string;
}

export interface VolitionPanelSection {
  title: string;
  rows: VolitionPanelRow[];
}

export interface VolitionPanelModel {
  kind: "empty" | "snapshot" | "decision";
  headline: string;
  banner: string;
  sections: VolitionPanelSection[];
}

export interface ConversationState {
  connection: ConnectionPhase;
  phase: RuntimePhase;
  /// Local microphone gate. When true, the browser stops transmitting the user's
  /// audio (the mic track is disabled) while the session, the assistant's audio, and
  /// volition initiative all stay live. Persists across stop so it can be pre-armed
  /// before a call begins.
  muted: boolean;
  sessionId: string | null;
  transcript: TranscriptEntry[];
  liveTranscript: string;
  responseDraft: string;
  /// Newest-first collapsed history of relay/lifecycle events (see EventLogEntry).
  /// Kept after stop for post-hoc review; cleared when a new session is allocated.
  eventLog: EventLogEntry[];
  /// Oldest-first runtime-phase history, pruned to the lane window. Empty means
  /// "idle so far". Kept after stop; cleared when a new session is allocated.
  phaseTimeline: PhaseSegment[];
  error: string | null;
  warning: string | null;
  latestTurnContext: TurnContextCapture | null;
  latestVolitionState: VolitionInspectionCapture | null;
  latestTokenUsage: TokenUsageSnapshot | null;
}

export type ConversationAction =
  | { type: "session_requested" }
  | { type: "mute_toggled" }
  | { type: "session_allocated"; sessionId: string }
  | { type: "connection_ready" }
  | { type: "provider_envelope"; envelope: RelayEnvelope; atMs: number }
  | { type: "connection_error"; message: string; atMs: number }
  | { type: "server_status"; sessionId: string; degraded: boolean; detail: string | null }
  | { type: "stop_requested"; atMs: number }
  | { type: "stopped"; atMs: number }
  | { type: "turn_context_captured"; capture: TurnContextCapture }
  | { type: "volition_state_captured"; capture: VolitionInspectionCapture }
  | { type: "token_usage_captured"; snapshot: TokenUsageSnapshot };

/// Server-originated status message pushed over the events socket, distinct
/// from relay acks by its `kind` discriminator.
export interface SidebandStatusMessage {
  kind: "sideband_status";
  qsf_session_id: string;
  degraded: boolean;
  detail: string | null;
}

/// A snapshot of the messages sent to the provider at the start of a turn,
/// pushed over the events socket with `kind: "turn_context"`.
export interface TurnContextCapture {
  qsfSessionId: string;
  exchangeIndex: number;
  capturedAt: string; // RFC 3339 string
  requestHash: string;
  messages: unknown[];
}

/// Token counts split by the classes the Tokens panel displays. "Fresh" input
/// excludes cached tokens; `cachedInput` is the full cached prefix (audio + text).
export interface TokenClassCounts {
  textInput: number;
  audioInput: number;
  cachedInput: number;
  textOutput: number;
  audioOutput: number;
}

/// Accumulated usage of one (role, model) pair, as aggregated server-side.
export interface ModelTokenUsage {
  modelId: string;
  role: string;
  calls: number;
  counts: TokenClassCounts;
}

/// One session-scoped token ledger snapshot pushed over the events socket.
export interface TokenUsageSnapshot {
  qsfSessionId: string;
  models: ModelTokenUsage[];
}

const DEFAULT_DEGRADED_WARNING =
  "The server lost its control channel; replies may not arrive. Check the server logs.";

export const MICROPHONE_AUDIO_CONSTRAINTS: MediaTrackConstraints = {
  echoCancellation: true,
  noiseSuppression: true,
  autoGainControl: true,
};

export const DEFAULT_SESSION_CONFIG: SessionConfig = {
  type: "realtime",
  model: "gpt-realtime-2",
  voice: "marin",
  reasoning_effort: "medium",
  output_modalities: ["audio"],
  instructions:
    "Speak briefly. Keep the browser UI informed, keep secrets server-side, and preserve the QSF trust boundary.",
  audio: {
    output: {
      voice: "marin",
    },
    input: {
      turn_detection: {
        type: "server_vad",
        create_response: false,
        interrupt_response: false,
      },
    },
  },
};

export const INITIAL_STATE: ConversationState = {
  connection: "idle",
  phase: "idle",
  muted: false,
  sessionId: null,
  transcript: [],
  liveTranscript: "",
  responseDraft: "",
  eventLog: [],
  phaseTimeline: [],
  error: null,
  warning: null,
  latestTurnContext: null,
  latestVolitionState: null,
  latestTokenUsage: null,
};

export function reduceConversationState(
  state: ConversationState,
  action: ConversationAction,
): ConversationState {
  switch (action.type) {
    case "session_requested":
      return {
        ...state,
        connection: "requesting_session",
        error: null,
        warning: null,
      };
    case "mute_toggled":
      return {
        ...state,
        muted: !state.muted,
      };
    case "session_allocated":
      return {
        ...state,
        connection: "connecting_media",
        sessionId: action.sessionId,
        error: null,
        eventLog: [],
        phaseTimeline: [],
        latestTurnContext: null,
        latestVolitionState: null,
        latestTokenUsage: null,
      };
    case "connection_ready":
      return {
        ...state,
        connection: "ready",
        error: null,
      };
    case "provider_envelope":
      return applyRelayEnvelope(state, action.envelope, action.atMs);
    case "connection_error":
      return {
        ...state,
        connection: "error",
        error: action.message,
        eventLog: appendEventLog(state.eventLog, "connection_error", action.atMs, state.phase),
      };
    case "server_status":
      // Ignore status for a session other than the active one: a queued message
      // from a closed socket must not re-raise a warning after stop or during a
      // newly allocated session.
      if (action.sessionId !== state.sessionId) {
        return state;
      }
      return {
        ...state,
        warning: action.degraded ? (action.detail ?? DEFAULT_DEGRADED_WARNING) : null,
      };
    case "stop_requested":
      return {
        ...state,
        connection: "stopping",
        eventLog: appendEventLog(state.eventLog, "stopping", action.atMs, state.phase),
      };
    case "stopped":
      return {
        ...state,
        connection: "idle",
        phase: "idle",
        sessionId: null,
        liveTranscript: "",
        responseDraft: "",
        eventLog: appendEventLog(state.eventLog, "stopped", action.atMs, "idle"),
        phaseTimeline: appendPhaseTimeline(state.phaseTimeline, "idle", action.atMs),
        warning: null,
      };
    case "turn_context_captured":
      // Ignore captures for a session other than the active one: a queued
      // message from a closed socket must not overwrite state after stop or
      // during a newly allocated session.
      if (action.capture.qsfSessionId !== state.sessionId) {
        return state;
      }
      return {
        ...state,
        latestTurnContext: action.capture,
      };
    case "volition_state_captured":
      // Ignore captures for a session other than the active one: a queued
      // message from a closed socket must not overwrite state after stop or
      // during a newly allocated session.
      if (action.capture.qsfSessionId !== state.sessionId) {
        return state;
      }
      return {
        ...state,
        latestVolitionState: action.capture,
      };
    case "token_usage_captured":
      // Ignore captures for a session other than the active one: a queued
      // message from a closed socket must not overwrite state after stop or
      // during a newly allocated session.
      if (action.snapshot.qsfSessionId !== state.sessionId) {
        return state;
      }
      return {
        ...state,
        latestTokenUsage: action.snapshot,
      };
  }
}

/// Parse a server→browser events-socket message, returning a sideband status
/// message when present and `null` for anything else (e.g. relay acks).
export function parseSidebandStatusMessage(raw: string): SidebandStatusMessage | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isRecord(parsed) || parsed.kind !== "sideband_status") {
    return null;
  }
  if (typeof parsed.qsf_session_id !== "string" || typeof parsed.degraded !== "boolean") {
    return null;
  }
  return {
    kind: "sideband_status",
    qsf_session_id: parsed.qsf_session_id,
    degraded: parsed.degraded,
    detail: typeof parsed.detail === "string" ? parsed.detail : null,
  };
}

/// Parse a server→browser events-socket message, returning a `TurnContextCapture`
/// when the message has `kind: "turn_context"` and all required fields are
/// present and correctly typed. Returns `null` for any other message.
///
/// Wire format uses snake_case field names (the Rust struct has no `rename_all`
/// attribute); this function maps them to camelCase TypeScript properties.
export function parseTurnContextMessage(raw: string): TurnContextCapture | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isRecord(parsed) || parsed.kind !== "turn_context") {
    return null;
  }
  const qsfSessionId = parsed.qsf_session_id;
  const exchangeIndex = parsed.exchange_index;
  const capturedAt = parsed.captured_at;
  const requestHash = parsed.request_hash;
  const messages = parsed.messages;
  if (
    typeof qsfSessionId !== "string" ||
    typeof exchangeIndex !== "number" ||
    typeof capturedAt !== "string" ||
    typeof requestHash !== "string" ||
    !Array.isArray(messages)
  ) {
    return null;
  }
  return {
    qsfSessionId,
    exchangeIndex,
    capturedAt,
    requestHash,
    messages,
  };
}

/// Parse a server→browser events-socket message, returning a volition capture
/// when the message has `kind: "volition_state"` and all required fields are
/// present and correctly typed. Returns `null` for any other message.
///
/// Wire format uses snake_case field names; this function maps them to camelCase
/// TypeScript properties.
export function parseVolitionStateMessage(raw: string): VolitionInspectionCapture | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isRecord(parsed) || parsed.kind !== "volition_state") {
    return null;
  }
  const qsfSessionId = parsed.qsf_session_id;
  const exchangeIndex = parsed.exchange_index;
  const capturedAt = parsed.captured_at;
  const responseCreateEventRef = parsed.response_create_event_ref;
  const inspection = parsed.inspection;
  const decision = parsed.decision;
  if (
    typeof qsfSessionId !== "string" ||
    typeof exchangeIndex !== "number" ||
    typeof capturedAt !== "string" ||
    typeof responseCreateEventRef !== "string" ||
    !isVolitionStateInspectionCapture(inspection)
  ) {
    return null;
  }
  const parsedInspection = convertVolitionStateInspectionCapture(inspection);
  if (parsedInspection === null) {
    return null;
  }
  if (decision !== null && !isVolitionTurnDecisionSummary(decision)) {
    return null;
  }
  const parsedDecision = decision === null ? null : convertVolitionTurnDecisionSummary(decision);
  if (decision !== null && parsedDecision === null) {
    return null;
  }
  return {
    qsfSessionId,
    exchangeIndex,
    capturedAt,
    responseCreateEventRef,
    inspection: parsedInspection,
    decision: parsedDecision,
    signals: parseFunctionalSignals(parsed.signals),
    forcedSurfaced: parseForcedSurfaced(parsed.forced_surfaced),
  };
}

/// Parse the top-level `forced_surfaced` array of a `volition_state` message. Defensive and
/// non-fatal, like `parseFunctionalSignals`: a missing key or malformed entry yields an empty
/// list / dropped entry rather than nulling out the capture — forced-surfacing is display-only.
function parseForcedSurfaced(value: unknown): VolitionForcedSurfacing[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const forced: VolitionForcedSurfacing[] = [];
  for (const entry of value) {
    const parsed = parseForcedSurfacing(entry);
    if (parsed !== null) {
      forced.push(parsed);
    }
  }
  return forced;
}

function parseForcedSurfacing(value: unknown): VolitionForcedSurfacing | null {
  if (!isRecord(value) || typeof value.goal_id !== "string") {
    return null;
  }
  const condition = parseForcingCondition(value.condition);
  if (condition === null) {
    return null;
  }
  return { goalId: value.goal_id, condition };
}

function parseForcingCondition(value: unknown): VolitionForcingCondition | null {
  if (!isRecord(value) || typeof value.kind !== "string" || typeof value.tick !== "number") {
    return null;
  }
  if (value.kind === "rendered_initiative") {
    return {
      kind: "rendered_initiative",
      tick: value.tick,
      renderedRef: typeof value.rendered_ref === "string" ? value.rendered_ref : null,
    };
  }
  if (
    value.kind === "coherence_conflict" &&
    typeof value.candidate_id === "string" &&
    typeof value.candidate_title === "string"
  ) {
    return {
      kind: "coherence_conflict",
      candidateId: value.candidate_id,
      candidateTitle: value.candidate_title,
      tick: value.tick,
    };
  }
  return null;
}

function parseTokenClassCounts(value: unknown): TokenClassCounts | null {
  if (!isRecord(value)) {
    return null;
  }
  const { text_input, audio_input, cached_input, text_output, audio_output } = value;
  if (
    typeof text_input !== "number" ||
    typeof audio_input !== "number" ||
    typeof cached_input !== "number" ||
    typeof text_output !== "number" ||
    typeof audio_output !== "number"
  ) {
    return null;
  }
  return {
    textInput: text_input,
    audioInput: audio_input,
    cachedInput: cached_input,
    textOutput: text_output,
    audioOutput: audio_output,
  };
}

/// Parse a server→browser events-socket message, returning a token-usage snapshot
/// when the message has `kind: "token_usage"` and all required fields are present
/// and correctly typed. Returns `null` for any other message.
///
/// Wire format uses snake_case field names; this function maps them to camelCase
/// TypeScript properties.
export function parseTokenUsageMessage(raw: string): TokenUsageSnapshot | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!isRecord(parsed) || parsed.kind !== "token_usage") {
    return null;
  }
  const qsfSessionId = parsed.qsf_session_id;
  const models = parsed.models;
  if (typeof qsfSessionId !== "string" || !Array.isArray(models)) {
    return null;
  }

  const parsedModels: ModelTokenUsage[] = [];
  for (const entry of models) {
    if (!isRecord(entry)) {
      return null;
    }
    const counts = parseTokenClassCounts(entry.counts);
    if (
      typeof entry.model_id !== "string" ||
      typeof entry.role !== "string" ||
      typeof entry.calls !== "number" ||
      counts === null
    ) {
      return null;
    }
    parsedModels.push({
      modelId: entry.model_id,
      role: entry.role,
      calls: entry.calls,
      counts,
    });
  }

  return {
    qsfSessionId,
    models: parsedModels,
  };
}

/// Parse the top-level `signals` array of a `volition_state` message. Defensive and non-fatal:
/// a missing key or non-array value yields an empty list (back-compat with captures that predate
/// the field), and any malformed entry is dropped rather than rejecting the whole message —
/// signals are display-only decoration and must never null out an otherwise valid capture.
function parseFunctionalSignals(value: unknown): VolitionFunctionalSignal[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const signals: VolitionFunctionalSignal[] = [];
  for (const entry of value) {
    const signal = parseFunctionalSignal(entry);
    if (signal !== null) {
      signals.push(signal);
    }
  }
  return signals;
}

function parseFunctionalSignal(value: unknown): VolitionFunctionalSignal | null {
  if (!isRecord(value) || typeof value.intensity !== "number") {
    return null;
  }
  // The wire also carries a redundant top-level `kind`, but the externally-tagged evidence key is
  // authoritative and always agrees with it (guaranteed server-side), so kind is taken from there.
  const evidence = parseSignalEvidence(value.evidence);
  if (evidence === null) {
    return null;
  }
  return { kind: evidence.kind, intensity: value.intensity, evidence };
}

function parseSignalEvidence(value: unknown): VolitionSignalEvidence | null {
  if (!isRecord(value)) {
    return null;
  }
  const coherenceDecline = value.coherence_decline;
  if (isRecord(coherenceDecline)) {
    const conflict = parseDeclineReason(coherenceDecline.conflict);
    if (
      typeof coherenceDecline.candidate_title === "string" &&
      conflict !== null &&
      typeof coherenceDecline.rationale === "string" &&
      typeof coherenceDecline.tick === "number"
    ) {
      return {
        kind: "coherence_decline",
        candidateTitle: coherenceDecline.candidate_title,
        conflict,
        rationale: coherenceDecline.rationale,
        tick: coherenceDecline.tick,
      };
    }
    return null;
  }
  const frustration = value.frustration;
  if (isRecord(frustration)) {
    if (
      typeof frustration.goal_id === "string" &&
      typeof frustration.blocked_count === "number" &&
      typeof frustration.last_blocked_tick === "number" &&
      typeof frustration.last_activated_tick === "number"
    ) {
      return {
        kind: "frustration",
        goalId: frustration.goal_id,
        blockedCount: frustration.blocked_count,
        lastBlockedTick: frustration.last_blocked_tick,
        lastActivatedTick: frustration.last_activated_tick,
      };
    }
    return null;
  }
  const satisfaction = value.satisfaction;
  if (isRecord(satisfaction)) {
    if (
      typeof satisfaction.goal_id === "string" &&
      typeof satisfaction.last_satisfied_tick === "number" &&
      typeof satisfaction.evidence_ref === "string"
    ) {
      return {
        kind: "satisfaction",
        goalId: satisfaction.goal_id,
        lastSatisfiedTick: satisfaction.last_satisfied_tick,
        evidenceRef: satisfaction.evidence_ref,
      };
    }
    return null;
  }
  const boredom = value.boredom;
  if (isRecord(boredom)) {
    const inspected = parseGoalSalienceArray(boredom.inspected);
    if (
      inspected !== null &&
      typeof boredom.threshold === "number" &&
      isBoredomGuard(boredom.guard)
    ) {
      return {
        kind: "boredom",
        inspected,
        threshold: boredom.threshold,
        guard: boredom.guard,
      };
    }
    return null;
  }
  return null;
}

function parseDeclineReason(value: unknown): VolitionDeclineReason | null {
  if (!isRecord(value)) {
    return null;
  }
  if (value.kind === "conflicting_goal" && typeof value.goal_id === "string") {
    return { kind: "conflicting_goal", goalId: value.goal_id };
  }
  if (value.kind === "protected_floor") {
    return { kind: "protected_floor" };
  }
  return null;
}

function parseGoalSalienceArray(value: unknown): VolitionGoalSalience[] | null {
  if (!Array.isArray(value)) {
    return null;
  }
  const inspected: VolitionGoalSalience[] = [];
  for (const entry of value) {
    if (
      !isRecord(entry) ||
      typeof entry.goal_id !== "string" ||
      typeof entry.salience !== "number"
    ) {
      return null;
    }
    inspected.push({ goalId: entry.goal_id, salience: entry.salience });
  }
  return inspected;
}

function isBoredomGuard(value: unknown): value is VolitionBoredomGuard {
  return value === "prior_activation" || value === "elapsed_ticks";
}

function convertVolitionStateInspectionCapture(
  value: unknown,
): VolitionStateInspectionCapture | null {
  if (!isVolitionStateInspectionCapture(value)) {
    return null;
  }
  const wire = value as {
    mode: string;
    tick: number;
    active_goals: Array<{
      id: string;
      title: string;
      salience: number;
      cooldown_until_tick: number | null;
      last_activated_tick: number | null;
    }>;
    accepted_goals: Array<{
      id: string;
      title: string;
      salience: number;
      cooldown_until_tick: number | null;
      last_activated_tick: number | null;
    }>;
    blocked_goals: Array<{
      id: string;
      title: string;
      salience: number;
      cooldown_until_tick: number | null;
      last_activated_tick: number | null;
    }>;
    cooldown_goals: Array<{
      id: string;
      title: string;
      salience: number;
      cooldown_until_tick: number | null;
      last_activated_tick: number | null;
    }>;
    retired_goals: Array<{
      id: string;
      title: string;
      salience: number;
      cooldown_until_tick: number | null;
      last_activated_tick: number | null;
    }>;
    pending_candidate_count: number;
    accepted_candidate_count: number;
    last_initiative_summaries: Array<{
      goal_id: string;
      goal_title: string;
      output_kind: string;
    }>;
  };
  return {
    mode: wire.mode,
    tick: wire.tick,
    activeGoals: wire.active_goals.map(convertVolitionGoalStatusSummary),
    acceptedGoals: wire.accepted_goals.map(convertVolitionGoalStatusSummary),
    blockedGoals: wire.blocked_goals.map(convertVolitionGoalStatusSummary),
    cooldownGoals: wire.cooldown_goals.map(convertVolitionGoalStatusSummary),
    retiredGoals: wire.retired_goals.map(convertVolitionGoalStatusSummary),
    pendingCandidateCount: wire.pending_candidate_count,
    acceptedCandidateCount: wire.accepted_candidate_count,
    lastInitiativeSummaries: wire.last_initiative_summaries.map(convertVolitionInitiativeSummary),
  };
}

function convertVolitionTurnDecisionSummary(value: unknown): VolitionTurnDecisionSummary | null {
  if (!isVolitionTurnDecisionSummary(value)) {
    return null;
  }
  const wire = value as {
    winner: {
      winner_goal_id: string;
      winner_goal_title: string;
      winner_effective_tier: number;
      winner_biased_tier: number;
      protected_tier_active: boolean;
      winner_visibility?: unknown;
    } | null;
    qualification_threshold: number;
    below_threshold: Array<{
      goal_id: string;
      goal_title: string;
      matched_keywords: Array<{ term: string; weight_class: KeywordWeightClass }>;
      match_strength: number;
    }>;
    mode_bias_outcomes: Array<{
      goal_id: string;
      goal_title: string;
      effective_tier: number;
      biased_tier: number;
      protected: boolean;
    }>;
    selected_goal_ids: string[];
    omitted_or_suppressed_goal_ids: string[];
    shaping_intensity: string;
    last_initiative_output_kind: string | null;
    last_initiative_surfaced: boolean;
    last_initiative_suppression_reason: VolitionSuppressionReason | null;
    last_initiative_rendered_line_present: boolean;
    ambient_exposure?: unknown;
    subconscious_selected_count?: unknown;
  };
  return {
    winner:
      wire.winner === null
        ? null
        : {
          winnerGoalId: wire.winner.winner_goal_id,
          winnerGoalTitle: wire.winner.winner_goal_title,
          winnerEffectiveTier: wire.winner.winner_effective_tier,
          winnerBiasedTier: wire.winner.winner_biased_tier,
          protectedTierActive: wire.winner.protected_tier_active,
          winnerVisibility: toGoalVisibility(wire.winner.winner_visibility),
        },
    qualificationThreshold: wire.qualification_threshold,
    belowThreshold: wire.below_threshold.map((candidate) => ({
      goalId: candidate.goal_id,
      goalTitle: candidate.goal_title,
      matchedKeywords: candidate.matched_keywords.map((keyword) => ({
        term: keyword.term,
        weightClass: keyword.weight_class,
      })),
      matchStrength: candidate.match_strength,
    })),
    modeBiasOutcomes: wire.mode_bias_outcomes.map(convertVolitionModeBiasOutcome),
    selectedGoalIds: wire.selected_goal_ids,
    omittedOrSuppressedGoalIds: wire.omitted_or_suppressed_goal_ids,
    shapingIntensity: wire.shaping_intensity,
    lastInitiativeOutputKind: wire.last_initiative_output_kind,
    lastInitiativeSurfaced: wire.last_initiative_surfaced,
    lastInitiativeSuppressionReason: wire.last_initiative_suppression_reason,
    lastInitiativeRenderedLinePresent: wire.last_initiative_rendered_line_present,
    ambientExposure: toAmbientExposure(wire.ambient_exposure),
    subconsciousSelectedCount:
      typeof wire.subconscious_selected_count === "number" ? wire.subconscious_selected_count : 0,
  };
}

/// Coerce a wire `ambient_exposure` value, defaulting to `"ordinary"` for missing/unknown
/// (back-compat with captures that predate the field).
function toAmbientExposure(value: unknown): VolitionAmbientExposure {
  return value === "reduced_subconscious" || value === "forced_surfaced_subconscious"
    ? value
    : "ordinary";
}

function isVolitionStateInspectionCapture(value: unknown): boolean {
  if (!isRecord(value)) {
    return false;
  }
  return (
    typeof value.mode === "string" &&
    typeof value.tick === "number" &&
    isVolitionGoalStatusSummaryArray(value.active_goals) &&
    isVolitionGoalStatusSummaryArray(value.accepted_goals) &&
    isVolitionGoalStatusSummaryArray(value.blocked_goals) &&
    isVolitionGoalStatusSummaryArray(value.cooldown_goals) &&
    isVolitionGoalStatusSummaryArray(value.retired_goals) &&
    typeof value.pending_candidate_count === "number" &&
    typeof value.accepted_candidate_count === "number" &&
    isVolitionInitiativeSummaryArray(value.last_initiative_summaries)
  );
}

function isVolitionTurnWinnerSummary(value: unknown): boolean {
  if (!isRecord(value)) {
    return false;
  }
  return (
    typeof value.winner_goal_id === "string" &&
    typeof value.winner_goal_title === "string" &&
    typeof value.winner_effective_tier === "number" &&
    typeof value.winner_biased_tier === "number" &&
    typeof value.protected_tier_active === "boolean"
  );
}

function isVolitionBelowThresholdSummary(value: unknown): boolean {
  if (!isRecord(value)) {
    return false;
  }
  return (
    typeof value.goal_id === "string" &&
    typeof value.goal_title === "string" &&
    typeof value.match_strength === "number" &&
    Array.isArray(value.matched_keywords) &&
    value.matched_keywords.every(
      (keyword) =>
        isRecord(keyword) &&
        typeof keyword.term === "string" &&
        (keyword.weight_class === "weak" ||
          keyword.weight_class === "normal" ||
          keyword.weight_class === "strong"),
    )
  );
}

function isVolitionTurnDecisionSummary(value: unknown): boolean {
  if (!isRecord(value)) {
    return false;
  }
  return (
    (value.winner === null || isVolitionTurnWinnerSummary(value.winner)) &&
    typeof value.qualification_threshold === "number" &&
    Array.isArray(value.below_threshold) &&
    value.below_threshold.every(isVolitionBelowThresholdSummary) &&
    Array.isArray(value.mode_bias_outcomes) &&
    value.mode_bias_outcomes.every(isVolitionModeBiasOutcome) &&
    Array.isArray(value.selected_goal_ids) &&
    value.selected_goal_ids.every((goalId) => typeof goalId === "string") &&
    Array.isArray(value.omitted_or_suppressed_goal_ids) &&
    value.omitted_or_suppressed_goal_ids.every((goalId) => typeof goalId === "string") &&
    typeof value.shaping_intensity === "string" &&
    (value.last_initiative_output_kind === null ||
      typeof value.last_initiative_output_kind === "string") &&
    typeof value.last_initiative_surfaced === "boolean" &&
    (value.last_initiative_suppression_reason === null ||
      isVolitionSuppressionReason(value.last_initiative_suppression_reason)) &&
    typeof value.last_initiative_rendered_line_present === "boolean"
  );
}

function isVolitionGoalStatusSummaryArray(value: unknown): value is VolitionGoalStatusSummary[] {
  return Array.isArray(value) && value.every(isVolitionGoalStatusSummary);
}

function isVolitionInitiativeSummaryArray(value: unknown): value is VolitionInitiativeSummary[] {
  return Array.isArray(value) && value.every(isVolitionInitiativeSummary);
}

function isVolitionGoalStatusSummary(value: unknown): value is VolitionGoalStatusSummary {
  if (!isRecord(value)) {
    return false;
  }
  return (
    typeof value.id === "string" &&
    typeof value.title === "string" &&
    typeof value.salience === "number" &&
    (value.cooldown_until_tick === null || typeof value.cooldown_until_tick === "number") &&
    (value.last_activated_tick === null || typeof value.last_activated_tick === "number")
  );
}

function convertVolitionGoalStatusSummary(value: {
  id: string;
  title: string;
  salience: number;
  cooldown_until_tick: number | null;
  last_activated_tick: number | null;
  visibility?: unknown;
}): VolitionGoalStatusSummary {
  return {
    id: value.id,
    title: value.title,
    salience: value.salience,
    cooldownUntilTick: value.cooldown_until_tick,
    lastActivatedTick: value.last_activated_tick,
    visibility: toGoalVisibility(value.visibility),
  };
}

/// Coerce a wire `visibility` value to a `VolitionGoalVisibility`, defaulting to `"conscious"`
/// for a missing/unknown value (back-compat with captures that predate the field).
function toGoalVisibility(value: unknown): VolitionGoalVisibility {
  return value === "subconscious" ? "subconscious" : "conscious";
}

function isVolitionInitiativeSummary(value: unknown): value is VolitionInitiativeSummary {
  if (!isRecord(value)) {
    return false;
  }
  return (
    typeof value.goal_id === "string" &&
    typeof value.goal_title === "string" &&
    typeof value.output_kind === "string"
  );
}

function convertVolitionInitiativeSummary(value: {
  goal_id: string;
  goal_title: string;
  output_kind: string;
}): VolitionInitiativeSummary {
  return {
    goalId: value.goal_id,
    goalTitle: value.goal_title,
    outputKind: value.output_kind,
  };
}

function isVolitionModeBiasOutcome(value: unknown): value is VolitionModeBiasOutcomeCapture {
  if (!isRecord(value)) {
    return false;
  }
  return (
    typeof value.goal_id === "string" &&
    typeof value.goal_title === "string" &&
    typeof value.effective_tier === "number" &&
    typeof value.biased_tier === "number" &&
    typeof value.protected === "boolean"
  );
}

function convertVolitionModeBiasOutcome(value: {
  goal_id: string;
  goal_title: string;
  effective_tier: number;
  biased_tier: number;
  protected: boolean;
}): VolitionModeBiasOutcomeCapture {
  return {
    goalId: value.goal_id,
    goalTitle: value.goal_title,
    effectiveTier: value.effective_tier,
    biasedTier: value.biased_tier,
    protected: value.protected,
  };
}

function isVolitionSuppressionReason(value: unknown): value is VolitionSuppressionReason {
  return (
    value === "intensity" ||
    value === "protected_no_opportunity" ||
    value === "anti_nag_repeat" ||
    value === "non_renderable_output" ||
    value === "below_qualification_threshold"
  );
}

function applyRelayEnvelope(
  state: ConversationState,
  envelope: RelayEnvelope,
  atMs: number,
): ConversationState {
  const base = { ...state };
  const next = applyRelayEnvelopeKind(base, envelope);
  return {
    ...next,
    eventLog: appendEventLog(state.eventLog, envelope.kind, atMs, next.phase),
    phaseTimeline: appendPhaseTimeline(state.phaseTimeline, next.phase, atMs),
  };
}

/// The per-kind switch that maps a relay envelope to the next runtime state. The
/// wrapper `applyRelayEnvelope` stamps the event log and phase timeline around it,
/// so this returns `{ ...base, ... }` for each kind without touching history.
function applyRelayEnvelopeKind(
  base: ConversationState,
  envelope: RelayEnvelope,
): ConversationState {
  switch (envelope.kind) {
    case "user_turn_started":
      return {
        ...base,
        phase: "listening",
        liveTranscript: "",
        responseDraft: "",
      };
    case "partial_transcript":
      return {
        ...base,
        phase: "listening",
        liveTranscript: envelope.transcript ?? base.liveTranscript,
      };
    case "final_transcript": {
      const text = envelope.transcript?.trim();
      return {
        ...base,
        phase: "thinking",
        liveTranscript: "",
        transcript: text
          ? appendTranscript(base.transcript, { role: "user", text })
          : base.transcript,
      };
    }
    case "response_started":
      return {
        ...base,
        phase: "thinking",
        responseDraft: "",
      };
    case "response_completed": {
      const completed = !envelope.status || envelope.status === "completed";
      const text = (envelope.text ?? (completed ? base.responseDraft : "")).trim();
      const phase = completed && base.phase !== "idle" ? "speaking" : "idle";
      return {
        ...base,
        phase,
        responseDraft: "",
        transcript: text
          ? appendTranscript(base.transcript, { role: "assistant", text })
          : base.transcript,
      };
    }
    case "speech_playback_started":
      return {
        ...base,
        phase: "speaking",
        responseDraft: base.responseDraft + (envelope.text ?? ""),
      };
    case "speech_playback_completed":
      return {
        ...base,
        phase: "idle",
      };
    case "session_stopped":
      return {
        ...base,
        connection: "idle",
        phase: "idle",
        sessionId: null,
        liveTranscript: "",
        responseDraft: "",
      };
  }
}

function appendEventLog(
  log: EventLogEntry[],
  kind: string,
  atMs: number,
  phase: RuntimePhase,
): EventLogEntry[] {
  const head = log[0];
  if (head !== undefined && head.kind === kind) {
    return [{ ...head, lastAtMs: atMs, count: head.count + 1, phase }, ...log.slice(1)];
  }
  return [{ kind, phase, firstAtMs: atMs, lastAtMs: atMs, count: 1 }, ...log].slice(
    0,
    EVENT_LOG_LIMIT,
  );
}

function appendPhaseTimeline(
  timeline: PhaseSegment[],
  phase: RuntimePhase,
  atMs: number,
): PhaseSegment[] {
  const last = timeline.at(-1);
  const appended =
    last !== undefined && last.phase === phase
      ? timeline
      : [...timeline, { phase, startedAtMs: atMs }];
  return prunePhaseTimeline(appended, atMs);
}

/// Drop segments that ended a full lane window of *activity time* ago, keeping
/// the segment that spans the cutoff so the lane's left edge is still painted.
/// Compressed idle gaps cost almost no lane time, so wall-clock-old history
/// survives a long wait — that is the point of the lane pause.
function prunePhaseTimeline(timeline: PhaseSegment[], nowMs: number): PhaseSegment[] {
  let laneFromNow = 0;
  for (let i = timeline.length - 1; i > 0; i--) {
    // After adding segment i, laneFromNow is the lane distance from now back to
    // segment i's start — which is where segment i-1 ends.
    laneFromNow += laneDurationOf(timeline, i, nowMs);
    if (laneFromNow >= PHASE_LANE_WINDOW_MS) {
      return timeline.slice(i);
    }
  }
  return timeline;
}

function appendTranscript(entries: TranscriptEntry[], entry: TranscriptEntry): TranscriptEntry[] {
  const lastEntry = entries.at(-1);
  if (lastEntry?.role === entry.role && lastEntry.text === entry.text) {
    return entries;
  }
  return entries.concat(entry);
}

export function describeConnection(state: ConversationState): string {
  switch (state.connection) {
    case "idle":
      return "Idle";
    case "requesting_session":
      return "Requesting session";
    case "connecting_media":
      return "Connecting media";
    case "ready":
      return "Ready";
    case "stopping":
      return "Stopping";
    case "error":
      return "Connection error";
  }
}

export function describeRuntimePhase(phase: RuntimePhase): string {
  switch (phase) {
    case "idle":
      return "Idle";
    case "listening":
      return "Listening";
    case "thinking":
      return "Thinking";
    case "speaking":
      return "Speaking";
  }
}

export interface MuteButtonModel {
  /// Action-label wording for the toggle: what the click will do.
  label: string;
  /// Current gate state, mapped to `aria-pressed` on the button.
  pressed: boolean;
}

export function selectMuteButton(state: ConversationState): MuteButtonModel {
  return {
    label: state.muted ? "Unmute" : "Mute",
    pressed: state.muted,
  };
}

export interface TickerRowModel {
  kind: string;
  /// "×N" for a collapsed burst, null for a single occurrence.
  countLabel: string | null;
  /// Local wall-clock "HH:MM:SS.d" of the row's first occurrence.
  timeLabel: string;
  /// "+X.Ys" gap since the previous (older) row's last occurrence; null on the oldest row.
  deltaLabel: string | null;
}

export function selectEventTickerModel(state: ConversationState): TickerRowModel[] {
  return state.eventLog.map((entry, index) => {
    const older = state.eventLog[index + 1];
    return {
      kind: entry.kind,
      countLabel: entry.count > 1 ? `×${entry.count}` : null,
      timeLabel: formatClockTime(entry.firstAtMs),
      deltaLabel:
        older === undefined ? null : `+${((entry.firstAtMs - older.lastAtMs) / 1000).toFixed(1)}s`,
    };
  });
}

export interface PhaseLaneSegmentModel {
  phase: RuntimePhase;
  startFraction: number;
  endFraction: number;
}

export interface PhaseLaneTickModel {
  fraction: number;
  kind: string;
  /// Reducer-derived phase after the event (copied from EventLogEntry.phase);
  /// the canvas colors the tick with this and needs no event-kind knowledge.
  phase: RuntimePhase;
  timeLabel: string;
}

export interface PhaseLaneGridlineModel {
  fraction: number;
  label: string;
}

export interface PhaseLaneBreakModel {
  startFraction: number;
  endFraction: number;
  /// Wall-clock duration of the whole compressed idle gap, e.g. "⫽ 41s".
  label: string;
}

export interface PhaseLaneModel {
  segments: PhaseLaneSegmentModel[];
  ticks: PhaseLaneTickModel[];
  gridlines: PhaseLaneGridlineModel[];
  breaks: PhaseLaneBreakModel[];
}

export const PHASE_LANE_GRIDLINE_STEP_MS = 15_000;

/// One wall-time interval annotated with the lane-time width it occupies.
interface LaneSpan {
  phase: RuntimePhase;
  wallStartMs: number;
  wallEndMs: number;
  laneMs: number;
  /// Non-null when this span is a compressed idle gap's break band.
  breakLabel: string | null;
}

/// Lane-time width of timeline segment `index`: non-idle and short-idle segments
/// map 1:1; a closed long idle gap costs cap + break band; the live trailing
/// idle freezes at the cap (the pause). Shared by the selector and pruning so
/// state retention matches what the lane can show.
function laneDurationOf(timeline: PhaseSegment[], index: number, nowMs: number): number {
  const { phase, startedAtMs } = timeline[index];
  const isTrailing = index + 1 === timeline.length;
  const endMs = isTrailing ? nowMs : timeline[index + 1].startedAtMs;
  const durationMs = endMs - startedAtMs;
  if (phase !== "idle" || durationMs <= PHASE_LANE_IDLE_CAP_MS) {
    return durationMs;
  }
  return isTrailing ? PHASE_LANE_IDLE_CAP_MS : PHASE_LANE_IDLE_CAP_MS + PHASE_LANE_BREAK_LANE_MS;
}

/// Expand the phase timeline into lane spans. A closed idle gap longer than the
/// cap splits into a true-scale head plus a break band; the live trailing idle
/// keeps a single capped span and gains its band only once the gap closes.
function laneSpansOf(timeline: PhaseSegment[], nowMs: number): LaneSpan[] {
  const spans: LaneSpan[] = [];
  for (let i = 0; i < timeline.length; i++) {
    const { phase, startedAtMs } = timeline[i];
    const isTrailing = i + 1 === timeline.length;
    const endMs = isTrailing ? nowMs : timeline[i + 1].startedAtMs;
    const durationMs = endMs - startedAtMs;
    const isCompressed = phase === "idle" && durationMs > PHASE_LANE_IDLE_CAP_MS;
    if (!isCompressed || isTrailing) {
      spans.push({
        phase,
        wallStartMs: startedAtMs,
        wallEndMs: endMs,
        laneMs: laneDurationOf(timeline, i, nowMs),
        breakLabel: null,
      });
      continue;
    }
    spans.push({
      phase,
      wallStartMs: startedAtMs,
      wallEndMs: startedAtMs + PHASE_LANE_IDLE_CAP_MS,
      laneMs: PHASE_LANE_IDLE_CAP_MS,
      breakLabel: null,
    });
    spans.push({
      phase,
      wallStartMs: startedAtMs + PHASE_LANE_IDLE_CAP_MS,
      wallEndMs: endMs,
      laneMs: PHASE_LANE_BREAK_LANE_MS,
      breakLabel: `⫽ ${formatGapDuration(durationMs)}`,
    });
  }
  return spans;
}

function formatGapDuration(ms: number): string {
  const totalSeconds = Math.round(ms / 1000);
  if (totalSeconds < 60) {
    return `${totalSeconds}s`;
  }
  return `${Math.floor(totalSeconds / 60)}m ${totalSeconds % 60}s`;
}

/// Geometry for the phase swimlane, all x-positions as fractions of the lane
/// width in [0, 1] with `now` at 1. The x-axis is *activity time*: idle gaps
/// longer than PHASE_LANE_IDLE_CAP_MS are compressed into break bands, and the
/// live trailing idle freezes the lane (gridline "now" reads "paused"). The
/// canvas renderer multiplies by pixel width and picks colors; it makes no
/// layout decisions of its own.
export function selectPhaseLaneModel(state: ConversationState, nowMs: number): PhaseLaneModel {
  const clamp01 = (value: number) => Math.min(1, Math.max(0, value));
  const spans = laneSpansOf(state.phaseTimeline, nowMs);

  // Lane-time distance from now back to each span's start.
  const laneStartFromNow: number[] = new Array(spans.length);
  let cumulative = 0;
  for (let i = spans.length - 1; i >= 0; i--) {
    cumulative += spans[i].laneMs;
    laneStartFromNow[i] = cumulative;
  }

  const fractionWithin = (span: LaneSpan, laneStart: number, atMs: number): number => {
    const wallSpanMs = span.wallEndMs - span.wallStartMs;
    // Break bands squash their wall interval linearly; other spans map 1:1 with
    // the offset clamped to laneMs (the frozen tail of a live idle span).
    const offset =
      span.breakLabel !== null && wallSpanMs > 0
        ? (span.laneMs * (atMs - span.wallStartMs)) / wallSpanMs
        : Math.min(atMs - span.wallStartMs, span.laneMs);
    return 1 - (laneStart - offset) / PHASE_LANE_WINDOW_MS;
  };

  /// Lane fraction of an arbitrary wall time. Times before the first span map
  /// 1:1 through the leading implicit idle; with no spans at all the axis is
  /// plain wall clock.
  const fractionOf = (atMs: number): number => {
    if (spans.length === 0) {
      return 1 - (nowMs - atMs) / PHASE_LANE_WINDOW_MS;
    }
    if (atMs < spans[0].wallStartMs) {
      const firstStartFraction = 1 - laneStartFromNow[0] / PHASE_LANE_WINDOW_MS;
      return firstStartFraction - (spans[0].wallStartMs - atMs) / PHASE_LANE_WINDOW_MS;
    }
    let index = spans.length - 1;
    for (let i = 0; i + 1 < spans.length; i++) {
      if (atMs < spans[i + 1].wallStartMs) {
        index = i;
        break;
      }
    }
    return fractionWithin(spans[index], laneStartFromNow[index], atMs);
  };

  const segments: PhaseLaneSegmentModel[] = [];
  const breaks: PhaseLaneBreakModel[] = [];
  // Before the first recorded segment the runtime phase was idle (INITIAL_STATE.phase).
  const firstStartFraction =
    spans.length === 0 ? 1 : clamp01(1 - laneStartFromNow[0] / PHASE_LANE_WINDOW_MS);
  if (firstStartFraction > 0) {
    segments.push({ phase: "idle", startFraction: 0, endFraction: firstStartFraction });
  }
  for (let i = 0; i < spans.length; i++) {
    // Hoisting the element is load-bearing: TypeScript only narrows
    // span.breakLabel to string on a const reference, not on spans[i] with a
    // mutable index.
    const span = spans[i];
    const startFraction = clamp01(1 - laneStartFromNow[i] / PHASE_LANE_WINDOW_MS);
    const endFraction = clamp01(1 - (laneStartFromNow[i] - span.laneMs) / PHASE_LANE_WINDOW_MS);
    if (endFraction <= 0) {
      continue;
    }
    if (span.breakLabel !== null) {
      breaks.push({ startFraction, endFraction, label: span.breakLabel });
    } else {
      segments.push({ phase: span.phase, startFraction, endFraction });
    }
  }

  const ticks: PhaseLaneTickModel[] = [];
  for (const entry of state.eventLog) {
    const atMss = entry.count > 1 ? [entry.firstAtMs, entry.lastAtMs] : [entry.firstAtMs];
    for (const atMs of atMss) {
      const fraction = fractionOf(Math.min(atMs, nowMs));
      if (fraction >= 0 && fraction <= 1) {
        ticks.push({
          fraction,
          kind: entry.kind,
          phase: entry.phase,
          timeLabel: formatClockTime(atMs),
        });
      }
    }
  }
  ticks.sort((a, b) => a.fraction - b.fraction);

  const lastSegment = state.phaseTimeline.at(-1);
  const paused =
    lastSegment !== undefined &&
    lastSegment.phase === "idle" &&
    nowMs - lastSegment.startedAtMs > PHASE_LANE_IDLE_CAP_MS;

  const gridlines: PhaseLaneGridlineModel[] = [];
  for (let backMs = 0; backMs <= PHASE_LANE_WINDOW_MS; backMs += PHASE_LANE_GRIDLINE_STEP_MS) {
    gridlines.push({
      fraction: 1 - backMs / PHASE_LANE_WINDOW_MS,
      label: backMs === 0 ? (paused ? "paused" : "now") : `-${backMs / 1000}s`,
    });
  }

  return { segments, ticks, gridlines, breaks };
}

function formatClockTime(atMs: number): string {
  const date = new Date(atMs);
  const pad = (value: number) => String(value).padStart(2, "0");
  const deciseconds = Math.floor(date.getMilliseconds() / 100);
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}.${deciseconds}`;
}

export interface TextTurnSubmitInput {
  hasText: boolean;
  pending: boolean;
}

export function selectCanSubmitTextTurn(
  state: ConversationState,
  input: TextTurnSubmitInput,
): boolean {
  if (input.pending || !input.hasText) {
    return false;
  }
  switch (state.connection) {
    case "idle":
    case "ready":
      return true;
    case "requesting_session":
    case "connecting_media":
    case "stopping":
    case "error":
      return false;
  }
}

export function mapProviderMessageToRelayEnvelope(
  qsfSessionId: string,
  message: ProviderDataChannelMessage,
): RelayEnvelope | null {
  const kind = providerTypeToRelayKind(message.type);
  if (kind === null) {
    return null;
  }

  return {
    qsf_session_id: qsfSessionId,
    event_id: message.event_id,
    kind,
    item_id: message.item_id,
    previous_item_id: message.previous_item_id,
    response_id: message.response_id,
    transcript: message.transcript,
    text: relayTextFor(kind, message),
    status: message.status,
    audio_marker: message.audio_marker,
    payload: message.payload ?? message,
  };
}

function relayTextFor(
  kind: RelayEventKind,
  message: ProviderDataChannelMessage,
): string | undefined {
  switch (kind) {
    case "response_completed":
      return message.text ?? message.transcript;
    case "speech_playback_started":
      return message.delta ?? message.text;
    default:
      return undefined;
  }
}

export function providerTypeToRelayKind(type: string): RelayEventKind | null {
  switch (type) {
    case "input_audio_buffer.speech_started":
      return "user_turn_started";
    case "conversation.item.input_audio_transcription.delta":
      return "partial_transcript";
    case "conversation.item.input_audio_transcription.completed":
      return "final_transcript";
    case "response.created":
      return "response_started";
    case "response.done":
      return "response_completed";
    case "response.audio_transcript.delta":
    case "response.output_audio_transcript.delta":
    case "response.text.delta":
    case "response.output_text.delta":
      return "speech_playback_started";
    case "response.audio_transcript.done":
    case "response.output_audio_transcript.done":
    case "response.text.done":
    case "response.output_text.done":
      return "response_completed";
    case "response.audio.delta":
    case "response.output_audio.delta":
      return "speech_playback_started";
    case "response.audio.done":
    case "response.output_audio.done":
      return "speech_playback_completed";
    case "session.closed":
      return "session_stopped";
    default:
      return null;
  }
}

export function parseProviderDataChannelMessage(raw: string): ProviderDataChannelMessage {
  const parsed: unknown = JSON.parse(raw);
  if (!isRecord(parsed)) {
    throw new Error("provider event payload must be an object");
  }
  const eventId = parsed.event_id;
  const type = parsed.type;
  if (typeof eventId !== "string" || typeof type !== "string") {
    throw new Error("provider event payload missing event_id or type");
  }
  return {
    event_id: eventId,
    type,
    item_id: stringField(parsed.item_id),
    previous_item_id: stringField(parsed.previous_item_id),
    response_id: stringField(parsed.response_id) ?? nestedStringField(parsed, "response", "id"),
    delta: stringField(parsed.delta),
    transcript: stringField(parsed.transcript),
    text:
      stringField(parsed.text) ??
      nestedStringField(parsed, "response", "text") ??
      responseOutputText(parsed),
    status: stringField(parsed.status) ?? nestedStringField(parsed, "response", "status"),
    audio_marker: stringField(parsed.audio_marker),
    payload: parsed.payload ?? parsed,
  };
}

function responseOutputText(value: Record<string, unknown>): string | undefined {
  const response = value.response;
  if (!isRecord(response)) {
    return undefined;
  }
  const output = response.output;
  if (!Array.isArray(output)) {
    return undefined;
  }

  const parts = output.flatMap((item) => responseOutputTextParts(item));
  return parts.length > 0 ? parts.join("\n") : undefined;
}

function responseOutputTextParts(value: unknown): string[] {
  if (!isRecord(value)) {
    return [];
  }

  const directText = stringField(value.text) ?? stringField(value.transcript);
  const content = value.content;
  const contentParts = Array.isArray(content)
    ? content.flatMap((part) => responseOutputTextParts(part))
    : [];
  return directText ? [directText, ...contentParts] : contentParts;
}

function nestedStringField(
  value: Record<string, unknown>,
  objectKey: string,
  fieldKey: string,
): string | undefined {
  const nested = value[objectKey];
  if (!isRecord(nested)) {
    return undefined;
  }
  return stringField(nested[fieldKey]);
}

function stringField(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function selectVolitionPanelModel(state: ConversationState): VolitionPanelModel {
  const capture = state.latestVolitionState;
  if (capture === null) {
    return {
      kind: "empty",
      headline: "No volition state yet",
      banner: "Awaiting the first trusted turn.",
      sections: [],
    };
  }

  const snapshotSections = [
    {
      title: "State snapshot",
      rows: [
        { label: "Mode", value: formatLabelValue(capture.inspection.mode) },
        { label: "Tick", value: String(capture.inspection.tick) },
        {
          label: "Active goals",
          value: formatGoalSummaries(capture.inspection.activeGoals, capture.forcedSurfaced),
        },
        {
          label: "Accepted goals",
          value: formatGoalSummaries(capture.inspection.acceptedGoals, capture.forcedSurfaced),
        },
        {
          label: "Blocked goals",
          value: formatGoalSummaries(capture.inspection.blockedGoals, capture.forcedSurfaced),
        },
        {
          label: "Cooldown goals",
          value: formatGoalSummaries(capture.inspection.cooldownGoals, capture.forcedSurfaced),
        },
        {
          label: "Retired goals",
          value: formatGoalSummaries(capture.inspection.retiredGoals, capture.forcedSurfaced),
        },
        {
          label: "Pending candidates",
          value: String(capture.inspection.pendingCandidateCount),
        },
        {
          label: "Accepted candidates",
          value: String(capture.inspection.acceptedCandidateCount),
        },
        {
          label: "Last initiative summaries",
          value: formatInitiativeSummaries(capture.inspection.lastInitiativeSummaries),
        },
      ],
    },
  ];

  const signalsSection: VolitionPanelSection = {
    title: "Functional signals",
    rows: formatSignalRows(capture.signals),
  };

  if (capture.decision === null) {
    return {
      kind: "snapshot",
      headline: "Volition state",
      banner: "No volition decision this turn.",
      sections: snapshotSections.concat(signalsSection),
    };
  }

  const decision = capture.decision;
  const winnerRows: VolitionPanelRow[] =
    decision.winner === null
      ? [
        {
          label: "Winner",
          value: `no goal qualified (threshold ${decision.qualificationThreshold})`,
        },
        {
          label: "Below threshold",
          value: formatBelowThreshold(decision.belowThreshold),
        },
      ]
      : [
        {
          label: "Winner",
          value: `${decision.winner.winnerGoalTitle} [${decision.winner.winnerGoalId}]`,
        },
        {
          label: "Winner visibility",
          value: formatLabelValue(decision.winner.winnerVisibility),
        },
        {
          label: "Winner tiers",
          value: `effective ${decision.winner.winnerEffectiveTier}, biased ${decision.winner.winnerBiasedTier}, protected ${yesNo(decision.winner.protectedTierActive)}`,
        },
      ];
  return {
    kind: "decision",
    headline: "Volition state",
    banner:
      decision.winner === null
        ? "No goal qualified this turn."
        : "Decision captured for this trusted turn.",
    sections: snapshotSections.concat(
      {
        title: "Decision detail",
        rows: [
          ...winnerRows,
          {
            label: "Mode bias outcomes",
            value: formatModeBiasOutcomes(decision.modeBiasOutcomes),
          },
          {
            label: "Selected goals",
            value: formatIdList(decision.selectedGoalIds),
          },
          {
            label: "Omitted/suppressed goals",
            value: formatIdList(decision.omittedOrSuppressedGoalIds),
          },
          {
            label: "Shaping intensity",
            value: formatLabelValue(decision.shapingIntensity),
          },
          {
            label: "Last initiative output",
            value: decision.lastInitiativeOutputKind
              ? formatLabelValue(decision.lastInitiativeOutputKind)
              : "none",
          },
          {
            label: "Last initiative surfaced",
            value: yesNo(decision.lastInitiativeSurfaced),
          },
          {
            label: "Suppression reason",
            value: decision.lastInitiativeSuppressionReason
              ? formatLabelValue(decision.lastInitiativeSuppressionReason)
              : "none",
          },
          {
            label: "Rendered line",
            value: yesNo(decision.lastInitiativeRenderedLinePresent),
          },
          {
            label: "Ambient exposure",
            value: formatLabelValue(decision.ambientExposure),
          },
          {
            label: "Subconscious selected",
            value: String(decision.subconsciousSelectedCount),
          },
          {
            label: "Trace ref",
            value: capture.responseCreateEventRef,
          },
        ],
      },
      signalsSection,
    ),
  };
}

/// Fixed display order of token classes: inputs before outputs, audio before
/// text within each direction, cached input between. The className suffixes
/// double as CSS hooks (`token-seg-<className>`).
const TOKEN_CLASS_ORDER = [
  { key: "audioInput", className: "audio-in", label: "audio in" },
  { key: "textInput", className: "text-in", label: "text in" },
  { key: "cachedInput", className: "cached-in", label: "cached in" },
  { key: "audioOutput", className: "audio-out", label: "audio out" },
  { key: "textOutput", className: "text-out", label: "text out" },
] as const;

const TOKEN_ROLE_LABELS: Record<string, string> = {
  realtime_voice: "voice",
  goal_formation: "goal formation",
};

export interface TokenUsageSegmentModel {
  className: string;
  label: string;
  tokens: number;
  exactLabel: string;
  widthPercent: number;
}

export interface TokenUsageRowModel {
  name: string;
  totalLabel: string;
  barPercent: number;
  segments: TokenUsageSegmentModel[];
}

export interface TokenUsageLegendEntry {
  className: string;
  label: string;
}

export interface TokenUsagePanelModel {
  kind: "empty" | "data";
  heroLabel: string;
  heroDetail: string;
  legend: TokenUsageLegendEntry[];
  rows: TokenUsageRowModel[];
}

/// Compact token-count formatting for headline and row totals: exact under 1k,
/// one decimal in k, two decimals in M.
export function formatTokenCount(tokens: number): string {
  if (tokens < 1_000) {
    return String(tokens);
  }
  if (tokens < 1_000_000) {
    return `${(tokens / 1_000).toFixed(1)}k`;
  }
  return `${(tokens / 1_000_000).toFixed(2)}M`;
}

function tokenClassTotal(counts: TokenClassCounts): number {
  return (
    counts.textInput +
    counts.audioInput +
    counts.cachedInput +
    counts.textOutput +
    counts.audioOutput
  );
}

/// View-model for the Tokens panel: rows sorted by total descending (stable, so
/// equal totals keep server order), bar lengths normalized to the largest row,
/// segments in fixed class order with zero classes dropped, and a legend listing
/// only the classes present anywhere. All formatting decisions live here; the
/// render function only builds DOM.
export function selectTokenUsagePanelModel(state: ConversationState): TokenUsagePanelModel {
  const models = state.latestTokenUsage?.models ?? [];
  const grandTotal = models.reduce((sum, model) => sum + tokenClassTotal(model.counts), 0);
  if (grandTotal === 0) {
    return { kind: "empty", heroLabel: "0", heroDetail: "", legend: [], rows: [] };
  }

  const totalCalls = models.reduce((sum, model) => sum + model.calls, 0);
  const ranked = models
    .map((model, index) => ({
      model,
      index,
      total: tokenClassTotal(model.counts),
    }))
    .sort((a, b) => b.total - a.total || a.index - b.index);
  const maxTotal = ranked[0]?.total ?? 0;

  const rows: TokenUsageRowModel[] = ranked.map(({ model, total }) => {
    const rowTotal = total;
    const roleLabel = TOKEN_ROLE_LABELS[model.role] ?? formatLabelValue(model.role);
    const segments: TokenUsageSegmentModel[] = [];
    for (const tokenClass of TOKEN_CLASS_ORDER) {
      const tokens = model.counts[tokenClass.key];
      if (tokens === 0) {
        continue;
      }
      segments.push({
        className: tokenClass.className,
        label: tokenClass.label,
        tokens,
        exactLabel: `${tokenClass.label} — ${tokens.toLocaleString("en-US")} tokens`,
        widthPercent: (tokens * 100) / rowTotal,
      });
    }
    return {
      name: `${model.modelId} · ${roleLabel}`,
      totalLabel: formatTokenCount(rowTotal),
      barPercent: maxTotal === 0 ? 0 : (rowTotal * 100) / maxTotal,
      segments,
    };
  });

  const legend: TokenUsageLegendEntry[] = TOKEN_CLASS_ORDER.filter((tokenClass) =>
    models.some((model) => model.counts[tokenClass.key] > 0),
  ).map((tokenClass) => ({ className: tokenClass.className, label: tokenClass.label }));

  return {
    kind: "data",
    heroLabel: formatTokenCount(grandTotal),
    heroDetail: `session total · ${totalCalls} model call${totalCalls === 1 ? "" : "s"}`,
    legend,
    rows,
  };
}

function formatBelowThreshold(candidates: VolitionBelowThresholdSummary[]): string {
  if (candidates.length === 0) {
    return "none";
  }
  return candidates
    .map((candidate) => {
      const keywords = candidate.matchedKeywords
        .map((keyword) => `${keyword.term}/${keyword.weightClass}`)
        .join(", ");
      return `${candidate.goalId} (strength ${candidate.matchStrength}: ${keywords})`;
    })
    .join("; ");
}

function formatGoalSummaries(
  goals: VolitionGoalStatusSummary[],
  forcedSurfaced: VolitionForcedSurfacing[],
): string {
  if (goals.length === 0) {
    return "none";
  }
  return goals
    .map((goal) => `${goal.title} [${goal.id}]${goalVisibilityBadge(goal, forcedSurfaced)}`)
    .join("; ");
}

/// A short badge appended to a subconscious goal so the operator can see it is a background
/// disposition and whether it is forced surfaced (and why). Conscious goals get no badge, so the
/// panel never hides a subconscious goal — it labels it (guardrail D2).
function goalVisibilityBadge(
  goal: VolitionGoalStatusSummary,
  forcedSurfaced: VolitionForcedSurfacing[],
): string {
  if (goal.visibility !== "subconscious") {
    return "";
  }
  const reasons = forcedSurfaced
    .filter((entry) => entry.goalId === goal.id)
    .map((entry) => forcingConditionLabel(entry.condition));
  if (reasons.length === 0) {
    return " (subconscious)";
  }
  return ` (subconscious · surfaced: ${reasons.join(", ")})`;
}

function forcingConditionLabel(condition: VolitionForcingCondition): string {
  return condition.kind === "rendered_initiative" ? "rendered initiative" : "coherence conflict";
}

function formatInitiativeSummaries(summaries: VolitionInitiativeSummary[]): string {
  if (summaries.length === 0) {
    return "none";
  }
  return summaries
    .map(
      (summary) =>
        `${summary.goalTitle} [${summary.goalId}] (${formatLabelValue(summary.outputKind)})`,
    )
    .join("; ");
}

function formatModeBiasOutcomes(outcomes: VolitionModeBiasOutcomeCapture[]): string {
  if (outcomes.length === 0) {
    return "none";
  }
  return outcomes
    .map(
      (outcome) =>
        `${outcome.goalTitle} [${outcome.goalId}] eff ${outcome.effectiveTier}, biased ${outcome.biasedTier}, protected ${yesNo(outcome.protected)}`,
    )
    .join("; ");
}

function formatIdList(ids: string[]): string {
  return ids.length === 0 ? "none" : ids.join(", ");
}

/// One panel row per functional signal, each carrying the concrete evidence that justifies it —
/// a signal name never appears without its evidence. Empty captures render a single "none" row,
/// matching the empty-state convention the other sections use for empty lists.
function formatSignalRows(signals: VolitionFunctionalSignal[]): VolitionPanelRow[] {
  if (signals.length === 0) {
    return [{ label: "Signals", value: "none" }];
  }
  return signals.map((signal) => ({
    label: signalKindLabel(signal.kind),
    value: formatSignalEvidence(signal),
  }));
}

/// Instrument-readout label for a signal kind. Sentence case to match the panel's other labels
/// (e.g. "Active goals"); deliberately no wording implying a felt state.
function signalKindLabel(kind: VolitionSignalKind): string {
  switch (kind) {
    case "coherence_decline":
      return "Coherence decline";
    case "frustration":
      return "Frustration";
    case "satisfaction":
      return "Satisfaction";
    case "boredom":
      return "Boredom";
  }
}

/// The evidence text for a signal row. Every branch names the recorded state (goal ids, ticks,
/// counts, rationale, declined-candidate title, threshold) and ends with the display intensity, so
/// the row reads as a verifiable readout rather than a bare emotion word.
function formatSignalEvidence(signal: VolitionFunctionalSignal): string {
  const intensity = formatSignalIntensity(signal.intensity);
  const evidence = signal.evidence;
  switch (evidence.kind) {
    case "coherence_decline":
      return `declined "${evidence.candidateTitle}" (tick ${evidence.tick}) — ${formatDeclineReason(evidence.conflict)} — ${evidence.rationale} · intensity ${intensity}`;
    case "frustration":
      return `goal ${evidence.goalId} blocked ${evidence.blockedCount} times despite activation (last blocked tick ${evidence.lastBlockedTick}, last activated tick ${evidence.lastActivatedTick}) · intensity ${intensity}`;
    case "satisfaction":
      return `goal ${evidence.goalId} satisfied at tick ${evidence.lastSatisfiedTick} (evidence: ${evidence.evidenceRef}) · intensity ${intensity}`;
    case "boredom":
      return `${evidence.inspected.length} non-retired goal(s) below engagement threshold ${evidence.threshold} via ${formatBoredomGuard(evidence.guard)} — ${formatInspectedSalience(evidence.inspected)} · intensity ${intensity}`;
  }
}

function formatDeclineReason(reason: VolitionDeclineReason): string {
  switch (reason.kind) {
    case "conflicting_goal":
      return `conflicts with goal ${reason.goalId}`;
    case "protected_floor":
      return "breaches the protected floor";
  }
}

function formatBoredomGuard(guard: VolitionBoredomGuard): string {
  switch (guard) {
    case "prior_activation":
      return "prior activation";
    case "elapsed_ticks":
      return "elapsed ticks";
  }
}

function formatInspectedSalience(inspected: VolitionGoalSalience[]): string {
  if (inspected.length === 0) {
    return "no goals inspected";
  }
  return inspected.map((goal) => `${goal.goalId} salience ${goal.salience}`).join(", ");
}

/// Display the `[0, 1]` intensity as a rounded percentage — the display-friendly form for the panel.
function formatSignalIntensity(intensity: number): string {
  return `${Math.round(intensity * 100)}%`;
}

function yesNo(value: boolean): string {
  return value ? "yes" : "no";
}

function formatLabelValue(value: string): string {
  return value
    .split("_")
    .filter((part) => part.length > 0)
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join(" ");
}

/// The exact prefix every volition turn-context packet's rendered text begins with — on the
/// qualified-winner, no-qualifier, and coherence-only paths alike. The realtime server renders it
/// in `crates/qsf_realtime_server/src/realtime/volition_injection.rs`; a Rust guard test
/// (`packet_text_starts_with_ui_locator_prefix`) pins it so a reword there fails CI before this
/// locator silently stops matching.
export const VOLITION_INJECTED_TEXT_PREFIX = "Simulated volition context for this turn";

/// Result of the injected-packet lookup (`selectInjectedVolitionText`, Task 2). Three states so no
/// consumer can claim nothing was injected when the text is merely unavailable: `found` carries the
/// verbatim packet text; `none_injected` means the exchange-matched turn context was inspected and
/// carried no volition packet; `unavailable` means either capture is missing or the two captures
/// describe different turns (the expected non-atomic watch-channel window).
export type InjectedVolitionText =
  | { status: "found"; text: string }
  | { status: "none_injected" }
  | { status: "unavailable" };

export type VolitionVerdictKind =
  | "not_evaluated"
  | "no_decision"
  | "context_only"
  | "quiet"
  | "spoke";

export interface VolitionVerdict {
  /// Machine-readable state, used only to pick a style class in the renderer.
  kind: VolitionVerdictKind;
  /// One plain-English sentence describing volition's role in the latest reply.
  line: string;
  /// Which turn this verdict describes, e.g. "Latest evaluated turn · exchange 4". Null before any
  /// capture arrives. Surfaced so drift between the panel and the visible answer stays honest.
  caption: string | null;
  /// For a spoken turn: whether an extra initiative line was actually injected ("nudge added") or
  /// held back with a reason ("nudge held back (Anti Nag Repeat)"). Null otherwise. A goal can win
  /// and shape framing while its initiative line is suppressed, so this is reported separately.
  nudge: string | null;
}

/// Derive the plain-English verdict for the latest evaluated turn. Takes the exchange-matched
/// injected-packet lookup as an explicit input: the server can inject a coherence-only packet
/// (declined candidates, no arbitration winner) on a turn whose capture has `decision: null`, so
/// the no-decision wording is only safe when no matching packet was found. Total: returns a
/// defined verdict for every input, including before any capture arrives.
export function selectVolitionVerdict(
  state: ConversationState,
  injected: InjectedVolitionText,
): VolitionVerdict {
  const capture = state.latestVolitionState;
  if (capture === null) {
    return {
      kind: "not_evaluated",
      line: "No evaluated turn yet — awaiting the first volition-evaluated turn.",
      caption: null,
      nudge: null,
    };
  }

  const caption = `Latest evaluated turn · exchange ${capture.exchangeIndex}`;
  const decision = capture.decision;
  if (decision === null) {
    if (injected.status === "found") {
      return {
        kind: "context_only",
        line: "No goal led this turn, but volition still injected context (declined-goal coherence packet).",
        caption,
        nudge: null,
      };
    }
    // Deliberately does not claim "nothing was injected": `injected` may be merely unavailable
    // during the non-atomic watch-channel window.
    return {
      kind: "no_decision",
      line: "Volition was watching but recorded no per-turn decision.",
      caption,
      nudge: null,
    };
  }

  if (decision.winner === null) {
    // A no-qualifier turn still injects a packet telling the model volition stays quiet, so this
    // must not read as "base-model reply".
    const count = decision.belowThreshold.length;
    return {
      kind: "quiet",
      line: `No goal qualified to lead this turn — ${count} goal(s) below the bar (threshold ${decision.qualificationThreshold}). No winning goal shaped this reply.`,
      caption,
      nudge: null,
    };
  }

  const nudge = decision.lastInitiativeRenderedLinePresent
    ? "nudge added"
    : decision.lastInitiativeSuppressionReason !== null
      ? `nudge held back (${formatLabelValue(decision.lastInitiativeSuppressionReason)})`
      : null;
  return {
    kind: "spoke",
    line: `Volition spoke: ${decision.winner.winnerGoalTitle} tilted this reply — ${describeShapingIntensity(decision.shapingIntensity)}.`,
    caption,
    nudge,
  };
}

/// Map the wire shaping-intensity string to a plain adverb. `none` still reads as "lightly" (not
/// "not at all") because a winning goal always injects a framing packet — the intensity governs how
/// hard, not whether. Unknown values fall back to a title-cased label rather than throwing.
function describeShapingIntensity(intensity: string): string {
  switch (intensity.toLowerCase()) {
    case "none":
      return "lightly";
    case "low":
      return "gently";
    case "medium":
      return "moderately";
    case "high":
      return "strongly";
    default:
      return formatLabelValue(intensity);
  }
}

/// Locate the verbatim volition turn packet the model saw this turn. The text is not carried on the
/// volition capture (kept out by a deliberate privacy guardrail); it rides inside the turn-context
/// messages as a `conversation.item.create` item whose text begins with
/// `VOLITION_INJECTED_TEXT_PREFIX`. Returns a status model rather than `string | null` so consumers
/// can tell "the matched turn context carried no packet" (`none_injected`) apart from "the matching
/// capture has not arrived or describes another turn" (`unavailable`). Total: never throws.
export function selectInjectedVolitionText(state: ConversationState): InjectedVolitionText {
  const capture = state.latestVolitionState;
  const context = state.latestTurnContext;
  if (capture === null || context === null) {
    return { status: "unavailable" };
  }
  // Only correlate when both captures describe the same response.create attempt. They are published
  // by two non-atomic watch-channel writes, so for a brief window the browser can hold a verdict for
  // one attempt and a context for another; matching the shared request hash rejects that mismatch
  // instead of showing another turn's injected text next to this verdict. The request hash is the
  // key rather than exchangeIndex because it is unique per response.create attempt — exchangeIndex
  // is constant across retries within one exchange, so it cannot distinguish an earlier attempt's
  // turn-context from a later attempt's volition capture. Both fields are the server's
  // `request_hash.to_string()`, stamped on the two captures back-to-back in the same send.
  if (capture.responseCreateEventRef !== context.requestHash) {
    return { status: "unavailable" };
  }
  for (const message of context.messages) {
    const text = volitionItemText(message);
    if (text?.startsWith(VOLITION_INJECTED_TEXT_PREFIX)) {
      return { status: "found", text };
    }
  }
  return { status: "none_injected" };
}

/// Extract the first content text from a `conversation.item.create` message, or null if the value
/// is not that shape. Defensive at every hop so a malformed capture can never throw.
function volitionItemText(message: unknown): string | null {
  if (!isRecord(message) || message.type !== "conversation.item.create") {
    return null;
  }
  const item = message.item;
  if (!isRecord(item)) {
    return null;
  }
  const content = item.content;
  if (!Array.isArray(content)) {
    return null;
  }
  const first = content[0];
  if (!isRecord(first) || typeof first.text !== "string") {
    return null;
  }
  return first.text;
}
