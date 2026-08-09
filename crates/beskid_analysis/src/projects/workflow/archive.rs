use std::fs;
use std::io::Cursor;
use std::path::Path;

use zip::ZipArchive;

use crate::projects::error::ProjectError;

pub(super) fn extract_zip_to_dir(bytes: &[u8], output_dir: &Path) -> Result<(), ProjectError> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader)
        .map_err(|err| ProjectError::Validation(format!("invalid registry artifact ZIP: {err}")))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| ProjectError::Validation(format!("failed to read registry artifact entry: {err}")))?;
        let Some(path) = entry.enclosed_name() else {
            continue;
        };
        let target = output_dir.join(path);
        if entry.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|source| ProjectError::MaterializationCreateDir { path: target, source })?;
            continue;
        }

        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|source| ProjectError::MaterializationCreateDir { path: parent.to_path_buf(), source })?;
        }

        let mut file = fs::File::create(&target).map_err(|source| ProjectError::MaterializationCopy {
            from: output_dir.to_path_buf(),
            to: target.clone(),
            source,
        })?;
        std::io::copy(&mut entry, &mut file).map_err(|source| ProjectError::MaterializationCopy {
            from: output_dir.to_path_buf(),
            to: target,
            source,
        })?;
    }

    Ok(())
}
