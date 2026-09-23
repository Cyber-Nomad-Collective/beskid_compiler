//! Install or refresh the bundled Beskid corelib snapshot in the resolved toolchain setup.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use beskid_abi::runtime_kit::installed_corelib_root;
use include_dir::{Dir, include_dir};
use semver::Version;

#[path = "../corelib_fingerprint.rs"]
mod corelib_fingerprint;

use corelib_fingerprint::{BUNDLE_FINGERPRINT_FILE, fingerprint_dir};

// Populated by build.rs from ../../corelib (workspace: *.bws + packages + beskid_corelib).
static EMBEDDED_CORELIB: Dir<'_> = include_dir!("$OUT_DIR/embedded_corelib");

/// Outcome of [`ensure_bundled_corelib`]: install root, embedded version string, and whether files were refreshed.
pub struct CorelibProvisioning {
    pub root: PathBuf,
    pub version: String,
    pub updated: bool,
}

/// `BESKID_CORELIB_ROOT` (or the resolved install root) exists, is non-empty, carries no
/// `.beskid-bundle.sha256` marker, and does not look like a valid corelib package either — so it
/// cannot be identified as either a managed bundle or a developer-provided checkout.
///
/// This is a fail-closed guard: [`ensure_bundled_corelib`] must never delete a directory it cannot
/// prove is a bundle it manages, since that directory may be an uncommitted source checkout.
#[derive(Debug)]
pub struct UnrecognizedCorelibRootError {
    pub path: PathBuf,
}

impl fmt::Display for UnrecognizedCorelibRootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "corelib root `{}` is not empty, has no `{BUNDLE_FINGERPRINT_FILE}` bundle marker, and does not \
             look like a valid corelib package (expected `beskid_corelib/corelib.bproj` or a workspace `.bws` \
             manifest next to a `beskid_corelib` directory). Refusing to delete it in case it is a source \
             checkout with uncommitted changes. Remove this directory yourself, or point BESKID_CORELIB_ROOT \
             at an empty or managed location.",
            self.path.display()
        )
    }
}

impl std::error::Error for UnrecognizedCorelibRootError {}

/// Ensure the embedded corelib template is materialized when newer than any existing install.
///
/// The install root is treated as one of three kinds, and only the first two may ever be replaced:
/// - missing or empty: materialize the embedded bundle into it.
/// - a managed bundle (carries `.beskid-bundle.sha256`): replace it on a fingerprint/version mismatch.
/// - anything else non-empty: never deleted. Used as-is if it looks like a valid corelib checkout
///   (a developer-provided root), otherwise this fails closed with [`UnrecognizedCorelibRootError`].
pub fn ensure_bundled_corelib() -> Result<CorelibProvisioning> {
    let target_root = corelib_install_root()?;

    if is_unmanaged_nonempty_root(&target_root)? {
        if !looks_like_corelib_checkout(&target_root) {
            return Err(UnrecognizedCorelibRootError { path: target_root }.into());
        }
        let version = installed_version(&target_root)?.ok_or_else(|| {
            anyhow::anyhow!("resolve corelib version at developer-provided root {}", target_root.display())
        })?;
        return Ok(CorelibProvisioning { root: target_root, version: version.to_string(), updated: false });
    }

    let bundled_version = embedded_version()?;
    let installed_version = installed_version(&target_root)?;
    let bundled_fingerprint = embedded_fingerprint()?;
    let installed_fingerprint = installed_fingerprint(&target_root)?;

    let should_install = should_install_corelib(
        &bundled_version,
        installed_version.as_ref(),
        &bundled_fingerprint,
        installed_fingerprint.as_deref(),
    );

    if should_install {
        if target_root.exists() {
            remove_dir_all_retry(&target_root)
                .with_context(|| format!("remove old corelib at {}", target_root.display()))?;
        }
        fs::create_dir_all(&target_root).with_context(|| format!("create corelib root {}", target_root.display()))?;
        write_embedded_dir(&EMBEDDED_CORELIB, &target_root)?;
        write_bundle_marker(&target_root, &bundled_fingerprint)?;
    } else {
        fs::create_dir_all(&target_root).with_context(|| format!("create corelib root {}", target_root.display()))?;
    }

    Ok(CorelibProvisioning { root: target_root, version: bundled_version.to_string(), updated: should_install })
}

