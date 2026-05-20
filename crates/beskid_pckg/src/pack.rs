//! `.bpk` source collection and readme injection for registry artifacts.

use std::fs;
use std::io;
use std::path::Path;

use beskid_analysis::projects::{
    PACKAGE_README_ARTIFACT_NAME, discover_readme_for_package_root, is_package_root_readme_entry,
    resolve_readme_file_path,
};
use walkdir::WalkDir;
use zip::result::ZipError;

use crate::PckgError;

pub fn collect_pack_entries(source_root: &Path) -> Result<Vec<(String, Vec<u8>)>, PckgError> {
    let mut entries = collect_pack_entries_from_tree(source_root)?;
    apply_pack_readme(source_root, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

fn collect_pack_entries_from_tree(source_root: &Path) -> Result<Vec<(String, Vec<u8>)>, PckgError> {
    let mut entries = Vec::new();

    for entry in WalkDir::new(source_root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let rel_path = path.strip_prefix(source_root).map_err(io::Error::other)?;
        let rel = normalize_rel_path(rel_path);

        if rel == "checksums.sha256" || rel == "package.json" {
            continue;
        }

        let bytes = fs::read(path)?;
        entries.push((rel, bytes));
    }

    Ok(entries)
}

/// Ensure the packed artifact exposes a root `README.md` entry for pckg when a readme is configured.
pub fn apply_pack_readme(
    source_root: &Path,
    entries: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), PckgError> {
    let manifest = discover_readme_for_package_root(source_root).map_err(|err| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("failed to read project manifest for readme: {err}"),
        body: None,
    })?;

    let Some(relative) = manifest else {
        return Ok(());
    };

    let readme_path = resolve_readme_file_path(source_root, &relative);
    if !readme_path.is_file() {
        return Err(PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!(
                "readme file `{}` does not exist or is not a file",
                readme_path.display()
            ),
            body: None,
        });
    }

    let bytes = fs::read(&readme_path)?;
    let normalized = normalize_rel_path(Path::new(&relative));

    if !is_package_root_readme_entry(&normalized) {
        entries.retain(|(name, _)| !name.eq_ignore_ascii_case(PACKAGE_README_ARTIFACT_NAME));
        entries.push((PACKAGE_README_ARTIFACT_NAME.to_string(), bytes.clone()));
    }

    Ok(())
}

pub fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn zip_to_pckg_error(source: ZipError) -> PckgError {
    PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("zip packaging error: {source}"),
        body: None,
    }
}
