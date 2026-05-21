//! DTOs returned over /api/*. These are not the persisted types; mapping
//! happens explicitly in memory::mapping (Phase 2).

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
