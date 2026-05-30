//! Reachability-based link planning and artifact validation for JIT/AOT.

mod call_graph;
mod def_index;
mod plan;
mod validate;

pub use def_index::FunctionDefIndex;
pub use plan::{LinkPlan, LinkSymbol};
pub use validate::{MissingSymbol, validate_artifact};
