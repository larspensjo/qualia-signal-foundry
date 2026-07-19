import "./styles.css";
import { attachPhaseLane } from "./phase-lane";
import {
  type ConversationAction,
  type ConversationState,
  describeConnection,
  describeRuntimePhase,
  INITIAL_STATE,
  MICROPHONE_AUDIO_CONSTRAINTS,
  mapProviderMessageToRelayEnvelope,
  parseProviderDataChannelMessage,
  parseSidebandStatusMessage,
  parseTokenUsageMessage,
  parseTurnContextMessage,
  parseVolitionStateMessage,
  parseWorldPerceptionMessage,
  reduceConversationState,
  type SdpExchangeResponse,
  type SessionAllocationResponse,
  selectCanSubmitTextTurn,
  selectEventTickerModel,
  selectInjectedVolitionText,
  selectMuteButton,
  selectTokenUsagePanelModel,
  selectVolitionPanelModel,
  selectVolitionVerdict,
  selectWorldPerceptionPanelModel,
  type TokenUsagePanelModel,
  type VolitionPanelModel,
  type VolitionPanelRow,
  type VolitionPanelSection,
  type WorldPerceptionPanelModel,
} from "./realtime";

interface UiRefs {
  startButton: HTMLButtonElement;
  stopButton: HTMLButtonElement;
  muteButton: HTMLButtonElement;
  textForm: HTMLFormElement;
  textInput: HTMLTextAreaElement;
  sendTextButton: HTMLButtonElement;
  sessionId: HTMLElement;
  connectionStatus: HTMLElement;
  runtimePhase: HTMLElement;
  liveTranscript: HTMLElement;
  eventTicker: HTMLOListElement;
  transcriptList: HTMLOListElement;
  errorBanner: HTMLElement;
  warningBanner: HTMLElement;
  remoteAudio: HTMLAudioElement;
  volitionStateBody: HTMLElement;
  worldPerceptionBody: HTMLElement;
  tokenUsageBody: HTMLElement;
  phaseLaneCanvas: HTMLCanvasElement;
  phaseLaneTip: HTMLElement;
}

interface ActiveConversation {
  sessionId: string;
  peerConnection: RTCPeerConnection;
  microphoneStream: MediaStream | null;
  relaySocket: WebSocket;
  dataChannel: RTCDataChannel;
  relayBuffer: string[];
}

interface ConversationStartOptions {
  captureMicrophone: boolean;
}

interface TextTurnResponse {
  qsf_session_id: string;
  accepted: boolean;
}

const root = document.querySelector<HTMLElement>("#app");
if (!root) {
  throw new Error("missing #app root");
}

root.innerHTML = `
  <main class="shell">
    <header class="toolbar">
      <p class="brand">QSF realtime voice</p>
      <dl class="status-chips">
        <div>
          <dt>Connection</dt>
          <dd data-role="connection">Idle</dd>
        </div>
        <div>
          <dt>Phase</dt>
          <dd data-role="phase">Idle</dd>
        </div>
        <div>
          <dt>Session</dt>
          <dd data-role="session">—</dd>
        </div>
      </dl>
      <button data-role="start" type="button">Start conversation</button>
      <button data-role="stop" type="button" disabled>Stop</button>
      <button data-role="mute" type="button" aria-pressed="false" title="Stop sending your microphone; the assistant stays live">Mute</button>
      <form data-role="text-form" class="text-turn-form">
        <textarea data-role="text-input" rows="1" placeholder="Type a turn for noisy rooms"></textarea>
        <button data-role="send-text" type="submit">Send text</button>
      </form>
      <p data-role="error" class="error" hidden></p>
      <p data-role="warning" class="warning" role="status" hidden></p>
    </header>

    <section class="grid">
      <article class="panel transcript-panel">
        <div class="panel-header">
          <h2>Transcript</h2>
          <span class="status-pill">Live</span>
        </div>
        <p data-role="live-transcript" class="live-transcript" aria-live="polite">Waiting for the first turn.</p>
        <ol data-role="transcript" class="transcript" aria-label="Conversation transcript"></ol>
      </article>

      <article class="panel events-panel">
        <div class="panel-header">
          <h2>Events</h2>
          <span class="status-pill muted">Browser view</span>
        </div>
        <ol data-role="event-ticker" class="event-ticker" aria-label="Recent relay events"></ol>
        <dl class="details channel-facts">
          <div>
            <dt>Media</dt>
            <dd>Direct browser to OpenAI</dd>
          </div>
          <div>
            <dt>Relay</dt>
            <dd>Typed browser-to-server envelopes</dd>
          </div>
        </dl>
      </article>

      <aside class="panel volition-panel">
        <div class="panel-header">
          <h2>Volition</h2>
        </div>
        <div class="volition-scroll">
          <details class="turn-context-details" open>
            <summary>What volition did this turn</summary>
            <div data-role="volition-state-body" class="volition-state-body"></div>
          </details>
        </div>
      </aside>

      <aside class="panel world-perception-panel">
        <div class="panel-header">
          <h2>World perception</h2>
        </div>
        <div data-role="world-perception-body" class="world-perception-body"></div>
      </aside>
    </section>

    <section class="bottom-strip">
      <section class="panel phase-strip">
        <div class="phase-strip-header">
          <h2>Phase timeline</h2>
          <ul class="phase-lane-legend" aria-hidden="true">
            <li><i style="background: var(--phase-idle)"></i>idle</li>
            <li><i style="background: var(--phase-listening)"></i>listening</li>
            <li><i style="background: var(--phase-thinking)"></i>thinking</li>
            <li><i style="background: var(--phase-speaking)"></i>speaking</li>
            <li><i class="legend-gap"></i>skipped idle</li>
          </ul>
        </div>
        <div class="phase-lane-wrap">
          <canvas data-role="phase-lane" aria-label="Runtime phase timeline, last 60 seconds of activity"></canvas>
          <div data-role="phase-lane-tip" class="phase-lane-tip" hidden></div>
        </div>
      </section>

      <aside class="panel token-panel">
        <div class="panel-header">
          <h2>Tokens</h2>
          <span class="status-pill muted">Session totals</span>
        </div>
        <div data-role="token-usage-body" class="token-usage-body"></div>
      </aside>
    </section>

    <audio data-role="remote-audio" autoplay playsinline></audio>
  </main>
`;

