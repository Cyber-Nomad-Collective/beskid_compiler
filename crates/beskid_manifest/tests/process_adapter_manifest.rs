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
fn windows_environment_adapter_imports_include_error_state_reset() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).expect("runtime manifest");
    let manifest = load_v5_manifest_source(&source).expect("valid runtime manifest");

    let import = manifest
        .platform_imports
        .iter()
        .find(|import| import.target == "x86_64-pc-windows-msvc" && import.symbol == "SetLastError")
        .expect("Windows environment adapter must declare SetLastError provenance");

    assert_eq!(import.library, "kernel32");
    assert_eq!(
        import.params.iter().map(|parameter| (parameter.name.as_str(), parameter.ty.as_str())).collect::<Vec<_>>(),
        [("code", "u32")]
    );
    assert_eq!(import.result, "void");
}

#[test]
fn corelib_string_services_accept_managed_string_views() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).expect("runtime manifest");
    let manifest = load_v5_manifest_source(&source).expect("valid runtime manifest");

    for (name, parameters) in [("__syscall_write", &["i64", "pointer"][..]), ("__panic_str", &["pointer"][..])] {
        let service = manifest
            .corelib_services
            .iter()
            .find(|service| service.name == name)
            .unwrap_or_else(|| panic!("manifest must declare {name}"));
        assert_eq!(
            service.params.iter().map(|parameter| parameter.ty.as_str()).collect::<Vec<_>>(),
            parameters,
            "the service adapter must receive the same managed-string ABI used by Corelib"
        );
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

#[test]
fn platform_environment_data_import_is_distinct_from_callable_imports() {
    let source = include_str!("../../../runtime_manifest.bsol");
    let manifest = load_v5_manifest_source(source).unwrap();
    // Darwin exports `environ`; glibc's canonical data symbol is `__environ` (`environ` is a weak
    // alias that a linked shared runtime does not reference).
    for (target, symbol) in [("aarch64-apple-darwin", "environ"), ("x86_64-unknown-linux-gnu", "__environ")] {
        let data =
            manifest.platform_imports.iter().find(|entry| entry.target == target && entry.symbol == symbol).unwrap();
        assert_eq!(data.kind, "data");
        assert_eq!(data.result, "pointer");
        assert!(data.params.is_empty());
        let row = format!(
            "platform_data_import \"{symbol}\" {{ target = \"{target}\" library = {} type = pointer }}",
            data.library
        );
        assert!(source.contains(&row));
        for bad in [
            row.replace("type = pointer", "type = void"),
            row.replace("type = pointer", "params = [] type = pointer"),
            row.replace("type = pointer", "returns = pointer"),
        ] {
            assert!(load_v5_manifest_source(&source.replace(&row, &bad)).is_err());
        }
    }
    assert!(
        !manifest
            .platform_imports
            .iter()
            .any(|entry| entry.target == "x86_64-pc-windows-msvc" && entry.symbol.ends_with("environ"))
    );
    assert!(!manifest.platform_imports.iter().any(|entry| {
        (entry.target == "x86_64-unknown-linux-gnu" && entry.symbol == "environ")
            || (entry.target == "aarch64-apple-darwin" && entry.symbol == "__environ")
    }));
    // The runtime source must reference the same symbol the manifest declares per target.
    let transport = include_str!("../../beskid_abi/assembly/common/process_transport.h");
    assert!(transport.contains("#if defined(__GLIBC__)\n"));
    assert!(transport.contains("extern char **__environ;\n#define BESKID_PROCESS_ENVIRON __environ\n#else\nextern char **environ;\n#define BESKID_PROCESS_ENVIRON environ\n#endif"));
}

#[test]
fn linux_signal_set_imports_are_exact_glibc_functions_for_linux_only() {
    let source = include_str!("../../../runtime_manifest.bsol");
    let manifest = load_v5_manifest_source(source).unwrap();
    let linux = "x86_64-unknown-linux-gnu";
    for (symbol, params) in [
        ("sigaddset", &["pointer", "i32"][..]),
        ("sigemptyset", &["pointer"][..]),
        ("sigismember", &["pointer", "i32"][..]),
    ] {
        let entries =
            manifest.platform_imports.iter().filter(|entry| entry.symbol == symbol).collect::<Vec<_>>();
        assert_eq!(entries.len(), 1, "{symbol} must be declared for exactly one target");
        let entry = entries[0];
        assert_eq!(entry.target, linux, "{symbol} is a header macro on Darwin and absent on Windows");
        assert_eq!(entry.kind, "function");
        assert_eq!(entry.library, "libc");
        assert_eq!(entry.result, "i32");
        assert_eq!(entry.params.iter().map(|param| param.ty.as_str()).collect::<Vec<_>>(), params);
    }
}
