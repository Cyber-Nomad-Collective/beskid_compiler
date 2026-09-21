//! Std-only helpers for native hosted C test fixtures, not production runtime linking.

use std::{
    ffi::OsString,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

pub fn supported_native_host() -> bool {
    cfg!(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"),
    ))
}

pub fn executable_name(stem: &str) -> OsString {
    let mut name = OsString::from(stem);
    if cfg!(windows) {
        name.push(".exe");
    }
    name
}

pub fn shared_library_name(stem: &str) -> OsString {
    let mut name = OsString::from(stem);
    name.push(if cfg!(windows) {
        ".dll"
    } else if cfg!(target_os = "macos") {
        ".dylib"
    } else {
        ".so"
    });
    name
}

/// Configure a hosted fixture's compile-and-link command. DLL callers add the
/// native shared-image switch; canonical CRT provider libraries remain source-owned.
pub fn native_c_compiler() -> Command {
    assert!(supported_native_host(), "unsupported native fixture host");
    if cfg!(windows) {
        let mut command = Command::new("clang");
        // GNU-mode Clang injects libcmt at link time even with the dynamic object
        // policy. Suppress only that contradictory default, never all providers.
        command.args([
            "--target=x86_64-pc-windows-msvc",
            "-std=c11",
            "-fms-runtime-lib=dll",
            "-Wl,/NODEFAULTLIB:libcmt",
        ]);
        command
    } else {
        let mut command = Command::new("cc");
        command.arg("-std=c11");
        command
    }
}

/// Run a direct child with closed stdin, draining both output pipes concurrently.
/// On failure, kill and reap the child before reporting its complete diagnostics.
pub fn run_bounded(label: &str, command: &mut Command, limit: Duration) -> Output {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("{label}: spawn failed: {error}"));
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let stdout = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    let failure = loop {
        match child.try_wait() {
            Ok(Some(_)) => break None,
            Ok(None) if started.elapsed() < limit => thread::sleep(Duration::from_millis(10)),
            Ok(None) => break Some(format!("deadline exceeded ({limit:?})")),
            Err(error) => break Some(format!("try_wait failed: {error}")),
        }
    };
    let kill_error = failure.as_ref().and_then(|_| child.kill().err());
    let status = child.wait();
    let stdout = stdout.join().expect("stdout reader panicked").expect("stdout read failed");
    let stderr = stderr.join().expect("stderr reader panicked").expect("stderr read failed");
    if let Some(failure) = failure {
        panic!(
            "{label}: {failure}; limit={limit:?}; status={status:?}; kill_error={kill_error:?}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr),
        );
    }
    Output { status: status.unwrap_or_else(|error| panic!("{label}: wait failed: {error}")), stdout, stderr }
}

