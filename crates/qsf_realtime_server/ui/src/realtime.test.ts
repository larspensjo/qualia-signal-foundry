import { describe, expect, it } from "vitest";

import {
  DEFAULT_SESSION_CONFIG,
  INITIAL_STATE,
  MICROPHONE_AUDIO_CONSTRAINTS,
  mapProviderMessageToRelayEnvelope,
  parseProviderDataChannelMessage,
  parseSidebandStatusMessage,
  parseTurnContextMessage,
  parseVolitionStateMessage,
  providerTypeToRelayKind,
  reduceConversationState,
  selectCanSubmitTextTurn,
  selectInjectedVolitionText,
  selectMuteButton,
  selectVolitionPanelModel,
  selectVolitionVerdict,
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

  it("maps assistant output transcript completion to visible response text", () => {
    const message = parseProviderDataChannelMessage(
      JSON.stringify({
        event_id: "evt_answer_done",
        type: "response.output_audio_transcript.done",
        response_id: "resp_1",
        transcript: "I am checking the realtime loop.",
      }),
    );

    expect(mapProviderMessageToRelayEnvelope("session_1", message)).toMatchObject({
      qsf_session_id: "session_1",
      event_id: "evt_answer_done",
      kind: "response_completed",
      response_id: "resp_1",
      text: "I am checking the realtime loop.",
    });
  });

  it("extracts assistant text from nested response output", () => {
    const message = parseProviderDataChannelMessage(
      JSON.stringify({
        event_id: "evt_response_done",
        type: "response.done",
        response: {
          id: "resp_1",
          status: "completed",
          output: [
            {
              type: "message",
              role: "assistant",
              content: [
                {
                  type: "audio",
                  transcript: "Here is the answer.",
                },
              ],
            },
          ],
        },
      }),
    );

    expect(mapProviderMessageToRelayEnvelope("session_1", message)).toMatchObject({
      qsf_session_id: "session_1",
      event_id: "evt_response_done",
      kind: "response_completed",
      response_id: "resp_1",
      status: "completed",
      text: "Here is the answer.",
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

  it("accumulates assistant transcript deltas until response completion", () => {
    const started = reduceConversationState(INITIAL_STATE, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_response_started",
        kind: "response_started",
      },
    });
    const firstDelta = reduceConversationState(started, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_answer_delta_1",
        kind: "speech_playback_started",
        text: "hi",
      },
    });
    const secondDelta = reduceConversationState(firstDelta, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_answer_delta_2",
        kind: "speech_playback_started",
        text: " there",
      },
    });
    const completed = reduceConversationState(secondDelta, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_answer_done",
        kind: "response_completed",
        status: "completed",
      },
    });

    expect(completed.transcript).toEqual([{ role: "assistant", text: "hi there" }]);
  });

  it("does not duplicate adjacent assistant completion text", () => {
    const firstCompletion = reduceConversationState(INITIAL_STATE, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_answer_done",
        kind: "response_completed",
        text: "same answer",
      },
    });
    const duplicateCompletion = reduceConversationState(firstCompletion, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_response_done",
        kind: "response_completed",
        text: "same answer",
      },
    });

    expect(duplicateCompletion.transcript).toEqual([{ role: "assistant", text: "same answer" }]);
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

  it("drops accumulated assistant draft text when a response is cancelled", () => {
    const withDraft = reduceConversationState(INITIAL_STATE, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_answer_delta",
        kind: "speech_playback_started",
        text: "partial answer",
      },
    });
    const cancelled = reduceConversationState(withDraft, {
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

describe("microphone mute", () => {
  it("toggles the muted gate on and off", () => {
    const muted = reduceConversationState(INITIAL_STATE, { type: "mute_toggled" });
    expect(muted.muted).toBe(true);

    const unmuted = reduceConversationState(muted, { type: "mute_toggled" });
    expect(unmuted.muted).toBe(false);
  });

  it("preserves a pre-armed mute across a stop", () => {
    const muted = reduceConversationState(INITIAL_STATE, { type: "mute_toggled" });
    const stopped = reduceConversationState(muted, { type: "stopped" });
    expect(stopped.muted).toBe(true);

    // A provider-driven session close must also leave the gate armed.
    const active = reduceConversationState(muted, {
      type: "session_allocated",
      sessionId: "session_1",
    });
    const closed = reduceConversationState(active, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_close",
        kind: "session_stopped",
      },
    });
    expect(closed.muted).toBe(true);
  });

  it("derives an action label and pressed state for the button", () => {
    expect(selectMuteButton(INITIAL_STATE)).toEqual({ label: "Mute", pressed: false });

    const muted = reduceConversationState(INITIAL_STATE, { type: "mute_toggled" });
    expect(selectMuteButton(muted)).toEqual({ label: "Unmute", pressed: true });
  });
});

