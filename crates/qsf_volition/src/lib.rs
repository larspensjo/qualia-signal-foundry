//! Volition domain: tensions, goals, initiatives, arbitration, and the pure reducer.
//! This crate root is a facade — every type and function lives in a focused module and is
//! re-exported here.

mod arbitration;
pub use arbitration::*;

mod candidate;
pub use candidate::*;

mod coherence;
pub use coherence::*;

mod continuity;
pub use continuity::*;

mod consolidation;
pub use consolidation::*;

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

mod selection;
pub use selection::*;

mod inspection;
pub use inspection::*;

mod opportunity;
pub use opportunity::*;

mod shaping;
pub use shaping::*;

mod signals;
pub use signals::*;

mod stance;
pub use stance::*;

mod visibility;
pub use visibility::*;
