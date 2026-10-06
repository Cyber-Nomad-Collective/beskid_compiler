//! One absolute native preparation budget shared across every phase.
/// Failure from the shared native execution authority. The owning subsystem maps
/// it to its own diagnostic without changing containment or deadline semantics.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct NativeExecutionError {
    message: String,
    kind: std::io::ErrorKind,
}
impl NativeExecutionError {
    pub fn kind(&self) -> std::io::ErrorKind {
        self.kind
    }
    fn from_io(error: std::io::Error, phase: &str) -> Self {
        Self { kind: error.kind(), message: format!("native {phase}: {error}") }
    }
}
pub type ExecutionResult<T> = Result<T, NativeExecutionError>;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct NativeExecutionControl {
    deadline: Instant,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    output_limit: u64,
    inherited_enclosure: bool,
}
impl NativeExecutionControl {
    pub fn new(deadline: Instant, cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self { deadline, cancelled, output_limit: 16 * 1024 * 1024, inherited_enclosure: false }
    }
    /// Reuse the enclosing producer's containment for a nested compiler driver.
    /// Safety: the caller must retain the owned parent enclosure until this tool
    /// and all of its descendants have settled. No new Unix group is created.
    pub unsafe fn with_inherited_enclosure(mut self) -> Self {
        self.inherited_enclosure = true;
        self
    }
    /// Remaining time is only a nested upper bound; the enclosing absolute
    /// deadline remains authoritative and cannot be restarted by the child.
    pub fn remaining_budget(&self) -> std::time::Duration {
        self.deadline.saturating_duration_since(std::time::Instant::now())
    }
    pub fn check(&self, phase: &str) -> ExecutionResult<()> {
        self.check_io(phase).map_err(|error| NativeExecutionError::from_io(error, phase))
    }
    fn check_io(&self, phase: &str) -> std::io::Result<()> {
        if (self.cancelled)() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                format!("native phase `{phase}` cancelled"),
            ));
        }
        if Instant::now() >= self.deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("native phase `{phase}` absolute deadline exceeded"),
            ));
        }
        Ok(())
    }
    pub fn run_command(&self, command: &mut Command, directory: &Path, phase: &str) -> ExecutionResult<Output> {
        self.run_command_io(command, directory, phase).map_err(|error| NativeExecutionError::from_io(error, phase))
    }
    /// Keep the same absolute deadline while tightening captured output bounds.
    pub fn with_output_limit(mut self, limit: u64) -> ExecutionResult<Self> {
        if limit == 0 || limit > 64 * 1024 * 1024 {
            return Err(NativeExecutionError::from_io(
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "native output limit must be between 1 and 64 MiB",
                ),
                "output policy",
            ));
        }
        self.output_limit = limit;
        Ok(self)
    }
    /// Service bounded nonblocking invocation work on the calling thread. Borrowed
    /// semantic issuers never cross a thread boundary or enter the child process.
    /// The service must return promptly; every return rechecks the same deadline.
    pub fn run_command_serviced(
        &self,
        command: &mut Command,
        directory: &Path,
        phase: &str,
        mut service: impl FnMut() -> std::io::Result<()>,
    ) -> ExecutionResult<Output> {
        self.run_command_serviced_io(command, directory, phase, &mut service)
            .map_err(|error| NativeExecutionError::from_io(error, phase))
    }
    fn run_command_io(&self, command: &mut Command, directory: &Path, phase: &str) -> std::io::Result<Output> {
        self.run_command_serviced_io(command, directory, phase, &mut || Ok(()))
    }
    fn run_command_serviced_io(
        &self,
        command: &mut Command,
        directory: &Path,
        phase: &str,
        service: &mut dyn FnMut() -> std::io::Result<()>,
    ) -> std::io::Result<Output> {
        self.check_io(phase)?;
        let capture = tempfile::tempdir_in(directory)?;
        let stdout = capture.path().join("stdout");
        let stderr = capture.path().join("stderr");
        let mut child = ContainedChild::spawn(
            command
                .stdin(Stdio::null())
                .stdout(Stdio::from(std::fs::File::create(&stdout)?))
                .stderr(Stdio::from(std::fs::File::create(&stderr)?)),
            self.inherited_enclosure,
        )?;
        let status = loop {
            self.check_io(phase)?;
            self.check_output(&stdout, &stderr)?;
            service()?;
            self.check_io(phase)?;
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    child.terminate();
                    return Err(error);
                }
            }
            if let Err(error) = self.check_io(phase) {
                child.terminate();
                return Err(error);
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        child.terminate();
        self.check_io(phase)?;
        self.check_output(&stdout, &stderr)?;
        Ok(Output {
            status,
            stdout: read_capped(&stdout, self.output_limit)?,
            stderr: read_capped(&stderr, self.output_limit)?,
        })
    }
    fn check_output(&self, stdout: &Path, stderr: &Path) -> std::io::Result<()> {
        if std::fs::metadata(stdout)?.len() > self.output_limit || std::fs::metadata(stderr)?.len() > self.output_limit
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "native captured output exceeds policy bounds",
            ));
        }
        Ok(())
    }
}

