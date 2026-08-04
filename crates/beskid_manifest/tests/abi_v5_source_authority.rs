use std::fs;

use beskid_manifest::{generate_v5_artifacts, load_v5_manifest_source};

const CORE_ARGS_SERVICES: &str = r#"
corelib_service "__args_count" {
  adapter = "beskid_rt_v5_args_count"
  params = []
  returns = i64
  target_bindings = [
    { target = "x86_64-unknown-linux-gnu", implementation = "beskid_rt_v5_args_count", os_imports = [] },
    { target = "aarch64-apple-darwin", implementation = "beskid_rt_v5_args_count", os_imports = [] },
    { target = "x86_64-pc-windows-msvc", implementation = "beskid_rt_v5_args_count", os_imports = [] }
  ]
}
corelib_service "__args_get" {
  adapter = "beskid_rt_v5_args_get"
  params = [{ name = index, type = i64 }]
  returns = string
  target_bindings = [
    { target = "x86_64-unknown-linux-gnu", implementation = "beskid_rt_v5_args_get", os_imports = [] },
    { target = "aarch64-apple-darwin", implementation = "beskid_rt_v5_args_get", os_imports = [] },
    { target = "x86_64-pc-windows-msvc", implementation = "beskid_rt_v5_args_get", os_imports = [] }
  ]
}
"#;

