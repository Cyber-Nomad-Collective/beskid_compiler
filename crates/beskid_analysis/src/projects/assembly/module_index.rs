//! Cross-unit module graph built by collection-only resolver passes.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::hir::HirProgram;
use crate::resolve::{
    ItemId, ItemInfo, ModuleGraph, Resolution, ResolveResult, Resolver, SymbolId, SymbolRegistry,
};
use crate::syntax::{Program, Spanned};

use super::SourceUnit;
use super::discovery::resolve_module_file;
use super::hir_units::UnitHir;
use super::loader::{
    import_paths_from_source_full, module_paths_from_qualified_references,
    parent_module_import_path,
};
use super::roots::EffectiveCompilationRoots;
use crate::projects::CompilePlan;

/// Items and module paths collected from non-entry compilation units.
#[derive(Clone)]
pub struct ModuleIndex {
    items: Vec<ItemInfo>,
    module_graph: ModuleGraph,
    builtin_items: HashMap<ItemId, usize>,
    symbols: SymbolRegistry,
    by_symbol: HashMap<SymbolId, ItemId>,
    entry_project_name: String,
    dependency_packages: HashMap<String, String>,
    prefetched_paths: Vec<PathBuf>,
    /// Lowered HIR for prefetch-only sources (built during index construction, not re-read from disk).
    prefetched_hir: HashMap<PathBuf, Arc<Spanned<HirProgram>>>,
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
        self.prefetched_hir
            .get(&key)
            .or_else(|| self.prefetched_hir.get(path))
            .map(|hir| hir.as_ref())
    }

    pub fn module_graph(&self) -> &ModuleGraph {
        &self.module_graph
    }

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

        let dependency_packages: HashMap<String, String> = plan
            .dependency_projects
            .iter()
            .map(|dep| (dep.dependency_name.clone(), dep.project_name.clone()))
            .collect();
        let unit_paths: HashSet<PathBuf> = units
            .iter()
            .map(|unit| normalize_assembly_path(&unit.path))
            .collect();
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

        let (items, module_graph, builtin_items, symbols, by_symbol) =
            resolver.into_prefetch_parts();
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
            let module_path =
                infer_logical_module_path(unit, &assembly.roots, assembly.has_std_dependency);
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
            let unit_resolution = unit_resolver.resolve_collected_program_for_api_documentation(
                &unit_hir.hir,
                module_path.as_deref(),
            );
            resolution
                .tables
                .merge_from(&unit_resolution.tables, unit_hir.path.clone());
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
    fn merge_prefetched_path_resolutions(
        &self,
        resolution: &mut Resolution,
        assembly: &super::ProgramAssembly,
    ) {
        for path in &self.prefetched_paths {
            if assembly
                .hir_units
                .iter()
                .any(|unit| crate::paths::same_file(&unit.path, path))
            {
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
            let unit_resolution = unit_resolver
                .resolve_collected_program_for_api_documentation(hir, module_path.as_deref());
            resolution.tables.merge_from(&unit_resolution.tables, key);
        }
        resolution.rebuild_span_index();
    }
}

fn declaring_package_for_prefetched_path(
    path: &Path,
    assembly: &super::ProgramAssembly,
    entry_project_name: &str,
    dependency_packages: &HashMap<String, String>,
) -> String {
    if path.starts_with(&assembly.roots.host.source_root) {
        return entry_project_name.to_string();
    }
    for dep in &assembly.roots.dependencies {
        if path.starts_with(&dep.source_root)
            && let Some(dep_name) = &dep.dependency_name
            && let Some(project_name) = dependency_packages.get(dep_name)
        {
            return project_name.clone();
        }
    }
    entry_project_name.to_string()
}

