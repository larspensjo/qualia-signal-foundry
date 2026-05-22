import type {
  HealthResponse,
  MemoryDetail,
  MemoryPage,
  Neighborhood,
  StoreSummary,
} from "./types";

export interface ListMemoriesQuery {
  q?: string;
  kind?: string;
  tag?: string[];
  createdFrom?: string;
  createdTo?: string;
  lastReinforcedFrom?: string;
  lastReinforcedTo?: string;
  deltaSince?: string;
  minImportance?: number;
  minReinforcementCount?: number;
  hasAssociations?: boolean;
  orphaned?: boolean;
  missingLastReinforced?: boolean;
  sort?: string;
  limit?: number;
  offset?: number;
}

function qs(params: Record<string, unknown>): string {
  const sp = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null || value === "") continue;
    if (Array.isArray(value)) {
      for (const item of value) sp.append(key, String(item));
    } else {
      sp.set(key, String(value));
    }
  }
  const s = sp.toString();
  return s ? `?${s}` : "";
}

async function getJson<T>(url: string): Promise<T> {
  const res = await fetch(url);
  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw Object.assign(new Error(`HTTP ${res.status} on ${url}`), {
      status: res.status,
      body,
    });
  }
  return res.json() as Promise<T>;
}

export const api = {
  health: () => getJson<HealthResponse>("/api/health"),
  storeSummary: () => getJson<StoreSummary>("/api/store/summary"),
  listMemories: (q: ListMemoriesQuery) =>
    getJson<MemoryPage>(
      "/api/memories" +
        qs({
          q: q.q,
          kind: q.kind,
          tag: q.tag,
          created_from: q.createdFrom,
          created_to: q.createdTo,
          last_reinforced_from: q.lastReinforcedFrom,
          last_reinforced_to: q.lastReinforcedTo,
          delta_since: q.deltaSince,
          min_importance: q.minImportance,
          min_reinforcement_count: q.minReinforcementCount,
          has_associations: q.hasAssociations,
          orphaned: q.orphaned,
          missing_last_reinforced: q.missingLastReinforced,
          sort: q.sort,
          limit: q.limit,
          offset: q.offset,
        }),
    ),
  getMemory: (id: string) =>
    getJson<MemoryDetail>(`/api/memories/${encodeURIComponent(id)}`),
  getMemoryRaw: (id: string) =>
    getJson<unknown>(`/api/memories/${encodeURIComponent(id)}/raw`),
  getMemoryNeighborhood: (id: string, limit = 8) =>
    getJson<Neighborhood>(
      `/api/memories/${encodeURIComponent(id)}/neighborhood?limit=${limit}`,
    ),
};
