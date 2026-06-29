import "./styles.css";
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
  reduceConversationState,
  type SdpExchangeResponse,
  type SessionAllocationResponse,
} from "./realtime";

interface UiRefs {
  startButton: HTMLButtonElement;
  stopButton: HTMLButtonElement;
  textForm: HTMLFormElement;
  textInput: HTMLTextAreaElement;
  sendTextButton: HTMLButtonElement;
  sessionId: HTMLElement;
  connectionStatus: HTMLElement;
  runtimePhase: HTMLElement;
  liveTranscript: HTMLElement;
  lastEvent: HTMLElement;
  transcriptList: HTMLOListElement;
  errorBanner: HTMLElement;
  warningBanner: HTMLElement;
  remoteAudio: HTMLAudioElement;
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
    <section class="hero">
      <p class="eyebrow">QSF realtime voice</p>
      <h1>Browser voice, server rendezvous.</h1>
      <p class="lede">
        The browser owns media, the server owns signaling and diagnostics, and the relay channel stays untrusted.
      </p>
      <div class="hero-metrics">
        <div class="metric">
          <span>Connection</span>
          <strong data-role="connection">Idle</strong>
        </div>
        <div class="metric">
          <span>Runtime phase</span>
          <strong data-role="phase">Idle</strong>
        </div>
        <div class="metric">
          <span>Session</span>
          <strong data-role="session">—</strong>
        </div>
      </div>
    </section>

    <section class="controls">
      <button data-role="start" type="button">Start conversation</button>
      <button data-role="stop" type="button" disabled>Stop</button>
      <form data-role="text-form" class="text-turn-form">
        <textarea data-role="text-input" rows="2" placeholder="Type a turn for noisy rooms"></textarea>
        <button data-role="send-text" type="submit">Send text</button>
      </form>
      <p data-role="error" class="error" hidden></p>
      <p data-role="warning" class="warning" role="status" hidden></p>
    </section>

    <section class="grid">
      <article class="panel transcript-panel">
        <div class="panel-header">
          <h2>Transcript</h2>
          <span class="status-pill">Live</span>
        </div>
        <p data-role="live-transcript" class="live-transcript" aria-live="polite">Waiting for the first turn.</p>
        <ol data-role="transcript" class="transcript" aria-label="Conversation transcript"></ol>
      </article>

      <aside class="panel details-panel">
        <div class="panel-header">
          <h2>Diagnostics</h2>
          <span class="status-pill muted">Browser view</span>
        </div>
        <dl class="details">
          <div>
            <dt>Last event</dt>
            <dd data-role="last-event">None yet</dd>
          </div>
          <div>
            <dt>Media</dt>
            <dd>Direct browser to OpenAI</dd>
          </div>
          <div>
            <dt>Relay</dt>
            <dd>Typed browser-to-server envelopes</dd>
          </div>
        </dl>
        <audio data-role="remote-audio" autoplay playsinline></audio>
      </aside>
    </section>
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
refs.textForm.addEventListener("submit", (event) => {
  event.preventDefault();
  void submitTextTurn();
});
refs.textInput.addEventListener("input", () => {
  render();
});

render();

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
      const status = parseSidebandStatusMessage(String(event.data));
      if (status !== null) {
        dispatch({
          type: "server_status",
          sessionId: status.qsf_session_id,
          degraded: status.degraded,
          detail: status.detail,
        });
      }
    });
    relaySocket.addEventListener("close", () => {
      if (activeConversation?.relaySocket === relaySocket) {
        dispatch({
          type: "connection_error",
          message: "relay socket closed",
        });
      }
    });

    peerConnection = new RTCPeerConnection();
    if (options.captureMicrophone) {
      microphoneStream = await navigator.mediaDevices.getUserMedia({
        audio: MICROPHONE_AUDIO_CONSTRAINTS,
      });
      for (const track of microphoneStream.getAudioTracks()) {
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
        dispatch({ type: "provider_envelope", envelope });
      } catch (error) {
        dispatch({
          type: "connection_error",
          message: messageFromError(error),
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
    });
    refs.textInput.value = "";
  } catch (error) {
    dispatch({
      type: "connection_error",
      message: messageFromError(error),
    });
  } finally {
    textTurnPending = false;
    render();
  }
}

async function stopConversation() {
  dispatch({ type: "stop_requested" });

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
      });
    }
  }

  await stopActiveConversation(true);
  dispatch({ type: "stopped" });
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
  refs.lastEvent.textContent = state.lastEvent ?? "None yet";

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
}

function scrollTranscriptToLatest() {
  window.requestAnimationFrame(() => {
    refs.transcriptList.scrollTop = refs.transcriptList.scrollHeight;
  });
}

function canSubmitTextTurn(): boolean {
  if (textTurnPending) {
    return false;
  }
  if (!refs.textInput.value.trim()) {
    return false;
  }
  if (
    state.connection === "requesting_session" ||
    state.connection === "connecting_media" ||
    state.connection === "stopping" ||
    state.connection === "error"
  ) {
    return false;
  }
  return state.connection === "idle" || (state.connection === "ready" && state.phase === "idle");
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
    textForm: query<HTMLFormElement>('[data-role="text-form"]'),
    textInput: query<HTMLTextAreaElement>('[data-role="text-input"]'),
    sendTextButton: query<HTMLButtonElement>('[data-role="send-text"]'),
    sessionId: query<HTMLElement>('[data-role="session"]'),
    connectionStatus: query<HTMLElement>('[data-role="connection"]'),
    runtimePhase: query<HTMLElement>('[data-role="phase"]'),
    liveTranscript: query<HTMLElement>('[data-role="live-transcript"]'),
    lastEvent: query<HTMLElement>('[data-role="last-event"]'),
    transcriptList: query<HTMLOListElement>('[data-role="transcript"]'),
    errorBanner: query<HTMLElement>('[data-role="error"]'),
    warningBanner: query<HTMLElement>('[data-role="warning"]'),
    remoteAudio: query<HTMLAudioElement>('[data-role="remote-audio"]'),
  };
}

function messageFromError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
