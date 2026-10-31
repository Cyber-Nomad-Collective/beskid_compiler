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
        let path = std::env::temp_dir().join(format!("beskid-owner-transport-{}-{nonce}", std::process::id()));
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

#[test]
fn native_owner_wake_closes_each_park_window_and_routes_only_to_its_owner() {
    assert!(supported_native_host(), "unsupported native fixture host");

    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output_dir = FixtureDirectory::create();
    let executable = output_dir.path().join(executable_name("owner_transport"));
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
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("close-rebind=preserved"),
        "fixture did not prove that an admitted worker duplicate survives caller close/rebind:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("queue-rejection=released"),
        "fixture did not prove that a duplicate acquired before queue-full rejection is released:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    assert!(
        ["blocked-read-shutdown=orphaned", "live-read-shutdown=orphaned", "queued-at-stop=settled"]
            .iter()
            .all(|marker| String::from_utf8_lossy(&run.stdout).contains(marker)),
        "fixture did not prove that pool shutdown orphans every worker blocked in a read and settles queued work:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr),
    );
    print!("{}", String::from_utf8_lossy(&run.stdout));
}
