//! Reachability-based link planning and artifact validation for JIT/AOT.

mod call_graph;
mod def_index;
mod plan;
mod validate;

pub(crate) use call_graph::{resolve_item_call_id, return_type_for_module_path_call};
pub use call_graph::resolve_path_item_id;
pub use def_index::{load_hir_program_for_item, FunctionDefIndex};
pub(crate) use def_index::{
    find_function_by_name, find_function_by_span, find_method_by_name, find_method_by_span,
};
pub use plan::{LinkPlan, LinkSymbol};
pub use validate::{MissingSymbol, referenced_extern_imports, validate_artifact};
