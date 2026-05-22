import { mustQuery } from "./html";

export function renderShell(root: HTMLElement) {
  root.className = "workbench";
  root.innerHTML = `
    <div class="top" id="top">
      <div class="toolbar" id="toolbar"></div>
    </div>
    <div class="main">
      <div class="list" id="list"></div>
      <div class="right">
        <div class="canvas-slot" id="canvas-slot">Select a memory to see its neighborhood.</div>
        <div class="inspector" id="inspector">Select a memory to inspect.</div>
      </div>
    </div>
    <div class="statusbar" id="statusbar"></div>
  `;
}

export function getSlots(root: HTMLElement) {
  return {
    top: mustQuery(root, "#top"),
    toolbar: mustQuery(root, "#toolbar"),
    list: mustQuery(root, "#list"),
    canvasSlot: mustQuery(root, "#canvas-slot"),
    inspector: mustQuery(root, "#inspector"),
    statusbar: mustQuery(root, "#statusbar"),
  };
}

export type Slots = ReturnType<typeof getSlots>;
