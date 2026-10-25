//! Global [`tracing`] subscriber wiring for Beskid binaries.

use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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

/// Process-wide amount of human progress and log output, selected once by `-q` / `-v` / `-vv`.
///
/// Default output carries warnings and errors only. Variants are ordered from least to most output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Verbosity {
    /// Errors, diagnostics and command results only: no status lines, progress or logs below error.
    Quiet,
    /// Warnings, errors, at most one status line per command and TTY-only progress for slow steps.
    #[default]
    Normal,
    /// `-v`: every pipeline phase line, every work unit and info-level logs.
    Verbose,
    /// `-vv`: as [`Verbosity::Verbose`] plus debug-level Beskid logs.
    Debug,
}

impl Verbosity {
    /// Select from the `-q` flag and the number of `-v` occurrences; `-q` wins.
    pub fn from_flags(quiet: bool, verbose: u8) -> Self {
        match (quiet, verbose) {
            (true, _) => Verbosity::Quiet,
            (false, 0) => Verbosity::Normal,
            (false, 1) => Verbosity::Verbose,
            (false, _) => Verbosity::Debug,
        }
    }

    /// Whether the single per-command status line (for example `Build complete in 2.4s`) is shown.
    pub fn shows_status(self) -> bool {
        self >= Verbosity::Normal
    }

    /// Whether phase trees, work units and info-level logs are shown (`-v` and above).
    pub fn shows_detail(self) -> bool {
        self >= Verbosity::Verbose
    }
}

static VERBOSITY: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(1);

/// Select the process verbosity. [`init`] calls this; it is public for binaries that print progress
/// without a subscriber.
pub fn set_verbosity(verbosity: Verbosity) {
    let value = match verbosity {
        Verbosity::Quiet => 0,
        Verbosity::Normal => 1,
        Verbosity::Verbose => 2,
        Verbosity::Debug => 3,
    };
    VERBOSITY.store(value, std::sync::atomic::Ordering::Relaxed);
}

/// The process verbosity; [`Verbosity::Normal`] until selected.
pub fn verbosity() -> Verbosity {
    match VERBOSITY.load(std::sync::atomic::Ordering::Relaxed) {
        0 => Verbosity::Quiet,
        2 => Verbosity::Verbose,
        3 => Verbosity::Debug,
        _ => Verbosity::Normal,
    }
}

/// Options for [`init`].
pub struct InitOptions {
    pub log_cranelift: bool,
    pub service_name: &'static str,
    pub verbosity: Verbosity,
    /// Write logs to this file instead of stderr (`--log-file` / `BESKID_LOG`).
    pub log_file: Option<PathBuf>,
}

impl InitOptions {
    pub fn cli(log_cranelift: bool) -> Self {
        Self { log_cranelift, service_name: "beskid", verbosity: Verbosity::Normal, log_file: None }
    }

    pub fn lsp() -> Self {
        Self { log_cranelift: false, service_name: "beskid-lsp", verbosity: Verbosity::Normal, log_file: None }
    }

    pub fn with_verbosity(self, verbosity: Verbosity) -> Self {
        Self { verbosity, ..self }
    }

    pub fn with_log_file(self, log_file: Option<PathBuf>) -> Self {
        Self { log_file, ..self }
    }
}

/// Default `RUST_LOG`-style filter; an explicit `RUST_LOG` replaces it.
///
/// Default output keeps warnings and errors only, so info lines such as the subscriber banner and
/// Salsa snapshot timings need `-v`; `-vv` adds debug-level Beskid logs.
pub(crate) fn default_filter(log_cranelift: bool, verbosity: Verbosity) -> String {
    let base = match verbosity {
        Verbosity::Quiet => return "error".to_string(),
        Verbosity::Normal => return "warn".to_string(),
        Verbosity::Verbose => BASE_FILTER.to_string(),
        Verbosity::Debug => BASE_FILTER.replace("=info", "=debug"),
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

/// Where log events go: the progress-aware stderr writer, or a shared append-only file.
#[derive(Clone)]
enum LogSink {
    Stderr,
    File(Arc<Mutex<File>>),
}

struct LogFileWriter(Arc<Mutex<File>>);

impl Write for LogFileWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).flush()
    }
}

enum SinkWriter {
    Stderr(ProgressStderrWriter),
    File(LogFileWriter),
}

impl Write for SinkWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            SinkWriter::Stderr(writer) => writer.write(buf),
            SinkWriter::File(writer) => writer.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            SinkWriter::Stderr(writer) => writer.flush(),
            SinkWriter::File(writer) => writer.flush(),
        }
    }
}

impl<'a> MakeWriter<'a> for LogSink {
    type Writer = SinkWriter;

