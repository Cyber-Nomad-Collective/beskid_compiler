use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use beskid_analysis::hir::HirProgram;
use beskid_analysis::resolve::Resolution;
use beskid_analysis::services::{FrontEndTypedResult, ResolvedInput};
use beskid_analysis::syntax::Spanned;
use beskid_analysis::types::TypeResult;
use beskid_pipeline::PipelineObserver;

use crate::{CodegenArtifact, RETIRED_HIR_LOWERING_PATH};

/// Fully lowered program: typed HIR plus the Cranelift artifact from [`lower_source`] /
/// [`lower_source_with_pipeline`].
///
/// The HIR-bearing constructors are retired; prefer [`crate::PreparedSyntaxEntrypoint`].
pub struct LoweredProgram {
    pub hir: Spanned<HirProgram>,
    pub resolution: Resolution,
    pub typed: TypeResult,
    pub artifact: CodegenArtifact,
}

static SCRATCH_FILE_ID: AtomicU64 = AtomicU64::new(0);

/// Ensure `source` is readable from disk for assembly discovery (`<memory>` and missing paths).
pub fn materialize_source_path_for_lowering(path: &Path, source: &str) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    let dir = std::env::temp_dir().join("beskid_codegen_scratch");
    std::fs::create_dir_all(&dir)?;
    let id = SCRATCH_FILE_ID.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|name| !name.is_empty() && *name != "<memory>")
        .unwrap_or("main.bd");
    let file = dir.join(format!("{id}_{file_name}"));
    std::fs::write(&file, source)?;
    Ok(file)
}

/// Cranelift linker symbol for a resolved function or test item.
pub fn jit_symbol_for_item(
    resolution: &beskid_analysis::resolve::Resolution,
    item_id: beskid_analysis::resolve::ItemId,
) -> String {
    crate::lowering::function::mangle_item_function(resolution, item_id)
}

/// Parse, optionally run semantic diagnostics, lower to HIR, and codegen to CLIF without pipeline hooks.
///
/// Retired: rejects without entering HIR/`Lowerable` emission. Use
/// [`crate::lower_syntax_assembly_entrypoint`] or [`crate::lower_prepared_syntax_entrypoint`].
pub fn lower_source(path: &Path, source: &str, with_diagnostics: bool) -> Result<LoweredProgram> {
    let _ = (path, source, with_diagnostics);
    anyhow::bail!("{RETIRED_HIR_LOWERING_PATH}")
}

/// Like [`lower_source`], limiting the link plan to a single entry function or test name.
///
/// Retired: rejects without entering HIR/`Lowerable` emission.
pub fn lower_source_for_entrypoint(
    path: &Path,
    source: &str,
    entrypoint: &str,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let _ = (path, source, entrypoint, with_diagnostics, pipeline);
    anyhow::bail!("{RETIRED_HIR_LOWERING_PATH}")
}

/// End-to-end lowering from source via the shared analysis front-end spine.
///
/// Retired: rejects without entering HIR/`Lowerable` emission.
pub fn lower_source_with_pipeline(
    path: &Path,
    source: &str,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let _ = (path, source, with_diagnostics, pipeline);
    anyhow::bail!("{RETIRED_HIR_LOWERING_PATH}")
}

/// Lower using a fully resolved CLI input (includes materialized assembly when available).
///
/// Retired: rejects without entering HIR/`Lowerable` emission.
pub fn lower_resolved_input_with_pipeline(
    resolved: &ResolvedInput,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let _ = (resolved, with_diagnostics, pipeline);
    anyhow::bail!("{RETIRED_HIR_LOWERING_PATH}")
}

/// Lower from an optional prepared front-end, else session cache or full compile.
///
/// Retired: rejects without entering HIR/`Lowerable` emission.
pub fn lower_from_prepared_or_cache(
    resolved: &ResolvedInput,
    front: Option<FrontEndTypedResult>,
    link_entrypoint: Option<&str>,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let _ = (resolved, front, link_entrypoint, with_diagnostics, pipeline);
    anyhow::bail!("{RETIRED_HIR_LOWERING_PATH}")
}

/// Lower a single entry function or test from a resolved project input.
///
/// Retired: rejects without entering HIR/`Lowerable` emission.
pub fn lower_resolved_entrypoint_with_pipeline(
    resolved: &ResolvedInput,
    link_entrypoint: Option<&str>,
    with_diagnostics: bool,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let _ = (resolved, link_entrypoint, with_diagnostics, pipeline);
    anyhow::bail!("{RETIRED_HIR_LOWERING_PATH}")
}

/// Lower a pre-built front-end result to CLIF, optionally linking a single entrypoint.
///
/// Retired: rejects without entering HIR/`Lowerable` emission. Prefer
/// [`crate::lower_prepared_syntax_entrypoint`] with the front-end syntax assembly.
pub fn lower_from_front_end(
    source_name: &str,
    source: &str,
    front: FrontEndTypedResult,
    link_entrypoint: Option<&str>,
    pipeline: Option<&dyn PipelineObserver>,
) -> Result<LoweredProgram> {
    let _ = (source_name, source, front, link_entrypoint, pipeline);
    anyhow::bail!("{RETIRED_HIR_LOWERING_PATH}")
}

/// Serialize every lowered function in `artifact` to textual CLIF, separated by `;; Function:` headers.
pub fn render_clif(artifact: &CodegenArtifact) -> String {
    let mut out = String::new();
    for function in &artifact.functions {
        out.push_str(&format!(";; Function: {}\n", function.name));
        out.push_str(&function.function.to_string());
        out.push('\n');
    }
    out
}