#[allow(dead_code)]
const MANIFEST: &str = r#"
manifest {
  abi_version = 5
  schema_version = 1
  runtime_publisher = "beskid-lang.org"
  runtime_package = "beskid-runtime-native"
  trap_exit_status = 101
  trap_diagnostic = "beskid runtime trap v5"
}
target "x86_64-unknown-linux-gnu" {
  endianness = little
  pointer_width = 64
  calling_convention = system_v
  object_format = elf
  symbol_prefix = ""
}
target "aarch64-apple-darwin" {
  endianness = little
  pointer_width = 64
  calling_convention = apple_aarch64
  object_format = macho
  symbol_prefix = "_"
}
target "x86_64-pc-windows-msvc" {
  endianness = little
  pointer_width = 64
  calling_convention = windows_x64
  object_format = coff
  symbol_prefix = ""
}
export "beskid_rt_v5_trap" {
  params = [{ name = code, type = u8 }, { name = message, type = pointer }, { name = message_len, type = usize }]
  returns = never
}
intrinsic "pointer_add" {
  symbol = "beskid_rt_v5_intrinsic_pointer_add"
  capability = "runtime.bootstrap.pointer_add"
  params = [{ name = base, type = pointer }, { name = offset, type = usize }]
  returns = pointer
}
trap "null_reference" { code = 1 }
trap "bounds" { code = 2 }
trap "overflow" { code = 3 }
trap "utf8" { code = 4 }
trap "oom" { code = 5 }
trap "handle" { code = 6 }
trap "deadlock" { code = 7 }
trap "abi" { code = 8 }
trap "unreachable" { code = 9 }
trap "corruption" { code = 10 }
assembly "beskid_arch_v5_context_switch" {
  params = [{ name = from, type = pointer }, { name = to, type = pointer }]
  returns = void
  x86_64_unknown_linux_gnu_preserved = [rbx, rbp, r12, r13, r14, r15]
  x86_64_unknown_linux_gnu_locations = [rdi, rsi]
  aarch64_apple_darwin_preserved = [x19, x20, x21, x22, x23, x24, x25, x26, x27, x28, x29, v8, v9, v10, v11, v12, v13, v14, v15]
  aarch64_apple_darwin_locations = [x0, x1]
  x86_64_pc_windows_msvc_preserved = [rbx, rbp, rdi, rsi, r12, r13, r14, r15, xmm6, xmm7, xmm8, xmm9, xmm10, xmm11, xmm12, xmm13, xmm14, xmm15]
  x86_64_pc_windows_msvc_locations = [rcx, rdx]
}
assembly "beskid_arch_v5_context_init" {
  params = [{ name = context, type = pointer }, { name = stack_top, type = pointer }, { name = entry, type = pointer }, { name = argument, type = pointer }, { name = return_trampoline, type = pointer }]
  returns = void
  x86_64_unknown_linux_gnu_preserved = [rbx, rbp, r12, r13, r14, r15]
  x86_64_unknown_linux_gnu_locations = [rdi, rsi, rdx, rcx, r8]
  aarch64_apple_darwin_preserved = [x19, x20, x21, x22, x23, x24, x25, x26, x27, x28, x29, v8, v9, v10, v11, v12, v13, v14, v15]
  aarch64_apple_darwin_locations = [x0, x1, x2, x3, x4]
  x86_64_pc_windows_msvc_preserved = [rbx, rbp, rdi, rsi, r12, r13, r14, r15, xmm6, xmm7, xmm8, xmm9, xmm10, xmm11, xmm12, xmm13, xmm14, xmm15]
  x86_64_pc_windows_msvc_locations = [rcx, rdx, r8, r9, "stack+40"]
}
audit {
  forbidden_symbol_families = [rust, _rust, __rust, "core::panicking", "std::panicking", "alloc::alloc", panic, _Unwind, __Unwind, eh_personality, gcc_personality, abfall, corosensei]
}
"#;

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
                vec![]
            ),
            (
                "__args_count",
                "beskid_rt_v5_args_count",
                vec![],
                "i64",
                "aarch64-apple-darwin",
                "beskid_rt_v5_args_count",
                vec![]
            ),
            (
                "__args_count",
                "beskid_rt_v5_args_count",
                vec![],
                "i64",
                "x86_64-pc-windows-msvc",
                "beskid_rt_v5_args_count",
                vec![]
            ),
            (
                "__args_get",
                "beskid_rt_v5_args_get",
                vec!["i64"],
                "string",
                "x86_64-unknown-linux-gnu",
                "beskid_rt_v5_args_get",
                vec![]
            ),
            (
                "__args_get",
                "beskid_rt_v5_args_get",
                vec!["i64"],
                "string",
                "aarch64-apple-darwin",
                "beskid_rt_v5_args_get",
                vec![]
            ),
            (
                "__args_get",
                "beskid_rt_v5_args_get",
                vec!["i64"],
                "string",
                "x86_64-pc-windows-msvc",
                "beskid_rt_v5_args_get",
                vec![]
            ),
        ]
    );
    assert!(artifacts.rust.contains("ABI_V5_CORELIB_SERVICE_BINDINGS"));
    assert!(artifacts.rust.contains("beskid_rt_v5_args_count"));
    assert!(artifacts.rust.contains("beskid_rt_v5_args_get"));
    assert!(artifacts.c_header.contains("beskid_rt_v5_args_count(void)"));
    assert!(artifacts.c_header.contains("beskid_rt_v5_args_get(int64_t index)"));
    assert!(artifacts.abi_json.contains("\"corelibServices\""));
    assert!(artifacts.audit_json.contains("\"corelibServices\""));
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
        "    { target = \"aarch64-apple-darwin\", implementation = \"beskid_rt_v5_args_count\", os_imports = [] },\n",
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
        "    { target = \"x86_64-unknown-linux-gnu\", implementation = \"beskid_rt_v5_args_count\", os_imports = [] },",
        "    { target = \"x86_64-unknown-linux-gnu\", implementation = \"beskid_rt_v5_args_count\", os_imports = [] },\n    { target = \"x86_64-unknown-linux-gnu\", implementation = \"beskid_rt_v5_args_count\", os_imports = [] },",
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
        "{ target = \"aarch64-apple-darwin\", implementation = \"beskid_rt_v5_args_count\", os_imports = [] }",
        "{ target = \"aarch64-apple-darwin\", implementation = \"beskid_rt_v5_args_count\", os_imports = [missing_args_import] }",
        1,
    );

    assert_eq!(
        load_v5_manifest_source(&undeclared).expect_err("undeclared target import must be rejected"),
        "corelib service `__args_count` binding for `aarch64-apple-darwin` names undeclared OS import `missing_args_import`"
    );
}

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

#[test]
fn generated_target_and_trap_tables_follow_manifest_facts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let changed = source
        .replacen("object_format = elf", "object_format = elf_test", 1)
        .replacen("symbol_prefix = \"\"", "symbol_prefix = \"_\"", 1)
        .replacen("trap \"null_reference\" { code = 1 }", "trap \"changed_null\" { code = 1 }", 1);
    let artifacts = generate_v5_artifacts(&load_v5_manifest_source(&changed).unwrap()).unwrap();
    assert!(artifacts.rust.contains("ABI_V5_TARGETS"));
    assert!(artifacts.rust.contains("elf_test"));
    assert!(artifacts.rust.contains("elf_test"));
    assert!(artifacts.rust.contains("changed_null"));
}

