//! Global [`tracing`] subscriber wiring for Beskid binaries.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::buffer::BufferLayer;
use crate::otel::{OtelGuard, install_otel_guard, otel_enabled, otel_tracer};

const CRANELIFT_QUIET: &str = "cranelift_jit=warn,cranelift_codegen=warn,cranelift_frontend=warn,cranelift_module=warn,cranelift_native=warn,cranelift_object=warn";

static STDERR_GATED: AtomicBool = AtomicBool::new(false);

struct GatedStderr;

struct GatedStderrWriter;

impl Write for GatedStderrWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if STDERR_GATED.load(Ordering::Relaxed) {
            return Ok(buf.len());
        }
        io::stderr().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        if STDERR_GATED.load(Ordering::Relaxed) {
            return Ok(());
        }
        io::stderr().flush()
    }
}

impl<'a> MakeWriter<'a> for GatedStderr {
    type Writer = GatedStderrWriter;

    fn make_writer(&'a self) -> Self::Writer {
        GatedStderrWriter
    }
}

/// Options for [`init`].
pub struct InitOptions {
    pub log_cranelift: bool,
    pub service_name: &'static str,
    pub include_tui_logger: bool,
}

impl InitOptions {
    pub fn cli(log_cranelift: bool) -> Self {
        Self { log_cranelift, service_name: "beskid", include_tui_logger: true }
    }

    pub fn lsp() -> Self {
        Self { log_cranelift: false, service_name: "beskid-lsp", include_tui_logger: false }
    }
}

fn default_filter(log_cranelift: bool) -> String {
    let base = "info,\
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
    if log_cranelift { base.to_string() } else { format!("{base},{CRANELIFT_QUIET}") }
}

macro_rules! stderr_fmt_layer {
    () => {
        tracing_subscriber::fmt::layer()
            .with_writer(GatedStderr)
            .with_target(true)
            .with_thread_ids(true)
            .with_level(true)
    };
}

fn install_local(filter: EnvFilter, buffer_layer: BufferLayer) {
    tracing_subscriber::registry().with(filter).with(buffer_layer).with(stderr_fmt_layer!()).init();
}

#[cfg(feature = "tui")]
fn install_local_with_tui(filter: EnvFilter, buffer_layer: BufferLayer) {
    tracing_subscriber::registry()
        .with(filter)
        .with(buffer_layer)
        .with(tui_logger::TuiTracingSubscriberLayer)
        .with(stderr_fmt_layer!())
        .init();
}

fn install_otel(filter: EnvFilter, buffer_layer: BufferLayer, guard: &OtelGuard) {
    let otel_layer = tracing_opentelemetry::layer().with_tracer(otel_tracer(guard));
    tracing_subscriber::registry().with(filter).with(buffer_layer).with(stderr_fmt_layer!()).with(otel_layer).init();
}

#[cfg(feature = "tui")]
fn install_otel_with_tui(filter: EnvFilter, buffer_layer: BufferLayer, guard: &OtelGuard) {
    let otel_layer = tracing_opentelemetry::layer().with_tracer(otel_tracer(guard));
    tracing_subscriber::registry()
        .with(filter)
        .with(buffer_layer)
        .with(tui_logger::TuiTracingSubscriberLayer)
        .with(stderr_fmt_layer!())
        .with(otel_layer)
        .init();
}

fn install_for_scope(
    filter: EnvFilter,
    buffer_layer: BufferLayer,
    _include_tui_logger: bool,
    otel_guard: Option<OtelGuard>,
) {
    match otel_guard {
        Some(guard) => {
            #[cfg(feature = "tui")]
            if _include_tui_logger {
                install_otel_with_tui(filter, buffer_layer, &guard);
                let _leaked = Box::leak(Box::new(guard));
                return;
            }
            install_otel(filter, buffer_layer, &guard);
            let _leaked = Box::leak(Box::new(guard));
        }
        None => {
            #[cfg(feature = "tui")]
            if _include_tui_logger {
                install_local_with_tui(filter, buffer_layer);
                return;
            }
            install_local(filter, buffer_layer);
        }
    }
}

/// Install the global subscriber (once per process).
pub fn init(options: InitOptions) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter(options.log_cranelift)));
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

    install_for_scope(filter, buffer_layer, options.include_tui_logger, otel_guard);
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

/// Gate stderr fmt layer while interactive TUI owns the terminal.
pub fn gate_stderr_logging(gated: bool) {
    STDERR_GATED.store(gated, Ordering::Relaxed);
}

pub fn shutdown_otel() {}
