//! DTOs returned over /api/*. These are not the persisted types; mapping
//! happens explicitly in memory::mapping (Phase 2).

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LoadError {
    MissingFile {
        path: String,
        message: String,
    },
    InvalidJson {
        path: String,
        message: String,
    },
    UnsupportedSchema {
        path: String,
        message: String,
        schema_versions_found: SchemaVersions,
        schema_versions_supported: SchemaVersions,
    },
    InvalidStoreShape {
        path: String,
        message: String,
        schema_versions_found: SchemaVersions,
        shape_errors: Vec<ShapeError>,
    },
    DuplicateMemoryIds {
        path: String,
        message: String,
        duplicate_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SchemaVersions {
    pub records: Vec<u16>,
    pub associations: Vec<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShapeError {
    pub field_path: String,
    pub message: String,
}

impl From<&qsf_memory::StoreLoadError> for LoadError {
    fn from(err: &qsf_memory::StoreLoadError) -> Self {
        use qsf_memory::StoreLoadError::*;
        match err {
            MissingFile { path, message } => LoadError::MissingFile {
                path: path.display().to_string(),
                message: message.clone(),
            },
            InvalidJson { path, message } => LoadError::InvalidJson {
                path: path.display().to_string(),
                message: message.clone(),
            },
            UnsupportedSchema {
                path,
                message,
                schema_versions_found,
                schema_versions_supported,
            } => LoadError::UnsupportedSchema {
                path: path.display().to_string(),
                message: message.clone(),
                schema_versions_found: SchemaVersions {
                    records: schema_versions_found.records.clone(),
                    associations: schema_versions_found.associations.clone(),
                },
                schema_versions_supported: SchemaVersions {
                    records: schema_versions_supported.records.clone(),
                    associations: schema_versions_supported.associations.clone(),
                },
            },
            InvalidStoreShape {
                path,
                message,
                schema_versions_found,
                shape_errors,
            } => LoadError::InvalidStoreShape {
                path: path.display().to_string(),
                message: message.clone(),
                schema_versions_found: SchemaVersions {
                    records: schema_versions_found.records.clone(),
                    associations: schema_versions_found.associations.clone(),
                },
                shape_errors: shape_errors
                    .iter()
                    .map(|e| ShapeError {
                        field_path: e.field_path.clone(),
                        message: e.message.clone(),
                    })
                    .collect(),
            },
            DuplicateMemoryIds {
                path,
                message,
                duplicate_ids,
            } => LoadError::DuplicateMemoryIds {
                path: path.display().to_string(),
                message: message.clone(),
                duplicate_ids: duplicate_ids.clone(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryListItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub last_reinforced_at: Option<String>,
    pub importance: f64,
    pub reinforcement_count: u32,
    pub estimated_tokens: usize,
    pub association_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssociationDisplay {
    pub other_id: String,
    pub other_title: Option<String>,
    pub weight: f64,
    pub last_reinforced_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryDetail {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub last_reinforced_at: Option<String>,
    pub importance: f64,
    pub reinforcement_count: u32,
    pub source_reference: String,
    pub estimated_tokens: usize,
    pub incoming_count: usize,
    pub outgoing_count: usize,
    pub incoming: Vec<AssociationDisplay>,
    pub outgoing: Vec<AssociationDisplay>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssociationDisplayEdge {
    pub from_id: String,
    pub to_id: String,
    pub weight: f64,
    pub last_reinforced_at: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Neighborhood {
    pub center: MemoryListItem,
    pub edges: Vec<AssociationDisplayEdge>,
    pub members: Vec<MemoryListItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoreSummary {
    pub record_count: usize,
    pub association_count: usize,
    pub broken_associations_count: usize,
    pub total_estimated_tokens: usize,
    pub records_by_kind: BTreeMap<String, usize>,
    pub records_by_tag: Vec<(String, usize)>,
    pub newest: Vec<MemoryListItem>,
    pub most_reinforced: Vec<MemoryListItem>,
    pub highest_importance: Vec<MemoryListItem>,
    pub strongest_associations: Vec<AssociationDisplayEdge>,
    pub orphaned_count: usize,
    pub missing_last_reinforced_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MemoryPage {
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<MemoryListItem>,
}
