use std::fs;

use beskid_manifest::{generate_v5_artifacts, load_v5_manifest_source};

#[test]
fn v5_manifest_is_the_only_input_to_every_generated_artifact() {
    let source =
        fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime_manifest.bsol"))
            .unwrap();
    let manifest = load_v5_manifest_source(&source).expect("valid v5 source");
    assert_eq!(manifest.meta.abi_version, 5);
    let assembly_symbols =
        manifest.assembly.iter().map(|entry| entry.symbol.as_str()).collect::<std::collections::HashSet<_>>();
    assert!(manifest.intrinsics.iter().all(|intrinsic| intrinsic.symbol.starts_with("beskid_rt_v5_")
        || assembly_symbols.contains(intrinsic.symbol.as_str())));
    let trap = manifest.exports.iter().find(|entry| entry.symbol == "beskid_rt_v5_trap").unwrap();
    assert_eq!(trap.params[0].name, "code");
    assert_eq!(trap.result, "never");
    assert_eq!(
        manifest.targets.iter().find(|entry| entry.triple == "x86_64-unknown-linux-gnu").unwrap().object_format,
        "elf"
    );

    let first = generate_v5_artifacts(&manifest).expect("artifacts");
    let second = generate_v5_artifacts(&manifest).expect("deterministic artifacts");
    assert_eq!(first, second);
    assert!(first.rust.contains("beskid_rt_v5_trap"));
    assert!(first.c_header.contains("_Noreturn"));
    assert!(first.gnu_asm["x86_64-unknown-linux-gnu"].contains("#define BESKID_CONTEXT_SWITCH_FROM_REGISTER rdi"));
    assert!(first.masm["x86_64-pc-windows-msvc"].contains("BESKID_CONTEXT_SWITCH_FROM_REGISTER TEXTEQU <rcx>"));
    assert!(first.abi_json.contains("\"trapExitStatus\": 101"));
    assert!(first.audit_json.contains("\"forbiddenSymbolFamilies\""));
}

#[test]
fn checked_in_v5_artifacts_are_fresh() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let manifest = load_v5_manifest_source(&source).expect("workspace v5 source");
    let artifacts = generate_v5_artifacts(&manifest).expect("artifacts");
    assert_eq!(
        fs::read_to_string(root.join("crates/beskid_abi/src/generated/abi_v5_contract.rs")).unwrap(),
        artifacts.rust
    );
    assert_eq!(
        fs::read_to_string(root.join("crates/beskid_abi/include/beskid_runtime_abi_v5.h")).unwrap(),
        artifacts.c_header
    );
    for (target, contents) in artifacts.gnu_asm.iter().chain(&artifacts.masm) {
        assert_eq!(
            fs::read_to_string(
                root.join(format!("crates/beskid_abi/include/beskid_runtime_abi_v5_{}.inc", target.replace('-', "_")))
            )
            .unwrap(),
            *contents
        );
    }
    assert_eq!(fs::read_to_string(root.join("crates/beskid_abi/include/abi-v5.json")).unwrap(), artifacts.abi_json);
    assert_eq!(
        fs::read_to_string(root.join("crates/beskid_abi/include/abi-v5-audit.json")).unwrap(),
        artifacts.audit_json
    );
}

#[test]
fn intrinsic_linker_symbols_are_explicit_and_unique() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let duplicate = source.replacen(
        "symbol = \"beskid_rt_v5_intrinsic_pointer_add\"",
        "symbol = \"beskid_rt_v5_intrinsic_memory_copy\"",
        1,
    );

    assert!(
        load_v5_manifest_source(&duplicate)
            .expect_err("duplicate linker symbols must be rejected")
            .contains("intrinsic linker symbol")
    );
}
