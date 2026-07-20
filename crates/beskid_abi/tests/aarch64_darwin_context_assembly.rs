#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata, render_runtime_asm_include};

struct TempDir(PathBuf);

static TEMP_DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = TEMP_DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "beskid-aarch64-context-{}-{nonce}-{sequence}",
            std::process::id(),
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
        .find(|target| target.triple.as_str() == "aarch64-apple-darwin")
        .unwrap()
}

fn source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assembly/aarch64-apple-darwin/context.S")
}

fn prepare_include(temp: &Path) {
    let manifest = AbiManifestV5::canonical_runtime(target());
    fs::write(
        temp.join("beskid_runtime_abi_v5_aarch64_apple_darwin.inc"),
        render_runtime_asm_include(&manifest).unwrap(),
    )
    .unwrap();
}

fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn object_exports_exactly_the_manifest_approved_symbols_and_saves_the_full_context() {
    let temp = TempDir::new();
    prepare_include(&temp.0);
    let object = temp.0.join("context.o");
    run(Command::new("clang")
        .args(["-c", "-arch", "arm64"])
        .arg(source())
        .arg("-I")
        .arg(&temp.0)
        .arg("-o")
        .arg(&object));

    let output = Command::new("nm")
        .args(["-gj"])
        .arg(&object)
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut symbols = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    symbols.sort();
    assert_eq!(
        symbols,
        [
            "_beskid_arch_v5_context_init".to_owned(),
            "_beskid_arch_v5_context_switch".to_owned(),
        ]
    );

    let source = fs::read_to_string(source()).unwrap();
    for register in [
        "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28", "x29", "x30", "d8",
        "d9", "d10", "d11", "d12", "d13", "d14", "d15",
    ] {
        assert!(
            source.contains(&format!("stp {register},"))
                || source.contains(&format!(", {register},"))
        );
        assert!(
            source.contains(&format!("ldp {register},"))
                || source.contains(&format!(", {register},"))
        );
    }
    assert!(
        !source.contains(".cfi_"),
        "context switching must not advertise unwind"
    );
}

#[test]
fn initialized_context_enters_returns_and_switches_repeatedly() {
    let temp = TempDir::new();
    prepare_include(&temp.0);
    let harness = temp.0.join("context_harness.c");
    fs::write(
        &harness,
        r#"
#include <stdint.h>
#include <stdlib.h>

typedef struct { unsigned char bytes[176]; } Context;
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
    run(Command::new("clang")
        .args(["-arch", "arm64"])
        .arg(source())
        .arg(&harness)
        .arg("-I")
        .arg(&temp.0)
        .arg("-o")
        .arg(&executable));
    run(&mut Command::new(executable));
}
