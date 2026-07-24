//! Name and module-path resolution over HIR: [`Resolver::resolve_program`], span-keyed tables, and warnings.

pub mod collect;
pub mod errors;
pub mod ids;
pub mod items;
pub mod member_items;
pub mod module_graph;
pub mod resolve_refs;
pub mod resolver;
pub mod span_index;
pub mod symbol;
pub mod symbol_lookup;
pub mod tables;

pub use errors::{ResolveError, ResolveResult, ResolveWarning};
pub use ids::{HirNodeId, ItemId, LocalId, ModuleId};
pub use items::{ItemInfo, ItemKind};
pub use module_graph::{ModuleGraph, ModuleInfo};
pub use resolver::{Resolution, Resolver};
pub use span_index::SpanIndex;
pub use symbol::{
    BUILTIN_PACKAGE, ExportKind, MemberKind, SymbolId, SymbolQualifier, SymbolRegistry, SymbolShape, symbol_key,
    symbol_shape_for_item, symbol_to_string,
};
pub use symbol_lookup::{canonical_item_id, item_id_for_symbol, qualified_name, symbol_for_item};
pub use tables::{LocalInfo, ResolutionTables, ResolvedType, ResolvedValue};
