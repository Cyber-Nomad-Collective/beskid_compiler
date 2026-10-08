//! Integrity checks for the Corelib snapshot shipped with the compiler.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub const CORELIB_BUNDLE_FINGERPRINT_FILE: &str = ".beskid-bundle.sha256";

/// Return the managed Corelib bundle containing `path` only when its marker matches the current
/// contents of the complete bundle. This is an integrity check: a complete byte-identical bundle
/// with a correctly recomputed marker is intentionally indistinguishable from a managed install.
/// Standalone copied source files have no verified bundle root, and modifying a marked bundle
/// invalidates its marker.
pub fn verified_corelib_bundle_root(path: &Path) -> Option<PathBuf> {
    verified_corelib_bundle_roots(&[path.to_path_buf()]).into_iter().next().flatten()
}

/// Verify several paths while hashing each containing bundle at most once.
pub fn verified_corelib_bundle_roots(paths: &[PathBuf]) -> Vec<Option<PathBuf>> {
    let mut verified = HashMap::new();
    paths
        .iter()
        .map(|path| {
            let root = bundle_root_with_marker(path)?;
            let valid = *verified.entry(root.clone()).or_insert_with(|| is_verified_bundle_root(&root));
            valid.then_some(root)
        })
        .collect()
}

fn bundle_root_with_marker(path: &Path) -> Option<PathBuf> {
    let physical_path = path.canonicalize().ok()?;
    for candidate in path.ancestors() {
        let physical_root = candidate.canonicalize().ok()?;
        if !physical_path.starts_with(&physical_root) {
            continue;
        }
        let marker = physical_root.join(CORELIB_BUNDLE_FINGERPRINT_FILE);
        let Ok(metadata) = std::fs::symlink_metadata(&marker) else {
            continue;
        };
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        return Some(physical_root);
    }
    None
}

fn is_verified_bundle_root(root: &Path) -> bool {
    let marker = root.join(CORELIB_BUNDLE_FINGERPRINT_FILE);
    let Ok(expected) = std::fs::read_to_string(&marker) else {
        return false;
    };
    let expected = expected.trim();
    expected.len() == 64
        && expected.bytes().all(|byte| byte.is_ascii_hexdigit())
        && corelib_bundle_members_present(root)
        && fingerprint_corelib_bundle_dir(root).ok().as_deref() == Some(expected)
}

/// A sealed bundle is only usable when it carries every project its own workspace manifest lists.
/// The fingerprint proves the bytes were not changed; this proves the producer did not omit a member.
fn corelib_bundle_members_present(root: &Path) -> bool {
    corelib_workspace_member_paths(root).is_ok()
}

/// Hash every regular file of a Corelib bundle except build/VCS components and the marker.
///
/// A bundle holds exactly its member inventory ([`corelib_bundle_inventory`]), so for a produced
/// bundle this covers the same files. Hashing everything present (rather than re-deriving the
/// inventory) also seals content that a producer should never have added.
pub fn fingerprint_corelib_bundle_dir(root: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect_fingerprint_files(root, root, &mut files)?;
    files.sort();

    let mut digest = Sha256::new();
    for relative in files {
        let path_bytes = relative.to_string_lossy();
        digest.update((path_bytes.len() as u64).to_le_bytes());
        digest.update(path_bytes.as_bytes());

        let mut file = File::open(root.join(&relative))?;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
    }

    let mut output = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(&mut output, "{byte:02x}").expect("write fingerprint hex");
    }
    Ok(output)
}

fn collect_fingerprint_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let name = entry.file_name();
        if should_skip_component(&name) || name == CORELIB_BUNDLE_FINGERPRINT_FILE {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Corelib bundle contains a symlink: {}", entry.path().display()),
            ));
        }
        let path = entry.path();
        if file_type.is_dir() {
            collect_fingerprint_files(root, &path, files)?;
        } else if file_type.is_file() {
            files.push(path.strip_prefix(root).expect("fingerprint path remains under root").to_path_buf());
        }
    }
    Ok(())
}