describe("text turn availability", () => {
  const readyState = reduceConversationState(
    reduceConversationState(INITIAL_STATE, {
      type: "session_allocated",
      sessionId: "session_1",
    }),
    { type: "connection_ready" },
  );

  it("allows text during an idle page and throughout a ready live session", () => {
    const listening = reduceConversationState(readyState, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_listening",
        kind: "user_turn_started",
      },
    });
    const thinking = reduceConversationState(readyState, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_final",
        kind: "final_transcript",
        transcript: "hello",
      },
    });
    const speaking = reduceConversationState(readyState, {
      type: "provider_envelope",
      envelope: {
        qsf_session_id: "session_1",
        event_id: "evt_done",
        kind: "response_completed",
        status: "completed",
        text: "hi",
      },
    });

    for (const state of [INITIAL_STATE, readyState, listening, thinking, speaking]) {
      expect(selectCanSubmitTextTurn(state, { hasText: true, pending: false })).toBe(true);
    }
  });

  it("blocks empty, pending, transitional, and error states", () => {
    expect(selectCanSubmitTextTurn(readyState, { hasText: false, pending: false })).toBe(false);
    expect(selectCanSubmitTextTurn(readyState, { hasText: true, pending: true })).toBe(false);

    const requesting = reduceConversationState(INITIAL_STATE, { type: "session_requested" });
    const connecting = reduceConversationState(requesting, {
      type: "session_allocated",
      sessionId: "session_1",
    });
    const stopping = reduceConversationState(readyState, { type: "stop_requested" });
    const error = reduceConversationState(readyState, {
      type: "connection_error",
      message: "failed",
    });

    for (const state of [requesting, connecting, stopping, error]) {
      expect(selectCanSubmitTextTurn(state, { hasText: true, pending: false })).toBe(false);
    }
  });
});

describe("turn context reducer", () => {
  const activeState = reduceConversationState(INITIAL_STATE, {
    type: "session_allocated",
    sessionId: "session_1",
  });

  const sampleCapture = {
    qsfSessionId: "session_1",
    exchangeIndex: 3,
    capturedAt: "2026-06-30T12:00:00Z",
    requestHash: "abc123",
    messages: [{ role: "user", content: "hello" }],
  };

  it("sets latestTurnContext when capture matches the active session", () => {
    const next = reduceConversationState(activeState, {
      type: "turn_context_captured",
      capture: sampleCapture,
    });
    expect(next.latestTurnContext).toEqual(sampleCapture);
  });

  it("ignores turn_context_captured for a different session", () => {
    const mismatch = { ...sampleCapture, qsfSessionId: "session_other" };
    const next = reduceConversationState(activeState, {
      type: "turn_context_captured",
      capture: mismatch,
    });
    expect(next).toBe(activeState);
    expect(next.latestTurnContext).toBeNull();
  });

  it("preserves latestTurnContext on stopped so diagnostics remain visible", () => {
    const withCapture = reduceConversationState(activeState, {
      type: "turn_context_captured",
      capture: sampleCapture,
    });
    expect(withCapture.latestTurnContext).not.toBeNull();

    const stopped = reduceConversationState(withCapture, { type: "stopped" });
    expect(stopped.latestTurnContext).toEqual(sampleCapture);
  });

  it("clears latestTurnContext on session_allocated", () => {
    const withCapture = reduceConversationState(activeState, {
      type: "turn_context_captured",
      capture: sampleCapture,
    });
    expect(withCapture.latestTurnContext).not.toBeNull();

    const reallocated = reduceConversationState(withCapture, {
      type: "session_allocated",
      sessionId: "session_2",
    });
    expect(reallocated.latestTurnContext).toBeNull();
  });
});

describe("volition state reducer", () => {
  const activeState = reduceConversationState(INITIAL_STATE, {
    type: "session_allocated",
    sessionId: "session_1",
  });

  const sampleCapture = {
    qsfSessionId: "session_1",
    exchangeIndex: 4,
    capturedAt: "2026-06-30T12:00:00Z",
    responseCreateEventRef: "hash-abc",
    inspection: {
      mode: "neutral",
      tick: 12,
      activeGoals: [
        {
          id: "serve-the-present-person",
          title: "Serve the present person",
          salience: 9,
          cooldownUntilTick: null,
          lastActivatedTick: 11,
        },
      ],
      acceptedGoals: [],
      blockedGoals: [],
      cooldownGoals: [],
      retiredGoals: [],
      pendingCandidateCount: 1,
      acceptedCandidateCount: 2,
      lastInitiativeSummaries: [
        {
          goalId: "serve-the-present-person",
          goalTitle: "Serve the present person",
          outputKind: "reflection_requested",
        },
      ],
    },
    decision: {
      winner: {
        winnerGoalId: "serve-the-present-person",
        winnerGoalTitle: "Serve the present person",
        winnerEffectiveTier: 2,
        winnerBiasedTier: 2,
        protectedTierActive: true,
      },
      qualificationThreshold: 4,
      belowThreshold: [],
      modeBiasOutcomes: [
        {
          goalId: "serve-the-present-person",
          goalTitle: "Serve the present person",
          effectiveTier: 2,
          biasedTier: 2,
          protected: true,
        },
      ],
      selectedGoalIds: ["serve-the-present-person"],
      omittedOrSuppressedGoalIds: ["world-curiosity"],
      shapingIntensity: "low",
      lastInitiativeOutputKind: "reflection_requested",
      lastInitiativeSurfaced: true,
      lastInitiativeSuppressionReason: null,
      lastInitiativeRenderedLinePresent: true,
    },
    signals: [],
  };

  it("sets latestVolitionState when capture matches the active session", () => {
    const next = reduceConversationState(activeState, {
      type: "volition_state_captured",
      capture: sampleCapture,
    });
    expect(next.latestVolitionState).toEqual(sampleCapture);
  });

  it("ignores volition_state_captured for a different session", () => {
    const mismatch = { ...sampleCapture, qsfSessionId: "session_other" };
    const next = reduceConversationState(activeState, {
      type: "volition_state_captured",
      capture: mismatch,
    });
    expect(next).toBe(activeState);
    expect(next.latestVolitionState).toBeNull();
  });

  it("preserves latestVolitionState on stopped so diagnostics remain visible", () => {
    const withCapture = reduceConversationState(activeState, {
      type: "volition_state_captured",
      capture: sampleCapture,
    });
    expect(withCapture.latestVolitionState).not.toBeNull();

    const stopped = reduceConversationState(withCapture, { type: "stopped" });
    expect(stopped.latestVolitionState).toEqual(sampleCapture);
  });

  it("clears latestVolitionState on session_allocated", () => {
    const withCapture = reduceConversationState(activeState, {
      type: "volition_state_captured",
      capture: sampleCapture,
    });
    expect(withCapture.latestVolitionState).not.toBeNull();

    const reallocated = reduceConversationState(withCapture, {
      type: "session_allocated",
      sessionId: "session_2",
    });
    expect(reallocated.latestVolitionState).toBeNull();
  });
});

