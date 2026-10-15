//! Embed the compiler-owned SDK source authority; installed binaries need no source checkout.
use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn regular(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "canonical SDK input must be a regular file"));
    }
    Ok(())
}
fn collect(root: &Path, path: &Path, files: &mut Vec<PathBuf>, depth: usize) -> io::Result<()> {
    if depth > 64 || files.len() > 4096 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "canonical SDK inventory exceeds limits"));
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "canonical SDK inventory contains symlink"));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            collect(root, &entry?.path(), files, depth + 1)?;
        }
    } else if metadata.is_file() {
        if path.extension().is_some_and(|extension| extension == "bd" || extension == "json") {
            if metadata.len() > 16 * 1024 * 1024 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "canonical SDK source exceeds limit"));
            }
            let relative = path.strip_prefix(root).map_err(io::Error::other)?;
            if relative.to_str().is_none() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "canonical SDK path is not UTF-8"));
            }
            files.push(path.to_owned());
        }
    } else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "canonical SDK inventory contains special file"));
    }
    Ok(())
}
pub fn emit(root: &Path, output_dir: &Path) {
    let root = root.canonicalize().expect("canonical SDK package");
    let selector = root.join("corelib_compiler_sdk.bproj");
    regular(&selector).expect("canonical SDK selector");
    let mut files = vec![selector.clone()];
    collect(&root, &root.join("src"), &mut files, 0).expect("canonical SDK source inventory");
    if files.len() > 4096 {
        panic!("canonical SDK inventory exceeds file limit");
    }
    files.sort_by_key(|path| path.strip_prefix(&root).unwrap().to_str().unwrap().replace('\\', "/"));
    let mut output = String::from("static CANONICAL_SDK_SOURCES: &[CanonicalSdkSource] = &[\n");
    for path in files {
        let relative = path.strip_prefix(&root).unwrap().to_str().unwrap().replace('\\', "/");
        output.push_str(&format!(
            "CanonicalSdkSource {{ path: {relative:?}, bytes: include_bytes!({:?}) }},\n",
            path.to_str().unwrap()
        ));
    }
    output.push_str("];\n");
    fs::write(output_dir.join("canonical_sdk_sources.rs"), output).expect("emit canonical SDK inventory");
    println!("cargo:rerun-if-changed={}", selector.display());
    // Cargo directory watches preserve additions/deletions as well as content changes.
    println!("cargo:rerun-if-changed={}", root.join("src").display());
}
