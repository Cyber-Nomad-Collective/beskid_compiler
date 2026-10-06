//! Library builds must lower the module without selecting an application entrypoint.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

struct LibraryCase {
    root: PathBuf,
}

impl LibraryCase {
    fn new(source: &str) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
        let sequence = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("beskid_library_target_{}_{nonce}_{sequence}", std::process::id()));
        fs::create_dir_all(root.join("Src")).expect("library source directory");
        fs::write(
            root.join("Library.bproj"),
            "Library { name = \"Library\" version = \"0.1.0\" root = \"Src\" }\n\
             target \"lib\" { kind = Lib entry = \"Lib.bd\" }\n",
        )
        .expect("library manifest");
        fs::write(root.join("Src/Lib.bd"), source).expect("library source");
        Self { root }
    }

    fn build(&self, kind: Option<&str>) -> (Output, PathBuf) {
        let artifact = self.root.join(format!("Library.{}", std::env::consts::DLL_EXTENSION));
        let mut command = Command::new(env!("CARGO_BIN_EXE_beskid_cli"));
        command
            .args(["build", "--plain", "--project"])
            .arg(self.root.join("Library.bproj"))
            .arg("--output")
            .arg(&artifact)
            .env("BESKID_CORELIB_ROOT", self.root.join("installed-corelib"))
            .current_dir(&self.root);
        if let Some(kind) = kind {
            command.args(["--kind", kind]);
            if kind == "object" {
                command.arg("--object-output").arg(&artifact);
            }
        }
        (command.output().expect("build library"), artifact)
    }
}

impl Drop for LibraryCase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn text(output: &Output) -> String {
    format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))
}

/// Linked library outputs (shared and static) link the exact installed ABI-v5 runtime kit, which a
/// `target/<profile>` CLI binary does not have. Tests that build them run with `--ignored` against
/// the staged native runtime-kit matrix prefix; object output needs no kit and always runs.
fn require_staged_runtime_prefix() {
    assert!(
        std::env::var_os("BESKID_RUNTIME_PREFIX").is_some_and(|prefix| !prefix.is_empty()),
        "linked library outputs require BESKID_RUNTIME_PREFIX to name the staged native runtime-kit matrix prefix"
    );
}

/// Global symbols of a native artifact, without the platform's leading underscore.
#[cfg(unix)]
fn global_symbols(artifact: &std::path::Path) -> Vec<String> {
    let symbols = Command::new("nm").args(["-g", "-j"]).arg(artifact).output().expect("inspect library symbols");
    assert!(symbols.status.success(), "nm failed: {}", text(&symbols));
    String::from_utf8_lossy(&symbols.stdout).lines().map(|symbol| symbol.trim_start_matches('_').to_string()).collect()
}

#[test]
fn type_only_library_builds_object_without_main() {
    let case = LibraryCase::new("pub type Placeholder { i32 value, }\n");
    let (output, artifact) = case.build(Some("object"));
    assert!(output.status.success(), "library object build failed: {}", text(&output));
    assert!(artifact.is_file(), "library object was not emitted: {}", text(&output));
    assert!(fs::metadata(artifact).expect("library object metadata").len() > 0);
}

#[test]
#[ignore = "requires the staged native runtime-kit matrix prefix"]
fn type_only_library_builds_without_main() {
    require_staged_runtime_prefix();
    let case = LibraryCase::new("pub type Placeholder { i32 value, }\n");
    let (output, artifact) = case.build(None);
    assert!(output.status.success(), "library build failed: {}", text(&output));
    assert!(artifact.is_file(), "library artifact was not emitted: {}", text(&output));
    assert!(fs::metadata(artifact).expect("library metadata").len() > 0);
}

#[test]
fn library_functions_build_object_without_main() {
    let case = LibraryCase::new(
        "i32 Value() { return 41; }\n\
         [Export(Abi:\"C\", Symbol:\"library_answer\")]\n\
         pub i32 Answer() { return Value(); }\n",
    );
    let (output, artifact) = case.build(Some("object"));
    assert!(output.status.success(), "library object build failed: {}", text(&output));
    assert!(artifact.is_file(), "library object was not emitted: {}", text(&output));
    #[cfg(unix)]
    assert!(
        global_symbols(&artifact).iter().any(|symbol| symbol == "library_answer"),
        "library object did not define its declared export",
    );
}

