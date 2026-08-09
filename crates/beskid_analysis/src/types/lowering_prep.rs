//! Codegen metadata produced after type checking: call dispatch kinds and numeric cast intents.

mod compatibility;
mod model;
mod substitution;
mod walker;

pub use model::{CastIntent, LoweringPrep, LoweringPrepSurfaces};
