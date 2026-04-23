use std::path::{Path, PathBuf};

const ENV_CORELIB_SOURCE: &str = "BESKID_CORELIB_SOURCE";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by Cargo"));
    let candidates: Vec<PathBuf> = corelib_source_candidates(manifest_dir);

    let corelib_dir = candidates
        .into_iter()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| {
            panic!(
                "beskid_cli: corelib sources not found. Expected `../../corelib/beskid_corelib` \
             (the `corelib` Git submodule at the compiler repository root). \
             Set BESKID_CORELIB_SOURCE to an absolute path to override. \
             Hint: `git submodule update --init --recursive` from the compiler repo root."
            )
        });

    let dest = out_dir.join("embedded_corelib");
    if dest.exists() {
        std::fs::remove_dir_all(&dest).expect("remove stale embedded_corelib");
    }
    copy_dir_all(&corelib_dir, &dest).expect("copy corelib into OUT_DIR");

    register_rerun_if_changed(&corelib_dir);
    println!("cargo:rerun-if-env-changed={ENV_CORELIB_SOURCE}");
}

fn corelib_source_candidates(manifest_dir: &Path) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(override_path) = std::env::var(ENV_CORELIB_SOURCE) {
        if !override_path.trim().is_empty() {
            candidates.push(PathBuf::from(override_path));
        }
    }
    candidates.push(manifest_dir.join("../../corelib/beskid_corelib"));
    candidates
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn register_rerun_if_changed(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            register_rerun_if_changed(&path);
        } else {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
