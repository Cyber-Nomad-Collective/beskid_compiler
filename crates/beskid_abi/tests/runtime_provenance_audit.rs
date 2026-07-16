use beskid_abi::abi_v5::{TargetMetadata, TargetTriple};
use beskid_abi::runtime_provenance::{
    parse_symbol_list, RuntimeProvenanceAudit, SymbolList, SymbolListError,
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