#[test]
#[ignore = "requires the staged native runtime-kit matrix prefix"]
fn library_functions_build_without_main_for_all_non_executable_outputs() {
    require_staged_runtime_prefix();
    for kind in [None, Some("shared"), Some("static")] {
        let case = LibraryCase::new(
            "i32 Value() { return 41; }\n\
             [Export(Abi:\"C\", Symbol:\"library_answer\")]\n\
             pub i32 Answer() { return Value(); }\n",
        );
        let (output, artifact) = case.build(kind);
        assert!(output.status.success(), "{kind:?} library build failed: {}", text(&output));
        assert!(artifact.is_file(), "{kind:?} library artifact was not emitted: {}", text(&output));
        assert!(fs::metadata(&artifact).expect("library metadata").len() > 0);
        #[cfg(unix)]
        assert!(
            global_symbols(&artifact).iter().any(|symbol| symbol == "library_answer"),
            "{kind:?} library did not preserve its declared export",
        );
    }
}

/// User exports are exactly the `[Export]` items: `pub` functions alone export no user symbol.
#[test]
#[ignore = "requires the staged native runtime-kit matrix prefix"]
fn library_without_export_attributes_exports_no_user_symbols() {
    require_staged_runtime_prefix();
    for kind in [None, Some("static")] {
        let case = LibraryCase::new("pub i32 Answer() { return 41; }\npub i32 Other() { return 2; }\n");
        let (output, artifact) = case.build(kind);
        assert!(output.status.success(), "{kind:?} library build failed: {}", text(&output));
        assert!(artifact.is_file(), "{kind:?} library artifact was not emitted: {}", text(&output));
        #[cfg(unix)]
        {
            let listing = global_symbols(&artifact);
            for name in ["Answer", "Other"] {
                assert!(
                    !listing.iter().any(|symbol| symbol.starts_with(name)),
                    "{kind:?} library exported non-[Export] function `{name}`: {listing:?}",
                );
            }
        }
    }
}

#[test]
fn executable_output_still_requires_main() {
    let case = LibraryCase::new("pub i32 Answer() { return 41; }\n");
    let (output, artifact) = case.build(Some("exe"));
    assert!(!output.status.success(), "executable without Main unexpectedly built: {}", text(&output));
    assert!(text(&output).contains("Missing entrypoint `Main`"), "wrong failure: {}", text(&output));
    assert!(!artifact.exists(), "failed executable build emitted an artifact");
}

/// An entry-less Lib target owns every unit of its source root: a library output must carry the
/// `[Export]` declarations of all of them, not only of the first unit.
fn entry_less_library_case() -> LibraryCase {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let sequence = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("beskid_library_entry_less_{}_{nonce}_{sequence}", std::process::id()));
    let case = LibraryCase { root: root.clone() };
    fs::create_dir_all(root.join("Src")).expect("library source directory");
    fs::write(
        root.join("Library.bproj"),
        "Library { name = \"Library\" version = \"0.1.0\" root = \"Src\" }\n\
         target \"lib\" { kind = Lib }\n",
    )
    .expect("library manifest");
    fs::write(
        root.join("Src/Alpha.bd"),
        "[Export(Abi:\"C\", Symbol:\"library_alpha\")]\npub i32 AlphaAnswer() { return 1; }\n",
    )
    .expect("Alpha source");
    fs::write(
        root.join("Src/Beta.bd"),
        "[Export(Abi:\"C\", Symbol:\"library_beta\")]\npub i32 BetaAnswer() { return 2; }\n",
    )
    .expect("Beta source");
    case
}

fn assert_entry_less_library_builds(case: &LibraryCase, kinds: &[Option<&str>]) {
    for &kind in kinds {
        let (output, artifact) = case.build(kind);
        assert!(output.status.success(), "{kind:?} entry-less library build failed: {}", text(&output));
        assert!(artifact.is_file(), "{kind:?} library artifact was not emitted: {}", text(&output));
        #[cfg(unix)]
        {
            let listed = global_symbols(&artifact);
            for export in ["library_alpha", "library_beta"] {
                assert!(
                    listed.iter().any(|symbol| symbol == export),
                    "{kind:?} library is missing `{export}` from its own units: {listed:?}",
                );
            }
        }
    }
}

#[test]
fn entry_less_library_object_defines_declarations_from_every_own_unit() {
    assert_entry_less_library_builds(&entry_less_library_case(), &[Some("object")]);
}

#[test]
#[ignore = "requires the staged native runtime-kit matrix prefix"]
fn entry_less_library_exports_declarations_from_every_own_unit() {
    require_staged_runtime_prefix();
    assert_entry_less_library_builds(&entry_less_library_case(), &[None, Some("static")]);
}