#[test]
fn windows_stack_parameter_is_typed_and_renders_as_a_masm_operand() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let artifacts = generate_v5_artifacts(&load_v5_manifest_source(&source).unwrap()).unwrap();
    let windows = &artifacts.masm["x86_64-pc-windows-msvc"];
    assert!(windows.contains("BESKID_CONTEXT_INIT_RETURN_TRAMPOLINE_STACK_OPERAND TEXTEQU <[rsp + 40]>"));
    assert!(!windows.contains("stack+40"));
    assert!(!windows.contains("RETURN_TRAMPOLINE_REGISTER"));
}

#[test]
fn generated_assembler_contracts_are_target_scoped_and_parseable() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let artifacts = generate_v5_artifacts(&load_v5_manifest_source(&source).unwrap()).unwrap();
    let linux = &artifacts.gnu_asm["x86_64-unknown-linux-gnu"];
    let darwin = &artifacts.gnu_asm["aarch64-apple-darwin"];
    let windows = &artifacts.masm["x86_64-pc-windows-msvc"];
    assert!(artifacts.c_header.contains("beskid_rt_v5_abi_version(void)"));
    let c_source = std::env::temp_dir().join(format!("beskid-abi-v5-{}.c", std::process::id()));
    let c_object = c_source.with_extension("o");
    fs::write(
        &c_source,
        format!("{}\nuint32_t smoke(void) {{ return beskid_rt_v5_abi_version(); }}\n", artifacts.c_header),
    )
    .unwrap();
    let c_output = std::process::Command::new("clang")
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-c",
            c_source.to_str().unwrap(),
            "-o",
            c_object.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        c_output.status.success(),
        "strict C compiler rejected header: {}",
        String::from_utf8_lossy(&c_output.stderr)
    );
    let _ = fs::remove_file(c_source);
    let _ = fs::remove_file(c_object);
    assert!(!linux.contains("xmm6") && !linux.contains("x19") && !linux.contains("STACK_OPERAND"));
    assert!(!darwin.contains("rdi") && !darwin.contains("xmm6") && !darwin.contains("STACK_OPERAND"));
    assert!(!windows.contains("x19") && !windows.contains("BESKID_AARCH64"));
    assert!(windows.contains("RETURN_TRAMPOLINE_STACK_OPERAND TEXTEQU <[rsp + 40]>"));
    assert!(!windows.contains("stack+40"));
    let mut lhs = std::collections::BTreeSet::new();
    for line in windows.lines().filter(|line| line.contains(" EQU ") || line.contains(" TEXTEQU ")) {
        assert!(lhs.insert(line.split_whitespace().next().unwrap()), "duplicate MASM definition: {line}");
    }
    let shadow_space = windows
        .lines()
        .find(|line| line.contains("SHADOW_SPACE EQU"))
        .and_then(|line| line.split_whitespace().last())
        .and_then(|value| value.parse::<u64>().ok())
        .expect("numeric Windows shadow space");
    let stack_location = windows
        .lines()
        .find(|line| line.contains("RETURN_TRAMPOLINE_STACK_OPERAND TEXTEQU"))
        .and_then(|line| line.split('<').nth(1))
        .and_then(|value| value.strip_suffix('>'))
        .and_then(|value| value.strip_prefix("[rsp + "))
        .and_then(|value| value.strip_suffix(']'))
        .and_then(|value| value.parse::<u64>().ok())
        .expect("typed Windows stack parameter offset");
    assert_eq!(stack_location, shadow_space + 8);
    for (triple, include, body) in [
        (
            "x86_64-unknown-linux-gnu",
            linux.as_str(),
            ".text\n.globl smoke\nsmoke:\n  mov %BESKID_CONTEXT_SWITCH_FROM_REGISTER, %rax\n  ret\n",
        ),
        (
            "arm64-apple-macos11",
            darwin.as_str(),
            ".text\n.globl _smoke\n_smoke:\n  mov x9, BESKID_CONTEXT_SWITCH_FROM_REGISTER\n  ret\n",
        ),
    ] {
        let temp = std::env::temp_dir().join(format!("beskid-abi-v5-{}-{triple}.S", std::process::id()));
        let object = temp.with_extension("o");
        fs::write(&temp, format!("{include}\n{body}")).unwrap();
        let output = std::process::Command::new("clang")
            .args(["-target", triple, "-c", temp.to_str().unwrap(), "-o", object.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(output.status.success(), "assembler rejected {triple}: {}", String::from_utf8_lossy(&output.stderr));
        let metadata = fs::metadata(&object).unwrap();
        assert!(metadata.len() > 0, "assembler produced an empty object");
        let bytes = fs::read(&object).unwrap();
        let expected_magic: &[u8] = match triple {
            "x86_64-unknown-linux-gnu" => b"\x7fELF",
            "arm64-apple-macos11" => b"\xcf\xfa\xed\xfe",
            _ => unreachable!(),
        };
        assert!(bytes.starts_with(expected_magic), "assembler produced the wrong object format for {triple}");
        let _ = fs::remove_file(temp);
        let _ = fs::remove_file(object);
    }

    let Some(llvm_ml) = std::env::var_os("LLVM_ML").map(std::path::PathBuf::from).or_else(|| {
        [std::path::PathBuf::from("llvm-ml"), std::path::PathBuf::from("/opt/homebrew/opt/llvm/bin/llvm-ml")]
            .into_iter()
            .find(|candidate| {
                std::process::Command::new(candidate)
                    .arg("--version")
                    .output()
                    .is_ok_and(|output| output.status.success())
            })
    }) else {
        // Linux CI runners do not ship llvm-ml; keep the GNU asm contract covered above.
        eprintln!("skipping MASM contract: LLVM_ML/llvm-ml not available");
        return;
    };
    let temp_dir = std::env::temp_dir().join(format!("beskid-v5-masm-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).unwrap();
    let include_path = temp_dir.join("beskid_runtime_abi_v5_windows.inc");
    let source_path = temp_dir.join("consumer.asm");
    let object_path = temp_dir.join("consumer.obj");
    fs::write(&include_path, windows).unwrap();
    fs::write(
        &source_path,
        "INCLUDE beskid_runtime_abi_v5_windows.inc\n.code\nsmoke PROC\n mov rax, BESKID_CONTEXT_INIT_CONTEXT_REGISTER\n mov r10, QWORD PTR BESKID_CONTEXT_INIT_RETURN_TRAMPOLINE_STACK_OPERAND\n mov r11, BESKID_X86_64_PC_WINDOWS_MSVC_CONTEXT_RIP_OFFSET\n ret\nsmoke ENDP\nEND\n",
    )
    .unwrap();
    let output = std::process::Command::new(&llvm_ml)
        .arg("-m64")
        .arg("/c")
        .arg("--fatal-warnings")
        .arg("/I")
        .arg(&temp_dir)
        .arg(format!("/Fo{}", object_path.display()))
        .arg(&source_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "llvm-ml rejected generated MASM include: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let object = fs::read(&object_path).unwrap();
    assert!(object.starts_with(b"\x64\x86"), "llvm-ml did not emit AMD64 COFF");
    let llvm_dir = llvm_ml.parent().unwrap_or_else(|| std::path::Path::new(""));
    let readobj = std::process::Command::new(llvm_dir.join("llvm-readobj"))
        .args(["--file-headers", "--relocations"])
        .arg(&object_path)
        .output()
        .expect("llvm-readobj must accompany llvm-ml");
    let readobj = String::from_utf8_lossy(&readobj.stdout);
    assert!(readobj.contains("IMAGE_FILE_MACHINE_AMD64"));
    assert!(!readobj.contains("IMAGE_REL_AMD64"), "generated include consumer must not leave relocations: {readobj}");
    let objdump = std::process::Command::new(llvm_dir.join("llvm-objdump"))
        .arg("-d")
        .arg(&object_path)
        .output()
        .expect("llvm-objdump must accompany llvm-ml");
    let disassembly = String::from_utf8_lossy(&objdump.stdout);
    assert!(disassembly.contains("%rcx"));
    assert!(disassembly.contains("0x28(%rsp)"));
    assert!(disassembly.contains("$0x48"));
    let _ = fs::remove_dir_all(temp_dir);
}
