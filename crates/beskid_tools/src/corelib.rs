//! Install or refresh the bundled Beskid corelib snapshot in the resolved toolchain setup.

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

/// Ensure the embedded corelib template is materialized when newer than any existing install.
pub fn ensure_bundled_corelib() -> Result<CorelibProvisioning> {
    let target_root = corelib_install_root()?;
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
    } else {
        fs::create_dir_all(&target_root).with_context(|| format!("create corelib root {}", target_root.display()))?;
    }

    Ok(CorelibProvisioning { root: target_root, version: bundled_version.to_string(), updated: should_install })
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
    use super::EMBEDDED_CORELIB;
    use semver::Version;

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
}
