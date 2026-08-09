//! Walk HIR bodies and discover call edges using [`TypeResult::call_kinds`].

mod generics;
mod method_contract;
mod path_resolution;
mod symbols;
mod traversal;

pub(crate) use path_resolution::resolve_item_call_id;
pub use path_resolution::resolve_path_item_id;
pub(crate) use traversal::collect_calls_in_body;
