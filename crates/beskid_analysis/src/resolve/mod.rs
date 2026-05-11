//! Name and module-path resolution over HIR: [`Resolver::resolve_program`], span-keyed tables, and warnings.

pub mod errors;
pub mod ids;
pub mod items;
pub mod member_items;
pub mod module_graph;
pub mod resolver;
pub mod tables;

pub use errors::{ResolveError, ResolveResult, ResolveWarning};
pub use ids::{ItemId, LocalId, ModuleId};
pub use items::{ItemInfo, ItemKind};
pub use module_graph::{ModuleGraph, ModuleInfo};
pub use resolver::{Resolution, Resolver};
pub use tables::{LocalInfo, ResolutionTables, ResolvedType, ResolvedValue};
