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

export type VolitionSuppressionReason =
  | "intensity"
  | "protected_no_opportunity"
  | "anti_nag_repeat"
  | "non_renderable_output";

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

export interface VolitionTurnDecisionSummary {
  winnerGoalId: string;
  winnerGoalTitle: string;
  winnerEffectiveTier: number;
  winnerBiasedTier: number;
  protectedTierActive: boolean;
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
  lastEvent: string | null;
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
  | { type: "provider_envelope"; envelope: RelayEnvelope }
  | { type: "connection_error"; message: string }
  | { type: "server_status"; sessionId: string; degraded: boolean; detail: string | null }
  | { type: "stop_requested" }
  | { type: "stopped" }
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
  lastEvent: null,
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
      return applyRelayEnvelope(state, action.envelope);
    case "connection_error":
      return {
        ...state,
        connection: "error",
        error: action.message,
        lastEvent: action.message,
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
        lastEvent: "stopping",
      };
    case "stopped":
      return {
        ...state,
        connection: "idle",
        phase: "idle",
        sessionId: null,
        liveTranscript: "",
        responseDraft: "",
        lastEvent: "stopped",
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
    winner_goal_id: string;
    winner_goal_title: string;
    winner_effective_tier: number;
    winner_biased_tier: number;
    protected_tier_active: boolean;
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
    winnerGoalId: wire.winner_goal_id,
    winnerGoalTitle: wire.winner_goal_title,
    winnerEffectiveTier: wire.winner_effective_tier,
    winnerBiasedTier: wire.winner_biased_tier,
    protectedTierActive: wire.protected_tier_active,
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

function isVolitionTurnDecisionSummary(value: unknown): boolean {
  if (!isRecord(value)) {
    return false;
  }
  return (
    typeof value.winner_goal_id === "string" &&
    typeof value.winner_goal_title === "string" &&
    typeof value.winner_effective_tier === "number" &&
    typeof value.winner_biased_tier === "number" &&
    typeof value.protected_tier_active === "boolean" &&
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
    value === "non_renderable_output"
  );
}

function applyRelayEnvelope(state: ConversationState, envelope: RelayEnvelope): ConversationState {
  const base = {
    ...state,
    lastEvent: envelope.kind,
  };

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
        liveTranscript: envelope.transcript ?? state.liveTranscript,
      };
    case "final_transcript": {
      const text = envelope.transcript?.trim();
      return {
        ...base,
        phase: "thinking",
        liveTranscript: "",
        transcript: text
          ? appendTranscript(state.transcript, { role: "user", text })
          : state.transcript,
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
      const text = (envelope.text ?? (completed ? state.responseDraft : "")).trim();
      const phase = completed ? "speaking" : "idle";
      return {
        ...base,
        phase,
        responseDraft: "",
        transcript: text
          ? appendTranscript(state.transcript, { role: "assistant", text })
          : state.transcript,
      };
    }
    case "speech_playback_started":
      return {
        ...base,
        phase: "speaking",
        responseDraft: state.responseDraft + (envelope.text ?? ""),
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
  return {
    kind: "decision",
    headline: "Volition state",
    banner: "Decision captured for this trusted turn.",
    sections: snapshotSections.concat({
      title: "Decision detail",
      rows: [
        {
          label: "Winner",
          value: `${decision.winnerGoalTitle} [${decision.winnerGoalId}]`,
        },
        {
          label: "Winner tiers",
          value: `effective ${decision.winnerEffectiveTier}, biased ${decision.winnerBiasedTier}, protected ${yesNo(decision.protectedTierActive)}`,
        },
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
