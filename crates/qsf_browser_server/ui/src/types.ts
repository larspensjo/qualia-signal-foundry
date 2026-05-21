export type MemoryKind =
  | "concept"
  | "architecture_note"
  | "experiment"
  | "decision"
  | "question"
  | "observation";

export interface MemoryListItem {
  id: string;
  kind: string;
  title: string;
  summary: string;
  tags: string[];
  created_at: string;
  last_reinforced_at: string | null;
  importance: number;
  reinforcement_count: number;
  estimated_tokens: number;
  association_count: number;
}

export interface AssociationDisplay {
  other_id: string;
  other_title: string | null;
  weight: number;
  last_reinforced_at: string;
  reason: string;
}

export interface MemoryDetail {
  id: string;
  kind: string;
  title: string;
  summary: string;
  tags: string[];
  created_at: string;
  last_reinforced_at: string | null;
  importance: number;
  reinforcement_count: number;
  source_reference: string;
  estimated_tokens: number;
  incoming_count: number;
  outgoing_count: number;
  incoming: AssociationDisplay[];
  outgoing: AssociationDisplay[];
}

export interface AssociationDisplayEdge {
  from_id: string;
  to_id: string;
  weight: number;
  last_reinforced_at: string;
  reason: string;
}

export interface Neighborhood {
  center: MemoryListItem;
  edges: AssociationDisplayEdge[];
  members: MemoryListItem[];
}

export interface StoreSummary {
  record_count: number;
  association_count: number;
  broken_associations_count: number;
  total_estimated_tokens: number;
  records_by_kind: Record<string, number>;
  records_by_tag: Array<[string, number]>;
  newest: MemoryListItem[];
  most_reinforced: MemoryListItem[];
  highest_importance: MemoryListItem[];
  strongest_associations: AssociationDisplayEdge[];
  orphaned_count: number;
  missing_last_reinforced_count: number;
}

export interface MemoryPage {
  total: number;
  offset: number;
  limit: number;
  items: MemoryListItem[];
}

export type LoadError =
  | { kind: "missing_file"; path: string; message: string }
  | { kind: "invalid_json"; path: string; message: string }
  | {
      kind: "unsupported_schema";
      path: string;
      message: string;
      schema_versions_found: { records: number[]; associations: number[] };
      schema_versions_supported: { records: number[]; associations: number[] };
    }
  | {
      kind: "invalid_store_shape";
      path: string;
      message: string;
      schema_versions_found: { records: number[]; associations: number[] };
      shape_errors: Array<{ field_path: string; message: string }>;
    }
  | {
      kind: "duplicate_memory_ids";
      path: string;
      message: string;
      duplicate_ids: string[];
    };

export type HealthResponse =
  | { status: "ok" }
  | { status: "error"; load_error: LoadError };
