use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::hir::HirProgram;
use crate::projects::CompilePlan;
use crate::resolve::Resolver;
use crate::syntax::Spanned;

use super::super::{
    SourceUnit,
    discovery::resolve_module_file,
    loader::{import_paths_from_source_full, module_paths_from_qualified_references, parent_module_import_path},
    roots::EffectiveCompilationRoots,
};
use super::path_inference::{
    collapse_homonymous_module_segment, declaring_package_for_dependency_path, module_path_from_file_suffix,
};

pub(super) fn collect_prefetched_import_closure(
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

    let enqueue_import = |queue: &mut VecDeque<String>, seen: &mut HashSet<String>, import: String| {
        if seen.insert(import.clone()) {
            queue.push_back(import);
        }
    };

    let enqueue_module_paths = |queue: &mut VecDeque<String>, seen: &mut HashSet<String>, source: &str| {
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
        let declaring_package =
            declaring_package_for_dependency_path(&path, roots, dependency_packages, &plan.project_name);
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
            || path.canonicalize().ok().zip(dep.source_root.canonicalize().ok()).is_some_and(|(a, b)| a.starts_with(b))
    })
}

pub(super) fn collect_prefetched_modules(
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

pub(super) fn normalize_assembly_path(path: &Path) -> PathBuf {
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
