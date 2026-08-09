use std::fs;

use beskid_manifest::{generate_v5_artifacts, load_v5_manifest_source};

use super::support::CORE_ARGS_SERVICES;

#[test]
fn core_args_adapter_bindings_generate_exact_target_facts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();

    let manifest = load_v5_manifest_source(&source).expect("Core.Args services are valid ABI-v5 manifest facts");
    let artifacts = generate_v5_artifacts(&manifest).expect("Core.Args binding artifacts");

    let bindings = manifest
        .corelib_services
        .iter()
        .flat_map(|service| {
            service.target_bindings.iter().map(move |binding| {
                (
                    service.name.as_str(),
                    service.adapter.as_str(),
                    service.params.iter().map(|parameter| parameter.ty.as_str()).collect::<Vec<_>>(),
                    service.result.as_str(),
                    binding.target.as_str(),
                    binding.implementation.as_str(),
                    binding.os_imports.iter().map(String::as_str).collect::<Vec<_>>(),
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        bindings,
        vec![
            (
                "__args_count",
                "beskid_rt_v5_args_count",
                vec![],
                "i64",
                "x86_64-unknown-linux-gnu",
                "beskid_rt_v5_args_count",
                vec!["mmap"]
            ),
            (
                "__args_count",
                "beskid_rt_v5_args_count",
                vec![],
                "i64",
                "aarch64-apple-darwin",
                "beskid_rt_v5_args_count",
                vec!["mmap"]
            ),
            (
                "__args_count",
                "beskid_rt_v5_args_count",
                vec![],
                "i64",
                "x86_64-pc-windows-msvc",
                "beskid_rt_v5_args_count",
                vec!["VirtualAlloc"]
            ),
            (
                "__args_get",
                "beskid_rt_v5_args_get",
                vec!["i64"],
                "string",
                "x86_64-unknown-linux-gnu",
                "beskid_rt_v5_args_get",
                vec!["mmap"]
            ),
            (
                "__args_get",
                "beskid_rt_v5_args_get",
                vec!["i64"],
                "string",
                "aarch64-apple-darwin",
                "beskid_rt_v5_args_get",
                vec!["mmap"]
            ),
            (
                "__args_get",
                "beskid_rt_v5_args_get",
                vec!["i64"],
                "string",
                "x86_64-pc-windows-msvc",
                "beskid_rt_v5_args_get",
                vec!["VirtualAlloc"]
            ),
        ]
    );
    assert!(artifacts.rust.contains("ABI_V5_CORELIB_SERVICE_BINDINGS"));
    assert!(artifacts.rust.contains("ABI_V5_CORE_ARGS_ENTRY_ADAPTERS"));
    assert!(artifacts.rust.contains("process_lifetime_copied_beskid_str_arena"));
    assert!(artifacts.rust.contains("beskid_rt_v5_args_count"));
    assert!(artifacts.rust.contains("beskid_rt_v5_args_get"));
    assert!(artifacts.c_header.contains("beskid_rt_v5_args_count(void)"));
    assert!(artifacts.c_header.contains("beskid_rt_v5_args_get(int64_t index)"));
    assert!(artifacts.abi_json.contains("\"corelibServices\""));
    assert!(artifacts.audit_json.contains("\"corelibServices\""));
    assert!(artifacts.audit_json.contains("\"entryAdapters\""));
    assert_eq!(
        manifest
            .entry_adapters
            .iter()
            .map(|adapter| (
                adapter.target.as_str(),
                adapter.executable_entry.as_str(),
                adapter.capture.as_str(),
                adapter.handoff.as_str()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("x86_64-unknown-linux-gnu", "main", "utf8_argv", "beskid_rt_v5_args_handoff_utf8"),
            ("aarch64-apple-darwin", "main", "utf8_argv", "beskid_rt_v5_args_handoff_utf8"),
            ("x86_64-pc-windows-msvc", "wmain", "utf16_wargv", "beskid_rt_v5_args_handoff_utf16"),
        ]
    );
}

#[test]
fn core_args_entry_adapter_rejects_missing_generated_provenance() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let source = source.replacen("  entry_source = \"args_entry.S\"\n", "", 1);
    assert!(
        load_v5_manifest_source(&source)
            .expect_err("entry adapter source is mandatory")
            .contains("missing `entry_source`")
    );
}

#[test]
fn core_args_adapter_binding_rejects_any_service_outside_the_exact_pair() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    source.push_str(
        r#"
corelib_service "__args_all" {
  adapter = "beskid_rt_v5_args_all"
  params = []
  returns = string
  target_bindings = [
    { target = "x86_64-unknown-linux-gnu", implementation = "beskid_rt_v5_args_all", os_imports = [] },
    { target = "aarch64-apple-darwin", implementation = "beskid_rt_v5_args_all", os_imports = [] },
    { target = "x86_64-pc-windows-msvc", implementation = "beskid_rt_v5_args_all", os_imports = [] }
  ]
}
"#,
    );

    assert_eq!(
        load_v5_manifest_source(&source).expect_err("__args_all must not become a Core.Args adapter"),
        "unexpected corelib service `__args_all`"
    );
}

#[test]
fn core_args_adapter_binding_rejects_noncanonical_implementation_symbols() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();

    for implementation in ["", "beskid_rt_v5_wrong_count", "wrong_count"] {
        let mutated = source.replacen(
            "implementation = \"beskid_rt_v5_args_count\"",
            &format!("implementation = \"{implementation}\""),
            1,
        );
        assert_eq!(
            load_v5_manifest_source(&mutated).expect_err("binding implementation must equal the canonical adapter"),
            format!(
                "corelib service `__args_count` binding for `x86_64-unknown-linux-gnu` must implement `beskid_rt_v5_args_count`"
            )
        );
    }
}

#[test]
fn core_args_adapter_binding_rejects_a_missing_target() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let missing = source.replacen(
        "    { target = \"aarch64-apple-darwin\", implementation = \"beskid_rt_v5_args_count\", os_imports = [mmap] },\n",
        "",
        1,
    );

    assert_eq!(
        load_v5_manifest_source(&missing).expect_err("missing target binding must be rejected"),
        "corelib service `__args_count` target bindings are incomplete"
    );
}

