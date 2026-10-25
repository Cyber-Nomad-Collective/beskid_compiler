//! The toolchain's shared, read-only Corelib.
//!
//! Every project prepared by one toolchain reads Corelib packages in place from the sealed Corelib
//! bundle that toolchain resolves (`BESKID_CORELIB_ROOT`, the installed `<prefix>/beskid_corelib`,
//! or the per-user managed root). The bundle is content-addressed by its fingerprint marker; it is
//! verified before use and a modified bundle fails closed. Nothing is copied into a project and
//! nothing is ever written below the shared root.
//!
//! An unsealed root (a developer's Corelib checkout named by `BESKID_CORELIB_ROOT`) is neither
//! read-only nor content-addressed, so it is not shared: its packages stay ordinary path
//! dependencies that preparation copies like any other user-owned tree.

use std::path::{Path, PathBuf};

use beskid_abi::{
    corelib_bundle::{corelib_bundle_marker_root, verified_corelib_bundle_root},
    runtime_kit::installed_corelib_root,
};

use crate::projects::error::ProjectError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SharedCorelib {
    root: PathBuf,
}

impl SharedCorelib {
    /// The shared Corelib of the running toolchain, when a sealed bundle is installed.
    pub(crate) fn resolve() -> Option<Self> {
        Self::from_installed_root(&installed_corelib_root().ok()?)
    }

    /// The shared Corelib whose install root (a bundle holding `beskid_corelib/`, or the
    /// aggregate package itself) is `installed`. The marker locates the bundle; its seal is
    /// checked by [`Self::verify_seal`] before any unit of it is used.
    pub(crate) fn from_installed_root(installed: &Path) -> Option<Self> {
        let installed = installed.canonicalize().ok()?;
        let nested = installed.join("beskid_corelib");
        let aggregate = if nested.is_dir() { nested } else { installed.clone() };
        let marked = corelib_bundle_marker_root(&aggregate)?;
        (marked.starts_with(&installed) || installed.starts_with(&marked)).then_some(Self { root: marked })
    }

    /// Canonical root that portable `source=corelib` lock paths are relative to.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the physical location of `path` lies in the shared Corelib.
    pub(crate) fn contains(&self, path: &Path) -> bool {
        path.canonicalize().is_ok_and(|physical| physical.starts_with(&self.root))
    }

    /// Fail closed when a sealed shared Corelib no longer matches its fingerprint. The verdict is
    /// cached per process by a stat signature of the bundle, so repeated preparation stays cheap.
    pub(crate) fn verify_seal(&self) -> Result<(), ProjectError> {
        if verified_corelib_bundle_root(&self.root).as_deref() == Some(self.root.as_path()) {
            return Ok(());
        }
        Err(ProjectError::SharedCorelibModified { root: self.root.clone() })
    }
}

/// The Corelib package that owns `path` (its manifest's file stem, for example
/// `corelib_console`) when `path` lies in the running toolchain's shared Corelib.
pub fn shared_corelib_package_of(path: &Path) -> Option<String> {
    let shared = SharedCorelib::resolve()?;
    let physical = path.canonicalize().ok()?;
    if !physical.starts_with(shared.root()) {
        return None;
    }
    physical.ancestors().take_while(|ancestor| ancestor.starts_with(shared.root())).find_map(|directory| {
        std::fs::read_dir(directory).ok()?.filter_map(Result::ok).find_map(|entry| {
            let path = entry.path();
            (path.extension().is_some_and(|extension| extension == "bproj") && path.is_file())
                .then(|| path.file_stem()?.to_str().map(str::to_owned))
                .flatten()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::SharedCorelib;
    use beskid_abi::corelib_bundle::{CORELIB_BUNDLE_FINGERPRINT_FILE, fingerprint_corelib_bundle_dir};
    use std::fs;
    use std::path::Path;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// A minimal Corelib workspace: workspace manifest, aggregate and one package.
    fn corelib_workspace(root: &Path) {
        write(
            &root.join("CoreLib.bws"),
            "workspace {\n  name = \"corelib\"\n}\n\nmember \"corelib\" {\n  path = \"beskid_corelib\"\n}\n\nmember \"core_text\" {\n  path = \"packages/core_text\"\n}\n",
        );
        write(&root.join("beskid_corelib/corelib.bproj"), "corelib {\n  name = \"corelib\"\n  version = \"0.6.7\"\n}\n");
        write(&root.join("packages/core_text/core_text.bproj"), "core_text {\n  name = \"core_text\"\n  version = \"0.6.7\"\n}\n");
        write(&root.join("packages/core_text/Src/Text.bd"), "pub i32 Length() { return 0; }\n");
    }

    fn seal(root: &Path) {
        let fingerprint = fingerprint_corelib_bundle_dir(root).unwrap();
        fs::write(root.join(CORELIB_BUNDLE_FINGERPRINT_FILE), format!("{fingerprint}\n")).unwrap();
    }

    #[test]
    fn an_unsealed_developer_root_is_not_shared() {
        let temp = tempfile::tempdir().unwrap();
        corelib_workspace(temp.path());
        assert_eq!(SharedCorelib::from_installed_root(temp.path()), None);

        seal(temp.path());
        let shared = SharedCorelib::from_installed_root(temp.path()).expect("a sealed bundle is shared");
        assert_eq!(shared.root(), temp.path().canonicalize().unwrap());
        assert!(shared.contains(&temp.path().join("packages/core_text")));
        // Naming the aggregate directly resolves the same bundle.
        let aggregate = SharedCorelib::from_installed_root(&temp.path().join("beskid_corelib")).unwrap();
        assert_eq!(aggregate, shared);
    }

    #[test]
    fn sealed_bundle_fails_closed_once_modified() {
        let temp = tempfile::tempdir().unwrap();
        corelib_workspace(temp.path());
        seal(temp.path());
        let shared = SharedCorelib::from_installed_root(temp.path()).unwrap();
        shared.verify_seal().expect("an intact bundle verifies");

        // A different size: the per-process verdict cache keys on size and modification time.
        write(&temp.path().join("packages/core_text/Src/Text.bd"), "pub i32 Length() { return 10; }\n");
        let error = shared.verify_seal().expect_err("an edited bundle must fail closed");
        assert_eq!(error.code(), "E3032");
        assert!(error.to_string().contains("read-only"), "{error}");
    }

    #[test]
    fn two_toolchains_side_by_side_each_have_their_own_shared_corelib() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("beskid-0.6.7-a/beskid_corelib");
        let second = temp.path().join("beskid-0.6.8-b/beskid_corelib");
        for root in [&first, &second] {
            corelib_workspace(root);
            seal(root);
        }
        let a = SharedCorelib::from_installed_root(&first).unwrap();
        let b = SharedCorelib::from_installed_root(&second).unwrap();
        assert_ne!(a.root(), b.root());
        assert!(a.contains(&first.join("packages/core_text")) && !a.contains(&second.join("packages/core_text")));
        // Editing one toolchain's Corelib breaks only that toolchain's seal.
        write(&second.join("packages/core_text/Src/Extra.bd"), "pub i32 Extra() { return 2; }\n");
        a.verify_seal().expect("the untouched toolchain still verifies");
        assert!(b.verify_seal().is_err());
    }
}
