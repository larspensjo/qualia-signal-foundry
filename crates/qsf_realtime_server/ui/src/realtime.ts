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
  lastEvent: string | null;
  error: string | null;
}

export type ConversationAction =
  | { type: "session_requested" }
  | { type: "session_allocated"; sessionId: string }
  | { type: "connection_ready" }
  | { type: "provider_envelope"; envelope: RelayEnvelope }
  | { type: "connection_error"; message: string }
  | { type: "stop_requested" }
  | { type: "stopped" };

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
        interrupt_response: true,
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
  lastEvent: null,
  error: null,
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
        lastEvent: "stopped",
      };
  }
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
      };
    case "response_completed": {
      const text = envelope.text?.trim();
      return {
        ...base,
        phase: "speaking",
        transcript: text
          ? appendTranscript(state.transcript, { role: "assistant", text })
          : state.transcript,
      };
    }
    case "speech_playback_started":
      return {
        ...base,
        phase: "speaking",
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
      };
  }
}

function appendTranscript(entries: TranscriptEntry[], entry: TranscriptEntry): TranscriptEntry[] {
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
    text: message.text,
    status: message.status,
    audio_marker: message.audio_marker,
    payload: message.payload ?? message,
  };
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
    case "response.audio.delta":
      return "speech_playback_started";
    case "response.audio.done":
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
    response_id: stringField(parsed.response_id),
    transcript: stringField(parsed.transcript),
    text: stringField(parsed.text),
    status: stringField(parsed.status),
    audio_marker: stringField(parsed.audio_marker),
    payload: parsed.payload ?? parsed,
  };
}

function stringField(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
