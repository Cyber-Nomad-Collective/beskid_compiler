use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use include_dir::{Dir, include_dir};
use semver::Version;

// Canonical checked-in stdlib source lives in corelib submodule.
static EMBEDDED_STDLIB: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../../../corelib/standard_library");

pub struct StdlibProvisioning {
    pub root: PathBuf,
    pub version: String,
    pub updated: bool,
}

pub fn ensure_bundled_stdlib() -> Result<StdlibProvisioning> {
    let target_root = stdlib_install_root()?;
    let bundled_version = embedded_version()?;
    let installed_version = installed_version(&target_root)?;

    let should_install = match installed_version.as_ref() {
        Some(version) => bundled_version > *version,
        None => true,
    };

    if should_install {
        if target_root.exists() {
            fs::remove_dir_all(&target_root)
                .with_context(|| format!("remove old stdlib at {}", target_root.display()))?;
        }
        fs::create_dir_all(&target_root)
            .with_context(|| format!("create stdlib root {}", target_root.display()))?;
        write_embedded_dir(&EMBEDDED_STDLIB, &target_root)?;
    } else {
        fs::create_dir_all(&target_root)
            .with_context(|| format!("create stdlib root {}", target_root.display()))?;
    }

    Ok(StdlibProvisioning {
        root: target_root,
        version: bundled_version.to_string(),
        updated: should_install,
    })
}

fn stdlib_install_root() -> Result<PathBuf> {
    if let Ok(explicit) = std::env::var("BESKID_STDLIB_ROOT") {
        let path = PathBuf::from(explicit);
        return Ok(path);
    }

    if let Ok(home) = std::env::var("HOME") {
        return Ok(PathBuf::from(home).join(".beskid").join("standard_library"));
    }

    let cwd = std::env::current_dir().context("resolve current working directory")?;
    Ok(cwd.join(".beskid").join("standard_library"))
}

fn embedded_version() -> Result<Version> {
    if let Some(file) = EMBEDDED_STDLIB.get_file("package.json") {
        return parse_package_json_version(
            file.contents_utf8().unwrap_or_default(),
            "embedded package.json",
        );
    }

    let project = EMBEDDED_STDLIB
        .get_file("Project.proj")
        .ok_or_else(|| anyhow::anyhow!("embedded stdlib is missing Project.proj"))?;
    parse_project_manifest_version(
        project.contents_utf8().unwrap_or_default(),
        "embedded Project.proj",
    )
}

fn installed_version(root: &Path) -> Result<Option<Version>> {
    let package_path = root.join("package.json");
    if package_path.is_file() {
        let content = fs::read_to_string(&package_path).with_context(|| {
            format!(
                "read installed stdlib package file {}",
                package_path.display()
            )
        })?;
        return Ok(Some(parse_package_json_version(
            &content,
            "installed package.json",
        )?));
    }

    let project_path = root.join("Project.proj");
    if project_path.is_file() {
        let content = fs::read_to_string(&project_path).with_context(|| {
            format!(
                "read installed stdlib project manifest {}",
                project_path.display()
            )
        })?;
        return Ok(Some(parse_project_manifest_version(
            &content,
            "installed Project.proj",
        )?));
    }

    Ok(None)
}

fn parse_package_json_version(content: &str, source: &str) -> Result<Version> {
    let value: serde_json::Value =
        serde_json::from_str(content).with_context(|| format!("parse JSON for {source}"))?;
    let raw = value
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing version in {source}"))?;
    Version::parse(raw).with_context(|| format!("invalid semver `{raw}` in {source}"))
}

fn parse_project_manifest_version(content: &str, source: &str) -> Result<Version> {
    let raw = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            if key.trim() != "version" {
                return None;
            }
            Some(value.trim().trim_matches('"').to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("missing version in {source}"))?;

    Version::parse(&raw).with_context(|| format!("invalid semver `{raw}` in {source}"))
}

fn write_embedded_dir(source: &Dir<'_>, destination: &Path) -> Result<()> {
    for file in source.files() {
        let rel = file.path();
        let target = destination.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create stdlib directory {}", parent.display()))?;
        }
        fs::write(&target, file.contents())
            .with_context(|| format!("write embedded stdlib file {}", target.display()))?;
    }

    for dir in source.dirs() {
        let target = destination.join(dir.path());
        fs::create_dir_all(&target)
            .with_context(|| format!("create stdlib directory {}", target.display()))?;
        write_embedded_dir(dir, destination)?;
    }

    Ok(())
}
