#![cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64")))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata, render_runtime_asm_include};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("beskid-x86-64-context-{}-{nonce}-{sequence}", std::process::id()));
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
    TargetMetadata::supported().into_iter().find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap()
}

fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assembly/x86_64-unknown-linux-gnu/context.S")
}

fn tls_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assembly/x86_64-unknown-linux-gnu/platform_tls.c")
}

fn prepare_include(temp: &Path) {
    let manifest = AbiManifestV5::canonical_runtime(target());
    fs::write(
        temp.join("beskid_runtime_abi_v5_x86_64_unknown_linux_gnu.inc"),
        render_runtime_asm_include(&manifest).unwrap(),
    )
    .unwrap();
}

fn output(command: &mut Command) -> std::process::Output {
    let output = command.output().unwrap();
    assert!(output.status.success(), "command failed: {}", String::from_utf8_lossy(&output.stderr));
    output
}

#[cfg(target_os = "macos")]
fn macos_x86_64_runner_available() -> bool {
    // macOS cannot execute the Linux ELF harness.  The compatibility run below instead links a
    // Mach-O x86_64 variant of the same SysV context assembly, which requires Rosetta on an
    // Apple-silicon host.  Keep the ELF object checks independent of this optional execution.
    Command::new("arch")
        .args(["-x86_64", "/usr/bin/true"])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(target_os = "macos")]
fn macos_x86_64_clang() -> Command {
    // Invoke xcrun as the driver rather than resolving clang and executing that path directly:
    // the wrapper supplies the selected macOS SDK's system headers and linker search paths.
    let mut command = Command::new("xcrun");
    command.args(["--sdk", "macosx", "clang"]);
    command
}

#[test]
fn elf_object_exports_exactly_two_symbols_and_saves_the_manifest_preserved_set() {
    let temp = TempDir::new();
    prepare_include(&temp.0);
    let object = temp.0.join("context.o");
    output(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-c"])
            .arg(source())
            .arg("-I")
            .arg(&temp.0)
            .arg("-o")
            .arg(&object),
    );

    let nm = if cfg!(target_os = "macos") {
        let mut command = Command::new("xcrun");
        command.args(["llvm-nm", "-g", "--defined-only", "-j"]);
        command
    } else {
        let mut command = Command::new("nm");
        command.args(["-g", "--defined-only", "-j"]);
        command
    };
    let mut nm = nm;
    let mut symbols =
        String::from_utf8(output(nm.arg(&object)).stdout).unwrap().lines().map(str::to_owned).collect::<Vec<_>>();
    symbols.sort();
    assert_eq!(symbols, ["beskid_arch_v5_context_init".to_owned(), "beskid_arch_v5_context_switch".to_owned(),]);

    let source = fs::read_to_string(source()).unwrap();
    for register in ["%rbx", "%rbp", "%r12", "%r13", "%r14", "%r15"] {
        assert!(source.contains(&format!("movq {register},")));
        assert!(source.contains(&format!(", {register}")));
    }
    assert!(!source.contains(".cfi_"), "context switching must not advertise unwind");
}

#[test]
fn linux_platform_tls_uses_dynamic_relocations_for_dlopen() {
    let temp = TempDir::new();
    let object = temp.0.join("platform_tls.o");
    output(
        Command::new("clang")
            .args(["-target", "x86_64-unknown-linux-gnu", "-std=c11", "-fPIC", "-c"])
            .arg(tls_source())
            .arg("-o")
            .arg(&object),
    );

    let relocations = String::from_utf8(output(Command::new("objdump").arg("-r").arg(&object)).stdout).unwrap();
    assert!(
        relocations.contains("TLSLD") || relocations.contains("TLSGD"),
        "dlopen-safe TLS must use dynamic TLS relocations, got:\n{relocations}"
    );
    assert!(
        relocations.contains("__tls_get_addr"),
        "dynamic TLS must resolve through the ELF loader, got:\n{relocations}"
    );
    assert!(
        !relocations.contains("GOTTPOFF"),
        "initial-exec TLS requires static TLS and cannot be used by a dlopen runtime:\n{relocations}"
    );
}

#[test]
fn sysv_context_enters_returns_and_switches_repeatedly() {
    let temp = TempDir::new();
    prepare_include(&temp.0);
    let harness = temp.0.join("context_harness.c");
    fs::write(
        &harness,
        r#"
#include <stdint.h>
#include <stdlib.h>

typedef struct { unsigned char bytes[64]; } Context;
extern void beskid_arch_v5_context_init(Context *, void *, void (*)(void *), void *, void (*)(void));
extern void beskid_arch_v5_context_switch(Context *, Context *);

static Context mainContext;
static Context fiberContext;
static uintptr_t token;
static int stage;

static void FiberReturn(void) {
  stage = 3;
  beskid_arch_v5_context_switch(&fiberContext, &mainContext);
  __builtin_trap();
}

static void FiberEntry(void *argument) {
  stage = argument == &token ? 1 : -1;
  beskid_arch_v5_context_switch(&fiberContext, &mainContext);
  stage = 2;
  beskid_arch_v5_context_switch(&fiberContext, &mainContext);
}

int main(void) {
  const size_t stackSize = 64 * 1024;
  unsigned char *stack = aligned_alloc(16, stackSize);
  if (stack == NULL) return 10;
  beskid_arch_v5_context_init(&fiberContext, stack + stackSize, FiberEntry, &token, FiberReturn);
  beskid_arch_v5_context_switch(&mainContext, &fiberContext);
  if (stage != 1) return 11;
  beskid_arch_v5_context_switch(&mainContext, &fiberContext);
  if (stage != 2) return 12;
  beskid_arch_v5_context_switch(&mainContext, &fiberContext);
  if (stage != 3) return 13;
  free(stack);
  return 0;
}
"#,
    )
    .unwrap();
    // Native Linux CI links and executes the ELF harness below.  macOS cannot execute an ELF
    // binary, so it validates the same source through an x86_64 Mach-O compatibility harness
    // only when Rosetta is installed.  Object-level ELF coverage above always remains required.
    #[cfg(target_os = "macos")]
    if !macos_x86_64_runner_available() {
        eprintln!("skipping x86_64 context runtime execution: Rosetta is unavailable (ELF object checks still ran)");
        return;
    }

    let executable = temp.0.join("context_harness");
    #[cfg(target_os = "macos")]
    let mut clang = macos_x86_64_clang();
    #[cfg(target_os = "linux")]
    let mut clang = Command::new("clang");
    if cfg!(target_os = "macos") {
        clang.args(["-arch", "x86_64", "-D__BESKID_TEST_MACHO=1"]);
    }
    output(clang.arg(source()).arg(&harness).arg("-I").arg(&temp.0).arg("-o").arg(&executable));
    if cfg!(target_os = "macos") {
        output(Command::new("arch").arg("-x86_64").arg(executable));
    } else {
        output(&mut Command::new(executable));
    }
}
