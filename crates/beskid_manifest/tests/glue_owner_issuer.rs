use beskid_manifest::load_v5_manifest_source;
fn source() -> String {
    include_str!("../../../runtime_manifest.bsol").to_owned()
}
#[test]
fn versioned_glue_identity_export_has_exact_signature() {
    assert!(load_v5_manifest_source(&source()).is_ok());
    for wrong in ["i64", "u32", "pointer", "void"] {
        let invalid = source().replace(
            "export \"beskid_glue_v1_next_identity\" { params = [] returns = u64 }",
            &format!("export \"beskid_glue_v1_next_identity\" {{ params = [] returns = {wrong} }}"),
        );
        assert!(load_v5_manifest_source(&invalid).is_err(), "issuer result must never collapse to {wrong}");
    }
    let invalid = source().replace(
        "export \"beskid_glue_v1_next_identity\" { params = [] returns = u64 }",
        "export \"beskid_glue_v1_next_identity\" { params = [{ name = seed, type = u64 }] returns = u64 }",
    );
    assert!(load_v5_manifest_source(&invalid).is_err(), "there is no caller-controlled reset/seed API");
}
#[test]
fn unrelated_v1_exports_remain_closed() {
    let invalid = source() + "\nexport \"beskid_glue_v1_guess\" { params = [] returns = u64 }\n";
    assert!(load_v5_manifest_source(&invalid).is_err());
}

#[test]
fn opaque_borrow_exports_reject_changed_contracts_and_unpinned_resolve() {
    let valid = source();
    for (name, parameters) in [
        ("begin", "[{ name = arg0, type = u64 }, { name = arg1, type = pointer }, { name = arg2, type = u64 }, { name = arg3, type = pointer }, { name = arg4, type = pointer }]"),
        ("end", "[{ name = arg0, type = u64 }, { name = arg1, type = pointer }, { name = arg2, type = u64 }]"),
    ] {
        let row = format!("export \"beskid_glue_v1_owner_opaque_borrow_{name}\" {{ params = {parameters} returns = i32 }}");
        assert!(valid.contains(&row), "control must match the real manifest row");
        for wrong in [row.replace("returns = i32", "returns = pointer"), row.replacen("type = u64", "type = pointer", 1), row.replace(parameters, "[]")] {
            assert!(load_v5_manifest_source(&valid.replace(&row, &wrong)).is_err());
        }
    }
    assert!(!valid.contains("export \"beskid_glue_v1_owner_opaque_resolve\""));
    assert!(load_v5_manifest_source(&(valid + "\nexport \"beskid_glue_v1_owner_opaque_resolve\" { params = [{ name = library, type = u64 }, { name = brand, type = pointer }, { name = token, type = u64 }, { name = out, type = pointer }] returns = i32 }\n")).is_err());
}
