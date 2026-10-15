//! Global [`tracing`] subscriber wiring for Beskid binaries.

use std::io::{self, Write};
use std::sync::Mutex;

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::buffer::BufferLayer;
use crate::otel::{OtelGuard, install_otel_guard, otel_enabled, otel_tracer};

const CRANELIFT_QUIET: &str = "cranelift_jit=warn,cranelift_codegen=warn,cranelift_frontend=warn,cranelift_module=warn,cranelift_native=warn,cranelift_object=warn";

static STDERR_PROGRESS: Mutex<bool> = Mutex::new(false);

fn clear_progress(out: &mut impl Write, visible: &mut bool) -> io::Result<()> {
    if *visible {
        write!(out, "\r\x1b[2K")?;
        *visible = false;
    }
    Ok(())
}

/// Render one bounded progress line without allowing tracing to append to it.
pub fn render_stderr_progress(line: &str) -> io::Result<()> {
    let mut visible = STDERR_PROGRESS.lock().expect("stderr progress mutex poisoned");
    let mut out = io::stderr().lock();
    write!(out, "\r\x1b[2K{line}")?;
    *visible = true;
    out.flush()
}

/// Clear an active progress line before ordinary command or diagnostic output.
pub fn clear_stderr_progress() -> io::Result<()> {
    let mut visible = STDERR_PROGRESS.lock().expect("stderr progress mutex poisoned");
    let mut out = io::stderr().lock();
    clear_progress(&mut out, &mut visible)?;
    out.flush()
}

struct ProgressStderr;

struct ProgressStderrWriter;

impl Write for ProgressStderrWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut visible = STDERR_PROGRESS.lock().expect("stderr progress mutex poisoned");
        let mut out = io::stderr().lock();
        clear_progress(&mut out, &mut visible)?;
        out.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}

impl<'a> MakeWriter<'a> for ProgressStderr {
    type Writer = ProgressStderrWriter;

    fn make_writer(&'a self) -> Self::Writer {
        ProgressStderrWriter
    }
}

/// Process-wide amount of human progress and log output, selected once by `--quiet` / `--verbose`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
    /// Errors, diagnostics and command results only: no progress, summaries or info logs.
    Quiet,
    #[default]
    Normal,
    /// Every progress work unit and debug-level Beskid logs.
    Verbose,
}

static VERBOSITY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

/// Select the process verbosity. [`init`] calls this; it is public for binaries that print progress
/// without a subscriber.
pub fn set_verbosity(verbosity: Verbosity) {
    let value = match verbosity {
        Verbosity::Quiet => 0,
        Verbosity::Normal => 1,
        Verbosity::Verbose => 2,
    };
    VERBOSITY.store(value, std::sync::atomic::Ordering::Relaxed);
}

/// The process verbosity; [`Verbosity::Normal`] until selected.
pub fn verbosity() -> Verbosity {
    match VERBOSITY.load(std::sync::atomic::Ordering::Relaxed) {
        0 => Verbosity::Quiet,
        2 => Verbosity::Verbose,
        _ => Verbosity::Normal,
    }
}

/// Options for [`init`].
pub struct InitOptions {
    pub log_cranelift: bool,
    pub service_name: &'static str,
    pub verbosity: Verbosity,
}

impl InitOptions {
    pub fn cli(log_cranelift: bool) -> Self {
        Self { log_cranelift, service_name: "beskid", verbosity: Verbosity::Normal }
    }

    pub fn lsp() -> Self {
        Self { log_cranelift: false, service_name: "beskid-lsp", verbosity: Verbosity::Normal }
    }

    pub fn with_verbosity(self, verbosity: Verbosity) -> Self {
        Self { verbosity, ..self }
    }
}

/// Default `RUST_LOG`-style filter; an explicit `RUST_LOG` replaces it.
pub(crate) fn default_filter(log_cranelift: bool, verbosity: Verbosity) -> String {
    let base = match verbosity {
        Verbosity::Quiet => return "error".to_string(),
        Verbosity::Normal => BASE_FILTER.to_string(),
        Verbosity::Verbose => BASE_FILTER.replace("=info", "=debug"),
    };
    if log_cranelift { base } else { format!("{base},{CRANELIFT_QUIET}") }
}

const BASE_FILTER: &str = "info,\
        beskid_pipeline=info,\
        beskid_analysis=info,\
        beskid_codegen=info,\
        beskid_engine=info,\
        beskid_queries=info,\
        beskid_tools=info,\
        beskid_lsp=info,\
        beskid_aot=info,\
        beskid_runtime=warn,\
        beskid_telemetry=info,\
        salsa=warn";

macro_rules! stderr_fmt_layer {
    () => {
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(ProgressStderr)
            .with_target(true)
            .with_thread_ids(true)
            .with_level(true)
    };
}

fn install_local(filter: EnvFilter, buffer_layer: BufferLayer) {
    tracing_subscriber::registry().with(filter).with(buffer_layer).with(stderr_fmt_layer!()).init();
}

fn install_otel(filter: EnvFilter, buffer_layer: BufferLayer, guard: &OtelGuard) {
    let otel_layer = tracing_opentelemetry::layer().with_tracer(otel_tracer(guard));
    tracing_subscriber::registry().with(filter).with(buffer_layer).with(stderr_fmt_layer!()).with(otel_layer).init();
}

fn install_for_scope(filter: EnvFilter, buffer_layer: BufferLayer, otel_guard: Option<OtelGuard>) {
    match otel_guard {
        Some(guard) => {
            install_otel(filter, buffer_layer, &guard);
            let _leaked = Box::leak(Box::new(guard));
        }
        None => {
            install_local(filter, buffer_layer);
        }
    }
}

/// Install the global subscriber (once per process).
pub fn init(options: InitOptions) {
    set_verbosity(options.verbosity);
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_filter(options.log_cranelift, options.verbosity)));
    let buffer_layer = BufferLayer::global();

    let otel_guard = if otel_enabled() {
        match install_otel_guard(options.service_name) {
            Ok(guard) => Some(guard),
            Err(err) => {
                eprintln!("beskid telemetry: OTLP disabled ({err}); continuing with local buffer only");
                None
            }
        }
    } else {
        None
    };

    install_for_scope(filter, buffer_layer, otel_guard);
    post_init(&options);
}

fn post_init(options: &InitOptions) {
    let _ = tracing_log::LogTracer::init();
    tracing::info!(
        target: "beskid.telemetry",
        service = options.service_name,
        otel = otel_enabled(),
        "tracing subscriber initialized"
    );
}

/// LSP entry: same subscriber, `beskid-lsp` service name.
pub fn init_lsp() {
    init(InitOptions::lsp());
}

pub fn shutdown_otel() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v06_verbosity_selects_the_default_filter() {
        assert_eq!(default_filter(false, Verbosity::Quiet), "error");
        let normal = default_filter(false, Verbosity::Normal);
        assert!(normal.starts_with("info,") && normal.contains("beskid_tools=info") && normal.contains("cranelift_codegen=warn"));
        let verbose = default_filter(false, Verbosity::Verbose);
        assert!(verbose.contains("beskid_tools=debug") && verbose.contains("beskid_runtime=warn") && verbose.contains("salsa=warn"));
        assert!(!default_filter(true, Verbosity::Verbose).contains("cranelift_codegen=warn"));
    }
}