describe("turn context message parsing", () => {
  it("parses a valid wire message and maps snake_case to camelCase", () => {
    const result = parseTurnContextMessage(
      JSON.stringify({
        kind: "turn_context",
        qsf_session_id: "session_1",
        exchange_index: 5,
        captured_at: "2026-06-30T12:34:56Z",
        request_hash: "deadbeef",
        messages: [{ role: "user", content: "hi" }],
      }),
    );
    expect(result).toEqual({
      qsfSessionId: "session_1",
      exchangeIndex: 5,
      capturedAt: "2026-06-30T12:34:56Z",
      requestHash: "deadbeef",
      messages: [{ role: "user", content: "hi" }],
    });
    // capturedAt must remain a string, never a Date or number
    expect(typeof result?.capturedAt).toBe("string");
  });

  it("returns null for malformed JSON", () => {
    expect(parseTurnContextMessage("not json at all")).toBeNull();
  });

  it("returns null when kind is not turn_context", () => {
    expect(
      parseTurnContextMessage(
        JSON.stringify({
          kind: "sideband_status",
          qsf_session_id: "session_1",
          exchange_index: 1,
          captured_at: "2026-06-30T00:00:00Z",
          request_hash: "abc",
          messages: [],
        }),
      ),
    ).toBeNull();
  });

  it("returns null when a required field is missing", () => {
    // Missing exchange_index
    expect(
      parseTurnContextMessage(
        JSON.stringify({
          kind: "turn_context",
          qsf_session_id: "session_1",
          captured_at: "2026-06-30T00:00:00Z",
          request_hash: "abc",
          messages: [],
        }),
      ),
    ).toBeNull();
  });
});

