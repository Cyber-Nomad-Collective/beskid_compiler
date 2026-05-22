//! Git template source cache (`templates/git/<hash>/`).

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::cache::beskid_config_root;
use crate::error::{TemplateError, TemplateResult};

#[derive(Debug, Clone)]
pub struct GitTemplateRef {
    pub url: String,
    pub git_ref: Option<String>,
    pub subpath: Option<String>,
}

pub fn git_cache_dir(key: &str) -> PathBuf {
    beskid_config_root().join("templates").join("git").join(key)
}

pub fn clone_or_update(
    spec: &GitTemplateRef,
    install: bool,
) -> TemplateResult<PathBuf> {
    let key = cache_key(spec);
    let dest = git_cache_dir(&key);
    if dest.join(".beskid").join("template.json").is_file() && !install {
        return Ok(resolve_subpath(&dest, spec.subpath.as_deref()));
    }

    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(TemplateError::Io)?;
    }
    std::fs::create_dir_all(&dest).map_err(TemplateError::Io)?;

    let mut cmd = Command::new("git");
    cmd.args(["clone", "--depth", "1"]);
    if let Some(ref_name) = &spec.git_ref {
        cmd.args(["--branch", ref_name]);
    }
    cmd.arg(&spec.url).arg(&dest);

    let status = cmd.status().map_err(TemplateError::Io)?;
    if !status.success() {
        return Err(TemplateError::GitSource(format!(
            "git clone failed for {}",
            spec.url
        )));
    }

    Ok(resolve_subpath(&dest, spec.subpath.as_deref()))
}

fn resolve_subpath(root: &Path, subpath: Option<&str>) -> PathBuf {
    match subpath.filter(|s| !s.is_empty()) {
        Some(sub) => root.join(sub),
        None => root.to_path_buf(),
    }
}

fn cache_key(spec: &GitTemplateRef) -> String {
    let mut hasher = Sha256::new();
    hasher.update(spec.url.as_bytes());
    if let Some(r) = &spec.git_ref {
        hasher.update(r.as_bytes());
    }
    if let Some(p) = &spec.subpath {
        hasher.update(p.as_bytes());
    }
    format!("{:x}", hasher.finalize())[..16].to_string()
}
