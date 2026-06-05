use std::collections::HashMap;
use std::path::PathBuf;

use crate::hir::HirItem;
use crate::syntax::Spanned;

use super::errors::{ResolveError, ResolveWarning};
use super::ids::{ItemId, ModuleId};
use super::items::ItemInfo;
use super::module_graph::ModuleGraph;
use super::symbol::{SymbolId, SymbolRegistry};
use super::tables::ResolutionTables;

#[derive(Debug, Default)]
pub struct Resolver {
    pub(crate) items: Vec<ItemInfo>,
    pub(crate) module_graph: ModuleGraph,
    pub(crate) current_module: ModuleId,
    pub(crate) tables: ResolutionTables,
    pub(crate) local_scopes: Vec<HashMap<String, super::ids::LocalId>>,
    pub(crate) generic_scopes: Vec<HashMap<String, ()>>,
    pub(crate) errors: Vec<ResolveError>,
    pub(crate) warnings: Vec<ResolveWarning>,
    pub(crate) builtin_items: HashMap<ItemId, usize>,
    pub(crate) module_imports: HashMap<String, Vec<String>>,
    pub(crate) current_source_path: Option<PathBuf>,
    pub(crate) symbols: SymbolRegistry,
    pub(crate) by_symbol: HashMap<SymbolId, ItemId>,
    pub(crate) declaring_package: String,
    /// When resolving method bodies, the extended/receiver type for bare field access (`handle` → `this.handle`).
    pub(crate) current_receiver_item_id: Option<super::ids::ItemId>,
}

impl Resolver {
    pub fn new() -> Self {
        Self::default()
    }
}

pub(super) fn path_segments(path: &Spanned<crate::hir::HirPath>) -> Vec<String> {
    path.node
        .segments
        .iter()
        .map(|segment| segment.node.name.node.name.clone())
        .collect()
}

pub(super) fn file_scoped_module_index(program: &Spanned<crate::hir::HirProgram>) -> Option<usize> {
    program.node.items.iter().position(|item| match &item.node {
        HirItem::ModuleDeclaration(def) => {
            def.node.visibility.node == crate::hir::HirVisibility::Private && def.node.attributes.is_empty()
        }
        _ => false,
    })
}

pub(super) fn file_scoped_module_path(program: &Spanned<crate::hir::HirProgram>) -> Option<Vec<String>> {
    let index = file_scoped_module_index(program)?;
    let HirItem::ModuleDeclaration(def) = &program.node.items.get(index)?.node else {
        return None;
    };
    Some(path_segments(&def.node.path))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub items: Vec<ItemInfo>,
    pub module_graph: ModuleGraph,
    pub tables: ResolutionTables,
    pub warnings: Vec<ResolveWarning>,
    pub builtin_items: HashMap<ItemId, usize>,
    pub module_imports: HashMap<String, Vec<String>>,
    pub symbols: SymbolRegistry,
    pub by_symbol: HashMap<SymbolId, ItemId>,
}