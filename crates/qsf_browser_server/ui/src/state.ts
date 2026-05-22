import type { ListMemoriesQuery } from "./api";

export interface ViewState {
  selectedId: string | null;
  query: ListMemoriesQuery;
  filtersExpanded: boolean;
}

export const initialState: ViewState = {
  selectedId: null,
  query: { sort: "recent_activity", limit: 50 },
  filtersExpanded: false,
};

export type Action =
  | { type: "select"; id: string | null }
  | { type: "setQuery"; query: ListMemoriesQuery }
  | { type: "toggleFilters" };

export function reduce(state: ViewState, action: Action): ViewState {
  switch (action.type) {
    case "select":
      return { ...state, selectedId: action.id };
    case "setQuery":
      return {
        ...state,
        query: normalizeQuery({
          ...state.query,
          ...action.query,
          offset: undefined,
        }),
      };
    case "toggleFilters":
      return { ...state, filtersExpanded: !state.filtersExpanded };
  }
}

function normalizeQuery(query: ListMemoriesQuery): ListMemoriesQuery {
  return Object.fromEntries(
    Object.entries(query).filter(([, value]) => {
      if (Array.isArray(value)) return value.length > 0;
      return value !== undefined && value !== null && value !== "";
    }),
  ) as ListMemoriesQuery;
}

export function stateToUrl(state: ViewState): string {
  const sp = new URLSearchParams();
  const { selectedId, query } = state;
  if (selectedId) sp.set("id", selectedId);
  for (const [key, value] of Object.entries(query)) {
    if (value === undefined || value === null || value === "") continue;
    if (Array.isArray(value)) {
      for (const x of value) sp.append(key, String(x));
    } else {
      sp.set(key, String(value));
    }
  }
  const s = sp.toString();
  return s ? `?${s}` : "";
}

export function urlToState(search: string): ViewState {
  const sp = new URLSearchParams(search);
  const query: ListMemoriesQuery = {};
  const set = (
    key: keyof ListMemoriesQuery,
    parser: (s: string) => unknown = (x) => x,
  ) => {
    const value = sp.get(String(key));
    if (value !== null) {
      (query as Record<string, unknown>)[key as string] = parser(value);
    }
  };
  set("q");
  set("kind");
  const tag = sp.getAll("tag");
  if (tag.length) query.tag = tag;
  set("createdFrom");
  set("createdTo");
  set("lastReinforcedFrom");
  set("lastReinforcedTo");
  set("deltaSince");
  set("minImportance", Number);
  set("minReinforcementCount", Number);
  set("hasAssociations", (s) => s === "true");
  set("orphaned", (s) => s === "true");
  set("missingLastReinforced", (s) => s === "true");
  set("sort");
  set("limit", Number);
  set("offset", Number);
  return {
    selectedId: sp.get("id"),
    query: { sort: "recent_activity", limit: 50, ...query },
    filtersExpanded: false,
  };
}
