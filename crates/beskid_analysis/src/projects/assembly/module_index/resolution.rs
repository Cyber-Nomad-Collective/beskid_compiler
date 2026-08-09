use std::collections::HashSet;

use crate::hir::HirProgram;
use crate::resolve::{Resolution, ResolveResult, Resolver};
use crate::syntax::{Program, Spanned};

use super::model::ModuleIndex;
use super::path_inference::{
    declaring_package_for_prefetched_path, infer_logical_module_path, package_for_unit, prefetched_module_path_for_file,
};

impl ModuleIndex {
    /// Logical module paths present in the prefetch graph (`Std::System::IO`, etc.).
    pub fn known_module_path_strings(&self) -> HashSet<String> {
        self.module_graph
            .modules()
            .iter()
            .filter(|module| !module.path.is_empty())
            .map(|module| module.path.join("::"))
            .collect()
    }

    /// Resolve the entry unit against prefetched external modules (lowers AST only; prefer [`Self::resolve_entry_hir`] when HIR is already normalized).
    pub fn resolve_entry(&self, entry_program: &Spanned<Program>) -> ResolveResult<Resolution> {
        use super::hir_units::unit_to_hir;
        let entry_hir = unit_to_hir(entry_program);
        self.resolve_entry_hir(&entry_hir, None)
    }

    /// Resolve entry HIR against prefetched modules (spans must match the HIR passed to type checking).
    pub fn resolve_entry_hir(
        &self,
        entry_hir: &Spanned<HirProgram>,
        entry_source_path: Option<&std::path::PathBuf>,
    ) -> ResolveResult<Resolution> {
        let mut resolver = Resolver::with_module_prefetch(
            self.items.clone(),
            self.module_graph.clone(),
            self.builtin_items.clone(),
            self.symbols.clone(),
            self.by_symbol.clone(),
        );
        resolver.set_declaring_package(self.entry_project_name.clone());
        resolver.set_current_source_path(entry_source_path.cloned());
        resolver.collect_program(entry_hir);
        resolver.resolve_collected_program(entry_hir)
    }

    /// Resolve references in a non-entry unit using the prefetch graph (skips re-collection).
    pub fn resolve_unit_hir(
        &self,
        unit_hir: &Spanned<HirProgram>,
        unit_source_path: &std::path::Path,
    ) -> ResolveResult<Resolution> {
        let mut resolver = Resolver::with_module_prefetch(
            self.items.clone(),
            self.module_graph.clone(),
            self.builtin_items.clone(),
            self.symbols.clone(),
            self.by_symbol.clone(),
        );
        resolver.set_current_source_path(Some(unit_source_path.to_path_buf()));
        resolver.resolve_collected_program(unit_hir)
    }

    /// Best-effort resolve for a non-entry unit (merges locals into entry resolution for codegen).
    pub fn resolve_unit_hir_best_effort(
        &self,
        unit_hir: &Spanned<HirProgram>,
        unit_source_path: &std::path::Path,
    ) -> Resolution {
        let mut resolver = Resolver::with_module_prefetch(
            self.items.clone(),
            self.module_graph.clone(),
            self.builtin_items.clone(),
            self.symbols.clone(),
            self.by_symbol.clone(),
        );
        resolver.set_current_source_path(Some(unit_source_path.to_path_buf()));
        resolver.resolve_collected_program_for_api_documentation(unit_hir, None)
    }

    /// Resolve entry plus every assembled unit (import closure). Skips prefetch-only paths.
    pub fn resolve_assembly_closure(
        &self,
        entry_hir: &Spanned<HirProgram>,
        assembly: &super::ProgramAssembly,
    ) -> Option<Resolution> {
        let entry_source_path = assembly.entry_unit().path.clone();
        let entry_module_path =
            infer_logical_module_path(assembly.entry_unit(), &assembly.roots, assembly.has_std_dependency);

        let mut resolver = Resolver::with_module_prefetch(
            self.items.clone(),
            self.module_graph.clone(),
            self.builtin_items.clone(),
            self.symbols.clone(),
            self.by_symbol.clone(),
        );
        resolver.set_declaring_package(self.entry_project_name.clone());
        resolver.set_current_source_path(Some(entry_source_path.clone()));
        resolver.collect_program(entry_hir);
        let mut resolution =
            resolver.resolve_collected_program_for_api_documentation(entry_hir, entry_module_path.as_deref());

        for (index, unit_hir) in assembly.hir_units.iter().enumerate() {
            if index == assembly.entry_index {
                continue;
            }
            let Some(unit) = assembly.units.get(index) else {
                continue;
            };
            let module_path = infer_logical_module_path(unit, &assembly.roots, assembly.has_std_dependency);
            let mut unit_resolver = Resolver::with_module_prefetch(
                self.items.clone(),
                self.module_graph.clone(),
                self.builtin_items.clone(),
                self.symbols.clone(),
                self.by_symbol.clone(),
            );
            unit_resolver.set_declaring_package(package_for_unit(
                unit,
                &assembly.roots,
                &self.entry_project_name,
                &self.dependency_packages,
            ));
            unit_resolver.set_current_source_path(Some(unit_hir.path.clone()));
            if let Some(ref path) = module_path {
                unit_resolver.collect_program_in_module(&unit_hir.hir, path, Some(&unit.path));
            } else {
                unit_resolver.collect_program(&unit_hir.hir);
            }
            let unit_resolution =
                unit_resolver.resolve_collected_program_for_api_documentation(&unit_hir.hir, module_path.as_deref());
            resolution.tables.merge_from(&unit_resolution.tables, unit_hir.path.clone());
        }

        self.merge_prefetched_path_resolutions(&mut resolution, assembly);

        Some(resolution)
    }

    /// Full-project resolution for `api.json`: assembly closure plus any remaining prefetch-only paths.
    pub fn resolve_for_api_documentation(
        &self,
        entry_hir: &Spanned<HirProgram>,
        assembly: &super::ProgramAssembly,
    ) -> Option<Resolution> {
        self.resolve_assembly_closure(entry_hir, assembly)
    }

    /// Resolve locals and value tables for prefetch-only sources not in the assembly closure.
    fn merge_prefetched_path_resolutions(&self, resolution: &mut Resolution, assembly: &super::ProgramAssembly) {
        for path in &self.prefetched_paths {
            if assembly.hir_units.iter().any(|unit| crate::paths::same_file(&unit.path, path)) {
                continue;
            }
            let Some(hir) = self.prefetched_hir(path) else {
                continue;
            };
            let key = crate::paths::unit_path_key(path);
            let declaring_package = declaring_package_for_prefetched_path(
                path,
                assembly,
                &self.entry_project_name,
                &self.dependency_packages,
            );
            let module_path = prefetched_module_path_for_file(path, assembly);

            let mut unit_resolver = Resolver::with_module_prefetch(
                self.items.clone(),
                self.module_graph.clone(),
                self.builtin_items.clone(),
                self.symbols.clone(),
                self.by_symbol.clone(),
            );
            unit_resolver.set_declaring_package(declaring_package);
            unit_resolver.set_current_source_path(Some(key.clone()));
            if let Some(ref module_path) = module_path {
                unit_resolver.collect_program_in_module(hir, module_path, Some(path));
            } else {
                unit_resolver.collect_program(hir);
            }
            let unit_resolution =
                unit_resolver.resolve_collected_program_for_api_documentation(hir, module_path.as_deref());
            resolution.tables.merge_from(&unit_resolution.tables, key);
        }
        resolution.rebuild_span_index();
    }
}
