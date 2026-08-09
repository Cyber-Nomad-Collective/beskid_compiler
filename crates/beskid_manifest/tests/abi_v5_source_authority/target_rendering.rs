use std::fs;

use beskid_manifest::{generate_v5_artifacts, load_v5_manifest_source};

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
