//! Synthetic [`CompilePlan`] for orphan `.bd` files without a project manifest.

use std::path::{Path, PathBuf};

use crate::projects::{CompilePlan, Target, TargetKind};

/// Synthetic sentinel manifest name used for orphan single-file compiles.
const SYNTHETIC_MANIFEST: &str = "__synthetic__.bproj";

/// Minimal host compile plan for a standalone `.bd` source file.
///
/// Single source root (parent of `path`), no dependency projects, default host `App` target
/// with `entry` set to the file basename. This pure constructor is for isolated fixtures and
/// tooling with explicitly supplied source authority; public source-input resolution uses
/// [`standalone_compile_plan_for_source`] to retain the default installed Corelib closure.
pub fn synthetic_compile_plan_for_source(path: &Path) -> CompilePlan {
    let absolute = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let source_root = absolute.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let entry = absolute
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| "__entry__.bd".to_owned());
    let project_root = source_root.clone();

    CompilePlan {
        project_root,
        manifest_path: source_root.join(SYNTHETIC_MANIFEST),
        project_name: "__synthetic__".to_owned(),
        source_root,
        target: Target { name: "main".to_owned(), kind: TargetKind::App, entry: Some(entry) },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_core_dependency: false,
    }
}

/// Resolve standalone source through the canonical in-memory project graph, including the implicit `Core` dependency.
/// The host manifest is an identity descriptor only and is never written to disk.
pub fn standalone_compile_plan_for_source(path: &Path) -> Result<CompilePlan, crate::projects::ProjectError> {
    use crate::projects::{UnresolvedDependencyPolicy, parser::parse_manifest};
    crate::projects::graph::resolver::require_default_corelib_dependency_path()?;
    let source = synthetic_compile_plan_for_source(path);
    let mut manifest = parse_manifest(
        "Standalone { name = \"Standalone\" version = \"0.1.0\" }\ntarget \"main\" { kind = \"App\" entry = \"Main.bd\" }\n",
    )?;
    manifest.project.root = ".".into();
    manifest.targets = vec![source.target];
    let graph = crate::projects::graph::builder::build_project_graph_from_manifest(&source.manifest_path, manifest)?;
    crate::projects::compile_plan::compile_plan_from_graph(graph, Some("main"), UnresolvedDependencyPolicy::Error)
}