/// Build output, VCS state and generated lockfiles never ship in a bundle. The directory names are
/// a superset of the package identity rule (`is_build_or_vcs_directory` in beskid_analysis), so no
/// file that a package source proof ignores can enter the bundle inventory.
pub(crate) fn should_skip_component(name: &OsStr) -> bool {
    matches!(
        name,
        n if n == ".git"
            || n == "Project.lock"
            || n == "obj"
            || n == ".beskid"
            || n == "node_modules"
            || n == "target"
            || n == ".venv-ci"
            || n == ".nox"
    )
}

/// Workspace-root files shipped next to the workspace manifest.
const CORELIB_BUNDLE_ROOT_LEGAL_FILES: [&str; 3] = ["LICENSE", "NOTICE", "LICENSING.md"];

fn invalid_data(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// The single `.bws` workspace manifest at the root of a Corelib workspace or bundle.
pub fn corelib_workspace_manifest(workspace: &Path) -> io::Result<PathBuf> {
    let mut manifests = Vec::new();
    for entry in std::fs::read_dir(workspace)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_file() && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("bws")) {
            manifests.push(path);
        }
    }
    match manifests.len() {
        1 => Ok(manifests.remove(0)),
        0 => Err(invalid_data(format!("Corelib workspace {} has no .bws manifest", workspace.display()))),
        _ => Err(invalid_data(format!("Corelib workspace {} has more than one .bws manifest", workspace.display()))),
    }
}

/// Workspace-relative member directories listed by the Corelib workspace manifest, in manifest
/// order. Every member must be an existing directory that holds a `.bproj` manifest.
pub fn corelib_workspace_member_paths(workspace: &Path) -> io::Result<Vec<PathBuf>> {
    let manifest = corelib_workspace_manifest(workspace)?;
    let source = std::fs::read_to_string(&manifest)?;
    let members = parse_corelib_workspace_member_paths(&source)
        .map_err(|message| invalid_data(format!("{}: {message}", manifest.display())))?;
    let mut paths = Vec::with_capacity(members.len());
    for member in members {
        let relative = PathBuf::from(&member);
        let valid = !member.is_empty()
            && !member.contains('\\')
            && member.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
            && relative.components().all(|component| match component {
                std::path::Component::Normal(name) => !should_skip_component(name),
                _ => false,
            });
        if !valid {
            return Err(invalid_data(format!(
                "{}: member path `{member}` must be a relative path below the workspace",
                manifest.display()
            )));
        }
        let directory = workspace.join(&relative);
        let metadata = std::fs::symlink_metadata(&directory).map_err(|error| {
            io::Error::new(error.kind(), format!("Corelib workspace member `{member}` is missing: {error}"))
        })?;
        if !metadata.file_type().is_dir() || !contains_project_manifest(&directory)? {
            return Err(invalid_data(format!(
                "Corelib workspace member `{member}` is not a project directory with a .bproj manifest"
            )));
        }
        if paths.contains(&relative) {
            return Err(invalid_data(format!("{}: member path `{member}` is listed twice", manifest.display())));
        }
        paths.push(relative);
    }
    Ok(paths)
}