const refs = collectRefs(root);
let state: ConversationState = INITIAL_STATE;
let activeConversation: ActiveConversation | null = null;
let textTurnPending = false;

refs.startButton.addEventListener("click", () => {
  void startConversation({ captureMicrophone: true });
});
refs.stopButton.addEventListener("click", () => {
  void stopConversation();
});
refs.muteButton.addEventListener("click", () => {
  dispatch({ type: "mute_toggled" });
  applyMicrophoneMute(state.muted);
});
refs.textForm.addEventListener("submit", (event) => {
  event.preventDefault();
  void submitTextTurn();
});
refs.textInput.addEventListener("input", () => {
  render();
});

render();
attachPhaseLane(refs.phaseLaneCanvas, refs.phaseLaneTip, () => state);

async function startConversation(options: ConversationStartOptions): Promise<boolean> {
  if (activeConversation) {
    return true;
  }

  dispatch({ type: "session_requested" });

  let relaySocket: WebSocket | null = null;
  let peerConnection: RTCPeerConnection | null = null;
  let microphoneStream: MediaStream | null = null;
  let dataChannel: RTCDataChannel | null = null;
  const relayBuffer: string[] = [];

  try {
    const allocation = await postJson<SessionAllocationResponse>("/api/realtime/session", {
      method: "POST",
    });

    dispatch({
      type: "session_allocated",
      sessionId: allocation.qsf_session_id,
    });

    relaySocket = await connectRelaySocket(allocation.qsf_session_id);
    relaySocket.addEventListener("message", (event) => {
      const raw = String(event.data);
      const status = parseSidebandStatusMessage(raw);
      if (status !== null) {
        dispatch({
          type: "server_status",
          sessionId: status.qsf_session_id,
          degraded: status.degraded,
          detail: status.detail,
        });
      }
      const turnCtx = parseTurnContextMessage(raw);
      if (turnCtx !== null) {
        dispatch({ type: "turn_context_captured", capture: turnCtx });
      }
      const volitionState = parseVolitionStateMessage(raw);
      if (volitionState !== null) {
        dispatch({ type: "volition_state_captured", capture: volitionState });
      }
      const worldPerception = parseWorldPerceptionMessage(raw);
      if (worldPerception !== null) {
        dispatch({ type: "world_perception_captured", capture: worldPerception });
      }
      const tokenUsage = parseTokenUsageMessage(raw);
      if (tokenUsage !== null) {
        dispatch({ type: "token_usage_captured", snapshot: tokenUsage });
      }
    });
    relaySocket.addEventListener("close", () => {
      if (activeConversation?.relaySocket === relaySocket) {
        dispatch({
          type: "connection_error",
          message: "relay socket closed",
          atMs: Date.now(),
        });
      }
    });

    peerConnection = new RTCPeerConnection();
    if (options.captureMicrophone) {
      microphoneStream = await navigator.mediaDevices.getUserMedia({
        audio: MICROPHONE_AUDIO_CONSTRAINTS,
      });
      for (const track of microphoneStream.getAudioTracks()) {
        // Honor a pre-armed mute so the call starts silent when the user toggled
        // Mute before pressing Start.
        track.enabled = !state.muted;
        const settings = track.getSettings();
        console.info("microphone capture settings", {
          echoCancellation: settings.echoCancellation,
          noiseSuppression: settings.noiseSuppression,
          autoGainControl: settings.autoGainControl,
        });
      }
      for (const track of microphoneStream.getTracks()) {
        peerConnection.addTrack(track, microphoneStream);
      }
    } else {
      peerConnection.addTransceiver("audio", { direction: "recvonly" });
    }

    dataChannel = peerConnection.createDataChannel("oai-events");
    dataChannel.addEventListener("message", (event) => {
      try {
        const message = parseProviderDataChannelMessage(String(event.data));
        const envelope = mapProviderMessageToRelayEnvelope(allocation.qsf_session_id, message);
        if (envelope === null) {
          return;
        }
        const serialized = JSON.stringify(envelope);
        if (relaySocket?.readyState === WebSocket.OPEN) {
          relaySocket.send(serialized);
        } else {
          relayBuffer.push(serialized);
        }
        dispatch({ type: "provider_envelope", envelope, atMs: Date.now() });
      } catch (error) {
        dispatch({
          type: "connection_error",
          message: messageFromError(error),
          atMs: Date.now(),
        });
      }
    });

    dataChannel.addEventListener("open", () => {
      if (!relaySocket) {
        return;
      }
      for (const payload of relayBuffer.splice(0)) {
        relaySocket.send(payload);
      }
    });

    peerConnection.addEventListener("track", (event) => {
      const stream = event.streams[0] ?? new MediaStream([event.track]);
      refs.remoteAudio.srcObject = stream;
      void refs.remoteAudio.play().catch(() => {
        // Browsers may require an explicit user gesture before audio starts.
      });
    });

    const offer = await peerConnection.createOffer();
    await peerConnection.setLocalDescription(offer);
    await waitForIceGatheringComplete(peerConnection);

    const localDescription = peerConnection.localDescription;
    if (!localDescription?.sdp) {
      throw new Error("missing SDP offer");
    }

    const answer = await postJson<SdpExchangeResponse>("/api/realtime/sdp", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        qsf_session_id: allocation.qsf_session_id,
        offer_sdp: localDescription.sdp,
      }),
    });

    await peerConnection.setRemoteDescription({
      type: "answer",
      sdp: answer.answer_sdp,
    });

    activeConversation = {
      sessionId: allocation.qsf_session_id,
      peerConnection,
      microphoneStream,
      relaySocket,
      dataChannel,
      relayBuffer,
    };
    dispatch({ type: "connection_ready" });
    return true;
  } catch (error) {
    await stopDetachedConversation(relaySocket, peerConnection, microphoneStream, dataChannel);
    dispatch({
      type: "connection_error",
      message: messageFromError(error),
      atMs: Date.now(),
    });
    return false;
  }
}

