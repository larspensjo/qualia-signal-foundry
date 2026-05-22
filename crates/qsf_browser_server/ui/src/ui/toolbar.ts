import type { Action, ViewState } from "../state";
import { mustQuery } from "./html";

const inputStyle =
  "background:rgba(7,18,32,0.4);border:1px solid var(--qsf-border-subtle);color:var(--qsf-signal-context);padding:4px 8px";
const controlStyle =
  "background:rgba(7,18,32,0.4);color:var(--qsf-text-primary);border:1px solid var(--qsf-border-subtle);padding:4px 8px";

export function renderToolbar(
  el: HTMLElement,
  state: ViewState,
  storePath: string,
  dispatch: (a: Action) => void,
) {
  if (!el.dataset.built) {
    el.innerHTML = `
      <span style="color:var(--qsf-text-muted)">store</span>
      <code id="store-path" style="color:var(--qsf-signal-context)"></code>
      <input id="q" placeholder="search or paste id" style="flex:1;${inputStyle}" />
      <select id="sort" style="${controlStyle}">
        <option value="recent_activity">recent activity</option>
        <option value="newest">newest</option>
        <option value="oldest">oldest</option>
        <option value="most_reinforced">most reinforced</option>
        <option value="highest_importance">highest importance</option>
        <option value="strongest_connected">strongest connected</option>
        <option value="largest_tokens">largest tokens</option>
      </select>
      <button id="toggle-filters" style="${controlStyle};cursor:pointer">filters</button>
    `;
    const q = mustQuery<HTMLInputElement>(el, "#q");
    q.addEventListener("change", () =>
      dispatch({ type: "setQuery", query: { q: q.value || undefined } }),
    );
    q.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        dispatch({ type: "setQuery", query: { q: q.value || undefined } });
      }
    });
    const sort = mustQuery<HTMLSelectElement>(el, "#sort");
    sort.addEventListener("change", () =>
      dispatch({ type: "setQuery", query: { sort: sort.value } }),
    );
    mustQuery<HTMLButtonElement>(el, "#toggle-filters").addEventListener(
      "click",
      () => dispatch({ type: "toggleFilters" }),
    );
    el.dataset.built = "true";
  }

  const storeEl = mustQuery(el, "#store-path");
  if (storeEl.textContent !== storePath) storeEl.textContent = storePath;

  const q = mustQuery<HTMLInputElement>(el, "#q");
  const desiredQ = state.query.q ?? "";
  if (document.activeElement !== q && q.value !== desiredQ) q.value = desiredQ;

  const sort = mustQuery<HTMLSelectElement>(el, "#sort");
  const desiredSort = state.query.sort ?? "recent_activity";
  if (sort.value !== desiredSort) sort.value = desiredSort;

  const toggle = mustQuery<HTMLButtonElement>(el, "#toggle-filters");
  const desiredLabel = state.filtersExpanded ? "hide filters" : "filters";
  if (toggle.textContent !== desiredLabel) toggle.textContent = desiredLabel;
}
