use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct E2eWorkspace {
    root: TempDir,
}

impl E2eWorkspace {
    pub fn from_fixture(name: &str) -> Self {
        let fixture_root = fixture_root(name);
        assert!(fixture_root.is_dir(), "fixture does not exist: {}", fixture_root.display());

        let tempdir =
            tempfile::Builder::new().prefix(&format!("beskid_e2e_{name}_")).tempdir().expect("create e2e temp dir");

        copy_dir_recursive(&fixture_root, tempdir.path());
        Self { root: tempdir }
    }

    pub fn join(&self, path: impl AsRef<Path>) -> PathBuf {
        self.root.path().join(path)
    }
}

fn fixture_root(name: &str) -> PathBuf {
    crate_root().join("fixtures").join(name)
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn copy_dir_recursive(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create destination directory");
    let entries = fs::read_dir(source).expect("read fixture directory");
    for entry in entries {
        let entry = entry.expect("read fixture entry");
        let src_path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "obj" || name == "Project.lock" || name == ".beskid" {
            continue;
        }
        let dest_path = destination.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dest_path);
        } else {
            fs::copy(&src_path, &dest_path).unwrap_or_else(|error| {
                panic!("copy fixture file {} -> {} failed: {error}", src_path.display(), dest_path.display());
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_copy_excludes_private_dependency_state_and_generated_outputs() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let private = source.path().join("app/.beskid/dependency-transaction");
        fs::create_dir_all(&private).unwrap();
        fs::write(private.join("guard"), b"stale transaction state").unwrap();
        fs::create_dir_all(source.path().join("app/obj")).unwrap();
        fs::write(source.path().join("app/obj/generated.bd"), b"generated").unwrap();
        fs::write(source.path().join("app/Project.lock"), b"stale lock").unwrap();
        fs::create_dir_all(source.path().join("app/Src")).unwrap();
        fs::write(source.path().join("app/App.bproj"), b"source manifest").unwrap();
        fs::write(source.path().join("app/Src/Main.bd"), b"source program").unwrap();

        copy_dir_recursive(source.path(), destination.path());
        assert!(!destination.path().join("app/.beskid").exists());
        assert!(!destination.path().join("app/obj").exists());
        assert!(!destination.path().join("app/Project.lock").exists());
        assert_eq!(fs::read(destination.path().join("app/App.bproj")).unwrap(), b"source manifest");
        assert_eq!(fs::read(destination.path().join("app/Src/Main.bd")).unwrap(), b"source program");
        assert!(private.join("guard").exists(), "copy must preserve source-side state");
    }
}
