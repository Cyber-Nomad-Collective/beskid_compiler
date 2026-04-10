use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR set by Cargo"));
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(override_path) = std::env::var("BESKID_STDLIB_SOURCE") {
        candidates.push(PathBuf::from(override_path));
    }
    candidates.extend([
        manifest_dir.join("../../corelib/standard_library"),
        manifest_dir.join("../../../corelib/standard_library"),
        manifest_dir.join("../../../../corelib/standard_library"),
    ]);

    let stdlib_dir = candidates.into_iter().find(|p| p.is_dir()).unwrap_or_else(|| {
        panic!(
            "beskid_cli: standard library sources not found. Expected `../../corelib/standard_library` \
             (compiler repo with `corelib` submodule) or `../../../corelib/standard_library` \
             (monorepo). Set BESKID_STDLIB_SOURCE to an absolute path to override. \
             Hint: `git submodule update --init --recursive` from the compiler repo root."
        )
    });

    let dest = out_dir.join("embedded_stdlib");
    if dest.exists() {
        std::fs::remove_dir_all(&dest).expect("remove stale embedded_stdlib");
    }
    copy_dir_all(&stdlib_dir, &dest).expect("copy standard library into OUT_DIR");

    register_rerun_if_changed(&stdlib_dir);
    println!("cargo:rerun-if-env-changed=BESKID_STDLIB_SOURCE");
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