async function submitTextTurn() {
  const text = refs.textInput.value.trim();
  if (!text) {
    return;
  }

  textTurnPending = true;
  render();
  try {
    const started = await startConversation({ captureMicrophone: false });
    if (!started || !activeConversation) {
      return;
    }
    await waitForDataChannelOpen(activeConversation.dataChannel);
    await postJson<TextTurnResponse>("/api/realtime/text", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        qsf_session_id: activeConversation.sessionId,
        text,
      }),
    });
    dispatch({
      type: "provider_envelope",
      envelope: {
        qsf_session_id: activeConversation.sessionId,
        event_id: `typed-turn-${Date.now()}`,
        kind: "final_transcript",
        transcript: text,
      },
      atMs: Date.now(),
    });
    refs.textInput.value = "";
  } catch (error) {
    dispatch({
      type: "connection_error",
      message: messageFromError(error),
      atMs: Date.now(),
    });
  } finally {
    textTurnPending = false;
    render();
  }
}

async function stopConversation() {
  dispatch({ type: "stop_requested", atMs: Date.now() });

  if (activeConversation) {
    try {
      await postJson("/api/realtime/stop", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          qsf_session_id: activeConversation.sessionId,
        }),
      });
    } catch (error) {
      dispatch({
        type: "connection_error",
        message: messageFromError(error),
        atMs: Date.now(),
      });
    }
  }

  await stopActiveConversation(true);
  dispatch({ type: "stopped", atMs: Date.now() });
}

/// Gate the live microphone track(s) to match the `muted` state. Isolated side
/// effect kept out of the reducer: the mic stream is imperative WebRTC media, not
/// reducer state. A no-op when there is no active mic (idle, or a text-only turn).
function applyMicrophoneMute(muted: boolean) {
  const stream = activeConversation?.microphoneStream;
  if (!stream) {
    return;
  }
  for (const track of stream.getAudioTracks()) {
    track.enabled = !muted;
  }
}

