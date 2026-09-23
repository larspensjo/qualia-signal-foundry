//! Re-export of the shared context domain from qsf_context.
//! Kept for backwards compatibility with existing import paths.
pub use qsf_context::{
    AdmissionBasis, ContextAssembly, ContextBudget, ContextFragment, ContextOmission,
    ContextSelection, ContextSourceKind, assemble_context, assemble_context_with_ordering,
};
