use std::collections::HashSet;
use std::path::Path;

use crate::resolve::{Resolution, ResolveResult, Resolver};
use crate::syntax::{Program, Spanned};

use super::super::ProgramAssembly;
use super::model::ModuleIndex;
use super::path_inference::infer_logical_module_path;

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

    /// Resolve the entry program against the assembled module graph.
    ///
    /// Seeds the resolver with all known module paths and collects dependency
    /// units so that `use` declarations and cross-module references in the entry
    /// program resolve against the full assembled closure.
    pub fn resolve_entry_program(
        &self,
        program: &Spanned<Program>,
        entry_source_path: Option<&Path>,
        assembly: &ProgramAssembly,
    ) -> ResolveResult<Resolution> {
        let mut resolver = Resolver::new();
        resolver.set_current_source_path(entry_source_path.map(|path| path.to_path_buf()));

        self.seed_resolver_from_assembly(&mut resolver, assembly);

        resolver.resolve_program(program)
    }

    /// Resolve a non-entry unit program against the assembled module graph.
    ///
    /// Like [`Self::resolve_entry_program`], seeds the resolver with all known
    /// module paths and collects sibling units so cross-module references resolve.
    pub fn resolve_unit_program(
        &self,
        program: &Spanned<Program>,
        unit_source_path: &Path,
        assembly: &ProgramAssembly,
    ) -> ResolveResult<Resolution> {
        let mut resolver = Resolver::new();
        resolver.set_current_source_path(Some(unit_source_path.to_path_buf()));

        self.seed_resolver_from_assembly(&mut resolver, assembly);

        resolver.resolve_program(program)
    }

    /// Seed `resolver` with all known module paths and collect every non-entry
    /// unit's items so the entry program's `use` declarations and references
    /// resolve against the full assembled closure.
    fn seed_resolver_from_assembly(&self, resolver: &mut Resolver, assembly: &ProgramAssembly) {
        for path in &self.known_paths {
            resolver.module_graph.ensure_module_path(path);
        }
        for (index, unit) in assembly.units.iter().enumerate() {
            if index == assembly.entry_index {
                continue;
            }
            let module_path = infer_logical_module_path(unit, &assembly.roots, assembly.has_std_dependency);
            if let Some(path) = module_path.as_ref() {
                resolver.collect_program_in_module(&unit.program, path, Some(&unit.path));
            }
        }
        resolver.errors.clear();
        resolver.module_imports.clear();
    }

    /// Full-project resolution for `api.json`: resolve entry plus every assembled unit, merging tables.
    pub fn resolve_for_api_documentation(
        &self,
        entry_program: &Spanned<Program>,
        assembly: &ProgramAssembly,
    ) -> Option<Resolution> {
        let entry_source_path = assembly.entry_unit().path.clone();
        let entry_module_path =
            infer_logical_module_path(assembly.entry_unit(), &assembly.roots, assembly.has_std_dependency);

        let mut resolver = Resolver::new();
        resolver.set_current_source_path(Some(entry_source_path.clone()));
        self.seed_resolver_from_assembly(&mut resolver, assembly);
        let mut resolution =
            resolver.resolve_collected_program_for_api_documentation(entry_program, entry_module_path.as_deref());

        for (index, unit) in assembly.units.iter().enumerate() {
            if index == assembly.entry_index {
                continue;
            }
            let module_path = infer_logical_module_path(unit, &assembly.roots, assembly.has_std_dependency);
            let mut unit_resolver = Resolver::new();
            unit_resolver.set_current_source_path(Some(unit.path.clone()));
            self.seed_resolver_from_assembly(&mut unit_resolver, assembly);
            let unit_resolution =
                unit_resolver.resolve_collected_program_for_api_documentation(&unit.program, module_path.as_deref());
            resolution.tables.merge_from(&unit_resolution.tables, unit.path.clone());
        }
        resolution.rebuild_span_index();
        Some(resolution)
    }

    fn resolve_longest_module_prefix(&self, path: &[String]) -> Option<&Path> {
        (1..=path.len()).rev().find_map(|length| self.resolve_module(&path[..length]))
    }
}
