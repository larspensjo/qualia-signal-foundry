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
export const PHASE_LANE_IDLE_CAP_MS = 2_000;

/// Fixed lane-time width of a compressed gap's break band.
export const PHASE_LANE_BREAK_LANE_MS = 1_500;

export type VolitionSuppressionReason =
  | "intensity"
  | "protected_no_opportunity"
  | "anti_nag_repeat"
  | "non_renderable_output"
  | "below_qualification_threshold";

export interface VolitionGoalStatusSummary {
  id: string;
  title: string;
  salience: number;
  cooldownUntilTick: number | null;
  lastActivatedTick: number | null;
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
}

export interface VolitionInspectionCapture {
  qsfSessionId: string;
  exchangeIndex: number;
  capturedAt: string;
  responseCreateEventRef: string;
  inspection: VolitionStateInspectionCapture;
  decision: VolitionTurnDecisionSummary | null;
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
  | { type: "volition_state_captured"; capture: VolitionInspectionCapture };

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
  };
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
  };
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
}): VolitionGoalStatusSummary {
  return {
    id: value.id,
    title: value.title,
    salience: value.salience,
    cooldownUntilTick: value.cooldown_until_tick,
    lastActivatedTick: value.last_activated_tick,
  };
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
      const phase = completed ? "speaking" : "idle";
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
        { label: "Active goals", value: formatGoalSummaries(capture.inspection.activeGoals) },
        { label: "Accepted goals", value: formatGoalSummaries(capture.inspection.acceptedGoals) },
        { label: "Blocked goals", value: formatGoalSummaries(capture.inspection.blockedGoals) },
        { label: "Cooldown goals", value: formatGoalSummaries(capture.inspection.cooldownGoals) },
        { label: "Retired goals", value: formatGoalSummaries(capture.inspection.retiredGoals) },
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

  if (capture.decision === null) {
    return {
      kind: "snapshot",
      headline: "Volition state",
      banner: "No volition decision this turn.",
      sections: snapshotSections,
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
    sections: snapshotSections.concat({
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
          label: "Trace ref",
          value: capture.responseCreateEventRef,
        },
      ],
    }),
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

function formatGoalSummaries(goals: VolitionGoalStatusSummary[]): string {
  if (goals.length === 0) {
    return "none";
  }
  return goals.map((goal) => `${goal.title} [${goal.id}]`).join("; ");
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
