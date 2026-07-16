use std::process::Command;

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_aot::{emit_host_context_library_pair, emit_library_pair, lower_canonical_runtime_prepared_syntax};
use beskid_codegen::CodegenArtifact;

#[test]
fn emits_static_and_shared_library_shells_without_runtime_kit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pair = emit_library_pair(
        CodegenArtifact::default(),
        temp.path().join("out"),
        "runtime_input",
        None,
        Vec::new(),
    )
    .expect("emit pair");
    assert!(pair.static_library.is_file());
    assert!(pair.shared_library.is_file());
    assert!(pair.provenance_symbols.is_empty());
}

#[test]
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
))]
fn host_context_pair_contains_the_manifest_context_exports_in_both_native_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let pair = emit_host_context_library_pair(
        CodegenArtifact::default(),
        temp.path().join("out"),
        "runtime_context",
    )
    .expect("emit host context pair");

    let expected = [
        "beskid_arch_v5_context_init".to_owned(),
        "beskid_arch_v5_context_switch".to_owned(),
    ];
    assert_eq!(pair.provenance_symbols, expected);

    for artifact in [&pair.static_library, &pair.shared_library] {
        let output = Command::new("nm")
            .args(["-g", "--defined-only", "-j"])
            .arg(artifact)
            .output()
            .expect("run nm");
        assert!(
            output.status.success(),
            "nm failed for {}",
            artifact.display()
        );
        let symbols = String::from_utf8(output.stdout).expect("utf-8 nm output");
        for symbol in &expected {
            assert!(
                symbols.lines().any(|line| {
                    line.trim_end_matches(':')
                        .strip_prefix('_')
                        .unwrap_or(line.trim_end_matches(':'))
                        == symbol
                }),
                "{} does not define {symbol}: {symbols}",
                artifact.display()
            );
        }
    }
}

#[test]
#[cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
))]
fn canonical_bootstrap_lowers_through_the_aot_prepared_syntax_boundary() {
    let triple = if cfg!(target_os = "macos") {
        "aarch64-apple-darwin"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == triple)
        .expect("supported host target");
    let artifact = lower_canonical_runtime_prepared_syntax(target.clone()).expect("lower Bootstrap");
    assert!(!artifact.functions.is_empty());
    let manifest = AbiManifestV5::canonical_runtime(target);
    for export in manifest.exports {
        assert!(
            artifact
                .exports
                .iter()
                .any(|entry| entry.exported_symbol == export.symbol),
            "missing manifest runtime export {}",
            export.symbol
        );
    }
}