    fn make_writer(&'a self) -> Self::Writer {
        match self {
            LogSink::Stderr => SinkWriter::Stderr(ProgressStderrWriter),
            LogSink::File(file) => SinkWriter::File(LogFileWriter(Arc::clone(file))),
        }
    }
}

/// Open `path` for appending, creating parent directories. A path that cannot be opened is an error
/// the caller reports; logs never silently fall back to stderr.
fn open_log_file(path: &std::path::Path) -> io::Result<LogSink> {
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    Ok(LogSink::File(Arc::new(Mutex::new(file))))
}

macro_rules! log_fmt_layer {
    ($sink:expr) => {
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer($sink)
            .with_target(true)
            .with_thread_ids(true)
            .with_level(true)
    };
}

fn install_local(filter: EnvFilter, buffer_layer: BufferLayer, sink: LogSink) {
    tracing_subscriber::registry().with(filter).with(buffer_layer).with(log_fmt_layer!(sink)).init();
}

fn install_otel(filter: EnvFilter, buffer_layer: BufferLayer, sink: LogSink, guard: &OtelGuard) {
    let otel_layer = tracing_opentelemetry::layer().with_tracer(otel_tracer(guard));
    tracing_subscriber::registry().with(filter).with(buffer_layer).with(log_fmt_layer!(sink)).with(otel_layer).init();
}

fn install_for_scope(filter: EnvFilter, buffer_layer: BufferLayer, sink: LogSink, otel_guard: Option<OtelGuard>) {
    match otel_guard {
        Some(guard) => {
            install_otel(filter, buffer_layer, sink, &guard);
            let _leaked = Box::leak(Box::new(guard));
        }
        None => {
            install_local(filter, buffer_layer, sink);
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

    let sink = match options.log_file.as_deref().map(open_log_file) {
        None => LogSink::Stderr,
        Some(Ok(sink)) => sink,
        Some(Err(err)) => {
            eprintln!("beskid: cannot open the log file ({err}); logging to stderr");
            LogSink::Stderr
        }
    };
    install_for_scope(filter, buffer_layer, sink, otel_guard);
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
    fn v067_verbosity_selects_the_default_filter() {
        assert_eq!(default_filter(false, Verbosity::Quiet), "error");
        assert_eq!(default_filter(false, Verbosity::Normal), "warn");
        assert_eq!(default_filter(true, Verbosity::Normal), "warn");
        let verbose = default_filter(false, Verbosity::Verbose);
        assert!(
            verbose.starts_with("info,")
                && verbose.contains("beskid_tools=info")
                && verbose.contains("cranelift_codegen=warn")
        );
        let debug = default_filter(false, Verbosity::Debug);
        assert!(
            debug.contains("beskid_tools=debug")
                && debug.contains("beskid_runtime=warn")
                && debug.contains("salsa=warn")
        );
        assert!(!default_filter(true, Verbosity::Debug).contains("cranelift_codegen=warn"));
    }

    #[test]
    fn v067_flags_map_to_verbosity() {
        assert_eq!(Verbosity::from_flags(false, 0), Verbosity::Normal);
        assert_eq!(Verbosity::from_flags(false, 1), Verbosity::Verbose);
        assert_eq!(Verbosity::from_flags(false, 2), Verbosity::Debug);
        assert_eq!(Verbosity::from_flags(false, 5), Verbosity::Debug);
        assert_eq!(Verbosity::from_flags(true, 0), Verbosity::Quiet);
        assert_eq!(Verbosity::from_flags(true, 2), Verbosity::Quiet);
    }

    #[test]
    fn v067_line_kinds_follow_verbosity() {
        assert!(!Verbosity::Quiet.shows_status() && !Verbosity::Quiet.shows_detail());
        assert!(Verbosity::Normal.shows_status() && !Verbosity::Normal.shows_detail());
        assert!(Verbosity::Verbose.shows_status() && Verbosity::Verbose.shows_detail());
        assert!(Verbosity::Debug.shows_status() && Verbosity::Debug.shows_detail());
    }

    #[test]
    fn v067_log_file_receives_appended_lines_and_creates_parents() {
        let root = std::env::temp_dir().join(format!("beskid-telemetry-log-{}", std::process::id()));
        let path = root.join("nested").join("run.log");
        let sink = open_log_file(&path).expect("log file opens");
        let mut writer = sink.make_writer();
        writer.write_all(b"first\n").expect("write");
        writer.flush().expect("flush");
        let again = open_log_file(&path).expect("log file reopens for append");
        again.make_writer().write_all(b"second\n").expect("append");
        assert_eq!(std::fs::read_to_string(&path).expect("read log"), "first\nsecond\n");
        let _ = std::fs::remove_dir_all(&root);
    }
}
