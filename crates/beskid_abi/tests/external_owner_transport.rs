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
    fn Create() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("beskid-owner-transport-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn Path(&self) -> &Path {
        &self.0
    }
}

impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0)
            .unwrap_or_else(|error| panic!("remove fixture directory {}: {error}", self.0.display()));
    }
}

#[test]
fn native_owner_wake_closes_each_park_window_and_routes_only_to_its_owner() {
    assert!(supported_native_host(), "unsupported native fixture host");

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let outputDir = FixtureDirectory::Create();
    let executable = outputDir.Path().join(executable_name("owner_transport"));
    let mut compile = native_c_compiler();
    compile
        .args(["-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(root.join("include"))
        .arg(root.join("tests/fixtures/external_owner_transport.c"));
    if !cfg!(windows) {
        compile.arg("-lpthread");
    }
    compile.args(["-o"]).arg(&executable);
    let compile = run_bounded("owner transport fixture compilation", &mut compile, Duration::from_secs(60));
    assert!(
        compile.status.success(),
        "fixture compilation failed: status={}\nstdout:\n{}\nstderr:\n{}",
        compile.status,
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr),
    );

    let mut run = Command::new(&executable);
    let run = run_bounded("owner transport fixture", &mut run, Duration::from_secs(60));
    assert!(
        run.status.success(),
        "fixture execution failed: status={}\nstdout:\n{}\nstderr:\n{}",
        run.status,
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    print!("{}", String::from_utf8_lossy(&run.stdout));
}
