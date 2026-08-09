use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::super::{SourceUnit, roots::EffectiveCompilationRoots};

pub(super) fn declaring_package_for_prefetched_path(
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

pub(super) fn prefetched_module_path_for_file(path: &Path, assembly: &super::ProgramAssembly) -> Option<Vec<String>> {
    if path.starts_with(&assembly.roots.host.source_root) {
        return module_path_from_file_suffix(path, assembly.has_std_dependency);
    }
    for dep in &assembly.roots.dependencies {
        if path.starts_with(&dep.source_root) {
            if let Some(segments) = module_path_from_file_suffix(path, assembly.has_std_dependency) {
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
pub(super) fn collapse_homonymous_module_segment(segments: &mut Vec<String>) {
    if segments.len() >= 2 {
        let last = segments.len() - 1;
        if segments[last] == segments[last - 1] {
            segments.pop();
        }
    }
}

pub(super) fn declaring_package_for_dependency_path(
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

pub(super) fn module_path_from_file_suffix(path: &Path, has_std_dependency: bool) -> Option<Vec<String>> {
    module_path_from_generated_suffix(path, has_std_dependency)
        .or_else(|| module_path_from_src_suffix(path, has_std_dependency))
}

pub(super) fn module_path_from_generated_suffix(path: &Path, has_std_dependency: bool) -> Option<Vec<String>> {
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

pub(super) fn module_path_from_src_suffix(path: &std::path::Path, has_std_dependency: bool) -> Option<Vec<String>> {
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
