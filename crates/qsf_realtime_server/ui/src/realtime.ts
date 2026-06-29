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

export interface ConversationState {
  connection: ConnectionPhase;
  phase: RuntimePhase;
  sessionId: string | null;
  transcript: TranscriptEntry[];
  liveTranscript: string;
  responseDraft: string;
  lastEvent: string | null;
  error: string | null;
  warning: string | null;
}

export type ConversationAction =
  | { type: "session_requested" }
  | { type: "session_allocated"; sessionId: string }
  | { type: "connection_ready" }
  | { type: "provider_envelope"; envelope: RelayEnvelope }
  | { type: "connection_error"; message: string }
  | { type: "server_status"; sessionId: string; degraded: boolean; detail: string | null }
  | { type: "stop_requested" }
  | { type: "stopped" };

/// Server-originated status message pushed over the events socket, distinct
/// from relay acks by its `kind` discriminator.
export interface SidebandStatusMessage {
  kind: "sideband_status";
  qsf_session_id: string;
  degraded: boolean;
  detail: string | null;
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
  sessionId: null,
  transcript: [],
  liveTranscript: "",
  responseDraft: "",
  lastEvent: null,
  error: null,
  warning: null,
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
    case "session_allocated":
      return {
        ...state,
        connection: "connecting_media",
        sessionId: action.sessionId,
        error: null,
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