fn contains_project_manifest(directory: &Path) -> io::Result<bool> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|ext| ext == "bproj") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Strictly extract the `path` of every top-level `member` block of a workspace manifest.
///
/// Build scripts cannot use the BSOL workspace parser in `beskid_analysis`, so this reads the small
/// block/assignment subset that workspace manifests use and rejects anything else. Tests check it
/// against the full parser on the checked-in `CoreLib.bws`.
pub fn parse_corelib_workspace_member_paths(source: &str) -> Result<Vec<String>, String> {
    #[derive(Debug, PartialEq)]
    enum Token {
        Word(String),
        Text(String),
        Open,
        Close,
        Equals,
        ListOpen,
        ListClose,
        Comma,
    }

    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((offset, ch)) = chars.next() {
        match ch {
            ch if ch.is_whitespace() => {}
            '/' if chars.peek().is_some_and(|(_, next)| *next == '/') => {
                for (_, next) in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
            }
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '=' => tokens.push(Token::Equals),
            '[' => tokens.push(Token::ListOpen),
            ']' => tokens.push(Token::ListClose),
            ',' => tokens.push(Token::Comma),
            '"' => {
                let mut text = String::new();
                let mut closed = false;
                for (_, next) in chars.by_ref() {
                    match next {
                        '"' => {
                            closed = true;
                            break;
                        }
                        '\\' | '\n' => return Err(format!("unsupported string content at byte {offset}")),
                        other => text.push(other),
                    }
                }
                if !closed {
                    return Err(format!("unterminated string at byte {offset}"));
                }
                tokens.push(Token::Text(text));
            }
            ch if ch.is_ascii_alphanumeric() || ch == '_' => {
                let mut word = String::from(ch);
                while let Some((_, next)) = chars.peek() {
                    if next.is_ascii_alphanumeric() || matches!(next, '_' | '-' | '.') {
                        word.push(*next);
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Word(word));
            }
            other => return Err(format!("unexpected character `{other}` at byte {offset}")),
        }
    }

    struct Cursor {
        tokens: Vec<Token>,
        at: usize,
    }
    impl Cursor {
        fn advance(&mut self) -> Option<&Token> {
            let token = self.tokens.get(self.at);
            self.at += 1;
            token
        }
        fn peek(&self) -> Option<&Token> {
            self.tokens.get(self.at)
        }
        fn expect(&mut self, wanted: Token, context: &str) -> Result<(), String> {
            match self.advance() {
                Some(token) if *token == wanted => Ok(()),
                other => Err(format!("expected {wanted:?} {context}, found {other:?}")),
            }
        }
        fn value(&mut self) -> Result<Option<String>, String> {
            match self.advance() {
                Some(Token::Text(text)) => Ok(Some(text.clone())),
                Some(Token::Word(_)) => Ok(None),
                Some(Token::ListOpen) => {
                    loop {
                        match self.advance() {
                            Some(Token::ListClose) => break,
                            Some(Token::Text(_) | Token::Word(_)) => match self.advance() {
                                Some(Token::Comma) => {}
                                Some(Token::ListClose) => break,
                                other => return Err(format!("expected `,` or `]` in list, found {other:?}")),
                            },
                            other => return Err(format!("unexpected {other:?} in list")),
                        }
                    }
                    Ok(None)
                }
                other => Err(format!("expected a value, found {other:?}")),
            }
        }
        /// Parse block entries after `{`; returns the `path` string assignment when present.
        fn block_body(&mut self, block: &str) -> Result<Option<String>, String> {
            let mut path = None;
            loop {
                match self.advance() {
                    Some(Token::Close) => return Ok(path),
                    Some(Token::Word(key)) => {
                        let key = key.clone();
                        match self.peek() {
                            Some(Token::Equals) => {
                                self.at += 1;
                                let value = self.value()?;
                                if key == "path" {
                                    if path.is_some() {
                                        return Err(format!("`{block}` block assigns `path` twice"));
                                    }
                                    path = Some(value.ok_or_else(|| format!("`{block}` block `path` must be a string"))?);
                                }
                            }
                            Some(Token::Text(_)) => {
                                self.at += 1;
                                self.expect(Token::Open, "after nested block label")?;
                                self.block_body(&key)?;
                            }
                            Some(Token::Open) => {
                                self.at += 1;
                                self.block_body(&key)?;
                            }
                            other => return Err(format!("unexpected {other:?} after `{key}`")),
                        }
                    }
                    other => return Err(format!("unexpected {other:?} in `{block}` block")),
                }
            }
        }
    }

    let mut cursor = Cursor { tokens, at: 0 };
    let mut members = Vec::new();
    while let Some(token) = cursor.advance() {
        let Token::Word(kind) = token else {
            return Err(format!("expected a top-level block, found {token:?}"));
        };
        let kind = kind.clone();
        let label = match cursor.peek() {
            Some(Token::Text(label)) => {
                let label = label.clone();
                cursor.at += 1;
                Some(label)
            }
            _ => None,
        };
        cursor.expect(Token::Open, "to open a top-level block")?;
        let path = cursor.block_body(&kind)?;
        if kind == "member" {
            let label = label.ok_or_else(|| "`member` block needs a name".to_string())?;
            members.push(path.ok_or_else(|| format!("member `{label}` has no `path`"))?);
        }
    }
    Ok(members)
}

/// Exact file inventory of a Corelib bundle produced from `workspace`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorelibBundleInventory {
    /// Workspace-relative files, sorted: the workspace manifest, root legal files, and the package
    /// sources of every workspace member.
    pub files: Vec<PathBuf>,
    /// Workspace-relative directories walked to find `files`, sorted (build-script watch inputs).
    pub directories: Vec<PathBuf>,
}

