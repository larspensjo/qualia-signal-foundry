//! Interchangeable implementations of the pair-scoring contract.

/// Deterministic backend for tests and no-cost end-to-end exercises.
pub mod fixture;
/// HTTP backend for the hosted System One judge.
#[cfg(feature = "remote-http")]
pub mod remote_http;
