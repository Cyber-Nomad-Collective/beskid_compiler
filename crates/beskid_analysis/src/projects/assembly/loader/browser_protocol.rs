use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use beskid_abi::runtime_source::browser_protocol::{SOURCES, Source};
use beskid_abi::runtime_source::corelib_source_locations_match;

use super::super::EffectiveCompilationRoots;
use crate::projects::CompilePlan;

/// Select source variants only for the dependency and physical source actually
/// resolved by the project's prepared assembly. A local module with the same
/// import spelling does not acquire the browser's Corelib service authority.
pub(super) fn sources(plan: &CompilePlan, roots: &EffectiveCompilationRoots) -> BTreeMap<PathBuf, &'static Source> {
    let mut selected = BTreeMap::new();
    for source in SOURCES {
        for dependency in &plan.dependency_projects {
            let Ok(project) = dependency.project_root.canonicalize() else { continue };
            if project.file_name() != Some(source.package.as_ref())
                || project.parent().and_then(Path::file_name) != Some("packages".as_ref())
                || dependency.project_name != format!("corelib_{}", source.package)
                || dependency.source_root.canonicalize().ok().as_deref() != Some(&project.join("src"))
                || !exact_file(&dependency.source_root.join(source.logical_path), source.native)
            {
                continue;
            }
            if let Some(root) = roots
                .dependencies
                .iter()
                .find(|root| root.dependency_name.as_deref() == Some(dependency.dependency_name.as_str()))
            {
                let effective = root.source_root.join(source.logical_path);
                if exact_file(&effective, source.native) {
                    selected.insert(effective, source);
                }
            }
        }
    }
    selected
}

fn exact_file(path: &Path, expected: &str) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
        && std::fs::read_to_string(path).is_ok_and(|source| source == expected)
}

pub(super) fn selected_source(variants: &BTreeMap<PathBuf, &'static Source>, path: &Path) -> Option<&'static str> {
    variants.iter().find_map(|(candidate, source)| corelib_source_locations_match(candidate, path).then_some(source.browser))
}