describe("volition state message parsing", () => {
  const baseMessage = {
    kind: "volition_state",
    qsf_session_id: "session_1",
    exchange_index: 4,
    captured_at: "2026-06-30T12:00:00Z",
    response_create_event_ref: "hash-abc",
    inspection: {
      mode: "neutral",
      tick: 12,
      active_goals: [
        {
          id: "serve-the-present-person",
          title: "Serve the present person",
          salience: 9,
          cooldown_until_tick: null,
          last_activated_tick: 11,
        },
      ],
      accepted_goals: [],
      blocked_goals: [],
      cooldown_goals: [],
      retired_goals: [],
      pending_candidate_count: 1,
      accepted_candidate_count: 2,
      last_initiative_summaries: [
        {
          goal_id: "serve-the-present-person",
          goal_title: "Serve the present person",
          output_kind: "reflection_requested",
        },
      ],
    },
    decision: {
      winner: {
        winner_goal_id: "serve-the-present-person",
        winner_goal_title: "Serve the present person",
        winner_effective_tier: 2,
        winner_biased_tier: 2,
        protected_tier_active: true,
      },
      qualification_threshold: 4,
      below_threshold: [],
      mode_bias_outcomes: [
        {
          goal_id: "serve-the-present-person",
          goal_title: "Serve the present person",
          effective_tier: 2,
          biased_tier: 2,
          protected: true,
        },
      ],
      selected_goal_ids: ["serve-the-present-person"],
      omitted_or_suppressed_goal_ids: ["world-curiosity"],
      shaping_intensity: "low",
      last_initiative_output_kind: "reflection_requested",
      last_initiative_surfaced: true,
      last_initiative_suppression_reason: null,
      last_initiative_rendered_line_present: true,
    },
  };

  const signalsWire = [
    {
      kind: "coherence_decline",
      intensity: 1.0,
      evidence: {
        coherence_decline: {
          candidate_title: "Chase an unrelated tangent",
          conflict: { kind: "conflicting_goal", goal_id: "serve-the-present-person" },
          rationale: "would derail the current task",
          tick: 2,
        },
      },
    },
    {
      kind: "frustration",
      intensity: 0.5,
      evidence: {
        frustration: {
          goal_id: "assemble-world-picture",
          blocked_count: 2,
          last_blocked_tick: 3,
          last_activated_tick: 1,
        },
      },
    },
    {
      kind: "satisfaction",
      intensity: 0.8,
      evidence: {
        satisfaction: {
          goal_id: "serve-the-present-person",
          last_satisfied_tick: 2,
          evidence_ref: "trace: user thanked the assistant",
        },
      },
    },
    {
      kind: "boredom",
      intensity: 1.0,
      evidence: {
        boredom: {
          inspected: [
            { goal_id: "assemble-world-picture", salience: 0 },
            { goal_id: "serve-the-present-person", salience: 1 },
          ],
          threshold: 5,
          guard: "elapsed_ticks",
        },
      },
    },
  ];

  it("parses top-level functional signals into camelCase per-kind evidence", () => {
    const parsed = parseVolitionStateMessage(
      JSON.stringify({ ...baseMessage, signals: signalsWire }),
    );
    expect(parsed?.signals).toEqual([
      {
        kind: "coherence_decline",
        intensity: 1,
        evidence: {
          kind: "coherence_decline",
          candidateTitle: "Chase an unrelated tangent",
          conflict: { kind: "conflicting_goal", goalId: "serve-the-present-person" },
          rationale: "would derail the current task",
          tick: 2,
        },
      },
      {
        kind: "frustration",
        intensity: 0.5,
        evidence: {
          kind: "frustration",
          goalId: "assemble-world-picture",
          blockedCount: 2,
          lastBlockedTick: 3,
          lastActivatedTick: 1,
        },
      },
      {
        kind: "satisfaction",
        intensity: 0.8,
        evidence: {
          kind: "satisfaction",
          goalId: "serve-the-present-person",
          lastSatisfiedTick: 2,
          evidenceRef: "trace: user thanked the assistant",
        },
      },
      {
        kind: "boredom",
        intensity: 1,
        evidence: {
          kind: "boredom",
          inspected: [
            { goalId: "assemble-world-picture", salience: 0 },
            { goalId: "serve-the-present-person", salience: 1 },
          ],
          threshold: 5,
          guard: "elapsed_ticks",
        },
      },
    ]);
  });

  it("defaults a missing signals key to an empty list (back-compat)", () => {
    const parsed = parseVolitionStateMessage(JSON.stringify(baseMessage));
    expect(parsed).not.toBeNull();
    expect(parsed?.signals).toEqual([]);
  });

  it("drops malformed signal entries while keeping the message and valid signals", () => {
    const malformed = [
      signalsWire[1], // valid frustration
      { kind: "frustration", intensity: 0.4, evidence: { frustration: { goal_id: "x" } } }, // missing fields
      { intensity: "hot", evidence: { boredom: {} } }, // non-numeric intensity
      "garbage",
      null,
      {
        kind: "coherence_decline",
        intensity: 0.9,
        evidence: {
          coherence_decline: {
            candidate_title: "Overstate the status",
            conflict: { kind: "protected_floor" },
            rationale: "would breach the protected floor",
            tick: 4,
          },
        },
      }, // valid
    ];
    const parsed = parseVolitionStateMessage(
      JSON.stringify({ ...baseMessage, signals: malformed }),
    );
    expect(parsed).not.toBeNull();
    expect(parsed?.signals).toEqual([
      {
        kind: "frustration",
        intensity: 0.5,
        evidence: {
          kind: "frustration",
          goalId: "assemble-world-picture",
          blockedCount: 2,
          lastBlockedTick: 3,
          lastActivatedTick: 1,
        },
      },
      {
        kind: "coherence_decline",
        intensity: 0.9,
        evidence: {
          kind: "coherence_decline",
          candidateTitle: "Overstate the status",
          conflict: { kind: "protected_floor" },
          rationale: "would breach the protected floor",
          tick: 4,
        },
      },
    ]);
  });

  it("parses a well-formed state message with and without a decision", () => {
    expect(parseVolitionStateMessage(JSON.stringify(baseMessage))).toEqual({
      qsfSessionId: "session_1",
      exchangeIndex: 4,
      capturedAt: "2026-06-30T12:00:00Z",
      responseCreateEventRef: "hash-abc",
      inspection: {
        mode: "neutral",
        tick: 12,
        activeGoals: [
          {
            id: "serve-the-present-person",
            title: "Serve the present person",
            salience: 9,
            cooldownUntilTick: null,
            lastActivatedTick: 11,
          },
        ],
        acceptedGoals: [],
        blockedGoals: [],
        cooldownGoals: [],
        retiredGoals: [],
        pendingCandidateCount: 1,
        acceptedCandidateCount: 2,
        lastInitiativeSummaries: [
          {
            goalId: "serve-the-present-person",
            goalTitle: "Serve the present person",
            outputKind: "reflection_requested",
          },
        ],
      },
      decision: {
        winner: {
          winnerGoalId: "serve-the-present-person",
          winnerGoalTitle: "Serve the present person",
          winnerEffectiveTier: 2,
          winnerBiasedTier: 2,
          protectedTierActive: true,
        },
        qualificationThreshold: 4,
        belowThreshold: [],
        modeBiasOutcomes: [
          {
            goalId: "serve-the-present-person",
            goalTitle: "Serve the present person",
            effectiveTier: 2,
            biasedTier: 2,
            protected: true,
          },
        ],
        selectedGoalIds: ["serve-the-present-person"],
        omittedOrSuppressedGoalIds: ["world-curiosity"],
        shapingIntensity: "low",
        lastInitiativeOutputKind: "reflection_requested",
        lastInitiativeSurfaced: true,
        lastInitiativeSuppressionReason: null,
        lastInitiativeRenderedLinePresent: true,
      },
      signals: [],
    });

    expect(parseVolitionStateMessage(JSON.stringify({ ...baseMessage, decision: null }))).toEqual({
      qsfSessionId: "session_1",
      exchangeIndex: 4,
      capturedAt: "2026-06-30T12:00:00Z",
      responseCreateEventRef: "hash-abc",
      inspection: {
        mode: "neutral",
        tick: 12,
        activeGoals: [
          {
            id: "serve-the-present-person",
            title: "Serve the present person",
            salience: 9,
            cooldownUntilTick: null,
            lastActivatedTick: 11,
          },
        ],
        acceptedGoals: [],
        blockedGoals: [],
        cooldownGoals: [],
        retiredGoals: [],
        pendingCandidateCount: 1,
        acceptedCandidateCount: 2,
        lastInitiativeSummaries: [
          {
            goalId: "serve-the-present-person",
            goalTitle: "Serve the present person",
            outputKind: "reflection_requested",
          },
        ],
      },
      decision: null,
      signals: [],
    });
  });

  it("parses a no-winner decision with below-threshold candidates", () => {
    const noWinner = {
      ...baseMessage,
      decision: {
        winner: null,
        qualification_threshold: 4,
        below_threshold: [
          {
            goal_id: "serve-the-present-person",
            goal_title: "Serve the present person",
            matched_keywords: [{ term: "what", weight_class: "weak" }],
            match_strength: 1,
          },
        ],
        mode_bias_outcomes: [],
        selected_goal_ids: ["serve-the-present-person"],
        omitted_or_suppressed_goal_ids: [],
        shaping_intensity: "none",
        last_initiative_output_kind: null,
        last_initiative_surfaced: false,
        last_initiative_suppression_reason: "below_qualification_threshold",
        last_initiative_rendered_line_present: false,
      },
    };
    const parsed = parseVolitionStateMessage(JSON.stringify(noWinner));
    expect(parsed?.decision?.winner).toBeNull();
    expect(parsed?.decision?.qualificationThreshold).toBe(4);
    expect(parsed?.decision?.belowThreshold).toEqual([
      {
        goalId: "serve-the-present-person",
        goalTitle: "Serve the present person",
        matchedKeywords: [{ term: "what", weightClass: "weak" }],
        matchStrength: 1,
      },
    ]);
    expect(parsed?.decision?.lastInitiativeSuppressionReason).toBe("below_qualification_threshold");
  });

  it("returns null for malformed or wrong-kind messages", () => {
    expect(parseVolitionStateMessage("not json")).toBeNull();
    expect(parseVolitionStateMessage(JSON.stringify({ kind: "turn_context" }))).toBeNull();
    expect(
      parseVolitionStateMessage(
        JSON.stringify({
          ...baseMessage,
          inspection: { ...baseMessage.inspection, active_goals: [{}] },
        }),
      ),
    ).toBeNull();
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

describe("volition panel selector", () => {
  const sampleCapture = {
    qsfSessionId: "session_1",
    exchangeIndex: 4,
    capturedAt: "2026-06-30T12:00:00Z",
    responseCreateEventRef: "hash-abc",
    inspection: {
      mode: "neutral",
      tick: 12,
      activeGoals: [
        {
          id: "serve-the-present-person",
          title: "Serve the present person",
          salience: 9,
          cooldownUntilTick: null,
          lastActivatedTick: 11,
        },
      ],
      acceptedGoals: [],
      blockedGoals: [],
      cooldownGoals: [],
      retiredGoals: [],
      pendingCandidateCount: 1,
      acceptedCandidateCount: 2,
      lastInitiativeSummaries: [
        {
          goalId: "serve-the-present-person",
          goalTitle: "Serve the present person",
          outputKind: "reflection_requested",
        },
      ],
    },
    decision: {
      winner: {
        winnerGoalId: "serve-the-present-person",
        winnerGoalTitle: "Serve the present person",
        winnerEffectiveTier: 2,
        winnerBiasedTier: 2,
        protectedTierActive: true,
      },
      qualificationThreshold: 4,
      belowThreshold: [],
      modeBiasOutcomes: [
        {
          goalId: "serve-the-present-person",
          goalTitle: "Serve the present person",
          effectiveTier: 2,
          biasedTier: 2,
          protected: true,
        },
      ],
      selectedGoalIds: ["serve-the-present-person"],
      omittedOrSuppressedGoalIds: ["world-curiosity"],
      shapingIntensity: "low",
      lastInitiativeOutputKind: "reflection_requested",
      lastInitiativeSurfaced: true,
      lastInitiativeSuppressionReason: null,
      lastInitiativeRenderedLinePresent: true,
    },
    signals: [],
  };

  it("renders protected winner tiers and trace details without hard-coded ids", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: sampleCapture,
    };
    const model = selectVolitionPanelModel(state);

    expect(model.kind).toBe("decision");
    expect(model.banner).toBe("Decision captured for this trusted turn.");
    expect(model.sections).toHaveLength(3);
    const decisionSection = model.sections[1];
    expect(decisionSection.title).toBe("Decision detail");
    const winnerTiers = decisionSection.rows.find((row) => row.label === "Winner tiers");
    expect(winnerTiers?.value).toContain("effective 2");
    expect(winnerTiers?.value).toContain("biased 2");
    expect(winnerTiers?.value).toContain("protected yes");
    expect(decisionSection.rows.find((row) => row.label === "Trace ref")?.value).toBe("hash-abc");
  });

  it("renders a no-winner decision with the qualification headline and below-threshold detail", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: {
        ...sampleCapture,
        decision: {
          winner: null,
          qualificationThreshold: 4,
          belowThreshold: [
            {
              goalId: "serve-the-present-person",
              goalTitle: "Serve the present person",
              matchedKeywords: [{ term: "what", weightClass: "weak" as const }],
              matchStrength: 1,
            },
          ],
          modeBiasOutcomes: [],
          selectedGoalIds: ["serve-the-present-person"],
          omittedOrSuppressedGoalIds: [],
          shapingIntensity: "none",
          lastInitiativeOutputKind: null,
          lastInitiativeSurfaced: false,
          lastInitiativeSuppressionReason: "below_qualification_threshold" as const,
          lastInitiativeRenderedLinePresent: false,
        },
      },
    };
    const model = selectVolitionPanelModel(state);

    expect(model.kind).toBe("decision");
    expect(model.banner).toBe("No goal qualified this turn.");
    const decisionSection = model.sections[1];
    const winnerRow = decisionSection.rows.find((row) => row.label === "Winner");
    expect(winnerRow?.value).toBe("no goal qualified (threshold 4)");
    const belowRow = decisionSection.rows.find((row) => row.label === "Below threshold");
    expect(belowRow?.value).toContain("serve-the-present-person");
    expect(belowRow?.value).toContain("strength 1");
    expect(belowRow?.value).toContain("what/weak");
    const suppressionRow = decisionSection.rows.find((row) => row.label === "Suppression reason");
    expect(suppressionRow?.value).toBe("Below Qualification Threshold");
  });

  it("renders a no-decision snapshot with an explicit marker", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: { ...sampleCapture, decision: null },
    };
    const model = selectVolitionPanelModel(state);

    expect(model.kind).toBe("snapshot");
    expect(model.banner).toBe("No volition decision this turn.");
    expect(model.sections).toHaveLength(2);
    const snapshotSection = model.sections[0];
    expect(snapshotSection.rows.find((row) => row.label === "Tick")?.value).toBe("12");
    expect(snapshotSection.rows.find((row) => row.label === "Active goals")?.value).toContain(
      "Serve the present person",
    );
  });

  it("renders a stable empty state before any capture arrives", () => {
    const model = selectVolitionPanelModel(INITIAL_STATE);

    expect(model.kind).toBe("empty");
    expect(model.headline).toBe("No volition state yet");
    expect(model.banner).toBe("Awaiting the first trusted turn.");
    expect(model.sections).toHaveLength(0);
  });

  it("renders a Functional signals section with concrete evidence for each kind", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: {
        ...sampleCapture,
        signals: [
          {
            kind: "coherence_decline" as const,
            intensity: 1,
            evidence: {
              kind: "coherence_decline" as const,
              candidateTitle: "Chase a tangent",
              conflict: { kind: "conflicting_goal" as const, goalId: "serve-the-present-person" },
              rationale: "would derail the task",
              tick: 2,
            },
          },
          {
            kind: "frustration" as const,
            intensity: 0.5,
            evidence: {
              kind: "frustration" as const,
              goalId: "assemble-world-picture",
              blockedCount: 2,
              lastBlockedTick: 3,
              lastActivatedTick: 1,
            },
          },
          {
            kind: "satisfaction" as const,
            intensity: 0.8,
            evidence: {
              kind: "satisfaction" as const,
              goalId: "serve-the-present-person",
              lastSatisfiedTick: 2,
              evidenceRef: "trace: user thanked",
            },
          },
          {
            kind: "boredom" as const,
            intensity: 1,
            evidence: {
              kind: "boredom" as const,
              inspected: [{ goalId: "assemble-world-picture", salience: 0 }],
              threshold: 5,
              guard: "elapsed_ticks" as const,
            },
          },
        ],
      },
    };
    const model = selectVolitionPanelModel(state);
    const section = model.sections.find((s) => s.title === "Functional signals");
    expect(section).toBeDefined();
    const rows = section?.rows ?? [];
    expect(rows).toHaveLength(4);

    const coherence = rows.find((row) => row.label === "Coherence decline");
    expect(coherence?.value).toContain('declined "Chase a tangent"');
    expect(coherence?.value).toContain("tick 2");
    expect(coherence?.value).toContain("conflicts with goal serve-the-present-person");
    expect(coherence?.value).toContain("would derail the task");
    expect(coherence?.value).toContain("100%");

    const frustration = rows.find((row) => row.label === "Frustration");
    expect(frustration?.value).toContain("assemble-world-picture");
    expect(frustration?.value).toContain("blocked 2 times");
    expect(frustration?.value).toContain("last blocked tick 3");
    expect(frustration?.value).toContain("last activated tick 1");
    expect(frustration?.value).toContain("50%");

    const satisfaction = rows.find((row) => row.label === "Satisfaction");
    expect(satisfaction?.value).toContain("serve-the-present-person");
    expect(satisfaction?.value).toContain("satisfied at tick 2");
    expect(satisfaction?.value).toContain("trace: user thanked");
    expect(satisfaction?.value).toContain("80%");

    const boredom = rows.find((row) => row.label === "Boredom");
    expect(boredom?.value).toContain("threshold 5");
    expect(boredom?.value).toContain("assemble-world-picture salience 0");
    expect(boredom?.value).toContain("elapsed ticks");
    expect(boredom?.value).toContain("100%");
  });

  it("shows a single empty-state row in the Functional signals section when there are no signals", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: { ...sampleCapture, signals: [] },
    };
    const model = selectVolitionPanelModel(state);
    const section = model.sections.find((s) => s.title === "Functional signals");
    expect(section).toBeDefined();
    expect(section?.rows).toEqual([{ label: "Signals", value: "none" }]);
  });
});

