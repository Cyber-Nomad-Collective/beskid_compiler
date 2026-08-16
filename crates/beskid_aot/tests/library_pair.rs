use std::process::Command;

use beskid_abi::{
    abi_v5::{AbiManifestV5, TargetMetadata},
    runtime_provenance::{RuntimeProvenanceAudit, parse_symbol_list},
};
use beskid_aot::{
    BuildProfile, emit_host_context_library_pair, emit_host_platform_library_pair,
    lower_canonical_runtime_prepared_syntax, require_canonical_host_emit_authority,
};

#[test]
fn host_emitters_mint_authority_only_from_the_embedded_canonical_corpus() {
    let authority = require_canonical_host_emit_authority().expect("canonical host authority");
    // Authority is a zero-sized opaque token; successful minting is the contract that
    // public host emitters can no longer accept an arbitrary CodegenArtifact.
    let _ = authority;
}

#[test]
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64"),))]
fn host_context_pair_contains_the_manifest_context_exports_in_both_native_artifacts() {
    let authority = require_canonical_host_emit_authority().expect("canonical host authority");
    let temp = tempfile::tempdir().expect("tempdir");
    let pair =
        emit_host_context_library_pair(&authority, temp.path().join("out"), "runtime_context", BuildProfile::Debug)
            .expect("emit host context pair");

    let expected = ["beskid_arch_v5_context_init".to_owned(), "beskid_arch_v5_context_switch".to_owned()];
    assert_eq!(pair.static_archive_inventory.defined, expected);
    assert_eq!(pair.shared_image_inventory.defined, expected);
    assert!(pair.canonical_object_inventory.defined.is_empty());
    assert_eq!(pair.additional_object_inventories.len(), 1);
    assert_eq!(pair.additional_object_inventories[0].defined, expected);

    for artifact in [&pair.static_library, &pair.shared_library] {
        let output = Command::new("nm").args(["-g", "--defined-only", "-j"]).arg(artifact).output().expect("run nm");
        assert!(output.status.success(), "nm failed for {}", artifact.display());
        let symbols = String::from_utf8(output.stdout).expect("utf-8 nm output");
        for symbol in &expected {
            assert!(
                symbols.lines().any(|line| {
                    line.trim_end_matches(':').strip_prefix('_').unwrap_or(line.trim_end_matches(':')) == symbol
                }),
                "{} does not define {symbol}: {symbols}",
                artifact.display()
            );
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn host_platform_pair_exports_canonical_runtime_and_host_platform_boundary() {
    let authority = require_canonical_host_emit_authority().expect("canonical host authority");
    let temp = tempfile::tempdir().expect("tempdir");
    let pair =
        emit_host_platform_library_pair(&authority, temp.path().join("out"), "runtime_platform", BuildProfile::Release)
            .expect("emit host platform pair");

    let required_exports = [
        "beskid_arch_v5_context_init",
        "beskid_arch_v5_context_switch",
        "beskid_rt_v5_abi_version",
        "beskid_rt_v5_intrinsic_system_allocate",
        "beskid_rt_v5_intrinsic_system_free",
        "beskid_rt_v5_intrinsic_guarded_stack_allocate",
        "beskid_rt_v5_intrinsic_guarded_stack_free",
        "beskid_rt_v5_intrinsic_tls_get",
        "beskid_rt_v5_intrinsic_tls_set",
    ];
    for symbol in required_exports {
        assert!(
            pair.static_archive_inventory.defined.iter().any(|entry| entry == symbol)
                && pair.shared_image_inventory.defined.iter().any(|entry| entry == symbol),
            "canonical platform provenance omitted {symbol}: static={:?}, shared={:?}",
            pair.static_archive_inventory.defined,
            pair.shared_image_inventory.defined
        );
    }

    for artifact in [&pair.static_library, &pair.shared_library] {
        let output = Command::new("nm").args(["-gU", "-j"]).arg(artifact).output().expect("run nm");
        assert!(output.status.success(), "nm failed for {}", artifact.display());
        let defined = String::from_utf8(output.stdout).expect("utf-8 nm output");
        for symbol in required_exports {
            assert!(
                defined.lines().any(|line| line.trim_start_matches('_') == symbol),
                "{} does not define {symbol}: {defined}",
                artifact.display()
            );
        }
    }

    let output = Command::new("nm").args(["-u", "-j"]).arg(&pair.static_library).output().expect("run nm");
    assert!(output.status.success(), "nm failed");
    let undefined = String::from_utf8(output.stdout).expect("utf-8 nm output");
    for symbol in ["_mmap", "_munmap", "__tlv_bootstrap"] {
        assert!(undefined.lines().any(|line| line == symbol), "platform archive does not import {symbol}: {undefined}");
    }
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn linux_host_platform_pair_exports_canonical_runtime_and_native_boundary() {
    let authority = require_canonical_host_emit_authority().expect("canonical host authority");
    let temp = tempfile::tempdir().expect("tempdir");
    let pair =
        emit_host_platform_library_pair(&authority, temp.path().join("out"), "runtime_platform", BuildProfile::Debug)
            .expect("emit Linux host platform pair");

    let required_exports = [
        "beskid_arch_v5_context_init",
        "beskid_arch_v5_context_switch",
        "beskid_rt_v5_abi_version",
        "beskid_rt_v5_intrinsic_system_allocate",
        "beskid_rt_v5_intrinsic_system_free",
        "beskid_rt_v5_intrinsic_guarded_stack_allocate",
        "beskid_rt_v5_intrinsic_guarded_stack_free",
        "beskid_rt_v5_intrinsic_tls_get",
        "beskid_rt_v5_intrinsic_tls_set",
    ];
    for symbol in required_exports {
        assert!(
            pair.static_archive_inventory.defined.iter().any(|entry| entry == symbol)
                && pair.shared_image_inventory.defined.iter().any(|entry| entry == symbol),
            "canonical platform provenance omitted {symbol}: static={:?}, shared={:?}",
            pair.static_archive_inventory.defined,
            pair.shared_image_inventory.defined
        );
    }

    for artifact in [&pair.static_library, &pair.shared_library] {
        let output = Command::new("nm").args(["-g", "--defined-only", "-j"]).arg(artifact).output().expect("run nm");
        assert!(output.status.success(), "nm failed for {}", artifact.display());
        let symbols = String::from_utf8(output.stdout).expect("utf-8 nm output");
        for symbol in required_exports {
            assert!(
                symbols.lines().any(|line| line == symbol),
                "{} does not define {symbol}: {symbols}",
                artifact.display()
            );
        }
    }
}

#[test]
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn windows_host_platform_pair_emits_a_coff_import_library_for_the_shared_runtime() {
    let authority = require_canonical_host_emit_authority().expect("canonical host authority");
    let temp = tempfile::tempdir().expect("tempdir");
    let pair =
        emit_host_platform_library_pair(&authority, temp.path().join("out"), "beskid_runtime", BuildProfile::Debug)
            .expect("emit Windows platform pair");

    let import_library = pair.shared_import_library.expect("Windows shared runtime must emit its COFF import library");
    assert!(import_library.is_file(), "missing import library: {}", import_library.display());
    assert_eq!(import_library.file_name().and_then(|name| name.to_str()), Some("beskid_runtime_import.lib"));
    assert!(pair.shared_library.is_file());
    assert!(pair.static_library.is_file());
    for symbol in [
        "beskid_rt_v5_intrinsic_system_allocate",
        "beskid_rt_v5_intrinsic_system_free",
        "beskid_rt_v5_intrinsic_guarded_stack_allocate",
        "beskid_rt_v5_intrinsic_guarded_stack_free",
        "beskid_rt_v5_intrinsic_tls_get",
        "beskid_rt_v5_intrinsic_tls_set",
    ] {
        assert!(
            pair.static_archive_inventory.defined.contains(&symbol.to_owned())
                && pair.shared_image_inventory.defined.contains(&symbol.to_owned()),
            "Windows platform pair omitted {symbol}"
        );
    }
}

#[test]
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64"),))]
fn canonical_bootstrap_lowers_through_the_aot_prepared_syntax_boundary() {
    let triple = if cfg!(target_os = "macos") { "aarch64-apple-darwin" } else { "x86_64-unknown-linux-gnu" };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == triple)
        .expect("supported host target");
    let artifact = lower_canonical_runtime_prepared_syntax(target.clone()).expect("lower Bootstrap");
    assert!(!artifact.functions.is_empty());
    let manifest = AbiManifestV5::canonical_runtime(target);
    for export in manifest.exports {
        assert!(
            artifact.exports.iter().any(|entry| entry.exported_symbol == export.symbol),
            "missing manifest runtime export {}",
            export.symbol
        );
    }
    let clif = artifact.functions.iter().map(|function| function.function.display().to_string()).collect::<String>();
    for intrinsic in [
        "beskid_rt_v5_intrinsic_memory_copy",
        "beskid_rt_v5_intrinsic_memory_set",
        "beskid_rt_v5_intrinsic_native_word_from_pointer",
        "beskid_rt_v5_intrinsic_pointer_add",
        "beskid_rt_v5_intrinsic_pointer_from_native_word",
        "beskid_rt_v5_intrinsic_raw_byte_load",
        "beskid_rt_v5_intrinsic_raw_byte_store",
        "beskid_rt_v5_intrinsic_raw_word_load",
        "beskid_rt_v5_intrinsic_raw_word_store",
    ] {
        assert!(!clif.contains(intrinsic), "direct ISLE intrinsic must not leave an object import: {intrinsic}");
    }
    assert!(
        !clif.contains("tls_value"),
        "TLS ownership is supplied by the native platform helper, not unsupported CLIF TLS globals"
    );
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn canonical_platform_pair_links_the_native_tls_helper() {
    let authority = require_canonical_host_emit_authority().expect("canonical host authority");
    let temp = tempfile::tempdir().expect("tempdir");
    let pair =
        emit_host_platform_library_pair(&authority, temp.path().join("out"), "beskid_runtime", BuildProfile::Debug)
            .expect("link canonical platform pair");
    for symbol in ["beskid_rt_v5_intrinsic_tls_get", "beskid_rt_v5_intrinsic_tls_set"] {
        assert!(pair.static_archive_inventory.defined.contains(&symbol.to_owned()));
        assert!(pair.shared_image_inventory.defined.contains(&symbol.to_owned()));
    }
    let symbols =
        Command::new("nm").args(["-u", pair.shared_library.to_str().expect("utf-8 path")]).output().expect("run nm");
    assert!(symbols.status.success());
    assert!(
        String::from_utf8_lossy(&symbols.stdout).contains("__tlv_bootstrap"),
        "Darwin TLV helper must retain its audited bootstrap import"
    );
    let source = temp.path().join("tls_isolation.c");
    std::fs::write(
        &source,
        r#"
#include <pthread.h>
extern void *beskid_rt_v5_intrinsic_tls_get(void);
extern void beskid_rt_v5_intrinsic_tls_set(void *);
static int main_token, thread_token;
static void *worker(void *unused) {
    (void)unused;
    if (beskid_rt_v5_intrinsic_tls_get() != 0) return (void *)1;
    beskid_rt_v5_intrinsic_tls_set(&thread_token);
    return beskid_rt_v5_intrinsic_tls_get() == &thread_token ? 0 : (void *)1;
}

int main(void) {
    pthread_t thread; void *result = 0;
    beskid_rt_v5_intrinsic_tls_set(&main_token);
    if (pthread_create(&thread, 0, worker, 0) != 0) return 1;
    if (pthread_join(thread, &result) != 0 || result != 0) return 2;
    return beskid_rt_v5_intrinsic_tls_get() == &main_token ? 0 : 3;
}

"#,
    )
    .expect("write TLS smoke");
    let executable = temp.path().join("tls_isolation");
    let status = Command::new("clang")
        .arg(&source)
        .arg(&pair.shared_library)
        .arg("-o")
        .arg(&executable)
        .status()
        .expect("compile TLS smoke");
    assert!(status.success());
    assert!(Command::new(executable).status().expect("run TLS smoke").success());
}

#[test]
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64"),))]
fn canonical_runtime_static_archive_hides_non_abi_implementation_symbols() {
    let triple = if cfg!(target_os = "macos") { "aarch64-apple-darwin" } else { "x86_64-unknown-linux-gnu" };
    let authority = require_canonical_host_emit_authority().expect("canonical host authority");
    let temp = tempfile::tempdir().expect("tempdir");
    let pair =
        emit_host_platform_library_pair(&authority, temp.path().join("out"), "beskid_runtime", BuildProfile::Debug)
            .expect("link canonical platform pair");
    let output =
        Command::new("nm").args(["-g", "--defined-only", "-j"]).arg(&pair.static_library).output().expect("run nm");
    assert!(output.status.success(), "nm failed");
    let symbols = String::from_utf8(output.stdout).expect("utf-8 nm output");
    assert!(!symbols.contains("#syntax"), "static runtime archive leaked syntax implementation symbols: {symbols}");
    assert!(
        !symbols.lines().map(|symbol| symbol.trim_start_matches('_')).any(|symbol| symbol == "panic"),
        "static runtime archive leaked forbidden non-ABI panic symbol: {symbols}"
    );

    let undefined =
        Command::new("nm").args(["-u", "-j"]).arg(&pair.static_library).output().expect("run nm for static imports");
    assert!(undefined.status.success(), "nm failed");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple.as_str() == triple)
        .expect("supported host target");
    let symbol_list = format!(
        "target={triple}\n{}{}",
        symbols
            .lines()
            .filter(|symbol| !symbol.is_empty() && !symbol.ends_with(':'))
            .map(|symbol| format!("defined={symbol}\n"))
            .collect::<String>(),
        String::from_utf8(undefined.stdout)
            .expect("utf-8 nm output")
            .lines()
            .filter(|symbol| !symbol.is_empty() && !symbol.ends_with(':'))
            .map(|symbol| format!("undefined={symbol}\n"))
            .collect::<String>(),
    );
    RuntimeProvenanceAudit::canonical(target)
        .expect("canonical provenance policy")
        .verify_static_archive(&parse_symbol_list(&symbol_list).expect("parse symbol list"))
        .expect("canonical static runtime archive satisfies provenance policy");
}
