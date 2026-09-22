#![cfg(any(target_os = "macos", target_os = "linux", all(target_os = "windows", target_env = "msvc")))]

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

    let sections = String::from_utf8(output(Command::new(&llvm_objdump).arg("-h").arg(&object)).stdout).unwrap();
    assert!(!sections.contains(".pdata"));
    assert!(!sections.contains(".xdata"));

    let relocations = String::from_utf8(output(Command::new(&llvm_objdump).arg("-r").arg(&object)).stdout).unwrap();
    let disassembly = String::from_utf8(
        output(Command::new(&llvm_objdump).args(["-d", "--x86-asm-syntax=intel"]).arg(&object)).stdout,
    )
    .unwrap();
    // Local labels can resolve at assembly time, leaving no relocation. Require
    // a 64-bit RIP-relative LEA as well, so an absent relocation cannot conceal
    // an absolute/truncated address or the wrong continuation target.
    let mut failures = Vec::new();
    for (label, register) in [("context_return", "r11"), ("context_resume", "rax")] {
        let label_relocations =
            relocations.lines().filter(|line| line.split_whitespace().last() == Some(label)).collect::<Vec<_>>();
        let relative_relocations =
            label_relocations.iter().all(|line| line.split_whitespace().nth(1) == Some("IMAGE_REL_AMD64_REL32"));
        let relative_lea = disassembly.lines().any(|line| {
            let instruction = line.split_whitespace().collect::<Vec<_>>().join(" ");
            instruction.contains(&format!("lea {register}, [rip +")) && instruction.ends_with(&format!("<{label}>"))
        });
        if !relative_relocations || !relative_lea {
            failures.push(format!(
                "{label}: expected 64-bit RIP-relative LEA into {register}; relocations={label_relocations:?}; RIP-relative LEA={relative_lea}"
            ));
        }
    }
    assert!(failures.is_empty(), "unsafe context continuation addresses:\n{}", failures.join("\n"));
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
    assert!(source.contains("mov [rcx + BESKID_X86_64_PC_WINDOWS_MSVC_CONTEXT_R13_OFFSET], r10"));
    assert!(source.contains("sub rsp, 8"));
    assert!(source.contains("jmp r13"));
    assert!(!source.contains(".pushreg"));
    assert!(!source.contains(".allocstack"));
    assert!(!source.contains(".endprolog"));
}

#[cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]
#[test]
fn windows_x64_context_enters_entry_and_return_trampoline_with_abi_aligned_stack() {
    let temp = TempDir::new();
    prepare_include(&temp.0);
    let harness = temp.0.join("context_harness.c");
    fs::write(
        &harness,
        r#"
#include <intrin.h>
#include <stdint.h>
#include <stdlib.h>

typedef __declspec(align(16)) struct { unsigned char bytes[240]; } Context;
extern void beskid_arch_v5_context_init(Context *, void *, void (*)(void *), void *, void (*)(void));
extern void beskid_arch_v5_context_switch(Context *, Context *);

static Context mainContext;
static Context fiberContext;
static uintptr_t token;
static int stage;

static __declspec(noinline) int StackIsAbiAligned(void) {
  return ((uintptr_t)_AddressOfReturnAddress() & 15) == 8;
}

static void FiberReturn(void) {
  stage = StackIsAbiAligned() ? 3 : -3;
  beskid_arch_v5_context_switch(&fiberContext, &mainContext);
  __debugbreak();
}

static void FiberEntry(void *argument) {
  stage = argument == &token && StackIsAbiAligned() ? 1 : -1;
  beskid_arch_v5_context_switch(&fiberContext, &mainContext);
  stage = 2;
}

int main(void) {
  const size_t stackSize = 64 * 1024;
  unsigned char *stack = _aligned_malloc(stackSize, 16);
  if (stack == NULL) return 10;
  beskid_arch_v5_context_init(&fiberContext, stack + stackSize, FiberEntry, &token, FiberReturn);
  beskid_arch_v5_context_switch(&mainContext, &fiberContext);
  if (stage != 1) return 11;
  beskid_arch_v5_context_switch(&mainContext, &fiberContext);
  if (stage != 3) return 12;
  _aligned_free(stack);
  return 0;
}
"#,
    )
    .unwrap();

    let object = temp.0.join("context.obj");
    output(
        Command::new("llvm-ml").args(["--m64", "/c", "/X", "/Fo"]).arg(&object).arg("/I").arg(&temp.0).arg(source()),
    );
    let executable = temp.0.join("context_harness.exe");
    output(
        Command::new("clang")
            .args(["--target=x86_64-pc-windows-msvc", "-std=c11", "-O0"])
            .arg(&harness)
            .arg(&object)
            .arg("-o")
            .arg(&executable),
    );
    output(&mut Command::new(executable));
}
