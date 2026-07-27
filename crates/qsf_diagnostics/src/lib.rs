//! Schema of the realtime diagnostics ledger (`state/realtime/diagnostics/<session>.jsonl`).
//!
//! These types are the persisted wire format, separated from the runtime that emits them so
//! readers (the sleep phase, the transcript command) share one definition instead of
//! re-describing the format by hand.

mod initiative_trace;
pub use initiative_trace::*;

mod live_goal_formation_trace;
pub use live_goal_formation_trace::*;

mod record;
pub use record::*;

mod turn_phase;
pub use turn_phase::*;

mod volition_injection_trace;
pub use volition_injection_trace::*;

mod world_consultation_trace;
pub use world_consultation_trace::*;

mod writer;
pub use writer::*;
