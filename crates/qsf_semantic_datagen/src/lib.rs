//! Offline construction contracts for the `goal_relevance` corpus.
//! The facade deliberately separates deterministic interchange handling from model transport.

mod artifacts;
mod generation;
mod preflight;
mod pricing;
mod transport;

pub use artifacts::*;
pub use generation::*;
pub use preflight::*;
pub use pricing::*;
pub use transport::*;

#[cfg(test)]
mod tests;
