use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub fn resolve_corelib_workspace(manifest_dir: &Path, override_path: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(override_path) = override_path.filter(|path| !path.is_empty()) {
        let path = PathBuf::from(override_path);
        if is_workspace(&path) {
            return Some(path);
        }
        if path.file_name().is_some_and(|name| name == "beskid_corelib") && has_manifest_with_extension(&path, "bproj")
        {
            if let Some(parent) = path.parent().filter(|parent| is_workspace(parent)) {
                return Some(parent.to_path_buf());
            }
        }
    }
    let in_tree = manifest_dir.join("../../corelib");
    is_workspace(&in_tree).then_some(in_tree)
}

fn is_workspace(path: &Path) -> bool {
    has_manifest_with_extension(path, "bws") && path.join("beskid_corelib").is_dir()
}

fn has_manifest_with_extension(path: &Path, extension: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        let path = entry.path();
        path.is_file() && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case(extension))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "beskid-corelib-workspace-source-{}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn workspace(root: &Path) {
        std::fs::create_dir_all(root.join("beskid_corelib")).unwrap();
        std::fs::write(root.join("CoreLib.bws"), "").unwrap();
    }

    #[test]
    fn accepts_valid_external_workspace_when_submodule_is_missing() {
        let temp = Scratch::new();
        let manifest_dir = temp.path().join("checkout/crates/beskid_abi");
        std::fs::create_dir_all(&manifest_dir).unwrap();
        let external = temp.path().join("external");
        workspace(&external);

        assert_eq!(resolve_corelib_workspace(&manifest_dir, Some(external.as_os_str())), Some(external));
    }

    #[test]
    fn rejects_invalid_override_when_submodule_is_missing() {
        let temp = Scratch::new();
        let manifest_dir = temp.path().join("checkout/crates/beskid_abi");
        std::fs::create_dir_all(&manifest_dir).unwrap();
        let invalid = temp.path().join("invalid");
        std::fs::create_dir_all(&invalid).unwrap();
        std::fs::write(invalid.join("CoreLib.bws"), "").unwrap();

        assert_eq!(resolve_corelib_workspace(&manifest_dir, Some(invalid.as_os_str())), None);
        assert_eq!(resolve_corelib_workspace(&manifest_dir, None), None);
    }

    #[test]
    fn accepts_project_directory_override_and_valid_in_tree_fallback() {
        let temp = Scratch::new();
        let manifest_dir = temp.path().join("checkout/crates/beskid_abi");
        std::fs::create_dir_all(&manifest_dir).unwrap();
        let external = temp.path().join("external");
        workspace(&external);
        let project = external.join("beskid_corelib");
        std::fs::write(project.join("corelib.bproj"), "").unwrap();
        assert_eq!(resolve_corelib_workspace(&manifest_dir, Some(project.as_os_str())), Some(external));

        let in_tree = temp.path().join("checkout/corelib");
        workspace(&in_tree);
        let invalid = temp.path().join("invalid");
        let selected = resolve_corelib_workspace(&manifest_dir, Some(invalid.as_os_str())).unwrap();
        assert_eq!(selected.canonicalize().unwrap(), in_tree.canonicalize().unwrap());
    }
}
