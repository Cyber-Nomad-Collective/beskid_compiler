//! Small filesystem helpers shared by project-resolution and workspace tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Write a `.bproj` manifest and return its path.
pub(crate) fn write_project_manifest(dir: impl AsRef<Path>, source: &str) -> PathBuf {
    let normalized = normalize_legacy_project_block(source);
    let manifest_path = dir
        .as_ref()
        .join(manifest_file_name_from_source(&normalized, "bproj"));
    fs::write(&manifest_path, &normalized).expect("write project manifest");
    manifest_path
}

/// Map legacy `project { name = "foo" ... }` test fixtures to `foo { ... }`.
pub(crate) fn normalize_legacy_project_block(source: &str) -> String {
    let lines: Vec<String> = source.lines().map(str::to_owned).collect();
    for (index, line) in lines.iter().enumerate() {
        if line.trim() != "project {" {
            continue;
        }
        for name_line in lines.iter().skip(index + 1) {
            let trimmed = name_line.trim();
            let Some(rest) = trimmed.strip_prefix("name = ") else {
                continue;
            };
            let Some(name) = parse_quoted_manifest_string(rest) else {
                continue;
            };
            let mut out = lines.clone();
            out[index] = line.replace("project {", &format!("{name} {{"));
            return out.join("\n");
        }
    }
    source.to_string()
}

fn parse_quoted_manifest_string(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix('"')?
        .split('"')
        .next()
        .filter(|name| !name.is_empty())
}

/// Write a `.bws` workspace manifest and return its path.
pub(crate) fn write_workspace_manifest(dir: impl AsRef<Path>, source: &str) -> PathBuf {
    let manifest_path = dir.as_ref().join(manifest_file_name_from_source(source, "bws"));
    fs::write(&manifest_path, source).expect("write workspace manifest");
    manifest_path
}

fn manifest_file_name_from_source(source: &str, extension: &str) -> String {
    let first_line = source
        .lines()
        .find(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .unwrap_or("test");
    let block = first_line.split('{').next().unwrap_or("test").trim();
    let stem = if block == "project" || block == "workspace" {
        "test"
    } else {
        block
    };
    format!("{stem}.{extension}")
}

/// Unique temp directory under the OS temp dir (PID + time); caller should remove when done.
pub(crate) fn temp_case_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time ok")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "beskid_tests_{prefix}_{}_{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Assert two paths refer to the same file after `canonicalize` (cross-symlink comparisons).
#[track_caller]
pub(crate) fn assert_same_canonical_path(left: impl AsRef<Path>, right: impl AsRef<Path>) {
    assert_eq!(
        left.as_ref()
            .canonicalize()
            .expect("left path canonicalize"),
        right
            .as_ref()
            .canonicalize()
            .expect("right path canonicalize"),
    );
}
