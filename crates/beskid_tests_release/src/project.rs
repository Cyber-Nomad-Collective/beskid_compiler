//! Temporary projects written below a private home; every path stays inside that home.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Manifest of an application project with the bundled layout (`Src/Main.bd`).
pub fn app_manifest(name: &str) -> String {
    format!("{name} {{\n  name = \"{name}\"\n  version = \"0.1.0\"\n}}\n\ntarget \"App\" {{\n  kind = \"App\"\n  entry = \"Main.bd\"\n}}\n")
}

/// Manifest of a library project whose single module is `<name>.bd`.
pub fn lib_manifest(name: &str) -> String {
    format!("{name} {{ name = \"{name}\" version = \"0.1.0\" }}\ntarget \"{name}\" {{ kind = \"Lib\" entry = \"{name}.bd\" }}\n")
}

/// Write an application project at `root/<name>` returning its manifest path.
pub fn write_app(root: &Path, name: &str, main: &str) -> PathBuf {
    let project = root.join(name);
    std::fs::create_dir_all(project.join("Src")).expect("project source root");
    std::fs::write(project.join("Src/Main.bd"), main).expect("Main.bd");
    let manifest = project.join(format!("{name}.bproj"));
    std::fs::write(&manifest, app_manifest(name)).expect("manifest");
    manifest
}

/// Write a library project at `root/<name>` returning its manifest path.
pub fn write_lib(root: &Path, name: &str, source: &str) -> PathBuf {
    let project = root.join(name);
    std::fs::create_dir_all(project.join("Src")).expect("project source root");
    std::fs::write(project.join("Src").join(format!("{name}.bd")), source).expect("library module");
    let manifest = project.join(format!("{name}.bproj"));
    std::fs::write(&manifest, lib_manifest(name)).expect("manifest");
    manifest
}

/// Write a workspace manifest naming `members` and one registry. Dependency intent without an alias
/// (every `beskid add` registry selection) resolves through the registry named `default`.
pub fn write_workspace(root: &Path, members: &[&str], registry_name: &str, registry_url: &str) -> PathBuf {
    let mut text = String::from("workspace { name = \"Test\" resolver = v1 }\n");
    for member in members {
        text.push_str(&format!("member \"{member}\" {{ path = \"{member}\" }}\n"));
    }
    text.push_str(&format!("registry \"{registry_name}\" {{ url = \"{registry_url}\" }}\n"));
    let path = root.join("Workspace.bws");
    std::fs::write(&path, text).expect("workspace manifest");
    path
}

/// Every file below `root` with its bytes, for byte-for-byte preservation assertions.
pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    walk(root, root, false, &mut out);
    out
}

/// The visible project bytes below `root`: every file except the CLI's private `.beskid` state
/// (transaction guard, journal, package cache), which the dependency transaction contract allows
/// to survive a failed or dry-run operation.
pub fn visible_snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    walk(root, root, true, &mut out);
    out
}

fn walk(root: &Path, at: &Path, skip_private: bool, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
    let Ok(entries) = std::fs::read_dir(at) else { return };
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if skip_private && path.file_name().is_some_and(|name| name == ".beskid") {
            continue;
        }
        if path.is_dir() {
            walk(root, &path, skip_private, out);
        } else {
            out.insert(path.strip_prefix(root).expect("below root").to_owned(), std::fs::read(&path).expect("read file"));
        }
    }
}

/// The `Project.lock` line pinning `name`, if any.
pub fn lock_pin(lock: &str, name: &str) -> Option<String> {
    lock.lines().find(|line| line.starts_with(&format!("- name={name};"))).map(str::to_owned)
}

/// Copy a directory tree (regular files and directories only; symbolic links are rejected).
pub fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("copy destination");
    for entry in std::fs::read_dir(from).expect("copy source") {
        let entry = entry.expect("copy entry");
        let kind = entry.file_type().expect("entry type");
        assert!(!kind.is_symlink(), "installed prefix contains a symbolic link: {}", entry.path().display());
        let destination = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), &destination).expect("copy file");
        }
    }
}
