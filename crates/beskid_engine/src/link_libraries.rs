//! Build `AotBuildRequest` linker inputs from extern imports and project `link` metadata.

use std::collections::HashSet;
use beskid_analysis::projects::{load_manifest_from_path, CompilePlan};
use beskid_codegen::CodegenArtifact;

/// Resolved linker inputs for an AOT build.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkLibraryInputs {
    pub external_libraries: Vec<String>,
    pub library_search_paths: Vec<std::path::PathBuf>,
}

/// Collect logical library names and search paths for `artifact`, optionally reading the project
/// manifest `link` block from `plan`.
pub fn link_libraries_for_artifact(
    artifact: &CodegenArtifact,
    plan: Option<&CompilePlan>,
) -> LinkLibraryInputs {
    let mut libraries = Vec::new();
    let mut search_paths = Vec::new();

    if let Some(plan) = plan
        && let Ok(manifest) = load_manifest_from_path(&plan.manifest_path)
        && let Some(link) = manifest.link
    {
        libraries.extend(link.libraries);
        search_paths.extend(link.search_paths.into_iter().map(std::path::PathBuf::from));
    }

    for import in &artifact.extern_imports {
        if let Some(library) = import.library.as_deref() {
            let canon = canonical_logical_name(library);
            if !libraries.iter().any(|name| canonical_logical_name(name) == canon) {
                libraries.push(canon);
            }
        }
    }

    LinkLibraryInputs {
        external_libraries: libraries,
        library_search_paths: search_paths,
    }
}

/// Apply [`LinkLibraryInputs`] to an [`beskid_aot::AotBuildRequest`] (manifest libraries first).
pub fn apply_link_libraries(
    request: &mut beskid_aot::AotBuildRequest,
    inputs: LinkLibraryInputs,
) {
    request.external_libraries = merge_libraries(&request.external_libraries, &inputs.external_libraries);
    request.library_search_paths =
        merge_search_paths(&request.library_search_paths, &inputs.library_search_paths);
}

fn merge_libraries(existing: &[String], extra: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for name in existing.iter().chain(extra.iter()) {
        let canon = canonical_logical_name(name);
        if seen.insert(canon.clone()) {
            out.push(canon);
        }
    }
    out
}

fn merge_search_paths(
    existing: &[std::path::PathBuf],
    extra: &[std::path::PathBuf],
) -> Vec<std::path::PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in existing.iter().chain(extra.iter()) {
        let key = path.display().to_string();
        if seen.insert(key) {
            out.push(path.clone());
        }
    }
    out
}

fn canonical_logical_name(logical: &str) -> String {
    let lower = logical.trim().to_ascii_lowercase();
    let stripped_prefix = lower.strip_prefix("lib").unwrap_or(&lower);
    let stripped_suffix = stripped_prefix
        .strip_suffix(".so")
        .or_else(|| stripped_prefix.strip_suffix(".dylib"))
        .or_else(|| stripped_prefix.strip_suffix(".a"))
        .unwrap_or(stripped_prefix);
    stripped_suffix.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_codegen::ExternImport;

    #[test]
    fn extern_import_libraries_are_discovered() {
        let artifact = CodegenArtifact {
            extern_imports: vec![ExternImport {
                symbol: "getpid".into(),
                abi: Some("C".into()),
                library: Some("libc".into()),
            }],
            ..Default::default()
        };
        let inputs = link_libraries_for_artifact(&artifact, None);
        assert!(inputs.external_libraries.contains(&"c".to_string()));
    }
}
