use std::{collections::BTreeSet, process::Command};

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
use beskid_abi::{abi_v5::RuntimeAuditMetadata, runtime_source::canonical_runtime_source_hash};
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
        "beskid_rt_v5_args_handoff_utf8",
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
        "beskid_rt_v5_args_handoff_utf8",
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
    let target = TargetMetadata::for_triple("x86_64-pc-windows-msvc").expect("Windows ABI-v5 target");
    let manifest = AbiManifestV5::canonical_runtime(target);
    let audit = RuntimeAuditMetadata::for_manifest(&manifest, &canonical_runtime_source_hash())
        .expect("canonical Windows runtime audit metadata");
    let descriptor_imports = manifest
        .platform_imports
        .iter()
        .filter(|entry| entry.library == "ucrt" && entry.symbol.starts_with('_'))
        .map(|entry| entry.symbol.as_str())
        .collect::<BTreeSet<_>>();
    assert!(!descriptor_imports.is_empty(), "Windows manifest must declare UCRT descriptor imports");
    for (label, profile) in [("debug", BuildProfile::Debug), ("release", BuildProfile::Release)] {
        let pair = emit_host_platform_library_pair(&authority, temp.path().join(label), "beskid_runtime", profile)
            .expect("emit Windows platform pair");

        let import_library =
            pair.shared_import_library.expect("Windows shared runtime must emit its COFF import library");
        assert!(import_library.is_file(), "missing import library: {}", import_library.display());
        assert_eq!(import_library.file_name().and_then(|name| name.to_str()), Some("beskid_runtime_import.lib"));
        assert!(pair.shared_library.is_file());
        assert!(pair.static_library.is_file());
        assert_windows_ucrt_descriptor_imports(&pair.shared_library, &descriptor_imports);
        let import_library_symbols = read_windows_import_library_symbols(&import_library);
        for symbol in &audit.loader_required_exports {
            assert!(
                import_library_symbols.direct_callable.contains(symbol),
                "Windows {label} import library is missing direct callable export {symbol}: {import_library_symbols:?}"
            );
            assert!(
                import_library_symbols.import_thunks.contains(&format!("__imp_{symbol}")),
                "Windows {label} import library is missing IAT thunk for {symbol}: {import_library_symbols:?}"
            );
        }
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
                "Windows {label} platform pair omitted {symbol}"
            );
        }
        for symbol in ["memset", "memcpy", "memmove", "memcmp"] {
            assert!(!pair.static_archive_inventory.defined.contains(&symbol.to_owned()));
            assert!(!pair.shared_image_inventory.defined.contains(&symbol.to_owned()));
        }
        assert_windows_memory_provider(&pair.shared_library);

        // No C-driver startup or explicit CRT libraries: extracting the owner helper
        // from the canonical archive must carry its provider directive to consumers.
        let consumer = temp.path().join(format!("{label}-static-consumer.dll"));
        let output = Command::new("lld-link")
            .args(["/NOLOGO", "/DLL", "/NOENTRY", "/INCLUDE:beskid_rt_v5_intrinsic_owner_create"])
            .arg(format!("/OUT:{}", consumer.display()))
            .arg(&pair.static_library)
            .output()
            .expect("link canonical static archive consumer");
        assert!(
            output.status.success(),
            "{label} static archive consumer link failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_windows_memory_provider(&consumer);
        eprintln!("Windows {label}: static/shared/import artifacts and VCRUNTIME140.dll!memset verified");
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn assert_windows_ucrt_descriptor_imports(image: &std::path::Path, expected_symbols: &BTreeSet<&str>) {
    let imports = read_windows_coff_imports(image);
    assert!(
        imports.iter().all(|(library, _)| !library.eq_ignore_ascii_case("msvcrt.dll")),
        "{} must not import msvcrt.dll: {imports:?}",
        image.display()
    );
    for symbol in expected_symbols {
        assert!(
            imports
                .iter()
                .any(|(library, imported_symbol)| { is_supported_ucrt_provider(library) && imported_symbol == symbol }),
            "{} does not import manifest UCRT descriptor symbol {symbol} from ucrtbase.dll or api-ms-win-crt-*.dll: {imports:?}",
            image.display()
        );
    }
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn read_windows_coff_imports(image: &std::path::Path) -> Vec<(String, String)> {
    let output = Command::new("llvm-readobj")
        .arg("--coff-imports")
        .arg(image)
        .output()
        .expect("inspect Windows PE import table");
    assert!(
        output.status.success(),
        "PE import inspection failed for {}: {}",
        image.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let imports = String::from_utf8(output.stdout).expect("UTF-8 PE imports");
    parse_windows_coff_imports(&imports)
}

fn parse_windows_coff_imports(imports: &str) -> Vec<(String, String)> {
    let mut parsed = Vec::new();
    for import in imports.split("Import {").skip(1) {
        let mut library = None;
        for line in import.lines().map(str::trim).take_while(|line| *line != "}") {
            if let Some(name) = line.strip_prefix("Name: ") {
                library = Some(name.to_owned());
            } else if let Some(symbol) = line.strip_prefix("Symbol: ") {
                let symbol = symbol.split_whitespace().next().expect("PE import symbol");
                parsed.push((library.clone().expect("PE import library before its symbols"), symbol.to_owned()));
            }
        }
    }
    parsed
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn is_supported_ucrt_provider(library: &str) -> bool {
    let library = library.to_ascii_lowercase();
    library == "ucrtbase.dll" || (library.starts_with("api-ms-win-crt-") && library.ends_with(".dll"))
}

#[derive(Debug, Default)]
struct WindowsCoffImportLibrarySymbols {
    direct_callable: BTreeSet<String>,
    import_thunks: BTreeSet<String>,
    // Keep llvm-nm import-data (`I`/`i`) records distinct from callable thunks.
    // Import archives contain metadata entries in addition to public callable
    // symbols; dropping them would erase their reported COFF classification.
    import_data: BTreeSet<String>,
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn read_windows_import_library_symbols(import_library: &std::path::Path) -> WindowsCoffImportLibrarySymbols {
    let output =
        Command::new("llvm-nm").arg("--extern-only").arg(import_library).output().expect("inspect COFF import library");
    assert!(
        output.status.success(),
        "COFF import library inspection failed for {}: {}",
        import_library.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let symbols = String::from_utf8(output.stdout).expect("UTF-8 llvm-nm output");
    parse_windows_import_library_symbols(&symbols)
}

fn parse_windows_import_library_symbols(symbols: &str) -> WindowsCoffImportLibrarySymbols {
    let mut parsed = WindowsCoffImportLibrarySymbols::default();
    for line in symbols.lines() {
        let mut fields = line.split_whitespace().rev();
        let Some(symbol) = fields.next() else {
            continue;
        };
        let Some(class) = fields.next() else {
            continue;
        };
        match class {
            "T" | "t" if symbol.starts_with("__imp_") => {
                parsed.import_thunks.insert(symbol.to_owned());
            }
            "T" | "t" => {
                parsed.direct_callable.insert(symbol.to_owned());
            }
            "I" | "i" => {
                parsed.import_data.insert(symbol.to_owned());
                if symbol.starts_with("__imp_") {
                    parsed.import_thunks.insert(symbol.to_owned());
                }
            }
            _ => {}
        }
    }
    parsed
}

#[test]
fn windows_coff_artifact_parsers_preserve_import_thunks_and_import_data_classes() {
    let imports =
        parse_windows_coff_imports("Import {\n  Name: api-ms-win-crt-stdio-l1-1-0.dll\n  Symbol: _read (0)\n}\n");
    assert_eq!(imports, [("api-ms-win-crt-stdio-l1-1-0.dll".into(), "_read".into())]);

    let symbols = parse_windows_import_library_symbols(
        "beskid_runtime.dll:\n00000000 T beskid_rt_v5_process_init\n00000000 T __imp_beskid_rt_v5_process_init\n00000000 I __IMPORT_DESCRIPTOR_beskid_runtime\n",
    );
    assert!(symbols.direct_callable.contains("beskid_rt_v5_process_init"));
    assert!(symbols.import_thunks.contains("__imp_beskid_rt_v5_process_init"));
    assert!(symbols.import_data.contains("__IMPORT_DESCRIPTOR_beskid_runtime"));
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn assert_windows_memory_provider(image: &std::path::Path) {
    let output =
        Command::new("llvm-readobj").arg("--coff-imports").arg(image).output().expect("inspect PE import providers");
    assert!(output.status.success(), "PE import inspection failed: {}", String::from_utf8_lossy(&output.stderr));
    let imports = String::from_utf8(output.stdout).expect("UTF-8 PE imports");
    let mut memory_imports = Vec::new();
    let mut vcruntime_imports = Vec::new();
    for import in imports.split("Import {").skip(1) {
        let mut library = None;
        for line in import.lines().map(str::trim).take_while(|line| *line != "}") {
            if let Some(name) = line.strip_prefix("Name: ") {
                library = Some(name);
            } else if let Some(symbol) = line.strip_prefix("Symbol: ") {
                let symbol = symbol.split_whitespace().next().expect("PE import symbol");
                let library = library.expect("PE import library before its symbols");
                if ["memset", "memcpy", "memmove", "memcmp"].contains(&symbol) {
                    memory_imports.push((library, symbol));
                }
                if library.to_ascii_lowercase().starts_with("vcruntime") {
                    vcruntime_imports.push((library, symbol));
                }
            }
        }
    }
    assert_eq!(memory_imports, [("VCRUNTIME140.dll", "memset")], "{} imports: {imports}", image.display());
    assert_eq!(vcruntime_imports, [("VCRUNTIME140.dll", "memset")], "{} imports: {imports}", image.display());
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
