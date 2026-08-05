#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir() -> PathBuf {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).expect("clock").as_nanos();
    let path = std::env::temp_dir().join(format!("beskid-guarded-stack-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).expect("temporary guarded-stack directory");
    path
}

#[test]
fn linux_adapter_reserves_an_inaccessible_lower_guard_and_enforces_stack_bounds() {
    let temp = temp_dir();
    let harness = temp.join("guarded_stack_harness.c");
    let executable = temp.join("guarded_stack_harness");
    let adapter = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assembly/x86_64-unknown-linux-gnu/platform_host.c");
    fs::write(
        &harness,
        r#"
#include <signal.h>
#include <stddef.h>
#include <stdint.h>
#include <sys/wait.h>
#include <unistd.h>

extern void *beskid_rt_v5_intrinsic_guarded_stack_allocate(size_t);
extern void beskid_rt_v5_intrinsic_guarded_stack_free(void *, size_t);

int main(void) {
  if (beskid_rt_v5_intrinsic_guarded_stack_allocate(65535) != 0) return 10;
  if (beskid_rt_v5_intrinsic_guarded_stack_allocate(8 * 1024 * 1024 + 4096) != 0) return 11;
  unsigned char *usable = beskid_rt_v5_intrinsic_guarded_stack_allocate(64 * 1024);
  if (usable == 0) return 12;
  usable[0] = 1;
  usable[64 * 1024 - 1] = 2;
  pid_t child = fork();
  if (child < 0) return 13;
  if (child == 0) {
    usable[-1] = 3;
    _exit(0);
  }
  int status = 0;
  if (waitpid(child, &status, 0) != child) return 14;
  if (!WIFSIGNALED(status) || (WTERMSIG(status) != SIGSEGV && WTERMSIG(status) != SIGBUS)) return 15;
  beskid_rt_v5_intrinsic_guarded_stack_free(usable, 64 * 1024);
  return 0;
}
"#,
    )
    .expect("write guarded-stack harness");
    let output = Command::new("clang")
        .args(["-target", "x86_64-unknown-linux-gnu", "-std=c11"])
        .arg(adapter)
        .arg(&harness)
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("invoke clang");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let output = Command::new(&executable).output().expect("run guarded-stack harness");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let _ = fs::remove_dir_all(temp);
}
