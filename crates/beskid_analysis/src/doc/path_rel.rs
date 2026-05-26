//! Artifact-relative paths for packed `api.json` (`source` and `location.file`).

use std::path::{Component, Path};

use crate::projects::assembly::effective_roots_for_plan;
use crate::projects::model::{CompilePlan, PreparedProjectWorkspace};
use crate::projects::{load_manifest_from_path, ResolvedDependencyProject};

use super::api_snapshot::{ApiDocItem, ApiDocRoot};
use super::graph_link::{ApiDocLinkContext, ApiDocPackageRoots};

/// Build link context for workspace doc runs (match roots + artifact path prefixes).
pub fn build_api_doc_link_context(
    plan: &CompilePlan,
    workspace: Option<&PreparedProjectWorkspace>,
) -> Option<ApiDocLinkContext> {
    let manifest = load_manifest_from_path(&plan.manifest_path).ok()?;
    let publishing = manifest.project.name.trim().to_string();
    if publishing.is_empty() {
        return None;
    }

    let effective = effective_roots_for_plan(plan, workspace);
    let mut packages = vec![host_package_roots(
        plan,
        &effective.host.source_root,
        publishing.clone(),
    )];

    for dep in &plan.dependency_projects {
        let match_root = effective
            .dependencies
            .iter()
            .find(|entry| entry.dependency_name.as_deref() == Some(dep.dependency_name.as_str()))
            .map(|entry| entry.source_root.clone())
            .unwrap_or_else(|| dep.source_root.clone());
        packages.push(dependency_package_roots(dep, &match_root));
    }

    Some(ApiDocLinkContext {
        publishing_package: publishing,
        packages,
    })
}

fn host_package_roots(
    plan: &CompilePlan,
    match_root: &Path,
    package: String,
) -> ApiDocPackageRoots {
    ApiDocPackageRoots {
        package,
        match_root: match_root.to_path_buf(),
        artifact_source_prefix: artifact_source_prefix(&plan.project_root, &plan.source_root),
    }
}

fn dependency_package_roots(
    dep: &ResolvedDependencyProject,
    match_root: &Path,
) -> ApiDocPackageRoots {
    let package = load_manifest_from_path(&dep.manifest_path)
        .ok()
        .map(|manifest| manifest.project.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| dep.dependency_name.clone());
    ApiDocPackageRoots {
        package,
        match_root: match_root.to_path_buf(),
        artifact_source_prefix: artifact_source_prefix(&dep.project_root, &dep.source_root),
    }
}

fn artifact_source_prefix(project_root: &Path, source_root: &Path) -> String {
    source_root
        .strip_prefix(project_root)
        .map(forward_slashes_path)
        .unwrap_or_default()
}

/// Rewrite root `source` and every item `location.file` to artifact-relative paths (`.bpk` layout).
///
/// Must run after [`super::assign_declaring_packages`] (which matches absolute `match_root` paths).
pub fn relativize_api_doc_paths(
    root: &mut ApiDocRoot,
    ctx: Option<&ApiDocLinkContext>,
) -> Result<(), String> {
    if let Some(ctx) = ctx {
        let publishing = ctx
            .packages
            .iter()
            .find(|pkg| pkg.package == ctx.publishing_package)
            .ok_or_else(|| {
                format!(
                    "api.json link context missing publishing package {:?}",
                    ctx.publishing_package
                )
            })?;
        root.source = to_artifact_path(publishing, &root.source)?;
        for item in &mut root.items {
            let package = resolve_package_for_item(ctx, item);
            item.location.file = to_artifact_path(package, &item.location.file)?;
        }
        return Ok(());
    }

    root.source = relativize_without_context(&root.source)?;
    for item in &mut root.items {
        item.location.file = relativize_without_context(&item.location.file)?;
    }
    Ok(())
}

fn resolve_package_for_item<'a>(
    ctx: &'a ApiDocLinkContext,
    item: &ApiDocItem,
) -> &'a ApiDocPackageRoots {
    if let Some(declaring) = item.declaring_package.as_deref() {
        if let Some(pkg) = ctx
            .packages
            .iter()
            .find(|pkg| pkg.package == declaring)
        {
            return pkg;
        }
    }
    ctx.packages
        .iter()
        .find(|pkg| pkg.package == ctx.publishing_package)
        .expect("publishing package must exist in link context")
}

fn to_artifact_path(package: &ApiDocPackageRoots, path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("api.json path must not be empty".to_string());
    }

    if !path_looks_absolute(trimmed) {
        return Ok(forward_slashes(trimmed));
    }

    let rel = strip_match_root(trimmed, &package.match_root).ok_or_else(|| {
        format!(
            "could not relativize {:?} against package {} (expected under {})",
            trimmed,
            package.package,
            package.match_root.display()
        )
    })?;
    Ok(join_artifact_prefix(&package.artifact_source_prefix, &rel))
}

fn strip_match_root(path: &str, match_root: &Path) -> Option<String> {
    Path::new(path)
        .strip_prefix(match_root)
        .ok()
        .map(|rel| forward_slashes_path(rel))
}

