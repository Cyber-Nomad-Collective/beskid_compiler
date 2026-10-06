//! Real local processes establish deadline/cancellation semantics without mutating any toolchain.
#[cfg(unix)]
mod unix {
    use beskid_aot::api::NativeExecutionControl;
    use std::process::Command;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::{Duration, Instant};

    #[test]
    fn v06_native_tool_deadline_kills_and_reaps_hung_process() {
        let root = tempfile::tempdir().unwrap();
        let pid = root.path().join("pid");
        let mut command = Command::new("sh");
        command.arg("-c").arg("echo $$ > \"$1\"; while :; do :; done").arg("native-fixture").arg(&pid);
        let started = Instant::now();
        let control = NativeExecutionControl::new(started + Duration::from_millis(150), Arc::new(|| false));
        let error = control.run_command(&mut command, root.path(), "bootstrap").unwrap_err().to_string();
        assert!(error.contains("deadline") || error.contains("budget"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
        let process = std::fs::read_to_string(pid).unwrap();
        assert!(
            !Command::new("kill").args(["-0", process.trim()]).status().unwrap().success(),
            "native child was not reaped"
        );
    }

    #[test]
    fn v06_native_tool_cancellation_kills_and_reaps_active_process() {
        let root = tempfile::tempdir().unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let observer = cancelled.clone();
        let control = NativeExecutionControl::new(
            Instant::now() + Duration::from_secs(10),
            Arc::new(move || observer.load(Ordering::SeqCst)),
        );
        let signal = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            cancelled.store(true, Ordering::SeqCst);
        });
        let mut command = Command::new("sh");
        command.args(["-c", "while :; do :; done"]);
        let started = Instant::now();
        let error = control.run_command(&mut command, root.path(), "link").unwrap_err().to_string();
        signal.join().unwrap();
        assert!(error.contains("cancel"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn v06_native_tool_deadline_terminates_spawned_descendants() {
        let root = tempfile::tempdir().unwrap();
        let descendant = root.path().join("descendant");
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 30 & echo $! > \"$1\"; wait", "native-tree"]).arg(&descendant);
        let control = NativeExecutionControl::new(Instant::now() + Duration::from_millis(200), Arc::new(|| false));
        assert!(control.run_command(&mut command, root.path(), "link").is_err());
        let pid = std::fs::read_to_string(descendant).unwrap();
        let state = Command::new("ps").args(["-o", "stat=", "-p", pid.trim()]).output().unwrap();
        let state = String::from_utf8_lossy(&state.stdout);
        let terminated = state.trim().is_empty() || state.trim().starts_with('Z');
        if !terminated {
            let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
        }
        assert!(terminated, "native deadline left descendant running: pid={} state={state}", pid.trim());
    }
}

#[cfg(windows)]
mod windows {
    use beskid_aot::api::NativeExecutionControl;
    use std::{
        process::Command,
        sync::Arc,
        time::{Duration, Instant},
    };

    #[test]
    fn v06_windows_native_job_stops_descendant_before_returning_timeout() {
        let root = tempfile::tempdir().unwrap();
        let shell = std::path::PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"))
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let script = root.path().join("descendant.ps1");
        let pidfile = root.path().join("descendant.pid");
        std::fs::write(&script, r#"$ErrorActionPreference = 'Stop'
$child = Start-Process -FilePath (Join-Path $PSHOME 'powershell.exe') -ArgumentList @('-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 30') -PassThru
[System.IO.File]::WriteAllText($args[0], [string]$child.Id)
Start-Sleep -Seconds 30
"#).unwrap();
        let started = Instant::now();
        let control = NativeExecutionControl::new(started + Duration::from_secs(5), Arc::new(|| false));
        let mut command = Command::new(&shell);
        command
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .arg(&pidfile);
        let error =
            control.run_command(&mut command, root.path(), "Windows descendant fixture").unwrap_err().to_string();
        assert!(error.contains("deadline"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(15));
        let pid = std::fs::read_to_string(pidfile).expect("fixture must create an actual descendant");
        let verify_script = root.path().join("verify.ps1");
        std::fs::write(
            &verify_script,
            r#"$process = Get-Process -Id ([int]$args[0]) -ErrorAction SilentlyContinue
if ($null -ne $process) { Stop-Process -Id $process.Id -Force; exit 1 }
exit 0
"#,
        )
        .unwrap();
        let verification = NativeExecutionControl::new(Instant::now() + Duration::from_secs(10), Arc::new(|| false));
        let mut verify = Command::new(shell);
        verify
            .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(verify_script)
            .arg(pid.trim());
        let output = verification.run_command(&mut verify, root.path(), "fixture descendant verification").unwrap();
        assert!(
            output.status.success(),
            "native job left descendant alive: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
