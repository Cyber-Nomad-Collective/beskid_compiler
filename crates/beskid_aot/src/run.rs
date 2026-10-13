//! Build a linked executable from a codegen artifact and run it in a subprocess.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use beskid_codegen::CodegenArtifact;

use crate::api::{AotBuildRequest, BuildOutputKind, BuildProfile, ExportPolicy, LinkMode, RuntimeKitRequest, build};
use crate::error::{AotError, AotResult};
use crate::target::{detect_target, output_filename};

const RUN_EXE_BASENAME: &str = "beskid_run";
const DEFAULT_RUN_TIMEOUT: Duration = Duration::from_secs(60);

/// Inputs for [`build_and_run`]: lowered artifact, entrypoint, output directory, and runtime strategy.
#[derive(Debug, Clone)]
pub struct AotRunRequest {
    pub artifact: CodegenArtifact,
    pub entrypoint: String,
    pub output_dir: PathBuf,
    pub runtime: RuntimeKitRequest,
}

/// Linked executable path plus captured subprocess outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AotRunResult {
    pub exe_path: PathBuf,
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Emit and link an executable under `request.output_dir`, then run it with a bounded wait.
pub fn build_and_run(request: AotRunRequest) -> AotResult<AotRunResult> {
    if request.entrypoint.trim().is_empty() {
        return Err(AotError::InvalidRequest { message: "entrypoint must not be empty".to_owned() });
    }

    std::fs::create_dir_all(&request.output_dir)
        .map_err(|err| AotError::Io { path: request.output_dir.clone(), message: err.to_string() })?;

    let target = detect_target(None)?;
    let exe_path = request.output_dir.join(output_filename(RUN_EXE_BASENAME, BuildOutputKind::Exe, &target));

    let build_result = build(AotBuildRequest {
        artifact: request.artifact,
        output_kind: BuildOutputKind::Exe,
        output_path: exe_path.clone(),
        object_path: None,
        target_triple: None,
        profile: BuildProfile::Debug,
        entrypoint: request.entrypoint,
        export_policy: ExportPolicy::Explicit(Vec::new()),
        link_mode: LinkMode::Auto,
        runtime: Some(request.runtime),
        verbose_link: false,
        external_libraries: Vec::new(),
        library_search_paths: Vec::new(),
        pipeline: None,
    })?;

    let exe_path = build_result.final_path.ok_or_else(|| AotError::InvalidRequest {
        message: "executable build did not produce a final linked artifact".to_owned(),
    })?;

    let (exit_code, stdout, stderr) = run_executable(&exe_path, DEFAULT_RUN_TIMEOUT)?;

    Ok(AotRunResult { exe_path, exit_code, stdout, stderr })
}

/// Run an already-linked executable produced by [`build`] or [`build_and_run`], capturing both
/// output streams under the internal bounded wait.
pub fn run_linked_executable(path: &Path) -> AotResult<AotRunResult> {
    let (exit_code, stdout, stderr) = run_executable(path, DEFAULT_RUN_TIMEOUT)?;
    Ok(AotRunResult { exe_path: path.to_path_buf(), exit_code, stdout, stderr })
}

/// Exit status reported for a cancelled program on Windows, and the Unix status of `SIGINT`.
pub const CANCELLED_EXIT_CODE: i32 = 130;

/// Run a linked program as the user's foreground program: `arguments` pass through unchanged,
/// stdin, stdout and stderr are inherited, there is no time limit, and cancellation reaches the
/// program while this process stays alive to reap it. Returns the program's exit code; Unix signal
/// termination maps to `128 + signal` and Windows console cancellation to [`CANCELLED_EXIT_CODE`].
pub fn run_program(path: &Path, arguments: &[std::ffi::OsString]) -> AotResult<i32> {
    // Signal dispositions are process-wide: one foreground program at a time.
    static FOREGROUND: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _foreground = FOREGROUND.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let failed = |err: std::io::Error| AotError::RunFailed { path: path.to_path_buf(), message: err.to_string() };
    let forwarding = cancellation::Forwarding::install().map_err(failed)?;
    let mut child = Command::new(path).args(arguments).env("BESKID_AOT_MAIN", "1").spawn().map_err(failed)?;
    forwarding.attach(&child);
    let status = child.wait().map_err(failed)?;
    let cancelled = forwarding.cancelled();
    drop(forwarding);
    Ok(program_exit_code(status, cancelled))
}

