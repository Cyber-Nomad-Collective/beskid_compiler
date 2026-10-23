use std::{collections::HashMap, path::Path};

use super::super::{SourceUnit, roots::EffectiveCompilationRoots};

/// Declaring package name for symbols collected from a compilation unit.
pub fn package_for_unit(
    unit: &SourceUnit,
    roots: &EffectiveCompilationRoots,
    host_project_name: &str,
    dependency_packages: &HashMap<String, String>,
) -> String {
    package_for_path(&unit.path, roots, host_project_name, dependency_packages)
}

/// Declaring package name of the source file at `path`: the package whose effective source root
/// holds it, by project name (the host project when no dependency root holds it).
pub fn package_for_path(
    path: &Path,
    roots: &EffectiveCompilationRoots,
    host_project_name: &str,
    dependency_packages: &HashMap<String, String>,
) -> String {
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

/// Logical module path of one source unit, derived only from its location under a source root.
///
/// Every package keeps its native module paths: the host project's modules are unprefixed and
/// Corelib packages declare their own roots (`Core.*`, `Testing.*`, `Concurrency.*`,
/// `Beskid.Compiler.*`). Dependency labels (for example the implicit `Core` dependency) are a
/// separate namespace and never become module path segments.
pub fn infer_logical_module_path(unit: &SourceUnit, roots: &EffectiveCompilationRoots) -> Option<Vec<String>> {
    let path = &unit.path;
    if let Some(module_path) = module_path_from_generated_suffix(path) {
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
        return Some(segments);
    }
    module_path_from_src_suffix(path)
}

/// When `Panel/Panel.bd` is inferred as `[…, Panel, Panel]`, items belong in module `[…, Panel]`.
pub(super) fn collapse_homonymous_module_segment(segments: &mut Vec<String>) {
    if segments.len() >= 2 {
        let last = segments.len() - 1;
        if segments[last] == segments[last - 1] {
            segments.pop();
        }
    }
}

pub(super) fn module_path_from_generated_suffix(path: &Path) -> Option<Vec<String>> {
    let path_str = path.to_string_lossy().replace('\\', "/");
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
    Some(segments)
}

/// Module path of a unit outside every effective root, taken below its package's `src` directory.
///
/// The package source directory is the innermost `src` component: an absolute checkout location
/// such as `/woodpecker/src/github.com/...` or `~/src/app/...` may itself contain `src` components
/// above the package, and they never become module path segments.
pub(super) fn module_path_from_src_suffix(path: &std::path::Path) -> Option<Vec<String>> {
    let path_str = path.to_string_lossy().replace('\\', "/");
    let marker = "/src/";
    let idx = path_str.rfind(marker)?;
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
    Some(segments)
}
