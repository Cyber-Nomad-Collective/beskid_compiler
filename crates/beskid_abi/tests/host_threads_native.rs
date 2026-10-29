//! Native evidence for the isolate-prerequisite host primitives (`assembly/common/host_threads.h`
//! and the reference-counted worker pool in `assembly/common/external_wait.h`): thread create and
//! join, the five atomic word intrinsics, the host logical processor count with its failure
//! fallback, the process state block, and pool sharing between holders.
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
    fn create() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("beskid-host-threads-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn host_threads_atomics_processor_count_and_shared_pool_behave_natively() {
    assert!(supported_native_host(), "unsupported native fixture host");
    // The release build of the platform object reports profile 0; the debug kit's build defines
    // BESKID_RUNTIME_KIT_DEBUG=1 and reports 1 (it poisons and quarantines freed slots).
    for (define, expected_profile) in [(None, "kit-profile=0"), (Some("BESKID_RUNTIME_KIT_DEBUG=1"), "kit-profile=1")] {
        let stdout = run_host_threads_fixture(define);
        for evidence in [
            expected_profile,
            "process-state=zeroed",
            "thread=create-join",
            "atomics=fetch-add-2000",
            "pool=shared-refcounted",
            "host-threads=ok",
        ] {
            assert!(stdout.contains(evidence), "missing `{evidence}` in fixture output ({define:?}):\n{stdout}");
        }
    }
}

fn run_host_threads_fixture(define: Option<&str>) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output_dir = FixtureDirectory::create();
    let executable = output_dir.0.join(executable_name("host_threads"));
    let mut compile = native_c_compiler();
    compile.args(["-Wall", "-Wextra", "-Werror"]).arg("-I").arg(root.join("include"));
    if let Some(define) = define {
        compile.arg(if cfg!(windows) { format!("/D{define}") } else { format!("-D{define}") });
    }
    compile.arg(root.join("tests/fixtures/host_threads.c"));
    if !cfg!(windows) {
        compile.arg("-lpthread");
    }
    compile.args(["-o"]).arg(&executable);
    let compile = run_bounded("host threads fixture compilation", &mut compile, Duration::from_secs(60));
    assert!(
        compile.status.success(),
        "fixture compilation failed: status={}\nstdout:\n{}\nstderr:\n{}",
        compile.status,
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );

    let mut run = Command::new(&executable);
    let run = run_bounded("host threads fixture", &mut run, Duration::from_secs(60));
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "fixture execution failed: status={}\nstdout:\n{}\nstderr:\n{}",
        run.status,
        stdout,
        String::from_utf8_lossy(&run.stderr),
    );
    stdout
}
