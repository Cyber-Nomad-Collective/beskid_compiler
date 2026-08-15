use std::collections::HashSet;
use std::path::Path;

use super::model::ModuleIndex;

impl ModuleIndex {
    /// Logical module paths present or explicitly declared in expanded syntax.
    pub fn known_module_path_strings(&self) -> HashSet<String> {
        self.known_paths.iter().filter(|path| !path.is_empty()).map(|path| path.join("::")).collect()
    }

    /// Resolve a complete module path to the assembled source that owns it.
    pub fn resolve_module(&self, path: &[String]) -> Option<&Path> {
        self.module_graph.module(path).map(|module| module.source_path.as_path())
    }

    /// Resolve one unit's expanded import paths against the assembled module graph.
    pub fn resolve_imports_for_source(&self, source_path: &Path) -> Vec<(Vec<String>, Option<&Path>)> {
        let Some(module) = self.module_graph.module_for_source(source_path) else {
            return Vec::new();
        };
        module.imports.iter().map(|import| (import.clone(), self.resolve_longest_module_prefix(import))).collect()
    }

    fn resolve_longest_module_prefix(&self, path: &[String]) -> Option<&Path> {
        (1..=path.len()).rev().find_map(|length| self.resolve_module(&path[..length]))
    }
}
