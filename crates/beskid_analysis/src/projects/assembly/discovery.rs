//! On-disk Beskid module path resolution against effective source roots.

use std::path::{Path, PathBuf};

use super::roots::EffectiveCompilationRoots;

pub fn module_path_to_relative_path(module_path: &str) -> PathBuf {
    let normalized = module_path.replace("::", ".");
    let segments: Vec<&str> = normalized
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect();
    let mut relative = PathBuf::new();
    for segment in segments {
        relative.push(segment);
    }
    relative
}

fn module_path_lookup_candidates(module_path: &str) -> Vec<String> {
    let mut out = vec![module_path.to_string()];
    if let Some(rest) = module_path.strip_prefix("Std.") {
        out.push(rest.to_string());
    }
    out
}

fn module_file_candidates(relative: &std::path::Path, source_root: &Path) -> Vec<PathBuf> {
    let flat = relative.with_extension("bd");
    let homonymous = relative
        .file_name()
        .map(|name| relative.join(name).with_extension("bd"))
        .unwrap_or_else(|| relative.with_extension("bd"));
    let mod_file = relative.join("mod.bd");
    let mut out = vec![flat, homonymous, mod_file];
    if let Some(generated_root) = source_root.parent().map(|parent| parent.join(".generated")) {
        let generated = generated_root
            .join(relative)
            .with_extension("g.bd");
        out.push(generated);
    }
    out
}

pub fn resolve_module_file(
    module_path: &str,
    roots: &EffectiveCompilationRoots,
) -> Option<PathBuf> {
    let roots_list = module_roots_from_effective(roots);
    for candidate in module_path_lookup_candidates(module_path) {
        let relative = module_path_to_relative_path(&candidate);
        if let Some(path) = roots_list.iter().find_map(|root| {
            module_file_candidates(&relative, root)
                .into_iter()
                .find(|candidate| root.join(candidate).is_file())
                .map(|candidate| root.join(candidate))
        }) {
            return Some(path);
        }
    }
    None
}

pub fn module_path_exists_on_disk(module_path: &str, roots: &[PathBuf]) -> bool {
    if roots.is_empty() {
        return false;
    }
    let relative = module_path_to_relative_path(module_path);
    if relative.as_os_str().is_empty() {
        return false;
    }
    roots.iter().any(|root| {
        module_file_candidates(&relative, root)
            .into_iter()
            .any(|candidate| root.join(candidate).is_file())
    })
}

fn module_roots_from_effective(roots: &EffectiveCompilationRoots) -> Vec<PathBuf> {
    super::roots::module_roots_from_effective(roots)
}