#[test]
fn core_args_adapter_binding_rejects_a_duplicate_service() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    source.push_str(CORE_ARGS_SERVICES);

    assert_eq!(
        load_v5_manifest_source(&source).expect_err("duplicate adapter service must be rejected"),
        "duplicate corelib service"
    );
}

#[test]
fn core_args_adapter_binding_rejects_a_duplicate_target() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let duplicated = source.replacen(
        "    { target = \"x86_64-unknown-linux-gnu\", implementation = \"beskid_rt_v5_args_count\", os_imports = [mmap] },",
        "    { target = \"x86_64-unknown-linux-gnu\", implementation = \"beskid_rt_v5_args_count\", os_imports = [mmap] },\n    { target = \"x86_64-unknown-linux-gnu\", implementation = \"beskid_rt_v5_args_count\", os_imports = [mmap] },",
        1,
    );

    assert_eq!(
        load_v5_manifest_source(&duplicated).expect_err("duplicate target binding must be rejected"),
        "duplicate corelib service `__args_count` target binding"
    );
}

#[test]
fn core_args_adapter_binding_rejects_a_signature_mismatch() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let mismatched = source.replacen(
        "corelib_service \"__args_get\" {\n  adapter = \"beskid_rt_v5_args_get\"\n  params = [{ name = index, type = i64 }]",
        "corelib_service \"__args_get\" {\n  adapter = \"beskid_rt_v5_args_get\"\n  params = []",
        1,
    );

    assert_eq!(
        load_v5_manifest_source(&mismatched).expect_err("signature mismatch must be rejected"),
        "corelib service `__args_get` signature must be [i64] -> string"
    );
}

#[test]
fn core_args_adapter_binding_rejects_an_undeclared_target_import() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let undeclared = source.replacen(
        "{ target = \"aarch64-apple-darwin\", implementation = \"beskid_rt_v5_args_count\", os_imports = [mmap] }",
        "{ target = \"aarch64-apple-darwin\", implementation = \"beskid_rt_v5_args_count\", os_imports = [missing_args_import] }",
        1,
    );

    assert_eq!(
        load_v5_manifest_source(&undeclared).expect_err("undeclared target import must be rejected"),
        "corelib service `__args_count` binding for `aarch64-apple-darwin` names undeclared OS import `missing_args_import`"
    );
}
