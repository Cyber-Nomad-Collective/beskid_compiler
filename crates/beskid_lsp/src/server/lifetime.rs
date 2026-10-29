//! When the language server process ends.
//!
//! The server ends the process itself, at once, in three cases: the client sent `exit` (code 0
//! after a `shutdown` request, else 1, as the protocol says), the client closed or broke its
//! stdin, or the client process named by `initialize`'s `processId` is gone (polled every
//! second, as rust-analyzer does). In every case in-flight work (an edit's analysis, the
//! workspace index, a diagnostics publication) is dropped with the process: a server that
//! waited for it kept running for minutes after its editor was gone, holding gigabytes.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use tokio::io::{AsyncRead, ReadBuf};

/// Ends the process with an exit code. The stdio server installs [`std::process::exit`];
/// tests that drive the service in-process install nothing.
pub(crate) type ExitHook = Arc<dyn Fn(i32) + Send + Sync>;

/// The exit code the protocol asks for: 0 when `shutdown` was requested before `exit`, else 1.
#[derive(Default)]
pub(crate) struct ExitState {
    shutdown_requested: AtomicBool,
}

impl ExitState {
    pub(crate) fn shutdown_requested(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
    }

    pub(crate) fn exit_code(&self) -> i32 {
        if self.shutdown_requested.load(Ordering::Acquire) { 0 } else { 1 }
    }
}

/// The server's stdin: its end or a read error ends the process (code 1: the client went away
/// without `exit`).
pub(crate) struct ExitOnEof<R> {
    inner: R,
    exit: ExitHook,
}

impl<R> ExitOnEof<R> {
    pub(crate) fn new(inner: R, exit: ExitHook) -> Self {
        Self { inner, exit }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ExitOnEof<R> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) if buf.filled().len() == before => {
                (this.exit)(1);
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => {
                (this.exit)(1);
                Poll::Ready(Err(error))
            }
            other => other,
        }
    }
}

/// How often the client process is checked.
const PARENT_POLL: Duration = Duration::from_secs(1);

/// Ends the process (code 1) once the client process `pid` is gone. A thread, so it needs no
/// runtime and keeps checking while the runtime is busy.
pub(crate) fn watch_parent(pid: u32, exit: ExitHook) {
    std::thread::Builder::new()
        .name("beskid-lsp-parent-watch".into())
        .spawn(move || {
            loop {
                std::thread::sleep(PARENT_POLL);
                if !process_alive(pid) {
                    exit(1);
                    return;
                }
            }
        })
        .expect("spawn the parent watch thread");
}

/// Whether a process with `pid` exists.
#[cfg(unix)]
pub(crate) fn process_alive(pid: u32) -> bool {
    // SAFETY: signal 0 delivers nothing; it only checks that the process exists.
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    // A process of another user exists but cannot be signalled.
    result == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Whether a process with `pid` exists (Windows: never answered, so the parent is never
/// declared dead; the end of stdin covers a client that went away).
#[cfg(not(unix))]
pub(crate) fn process_alive(_pid: u32) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use tokio::io::AsyncReadExt;

    use super::*;

    fn recording_exit() -> (ExitHook, Arc<Mutex<Vec<i32>>>) {
        let codes = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&codes);
        (Arc::new(move |code| seen.lock().expect("codes").push(code)), codes)
    }

    #[tokio::test]
    async fn the_end_of_stdin_ends_the_process_with_code_1() {
        let (exit, codes) = recording_exit();
        let mut stdin = ExitOnEof::new(std::io::Cursor::new(b"abc".to_vec()), exit);
        let mut out = Vec::new();
        stdin.read_to_end(&mut out).await.expect("read");
        assert_eq!(out, b"abc");
        assert_eq!(*codes.lock().expect("codes"), vec![1], "the end of the input asked for the exit once");
    }

    #[test]
    fn exit_code_follows_the_protocol() {
        let state = ExitState::default();
        assert_eq!(state.exit_code(), 1, "exit without shutdown");
        state.shutdown_requested();
        assert_eq!(state.exit_code(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_gone_process_is_not_alive() {
        assert!(process_alive(std::process::id()));
        let mut child = std::process::Command::new("sleep").arg("30").spawn().expect("spawn sleep");
        let pid = child.id();
        assert!(process_alive(pid));
        child.kill().expect("kill");
        child.wait().expect("reap");
        assert!(!process_alive(pid));
    }
}