/// True when `root` exists, has at least one entry, and carries no bundle marker — i.e. it is
/// neither the "materialize into an empty/missing root" case nor the "replace a managed bundle"
/// case, so it must be classified as a developer-provided root (valid checkout, or fail closed).
fn is_unmanaged_nonempty_root(root: &Path) -> Result<bool> {
    if !root.is_dir() {
        return Ok(false);
    }
    let mut entries =
        fs::read_dir(root).with_context(|| format!("read corelib root directory {}", root.display()))?;
    if entries.next().is_none() {
        return Ok(false);
    }
    if root.join(BUNDLE_FINGERPRINT_FILE).is_file() {
        return Ok(false);
    }
    Ok(true)
}

/// Best-effort recognition of a developer-provided corelib checkout: either the aggregate
/// `beskid_corelib/` package directly, or a workspace root (a `.bws` manifest alongside a
/// `beskid_corelib/` directory).
fn looks_like_corelib_checkout(root: &Path) -> bool {
    if root.join("beskid_corelib/corelib.bproj").is_file() {
        return true;
    }
    if !root.join("beskid_corelib").is_dir() {
        return false;
    }
    fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("bws")))
}

/// Write the bundle marker last, after every other file, via a same-directory rename so a reader
/// never observes a marker whose bundle contents are only partially written.
fn write_bundle_marker(destination: &Path, fingerprint: &str) -> Result<()> {
    let marker_path = destination.join(BUNDLE_FINGERPRINT_FILE);
    let tmp_path = destination.join(format!("{BUNDLE_FINGERPRINT_FILE}.tmp"));
    fs::write(&tmp_path, format!("{fingerprint}\n"))
        .with_context(|| format!("write corelib bundle marker {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &marker_path)
        .with_context(|| format!("finalize corelib bundle marker {}", marker_path.display()))?;
    Ok(())
}

fn should_install_corelib(
    bundled_version: &Version,
    installed_version: Option<&Version>,
    bundled_fingerprint: &str,
    installed_fingerprint: Option<&str>,
) -> bool {
    match installed_version {
        None => true,
        Some(version) if bundled_version > version => true,
        Some(version) if bundled_version < version => false,
        Some(_) => installed_fingerprint != Some(bundled_fingerprint),
    }
}

fn embedded_fingerprint() -> Result<String> {
    let file = EMBEDDED_CORELIB
        .get_file(BUNDLE_FINGERPRINT_FILE)
        .ok_or_else(|| anyhow::anyhow!("embedded corelib is missing {BUNDLE_FINGERPRINT_FILE}"))?;
    let fingerprint = file.contents_utf8().unwrap_or_default().trim();
    if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        anyhow::bail!("embedded corelib has an invalid {BUNDLE_FINGERPRINT_FILE}");
    }
    Ok(fingerprint.to_owned())
}

fn installed_fingerprint(root: &Path) -> Result<Option<String>> {
    if !root.is_dir() {
        return Ok(None);
    }
    fingerprint_dir(root).map(Some).with_context(|| format!("fingerprint installed corelib at {}", root.display()))
}

fn remove_dir_all_retry(path: &Path) -> Result<()> {
    let mut last_err: Option<std::io::Error> = None;
    for _ in 0..5 {
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                last_err = Some(err);
                thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(err.into()),
        }
    }

    if let Some(err) = last_err {
        return Err(err.into());
    }
    Ok(())
}

fn corelib_install_root() -> Result<PathBuf> {
    installed_corelib_root().context("resolve installed Beskid corelib root")
}

