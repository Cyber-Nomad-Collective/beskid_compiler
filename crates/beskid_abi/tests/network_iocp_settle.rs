//! IOCP deadline/completion settlement (docs/design/2026-10-12-iocp-deadline-race.md).
//!
//! The decision rules live in `assembly/common/network_settle.h`, which has no Windows
//! dependency, so the state-machine fixture runs on every supported native host. The
//! Windows fixture witnesses the completion-port behavior those rules depend on.
#![cfg(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
))]

use std::{
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use beskid_tests_support::native_harness::{executable_name, native_c_compiler, run_bounded, supported_native_host};

struct FixtureDirectory(PathBuf);

impl FixtureDirectory {
    fn create(stem: &str) -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("beskid-{stem}-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0)
            .unwrap_or_else(|error| panic!("remove fixture directory {}: {error}", self.0.display()));
    }
}

fn compile_and_run(fixture: &str, extra_args: &[&str], marker: &str) {
    assert!(supported_native_host(), "unsupported native fixture host");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let stem = fixture.trim_end_matches(".c");
    let output_dir = FixtureDirectory::create(stem);
    let executable = output_dir.path().join(executable_name(stem));
    let mut compile = native_c_compiler();
    compile.args(["-Wall", "-Wextra", "-Werror"]).arg(root.join("tests/fixtures").join(fixture)).args(extra_args);
    compile.args(["-o"]).arg(&executable);
    let compile = run_bounded(&format!("{fixture} compilation"), &mut compile, Duration::from_secs(60));
    assert!(
        compile.status.success(),
        "{fixture} compilation failed: status={}\nstdout:\n{}\nstderr:\n{}",
        compile.status,
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );
    let run = run_bounded(fixture, &mut Command::new(&executable), Duration::from_secs(60));
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success() && stdout.contains(marker),
        "{fixture} failed: status={}\nstdout:\n{}\nstderr:\n{}",
        run.status,
        stdout,
        String::from_utf8_lossy(&run.stderr),
    );
}

#[test]
fn iocp_settlement_lets_a_queued_completion_win_over_its_deadline() {
    compile_and_run("network_settle.c", &[], "settle=ok");
}

#[cfg(windows)]
#[test]
fn iocp_cancel_leaves_a_completed_packet_queued_and_aborts_an_idle_receive() {
    compile_and_run("network_settle_iocp.c", &["-lws2_32"], "iocp-settle=ok");
}

#[test]
fn iocp_transport_settles_runtime_cancels_and_orphans_only_on_native_close() {
    let source = include_str!("../assembly/x86_64-pc-windows-msvc/network_iocp.h").replace("\r\n", "\n");
    assert!(source.contains("#include \"../common/network_settle.h\""));
    let cancel = &source[source.find("void beskid_rt_v5_intrinsic_network_cancel").unwrap()..];
    let cancel = &cancel[..cancel.find("\n}\n").unwrap()];
    assert!(cancel.contains("BeskidNetworkSettleLocked(request->native_state)"), "runtime cancel must settle");
    let close = &source[source.find("int32_t beskid_rt_v5_intrinsic_network_close").unwrap()..];
    let close = &close[..close.find("\n}\n").unwrap()];
    assert!(close.contains("BeskidNetworkOrphanLocked("), "native close must orphan");
    assert!(
        source.contains("SleepConditionVariableSRW(&beskid_network_settled, &beskid_network_lock, INFINITE, 0)"),
        "settlement must wait for the dequeued packet with the network lock released"
    );
    assert!(source.contains("BeskidSettleClassify("), "the reactor must classify packets with the shared rules");
    assert!(source.contains("WakeAllConditionVariable(&beskid_network_settled)"));
}
