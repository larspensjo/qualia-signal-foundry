//! Context fragments, budgets, and deterministic assembly.

pub mod context_assembler;
pub mod context_budget;
pub mod context_fragment;

pub use context_assembler::{ContextAssembly, ContextOmission, ContextSelection, assemble_context};
pub use context_budget::ContextBudget;
pub use context_fragment::{ContextFragment, ContextSourceKind};