async function stopActiveConversation(closeRelay: boolean) {
  const conversation = activeConversation;
  activeConversation = null;
  await stopDetachedConversation(
    conversation?.relaySocket ?? null,
    conversation?.peerConnection ?? null,
    conversation?.microphoneStream ?? null,
    conversation?.dataChannel ?? null,
    closeRelay,
  );
}

async function stopDetachedConversation(
  relaySocket: WebSocket | null,
  peerConnection: RTCPeerConnection | null,
  microphoneStream: MediaStream | null,
  dataChannel: RTCDataChannel | null,
  closeRelay = true,
) {
  dataChannel?.close();
  peerConnection?.close();
  microphoneStream?.getTracks().forEach((track) => {
    track.stop();
  });
  refs.remoteAudio.srcObject = null;

  if (closeRelay) {
    relaySocket?.close();
  }
}

function dispatch(action: ConversationAction) {
  state = reduceConversationState(state, action);
  render();
}

function render() {
  refs.connectionStatus.textContent = describeConnection(state);
  refs.runtimePhase.textContent = describeRuntimePhase(state.phase);
  refs.sessionId.textContent = state.sessionId ?? "—";
  refs.liveTranscript.textContent = state.liveTranscript || "Waiting for the next turn.";

  const tickerRows = selectEventTickerModel(state);
  if (tickerRows.length === 0) {
    const empty = document.createElement("li");
    empty.className = "event-ticker-empty";
    empty.textContent = "None yet";
    refs.eventTicker.replaceChildren(empty);
  } else {
    refs.eventTicker.replaceChildren(
      ...tickerRows.map((row) => {
        const item = document.createElement("li");
        const time = document.createElement("span");
        time.className = "event-ticker-time";
        time.textContent = row.timeLabel;
        const kind = document.createElement("span");
        kind.className = "event-ticker-kind";
        kind.textContent = row.countLabel === null ? row.kind : `${row.kind} ${row.countLabel}`;
        const delta = document.createElement("span");
        delta.className = "event-ticker-delta";
        delta.textContent = row.deltaLabel ?? "";
        item.append(time, kind, delta);
        return item;
      }),
    );
  }

  refs.errorBanner.hidden = state.error === null;
  refs.errorBanner.textContent = state.error ?? "";

  refs.warningBanner.hidden = state.warning === null;
  refs.warningBanner.textContent = state.warning ?? "";

  refs.startButton.disabled =
    Boolean(activeConversation) ||
    state.connection === "requesting_session" ||
    state.connection === "connecting_media" ||
    state.connection === "stopping";
  refs.stopButton.disabled = !activeConversation && state.connection !== "stopping";
  refs.sendTextButton.disabled = !canSubmitTextTurn();

  const muteButton = selectMuteButton(state);
  refs.muteButton.textContent = muteButton.label;
  refs.muteButton.setAttribute("aria-pressed", String(muteButton.pressed));

  refs.transcriptList.replaceChildren(
    ...state.transcript.map((entry) => {
      const item = document.createElement("li");
      item.className = `turn turn-${entry.role}`;
      const label = document.createElement("span");
      label.className = "turn-role";
      label.textContent =
        entry.role === "user" ? "User" : entry.role === "assistant" ? "Assistant" : "System";
      const text = document.createElement("p");
      text.textContent = entry.text;
      item.append(label, text);
      return item;
    }),
  );
  scrollTranscriptToLatest();

  const injectedVolition = selectInjectedVolitionText(state);
  renderWhyThisAnswerPanel(
    refs.volitionStateBody,
    selectVolitionVerdict(state, injectedVolition),
    injectedVolition,
    selectVolitionPanelModel(state),
  );
  renderWorldPerceptionPanel(refs.worldPerceptionBody, selectWorldPerceptionPanelModel(state));
  renderTokenUsagePanel(refs.tokenUsageBody, selectTokenUsagePanelModel(state));
}

function scrollTranscriptToLatest() {
  window.requestAnimationFrame(() => {
    refs.transcriptList.scrollTop = refs.transcriptList.scrollHeight;
  });
}

function canSubmitTextTurn(): boolean {
  return selectCanSubmitTextTurn(state, {
    hasText: Boolean(refs.textInput.value.trim()),
    pending: textTurnPending,
  });
}

async function connectRelaySocket(sessionId: string): Promise<WebSocket> {
  const socket = new WebSocket(`/api/realtime/events?session=${encodeURIComponent(sessionId)}`);
  await new Promise<void>((resolve, reject) => {
    socket.addEventListener("open", () => resolve(), { once: true });
    socket.addEventListener("error", () => reject(new Error("failed to open relay websocket")), {
      once: true,
    });
  });
  return socket;
}

async function postJson<T>(url: string, init: RequestInit): Promise<T> {
  const response = await fetch(url, init);
  if (!response.ok) {
    const body = await response.text().catch(() => "");
    throw new Error(`HTTP ${response.status} on ${url}${body ? `: ${body}` : ""}`);
  }
  return (await response.json()) as T;
}