fn prefetched_module_path_for_file(
    path: &Path,
    assembly: &super::ProgramAssembly,
) -> Option<Vec<String>> {
    if path.starts_with(&assembly.roots.host.source_root) {
        return module_path_from_file_suffix(path, assembly.has_std_dependency);
    }
    for dep in &assembly.roots.dependencies {
        if path.starts_with(&dep.source_root) {
            if let Some(segments) = module_path_from_file_suffix(path, assembly.has_std_dependency)
            {
                return Some(segments);
            }
            let Ok(rel) = path.strip_prefix(&dep.source_root) else {
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
                return None;
            }
            collapse_homonymous_module_segment(&mut segments);
            if assembly.has_std_dependency {
                let mut with_std = vec!["Std".to_string()];
                with_std.extend(segments);
                return Some(with_std);
            }
            return Some(segments);
        }
    }
    module_path_from_file_suffix(path, assembly.has_std_dependency)
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
        if path.starts_with(&dep.source_root)
            && let Some(dep_name) = &dep.dependency_name
        {
            if let Some(project_name) = dependency_packages.get(dep_name) {
                return project_name.clone();
            }
            return dep_name.clone();
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
    if let Some(module_path) = module_path_from_generated_suffix(path, has_std_dependency) {
        return Some(module_path);
    }
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

fn collect_prefetched_import_closure(
    resolver: &mut Resolver,
    roots: &EffectiveCompilationRoots,
    units: &[SourceUnit],
    unit_paths: &HashSet<PathBuf>,
    dependency_packages: &HashMap<String, String>,
    plan: &CompilePlan,
    prefetched_paths: &mut Vec<PathBuf>,
    prefetched_hir: &mut HashMap<PathBuf, Arc<Spanned<HirProgram>>>,
) {
    if roots.dependencies.is_empty() {
        return;
    }

    let mut queue = VecDeque::new();
    let mut seen_imports = HashSet::new();
    let mut seen_paths = HashSet::new();

    let enqueue_import =
        |queue: &mut VecDeque<String>, seen: &mut HashSet<String>, import: String| {
            if seen.insert(import.clone()) {
                queue.push_back(import);
            }
        };

    let enqueue_module_paths =
        |queue: &mut VecDeque<String>, seen: &mut HashSet<String>, source: &str| {
            for import_path in import_paths_from_source_full(source) {
                enqueue_import(queue, seen, import_path);
            }
            for module_path in module_paths_from_qualified_references(source) {
                enqueue_import(queue, seen, module_path);
            }
        };

    for unit in units {
        enqueue_module_paths(&mut queue, &mut seen_imports, &unit.source);
    }

    while let Some(import_path) = queue.pop_front() {
        if let Some(parent_import) = parent_module_import_path(&import_path) {
            enqueue_import(&mut queue, &mut seen_imports, parent_import);
        }
        let Some(path) = resolve_module_file(&import_path, roots) else {
            continue;
        };
        let normalized = normalize_assembly_path(&path);
        if unit_paths.contains(&normalized) || !seen_paths.insert(normalized) {
            continue;
        }
        if !path_in_dependency_roots(&path, roots) {
            continue;
        }
        let declaring_package = declaring_package_for_dependency_path(
            &path,
            roots,
            dependency_packages,
            &plan.project_name,
        );
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        prefetch_module_at_path(
            resolver,
            &path,
            &source,
            &declaring_package,
            plan.has_std_dependency,
            prefetched_paths,
            prefetched_hir,
            None,
        );
        enqueue_module_paths(&mut queue, &mut seen_imports, &source);
    }
}

fn path_in_dependency_roots(path: &Path, roots: &EffectiveCompilationRoots) -> bool {
    roots.dependencies.iter().any(|dep| {
        path.starts_with(&dep.source_root)
            || path
                .canonicalize()
                .ok()
                .zip(dep.source_root.canonicalize().ok())
                .is_some_and(|(a, b)| a.starts_with(b))
    })
}

fn declaring_package_for_dependency_path(
    path: &Path,
    roots: &EffectiveCompilationRoots,
    dependency_packages: &HashMap<String, String>,
    host_project_name: &str,
) -> String {
    for dep in &roots.dependencies {
        if path.starts_with(&dep.source_root) {
            return dep
                .dependency_name
                .as_ref()
                .and_then(|name| dependency_packages.get(name))
                .cloned()
                .or_else(|| dep.dependency_name.clone())
                .unwrap_or_else(|| host_project_name.to_string());
        }
    }
    host_project_name.to_string()
}

fn collect_prefetched_modules(
    resolver: &mut Resolver,
    source_root: &Path,
    unit_paths: &HashSet<PathBuf>,
    declaring_package: &str,
    has_std_dependency: bool,
    prefetched_paths: &mut Vec<PathBuf>,
    prefetched_hir: &mut HashMap<PathBuf, Arc<Spanned<HirProgram>>>,
) {
    let mut bd_files = Vec::new();
    collect_source_files(source_root, &mut bd_files);
    if let Some(generated_root) = source_root.parent().map(|parent| parent.join(".generated"))
        && generated_root.is_dir()
    {
        collect_source_files(&generated_root, &mut bd_files);
    }
    bd_files.sort();
    for path in bd_files {
        if unit_paths.contains(&normalize_assembly_path(&path)) {
            continue;
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        prefetch_module_at_path(
            resolver,
            &path,
            &source,
            declaring_package,
            has_std_dependency,
            prefetched_paths,
            prefetched_hir,
            Some(source_root),
        );
    }
}

fn prefetch_module_at_path(
    resolver: &mut Resolver,
    path: &Path,
    source: &str,
    declaring_package: &str,
    has_std_dependency: bool,
    prefetched_paths: &mut Vec<PathBuf>,
    prefetched_hir: &mut HashMap<PathBuf, Arc<Spanned<HirProgram>>>,
    source_root: Option<&Path>,
) {
    let logical_name = path.display().to_string();
    let Ok(program) = crate::services::parse_program_with_source_name(&logical_name, source) else {
        return;
    };
    let ast: crate::syntax::Spanned<crate::hir::AstProgram> = program.into();
    let hir = crate::hir::lower_program(&ast);
    let module_path = module_path_from_file_suffix(path, has_std_dependency).or_else(|| {
        let source_root = source_root?;
        let Ok(rel) = path.strip_prefix(source_root) else {
            return None;
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
            return None;
        }
        collapse_homonymous_module_segment(&mut segments);
        if has_std_dependency {
            let mut with_std = vec!["Std".to_string()];
            with_std.extend(segments);
            Some(with_std)
        } else {
            Some(segments)
        }
    });
    resolver.set_declaring_package(declaring_package.to_string());
    if let Some(module_path) = module_path {
        resolver.collect_program_in_module(&hir, &module_path, Some(&path.to_path_buf()));
    } else {
        resolver.set_current_source_path(Some(path.to_path_buf()));
        resolver.collect_program(&hir);
    }
    prefetched_paths.push(normalize_assembly_path(path));
    prefetched_hir.insert(normalize_assembly_path(path), Arc::new(hir));
}

fn normalize_assembly_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn collect_source_files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read_dir) = std::fs::read_dir(root) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, out);
        } else if is_bd_source_file(&path) {
            out.push(path);
        }
    }
}

