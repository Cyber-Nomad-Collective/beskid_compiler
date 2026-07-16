use beskid_abi::abi_v5::{TargetMetadata, TargetTriple};
use beskid_abi::runtime_provenance::{
    RuntimeProvenanceAudit, SymbolList, SymbolListError, parse_symbol_list,
};

fn target(triple: &str) -> TargetMetadata {
    TargetMetadata::supported()
        .into_iter()
        .find(|candidate| candidate.triple == TargetTriple::from(triple))
        .expect("supported target")
}

#[test]
fn canonical_audit_is_deterministic_and_derives_target_allowlists() {
    let first = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();
    let second = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();

    assert_eq!(first, second);
    assert_eq!(first.target, "x86_64-unknown-linux-gnu");
    assert!(first.allowed_exports.contains(&"beskid_rt_v5_trap".into()));
    assert!(first.allowed_imports.contains(&"mmap".into()));
    assert!(first.forbidden_symbol_families.contains(&"rust".into()));
}

#[test]
fn canonical_audit_serializes_a_stable_machine_readable_contract() {
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-pc-windows-msvc")).unwrap();
    let json = audit.to_json().unwrap();

    assert_eq!(json, audit.to_json().unwrap());
    assert!(json.contains("\"target\":\"x86_64-pc-windows-msvc\""));
    assert!(json.contains("\"allowedImports\""));
    assert!(json.contains("\"forbiddenSymbolFamilies\""));
}

#[test]
fn portable_fixture_uses_each_targets_native_symbol_spelling() {
    for metadata in TargetMetadata::supported() {
        let audit = RuntimeProvenanceAudit::canonical(metadata).unwrap();
        let fixture = audit.fixture_symbol_list().unwrap();
        audit.verify(&fixture).unwrap();
    }
}

#[test]
fn symbol_list_parser_rejects_target_mismatch() {
    let list =
        parse_symbol_list("target=x86_64-pc-windows-msvc\ndefined=beskid_rt_v5_abi_version\n")
            .unwrap();
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();

    assert_eq!(
        audit.verify(&list),
        Err(SymbolListError::TargetMismatch {
            expected: "x86_64-unknown-linux-gnu".into(),
            actual: "x86_64-pc-windows-msvc".into(),
        })
    );
}

#[test]
fn symbol_list_rejects_rust_bridge_and_unwind_provenance() {
    let audit = RuntimeProvenanceAudit::canonical(target("aarch64-apple-darwin")).unwrap();
    let list = SymbolList {
        target: "aarch64-apple-darwin".into(),
        defined: audit
            .allowed_exports
            .iter()
            .map(|symbol| format!("_{symbol}"))
            .chain(std::iter::once("_beskid_runtime_bridge_init".into()))
            .collect(),
        undefined: audit
            .allowed_imports
            .iter()
            .map(|symbol| format!("_{symbol}"))
            .chain(std::iter::once("__Unwind_Resume".into()))
            .collect(),
    };

    let error = audit.verify(&list).unwrap_err();
    assert!(error.to_string().contains("forbidden runtime provenance"));
}

#[test]
fn symbol_list_rejects_non_abi_panic_export() {
    let audit = RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();
    let mut list = audit.fixture_symbol_list().unwrap();
    list.defined.push("panic".into());

    let error = audit.verify(&list).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("forbidden runtime provenance symbol `panic`"),
        "unexpected provenance error: {error}"
    );
}

#[test]
fn linux_shared_runtime_allows_only_documented_dynamic_loader_imports() {
    let loader_imports = [
        "_ITM_deregisterTMCloneTable",
        "_ITM_registerTMCloneTable",
        "__cxa_finalize",
        "__gmon_start__",
    ];
    let linux_audit =
        RuntimeProvenanceAudit::canonical(target("x86_64-unknown-linux-gnu")).unwrap();
    let mut linux_symbols = linux_audit.fixture_symbol_list().unwrap();
    linux_symbols
        .undefined
        .extend(loader_imports.iter().map(ToString::to_string));
    let static_error = linux_audit.verify(&linux_symbols).unwrap_err();
    assert!(static_error.to_string().contains("unexpected"));
    linux_audit.verify_shared(&linux_symbols).unwrap();

    let mut unexpected_linux_symbols = linux_symbols;
    unexpected_linux_symbols
        .undefined
        .push("__cxa_atexit".to_string());
    let error = linux_audit
        .verify_shared(&unexpected_linux_symbols)
        .unwrap_err();
    assert!(error.to_string().contains("unexpected"));

    let darwin_audit = RuntimeProvenanceAudit::canonical(target("aarch64-apple-darwin")).unwrap();
    let mut darwin_symbols = darwin_audit.fixture_symbol_list().unwrap();
    darwin_symbols
        .undefined
        .extend(loader_imports.iter().map(ToString::to_string));
    let error = darwin_audit.verify_shared(&darwin_symbols).unwrap_err();
    assert!(error.to_string().contains("unexpected"));
}
