//! Facade over the persisted diagnostics schema, which lives in `qsf_diagnostics` so readers
//! outside this crate share one definition. Kept as a module path because the server's write
//! sites refer to `crate::diagnostics::*`.

pub use qsf_diagnostics::{DiagnosticRecord, DiagnosticTrust, DiagnosticWriter};
