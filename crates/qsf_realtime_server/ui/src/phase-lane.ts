import {
  type ConversationState,
  type PhaseLaneModel,
  type PhaseLaneTickModel,
  type RuntimePhase,
  selectPhaseLaneModel,
} from "./realtime";

export const PHASE_LANE_REDRAW_INTERVAL_MS = 100;

const BAND_TOP = 14;
const BAND_HEIGHT = 52;
const TICK_TOP = BAND_TOP + BAND_HEIGHT + 8;
const TICK_HEIGHT = 12;
/// Hover match radius for event ticks, in CSS pixels.
const HOVER_RADIUS_PX = 6;

/// Attach the swimlane renderer to its canvas: a 100 ms interval re-derives the
/// lane model (time advances even without actions) and repaints. The module is
/// a dumb consumer of selectPhaseLaneModel — geometry and formatting live there.
export function attachPhaseLane(
  canvas: HTMLCanvasElement,
  tip: HTMLElement,
  getState: () => ConversationState,
): void {
  const context = canvas.getContext("2d");
  if (context === null) {
    return;
  }
  const colors = readPhaseColors(canvas);
  let pointerX: number | null = null;
  let model: PhaseLaneModel = { segments: [], ticks: [], gridlines: [], breaks: [] };

  canvas.addEventListener("mousemove", (event) => {
    pointerX = event.offsetX;
    draw();
  });
  canvas.addEventListener("mouseleave", () => {
    pointerX = null;
    tip.hidden = true;
    draw();
  });

  function draw() {
    const width = canvas.clientWidth;
    const height = canvas.clientHeight;
    if (width === 0 || height === 0 || context === null) {
      return;
    }
    const dpr = window.devicePixelRatio || 1;
    if (canvas.width !== Math.round(width * dpr) || canvas.height !== Math.round(height * dpr)) {
      canvas.width = Math.round(width * dpr);
      canvas.height = Math.round(height * dpr);
    }
    context.setTransform(dpr, 0, 0, dpr, 0, 0);
    model = selectPhaseLaneModel(getState(), Date.now());

    context.clearRect(0, 0, width, height);
    context.fillStyle = "rgba(7, 10, 20, 0.55)";
    context.fillRect(0, 0, width, height);

    context.font = '10px Consolas, "Cascadia Mono", ui-monospace, monospace';
    context.textAlign = "center";
    for (const gridline of model.gridlines) {
      const x = gridline.fraction * width;
      context.strokeStyle = "rgba(255, 255, 255, 0.07)";
      context.beginPath();
      context.moveTo(x, BAND_TOP - 6);
      context.lineTo(x, TICK_TOP + TICK_HEIGHT);
      context.stroke();
      context.fillStyle = "rgba(184, 191, 215, 0.65)";
      context.fillText(gridline.label, Math.min(Math.max(x, 16), width - 16), height - 4);
    }

    for (const segment of model.segments) {
      const x1 = segment.startFraction * width;
      const x2 = segment.endFraction * width;
      context.globalAlpha = segment.phase === "idle" ? 0.28 : 0.75;
      context.fillStyle = colors[segment.phase];
      context.fillRect(x1, BAND_TOP, Math.max(1, x2 - x1), BAND_HEIGHT);
      context.globalAlpha = 1;
    }

    for (const tick of model.ticks) {
      const x = tick.fraction * width;
      context.strokeStyle = colors[tick.phase];
      context.lineWidth = 2;
      context.beginPath();
      context.moveTo(x, TICK_TOP);
      context.lineTo(x, TICK_TOP + TICK_HEIGHT);
      context.stroke();
      context.lineWidth = 1;
    }

    updateTip(width);
  }

  function updateTip(width: number) {
    if (pointerX === null) {
      tip.hidden = true;
      return;
    }
    let nearest: PhaseLaneTickModel | null = null;
    let nearestDistance = HOVER_RADIUS_PX;
    for (const tick of model.ticks) {
      const distance = Math.abs(tick.fraction * width - pointerX);
      if (distance < nearestDistance) {
        nearest = tick;
        nearestDistance = distance;
      }
    }
    if (nearest === null) {
      tip.hidden = true;
      return;
    }
    tip.hidden = false;
    tip.style.left = `${nearest.fraction * width}px`;
    tip.style.top = `${TICK_TOP}px`;
    tip.textContent = `${nearest.kind} · ${nearest.timeLabel}`;
  }

  window.setInterval(draw, PHASE_LANE_REDRAW_INTERVAL_MS);
  draw();
}

function readPhaseColors(element: Element): Record<RuntimePhase, string> {
  const style = getComputedStyle(element);
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    idle: read("--phase-idle", "#64748b"),
    listening: read("--phase-listening", "#7dd3fc"),
    thinking: read("--phase-thinking", "#f59e0b"),
    speaking: read("--phase-speaking", "#4ade80"),
  };
}
