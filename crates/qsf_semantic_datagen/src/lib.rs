//! Offline construction contracts for the `goal_relevance` corpus.
//! The facade deliberately separates deterministic interchange handling from model transport.

mod artifacts;
mod transport;

pub use artifacts::*;
pub use transport::*;

#[cfg(test)]
mod tests;
