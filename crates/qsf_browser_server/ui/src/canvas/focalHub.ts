import { Application, Container, Graphics, Text } from "pixi.js";
import type { Neighborhood } from "../types";
import { radialPositions } from "./radial";
import { computeNeighborIds, edgeWidth, maxEdgeWeight } from "./scene";

const COLOR_MEMORY = 0xffd76a;
const COLOR_EDGE = 0xffb94a;
const COLOR_BROKEN = 0xff5d73;
const COLOR_LABEL = 0xeaf6ff;
const COLOR_TOOLTIP_BORDER = 0xacd7ff;
const COLOR_BACKGROUND = 0x07162a;
const BACKGROUND = "#07162a";

interface Point {
  x: number;
  y: number;
}

export class FocalHubScene {
  private app: Application;
  private layer = new Container();
  private hoverLayer = new Container();
  private onSelect: (id: string) => void;
  private ready = false;
  private disposed = false;
  private resizeObserver: ResizeObserver | null = null;
  private pendingRender: {
    centerId: string;
    neighborhood: Neighborhood;
  } | null = null;
  private pendingMessage: string | null = null;
  private lastRender: {
    centerId: string;
    neighborhood: Neighborhood;
  } | null = null;
  private lastMessage: string | null = null;

  constructor(slot: HTMLElement, onSelect: (id: string) => void) {
    this.app = new Application();
    this.onSelect = onSelect;
    void this.init(slot);
  }

  destroy() {
    this.disposed = true;
    this.pendingRender = null;
    this.pendingMessage = null;
    this.resizeObserver?.disconnect();
    this.resizeObserver = null;
    if (this.ready) {
      this.app.destroy(
        { removeView: true },
        {
          children: true,
          texture: true,
          textureSource: true,
          context: true,
        },
      );
      this.ready = false;
    }
  }

  renderMessage(message: string) {
    if (this.disposed) return;
    if (!this.ready) {
      this.pendingRender = null;
      this.pendingMessage = message;
      return;
    }

    this.lastRender = null;
    this.lastMessage = message;
    this.clearLayer(this.layer);
    this.clearLayer(this.hoverLayer);

    const { width, height } = this.app.screen;
    const label = new Text({
      text: message,
      style: {
        fill: COLOR_LABEL,
        fontFamily: "Inter, Segoe UI, system-ui, sans-serif",
        fontSize: 13,
      },
    });
    label.anchor.set(0.5);
    label.position.set(width / 2, height / 2);
    this.layer.addChild(label);
  }

  render(centerId: string, neighborhood: Neighborhood) {
    if (this.disposed) return;
    if (!this.ready) {
      this.pendingRender = { centerId, neighborhood };
      return;
    }

    this.lastRender = { centerId, neighborhood };
    this.lastMessage = null;
    this.clearLayer(this.layer);
    this.clearLayer(this.hoverLayer);

    const { width, height } = this.app.screen;
    const center = { x: width / 2, y: height / 2 };
    const radius = Math.max(56, Math.min(width, height) * 0.35);
    const memberById = new Map(neighborhood.members.map((m) => [m.id, m]));
    const neighborIds = computeNeighborIds(centerId, neighborhood.edges);
    const idToPos = this.positionNeighbors(neighborIds, center, radius);
    const maxWeight = maxEdgeWeight(neighborhood.edges);

    for (const edge of neighborhood.edges) {
      const otherId = edge.from_id === centerId ? edge.to_id : edge.from_id;
      const pos = idToPos.get(otherId);
      if (!pos) continue;
      const broken = !memberById.has(otherId);
      this.drawEdge(center, pos, edge.weight, maxWeight, broken);
    }

    for (const id of neighborIds) {
      const pos = idToPos.get(id);
      if (!pos) continue;
      const member = memberById.get(id);
      this.drawNeighbor(id, pos, member?.title ?? null, !member);
    }

    this.drawCenter(center, neighborhood.center.title);
  }

  private async init(slot: HTMLElement) {
    try {
      await this.app.init({
        background: BACKGROUND,
        resizeTo: slot,
        antialias: true,
        resolution: window.devicePixelRatio || 1,
        autoDensity: true,
      });
      if (this.disposed) {
        this.app.destroy(
          { removeView: true },
          {
            children: true,
            texture: true,
            textureSource: true,
            context: true,
          },
        );
        return;
      }
      slot.innerHTML = "";
      slot.appendChild(this.app.canvas);
      this.app.stage.addChild(this.layer);
      this.app.stage.addChild(this.hoverLayer);
      this.resizeObserver = new ResizeObserver(() => {
        window.requestAnimationFrame(() => this.rerenderLast());
      });
      this.resizeObserver.observe(slot);
      this.ready = true;
      if (this.pendingRender) {
        const pending = this.pendingRender;
        this.pendingRender = null;
        this.render(pending.centerId, pending.neighborhood);
      } else if (this.pendingMessage) {
        const message = this.pendingMessage;
        this.pendingMessage = null;
        this.renderMessage(message);
      }
    } catch (err: unknown) {
      try {
        this.app.destroy({ removeView: true }, { children: true });
      } catch {
        // Best-effort cleanup for partially initialized Pixi applications.
      }
      if (this.disposed) return;
      slot.textContent = `canvas init failed: ${
        err instanceof Error ? err.message : String(err)
      }`;
    }
  }

