import type { Action, ViewState } from "../state";
import { escapeHtml, mustQuery } from "./html";

const filterInputStyle =
  "background:transparent;color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);padding:2px 6px";

export function renderFilters(
  parent: HTMLElement,
  state: ViewState,
  dispatch: (a: Action) => void,
) {
  if (!state.filtersExpanded) {
    parent.querySelector("#filters")?.remove();
    return;
  }

  let row = parent.querySelector<HTMLElement>("#filters");
  if (!row) {
    row = document.createElement("div");
    row.id = "filters";
    row.style.cssText =
      "display:flex;flex-wrap:wrap;gap:8px;padding:6px 12px;background:rgba(7,18,32,0.4);border-top:1px solid var(--qsf-border-subtle);font-size:12px;color:var(--qsf-text-secondary)";
    parent.appendChild(row);
  }

  row.innerHTML = `
    <label>kind <input id="f-kind" value="${escapeHtml(state.query.kind ?? "")}" style="${filterInputStyle};width:120px" /></label>
    <label>tag <input id="f-tag" value="${escapeHtml((state.query.tag ?? []).join(","))}" placeholder="comma,separated" style="${filterInputStyle};width:160px" /></label>
    <label>created >= <input id="f-created-from" value="${escapeHtml(state.query.createdFrom ?? "")}" placeholder="YYYY-MM-DD" style="${filterInputStyle};width:120px" /></label>
    <label>delta since <input id="f-delta-since" value="${escapeHtml(state.query.deltaSince ?? "")}" placeholder="ISO 8601" style="${filterInputStyle};width:170px" /></label>
    <label>min importance <input id="f-min-imp" type="number" step="0.05" value="${state.query.minImportance ?? ""}" style="${filterInputStyle};width:80px" /></label>
    <label><input type="checkbox" id="f-orphaned" ${state.query.orphaned ? "checked" : ""} /> orphaned only <span style="color:var(--qsf-text-muted)">(no association references this id)</span></label>
    <label><input type="checkbox" id="f-missing-lr" ${state.query.missingLastReinforced ? "checked" : ""} /> missing last_reinforced</label>
  `;

  const filterRow = row;
  const sync = () => {
    const minImportance = mustQuery<HTMLInputElement>(
      filterRow,
      "#f-min-imp",
    ).value;
    dispatch({
      type: "setQuery",
      query: {
        kind:
          mustQuery<HTMLInputElement>(filterRow, "#f-kind").value || undefined,
        tag: mustQuery<HTMLInputElement>(filterRow, "#f-tag")
          .value.split(",")
          .map((s) => s.trim())
          .filter(Boolean),
        createdFrom:
          mustQuery<HTMLInputElement>(filterRow, "#f-created-from").value ||
          undefined,
        deltaSince:
          mustQuery<HTMLInputElement>(filterRow, "#f-delta-since").value ||
          undefined,
        minImportance: minImportance === "" ? undefined : Number(minImportance),
        orphaned:
          mustQuery<HTMLInputElement>(filterRow, "#f-orphaned").checked ||
          undefined,
        missingLastReinforced:
          mustQuery<HTMLInputElement>(filterRow, "#f-missing-lr").checked ||
          undefined,
      },
    });
  };
  for (const input of row.querySelectorAll("input")) {
    input.addEventListener("change", sync);
  }
}
