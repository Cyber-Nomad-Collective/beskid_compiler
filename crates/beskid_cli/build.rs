use std::ffi::OsStr;
use std::path::{Path, PathBuf};

const ENV_CORELIB_SOURCE: &str = "BESKID_CORELIB_SOURCE";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by Cargo"));
    let candidates: Vec<PathBuf> = corelib_workspace_candidates(manifest_dir);

    let corelib_workspace_dir = candidates
        .into_iter()
        .find(|p| p.join("Workspace.proj").is_file() && p.join("beskid_corelib").is_dir())
        .unwrap_or_else(|| {
            panic!(
                "beskid_cli: corelib workspace not found. Expected `../../corelib` with \
                 `Workspace.proj` plus `beskid_corelib/` (init the `compiler/corelib` submodule). \
                 Set {} to an absolute path to the **workspace** directory (parent of \
                 `beskid_corelib/`) to override. Hint: `git submodule update --init --recursive` \
                 from the compiler repo root.",
                ENV_CORELIB_SOURCE
            )
        });

    let dest = out_dir.join("embedded_corelib");
    if dest.exists() {
        std::fs::remove_dir_all(&dest).expect("remove stale embedded_corelib");
    }
    copy_corelib_workspace_for_embed(&corelib_workspace_dir, &dest).expect("copy corelib slice");

    register_rerun_if_changed(&corelib_workspace_dir);
    println!("cargo:rerun-if-env-changed={ENV_CORELIB_SOURCE}");
}

fn corelib_workspace_candidates(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(override_path) = std::env::var(ENV_CORELIB_SOURCE)
        && !override_path.trim().is_empty()
    {
        let p = PathBuf::from(&override_path);
        // Allow override to point at either workspace root or legacy beskid_corelib only.
        if p.join("Workspace.proj").is_file() {
            candidates.push(p);
        } else if p.file_name().is_some_and(|n| n == "beskid_corelib")
            && p.join("Project.proj").is_file()
        {
            if let Some(parent) = p.parent() {
                candidates.push(parent.to_path_buf());
            }
            candidates.push(p);
        } else {
            candidates.push(p);
        }
    }
    candidates.push(manifest_dir.join("../../corelib"));
    candidates
}

fn copy_corelib_workspace_for_embed(src_workspace: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    let workspace_proj = src_workspace.join("Workspace.proj");
    if workspace_proj.is_file() {
        std::fs::copy(&workspace_proj, dst.join("Workspace.proj"))?;
    }
    copy_dir_for_embed(&src_workspace.join("packages"), &dst.join("packages"))?;
    copy_dir_for_embed(
        &src_workspace.join("beskid_corelib"),
        &dst.join("beskid_corelib"),
    )?;
    Ok(())
}

fn copy_dir_for_embed(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        if should_skip_embed_copy_component(&name) {
            continue;
        }
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(&name);
        if ty.is_dir() {
            copy_dir_for_embed(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn should_skip_embed_copy_component(name: &OsStr) -> bool {
    matches!(
        name,
        n if n == ".git"
            || n == "obj"
            || n == ".beskid"
            || n == "target"
            || n == ".venv-ci"
            || n == ".nox"
    )
}

fn register_rerun_if_changed(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" || name == ".venv-ci" || name == ".nox" {
            continue;
        }
        if path.is_dir() {
            if name == "beskid_corelib" || name == "packages" {
                register_rerun_if_changed(&path);
            }
        } else if path.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
