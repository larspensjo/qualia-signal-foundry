//! Offline construction contracts for the `goal_relevance` corpus.
//! The facade deliberately separates deterministic interchange handling from model transport.

mod artifacts;
mod frozen;
mod generation;
mod labeling;
mod preflight;
mod pricing;
mod review;
mod transport;

pub use artifacts::*;
pub use frozen::*;
pub use generation::*;
pub use labeling::*;
pub use preflight::*;
pub use pricing::*;
pub use review::*;
pub use transport::*;

#[cfg(test)]
mod tests;
