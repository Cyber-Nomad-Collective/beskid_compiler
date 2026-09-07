use std::fs;

use beskid_manifest::load_v5_manifest_source;

#[test]
fn process_adapter_intrinsics_have_the_canonical_abi_v5_contract() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).expect("runtime manifest");
    let manifest = load_v5_manifest_source(&source).expect("valid runtime manifest");

    for (name, params, result, target_bound) in [
        ("env_get", &["key"][..], "pointer", false),
        ("env_set", &["key", "value"][..], "i32", false),
        ("env_getcwd", &[][..], "pointer", false),
        ("fs_read_text", &["path", "bytes_out", "length_out"][..], "i32", true),
        ("fs_read_text_release", &["bytes", "length"][..], "void", true),
        ("fs_write_text", &["path", "text"][..], "i32", true),
        ("fs_exists", &["path"][..], "i32", true),
        ("fs_mkdir", &["path"][..], "i32", true),
        ("fs_delete", &["path"][..], "i32", true),
        ("tty_winsize", &["fd"][..], "i64", false),
    ] {
        let intrinsic = manifest
            .intrinsics
            .iter()
            .find(|intrinsic| intrinsic.name == name)
            .unwrap_or_else(|| panic!("manifest must declare {name}"));
        assert_eq!(intrinsic.symbol, format!("beskid_rt_v5_intrinsic_{name}"));
        assert_eq!(intrinsic.capability, format!("runtime.adapter.{name}"));
        assert_eq!(intrinsic.params.iter().map(|parameter| parameter.name.as_str()).collect::<Vec<_>>(), params);
        if name == "tty_winsize" {
            assert_eq!(intrinsic.params[0].ty, "i64");
        } else {
            assert!(intrinsic.params.iter().all(|parameter| {
                parameter.ty == "pointer" || (name == "fs_read_text_release" && parameter.ty == "usize")
            }));
        }
        assert_eq!(intrinsic.result, result);
        assert_eq!(!intrinsic.target_bindings.is_empty(), target_bound);
    }
}

#[test]
fn console_terminal_detection_uses_the_canonical_runtime_adapter() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for platform in ["Linux", "MacOS", "Windows"] {
        let source = fs::read_to_string(root.join(format!("corelib/packages/console/src/Platform/{platform}.bd")))
            .unwrap_or_else(|error| panic!("read {platform} terminal adapter: {error}"));

        assert!(
            !source.contains("[Extern(") && !source.contains("isatty"),
            "{platform} terminal detection must not bypass the exact ABI-v5 runtime kit"
        );
        assert!(
            source.contains("__tty_winsize"),
            "{platform} terminal detection must use the canonical terminal adapter"
        );
    }
}
