use std::fs;

use beskid_manifest::{generate_v5_artifacts, load_v5_manifest_source};

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
    let source = fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime_manifest.bsol"),
    )
    .unwrap();
    let manifest = load_v5_manifest_source(&source).expect("valid v5 source");
    assert_eq!(manifest.meta.abi_version, 5);
    let trap = manifest
        .exports
        .iter()
        .find(|entry| entry.symbol == "beskid_rt_v5_trap")
        .unwrap();
    assert_eq!(trap.params[0].name, "code");
    assert_eq!(trap.result, "never");
    assert_eq!(
        manifest
            .targets
            .iter()
            .find(|entry| entry.triple == "x86_64-unknown-linux-gnu")
            .unwrap()
            .object_format,
        "elf"
    );

    let first = generate_v5_artifacts(&manifest).expect("artifacts");
    let second = generate_v5_artifacts(&manifest).expect("deterministic artifacts");
    assert_eq!(first, second);
    assert!(first.rust.contains("beskid_rt_v5_trap"));
    assert!(first.c_header.contains("_Noreturn"));
    assert!(
        first
            .gnu_asm
            .contains("BESKID_X86_64_UNKNOWN_LINUX_GNU_CONTEXT_SWITCH_FROM_REGISTER = rdi")
    );
    assert!(
        first
            .masm
            .contains("BESKID_X86_64_PC_WINDOWS_MSVC_CONTEXT_SWITCH_FROM_REGISTER TEXTEQU <rcx>")
    );
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
        fs::read_to_string(root.join("crates/beskid_abi/src/generated/abi_v5_contract.rs"))
            .unwrap(),
        artifacts.rust
    );
    assert_eq!(
        fs::read_to_string(root.join("crates/beskid_abi/include/beskid_runtime_abi_v5.h")).unwrap(),
        artifacts.c_header
    );
    assert_eq!(
        fs::read_to_string(root.join("crates/beskid_abi/include/beskid_runtime_abi_v5.inc"))
            .unwrap(),
        artifacts.gnu_asm
    );
    assert_eq!(
        fs::read_to_string(root.join("crates/beskid_abi/include/beskid_runtime_abi_v5_masm.inc"))
            .unwrap(),
        artifacts.masm
    );
    assert_eq!(
        fs::read_to_string(root.join("crates/beskid_abi/include/abi-v5.json")).unwrap(),
        artifacts.abi_json
    );
    assert_eq!(
        fs::read_to_string(root.join("crates/beskid_abi/include/abi-v5-audit.json")).unwrap(),
        artifacts.audit_json
    );
}

#[test]
fn generation_is_invariant_under_nonsemantic_collection_order() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let manifest = load_v5_manifest_source(&source).unwrap();
    let expected = generate_v5_artifacts(&manifest).unwrap();
    let mut permuted = manifest;
    permuted.targets.reverse();
    permuted.exports.reverse();
    permuted.intrinsics.reverse();
    permuted.layouts.reverse();
    permuted.platform_imports.reverse();
    permuted.assembly.reverse();
    permuted.traps.reverse();
    permuted.audit.forbidden_symbol_families.reverse();
    assert_eq!(generate_v5_artifacts(&permuted).unwrap(), expected);
}

#[test]
fn parser_rejects_unknown_duplicate_and_invalid_contract_fields() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    assert!(
        load_v5_manifest_source(&source.replacen(
            "schema_version = 1",
            "schema_version = 1\n  surprise = true",
            1
        ))
        .unwrap_err()
        .contains("unknown field")
    );
    assert!(
        load_v5_manifest_source(&source.replacen(
            "schema_version = 1",
            "schema_version = 1\n  schema_version = 1",
            1
        ))
        .unwrap_err()
        .contains("duplicate field")
    );
    assert!(
        load_v5_manifest_source(&source.replacen("returns = never", "returns = void", 1))
            .unwrap_err()
            .contains("noreturn")
    );
    assert!(
        load_v5_manifest_source(&source.replacen(
            "offset = 8, type = usize",
            "offset = 0, type = usize",
            1
        ))
        .unwrap_err()
        .contains("overlapping")
    );
    assert!(
        load_v5_manifest_source(&source.replacen(
            "{ name = base, type = pointer }",
            "{ name = base, type = u64 }",
            1
        ))
        .unwrap_err()
        .contains("intrinsic `pointer_add`")
    );
    assert!(
        load_v5_manifest_source(&source.replacen(
            "[rcx, rdx, r8, r9, \"stack+40\"]",
            "[rcx, rdx, r8, r9, r10]",
            1
        ))
        .unwrap_err()
        .contains("ABI mapping")
    );
}

#[test]
fn generated_assembler_contracts_are_target_scoped_and_parseable() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(root.join("runtime_manifest.bsol")).unwrap();
    let artifacts = generate_v5_artifacts(&load_v5_manifest_source(&source).unwrap()).unwrap();
    for target in [
        "X86_64_UNKNOWN_LINUX_GNU",
        "AARCH64_APPLE_DARWIN",
        "X86_64_PC_WINDOWS_MSVC",
    ] {
        assert!(
            artifacts
                .gnu_asm
                .contains(&format!("BESKID_{target}_CONTEXT_SIZE"))
        );
        assert!(
            artifacts
                .gnu_asm
                .contains(&format!("BESKID_{target}_SHADOW_SPACE"))
        );
        assert!(
            artifacts
                .masm
                .contains(&format!("BESKID_{target}_CONTEXT_SIZE"))
        );
    }
    let mut lhs = std::collections::BTreeSet::new();
    for line in artifacts
        .masm
        .lines()
        .filter(|line| line.contains(" EQU ") || line.contains(" TEXTEQU "))
    {
        assert!(
            lhs.insert(line.split_whitespace().next().unwrap()),
            "duplicate MASM definition: {line}"
        );
    }
    let temp = std::env::temp_dir().join(format!("beskid-abi-v5-{}.s", std::process::id()));
    let object = temp.with_extension("o");
    fs::write(
        &temp,
        format!(
            "{}\n.text\n.globl beskid_abi_v5_include_smoke\nbeskid_abi_v5_include_smoke:\n  nop\n",
            artifacts.gnu_asm
        ),
    )
    .unwrap();
    let output = std::process::Command::new("cc")
        .args(["-c", temp.to_str().unwrap(), "-o", object.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "assembler rejected generated include: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_file(temp);
    let _ = fs::remove_file(object);
}
