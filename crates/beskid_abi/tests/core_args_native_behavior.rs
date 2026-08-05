#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir() -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let path = std::env::temp_dir().join(format!("beskid-core-args-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).expect("temporary Core.Args directory");
    path
}

fn compile(temp: &Path, name: &str, source: &str, entry: bool) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let harness = temp.join(format!("{name}.c"));
    let executable = temp.join(name);
    fs::write(&harness, source).expect("write Core.Args harness");
    let mut command = Command::new("clang");
    command.args(["-std=c11", "-arch", "arm64"]).arg(root.join("assembly/aarch64-apple-darwin/platform_host.c"));
    if entry {
        command.arg(root.join("assembly/aarch64-apple-darwin/args_entry.S"));
    }
    let output = command.arg(&harness).arg("-o").arg(&executable).output().expect("invoke clang");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    executable
}

#[test]
fn executable_entry_preserves_argv_zero_order_and_main_return_abi() {
    let temp = temp_dir();
    let executable = compile(
        &temp,
        "entry",
        r#"
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <unistd.h>
struct BeskidStr { const uint8_t *ptr; size_t len; };
extern int64_t beskid_rt_v5_args_count(void);
extern struct BeskidStr *beskid_rt_v5_args_get(int64_t);
_Noreturn void beskid_rt_v5_trap(uint8_t code, void *message, size_t len) { write(2, message, len); _exit(101); }
int beskid_program_main(void) {
  if (beskid_rt_v5_args_count() != 3) return 10;
  struct BeskidStr *zero = beskid_rt_v5_args_get(0), *one = beskid_rt_v5_args_get(1), *two = beskid_rt_v5_args_get(2);
  if (zero->len == 0 || memcmp(one->ptr, "alpha", 5) || one->len != 5 || memcmp(two->ptr, "beta", 4) || two->len != 4) return 11;
  return 47;
}
"#,
        true,
    );
    let output = Command::new(&executable).args(["alpha", "beta"]).output().expect("run Core.Args executable");
    assert_eq!(output.status.code(), Some(47), "{}", String::from_utf8_lossy(&output.stderr));
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn copied_arguments_outlive_input_and_bounds_trap_is_stable() {
    let temp = temp_dir();
    let executable = compile(
        &temp,
        "direct",
        r#"
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
struct BeskidStr { const uint8_t *ptr; size_t len; };
extern void beskid_rt_v5_args_handoff_utf8(int64_t, const char *const *);
extern int64_t beskid_rt_v5_args_count(void);
extern struct BeskidStr *beskid_rt_v5_args_get(int64_t);
_Noreturn void beskid_rt_v5_trap(uint8_t code, void *message, size_t len) { write(2, message, len); _exit(101); }
int main(void) {
  char value[] = "alpha"; const char *argv[] = { "argv0", value };
  beskid_rt_v5_args_handoff_utf8(2, argv); value[0] = 'X';
  struct BeskidStr *saved = beskid_rt_v5_args_get(1);
  if (beskid_rt_v5_args_count() != 2 || saved->len != 5 || memcmp(saved->ptr, "alpha", 5)) return 10;
  pid_t child = fork(); if (child == 0) { (void)beskid_rt_v5_args_get(2); _exit(0); }
  int status = 0; if (waitpid(child, &status, 0) != child) return 11;
  if (!WIFEXITED(status) || WEXITSTATUS(status) != 101) return 12;
  return 0;
}
"#,
        false,
    );
    let output = Command::new(&executable).output().expect("run Core.Args direct harness");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn windows_six_utf16_unit_witness_has_deterministic_utf8_replacement() {
    let temp = temp_dir();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let harness = temp.join("utf16_witness.c");
    let executable = temp.join("utf16_witness");
    fs::write(
        &harness,
        r#"
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include "args_utf16.h"
int main(void) {
  const uint16_t input[] = { 0x0041, 0xD83D, 0xDE00, 0xD800, 0x0062, 0xDC00, 0 };
  const unsigned char expected[] = { 0x41, 0xF0, 0x9F, 0x98, 0x80, 0xEF, 0xBF, 0xBD, 0x62, 0xEF, 0xBF, 0xBD };
  unsigned char output[sizeof expected];
  if (beskid_args_utf8_length(input) != sizeof expected) return 10;
  if (beskid_args_write_utf8(output, input) != output + sizeof expected) return 11;
  return memcmp(output, expected, sizeof expected) == 0 ? 0 : 12;
}
"#,
    )
    .expect("write UTF-16 witness");
    let output = Command::new("clang")
        .args(["-std=c11", "-I"])
        .arg(root.join("assembly/common"))
        .arg(&harness)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("compile UTF-16 witness");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = Command::new(&executable).output().expect("run UTF-16 witness");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let _ = fs::remove_dir_all(temp);
}
