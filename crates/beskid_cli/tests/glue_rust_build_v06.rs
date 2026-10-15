//! `beskid build --backend glue-rust` rejections. Each case must fail before Cargo, rustc or the
//! linker runs, and must write no output. The tool flags name empty placeholder files: a case that
//! reached a tool would fail differently. The positive owned build runs in the interop harness.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GLUE_IMPORT_SOURCE: &str = "[Extern(Abi:\"C\", Library:\"glue_a\")]\n\
    pub contract ForeignA { i32 glue_a_value(i32 value); }\n\
    [Export(Abi:\"C\", Symbol:\"use_glue_a\")]\n\
    pub i32 UseGlueA(i32 value) { return ForeignA.glue_a_value(value); }\n";

struct GlueCase {
    root: tempfile::TempDir,
}

impl GlueCase {
    /// A project `Consumer` whose target has `kind`, with the given `glue` labels and entry source.
    fn new(kind: &str, glue: &[&str], source: &str) -> Self {
        let root = tempfile::Builder::new().prefix("beskid-glue-rust-cli-").tempdir().expect("case root");
        let mut manifest = String::from("Consumer { name = \"Consumer\" version = \"0.1.0\" root = \"Src\" }\n");
        for label in glue {
            manifest.push_str(&format!("glue \"{label}\" {{ backend = rust path = \"owners/{label}\" }}\n"));
            let owner = root.path().join("owners").join(label);
            fs::create_dir_all(&owner).expect("owner directory");
            fs::write(owner.join("implementation.rs"), "pub fn glue_a_value(value: i32) -> i32 { value }\n")
                .expect("owner source");
        }
        manifest.push_str(&format!("target \"GlueConsumer\" {{ kind = {kind} entry = \"Lib.bd\" }}\n"));
        fs::create_dir_all(root.path().join("Src")).expect("source directory");
        fs::write(root.path().join("Consumer.bproj"), manifest).expect("manifest");
        fs::write(root.path().join("Src/Lib.bd"), source).expect("source");
        let tools = root.path().join("tools/bin");
        fs::create_dir_all(&tools).expect("tool directory");
        for tool in ["cargo", "rustc", "cc"] {
            fs::write(tools.join(format!("{tool}{}", std::env::consts::EXE_SUFFIX)), b"").expect("placeholder tool");
        }
        Self { root }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn tool(&self, name: &str) -> PathBuf {
        self.path().join("tools/bin").join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    }

    fn output(&self) -> PathBuf {
        self.path().join("out").join(format!(
            "{}GlueConsumer{}",
            std::env::consts::DLL_PREFIX,
            std::env::consts::DLL_SUFFIX
        ))
    }

    /// `build --plain --project <manifest> --output <out>` plus `extra`.
    fn build(&self, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_beskid_cli"))
            .args(["build", "--plain", "--project"])
            .arg(self.path().join("Consumer.bproj"))
            .arg("--output")
            .arg(self.output())
            .args(extra)
            .env("BESKID_CORELIB_ROOT", self.path().join("installed-corelib"))
            .current_dir(self.path())
            .output()
            .expect("run beskid build")
    }

    /// The canonical explicit tool flags: `--cargo`, `--rustc` and `--linker`.
    fn explicit_tools(&self) -> Vec<String> {
        vec![
            "--cargo".into(),
            self.tool("cargo").display().to_string(),
            "--rustc".into(),
            self.tool("rustc").display().to_string(),
            "--linker".into(),
            self.tool("cc").display().to_string(),
        ]
    }

    fn glue_rust(&self, extra: &[&str]) -> Output {
        let mut args = vec!["--backend".to_string(), "glue-rust".to_string()];
        args.extend(self.explicit_tools());
        args.extend(extra.iter().map(|value| value.to_string()));
        self.build(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }

    /// The command failed, mentioned `expected`, and left no output image or staging directory.
    fn assert_rejected(&self, output: &Output, expected: &str) {
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(!output.status.success(), "build unexpectedly succeeded: {text}");
        assert!(text.contains(expected), "expected `{expected}` in: {text}");
        assert!(!self.output().exists(), "rejected build wrote {}", self.output().display());
        if let Ok(entries) = fs::read_dir(self.path().join("out")) {
            let left = entries.map(|entry| entry.expect("out entry").file_name()).collect::<Vec<_>>();
            assert!(left.is_empty(), "rejected build left output entries: {left:?}");
        }
    }
}

#[test]
fn glue_dotnet_is_unavailable() {
    let case = GlueCase::new("Lib", &["glue_a"], GLUE_IMPORT_SOURCE);
    let output = case.build(&["--backend", "glue-dotnet"]);
    case.assert_rejected(&output, "backend `glue-dotnet` is unavailable in 0.6");
}

#[test]
fn stale_backend_message_is_gone() {
    let case = GlueCase::new("Lib", &["glue_a"], GLUE_IMPORT_SOURCE);
    for backend in ["glue-rust", "glue-dotnet"] {
        let output = case.build(&["--backend", backend]);
        let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        assert!(!text.contains("declared for 0.4"), "{backend}: {text}");
        assert!(!text.contains("lands in 0.5"), "{backend}: {text}");
    }
}

#[test]
fn glue_rust_without_tool_flags_names_the_required_form() {
    let case = GlueCase::new("Lib", &["glue_a"], GLUE_IMPORT_SOURCE);
    let output = case.build(&["--backend", "glue-rust"]);
    case.assert_rejected(&output, "--rust-toolchain <prefix>");
}

#[test]
fn rust_toolchain_conflicts_with_cargo() {
    let case = GlueCase::new("Lib", &["glue_a"], GLUE_IMPORT_SOURCE);
    let prefix = case.path().join("tools").display().to_string();
    let cargo = case.tool("cargo").display().to_string();
    let linker = case.tool("cc").display().to_string();
    let output = case.build(&[
        "--backend",
        "glue-rust",
        "--rust-toolchain",
        &prefix,
        "--cargo",
        &cargo,
        "--linker",
        &linker,
    ]);
    case.assert_rejected(&output, "`--rust-toolchain` conflicts with `--cargo`/`--rustc`");
}

#[test]
fn cargo_without_rustc_is_rejected() {
    let case = GlueCase::new("Lib", &["glue_a"], GLUE_IMPORT_SOURCE);
    let cargo = case.tool("cargo").display().to_string();
    let linker = case.tool("cc").display().to_string();
    let output = case.build(&["--backend", "glue-rust", "--cargo", &cargo, "--linker", &linker]);
    case.assert_rejected(&output, "`--cargo` requires `--rustc`");
}

#[test]
fn missing_linker_is_rejected() {
    let case = GlueCase::new("Lib", &["glue_a"], GLUE_IMPORT_SOURCE);
    let prefix = case.path().join("tools").display().to_string();
    let output = case.build(&["--backend", "glue-rust", "--rust-toolchain", &prefix]);
    case.assert_rejected(&output, "requires `--linker <path>`");
}

#[test]
fn tool_flags_are_rejected_for_clif() {
    let case = GlueCase::new("Lib", &[], "pub type Placeholder { i32 value, }\n");
    let linker = case.tool("cc").display().to_string();
    let output = case.build(&["--linker", &linker]);
    case.assert_rejected(&output, "apply only to `--backend glue-rust`");
}

#[test]
fn non_shared_kind_is_rejected() {
    let case = GlueCase::new("Lib", &["glue_a"], GLUE_IMPORT_SOURCE);
    for kind in ["exe", "static", "object"] {
        let output = case.glue_rust(&["--kind", kind]);
        case.assert_rejected(&output, &format!("`--kind {kind}` is not supported"));
    }
}

#[test]
fn app_and_test_targets_are_rejected() {
    for kind in ["App", "Test"] {
        let case = GlueCase::new(kind, &["glue_a"], GLUE_IMPORT_SOURCE);
        let output = case.glue_rust(&[]);
        case.assert_rejected(&output, "`--backend glue-rust` requires a Lib target");
    }
}

#[test]
fn glue_rust_without_glue_block_is_rejected() {
    let case = GlueCase::new("Lib", &[], GLUE_IMPORT_SOURCE);
    let output = case.glue_rust(&[]);
    case.assert_rejected(&output, "declares none");
}

#[test]
fn glue_blocks_with_default_clif_backend_are_rejected() {
    let case = GlueCase::new("Lib", &["glue_a"], GLUE_IMPORT_SOURCE);
    let output = case.build(&[]);
    case.assert_rejected(&output, "build it with `--backend glue-rust`");
}

#[test]
fn glue_block_without_referencing_binding_is_rejected() {
    let case = GlueCase::new("Lib", &["glue_a"], "pub type Placeholder { i32 value, }\n");
    let output = case.glue_rust(&[]);
    case.assert_rejected(&output, "requires at least one glue block referenced by an Extern import");
}

#[test]
fn unreferenced_glue_block_is_rejected() {
    let case = GlueCase::new("Lib", &["glue_a", "glue_b"], GLUE_IMPORT_SOURCE);
    let output = case.glue_rust(&[]);
    case.assert_rejected(&output, "glue `glue_b` is not referenced");
}