async function waitForIceGatheringComplete(peerConnection: RTCPeerConnection) {
  if (peerConnection.iceGatheringState === "complete") {
    return;
  }
  await new Promise<void>((resolve) => {
    const listener = () => {
      if (peerConnection.iceGatheringState === "complete") {
        peerConnection.removeEventListener("icegatheringstatechange", listener);
        resolve();
      }
    };
    peerConnection.addEventListener("icegatheringstatechange", listener);
  });
}

async function waitForDataChannelOpen(dataChannel: RTCDataChannel) {
  if (dataChannel.readyState === "open") {
    return;
  }
  await new Promise<void>((resolve, reject) => {
    const timeout = window.setTimeout(() => {
      cleanup();
      reject(new Error("data channel did not open in time"));
    }, 8000);
    const cleanup = () => {
      window.clearTimeout(timeout);
      dataChannel.removeEventListener("open", handleOpen);
      dataChannel.removeEventListener("error", handleError);
      dataChannel.removeEventListener("close", handleClose);
    };
    const handleOpen = () => {
      cleanup();
      resolve();
    };
    const handleError = () => {
      cleanup();
      reject(new Error("data channel failed to open"));
    };
    const handleClose = () => {
      cleanup();
      reject(new Error("data channel closed before opening"));
    };
    dataChannel.addEventListener("open", handleOpen);
    dataChannel.addEventListener("error", handleError);
    dataChannel.addEventListener("close", handleClose);
  });
}

function collectRefs(container: HTMLElement): UiRefs {
  const query = <T extends HTMLElement>(selector: string): T => {
    const element = container.querySelector<T>(selector);
    if (!element) {
      throw new Error(`missing required element ${selector}`);
    }
    return element;
  };

  return {
    startButton: query<HTMLButtonElement>('[data-role="start"]'),
    stopButton: query<HTMLButtonElement>('[data-role="stop"]'),
    muteButton: query<HTMLButtonElement>('[data-role="mute"]'),
    textForm: query<HTMLFormElement>('[data-role="text-form"]'),
    textInput: query<HTMLTextAreaElement>('[data-role="text-input"]'),
    sendTextButton: query<HTMLButtonElement>('[data-role="send-text"]'),
    sessionId: query<HTMLElement>('[data-role="session"]'),
    connectionStatus: query<HTMLElement>('[data-role="connection"]'),
    runtimePhase: query<HTMLElement>('[data-role="phase"]'),
    liveTranscript: query<HTMLElement>('[data-role="live-transcript"]'),
    eventTicker: query<HTMLOListElement>('[data-role="event-ticker"]'),
    transcriptList: query<HTMLOListElement>('[data-role="transcript"]'),
    errorBanner: query<HTMLElement>('[data-role="error"]'),
    warningBanner: query<HTMLElement>('[data-role="warning"]'),
    remoteAudio: query<HTMLAudioElement>('[data-role="remote-audio"]'),
    volitionStateBody: query<HTMLElement>('[data-role="volition-state-body"]'),
    worldPerceptionBody: query<HTMLElement>('[data-role="world-perception-body"]'),
    tokenUsageBody: query<HTMLElement>('[data-role="token-usage-body"]'),
    phaseLaneCanvas: query<HTMLCanvasElement>('[data-role="phase-lane"]'),
    phaseLaneTip: query<HTMLElement>('[data-role="phase-lane-tip"]'),
  };
}

function renderWhyThisAnswerPanel(
  container: HTMLElement,
  verdict: ReturnType<typeof selectVolitionVerdict>,
  injected: ReturnType<typeof selectInjectedVolitionText>,
  model: ReturnType<typeof selectVolitionPanelModel>,
) {
  container.replaceChildren();

  // Tier 1 — verdict.
  const verdictBlock = document.createElement("div");
  verdictBlock.className = `why-verdict why-verdict-${verdict.kind}`;
  const line = document.createElement("p");
  line.className = "why-verdict-line";
  line.textContent = verdict.line;
  verdictBlock.appendChild(line);
  if (verdict.nudge !== null) {
    const nudge = document.createElement("span");
    nudge.className = "why-nudge";
    nudge.textContent = verdict.nudge;
    verdictBlock.appendChild(nudge);
  }
  if (verdict.caption !== null) {
    const caption = document.createElement("p");
    caption.className = "why-verdict-caption";
    caption.textContent = verdict.caption;
    verdictBlock.appendChild(caption);
  }
  container.appendChild(verdictBlock);

  // Tier 2 — what volition told the model. "Nothing was injected" is only claimed when the
  // exchange-matched turn context was actually inspected; a missing/mismatched capture (the
  // non-atomic watch-channel window) gets a neutral "not captured" line instead.
  const injectedSection = document.createElement("section");
  injectedSection.className = "why-injected";
  const injectedHeading = document.createElement("h3");
  injectedHeading.textContent = "What volition told the model";
  injectedSection.appendChild(injectedHeading);
  if (injected.status === "found") {
    const pre = document.createElement("pre");
    pre.className = "why-injected-text";
    pre.textContent = injected.text;
    injectedSection.appendChild(pre);
  } else {
    const placeholder = document.createElement("p");
    placeholder.className = "why-injected-empty";
    placeholder.textContent =
      injected.status === "none_injected"
        ? "Nothing was injected this turn."
        : "No matching injected packet captured for this evaluated turn.";
    injectedSection.appendChild(placeholder);
  }
  container.appendChild(injectedSection);

  // Tier 3 — scoring detail, expanded so functional-signal evidence is visible by default.
  const scoring = document.createElement("details");
  scoring.className = "why-scoring";
  scoring.open = true;
  const scoringSummary = document.createElement("summary");
  scoringSummary.textContent = "Scoring detail";
  scoring.appendChild(scoringSummary);
  const scoringBody = document.createElement("div");
  scoringBody.className = "scoring-compact-body";
  renderCompactScoringPanel(scoringBody, model);
  scoring.appendChild(scoringBody);
  container.appendChild(scoring);
}