/// Derive the bundle inventory from the workspace manifest's member list.
///
/// Each member directory is walked with the package boundary rule: build/VCS components are
/// skipped, and a subdirectory that holds its own `.bproj` is a nested project that belongs to the
/// bundle only when it is itself a workspace member (it is then walked from its own entry).
pub fn corelib_bundle_inventory(workspace: &Path) -> io::Result<CorelibBundleInventory> {
    fn visit(
        workspace: &Path,
        directory: &Path,
        files: &mut Vec<PathBuf>,
        directories: &mut Vec<PathBuf>,
    ) -> io::Result<()> {
        directories.push(directory.strip_prefix(workspace).expect("member below workspace").to_path_buf());
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            if should_skip_component(&entry.file_name()) || entry.file_name() == CORELIB_BUNDLE_FINGERPRINT_FILE {
                continue;
            }
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(invalid_data(format!("Corelib workspace contains a symlink: {}", path.display())));
            }
            if file_type.is_dir() {
                if contains_project_manifest(&path)? {
                    continue;
                }
                visit(workspace, &path, files, directories)?;
            } else if file_type.is_file() {
                files.push(path.strip_prefix(workspace).expect("file below workspace").to_path_buf());
            } else {
                return Err(invalid_data(format!("Corelib workspace contains a non-regular entry: {}", path.display())));
            }
        }
        Ok(())
    }

    let manifest = corelib_workspace_manifest(workspace)?;
    let mut files = vec![manifest.strip_prefix(workspace).expect("manifest below workspace").to_path_buf()];
    for name in CORELIB_BUNDLE_ROOT_LEGAL_FILES {
        let path = workspace.join(name);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => files.push(PathBuf::from(name)),
            Ok(_) => return Err(invalid_data(format!("Corelib legal file {} is not a regular file", path.display()))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    let mut directories = Vec::new();
    for member in corelib_workspace_member_paths(workspace)? {
        visit(workspace, &workspace.join(member), &mut files, &mut directories)?;
    }
    files.sort();
    files.dedup();
    directories.sort();
    directories.dedup();
    Ok(CorelibBundleInventory { files, directories })
}

/// Copy the member-derived bundle inventory of `workspace` into `destination` (without a marker).
pub fn copy_corelib_bundle(workspace: &Path, destination: &Path) -> io::Result<CorelibBundleInventory> {
    let inventory = corelib_bundle_inventory(workspace)?;
    std::fs::create_dir_all(destination)?;
    for relative in &inventory.files {
        let target = destination.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(workspace.join(relative), &target)?;
    }
    Ok(inventory)
}

#[cfg(test)]
mod bundle_inventory_tests {
    use std::ffi::OsStr;

    use super::should_skip_component;

    #[test]
    fn generated_project_lockfiles_are_not_embedded_in_release_corelib() {
        assert!(should_skip_component(OsStr::new("Project.lock")));
    }

    use super::{
        CORELIB_BUNDLE_FINGERPRINT_FILE, copy_corelib_bundle, corelib_bundle_inventory, corelib_workspace_member_paths,
        fingerprint_corelib_bundle_dir, parse_corelib_workspace_member_paths, verified_corelib_bundle_root,
    };
    use std::path::{Path, PathBuf};

    fn checkout() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib")
    }

    fn sealed_copy() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().expect("bundle scratch");
        let bundle = temp.path().join("beskid_corelib");
        copy_corelib_bundle(&checkout(), &bundle).expect("copy member-derived Corelib bundle");
        let fingerprint = fingerprint_corelib_bundle_dir(&bundle).expect("fingerprint bundle");
        std::fs::write(bundle.join(CORELIB_BUNDLE_FINGERPRINT_FILE), format!("{fingerprint}\n")).expect("seal");
        (temp, bundle)
    }

    #[test]
    fn checked_in_workspace_lists_both_compiler_mods() {
        let members = corelib_workspace_member_paths(&checkout()).expect("CoreLib.bws members");
        for wanted in ["beskid_corelib", "packages/serialization", "mods/serialization_mod", "mods/corelib_pest_gen"] {
            assert!(members.contains(&PathBuf::from(wanted)), "CoreLib.bws must list `{wanted}`: {members:?}");
        }
    }

    #[test]
    fn bundle_inventory_covers_every_member_and_excludes_nested_non_member_projects() {
        let workspace = checkout();
        let inventory = corelib_bundle_inventory(&workspace).expect("Corelib bundle inventory");
        for member in corelib_workspace_member_paths(&workspace).expect("members") {
            assert!(
                inventory.files.iter().any(|file| file.parent() == Some(member.as_path())
                    && file.extension().is_some_and(|ext| ext == "bproj")),
                "bundle inventory omits the manifest of member {}",
                member.display()
            );
        }
        for wanted in [
            "CoreLib.bws",
            "LICENSE",
            "mods/serialization_mod/serialization_mod.bproj",
            "mods/serialization_mod/Src/Mod.bd",
            "mods/corelib_pest_gen/corelib_pest_gen.bproj",
            "packages/serialization/corelib_serialization.bproj",
        ] {
            assert!(inventory.files.contains(&PathBuf::from(wanted)), "bundle inventory omits {wanted}");
        }
        for file in &inventory.files {
            assert!(!file.starts_with("mods/serialization_mod/Tests"), "nested non-member project shipped: {}", file.display());
            assert!(
                !file.components().any(|component| should_skip_component(component.as_os_str())),
                "build or VCS output shipped: {}",
                file.display()
            );
        }
    }

    #[test]
    fn fingerprint_and_verification_follow_mod_sources() {
        let (_temp, bundle) = sealed_copy();
        assert_eq!(verified_corelib_bundle_root(&bundle.join("packages/serialization")), Some(bundle.canonicalize().unwrap()));
        let before = fingerprint_corelib_bundle_dir(&bundle).expect("fingerprint before");
        let source = bundle.join("mods/serialization_mod/Src/Mod.bd");
        let mut text = std::fs::read_to_string(&source).expect("read mod source");
        text.push_str("\n// changed\n");
        std::fs::write(&source, text).expect("change mod source");
        assert_ne!(fingerprint_corelib_bundle_dir(&bundle).expect("fingerprint after"), before);
        assert_eq!(verified_corelib_bundle_root(&bundle), None, "an edited mod source must break the seal");
    }

    #[test]
    fn resealed_bundle_without_a_member_is_not_verified() {
        let (_temp, bundle) = sealed_copy();
        std::fs::remove_dir_all(bundle.join("mods/serialization_mod")).expect("drop mod member");
        let fingerprint = fingerprint_corelib_bundle_dir(&bundle).expect("refingerprint");
        std::fs::write(bundle.join(CORELIB_BUNDLE_FINGERPRINT_FILE), format!("{fingerprint}\n")).expect("reseal");
        assert_eq!(verified_corelib_bundle_root(&bundle), None);
    }

    #[test]
    fn member_parser_reads_blocks_and_rejects_unexpected_syntax() {
        let source = "workspace { name = fixture }\n// note\nmember \"a\" {\n  path = \"packages/a\"\n  tags = [\"x\", \"y\"]\n  nested { k = v }\n}\n";
        assert_eq!(parse_corelib_workspace_member_paths(source), Ok(vec!["packages/a".to_string()]));
        assert!(parse_corelib_workspace_member_paths("member \"a\" { package = \"a\" }").is_err());
        assert!(parse_corelib_workspace_member_paths("member \"a\" { path = \"a\"\n path = \"b\" }").is_err());
        assert!(parse_corelib_workspace_member_paths("member \"a\" { path = \"a\" ").is_err());
        assert!(parse_corelib_workspace_member_paths("member \"a\" { path = \"a\" } # x").is_err());
    }
}
