use std::fs;

use beskid_manifest::load_v5_manifest_source;

#[test]
fn process_adapter_intrinsics_have_the_canonical_abi_v5_contract() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).expect("runtime manifest");
    let manifest = load_v5_manifest_source(&source).expect("valid runtime manifest");

    for (name, params, result) in [
        ("env_get", &["key"][..], "pointer"),
        ("env_set", &["key", "value"][..], "i32"),
        ("env_getcwd", &[][..], "pointer"),
        ("fs_read_text", &["path"][..], "pointer"),
        ("fs_write_text", &["path", "content"][..], "i32"),
        ("fs_exists", &["path"][..], "i32"),
        ("fs_mkdir", &["path"][..], "i32"),
        ("fs_delete", &["path"][..], "i32"),
        ("tty_winsize", &[][..], "pointer"),
    ] {
        let intrinsic = manifest
            .intrinsics
            .iter()
            .find(|intrinsic| intrinsic.name == name)
            .unwrap_or_else(|| panic!("manifest must declare {name}"));
        assert_eq!(intrinsic.symbol, format!("beskid_rt_v5_intrinsic_{name}"));
        assert_eq!(intrinsic.capability, format!("runtime.adapter.{name}"));
        assert_eq!(intrinsic.params.iter().map(|parameter| parameter.name.as_str()).collect::<Vec<_>>(), params);
        assert!(intrinsic.params.iter().all(|parameter| parameter.ty == "pointer"));
        assert_eq!(intrinsic.result, result);
    }
}
