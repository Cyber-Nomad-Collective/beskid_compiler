//! Reads [`runtime_manifest.bsol`] and generates ABI / analysis registry Rust sources.

mod codegen;
mod lower;
mod model;

pub use model::ManifestRoot;

use std::fs;
use std::path::{Path, PathBuf};

use bsol::{load_profile, parse_bsol_document, validate};

use lower::lower_runtime_manifest;
use model::ManifestRoot as Manifest;

/// Load and parse the runtime manifest from `path`.
pub fn load_manifest(path: &Path) -> Result<Manifest, String> {
    let text = fs::read_to_string(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let document = parse_bsol_document(&text).map_err(|err| err.to_string())?;
    let profile = load_profile("runtime.v1").map_err(|err| err.to_string())?;
    let validated = validate(&document, &profile).map_err(|err| err.to_string())?;
    lower_runtime_manifest(validated)
}

/// Generate `beskid_host` handler registration table under `out_dir`.
pub fn generate_host(manifest: &Manifest, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("host_handlers.rs"),
        codegen::render_host_handler_table(manifest),
    )
    .map_err(|err| err.to_string())
}

/// Convenience: load manifest and write host handler table.
pub fn generate_host_from_path(manifest_path: &Path, out_dir: &Path) -> Result<(), String> {
    let manifest = load_manifest(manifest_path)?;
    generate_host(&manifest, out_dir)
}

/// Generate `beskid_runtime` dispatch table under `out_dir`.
pub fn generate_runtime(
    manifest: &Manifest,
    out_dir: &Path,
    rust_fallback_handlers: bool,
) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("dispatch_table.rs"),
        codegen::render_runtime_dispatch_table(manifest, rust_fallback_handlers),
    )
    .map_err(|err| err.to_string())
}

/// Generate `beskid_runtime_handlers` language handler table under `out_dir`.
pub fn generate_language(manifest: &Manifest, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("language_handlers.rs"),
        codegen::render_language_handler_table(manifest),
    )
    .map_err(|err| err.to_string())
}

/// Convenience: load manifest and write language handler table.
pub fn generate_language_from_path(manifest_path: &Path, out_dir: &Path) -> Result<(), String> {
    let manifest = load_manifest(manifest_path)?;
    generate_language(&manifest, out_dir)
}

/// Convenience: load manifest and write runtime dispatch table.
pub fn generate_runtime_from_path(
    manifest_path: &Path,
    out_dir: &Path,
    rust_fallback_handlers: bool,
) -> Result<(), String> {
    let manifest = load_manifest(manifest_path)?;
    generate_runtime(&manifest, out_dir, rust_fallback_handlers)
}

/// Generate `beskid_abi` outputs under `out_dir`.
pub fn generate_abi(manifest: &Manifest, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("builtins.rs"),
        codegen::render_abi_builtins(manifest),
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("symbols.rs"),
        codegen::render_abi_symbols(manifest),
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("dispatch_tags.rs"),
        codegen::render_dispatch_tags(manifest),
    )
    .map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("dispatch_lookup.rs"),
        codegen::render_dispatch_lookup(manifest),
    )
    .map_err(|err| err.to_string())?;
    Ok(())
}

/// Generate runtime dispatch router under `out_dir`.
pub fn generate_runtime_dispatch(
    manifest: &Manifest,
    out_dir: &Path,
    rust_fallback_handlers: bool,
) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("dispatch_table.rs"),
        codegen::render_runtime_dispatch_table(manifest, rust_fallback_handlers),
    )
    .map_err(|err| err.to_string())
}

/// Generate JIT kernel registration helper under `out_dir`.
pub fn generate_jit_registration(manifest: &Manifest, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("kernel_registration.rs"),
        codegen::render_jit_kernel_registration(manifest),
    )
    .map_err(|err| err.to_string())
}

/// Generate staticlib link anchor helper under `out_dir`.
pub fn generate_link_anchor(manifest: &Manifest, out_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(out_dir).map_err(|err| err.to_string())?;
    fs::write(
        out_dir.join("link_anchor.rs"),
        codegen::render_link_anchor(manifest),
    )
    .map_err(|err| err.to_string())
}

/// Convenience: load manifest and write JIT kernel registration helper.
pub fn generate_jit_registration_from_path(
    manifest_path: &Path,
    out_dir: &Path,
) -> Result<(), String> {
    let manifest = load_manifest(manifest_path)?;
    generate_jit_registration(&manifest, out_dir)
}

/// Convenience: load manifest and write staticlib link anchor helper.
pub fn generate_link_anchor_from_path(manifest_path: &Path, out_dir: &Path) -> Result<(), String> {
    let manifest = load_manifest(manifest_path)?;
    generate_link_anchor(&manifest, out_dir)
}

/// Generate `define_builtins!` body for `beskid_analysis`.
pub fn generate_analysis(manifest: &Manifest, out_path: &Path) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(out_path, codegen::render_analysis_builtins(manifest)).map_err(|err| err.to_string())
}

/// Convenience: load manifest and write all ABI generated files.
pub fn generate_abi_from_path(manifest_path: &Path, out_dir: &Path) -> Result<(), String> {
    let manifest = load_manifest(manifest_path)?;
    generate_abi(&manifest, out_dir)
}

/// Generate runtime handler metadata for `beskid_analysis`.
pub fn generate_runtime_handlers(manifest: &Manifest, out_path: &Path) -> Result<(), String> {
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    fs::write(out_path, codegen::render_runtime_handler_specs(manifest))
        .map_err(|err| err.to_string())
}

/// Convenience: load manifest and write runtime handler metadata.
pub fn generate_runtime_handlers_from_path(
    manifest_path: &Path,
    out_path: &Path,
) -> Result<(), String> {
    let manifest = load_manifest(manifest_path)?;
    generate_runtime_handlers(&manifest, out_path)
}

/// Convenience: load manifest and write analysis include file.
pub fn generate_analysis_from_path(manifest_path: &Path, out_path: &Path) -> Result<(), String> {
    let manifest = load_manifest(manifest_path)?;
    generate_analysis(&manifest, out_path)
}

/// Default manifest path relative to the compiler workspace root.
pub fn default_manifest_path(manifest_dir: &Path) -> PathBuf {
    manifest_dir.join("../../runtime_manifest.bsol")
}
