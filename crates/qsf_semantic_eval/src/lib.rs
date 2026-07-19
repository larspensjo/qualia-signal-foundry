//! Deterministic evaluation foundation for the `goal_relevance` behavior.
//! The crate root is deliberately a facade; schema, frozen roster, scoring, and reports
//! each live in focused modules.

mod report;
mod roster;
mod runner;
mod schema;

pub use report::*;
pub use roster::*;
pub use runner::*;
pub use schema::*;

#[cfg(test)]
mod tests;