fn embedded_version() -> Result<Version> {
    if let Some(file) = EMBEDDED_CORELIB.get_file("package.json") {
        return parse_package_json_version(file.contents_utf8().unwrap_or_default(), "embedded package.json");
    }

    let project = EMBEDDED_CORELIB
        .get_file("beskid_corelib/corelib.bproj")
        .ok_or_else(|| anyhow::anyhow!("embedded corelib is missing beskid_corelib/corelib.bproj"))?;
    parse_project_manifest_version(project.contents_utf8().unwrap_or_default(), "embedded corelib.bproj")
}

fn installed_version(root: &Path) -> Result<Option<Version>> {
    for package_path in [root.join("beskid_corelib/package.json"), root.join("package.json")] {
        if package_path.is_file() {
            let content = fs::read_to_string(&package_path)
                .with_context(|| format!("read installed corelib package file {}", package_path.display()))?;
            return Ok(Some(parse_package_json_version(&content, "installed package.json")?));
        }
    }

    for project_path in [root.join("beskid_corelib/corelib.bproj"), root.join("corelib.bproj")] {
        if !project_path.is_file() {
            continue;
        }
        let content = fs::read_to_string(&project_path)
            .with_context(|| format!("read installed corelib project manifest {}", project_path.display()))?;
        return Ok(Some(parse_project_manifest_version(&content, "installed .bproj manifest")?));
    }

    Ok(None)
}

fn parse_package_json_version(content: &str, source: &str) -> Result<Version> {
    let value: serde_json::Value = serde_json::from_str(content).with_context(|| format!("parse JSON for {source}"))?;
    let raw = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing version in {source}"))?;
    Version::parse(raw).with_context(|| format!("invalid semver `{raw}` in {source}"))
}

fn parse_project_manifest_version(content: &str, source: &str) -> Result<Version> {
    let manifest = beskid_analysis::projects::parse_manifest(content)
        .with_context(|| format!("parse project manifest for {source}"))?;
    let raw = manifest.project.version.as_str();
    Version::parse(raw).with_context(|| format!("invalid semver `{raw}` in {source}"))
}

