//! OpenTelemetry-compatible tracing for Beskid compiler binaries.
//!
//! Installs a global [`tracing`] subscriber with:
//! - [`EnvFilter`] (respects `RUST_LOG`)
//! - [`BufferLayer`] — in-memory ring buffer of captured spans and events
//! - optional OTLP export, only when `BESKID_TELEMETRY=1` (the endpoint comes from
//!   `OTEL_EXPORTER_OTLP_ENDPOINT`); call [`shutdown_otel`] before the process exits to flush it
//! - stderr formatting coordinated with bounded, line-oriented progress

mod buffer;
mod init;
mod otel;

pub use buffer::{BufferLayer, TelemetryBuffer, TelemetryEvent, TelemetrySnapshot, TelemetrySpan, telemetry_buffer};
pub use init::{
    InitOptions, Verbosity, clear_stderr_progress, init, init_lsp, render_stderr_progress, set_verbosity,
    shutdown_otel, verbosity,
};