fn is_bd_source_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("bd")
}

fn module_path_from_file_suffix(path: &Path, has_std_dependency: bool) -> Option<Vec<String>> {
    module_path_from_generated_suffix(path, has_std_dependency)
        .or_else(|| module_path_from_src_suffix(path, has_std_dependency))
}

fn module_path_from_generated_suffix(path: &Path, has_std_dependency: bool) -> Option<Vec<String>> {
    let path_str = path.to_string_lossy();
    let marker = "/.generated/";
    let idx = path_str.find(marker)?;
    let rel = &path_str[idx + marker.len()..];
    let rel_path = Path::new(rel);
    let file_name = rel_path.file_name()?.to_str()?;
    if !file_name.ends_with(".g.bd") {
        return None;
    }
    let module_name = &file_name[..file_name.len().saturating_sub(5)];
    let parent = rel_path.parent()?;
    let mut segments: Vec<String> = parent
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    segments.push(module_name.to_string());
    collapse_homonymous_module_segment(&mut segments);
    if has_std_dependency {
        let mut with_std = vec!["Std".to_string()];
        with_std.extend(segments);
        Some(with_std)
    } else {
        Some(segments)
    }
}

fn module_path_from_src_suffix(
    path: &std::path::Path,
    has_std_dependency: bool,
) -> Option<Vec<String>> {
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

#[cfg(test)]
mod tests {
    use super::module_path_from_generated_suffix;
    use std::path::Path;

    #[test]
    fn generated_file_suffix_keeps_the_complete_module_name() {
        assert_eq!(
            module_path_from_generated_suffix(
                Path::new("/packages/corelib/.generated/Core/Text/Regex/Generated.g.bd"),
                false,
            ),
            Some(vec![
                "Core".to_string(),
                "Text".to_string(),
                "Regex".to_string(),
                "Generated".to_string(),
            ])
        );
    }
}