  private clearLayer(layer: Container) {
    for (const child of layer.removeChildren()) {
      child.destroy({ children: true });
    }
  }

  private rerenderLast() {
    if (this.disposed || !this.ready) return;
    if (this.lastRender) {
      this.render(this.lastRender.centerId, this.lastRender.neighborhood);
    } else if (this.lastMessage) {
      this.renderMessage(this.lastMessage);
    }
  }

  private positionNeighbors(
    neighborIds: string[],
    center: Point,
    radius: number,
  ): Map<string, Point> {
    const positions = radialPositions(neighborIds.length, radius);
    const idToPos = new Map<string, Point>();
    neighborIds.forEach((id, i) => {
      idToPos.set(id, {
        x: center.x + positions[i].x,
        y: center.y + positions[i].y,
      });
    });
    return idToPos;
  }

  private drawEdge(
    from: Point,
    to: Point,
    weight: number,
    maxWeight: number,
    broken: boolean,
  ) {
    const lineWidth = edgeWidth(weight, maxWeight);
    const color = broken ? COLOR_BROKEN : COLOR_EDGE;
    const edge = new Graphics();
    if (broken) {
      drawDashed(edge, from, to, lineWidth, color);
    } else {
      edge
        .moveTo(from.x, from.y)
        .lineTo(to.x, to.y)
        .stroke({ width: lineWidth, color, alpha: 0.7 });
    }
    this.layer.addChild(edge);
  }

  private drawNeighbor(
    id: string,
    pos: Point,
    title: string | null,
    broken: boolean,
  ) {
    const node = new Graphics();
    node.circle(pos.x, pos.y, broken ? 7 : 10).fill({
      color: broken ? COLOR_BROKEN : COLOR_MEMORY,
      alpha: broken ? 0.5 : 0.85,
    });
    node.eventMode = "static";
    node.cursor = broken ? "default" : "pointer";
    if (!broken) {
      node.on("pointertap", () => this.onSelect(id));
    }
    node.on("pointerover", () =>
      this.showTooltip(pos, title ?? truncateId(id), broken),
    );
    node.on("pointerout", () => this.clearLayer(this.hoverLayer));

    const label = new Text({
      text: title ?? truncateId(id),
      style: {
        fill: broken ? COLOR_BROKEN : COLOR_LABEL,
        fontFamily: "Inter, Segoe UI, system-ui, sans-serif",
        fontSize: 11,
      },
    });
    label.anchor.set(0.5, 0);
    label.position.set(pos.x, pos.y + 14);

    this.layer.addChild(node);
    this.layer.addChild(label);
  }

  private drawCenter(pos: Point, title: string) {
    const center = new Graphics();
    center.circle(pos.x, pos.y, 18).fill({ color: COLOR_MEMORY, alpha: 0.95 });
    this.layer.addChild(center);

    const label = new Text({
      text: title,
      style: {
        fill: COLOR_LABEL,
        fontFamily: "Inter, Segoe UI, system-ui, sans-serif",
        fontSize: 13,
      },
    });
    label.anchor.set(0.5, 0);
    label.position.set(pos.x, pos.y + 22);
    this.layer.addChild(label);
  }

  private showTooltip(pos: Point, text: string, broken: boolean) {
    this.clearLayer(this.hoverLayer);
    const tip = new Text({
      text: `${broken ? "broken -> " : ""}${text}`,
      style: {
        fill: COLOR_LABEL,
        fontFamily: "Inter, Segoe UI, system-ui, sans-serif",
        fontSize: 11,
      },
    });
    tip.position.set(pos.x + 12, pos.y - 18);

    const bg = new Graphics();
    bg.roundRect(pos.x + 8, pos.y - 22, tip.width + 12, tip.height + 6, 3)
      .fill({ color: COLOR_BACKGROUND, alpha: 0.9 })
      .stroke({ color: COLOR_TOOLTIP_BORDER, width: 1, alpha: 0.4 });
    this.hoverLayer.addChild(bg);
    this.hoverLayer.addChild(tip);
  }
}

function drawDashed(
  g: Graphics,
  from: Point,
  to: Point,
  width: number,
  color: number,
) {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const dist = Math.sqrt(dx * dx + dy * dy);
  if (dist === 0) return;

  const dashLen = 6;
  const gapLen = 4;
  const segLen = dashLen + gapLen;
  const ux = dx / dist;
  const uy = dy / dist;
  let drawn = 0;
  while (drawn < dist) {
    const end = Math.min(drawn + dashLen, dist);
    g.moveTo(from.x + ux * drawn, from.y + uy * drawn);
    g.lineTo(from.x + ux * end, from.y + uy * end);
    drawn += segLen;
  }
  g.stroke({ width, color, alpha: 0.7 });
}

function truncateId(id: string): string {
  return id.length > 10 ? `${id.slice(0, 10)}...` : id;
}
