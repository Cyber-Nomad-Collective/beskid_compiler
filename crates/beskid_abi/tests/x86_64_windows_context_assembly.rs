#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata, render_runtime_asm_include};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("beskid-windows-context-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn target() -> TargetMetadata {
    TargetMetadata::supported().into_iter().find(|target| target.triple.as_str() == "x86_64-pc-windows-msvc").unwrap()
}

fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assembly/x86_64-pc-windows-msvc/context.asm")
}

fn llvm_tool(name: &str) -> PathBuf {
    let homebrew = Path::new("/opt/homebrew/opt/llvm/bin").join(name);
    if homebrew.is_file() { homebrew } else { PathBuf::from(name) }
}

fn llvm_tool_or_skip(name: &str) -> Option<PathBuf> {
    let path = llvm_tool(name);
    match Command::new(&path).arg("--help").output() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        _ => Some(path),
    }
}

fn prepare_include(temp: &Path) {
    let manifest = AbiManifestV5::canonical_runtime(target());
    fs::write(
        temp.join("beskid_runtime_abi_v5_x86_64_pc_windows_msvc.inc"),
        render_runtime_asm_include(&manifest).unwrap(),
    )
    .unwrap();
}

fn output(command: &mut Command) -> std::process::Output {
    let output = command.output().unwrap();
    assert!(output.status.success(), "command failed: {}", String::from_utf8_lossy(&output.stderr));
    output
}

#[test]
fn coff_object_exports_exactly_two_symbols_and_contains_no_unwind_sections() {
    let Some(llvm_ml) = llvm_tool_or_skip("llvm-ml") else {
        eprintln!("skipping: llvm-ml not available on this host");
        return;
    };
    let Some(llvm_nm) = llvm_tool_or_skip("llvm-nm") else {
        eprintln!("skipping: llvm-nm not available on this host");
        return;
    };
    let Some(llvm_objdump) = llvm_tool_or_skip("llvm-objdump") else {
        eprintln!("skipping: llvm-objdump not available on this host");
        return;
    };
    let temp = TempDir::new();
    prepare_include(&temp.0);
    let object = temp.0.join("context.obj");
    output(Command::new(llvm_ml).args(["--m64", "/c", "/X", "/Fo"]).arg(&object).arg("/I").arg(&temp.0).arg(source()));

    let mut symbols =
        String::from_utf8(output(Command::new(llvm_nm).args(["-g", "--defined-only", "-P"]).arg(&object)).stdout)
            .unwrap()
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let name = fields.next()?;
                let kind = fields.next()?;
                matches!(kind, "T" | "t").then(|| name.to_owned())
            })
            .collect::<Vec<_>>();
    symbols.sort();
    assert_eq!(symbols, ["beskid_arch_v5_context_init".to_owned(), "beskid_arch_v5_context_switch".to_owned(),]);

    let sections = String::from_utf8(output(Command::new(llvm_objdump).arg("-h").arg(&object)).stdout).unwrap();
    assert!(!sections.contains(".pdata"));
    assert!(!sections.contains(".xdata"));
}

#[test]
fn masm_source_saves_the_complete_manifest_preserved_register_set() {
    let source = fs::read_to_string(source()).unwrap();
    for register in ["rbx", "rbp", "rdi", "rsi", "r12", "r13", "r14", "r15"] {
        assert!(source.contains(&format!(
            "mov [rcx + BESKID_X86_64_PC_WINDOWS_MSVC_CONTEXT_{}_OFFSET], {register}",
            register.to_ascii_uppercase()
        )));
        assert!(source.contains(&format!(
            "mov {register}, [r10 + BESKID_X86_64_PC_WINDOWS_MSVC_CONTEXT_{}_OFFSET]",
            register.to_ascii_uppercase()
        )));
    }
    for register in ["xmm6", "xmm7", "xmm8", "xmm9", "xmm10", "xmm11", "xmm12", "xmm13", "xmm14", "xmm15"] {
        assert!(source.contains(&format!(
            "movdqu [rcx + BESKID_X86_64_PC_WINDOWS_MSVC_CONTEXT_{}_OFFSET], {register}",
            register.to_ascii_uppercase()
        )));
        assert!(source.contains(&format!(
            "movdqu {register}, [r10 + BESKID_X86_64_PC_WINDOWS_MSVC_CONTEXT_{}_OFFSET]",
            register.to_ascii_uppercase()
        )));
    }
    assert!(source.contains("BESKID_CONTEXT_INIT_RETURN_TRAMPOLINE_STACK_OPERAND"));
    assert!(!source.contains(".pushreg"));
    assert!(!source.contains(".allocstack"));
    assert!(!source.contains(".endprolog"));
}