fn read_capped(path: &Path, limit: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "native captured output exceeds policy bounds",
        ));
    }
    Ok(bytes)
}

/// A spawned process owns its descendant containment until every exit path.
struct ContainedChild {
    child: Child,
    #[cfg(unix)]
    group: Option<i32>,
    #[cfg(windows)]
    job: windows_job::Job,
    terminated: bool,
}
impl ContainedChild {
    fn spawn(command: &mut Command, inherited_enclosure: bool) -> std::io::Result<Self> {
        if !inherited_enclosure {
            command.env("BESKID_EXECUTION_ENCLOSURE", "owned");
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            if !inherited_enclosure {
                command.process_group(0);
            }
            let child = command.spawn()?;
            let group = if inherited_enclosure {
                None
            } else {
                Some(i32::try_from(child.id()).map_err(std::io::Error::other)?)
            };
            Ok(Self { child, group, terminated: false })
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let job = windows_job::Job::new()?;
            command.creation_flags(0x00000004 | 0x00000200);
            let mut child = command.spawn()?;
            if let Err(error) = job.assign_and_resume(&child) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            Ok(Self { child, job, terminated: false })
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (command, inherited_enclosure);
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "native process-tree containment is unavailable on this platform",
            ))
        }
    }
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        self.child.try_wait()
    }
    fn terminate(&mut self) {
        if self.terminated {
            return;
        }
        self.terminated = true;
        #[cfg(unix)]
        // The child created its own group atomically during spawn. Kill the group
        // even after its leader exits, because descendants can retain capture files.
        unsafe {
            if let Some(group) = self.group {
                libc::kill(-group, libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        self.job.terminate();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for ContainedChild {
    fn drop(&mut self) {
        self.terminate();
    }
}

#[cfg(windows)]
mod windows_job {
    use std::{ffi::c_void, io, mem, os::windows::io::AsRawHandle, process::Child};
    type Handle = *mut c_void;
    #[repr(C)]
    struct BasicLimit {
        process_time: i64,
        job_time: i64,
        flags: u32,
        min_working_set: usize,
        max_working_set: usize,
        active_processes: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    struct ExtendedLimit {
        basic: BasicLimit,
        io: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process: usize,
        peak_job: usize,
    }
    #[repr(C)]
    struct ThreadEntry {
        size: u32,
        usage: u32,
        thread: u32,
        process: u32,
        base_priority: i32,
        delta_priority: i32,
        flags: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: Handle, name: *const u16) -> Handle;
        fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, length: u32) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        fn TerminateJobObject(job: Handle, code: u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn CreateToolhelp32Snapshot(flags: u32, process: u32) -> Handle;
        fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
        fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry) -> i32;
        fn OpenThread(access: u32, inherit: i32, thread: u32) -> Handle;
        fn ResumeThread(thread: Handle) -> u32;
    }
    struct OwnedHandle(Handle);
    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    pub(super) struct Job(OwnedHandle);
    impl Job {
        pub(super) fn new() -> io::Result<Self> {
            let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(OwnedHandle(handle));
            let mut limits: ExtendedLimit = unsafe { mem::zeroed() };
            limits.basic.flags = 0x00002000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            if unsafe {
                SetInformationJobObject(
                    handle,
                    9,
                    (&limits as *const ExtendedLimit).cast(),
                    mem::size_of::<ExtendedLimit>() as u32,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }
        pub(super) fn assign_and_resume(&self, child: &Child) -> io::Result<()> {
            if unsafe { AssignProcessToJobObject(self.0.0, child.as_raw_handle()) } == 0 {
                return Err(io::Error::last_os_error());
            }
            // CREATE_SUSPENDED prevents descendants before job assignment. The
            // sole initial thread is discovered while the process is suspended.
            let snapshot = unsafe { CreateToolhelp32Snapshot(0x00000004, 0) };
            if snapshot as isize == -1 {
                return Err(io::Error::last_os_error());
            }
            let snapshot = OwnedHandle(snapshot);
            let mut entry: ThreadEntry = unsafe { mem::zeroed() };
            entry.size = mem::size_of::<ThreadEntry>() as u32;
            let mut found = None;
            let mut present = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
            while present {
                if entry.process == child.id() {
                    if found.replace(entry.thread).is_some() {
                        return Err(io::Error::other("suspended native process has multiple initial threads"));
                    }
                }
                present = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
            }
            let thread =
                found.ok_or_else(|| io::Error::other("suspended native process initial thread was not found"))?;
            let thread = unsafe { OpenThread(0x0002, 0, thread) };
            if thread.is_null() {
                return Err(io::Error::last_os_error());
            }
            let thread = OwnedHandle(thread);
            if unsafe { ResumeThread(thread.0) } == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        pub(super) fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.0.0, 1);
            }
        }
    }
}

mod compiler_driver;
pub use compiler_driver::{
    CompilerDriverConfiguration, CompilerDriverReceipt, CompilerDriverTool, run_compiler_driver,
};