function renderWorldPerceptionPanel(container: HTMLElement, model: WorldPerceptionPanelModel) {
  const retrievalDetailsOpen =
    container.querySelector<HTMLDetailsElement>(".world-retrieval-details")?.open ?? false;
  container.replaceChildren();

  const verdict = document.createElement("section");
  verdict.className = `world-perception-verdict world-perception-verdict-${model.kind}`;
  const verdictLine = document.createElement("p");
  verdictLine.textContent = model.verdict;
  verdict.appendChild(verdictLine);
  if (model.caption !== null) {
    const caption = document.createElement("span");
    caption.className = "world-perception-caption";
    caption.textContent = model.caption;
    verdict.appendChild(caption);
  }
  if (model.latency !== null) {
    const latency = document.createElement("span");
    latency.className = "world-perception-latency";
    latency.textContent = model.latency;
    verdict.appendChild(latency);
  }
  container.appendChild(verdict);

  const modelSection = document.createElement("section");
  modelSection.className = "world-model-visible";
  const modelHeading = document.createElement("h3");
  modelHeading.textContent = "What reached the model";
  modelSection.appendChild(modelHeading);
  if (model.modelVisibleInjection !== null) {
    const block = document.createElement("blockquote");
    block.className = "world-untrusted-block";
    block.textContent = model.modelVisibleInjection;
    modelSection.appendChild(block);
  } else {
    const placeholder = document.createElement("p");
    placeholder.className = "world-perception-empty";
    placeholder.textContent = model.injectionPlaceholder ?? "No model-visible external material.";
    modelSection.appendChild(placeholder);
  }
  container.appendChild(modelSection);

  if (model.facts.length > 0) {
    const sources = document.createElement("section");
    sources.className = "world-source-cards";
    const sourceHeading = document.createElement("h3");
    sourceHeading.textContent = "Surfaced sources";
    sources.appendChild(sourceHeading);
    for (const fact of model.facts) {
      const card = document.createElement("article");
      card.className = "world-source-card";
      const title = document.createElement("a");
      title.href = fact.url;
      title.target = "_blank";
      title.rel = "noreferrer";
      title.textContent = fact.title;
      const detail = document.createElement("p");
      detail.textContent = `${fact.sourceDomain} · ${fact.age}`;
      const pills = document.createElement("p");
      const untrusted = document.createElement("span");
      untrusted.className = "status-pill muted";
      untrusted.textContent = "Untrusted external";
      const trust = document.createElement("span");
      trust.className = "world-trust-tier";
      trust.textContent = fact.trustTier;
      pills.append(untrusted, trust);
      card.append(title, detail, pills);
      sources.appendChild(card);
    }
    container.appendChild(sources);
  }

  const details = document.createElement("details");
  details.className = "world-retrieval-details";
  details.open = retrievalDetailsOpen;
  const summary = document.createElement("summary");
  summary.textContent = "Retrieval detail";
  details.appendChild(summary);
  const detailBody = document.createElement("div");
  detailBody.className = "world-retrieval-body";
  const terms = document.createElement("p");
  terms.textContent =
    model.queryTerms.length === 0
      ? "No consultation terms this turn."
      : `Query terms: ${model.queryTerms.map((term) => `${term.term} (${term.source})`).join(", ")}`;
  detailBody.appendChild(terms);
  const anchors = document.createElement("p");
  anchors.textContent =
    model.requiredAnchors.length === 0
      ? "Required anchors: none"
      : `Required anchors: ${model.requiredAnchors.join(", ")}`;
  detailBody.appendChild(anchors);
  if (model.goalDerivedRequiredAnchors.length > 0) {
    const goalAnchors = document.createElement("p");
    goalAnchors.textContent = `Goal-derived required anchors: ${model.goalDerivedRequiredAnchors.join(", ")}`;
    detailBody.appendChild(goalAnchors);
  }
  if (model.topicTermMajorityThreshold !== null) {
    const topicThreshold = document.createElement("p");
    topicThreshold.textContent = `Topic-term threshold: ${model.topicTermMajorityThreshold}`;
    detailBody.appendChild(topicThreshold);
  }
  if (model.corpus !== null) {
    const corpus = document.createElement("p");
    corpus.textContent = `Corpus: ${model.corpus}`;
    detailBody.appendChild(corpus);
  }
  if (model.candidates.length > 0) {
    const candidates = document.createElement("ul");
    candidates.className = "world-candidate-list";
    for (const candidate of model.candidates) {
      const item = document.createElement("li");
      item.textContent = `${candidate.title} · ${candidate.source} · ${candidate.age} · score ${candidate.score} · ${candidate.eligibility} · matched ${candidate.matchedTerms}`;
      candidates.appendChild(item);
    }
    detailBody.appendChild(candidates);
  }
  details.appendChild(detailBody);
  container.appendChild(details);
}

