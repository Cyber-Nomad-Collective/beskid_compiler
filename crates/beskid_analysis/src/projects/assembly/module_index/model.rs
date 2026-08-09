use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::hir::HirProgram;
use crate::resolve::{ItemId, ItemInfo, ModuleGraph, SymbolId, SymbolRegistry};
use crate::syntax::Spanned;

use super::discovery::normalize_assembly_path;

/// Items and module paths collected from non-entry compilation units.
#[derive(Clone)]
pub struct ModuleIndex {
    pub(super) items: Vec<ItemInfo>,
    pub(super) module_graph: ModuleGraph,
    pub(super) builtin_items: HashMap<ItemId, usize>,
    pub(super) symbols: SymbolRegistry,
    pub(super) by_symbol: HashMap<SymbolId, ItemId>,
    pub(super) entry_project_name: String,
    pub(super) dependency_packages: HashMap<String, String>,
    pub(super) prefetched_paths: Vec<PathBuf>,
    /// Lowered HIR for prefetch-only sources (built during index construction, not re-read from disk).
    pub(super) prefetched_hir: HashMap<PathBuf, Arc<Spanned<HirProgram>>>,
}

impl std::fmt::Debug for ModuleIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModuleIndex")
            .field("items", &self.items.len())
            .field("prefetched_paths", &self.prefetched_paths.len())
            .field("prefetched_hir", &self.prefetched_hir.len())
            .finish()
    }
}

impl ModuleIndex {
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            module_graph: ModuleGraph::new_root(),
            builtin_items: HashMap::new(),
            symbols: SymbolRegistry::default(),
            by_symbol: HashMap::new(),
            entry_project_name: String::new(),
            dependency_packages: HashMap::new(),
            prefetched_paths: Vec::new(),
            prefetched_hir: HashMap::new(),
        }
    }

    /// Source paths scanned from dependency roots but not in the import-closure assembly.
    pub fn prefetched_paths(&self) -> &[PathBuf] {
        &self.prefetched_paths
    }

    /// Lowered HIR for a prefetch-only path (when present).
    pub fn prefetched_hir(&self, path: &Path) -> Option<&Spanned<HirProgram>> {
        let key = normalize_assembly_path(path);
        self.prefetched_hir.get(&key).or_else(|| self.prefetched_hir.get(path)).map(|hir| hir.as_ref())
    }

    pub fn module_graph(&self) -> &ModuleGraph {
        &self.module_graph
    }
}
