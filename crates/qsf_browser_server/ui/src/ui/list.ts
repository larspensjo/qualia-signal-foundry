import type { Action } from "../state";
import type { MemoryListItem, MemoryPage } from "../types";
import { escapeHtml } from "./html";

export function renderList(
  el: HTMLElement,
  page: MemoryPage,
  selectedId: string | null,
  dispatch: (a: Action) => void,
) {
  el.innerHTML =
    page.items.map((m) => rowHtml(m, m.id === selectedId)).join("") ||
    `<div style="padding:12px;color:var(--qsf-text-muted)">No memories match the current filters.</div>`;
  el.querySelectorAll<HTMLElement>(".row").forEach((row) => {
    row.addEventListener("click", () => {
      const id = row.dataset.id;
      if (!id) throw new Error("row missing data-id");
      dispatch({ type: "select", id });
    });
  });
}

function rowHtml(m: MemoryListItem, selected: boolean): string {
  return `
    <div class="row ${selected ? "selected" : ""}" data-id="${escapeHtml(m.id)}">
      <div class="row-title">${escapeHtml(m.title)}</div>
      <div class="row-meta">${escapeHtml(m.kind)} · ${m.association_count} assoc · ${m.created_at.slice(0, 10)}</div>
    </div>
  `;
}
