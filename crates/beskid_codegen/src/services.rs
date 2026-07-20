use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use crate::CodegenArtifact;

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
