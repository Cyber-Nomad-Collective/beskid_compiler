//! Library builds must lower the module without selecting an application entrypoint.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct LibraryCase {
    root: PathBuf,
}

impl LibraryCase {
    fn new(source: &str) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid_library_target_{}_{nonce}", std::process::id()));
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

#[test]
fn type_only_library_builds_without_main() {
    let case = LibraryCase::new("pub type Placeholder { i32 value; }\n");
    let (output, artifact) = case.build(None);
    assert!(output.status.success(), "library build failed: {}", text(&output));
    assert!(artifact.is_file(), "library artifact was not emitted: {}", text(&output));
    assert!(fs::metadata(artifact).expect("library metadata").len() > 0);
}

#[test]
fn library_functions_build_without_main_for_all_non_executable_outputs() {
    for kind in [None, Some("shared"), Some("static"), Some("object")] {
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
        {
            let symbols =
                Command::new("nm").args(["-g", "-j"]).arg(&artifact).output().expect("inspect library symbols");
            assert!(symbols.status.success(), "nm failed: {}", text(&symbols));
            assert!(
                String::from_utf8_lossy(&symbols.stdout)
                    .lines()
                    .any(|symbol| symbol.trim_start_matches('_') == "library_answer"),
                "{kind:?} library did not preserve its declared export: {}",
                text(&symbols),
            );
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
