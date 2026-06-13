import { describe, expect, it } from "vitest";

import {
  DEFAULT_SESSION_CONFIG,
  INITIAL_STATE,
  MICROPHONE_AUDIO_CONSTRAINTS,
  mapProviderMessageToRelayEnvelope,
  parseProviderDataChannelMessage,
  parseSidebandStatusMessage,
  providerTypeToRelayKind,
  reduceConversationState,
} from "./realtime";

describe("provider relay mapping", () => {
  it("maps provider messages to typed relay envelopes", () => {
    const message = parseProviderDataChannelMessage(
      JSON.stringify({
        event_id: "evt_1",
        type: "conversation.item.input_audio_transcription.completed",
        item_id: "item_1",
        transcript: "hello world",
        payload: { raw: true },
      }),
    );

    expect(mapProviderMessageToRelayEnvelope("session_1", message)).toEqual({
      qsf_session_id: "session_1",
      event_id: "evt_1",
      kind: "final_transcript",
      item_id: "item_1",
      previous_item_id: undefined,
      response_id: undefined,
      transcript: "hello world",
      text: undefined,
      status: undefined,
      audio_marker: undefined,
      payload: { raw: true },
    });
  });

  it("extracts nested response status and id from provider messages", () => {
    const message = parseProviderDataChannelMessage(
      JSON.stringify({
        event_id: "evt_cancelled",
        type: "response.done",
        response: {
          id: "resp_cancelled",
          status: "cancelled",
        },
      }),
    );

    expect(mapProviderMessageToRelayEnvelope("session_1", message)).toEqual({
      qsf_session_id: "session_1",
      event_id: "evt_cancelled",
      kind: "response_completed",
      item_id: undefined,
      previous_item_id: undefined,
      response_id: "resp_cancelled",
      transcript: undefined,
      text: undefined,
      status: "cancelled",
      audio_marker: undefined,
      payload: {
        event_id: "evt_cancelled",
        type: "response.done",
        response: {
          id: "resp_cancelled",
          status: "cancelled",
        },
      },
    });
  });

  it("ignores unsupported provider event types", () => {
    expect(providerTypeToRelayKind("unknown.type")).toBeNull();
    expect(
      mapProviderMessageToRelayEnvelope("session_1", {
        event_id: "evt_ignored",
        type: "rate_limits.updated",
      }),
    ).toBeNull();
  });
});

describe("microphone capture constraints", () => {
  it("enables browser echo cancellation, noise suppression, and auto gain control by default", () => {
    expect(MICROPHONE_AUDIO_CONSTRAINTS).toEqual({
      echoCancellation: true,
      noiseSuppression: true,
      autoGainControl: true,
    });
  });

  it("leaves provider auto-interrupt disabled by default", () => {
    expect(DEFAULT_SESSION_CONFIG.audio.input.turn_detection.interrupt_response).toBe(false);
  });
});

describe("conversation reducer", () => {
  it("tracks listening, thinking, speaking, and idle transitions", () => {
    const afterPartial = reduceConversationState(INITIAL_STATE, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_2",
        kind: "partial_transcript",
        transcript: "hel",
      },
    });

    expect(afterPartial.phase).toBe("listening");
    expect(afterPartial.liveTranscript).toBe("hel");

    const afterFinal = reduceConversationState(afterPartial, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_3",
        kind: "final_transcript",
        transcript: "hello",
      },
    });

    expect(afterFinal.phase).toBe("thinking");
    expect(afterFinal.liveTranscript).toBe("");
    expect(afterFinal.transcript).toEqual([{ role: "user", text: "hello" }]);

    const afterResponse = reduceConversationState(afterFinal, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_4",
        kind: "response_completed",
        text: "hi there",
      },
    });

    expect(afterResponse.phase).toBe("speaking");
    expect(afterResponse.transcript).toEqual([
      { role: "user", text: "hello" },
      { role: "assistant", text: "hi there" },
    ]);

    const afterPlayback = reduceConversationState(afterResponse, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_5",
        kind: "speech_playback_completed",
      },
    });

    expect(afterPlayback.phase).toBe("idle");
  });

  it("returns to idle when a response is cancelled", () => {
    const speaking = {
      ...INITIAL_STATE,
      phase: "speaking" as const,
    };

    const cancelled = reduceConversationState(speaking, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_cancelled",
        kind: "response_completed",
        status: "cancelled",
      },
    });

    expect(cancelled.phase).toBe("idle");
    expect(cancelled.transcript).toEqual([]);
  });

  it("raises and clears a warning from sideband status for the active session", () => {
    const active = reduceConversationState(INITIAL_STATE, {
      type: "session_allocated",
      sessionId: "session_1",
    });

    const degraded = reduceConversationState(active, {
      type: "server_status",
      sessionId: "session_1",
      degraded: true,
      detail: "failed to connect sideband websocket",
    });
    expect(degraded.warning).toBe("failed to connect sideband websocket");

    const recovered = reduceConversationState(degraded, {
      type: "server_status",
      sessionId: "session_1",
      degraded: false,
      detail: null,
    });
    expect(recovered.warning).toBeNull();
  });

  it("falls back to a default warning when degraded without detail", () => {
    const active = reduceConversationState(INITIAL_STATE, {
      type: "session_allocated",
      sessionId: "session_1",
    });
    const degraded = reduceConversationState(active, {
      type: "server_status",
      sessionId: "session_1",
      degraded: true,
      detail: null,
    });
    expect(degraded.warning).not.toBeNull();
    expect(degraded.warning?.length ?? 0).toBeGreaterThan(0);
  });

  it("ignores sideband status for a stale or mismatched session", () => {
    const active = reduceConversationState(INITIAL_STATE, {
      type: "session_allocated",
      sessionId: "session_2",
    });

    // A status pushed for a previous/other session must not raise a warning.
    const ignored = reduceConversationState(active, {
      type: "server_status",
      sessionId: "session_1",
      degraded: true,
      detail: "stale degraded status",
    });
    expect(ignored.warning).toBeNull();
    expect(ignored).toBe(active);

    // After stop, sessionId is null, so a late message is still ignored.
    const stopped = reduceConversationState(active, { type: "stopped" });
    const afterStop = reduceConversationState(stopped, {
      type: "server_status",
      sessionId: "session_2",
      degraded: true,
      detail: "late degraded status",
    });
    expect(afterStop.warning).toBeNull();
  });
});

describe("sideband status message parsing", () => {
  it("parses a well-formed status message", () => {
    expect(
      parseSidebandStatusMessage(
        JSON.stringify({
          kind: "sideband_status",
          qsf_session_id: "session_1",
          degraded: true,
          detail: "boom",
        }),
      ),
    ).toEqual({
      kind: "sideband_status",
      qsf_session_id: "session_1",
      degraded: true,
      detail: "boom",
    });
  });

  it("ignores relay acks and malformed payloads", () => {
    expect(
      parseSidebandStatusMessage(
        JSON.stringify({ qsf_session_id: "session_1", event_id: "evt_1", accepted: true }),
      ),
    ).toBeNull();
    expect(parseSidebandStatusMessage("not json")).toBeNull();
  });
});