/// Place only the metadata-selected DLL beside an executable in its caller-owned
/// unique route directory. Unix keeps the selected library and loader environment.
pub fn place_shared_runtime(executable_dir: &Path, shared_library: &Path) -> PathBuf {
    if cfg!(windows) {
        let destination = executable_dir.join(shared_library.file_name().expect("runtime library must have a name"));
        std::fs::copy(shared_library, &destination).unwrap_or_else(|error| {
            panic!("place shared runtime {} beside {}: {error}", shared_library.display(), executable_dir.display())
        });
        destination
    } else {
        shared_library.to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        panic::AssertUnwindSafe,
        process::Command,
        time::{Duration, Instant},
    };

    const CHILD_MODE: &str = "BESKID_NATIVE_HARNESS_TEST_CHILD";

    fn child(mode: &str) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["native_harness::tests::child_probe", "--exact", "--nocapture", "--test-threads=1"])
            .env(CHILD_MODE, mode);
        command
    }

    #[test]
    fn host_and_artifact_names_match_the_native_contract() {
        let expected_host =
            matches!((std::env::consts::OS, std::env::consts::ARCH), ("linux", "x86_64") | ("macos", "aarch64"))
                || (cfg!(target_env = "msvc")
                    && std::env::consts::OS == "windows"
                    && std::env::consts::ARCH == "x86_64");
        assert_eq!(supported_native_host(), expected_host);
        assert_eq!(executable_name("fixture"), if cfg!(windows) { "fixture.exe" } else { "fixture" });
        assert_eq!(
            shared_library_name("fixture"),
            match std::env::consts::OS {
                "windows" => "fixture.dll",
                "macos" => "fixture.dylib",
                _ => "fixture.so",
            }
        );
    }

    #[test]
    fn hosted_c_command_uses_one_release_dynamic_crt_policy() {
        if !supported_native_host() {
            return;
        }
        let mut command = native_c_compiler();
        let args = command.get_args().map(|arg| arg.to_str().unwrap()).collect::<Vec<_>>();
        if cfg!(windows) {
            assert_eq!(command.get_program(), "clang");
            assert_eq!(
                args,
                ["--target=x86_64-pc-windows-msvc", "-std=c11", "-fms-runtime-lib=dll", "-Wl,/NODEFAULTLIB:libcmt"]
            );
            command.arg("-shared");
            assert_eq!(
                command.get_args().map(|arg| arg.to_str().unwrap()).collect::<Vec<_>>(),
                [
                    "--target=x86_64-pc-windows-msvc",
                    "-std=c11",
                    "-fms-runtime-lib=dll",
                    "-Wl,/NODEFAULTLIB:libcmt",
                    "-shared"
                ]
            );
        } else {
            assert_eq!(command.get_program(), "cc");
            assert_eq!(args, ["-std=c11"]);
        }
    }

    #[test]
    fn shared_runtime_placement_uses_only_the_selected_library() {
        let root = std::env::temp_dir().join(format!("beskid-native-placement-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let source = root.join(shared_library_name("selected"));
        std::fs::write(&source, b"selected-kit-library").unwrap();
        std::fs::write(root.join("unrelated.dll"), b"not-selected").unwrap();
        let route = root.join("route");
        std::fs::create_dir(&route).unwrap();
        let placed = place_shared_runtime(&route, &source);
        if cfg!(windows) {
            assert_eq!(placed, route.join(source.file_name().unwrap()));
            assert_eq!(std::fs::read(&placed).unwrap(), b"selected-kit-library");
            assert_eq!(std::fs::read_dir(&route).unwrap().count(), 1);
        } else {
            assert_eq!(placed, source);
            assert_eq!(std::fs::read_dir(&route).unwrap().count(), 0);
        }
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn child_probe() {
        match std::env::var(CHILD_MODE).as_deref() {
            Ok("timeout") => {
                println!("timeout-stdout-marker");
                eprintln!("timeout-stderr-marker");
                std::io::stdout().flush().unwrap();
                std::io::stderr().flush().unwrap();
                std::thread::sleep(Duration::from_secs(30));
                panic!("timeout child was not killed");
            }
            Ok("drain") => {
                let stdout = std::thread::spawn(|| {
                    std::io::stdout().write_all(&vec![b'O'; 1024 * 1024]).unwrap();
                    println!("stdout-complete");
                });
                std::io::stderr().write_all(&vec![b'E'; 1024 * 1024]).unwrap();
                eprintln!("stderr-complete");
                stdout.join().unwrap();
                let mut stdin = Vec::new();
                std::io::stdin().read_to_end(&mut stdin).unwrap();
                assert!(stdin.is_empty(), "bounded child must receive EOF on stdin");
            }
            Err(_) => {}
            mode => panic!("unsupported probe: {mode:?}"),
        }
    }

    #[test]
    fn timeout_kills_reaps_and_retains_both_streams() {
        let started = Instant::now();
        let failure = std::panic::catch_unwind(AssertUnwindSafe(|| {
            run_bounded("timeout probe", &mut child("timeout"), Duration::from_secs(1));
        }))
        .expect_err("the timeout must fail");
        assert!(started.elapsed() < Duration::from_secs(10), "kill/reap exceeded its cap");
        let diagnostic = failure.downcast_ref::<String>().expect("timeout diagnostic");
        for marker in [
            "timeout probe",
            "deadline exceeded",
            "status=Ok(",
            "kill_error=None",
            "timeout-stdout-marker",
            "timeout-stderr-marker",
        ] {
            assert!(diagnostic.contains(marker), "missing {marker}: {diagnostic}");
        }
    }

    #[test]
    fn simultaneous_large_streams_are_drained_completely() {
        let output = run_bounded("drain probe", &mut child("drain"), Duration::from_secs(10));
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout.iter().filter(|&&byte| byte == b'O').count(), 1024 * 1024);
        assert_eq!(output.stderr.iter().filter(|&&byte| byte == b'E').count(), 1024 * 1024);
        assert!(output.stdout.ends_with(b"\n") && String::from_utf8_lossy(&output.stdout).contains("stdout-complete"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("stderr-complete"));
    }
}
