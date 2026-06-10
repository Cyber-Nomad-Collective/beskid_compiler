//! OpenTelemetry-compatible tracing for Beskid compiler binaries.
//!
//! Installs a global [`tracing`] subscriber with:
//! - [`EnvFilter`] (respects `RUST_LOG`)
//! - [`BufferLayer`] — in-memory ring buffer for the hi developer trace widget
//! - optional OTLP export when `OTEL_EXPORTER_OTLP_ENDPOINT` is set
//! - stderr formatting (unless a TUI sink gates it)

mod buffer;
mod init;
mod otel;

pub use buffer::{
    BufferLayer, TelemetryBuffer, TelemetryEvent, TelemetrySnapshot, TelemetrySpan,
    telemetry_buffer,
};
pub use init::{gate_stderr_logging, init, init_lsp, shutdown_otel, InitOptions};
