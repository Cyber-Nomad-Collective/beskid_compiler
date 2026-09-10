use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::CodegenArtifact;
use anyhow::Result;

static SCRATCH_DIRECTORY_ID: AtomicU64 = AtomicU64::new(0);

/// Ensure `source` is readable from an isolated assembly-discovery root
/// (virtual labels such as `<memory>` and `<repl>`, plus missing paths).
pub fn materialize_source_path_for_lowering(path: &Path, source: &str) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_path_buf());
    }
    let scratch_root = std::env::temp_dir().join("beskid_codegen_scratch");
    std::fs::create_dir_all(&scratch_root)?;
    let dir = loop {
        let id = SCRATCH_DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
        let candidate = scratch_root.join(format!("{}-{id}", std::process::id()));
        match std::fs::create_dir(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|name| !name.is_empty() && !(name.starts_with('<') && name.ends_with('>')))
        .unwrap_or("main.bd");
    let file = dir.join(file_name);
    std::fs::write(&file, source)?;
    Ok(file)
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
