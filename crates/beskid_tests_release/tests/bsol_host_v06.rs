//! BSOL-06-01: the installed native `Core.Bsol` is a Beskid implementation that agrees with the Rust
//! `bsol` tooling on the shared corpus (`tooling--manifests-and-lockfiles--bsol`): the same accept/reject
//! decision and the same normalized syntax structure.
//!
//! The corpus comes from the retained packet input (`BESKID_BSOL_CORPUS`, the `beskid_bsol/conformance`
//! tree); the probe program is compiled by the installed CLI against the installed Corelib.
use beskid_tests_release::installed::{Cli, Home, executable_name, transcript};
use beskid_tests_release::project::write_app;
use std::path::{Path, PathBuf};

const CORPUS_ENV: &str = "BESKID_BSOL_CORPUS";

/// Parse argv[1] with the native parser and print the canonical native writer output, or a rejection.
const PROBE: &str = r#"use Core.Args;
use Core.Bsol.Limits;
use Core.Bsol.Parser;
use Core.Bsol.Writer;
use Core.FS;
use Core.Output;
use Core.Results;

i32 Main() {
    match Args.Get(1_i64) {
        Result::Error(_) => { Output.WriteLine("usage"); return 2; },
        Result::Ok(path) => match FS.ReadAllText(path) {
            Result::Error(_) => { Output.WriteLine("unreadable"); return 2; },
            Result::Ok(source) => match Parser.Parse(source, Limits.DefaultLimits()) {
                Result::Error(_) => { Output.WriteLine("reject"); return 1; },
                Result::Ok(document) => match Writer.Write(document, Limits.DefaultLimits()) {
                    Result::Error(_) => { Output.WriteLine("write-failed"); return 3; },
                    Result::Ok(canonical) => { Output.Write(canonical); return 0; },
                },
            },
        },
    };
    return 4;
}
"#;

fn corpus_root() -> PathBuf {
    let root = std::env::var_os(CORPUS_ENV).filter(|value| !value.is_empty()).map(PathBuf::from)
        .unwrap_or_else(|| panic!("{CORPUS_ENV} must name the retained shared BSOL corpus"));
    assert!(root.join("cases").is_dir(), "{CORPUS_ENV} must contain the `cases` tree: {}", root.display());
    root
}

fn files_with_extension(root: &Path, extension: &str) -> Vec<PathBuf> {
    fn walk(at: &Path, extension: &str, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(at).unwrap_or_else(|error| panic!("read {}: {error}", at.display())) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, extension, out);
            } else if path.extension().is_some_and(|value| value == extension) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, extension, &mut out);
    out.sort();
    out
}

// ---------------------------------------------------------------------------------------------

#[test]
fn bsol06_01_installed_corelib_bsol_declares_no_extern_to_rust_bsol() {
    let cli = Cli::qualified();
    let package = cli.corelib().join("packages/bsol");
    assert!(package.join("corelib_bsol.bproj").is_file(), "the installed Corelib ships the native BSOL package at {}", package.display());
    let sources = files_with_extension(&package.join("src"), "bd");
    assert!(sources.len() >= 10, "the native BSOL package has its Beskid sources ({} found)", sources.len());
    for path in &sources {
        let text = std::fs::read_to_string(path).unwrap();
        for line in text.lines() {
            let trimmed = line.trim_start();
            assert!(!trimmed.starts_with("[Extern"), "{} forwards to a native library: {trimmed}", path.display());
            assert!(!trimmed.contains("Library:") && !trimmed.contains("Library :"), "{} names a native library: {trimmed}", path.display());
        }
        let lowered = text.to_ascii_lowercase();
        assert!(!lowered.contains("bsol_syntax") && !lowered.contains("bsol-syntax") && !lowered.contains("libbsol"), "{} references the Rust BSOL implementation", path.display());
    }
    let manifest = std::fs::read_to_string(package.join("corelib_bsol.bproj")).unwrap();
    assert!(!manifest.contains("glue") && !manifest.contains("native"), "the BSOL package manifest declares no native owner:\n{manifest}");
    // Dependency inspection: the installed CLI itself reports the package as a pure Beskid library.
    let home = Home::new();
    let graph = cli.run(cli.command(&home).args(["dev", "project", "graph", "--project", package.join("corelib_bsol.bproj").to_str().unwrap(), "--offline"]));
    assert!(graph.status.success(), "dependency inspection of the installed BSOL package:\n{}", transcript(&graph));
    let listing = format!("{}{}", String::from_utf8_lossy(&graph.stdout), String::from_utf8_lossy(&graph.stderr)).to_ascii_lowercase();
    assert!(!listing.contains("bsol_syntax") && !listing.contains("glue"), "the dependency graph reaches no Rust BSOL owner:\n{listing}");
}

#[test]
fn bsol06_01_rust_and_native_agree_on_corpus_syntax_and_diagnostics() {
    let cli = Cli::qualified();
    let home = Home::new();
    let corpus = corpus_root();
    let manifest = write_app(&home.work(), "BsolProbe", PROBE);
    let probe = home.work().join(executable_name("bsol-probe"));
    let built = cli.run(cli.command(&home).args(["build", "--plain", "--offline", "--project", manifest.to_str().unwrap(), "--output", probe.to_str().unwrap()]));
    assert!(built.status.success(), "the native probe builds against the installed Corelib:\n{}", transcript(&built));
    let files = files_with_extension(&corpus.join("cases"), "bsol");
    assert!(files.len() >= 20, "the corpus has its syntax cases ({} found)", files.len());
    let mut mismatches = Vec::new();
    let mut accepted = 0;
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap();
        let rust = bsol::parse_document(&text);
        let native = std::process::Command::new(&probe).arg(path).env_remove("BESKID_CORELIB_ROOT").output().unwrap_or_else(|error| panic!("run probe: {error}"));
        let name = path.strip_prefix(&corpus).unwrap().display().to_string();
        match (rust, native.status.code()) {
            (Ok(document), Some(0)) => {
                accepted += 1;
                // BSOL-06-01 requires agreement on normalized syntax structure, not on writer layout: the
                // native writer keeps comment trivia and a compact layout. Both documents are normalized
                // through the Rust writer and compared there.
                let canonical = bsol::write_bsol_document(&document).unwrap_or_else(|error| panic!("{name}: Rust writer failed: {error}"));
                let native_text = String::from_utf8_lossy(&native.stdout).into_owned();
                match bsol::parse_document(&native_text) {
                    Ok(native_document) => {
                        let normalized = bsol::write_bsol_document(&native_document).unwrap_or_else(|error| panic!("{name}: Rust writer failed on native output: {error}"));
                        if canonical != normalized {
                            mismatches.push(format!("{name}: normalized structure differs\n--- rust\n{canonical}\n--- native (normalized)\n{normalized}\n--- native (raw)\n{native_text}"));
                        }
                    }
                    Err(error) => mismatches.push(format!("{name}: native writer output does not parse ({error}):\n{native_text}")),
                }
            }
            (Err(_), Some(1)) => {}
            (Ok(_), code) => mismatches.push(format!("{name}: Rust accepts, native exit {code:?}: {}", String::from_utf8_lossy(&native.stdout))),
            (Err(error), code) => mismatches.push(format!("{name}: Rust rejects ({error}), native exit {code:?}")),
        }
    }
    assert!(accepted >= 10, "the corpus has accepted syntax cases ({accepted} agreed)");
    assert!(mismatches.is_empty(), "{} of {} corpus files disagree between Rust and native BSOL:\n{}", mismatches.len(), files.len(), mismatches.join("\n\n"));
}
