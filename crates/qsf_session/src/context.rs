//! Re-export of the shared context domain from qsf_context.
//! Kept for backwards compatibility with existing import paths.
pub use qsf_context::{
    ContextAssembly, ContextBudget, ContextFragment, ContextOmission, ContextSelection,
    ContextSourceKind, assemble_context,
};
