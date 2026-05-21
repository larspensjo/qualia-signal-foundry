export function renderShell(root: HTMLElement) {
  root.className = "workbench";
  root.innerHTML = `
    <div class="top" id="top">
      <div class="toolbar" id="toolbar"></div>
    </div>
    <div class="main">
      <div class="list" id="list"></div>
      <div class="right">
        <div class="canvas-slot" id="canvas-slot">Canvas placeholder - focal hub lands in Phase 4</div>
        <div class="inspector" id="inspector">Select a memory to inspect.</div>
      </div>
    </div>
    <div class="statusbar" id="statusbar"></div>
  `;
}

export function getSlots(root: HTMLElement) {
  return {
    top: root.querySelector<HTMLElement>("#top")!,
    toolbar: root.querySelector<HTMLElement>("#toolbar")!,
    list: root.querySelector<HTMLElement>("#list")!,
    canvasSlot: root.querySelector<HTMLElement>("#canvas-slot")!,
    inspector: root.querySelector<HTMLElement>("#inspector")!,
    statusbar: root.querySelector<HTMLElement>("#statusbar")!,
  };
}

export type Slots = ReturnType<typeof getSlots>;