function renderTokenUsagePanel(container: HTMLElement, model: TokenUsagePanelModel) {
  container.replaceChildren();

  if (model.kind === "empty") {
    const empty = document.createElement("p");
    empty.className = "token-usage-empty";
    empty.textContent = "No model calls yet.";
    container.appendChild(empty);
    return;
  }

  const hero = document.createElement("p");
  hero.className = "token-hero";
  const heroValue = document.createElement("strong");
  heroValue.textContent = model.heroLabel;
  const heroDetail = document.createElement("span");
  heroDetail.textContent = model.heroDetail;
  hero.append(heroValue, heroDetail);
  container.appendChild(hero);

  const legend = document.createElement("ul");
  legend.className = "token-legend";
  for (const entry of model.legend) {
    const item = document.createElement("li");
    const chip = document.createElement("i");
    chip.className = `token-seg-${entry.className}`;
    const text = document.createElement("span");
    text.textContent = entry.label;
    item.append(chip, text);
    legend.appendChild(item);
  }
  container.appendChild(legend);

  for (const row of model.rows) {
    const rowElement = document.createElement("div");
    rowElement.className = "token-model-row";
    const head = document.createElement("div");
    head.className = "token-model-head";
    const name = document.createElement("span");
    name.className = "token-model-name";
    name.textContent = row.name;
    const total = document.createElement("span");
    total.className = "token-model-total";
    total.textContent = row.totalLabel;
    head.append(name, total);

    const bar = document.createElement("div");
    bar.className = "token-bar";
    bar.style.width = `${row.barPercent}%`;
    for (const segment of row.segments) {
      const segmentElement = document.createElement("i");
      segmentElement.className = `token-seg-${segment.className}`;
      segmentElement.style.width = `${segment.widthPercent}%`;
      segmentElement.title = segment.exactLabel;
      bar.appendChild(segmentElement);
    }

    rowElement.append(head, bar);
    container.appendChild(rowElement);
  }
}

function renderCompactScoringPanel(container: HTMLElement, model: VolitionPanelModel) {
  container.replaceChildren();

  const banner = document.createElement("p");
  banner.className = `volition-state-banner volition-state-banner-${model.kind}`;
  banner.textContent = model.banner;
  container.appendChild(banner);

  const stateSection = findVolitionSection(model, "State snapshot");
  const decisionSection = findVolitionSection(model, "Decision detail");
  const signalsSection = findVolitionSection(model, "Functional signals");
  const stateRows = rowsByLabel(stateSection);
  const decisionRows = rowsByLabel(decisionSection);

  renderScoringSummary(container, stateRows, decisionRows);
  renderScoringCounters(container, stateRows);
  renderScoringGridSection(container, "Goal sets", [
    rowFromMap(stateRows, "Active goals"),
    rowFromMap(stateRows, "Accepted goals"),
    rowFromMap(decisionRows, "Selected goals"),
    rowFromMap(decisionRows, "Omitted/suppressed goals"),
    rowFromMap(stateRows, "Last initiative summaries"),
  ]);
  renderScoringGridSection(container, "Decision detail", [
    rowFromMap(decisionRows, "Winner"),
    rowFromMap(decisionRows, "Winner tiers"),
    rowFromMap(decisionRows, "Below threshold"),
    rowFromMap(decisionRows, "Mode bias outcomes"),
    rowFromMap(decisionRows, "Last initiative output"),
    rowFromMap(decisionRows, "Suppression reason"),
    rowFromMap(decisionRows, "Trace ref"),
  ]);
  renderBooleanStrip(container, decisionRows);
  if (signalsSection) {
    renderScoringGridSection(container, signalsSection.title, signalsSection.rows, true);
  }
}