fn write_embedded_dir(source: &Dir<'_>, destination: &Path) -> Result<()> {
    for file in source.files() {
        let rel = file.path();
        if rel == Path::new(BUNDLE_FINGERPRINT_FILE) {
            // Written last, atomically, by `write_bundle_marker` once every other file lands.
            continue;
        }
        let target = destination.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create corelib directory {}", parent.display()))?;
        }
        fs::write(&target, file.contents())
            .with_context(|| format!("write embedded corelib file {}", target.display()))?;
    }

    for dir in source.dirs() {
        let target = destination.join(dir.path());
        fs::create_dir_all(&target).with_context(|| format!("create corelib directory {}", target.display()))?;
        write_embedded_dir(dir, destination)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        BUNDLE_FINGERPRINT_FILE, EMBEDDED_CORELIB, UnrecognizedCorelibRootError, is_unmanaged_nonempty_root,
        looks_like_corelib_checkout, write_bundle_marker,
    };
    use semver::Version;
    use std::fs;

    #[test]
    fn embedded_corelib_carries_its_license_and_notice() {
        let license = EMBEDDED_CORELIB.get_file("LICENSE").expect("embedded corelib Apache license");
        assert!(license.contents_utf8().expect("UTF-8 corelib license").starts_with(
            "                                 Apache License\n                           Version 2.0, January 2004"
        ));

        let notice = EMBEDDED_CORELIB.get_file("NOTICE").expect("embedded corelib notice");
        assert!(notice.contents_utf8().expect("UTF-8 corelib notice").contains("Beskid core library"));
    }

    #[test]
    fn matching_version_with_different_bundle_fingerprint_requires_refresh() {
        let version = Version::new(0, 4, 598);

        assert!(super::should_install_corelib(&version, Some(&version), "current-bundle", Some("stale-bundle"),));
        assert!(super::should_install_corelib(&version, Some(&version), "current-bundle", None));
        assert!(!super::should_install_corelib(&version, Some(&version), "current-bundle", Some("current-bundle"),));
    }

    #[test]
    fn missing_root_is_not_unmanaged() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist");

        assert!(!is_unmanaged_nonempty_root(&missing).expect("classify missing root"));
    }

    #[test]
    fn empty_root_is_not_unmanaged() {
        let dir = tempfile::tempdir().expect("tempdir");

        assert!(!is_unmanaged_nonempty_root(dir.path()).expect("classify empty root"));
    }

    #[test]
    fn managed_bundle_root_is_not_unmanaged() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join(BUNDLE_FINGERPRINT_FILE), "a".repeat(64)).expect("write marker");

        assert!(!is_unmanaged_nonempty_root(dir.path()).expect("classify managed bundle root"));
    }

    #[test]
    fn nonempty_root_without_marker_is_unmanaged() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("README.md"), "hello").expect("write file");

        assert!(is_unmanaged_nonempty_root(dir.path()).expect("classify unmanaged root"));
    }

    #[test]
    fn source_checkout_with_corelib_bproj_is_recognized() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("beskid_corelib")).expect("create beskid_corelib");
        fs::write(dir.path().join("beskid_corelib/corelib.bproj"), "name = \"corelib\"\n").expect("write bproj");

        assert!(looks_like_corelib_checkout(dir.path()));
    }

    #[test]
    fn source_checkout_with_workspace_manifest_is_recognized() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("beskid_corelib")).expect("create beskid_corelib");
        fs::write(dir.path().join("workspace.bws"), "").expect("write workspace manifest");

        assert!(looks_like_corelib_checkout(dir.path()));
    }

    #[test]
    fn unrelated_nonempty_directory_is_not_a_corelib_checkout() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("notes.txt"), "not a corelib").expect("write file");

        assert!(!looks_like_corelib_checkout(dir.path()));
    }

    /// A source checkout (uncommitted edits, no bundle marker) must be classified for the
    /// "use as-is" branch, not the "may be deleted and recreated" branch. This is the guard
    /// against `ensure_bundled_corelib` running `remove_dir_all` on a developer's checkout.
    #[test]
    fn source_checkout_is_never_deleted() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("beskid_corelib")).expect("create beskid_corelib");
        fs::write(dir.path().join("beskid_corelib/corelib.bproj"), "name = \"corelib\"\nversion = \"0.4.1\"\n")
            .expect("write bproj");
        let sentinel = dir.path().join("beskid_corelib/uncommitted_edit.bd");
        fs::write(&sentinel, "// local work in progress").expect("write sentinel file");

        assert!(is_unmanaged_nonempty_root(dir.path()).expect("classify source checkout root"));
        assert!(looks_like_corelib_checkout(dir.path()));

        // Neither classification call may have touched the filesystem: the checkout, including
        // the developer's uncommitted file, is exactly as it was written.
        assert!(sentinel.is_file(), "uncommitted checkout file must survive classification untouched");
    }

    #[test]
    fn unrecognized_root_error_names_the_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("weird-root");
        let error = UnrecognizedCorelibRootError { path: path.clone() };

        let message = error.to_string();
        assert!(message.contains(&path.display().to_string()));
        assert!(message.contains(BUNDLE_FINGERPRINT_FILE));
    }

    #[test]
    fn bundle_marker_is_written_atomically_without_leaving_a_temp_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fingerprint = "b".repeat(64);

        write_bundle_marker(dir.path(), &fingerprint).expect("write marker");

        let marker_path = dir.path().join(BUNDLE_FINGERPRINT_FILE);
        let contents = fs::read_to_string(&marker_path).expect("read marker");
        assert_eq!(contents.trim(), fingerprint);

        let tmp_path = dir.path().join(format!("{BUNDLE_FINGERPRINT_FILE}.tmp"));
        assert!(!tmp_path.exists(), "temp marker file must be renamed away, not left behind");
    }
}
