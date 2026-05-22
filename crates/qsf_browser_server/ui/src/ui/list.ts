import type { Action } from "../state";
import type {
  MemoryListItem,
  MemoryPage,
  SessionSearchResponse,
} from "../types";
import { escapeHtml } from "./html";

export function renderList(
  el: HTMLElement,
  page: MemoryPage,
  selectedId: string | null,
  dispatch: (a: Action) => void,
  sessionSearch?: SessionSearchResponse | null,
) {
  const memoryHtml =
    page.items.map((m) => rowHtml(m, m.id === selectedId)).join("") ||
    `<div style="padding:12px;color:var(--qsf-text-muted)">No accepted memories match the current filters.</div>`;
  el.innerHTML = memoryHtml + sessionSearchHtml(sessionSearch);
  el.querySelectorAll<HTMLElement>(".row").forEach((row) => {
    row.addEventListener("click", () => {
      const id = row.dataset.id;
      if (!id) throw new Error("row missing data-id");
      dispatch({ type: "select", id });
    });
  });
}

function rowHtml(m: MemoryListItem, selected: boolean): string {
  const dateLabel =
    m.last_reinforced_at === null
      ? `created ${m.created_at.slice(0, 10)}`
      : `reinforced ${m.last_reinforced_at.slice(0, 10)}`;
  return `
    <div class="row ${selected ? "selected" : ""}" data-id="${escapeHtml(m.id)}">
      <div class="row-title">${escapeHtml(m.title)}</div>
      <div class="row-meta">${escapeHtml(m.kind)} · ${m.association_count} assoc · ${escapeHtml(dateLabel)}</div>
    </div>
  `;
}

function sessionSearchHtml(
  sessionSearch?: SessionSearchResponse | null,
): string {
  if (!sessionSearch) return "";
  if (!sessionSearch.available) {
    return `
      <div class="session-matches">
        <div class="session-heading">Session context</div>
        <div class="session-empty">${escapeHtml(sessionSearch.message ?? "No session-state.json found next to this store.")}</div>
      </div>
    `;
  }
  if (sessionSearch.total === 0) {
    return `
      <div class="session-matches">
        <div class="session-heading">Session context</div>
        <div class="session-empty">No session context matches.</div>
      </div>
    `;
  }

  const count =
    sessionSearch.total === sessionSearch.items.length
      ? `${sessionSearch.total}`
      : `${sessionSearch.items.length} of ${sessionSearch.total}`;
  return `
    <div class="session-matches">
      <div class="session-heading">Session context matches · ${count}</div>
      ${sessionSearch.items
        .map(
          (item) => `
            <div class="session-row">
              <div class="row-title">${escapeHtml(item.title)}</div>
              <div class="row-meta">${escapeHtml(item.kind)} · turn ${item.turn_index}</div>
              <div class="session-excerpt">${escapeHtml(item.excerpt)}</div>
            </div>
          `,
        )
        .join("")}
    </div>
  `;
}
