import { api } from "../api";
import type { Action } from "../state";
import type { AssociationDisplay, MemoryDetail } from "../types";
import { escapeHtml } from "./html";

export async function renderInspector(
  el: HTMLElement,
  id: string,
  dispatch: (a: Action) => void,
  isCurrent: () => boolean = () => true,
) {
  el.innerHTML = `<div style="color:var(--qsf-text-muted)">loading...</div>`;
  let detail: MemoryDetail;
  try {
    detail = await api.getMemory(id);
  } catch {
    if (!isCurrent()) return;
    el.innerHTML = `<div style="color:var(--qsf-signal-error)">failed to load ${escapeHtml(id)}</div>`;
    return;
  }
  if (!isCurrent()) return;

  el.innerHTML = `
    <div style="display:flex;justify-content:space-between;align-items:flex-start;gap:12px">
      <h2 style="margin:0 0 4px 0;color:var(--qsf-signal-memory)">${escapeHtml(detail.title)}</h2>
      <button id="view-raw" style="background:transparent;color:var(--qsf-signal-context);border:1px solid var(--qsf-border-subtle);padding:4px 8px;cursor:pointer">view raw JSON</button>
    </div>
    <div style="color:var(--qsf-text-muted);font-size:12px;margin-bottom:12px">
      ${escapeHtml(detail.kind)} · created ${detail.created_at.slice(0, 10)} · last reinforced ${detail.last_reinforced_at?.slice(0, 10) ?? "none"} · x${detail.reinforcement_count} · imp ${detail.importance.toFixed(2)}
    </div>
    <h3 style="margin:8px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Summary</h3>
    <div style="white-space:pre-wrap">${escapeHtml(detail.summary)}</div>
    <h3 style="margin:12px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Tags</h3>
    <div>${detail.tags.map((t) => `<span style="display:inline-block;padding:1px 6px;margin-right:4px;border:1px solid var(--qsf-border-subtle);border-radius:3px">${escapeHtml(t)}</span>`).join("")}</div>
    <h3 style="margin:12px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Source</h3>
    <div style="color:var(--qsf-text-secondary);font-size:12px">${escapeHtml(detail.source_reference)}</div>
    <h3 style="margin:12px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Associations · outgoing (${detail.outgoing_count})</h3>
    ${detail.outgoing.map(assocRow).join("") || `<div style="color:var(--qsf-text-muted)">none</div>`}
    <h3 style="margin:12px 0 4px 0;color:var(--qsf-text-secondary);text-transform:uppercase;font-size:11px">Associations · incoming (${detail.incoming_count})</h3>
    ${detail.incoming.map(assocRow).join("") || `<div style="color:var(--qsf-text-muted)">none</div>`}
  `;

  el.querySelectorAll<HTMLElement>(".assoc").forEach((row) => {
    row.addEventListener("click", () => {
      const otherId = row.dataset.otherId;
      if (otherId && !row.classList.contains("broken")) {
        dispatch({ type: "select", id: otherId });
      }
    });
  });
  el.querySelector<HTMLButtonElement>("#view-raw")!.addEventListener("click", () =>
    openRawOverlay(id),
  );
}

function assocRow(a: AssociationDisplay): string {
  const broken = a.other_title === null;
  return `
    <div class="assoc ${broken ? "broken" : ""}" data-other-id="${escapeHtml(a.other_id)}">
      <div>${broken ? `<span class="broken">broken -> ${escapeHtml(a.other_id)}</span>` : escapeHtml(a.other_title!)}</div>
      <div class="weight">${a.weight.toFixed(2)}</div>
      <div style="color:var(--qsf-text-muted);font-size:12px">${a.last_reinforced_at.slice(0, 10)}</div>
    </div>
  `;
}

async function openRawOverlay(id: string) {
  const overlay = document.createElement("div");
  overlay.style.cssText =
    "position:fixed;inset:0;background:rgba(5,8,18,0.85);display:flex;align-items:center;justify-content:center;z-index:1000";
  overlay.innerHTML = `<pre style="background:var(--qsf-bg-panel-elevated);padding:24px;max-width:90vw;max-height:90vh;overflow:auto;color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);border-radius:8px">loading...</pre>`;
  const pre = overlay.querySelector("pre")!;
  const close = () => {
    document.removeEventListener("keydown", onKeydown);
    overlay.remove();
  };
  overlay.addEventListener("click", (event) => {
    if (event.target === overlay) close();
  });
  pre.addEventListener("click", (event) => event.stopPropagation());
  const onKeydown = (event: KeyboardEvent) => {
    if (event.key === "Escape") close();
  };
  document.addEventListener("keydown", onKeydown);
  document.body.appendChild(overlay);
  try {
    const raw = await api.getMemoryRaw(id);
    pre.textContent = JSON.stringify(raw, null, 2);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    pre.innerHTML = `<span style="color:var(--qsf-signal-error)">failed to load raw JSON: ${escapeHtml(message)}</span>`;
  }
}