describe("volition verdict selector", () => {
  const unavailable = { status: "unavailable" } as const;
  const spokeCapture = {
    qsfSessionId: "session_1",
    exchangeIndex: 4,
    capturedAt: "2026-06-30T12:00:00Z",
    responseCreateEventRef: "hash-abc",
    inspection: {
      mode: "neutral",
      tick: 12,
      activeGoals: [],
      acceptedGoals: [],
      blockedGoals: [],
      cooldownGoals: [],
      retiredGoals: [],
      pendingCandidateCount: 0,
      acceptedCandidateCount: 0,
      lastInitiativeSummaries: [],
    },
    decision: {
      winner: {
        winnerGoalId: "serve-the-present-person",
        winnerGoalTitle: "Serve the present person",
        winnerEffectiveTier: 2,
        winnerBiasedTier: 2,
        protectedTierActive: true,
      },
      qualificationThreshold: 4,
      belowThreshold: [],
      modeBiasOutcomes: [],
      selectedGoalIds: ["serve-the-present-person"],
      omittedOrSuppressedGoalIds: [],
      shapingIntensity: "low",
      lastInitiativeOutputKind: "reflection_requested",
      lastInitiativeSurfaced: true,
      lastInitiativeSuppressionReason: null,
      lastInitiativeRenderedLinePresent: true,
    },
    signals: [],
  };

  it("reports awaiting-first-turn when no capture has arrived", () => {
    const verdict = selectVolitionVerdict(INITIAL_STATE, unavailable);
    expect(verdict.kind).toBe("not_evaluated");
    expect(verdict.line).toContain("No evaluated turn yet");
    expect(verdict.caption).toBeNull();
    expect(verdict.nudge).toBeNull();
  });

  it("reports a spoken verdict with the winning goal, intensity word, and added nudge", () => {
    const state = { ...INITIAL_STATE, sessionId: "session_1", latestVolitionState: spokeCapture };
    const verdict = selectVolitionVerdict(state, unavailable);
    expect(verdict.kind).toBe("spoke");
    expect(verdict.line).toContain("Serve the present person");
    expect(verdict.line).toContain("gently");
    expect(verdict.caption).toBe("Latest evaluated turn · exchange 4");
    expect(verdict.nudge).toBe("nudge added");
  });

  it("reports a held-back nudge with the suppression reason when no line was rendered", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: {
        ...spokeCapture,
        decision: {
          ...spokeCapture.decision,
          lastInitiativeSurfaced: false,
          lastInitiativeSuppressionReason: "anti_nag_repeat" as const,
          lastInitiativeRenderedLinePresent: false,
        },
      },
    };
    const verdict = selectVolitionVerdict(state, unavailable);
    expect(verdict.kind).toBe("spoke");
    expect(verdict.nudge).toBe("nudge held back (Anti Nag Repeat)");
  });

  it("reports a quiet verdict with the below-threshold count and bar", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: {
        ...spokeCapture,
        decision: {
          ...spokeCapture.decision,
          winner: null,
          belowThreshold: [
            {
              goalId: "learn-what-drives-this-person",
              goalTitle: "Learn what drives this person",
              matchedKeywords: [{ term: "me", weightClass: "weak" as const }],
              matchStrength: 1,
            },
          ],
          shapingIntensity: "none",
        },
      },
    };
    const verdict = selectVolitionVerdict(state, unavailable);
    expect(verdict.kind).toBe("quiet");
    expect(verdict.line).toContain("No goal qualified to lead this turn");
    expect(verdict.line).toContain("1 goal(s) below the bar (threshold 4)");
    expect(verdict.line).not.toContain("base-model");
    expect(verdict.caption).toBe("Latest evaluated turn · exchange 4");
  });

  it("reports context-only when a no-decision capture pairs with an injected packet", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: { ...spokeCapture, decision: null },
    };
    const verdict = selectVolitionVerdict(state, {
      status: "found",
      text: "Simulated volition context for this turn (internal state only; not a claim of real desire or consciousness).\nDeclined goal candidates (coherence): pursue an unrelated tangent — would derail the current task.",
    });
    expect(verdict.kind).toBe("context_only");
    expect(verdict.line).toContain("still injected context");
  });

  it("reports a no-decision verdict when a capture has no decision and no packet was found", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: { ...spokeCapture, decision: null },
    };
    const verdict = selectVolitionVerdict(state, { status: "none_injected" });
    expect(verdict.kind).toBe("no_decision");
    expect(verdict.line).toContain("no per-turn decision");
  });
});

