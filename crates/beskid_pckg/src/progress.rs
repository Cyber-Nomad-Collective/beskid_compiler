//! Simple stderr upload progress (replaces indicatif for pckg publish).

use std::io::{self, Write};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, ReadBuf};

#[derive(Clone)]
pub struct UploadProgress {
    total: u64,
    done: Arc<AtomicU64>,
    last_draw: Arc<std::sync::Mutex<Instant>>,
}

impl UploadProgress {
    pub fn new(total: u64) -> Self {
        Self {
            total,
            done: Arc::new(AtomicU64::new(0)),
            last_draw: Arc::new(std::sync::Mutex::new(Instant::now())),
        }
    }

    pub fn wrap_async_read<R>(&self, inner: R) -> ProgressReader<R>
    where
        R: AsyncRead + Send,
    {
        ProgressReader {
            inner,
            progress: self.clone(),
        }
    }

    fn note_bytes(&self, n: u64) {
        let done = self.done.fetch_add(n, Ordering::Relaxed) + n;
        let mut last = self.last_draw.lock().expect("upload progress lock");
        if last.elapsed() < Duration::from_millis(100) && done < self.total {
            return;
        }
        *last = Instant::now();
        let pct = if self.total == 0 {
            100
        } else {
            ((done.saturating_mul(100)) / self.total).min(100)
        };
        let bar_width = 30usize;
        let filled = ((pct as usize).saturating_mul(bar_width)) / 100;
        let bar: String = "#".repeat(filled) + &"-".repeat(bar_width.saturating_sub(filled));
        let _ = write!(
            io::stderr(),
            "\ruploading [{bar}] {done}/{total} ({pct}%)",
            total = self.total
        );
        let _ = io::stderr().flush();
        if done >= self.total {
            let _ = writeln!(io::stderr());
        }
    }
}

pub struct ProgressReader<R> {
    inner: R,
    progress: UploadProgress,
}

impl<R: AsyncRead + Unpin> AsyncRead for ProgressReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let poll = Pin::new(&mut self.inner).poll_read(cx, buf);
        if poll.is_ready() {
            let read = (buf.filled().len().saturating_sub(before)) as u64;
            if read > 0 {
                self.progress.note_bytes(read);
            }
        }
        poll
    }
}
