//! Reachability-based link planning and artifact validation for JIT/AOT.

mod call_graph;
mod def_index;
mod plan;
mod validate;

pub(crate) use call_graph::resolve_item_call_id;
pub use call_graph::resolve_path_item_id;
pub use def_index::FunctionDefIndex;
pub use plan::{LinkPlan, LinkSymbol};
pub use validate::{MissingSymbol, referenced_extern_imports, validate_artifact};
