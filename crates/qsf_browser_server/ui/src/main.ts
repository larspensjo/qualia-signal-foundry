import "./tokens.css";
import "./ui/layout.css";
import { api } from "./api";
import {
  reduce,
  stateToUrl,
  urlToState,
  type Action,
  type ViewState,
} from "./state";
import { renderFilters } from "./ui/filters";
import { renderInspector } from "./ui/inspector";
import { renderList } from "./ui/list";
import { renderLoadError } from "./ui/loadError";
import { getSlots, renderShell } from "./ui/shell";
import { renderToolbar } from "./ui/toolbar";

const root = document.getElementById("app")!;
let state: ViewState = urlToState(window.location.search);
let storePath = "";
let reloadSeq = 0;
let inspectorSeq = 0;

(async function bootstrap() {
  const health = await api.health();
  if (health.status === "error") {
    renderLoadError(root, health.load_error);
    return;
  }

  renderShell(root);
  const slots = getSlots(root);
  const summary = await api.storeSummary();
  storePath = "";
  slots.statusbar.textContent = `${summary.record_count} records · ${summary.association_count} associations · ${summary.broken_associations_count} broken edges · ${summary.total_estimated_tokens.toLocaleString()} tokens`;

  async function reload() {
    const seq = ++reloadSeq;
    renderToolbar(slots.toolbar, state, storePath || "(store)", dispatch);
    renderFilters(slots.top, state, dispatch);
    const page = await api.listMemories(state.query);
    if (seq !== reloadSeq) return;
    renderList(slots.list, page, state.selectedId, dispatch);
    if (state.selectedId) {
      const detailSeq = ++inspectorSeq;
      void renderInspector(
        slots.inspector,
        state.selectedId,
        dispatch,
        () => detailSeq === inspectorSeq,
      );
    } else {
      ++inspectorSeq;
      slots.inspector.innerHTML = `<div style="color:var(--qsf-text-muted)">Select a memory to inspect.</div>`;
    }
    history.replaceState(null, "", window.location.pathname + stateToUrl(state));
  }

  function dispatch(action: Action) {
    state = reduce(state, action);
    void reload();
  }

  await reload();
})().catch((err: unknown) => {
  root.className = "";
  root.innerHTML = `<div class="load-error"><h2>browser failed to start</h2><p>${err instanceof Error ? err.message : String(err)}</p></div>`;
});
