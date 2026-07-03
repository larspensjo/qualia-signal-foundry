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
  selectVolitionPanelModel,
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
      winnerGoalId: "serve-the-present-person",
      winnerGoalTitle: "Serve the present person",
      winnerEffectiveTier: 2,
      winnerBiasedTier: 2,
      protectedTierActive: true,
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
      winner_goal_id: "serve-the-present-person",
      winner_goal_title: "Serve the present person",
      winner_effective_tier: 2,
      winner_biased_tier: 2,
      protected_tier_active: true,
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
        winnerGoalId: "serve-the-present-person",
        winnerGoalTitle: "Serve the present person",
        winnerEffectiveTier: 2,
        winnerBiasedTier: 2,
        protectedTierActive: true,
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
    });
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
      winnerGoalId: "serve-the-present-person",
      winnerGoalTitle: "Serve the present person",
      winnerEffectiveTier: 2,
      winnerBiasedTier: 2,
      protectedTierActive: true,
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
    expect(model.sections).toHaveLength(2);
    const decisionSection = model.sections[1];
    expect(decisionSection.title).toBe("Decision detail");
    const winnerTiers = decisionSection.rows.find((row) => row.label === "Winner tiers");
    expect(winnerTiers?.value).toContain("effective 2");
    expect(winnerTiers?.value).toContain("biased 2");
    expect(winnerTiers?.value).toContain("protected yes");
    expect(decisionSection.rows.find((row) => row.label === "Trace ref")?.value).toBe("hash-abc");
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
    expect(model.sections).toHaveLength(1);
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
});
