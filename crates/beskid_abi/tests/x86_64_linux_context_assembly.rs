#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata, render_runtime_asm_include};

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "beskid-x86-64-context-{}-{nonce}",
            std::process::id()
        ));
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
    TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .unwrap()
}

fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assembly/x86_64-unknown-linux-gnu/context.S")
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
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
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
    let mut symbols = String::from_utf8(output(nm.arg(&object)).stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    symbols.sort();
    assert_eq!(
        symbols,
        [
            "beskid_arch_v5_context_init".to_owned(),
            "beskid_arch_v5_context_switch".to_owned(),
        ]
    );

    let source = fs::read_to_string(source()).unwrap();
    for register in ["%rbx", "%rbp", "%r12", "%r13", "%r14", "%r15"] {
        assert!(source.contains(&format!("movq {register},")));
        assert!(source.contains(&format!(", {register}")));
    }
    assert!(
        !source.contains(".cfi_"),
        "context switching must not advertise unwind"
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
    let executable = temp.0.join("context_harness");
    let mut clang = Command::new("clang");
    if cfg!(target_os = "macos") {
        clang.args(["-arch", "x86_64", "-D__BESKID_TEST_MACHO=1"]);
    }
    output(
        clang
            .arg(source())
            .arg(&harness)
            .arg("-I")
            .arg(&temp.0)
            .arg("-o")
            .arg(&executable),
    );
    if cfg!(target_os = "macos") {
        output(Command::new("arch").arg("-x86_64").arg(executable));
    } else {
        output(&mut Command::new(executable));
    }
}
