//! Synthetic [`CompilePlan`] for orphan `.bd` files without a project manifest.

use std::path::{Path, PathBuf};

use crate::projects::{CompilePlan, Target, TargetKind};

/// Synthetic sentinel manifest name used for orphan single-file compiles.
const SYNTHETIC_MANIFEST: &str = "__synthetic__.bproj";

/// Minimal host compile plan for a standalone `.bd` source file.
///
/// Single source root (parent of `path`), no dependency projects, default host `App` target
/// with `entry` set to the file basename. Used by codegen and CLI paths that must prepare
/// through the shared spine when no `.bproj` was discovered.
pub fn synthetic_compile_plan_for_source(path: &Path) -> CompilePlan {
    let absolute = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let source_root = absolute
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
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
        target: Target {
            name: "main".to_owned(),
            kind: TargetKind::App,
            entry: Some(entry),
        },
        dependency_projects: Vec::new(),
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    }
}
