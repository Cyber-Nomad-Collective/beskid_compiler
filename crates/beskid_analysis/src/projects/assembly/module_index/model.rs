use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// One syntax-discovered module in an assembled project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssemblyModule {
    pub path: Vec<String>,
    pub source_path: PathBuf,
    pub package: String,
    pub imports: Vec<Vec<String>>,
    pub declarations: Vec<Vec<String>>,
}

/// Syntax-only module graph for project discovery and path resolution.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleGraph {
    modules: Vec<AssemblyModule>,
    by_path: HashMap<Vec<String>, usize>,
    by_source: HashMap<PathBuf, usize>,
}

impl ModuleGraph {
    pub(super) fn insert(&mut self, module: AssemblyModule) {
        let index = self.modules.len();
        self.by_source.insert(super::discovery::normalize_assembly_path(&module.source_path), index);
        self.by_path.entry(module.path.clone()).or_insert(index);
        self.modules.push(module);
    }

    pub fn modules(&self) -> &[AssemblyModule] {
        &self.modules
    }

    pub fn module(&self, path: &[String]) -> Option<&AssemblyModule> {
        self.by_path.get(path).and_then(|index| self.modules.get(*index))
    }

    pub fn module_for_source(&self, path: &Path) -> Option<&AssemblyModule> {
        let key = super::discovery::normalize_assembly_path(path);
        self.by_source.get(&key).and_then(|index| self.modules.get(*index))
    }
}

/// Expanded syntax declarations and imports for an assembled project generation.
#[derive(Debug, Clone)]
pub struct ModuleIndex {
    pub(super) module_graph: ModuleGraph,
    pub(super) known_paths: HashSet<Vec<String>>,
    pub(super) prefetched_paths: Vec<PathBuf>,
}

impl ModuleIndex {
    pub fn empty() -> Self {
        Self { module_graph: ModuleGraph::default(), known_paths: HashSet::new(), prefetched_paths: Vec::new() }
    }

    pub fn module_graph(&self) -> &ModuleGraph {
        &self.module_graph
    }

    /// Source paths discovered outside the assembled unit set.
    pub fn prefetched_paths(&self) -> &[PathBuf] {
        &self.prefetched_paths
    }
}
