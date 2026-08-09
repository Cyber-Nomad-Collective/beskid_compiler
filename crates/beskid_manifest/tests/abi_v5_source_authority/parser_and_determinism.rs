use std::fs;

use beskid_manifest::{generate_v5_artifacts, load_v5_manifest_source};

#[test]
fn generation_is_invariant_under_nonsemantic_collection_order() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let manifest = load_v5_manifest_source(&source).unwrap();
    let expected = generate_v5_artifacts(&manifest).unwrap();
    let mut permuted = manifest.clone();
    permuted.targets.reverse();
    permuted.exports.reverse();
    permuted.intrinsics.reverse();
    permuted.layouts.reverse();
    permuted.platform_imports.reverse();
    permuted.corelib_services.reverse();
    for service in &mut permuted.corelib_services {
        service.target_bindings.reverse();
    }
    permuted.assembly.reverse();
    permuted.traps.reverse();
    permuted.audit.forbidden_symbol_families.reverse();
    assert_eq!(generate_v5_artifacts(&permuted).unwrap(), expected);

    let mut imports_in_one_order = manifest.clone();
    imports_in_one_order.corelib_services[0].target_bindings[0].os_imports = vec!["write".into(), "mmap".into()];
    let mut imports_in_another_order = imports_in_one_order.clone();
    imports_in_another_order.corelib_services[0].target_bindings[0].os_imports.reverse();
    assert_eq!(
        generate_v5_artifacts(&imports_in_one_order).unwrap(),
        generate_v5_artifacts(&imports_in_another_order).unwrap()
    );
}

#[test]
fn parser_rejects_unknown_duplicate_and_invalid_contract_fields() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    assert!(
        load_v5_manifest_source(&source.replacen("schema_version = 1", "schema_version = 1\n  surprise = true", 1))
            .unwrap_err()
            .contains("unknown field")
    );
    assert!(
        load_v5_manifest_source(&source.replacen("schema_version = 1", "schema_version = 1\n  schema_version = 1", 1))
            .unwrap_err()
            .contains("duplicate field")
    );
    assert!(
        load_v5_manifest_source(&source.replacen("returns = never", "returns = void", 1))
            .unwrap_err()
            .contains("noreturn")
    );
    assert!(
        load_v5_manifest_source(&source.replacen("offset = 8, type = usize", "offset = 0, type = usize", 1))
            .unwrap_err()
            .contains("overlapping")
    );
    assert!(
        load_v5_manifest_source(&source.replacen("target = \"x86_64-unknown-linux-gnu\"", "target = [bad]", 1,))
            .is_err()
    );
    assert!(
        load_v5_manifest_source(&source.replacen(
            "{ name = base, type = pointer }",
            "{ name = base, type = pointer, surprise = nope }",
            1,
        ))
        .is_err()
    );
    assert!(
        load_v5_manifest_source(&source.replacen(
            "{ name = base, type = pointer }",
            "{ name = base, name = duplicate, type = pointer }",
            1,
        ))
        .is_err()
    );
    assert!(
        load_v5_manifest_source(&source.replacen(
            "trap \"null_reference\" { code = 1 }",
            "trap \"bounds\" { code = 1 }",
            1,
        ))
        .is_err()
    );
    assert!(
        load_v5_manifest_source(&source.replacen(
            "{ stack_base = rsp, stack_offset = 40 }",
            "{ register = rsp, stack_offset = 40 }",
            1,
        ))
        .is_err()
    );
    assert!(
        load_v5_manifest_source(&source.replacen(
            "{ stack_base = rsp, stack_offset = 40 }",
            "{ stack_base = rsp, stack_offset = 40, surprise = nope }",
            1,
        ))
        .is_err()
    );
}
