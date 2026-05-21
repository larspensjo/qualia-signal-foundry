use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StoreLoadError {
    #[error("memory store file not found at {path}")]
    MissingFile { path: PathBuf, message: String },

    #[error("memory store at {path} is not valid JSON: {message}")]
    InvalidJson { path: PathBuf, message: String },

    #[error("memory store at {path} uses unsupported schema versions")]
    UnsupportedSchema {
        path: PathBuf,
        message: String,
        // Boxed to keep StoreLoadError small enough for Result-returning APIs
        // under clippy::result_large_err.
        schema_versions_found: Box<SchemaVersions>,
        schema_versions_supported: Box<SchemaVersions>,
    },

    #[error("memory store at {path} fails structural validation")]
    InvalidStoreShape {
        path: PathBuf,
        message: String,
        // Boxed to keep StoreLoadError small enough for Result-returning APIs
        // under clippy::result_large_err.
        schema_versions_found: Box<SchemaVersions>,
        shape_errors: Vec<ShapeError>,
    },

    #[error("memory store at {path} contains duplicate memory ids")]
    DuplicateMemoryIds {
        path: PathBuf,
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
