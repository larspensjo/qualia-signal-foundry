//! Context fragments, budgets, and deterministic assembly.

pub mod context_assembler;
pub mod context_budget;
pub mod context_fragment;

pub use context_assembler::assemble_context;
pub use qsf_session::context::{
    ContextAssembly, ContextBudget, ContextFragment, ContextOmission, ContextSelection,
    ContextSourceKind,
};