fn program_exit_code(status: std::process::ExitStatus, cancelled: bool) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        let _ = cancelled;
        match (status.code(), status.signal()) {
            (Some(code), _) => code,
            (None, Some(signal)) => 128 + signal,
            (None, None) => 1,
        }
    }
    #[cfg(windows)]
    {
        // STATUS_CONTROL_C_EXIT: the default console handler terminated the program.
        const STATUS_CONTROL_C_EXIT: i32 = 0xC000_013A_u32 as i32;
        match status.code() {
            Some(STATUS_CONTROL_C_EXIT) => CANCELLED_EXIT_CODE,
            Some(_) if cancelled && !status.success() => CANCELLED_EXIT_CODE,
            Some(code) => code,
            None => 1,
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = cancelled;
        status.code().unwrap_or(1)
    }
}

/// Keeps this process alive across user cancellation while a foreground program runs.
mod cancellation {
    use std::process::Child;

    #[cfg(unix)]
    pub(super) use unix::Forwarding;
    #[cfg(windows)]
    pub(super) use windows::Forwarding;

    #[cfg(unix)]
    mod unix {
        use super::Child;
        use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

        static CHILD: AtomicI32 = AtomicI32::new(0);
        static CANCELLED: AtomicBool = AtomicBool::new(false);
        const SIGNALS: [libc::c_int; 4] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];

        /// Sender of a delivered signal; 0 for signals the terminal driver sends to the whole
        /// foreground process group, which the program has already received itself.
        #[cfg(any(target_os = "linux", target_os = "android", target_vendor = "apple"))]
        unsafe fn sender(info: *mut libc::siginfo_t) -> libc::pid_t {
            if info.is_null() { 1 } else { unsafe { (*info).si_pid() } }
        }
        #[cfg(not(any(target_os = "linux", target_os = "android", target_vendor = "apple")))]
        unsafe fn sender(_: *mut libc::siginfo_t) -> libc::pid_t {
            1
        }

        extern "C" fn forward(signal: libc::c_int, info: *mut libc::siginfo_t, _: *mut libc::c_void) {
            CANCELLED.store(true, Ordering::SeqCst);
            let child = CHILD.load(Ordering::SeqCst);
            // Only async-signal-safe calls: an atomic load and kill(2).
            if child > 0 && unsafe { sender(info) } != 0 {
                unsafe { libc::kill(child, signal) };
            }
        }

        /// Installed handlers; dropping restores the previous dispositions. Handlers (unlike
        /// ignored dispositions) reset to the default in the exec'd program.
        pub(in super::super) struct Forwarding {
            previous: Vec<(libc::c_int, libc::sigaction)>,
        }

        impl Forwarding {
            pub(in super::super) fn install() -> std::io::Result<Self> {
                CHILD.store(0, Ordering::SeqCst);
                CANCELLED.store(false, Ordering::SeqCst);
                let mut forwarding = Self { previous: Vec::with_capacity(SIGNALS.len()) };
                for signal in SIGNALS {
                    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
                    let handler: extern "C" fn(libc::c_int, *mut libc::siginfo_t, *mut libc::c_void) = forward;
                    action.sa_sigaction = handler as libc::sighandler_t;
                    action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
                    let mut previous: libc::sigaction = unsafe { std::mem::zeroed() };
                    unsafe { libc::sigemptyset(&mut action.sa_mask) };
                    if unsafe { libc::sigaction(signal, &action, &mut previous) } != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    forwarding.previous.push((signal, previous));
                }
                Ok(forwarding)
            }

            pub(in super::super) fn attach(&self, child: &Child) {
                CHILD.store(i32::try_from(child.id()).unwrap_or(0), Ordering::SeqCst);
            }

            pub(in super::super) fn cancelled(&self) -> bool {
                CANCELLED.load(Ordering::SeqCst)
            }
        }

