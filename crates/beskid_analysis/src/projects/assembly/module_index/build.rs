use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::projects::CompilePlan;
use crate::resolve::Resolver;

use super::super::{SourceUnit, hir_units::UnitHir, roots::EffectiveCompilationRoots};
use super::discovery::{collect_prefetched_import_closure, collect_prefetched_modules, normalize_assembly_path};
use super::model::ModuleIndex;
use super::path_inference::{infer_logical_module_path, package_for_unit};

impl ModuleIndex {
    /// Collect symbols from all units except `entry_index`.
    ///
    /// When `prefetch_dependency_roots` is false ([`AssemblyDiscovery::ImportClosure`]), symbols
    /// come only from assembled units — not a full scan of materialized dependency trees.
    pub fn build(
        units: &[SourceUnit],
        hir_units: &[UnitHir],
        entry_index: usize,
        roots: &EffectiveCompilationRoots,
        plan: &CompilePlan,
        prefetch_dependency_roots: bool,
    ) -> Self {
        let prefetch_span = tracing::info_span!(
            target: "beskid.analysis.assembly",
            "module_index.build",
            prefetch_dependency_roots,
            unit_count = units.len(),
            prefetched_paths = tracing::field::Empty,
        );
        let _prefetch_guard = prefetch_span.enter();

        let mut resolver = Resolver::new();
        resolver.collect_builtins();

        for (index, unit) in units.iter().enumerate() {
            if index == entry_index {
                continue;
            }
            let Some(unit_hir) = hir_units.get(index) else {
                continue;
            };
            let hir = &unit_hir.hir;
            let dep_packages: HashMap<String, String> = plan
                .dependency_projects
                .iter()
                .map(|dep| (dep.dependency_name.clone(), dep.project_name.clone()))
                .collect();
            resolver.set_declaring_package(package_for_unit(unit, roots, &plan.project_name, &dep_packages));
            if let Some(module_path) = infer_logical_module_path(unit, roots, plan.has_std_dependency) {
                resolver.collect_program_in_module(hir, &module_path, Some(&unit.path));
            } else {
                resolver.set_current_source_path(Some(unit.path.clone()));
                resolver.collect_program(hir);
            }
        }

        let dependency_packages: HashMap<String, String> = plan
            .dependency_projects
            .iter()
            .map(|dep| (dep.dependency_name.clone(), dep.project_name.clone()))
            .collect();
        let unit_paths: HashSet<PathBuf> = units.iter().map(|unit| normalize_assembly_path(&unit.path)).collect();
        let mut prefetched_paths = Vec::new();
        let mut prefetched_hir = HashMap::new();
        if prefetch_dependency_roots {
            for dep in &roots.dependencies {
                let declaring_package = dep
                    .dependency_name
                    .as_ref()
                    .and_then(|name| dependency_packages.get(name))
                    .cloned()
                    .or_else(|| dep.dependency_name.clone())
                    .unwrap_or_else(|| plan.project_name.clone());
                collect_prefetched_modules(
                    &mut resolver,
                    &dep.source_root,
                    &unit_paths,
                    &declaring_package,
                    plan.has_std_dependency,
                    &mut prefetched_paths,
                    &mut prefetched_hir,
                );
            }
        } else {
            collect_prefetched_import_closure(
                &mut resolver,
                roots,
                units,
                &unit_paths,
                &dependency_packages,
                plan,
                &mut prefetched_paths,
                &mut prefetched_hir,
            );
        }

        let (items, module_graph, builtin_items, symbols, by_symbol) = resolver.into_prefetch_parts();
        prefetch_span.record("prefetched_paths", prefetched_paths.len() as u64);
        Self {
            items,
            module_graph,
            builtin_items,
            symbols,
            by_symbol,
            entry_project_name: plan.project_name.clone(),
            dependency_packages,
            prefetched_paths,
            prefetched_hir,
        }
    }
}