function findVolitionSection(
  model: VolitionPanelModel,
  title: string,
): VolitionPanelSection | undefined {
  return model.sections.find((section) => section.title === title);
}

function rowsByLabel(section: VolitionPanelSection | undefined): Map<string, VolitionPanelRow> {
  return new Map((section?.rows ?? []).map((row) => [row.label, row]));
}

function rowFromMap(
  rows: Map<string, VolitionPanelRow>,
  label: string,
): VolitionPanelRow | undefined {
  return rows.get(label);
}

function renderScoringSummary(
  container: HTMLElement,
  stateRows: Map<string, VolitionPanelRow>,
  decisionRows: Map<string, VolitionPanelRow>,
) {
  const summary = document.createElement("div");
  summary.className = "scoring-summary-strip";
  for (const row of [
    rowFromMap(stateRows, "Mode"),
    rowFromMap(stateRows, "Tick"),
    rowFromMap(decisionRows, "Winner"),
    rowFromMap(decisionRows, "Shaping intensity"),
  ]) {
    if (row) {
      summary.appendChild(scoringChip(row.label, row.value));
    }
  }
  if (summary.childElementCount > 0) {
    container.appendChild(summary);
  }
}

function renderScoringCounters(container: HTMLElement, stateRows: Map<string, VolitionPanelRow>) {
  const counters = [
    rowFromMap(stateRows, "Pending candidates"),
    rowFromMap(stateRows, "Accepted candidates"),
    rowFromMap(stateRows, "Blocked goals"),
    rowFromMap(stateRows, "Cooldown goals"),
    rowFromMap(stateRows, "Retired goals"),
  ].filter((row): row is VolitionPanelRow => row !== undefined);
  if (counters.length === 0) {
    return;
  }

  const section = document.createElement("section");
  section.className = "scoring-compact-section";
  const heading = document.createElement("h3");
  heading.textContent = "Counters";
  section.appendChild(heading);

  const strip = document.createElement("div");
  strip.className = "scoring-summary-strip scoring-counter-strip";
  for (const row of counters) {
    strip.appendChild(scoringChip(row.label, row.value === "none" ? "0" : row.value));
  }
  section.appendChild(strip);
  container.appendChild(section);
}

function renderScoringGridSection(
  container: HTMLElement,
  title: string,
  rows: Array<VolitionPanelRow | undefined>,
  emphasize = false,
) {
  const presentRows = rows.filter((row): row is VolitionPanelRow => row !== undefined);
  if (presentRows.length === 0) {
    return;
  }

  const section = document.createElement("section");
  section.className = emphasize
    ? "scoring-compact-section scoring-signals-section"
    : "scoring-compact-section";
  const heading = document.createElement("h3");
  heading.textContent = title;
  section.appendChild(heading);

  const list = document.createElement("dl");
  list.className = "scoring-grid-list";
  for (const row of presentRows) {
    const rowElement = document.createElement("div");
    rowElement.className = "scoring-grid-row";

    const label = document.createElement("dt");
    label.textContent = row.label;

    const value = document.createElement("dd");
    value.textContent = row.value;
    if (row.value === "none") {
      value.className = "scoring-muted-value";
    }

    rowElement.append(label, value);
    list.appendChild(rowElement);
  }
  section.appendChild(list);
  container.appendChild(section);
}

function renderBooleanStrip(container: HTMLElement, decisionRows: Map<string, VolitionPanelRow>) {
  const flags = [
    rowFromMap(decisionRows, "Last initiative surfaced"),
    rowFromMap(decisionRows, "Rendered line"),
  ].filter((row): row is VolitionPanelRow => row !== undefined);
  if (flags.length === 0) {
    return;
  }

  const section = document.createElement("section");
  section.className = "scoring-compact-section";
  const heading = document.createElement("h3");
  heading.textContent = "Flags";
  section.appendChild(heading);

  const strip = document.createElement("div");
  strip.className = "scoring-flag-strip";
  for (const row of flags) {
    const flag = document.createElement("label");
    flag.className = "scoring-flag";
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.disabled = true;
    checkbox.checked = row.value === "yes";
    const text = document.createElement("span");
    text.textContent = row.label;
    flag.append(checkbox, text);
    strip.appendChild(flag);
  }
  section.appendChild(strip);
  container.appendChild(section);
}

function scoringChip(label: string, value: string): HTMLElement {
  const chip = document.createElement("div");
  chip.className = "scoring-chip";
  const labelElement = document.createElement("span");
  labelElement.className = "scoring-chip-label";
  labelElement.textContent = label;
  const valueElement = document.createElement("strong");
  valueElement.textContent = value;
  chip.append(labelElement, valueElement);
  return chip;
}

function messageFromError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