fn join_artifact_prefix(prefix: &str, relative: &str) -> String {
    let relative = relative.trim_start_matches("./");
    if prefix.is_empty() {
        relative.to_string()
    } else if relative.is_empty() || relative == "." {
        prefix.trim_end_matches('/').to_string()
    } else {
        format!("{}/{}", prefix.trim_end_matches('/'), relative)
    }
}

fn relativize_without_context(path: &str) -> Result<String, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("api.json path must not be empty".to_string());
    }
    if path_looks_absolute(trimmed) {
        Path::new(trimmed)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| format!("could not relativize absolute path {trimmed:?}"))
    } else {
        Ok(forward_slashes(trimmed))
    }
}

/// Returns true when `path` must not appear in packed `api.json` (not artifact-relative).
pub fn path_looks_absolute(path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return false;
    }
    if Path::new(trimmed).is_absolute() || trimmed.starts_with("\\\\") {
        return true;
    }
    if trimmed.starts_with('/') {
        return true;
    }
    let mut chars = trimmed.chars();
    if let Some(first) = chars.next() {
        if first.is_ascii_alphabetic() {
            if matches!(chars.next(), Some(':')) {
                return true;
            }
        }
    }
    false
}

fn forward_slashes(path: &str) -> String {
    forward_slashes_path(Path::new(path))
}

fn forward_slashes_path(path: &Path) -> String {
    let mut out = String::new();
    for (i, component) in path.components().enumerate() {
        match component {
            Component::Normal(seg) => {
                if i > 0 {
                    out.push('/');
                }
                out.push_str(&seg.to_string_lossy());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if i > 0 {
                    out.push('/');
                }
                out.push_str("..");
            }
            _ => {}
        }
    }
    if out.is_empty() {
        ".".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::doc::api_snapshot::{ApiDocItem, ApiLocation};
    use crate::doc::graph_link::{ApiDocLinkContext, ApiDocPackageRoots};

    fn ctx(host_match: &str, host_prefix: &str, dep: Option<(&str, &str, &str)>) -> ApiDocLinkContext {
        let mut packages = vec![ApiDocPackageRoots {
            package: "host_pkg".into(),
            match_root: PathBuf::from(host_match),
            artifact_source_prefix: host_prefix.into(),
        }];
        if let Some((match_root, prefix, name)) = dep {
            packages.push(ApiDocPackageRoots {
                package: name.into(),
                match_root: PathBuf::from(match_root),
                artifact_source_prefix: prefix.into(),
            });
        }
        ApiDocLinkContext {
            publishing_package: "host_pkg".into(),
            packages,
        }
    }

    #[test]
    fn relativizes_to_artifact_paths_with_source_prefix() {
        let host_match = "/work/pkg/obj/beskid/root/src";
        let dep_match = "/work/pkg/obj/beskid/deps/lib/src";
        let link = ctx(host_match, "src", Some((dep_match, "src", "lib_pkg")));

        let mut root = ApiDocRoot {
            schema_version: 4,
            navigation_model: Some("graph-v1".into()),
            generator: "test".into(),
            source: format!("{host_match}/Main.bd"),
            items: vec![ApiDocItem {
                id: Some(1),
                qualified_name: "T".into(),
                name: "T".into(),
                kind: "type".into(),
                visibility: None,
                location: ApiLocation {
                    file: format!("{dep_match}/Lib.bd"),
                    start_line: 1,
                    start_column: 1,
                    end_line: 1,
                    end_column: 1,
                },
                parent_id: None,
                member_ids: vec![],
                display_name: None,
                module_path: vec![],
                signature: None,
                field_type: None,
                return_type: None,
                parameters: vec![],
                generic_parameters: vec![],
                doc_markdown: None,
                doc: None,
                declaring_package: Some("lib_pkg".into()),
                controls: vec![],
                tier: None,
            }],
        };

        relativize_api_doc_paths(&mut root, Some(&link)).expect("relativize");

        assert_eq!(root.source, "src/Main.bd");
        assert_eq!(root.items[0].location.file, "src/Lib.bd");
        assert!(!path_looks_absolute(&root.source));
    }

    #[test]
    fn fails_when_absolute_path_does_not_match_any_root() {
        let link = ctx("/work/host/src", "src", None);
        let mut root = ApiDocRoot {
            schema_version: 4,
            navigation_model: None,
            generator: "test".into(),
            source: "/other/place/Main.bd".into(),
            items: vec![],
        };
        let err = relativize_api_doc_paths(&mut root, Some(&link)).expect_err("expected error");
        assert!(err.contains("could not relativize"));
    }

    #[test]
    fn path_looks_absolute_detects_unix_and_drive_paths() {
        assert!(path_looks_absolute("/tmp/x.bd"));
        assert!(path_looks_absolute("C:\\tmp\\x.bd"));
        assert!(path_looks_absolute("D:foo.bd"));
        assert!(!path_looks_absolute("src/Main.bd"));
    }
}