        impl Drop for Forwarding {
            fn drop(&mut self) {
                CHILD.store(0, Ordering::SeqCst);
                for (signal, previous) in self.previous.drain(..).rev() {
                    unsafe { libc::sigaction(signal, &previous, std::ptr::null_mut()) };
                }
            }
        }
    }

    #[cfg(windows)]
    mod windows {
        use super::Child;
        use std::sync::atomic::{AtomicBool, Ordering};

        static CANCELLED: AtomicBool = AtomicBool::new(false);
        type Handler = unsafe extern "system" fn(u32) -> i32;

        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn SetConsoleCtrlHandler(handler: Option<Handler>, add: i32) -> i32;
        }

        /// The program shares this console and receives the same control event; this process
        /// only records it and keeps running until the program exits.
        unsafe extern "system" fn swallow(_event: u32) -> i32 {
            CANCELLED.store(true, Ordering::SeqCst);
            1
        }

        pub(in super::super) struct Forwarding;

        impl Forwarding {
            pub(in super::super) fn install() -> std::io::Result<Self> {
                CANCELLED.store(false, Ordering::SeqCst);
                if unsafe { SetConsoleCtrlHandler(Some(swallow), 1) } == 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(Self)
            }

            pub(in super::super) fn attach(&self, _child: &Child) {}

            pub(in super::super) fn cancelled(&self) -> bool {
                CANCELLED.load(Ordering::SeqCst)
            }
        }

        impl Drop for Forwarding {
            fn drop(&mut self) {
                unsafe { SetConsoleCtrlHandler(Some(swallow), 0) };
            }
        }
    }

    #[cfg(not(any(unix, windows)))]
    pub(in super) struct Forwarding;
    #[cfg(not(any(unix, windows)))]
    impl Forwarding {
        pub(in super) fn install() -> std::io::Result<Self> {
            Ok(Self)
        }
        pub(in super) fn attach(&self, _child: &Child) {}
        pub(in super) fn cancelled(&self) -> bool {
            false
        }
    }
}

/// Captured internal execution: both pipes drain concurrently so a program writing more than the
/// pipe capacity to either stream cannot deadlock against the bounded wait.
fn run_executable(path: &Path, timeout: Duration) -> AotResult<(i32, Vec<u8>, Vec<u8>)> {
    let failed = |err: std::io::Error| AotError::RunFailed { path: path.to_path_buf(), message: err.to_string() };
    let mut child = Command::new(path)
        .env("BESKID_AOT_MAIN", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(failed)?;
    let drain = |stream: Option<Box<dyn std::io::Read + Send>>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut stream) = stream {
                let _ = stream.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = drain(child.stdout.take().map(|stream| Box::new(stream) as Box<dyn std::io::Read + Send>));
    let stderr = drain(child.stderr.take().map(|stream| Box::new(stream) as Box<dyn std::io::Read + Send>));

    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(failed)? {
            break status;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = (stdout.join(), stderr.join());
            return Err(AotError::RunTimedOut { path: path.to_path_buf(), seconds: timeout.as_secs() });
        }
        thread::sleep(Duration::from_millis(20));
    };
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    Ok((status.code().unwrap_or(-1), stdout, stderr))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn script(body: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("program.sh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (root, path)
    }

    #[test]
    fn v06_captured_run_drains_more_than_pipe_capacity_on_both_streams() {
        let (_root, path) = script("i=0; while [ $i -lt 4096 ]; do echo 0123456789abcdef0123456789abcdef; echo fedcba9876543210fedcba9876543210 >&2; i=$((i+1)); done; exit 3");
        let (code, stdout, stderr) = run_executable(&path, Duration::from_secs(60)).unwrap();
        assert_eq!(code, 3);
        assert_eq!(stdout.len(), 4096 * 33);
        assert_eq!(stderr.len(), 4096 * 33);
    }

    #[test]
    fn v06_program_passes_argument_boundaries_and_exit_status() {
        let root = tempfile::tempdir().unwrap();
        let record = root.path().join("args.txt");
        let (_script_root, path) =
            script(&format!("for argument in \"$@\"; do printf '[%s]\\n' \"$argument\" >> '{}'; done; exit 7", record.display()));
        let arguments: Vec<std::ffi::OsString> =
            ["", "two words", "--dash", "caf\u{e9}"].into_iter().map(Into::into).collect();
        assert_eq!(run_program(&path, &arguments).unwrap(), 7);
        assert_eq!(std::fs::read_to_string(record).unwrap(), "[]\n[two words]\n[--dash]\n[caf\u{e9}]\n");
    }

    #[test]
    fn v06_program_signal_termination_maps_to_128_plus_signal() {
        let (_root, path) = script("kill -TERM $$");
        assert_eq!(run_program(&path, &[]).unwrap(), 128 + libc::SIGTERM);
    }

    #[test]
    fn v06_program_success_returns_zero() {
        let (_root, path) = script("exit 0");
        assert_eq!(run_program(&path, &[]).unwrap(), 0);
    }
}
