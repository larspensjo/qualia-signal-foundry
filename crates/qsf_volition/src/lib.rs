//! Volition domain: tensions, goals, initiatives, arbitration, and the pure reducer.
//! This crate root is a facade — every type and function lives in a focused module and is
//! re-exported here.

mod arbitration;
pub use arbitration::*;

mod candidate;
pub use candidate::*;

mod evidence;
pub use evidence::*;

mod fixture;
pub use fixture::*;

mod initiative;
pub use initiative::*;

mod model;
pub use model::*;

mod reducer;
pub use reducer::*;

mod terms;
pub use terms::*;
