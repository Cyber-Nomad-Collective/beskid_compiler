//! CLI tracing: [`tracing`] subscriber with optional [`tui_logger`] TUI sink.
//!
//! Default filter keeps Cranelift quiet unless `--log-cranelift` or `RUST_LOG` is set.
//! Interactive pipeline UI routes events through [`TuiTracingSubscriberLayer`] into the
//! build-log panel; plain mode prints the same stream to stderr.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

const CRANELIFT_QUIET: &str =
    "cranelift_jit=warn,cranelift_codegen=warn,cranelift_frontend=warn,cranelift_module=warn,cranelift_native=warn,cranelift_object=warn";

static TUI_LOG_SINK_ACTIVE: AtomicBool = AtomicBool::new(false);

struct GatedStderr;

impl Write for GatedStderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if TUI_LOG_SINK_ACTIVE.load(Ordering::Relaxed) {
            return Ok(buf.len());
        }
        io::stderr().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        if TUI_LOG_SINK_ACTIVE.load(Ordering::Relaxed) {
            return Ok(());
        }
        io::stderr().flush()
    }
}

fn default_filter(log_cranelift: bool) -> String {
    let base = "info,beskid_pipeline=info,beskid_tools=info,beskid_queries=info,salsa=warn";
    if log_cranelift {
        base.to_string()
    } else {
        format!("{base},{CRANELIFT_QUIET}")
    }
}

/// Install the global [`tracing`] subscriber (once per process).
///
/// When `RUST_LOG` is unset, Cranelift crates default to `warn` unless `log_cranelift` is true.
pub fn init(log_cranelift: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&default_filter(log_cranelift)));

    tracing_subscriber::registry()
        .with(filter)
        .with(tui_logger::TuiTracingSubscriberLayer)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(|| GatedStderr)
                .with_target(true)
                .without_time(),
        )
        .init();

    let _ = tracing_log::LogTracer::init();
}

/// Start draining log events into the tui-logger buffer (interactive pipeline UI).
pub fn activate_tui_log_sink() {
    TUI_LOG_SINK_ACTIVE.store(true, Ordering::Relaxed);
    let _ = tui_logger::init_logger(log::LevelFilter::Trace);
    tui_logger::set_default_level(log::LevelFilter::Trace);
}

/// Resume stderr formatting when leaving alternate-screen TUI mode.
pub fn deactivate_tui_log_sink() {
    TUI_LOG_SINK_ACTIVE.store(false, Ordering::Relaxed);
}

/// Default circular-buffer depth for [`tui_logger`] (matches crate default).
const TUI_LOG_BUFFER_DEPTH: usize = 10_000;

/// Drop buffered log lines (e.g. when switching pipeline → test mode).
pub fn clear_tui_log_buffer() {
    tui_logger::move_events();
    tui_logger::set_buffer_depth(TUI_LOG_BUFFER_DEPTH);
}
