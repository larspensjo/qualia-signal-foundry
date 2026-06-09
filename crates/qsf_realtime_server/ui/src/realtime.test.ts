import { describe, expect, it } from "vitest";

import {
  INITIAL_STATE,
  mapProviderMessageToRelayEnvelope,
  parseProviderDataChannelMessage,
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
});
