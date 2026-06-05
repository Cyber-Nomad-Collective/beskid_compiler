//! Cross-unit module graph built by collection-only resolver passes.

use std::collections::{HashMap, HashSet};

use crate::hir::HirProgram;
use crate::resolve::{
    ItemId, ItemInfo, ModuleGraph, Resolution, ResolveResult, Resolver, SymbolId, SymbolRegistry,
};
use crate::syntax::{Program, Spanned};

use super::SourceUnit;
use super::hir_units::UnitHir;
use super::roots::EffectiveCompilationRoots;
use crate::projects::CompilePlan;

/// Items and module paths collected from non-entry compilation units.
#[derive(Debug, Clone)]
pub struct ModuleIndex {
    items: Vec<ItemInfo>,
    module_graph: ModuleGraph,
    builtin_items: HashMap<ItemId, usize>,
    symbols: SymbolRegistry,
    by_symbol: HashMap<SymbolId, ItemId>,
    entry_project_name: String,
    dependency_packages: HashMap<String, String>,
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
        }
    }

    pub fn module_graph(&self) -> &ModuleGraph {
        &self.module_graph
    }

    /// Collect symbols from all units except `entry_index`.
    pub fn build(
        units: &[SourceUnit],
        hir_units: &[UnitHir],
        entry_index: usize,
        roots: &EffectiveCompilationRoots,
        plan: &CompilePlan,
    ) -> Self {
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
            resolver.set_declaring_package(package_for_unit(
                unit,
                roots,
                &plan.project_name,
                &dep_packages,
            ));
            if let Some(module_path) =
                infer_logical_module_path(unit, roots, plan.has_std_dependency)
            {
                resolver.collect_program_in_module(hir, &module_path, Some(&unit.path));
            } else {
                resolver.set_current_source_path(Some(unit.path.clone()));
                resolver.collect_program(hir);
            }
        }

        let dependency_packages = plan
            .dependency_projects
            .iter()
            .map(|dep| (dep.dependency_name.clone(), dep.project_name.clone()))
            .collect();
        let (items, module_graph, builtin_items, symbols, by_symbol) = resolver.into_prefetch_parts();
        Self {
            items,
            module_graph,
            builtin_items,
            symbols,
            by_symbol,
            entry_project_name: plan.project_name.clone(),
            dependency_packages,
        }
    }

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

    /// Full-project resolution for `api.json`: prefetch symbols, best-effort resolve entry + every unit.
    pub fn resolve_for_api_documentation(
        &self,
        entry_hir: &Spanned<HirProgram>,
        assembly: &super::ProgramAssembly,
    ) -> Option<Resolution> {
        let entry_source_path = assembly.entry_unit().path.clone();
        let entry_module_path = infer_logical_module_path(
            assembly.entry_unit(),
            &assembly.roots,
            assembly.has_std_dependency,
        );

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
        let mut resolution = resolver.resolve_collected_program_for_api_documentation(
            entry_hir,
            entry_module_path.as_deref(),
        );

        for (index, unit_hir) in assembly.hir_units.iter().enumerate() {
            if index == assembly.entry_index {
                continue;
            }
            let Some(unit) = assembly.units.get(index) else {
                continue;
            };
            let module_path = infer_logical_module_path(
                unit,
                &assembly.roots,
                assembly.has_std_dependency,
            );
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
            let unit_resolution = unit_resolver.resolve_collected_program_for_api_documentation(
                &unit_hir.hir,
                module_path.as_deref(),
            );
            resolution.tables.merge_from(&unit_resolution.tables, unit_hir.path.clone());
        }

        Some(resolution)
    }
}

/// Declaring package name for symbols collected from a compilation unit.
pub fn package_for_unit(
    unit: &SourceUnit,
    roots: &EffectiveCompilationRoots,
    host_project_name: &str,
    dependency_packages: &HashMap<String, String>,
) -> String {
    let path = &unit.path;
    if path.starts_with(&roots.host.source_root) {
        return host_project_name.to_string();
    }
    for dep in &roots.dependencies {
        if path.starts_with(&dep.source_root) {
            if let Some(dep_name) = &dep.dependency_name {
                if let Some(project_name) = dependency_packages.get(dep_name) {
                    return project_name.clone();
                }
                return dep_name.clone();
            }
        }
    }
    host_project_name.to_string()
}

pub fn infer_logical_module_path(
    unit: &SourceUnit,
    roots: &EffectiveCompilationRoots,
    has_std_dependency: bool,
) -> Option<Vec<String>> {
    let path = &unit.path;
    for root in std::iter::once(&roots.host).chain(roots.dependencies.iter()) {
        let Ok(rel) = path.strip_prefix(&root.source_root) else {
            continue;
        };
        let rel = rel.with_extension("");
        let mut segments: Vec<String> = rel
            .components()
            .filter_map(|component| match component {
                std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();
        if segments.is_empty() {
            continue;
        }
        collapse_homonymous_module_segment(&mut segments);
        if has_std_dependency {
            let mut with_std = vec!["Std".to_string()];
            with_std.extend(segments);
            return Some(with_std);
        }
        return Some(segments);
    }
    module_path_from_src_suffix(path, has_std_dependency)
}

/// When `Panel/Panel.bd` is inferred as `[…, Panel, Panel]`, items belong in module `[…, Panel]`.
fn collapse_homonymous_module_segment(segments: &mut Vec<String>) {
    if segments.len() >= 2 {
        let last = segments.len() - 1;
        if segments[last] == segments[last - 1] {
            segments.pop();
        }
    }
}

fn module_path_from_src_suffix(path: &std::path::Path, has_std_dependency: bool) -> Option<Vec<String>> {
    let path_str = path.to_string_lossy();
    let marker = "/src/";
    let idx = path_str.find(marker)?;
    let rel = std::path::Path::new(&path_str[idx + marker.len()..]).with_extension("");
    let segments: Vec<String> = rel
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    if segments.is_empty() {
        return None;
    }
    let mut segments = segments;
    collapse_homonymous_module_segment(&mut segments);
    if has_std_dependency {
        let mut with_std = vec!["Std".to_string()];
        with_std.extend(segments);
        Some(with_std)
    } else {
        Some(segments)
    }
}