describe("injected volition text locator", () => {
  const volitionMessage = {
    type: "conversation.item.create",
    item: {
      type: "message",
      role: "system",
      content: [
        {
          type: "input_text",
          text: "Simulated volition context for this turn (internal state only; not a claim of real desire or consciousness).\nActive goal: Serve the present person (serve-the-present-person) — be useful now.",
        },
      ],
    },
  };
  const memoryMessage = {
    type: "conversation.item.create",
    item: {
      type: "message",
      role: "system",
      content: [{ type: "input_text", text: "Relevant memories: none." }],
    },
  };
  const captureAtExchange = (exchangeIndex: number, requestHash = "hash-abc") => ({
    qsfSessionId: "session_1",
    exchangeIndex,
    capturedAt: "2026-06-30T12:00:00Z",
    responseCreateEventRef: requestHash,
    inspection: {
      mode: "neutral",
      tick: 12,
      activeGoals: [],
      acceptedGoals: [],
      blockedGoals: [],
      cooldownGoals: [],
      retiredGoals: [],
      pendingCandidateCount: 0,
      acceptedCandidateCount: 0,
      lastInitiativeSummaries: [],
    },
    decision: null,
    signals: [],
  });
  const turnContextAtExchange = (
    exchangeIndex: number,
    messages: unknown[],
    requestHash = "hash-abc",
  ) => ({
    qsfSessionId: "session_1",
    exchangeIndex,
    capturedAt: "2026-06-30T12:00:00Z",
    requestHash,
    messages,
  });

  const expectFound = (result: ReturnType<typeof selectInjectedVolitionText>): string => {
    if (result.status !== "found") {
      throw new Error(`expected found, got ${result.status}`);
    }
    return result.text;
  };

  it("returns the verbatim volition item text when both captures describe the same turn", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: captureAtExchange(4),
      latestTurnContext: turnContextAtExchange(4, [memoryMessage, volitionMessage]),
    };
    const text = expectFound(selectInjectedVolitionText(state));
    expect(text).toContain("Active goal: Serve the present person");
    expect(text).toMatch(/^Simulated volition context for this turn/);
  });

  it("reports unavailable when the two captures describe different turns", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: captureAtExchange(5, "hash-turn-5"),
      latestTurnContext: turnContextAtExchange(4, [volitionMessage], "hash-turn-4"),
    };
    expect(selectInjectedVolitionText(state).status).toBe("unavailable");
  });

  it("reports unavailable when captures share an exchange but are different attempts", () => {
    // Two response.create attempts in one exchange keep the same exchangeIndex but get distinct
    // request hashes, so correlating by request hash (not exchangeIndex) rejects the mismatch.
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: captureAtExchange(4, "hash-attempt-b"),
      latestTurnContext: turnContextAtExchange(4, [volitionMessage], "hash-attempt-a"),
    };
    expect(selectInjectedVolitionText(state).status).toBe("unavailable");
  });

  it("reports none injected when the matched turn context has no volition item", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: captureAtExchange(4),
      latestTurnContext: turnContextAtExchange(4, [memoryMessage]),
    };
    expect(selectInjectedVolitionText(state).status).toBe("none_injected");
  });

  it("reports unavailable when either capture is missing", () => {
    expect(selectInjectedVolitionText(INITIAL_STATE).status).toBe("unavailable");
    expect(
      selectInjectedVolitionText({
        ...INITIAL_STATE,
        sessionId: "session_1",
        latestVolitionState: captureAtExchange(4),
      }).status,
    ).toBe("unavailable");
  });

  it("tolerates malformed messages without throwing", () => {
    const state = {
      ...INITIAL_STATE,
      sessionId: "session_1",
      latestVolitionState: captureAtExchange(4),
      latestTurnContext: turnContextAtExchange(4, [
        null,
        "a string",
        { type: "conversation.item.create" },
        { type: "conversation.item.create", item: { content: [] } },
        volitionMessage,
      ]),
    };
    expect(expectFound(selectInjectedVolitionText(state))).toContain("Active goal:");
  });
});
