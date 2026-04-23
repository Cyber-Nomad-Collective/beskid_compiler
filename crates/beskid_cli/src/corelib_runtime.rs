use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use include_dir::{Dir, include_dir};
use semver::Version;

// Populated by build.rs from ../../corelib/beskid_corelib.
static EMBEDDED_CORELIB: Dir<'_> = include_dir!("$OUT_DIR/embedded_corelib");

pub struct CorelibProvisioning {
    pub root: PathBuf,
    pub version: String,
    pub updated: bool,
}

pub fn ensure_bundled_corelib() -> Result<CorelibProvisioning> {
    let target_root = corelib_install_root()?;
    let bundled_version = embedded_version()?;
    let installed_version = installed_version(&target_root)?;

    let should_install = match installed_version.as_ref() {
        Some(version) => bundled_version > *version,
        None => true,
    };

    if should_install {
        if target_root.exists() {
            remove_dir_all_retry(&target_root)
                .with_context(|| format!("remove old corelib at {}", target_root.display()))?;
        }
        fs::create_dir_all(&target_root)
            .with_context(|| format!("create corelib root {}", target_root.display()))?;
        write_embedded_dir(&EMBEDDED_CORELIB, &target_root)?;
    } else {
        fs::create_dir_all(&target_root)
            .with_context(|| format!("create corelib root {}", target_root.display()))?;
    }

    Ok(CorelibProvisioning {
        root: target_root,
        version: bundled_version.to_string(),
        updated: should_install,
    })
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
    if let Ok(explicit) = std::env::var("BESKID_CORELIB_ROOT") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        return Ok(PathBuf::from(home).join(".beskid").join("beskid_corelib"));
    }

    let cwd = std::env::current_dir().context("resolve current working directory")?;
    Ok(cwd.join(".beskid").join("beskid_corelib"))
}

fn embedded_version() -> Result<Version> {
    if let Some(file) = EMBEDDED_CORELIB.get_file("package.json") {
        return parse_package_json_version(
            file.contents_utf8().unwrap_or_default(),
            "embedded package.json",
        );
    }

    let project = EMBEDDED_CORELIB
        .get_file("Project.proj")
        .ok_or_else(|| anyhow::anyhow!("embedded corelib is missing Project.proj"))?;
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
                "read installed corelib package file {}",
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
                "read installed corelib project manifest {}",
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
    let raw = parse_project_field(content, "version")
        .ok_or_else(|| anyhow::anyhow!("missing version in {source}"))?;

    Version::parse(&raw).with_context(|| format!("invalid semver `{raw}` in {source}"))
}

fn parse_project_field(content: &str, key: &str) -> Option<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find_map(|line| {
            let (line_key, value) = line.split_once('=')?;
            if line_key.trim() != key {
                return None;
            }
            Some(value.trim().trim_matches('"').to_string())
        })
}

fn write_embedded_dir(source: &Dir<'_>, destination: &Path) -> Result<()> {
    for file in source.files() {
        let rel = file.path();
        let target = destination.join(rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create corelib directory {}", parent.display()))?;
        }
        fs::write(&target, file.contents())
            .with_context(|| format!("write embedded corelib file {}", target.display()))?;
    }

    for dir in source.dirs() {
        let target = destination.join(dir.path());
        fs::create_dir_all(&target)
            .with_context(|| format!("create corelib directory {}", target.display()))?;
        write_embedded_dir(dir, destination)?;
    }

    Ok(())
}
