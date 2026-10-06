//! Actual nested tools, receipts and inherited containment; no global environment changes.
#[cfg(unix)]
mod unix {
    use beskid_execution::{
        CompilerDriverConfiguration, CompilerDriverReceipt, CompilerDriverTool, NativeExecutionControl,
    };
    use sha2::{Digest, Sha256};
    use std::{
        fs,
        path::Path,
        process::Command,
        sync::Arc,
        time::{Duration, Instant},
    };

    fn tool(path: &str) -> CompilerDriverTool {
        let executable = fs::canonicalize(path).unwrap();
        let sha256 = format!("{:x}", Sha256::digest(fs::read(&executable).unwrap()));
        CompilerDriverTool { executable, sha256 }
    }
    fn driver(root: &Path, rustc: CompilerDriverTool, args: &[&str]) -> Command {
        let receipts = root.join("receipts");
        fs::create_dir(&receipts).unwrap();
        let config = CompilerDriverConfiguration {
            version: 1,
            nonce: "ab".repeat(32),
            rustc: rustc.clone(),
            linker: tool("/usr/bin/true"),
            receipts: fs::canonicalize(receipts).unwrap(),
            remaining_millis: 5000,
            output_limit: 1024 * 1024,
        };
        let path = root.join("driver.json");
        fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "\"$@\"; status=$?; exit \"$status\"", "cargo-parent"])
            .arg(env!("CARGO_BIN_EXE_beskid_native_tool_driver"));
        command.arg(rustc.executable).args(args).env("BESKID_NATIVE_COMPILER_DRIVER_CONFIG", path);
        command
    }
    #[test]
    fn v06_driver_records_actual_nested_executable_arguments_and_exit() {
        let root = tempfile::tempdir().unwrap();
        let expected = tool("/bin/echo");
        let mut command = driver(root.path(), expected.clone(), &["typed", "argv"]);
        let control = NativeExecutionControl::new(Instant::now() + Duration::from_secs(5), Arc::new(|| false));
        let output = control.run_command(&mut command, root.path(), "cargo-driver").unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, b"typed argv\n");
        let files = fs::read_dir(root.path().join("receipts")).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(files.len(), 1);
        let receipt: CompilerDriverReceipt = serde_json::from_slice(&fs::read(files[0].path()).unwrap()).unwrap();
        assert_eq!(receipt.phase, "rustc");
        assert_eq!(receipt.executable, expected.executable);
        assert_eq!(receipt.sha256, expected.sha256);
        assert_eq!(receipt.arguments, ["typed", "argv"]);
        assert!(receipt.success);
        assert_eq!(receipt.exit_code, Some(0));
    }
    #[test]
    fn v06_driver_rejects_uncontained_invocation_without_tool_or_receipt() {
        let root = tempfile::tempdir().unwrap();
        let mut command = driver(root.path(), tool("/bin/echo"), &["untrusted"]);
        command.env_remove("BESKID_EXECUTION_ENCLOSURE");
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(fs::read_dir(root.path().join("receipts")).unwrap().count(), 0);
    }
    #[test]
    fn v06_driver_nested_descendant_remains_in_parent_deadline_enclosure() {
        let root = tempfile::tempdir().unwrap();
        let pid = root.path().join("descendant");
        let mut command = driver(
            root.path(),
            tool("/bin/sh"),
            &["-c", "sleep 30 & echo $! > \"$1\"; wait", "fixture", pid.to_str().unwrap()],
        );
        let ready = pid.clone();
        let control =
            NativeExecutionControl::new(Instant::now() + Duration::from_secs(5), Arc::new(move || ready.is_file()));
        let error = control.run_command(&mut command, root.path(), "cargo-driver").unwrap_err();
        assert!(
            error.to_string().contains("cancel"),
            "nested tool did not publish readiness before cancellation: {error}"
        );
        let pid = fs::read_to_string(pid).expect("actual nested descendant readiness");
        let state = Command::new("ps").args(["-o", "stat=", "-p", pid.trim()]).output().unwrap();
        let state = String::from_utf8_lossy(&state.stdout);
        let stopped = state.trim().is_empty() || state.trim().starts_with('Z');
        if !stopped {
            let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
        }
        assert!(stopped, "nested descendant escaped owned parent: {pid} {state}");
    }
}
