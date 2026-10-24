//! Typed errors for ISA setup, object emission, runtime preparation, and linking.

use std::path::PathBuf;

use crate::api::BuildOutputKind;

/// Result alias used across the AOT crate.
pub type AotResult<T> = Result<T, AotError>;

/// Failure modes for the AOT pipeline (codes in `thiserror` messages align with compiler diagnostics).
#[derive(Debug, thiserror::Error)]
pub enum AotError {
    #[error("[E4001] ISA initialization failed: {message}")]
    IsaInit { message: String },

    #[error("[E4002] Object module error: {message}")]
    ObjectModule { message: String },

    #[error("[E4003] Missing function during object finalize: {name}")]
    MissingFunction { name: String },

    #[error("[E4010] Runtime build failed: {message}")]
    RuntimeBuild { message: String },

    #[error("[E4011] Runtime archive not found at: {path}")]
    RuntimeArchiveMissing { path: PathBuf },

    #[error("[E4012] Runtime ABI version mismatch (expected {expected}, got {actual})")]
    RuntimeAbiMismatch { expected: u32, actual: u32 },

    #[error(
        "[E4020] Linker tool not available: no host linker could be started (on Windows, install the Visual \
         Studio Build Tools `Desktop development with C++` workload including MSVC x64 tools and \
         the Windows SDK so `link.exe` and import libraries are available)"
    )]
    LinkerUnavailable,

    /// A native assembler or C compiler required by the runtime/bootstrap objects could not be
    /// spawned. Names the tool so a missing PATH entry (for example LLVM's `llvm-ml`/`clang` on
    /// Windows) is not reported as an anonymous linker failure.
    #[error("[E4020] Native build tool `{tool}` not available: {message}")]
    NativeToolUnavailable { tool: String, message: String },

    #[error("[E4021] Link step failed (exit {status}): {command}{detail}")]
    LinkFailed { status: i32, command: String, detail: String },

    #[error("[E4022] Unsupported output kind for target {target}: {kind:?}")]
    UnsupportedOutputKind { target: String, kind: BuildOutputKind },

    #[error("[E4023] Unsupported linker strategy for target {target}: {message}")]
    UnsupportedLinkerStrategy { target: String, message: String },

    #[error("[E4030] Entrypoint symbol not found: {symbol}")]
    MissingEntrypoint { symbol: String },

    #[error("[E4040] IO error at {path}: {message}")]
    Io { path: PathBuf, message: String },

    #[error("[E4041] Invalid build request: {message}")]
    InvalidRequest { message: String },

    /// Lowering stopped on structured compiler diagnostics (user errors or coded internal errors
    /// with their sites). The bundle is the error's source, so a reporter that walks the error
    /// chain renders each diagnostic with its code, label, help and source excerpt.
    #[error("the build stopped on compiler diagnostics")]
    Diagnostics(#[source] beskid_analysis::services::SemanticDiagnosticsError),

    /// A compiler inconsistency that reached the build only as text (an unavailable semantic
    /// fact behind a stringified upstream error). Never a user request error.
    #[error("[E2105] internal compiler error: the build stopped on an inconsistency in the compiler; please report this ({reason})")]
    Internal { reason: String },

    #[error("[E4050] unresolved extern library `{library}` for symbol `{symbol}`")]
    UnresolvedExternLibrary { library: String, symbol: String },

    #[error("[E4060] Failed to run executable at `{path}`: {message}")]
    RunFailed { path: PathBuf, message: String },

    #[error("[E4061] Executable timed out after {seconds}s: `{path}`")]
    RunTimedOut { path: PathBuf, seconds: u64 },
}

impl AotError {
    /// Classify a lowering failure from the codegen boundary. Structured diagnostics stay
    /// structured; text that carries the compiler-gap marker of an unavailable semantic fact is an
    /// internal compiler error; anything else is a request the build cannot satisfy.
    pub fn from_lowering(error: anyhow::Error) -> Self {
        if let Some(bundle) = error.downcast_ref::<beskid_analysis::services::SemanticDiagnosticsError>() {
            return Self::Diagnostics(bundle.clone());
        }
        if let Some(wrapped) = error.downcast_ref::<beskid_analysis::analysis::MietteReportError>() {
            return Self::Diagnostics(beskid_analysis::services::SemanticDiagnosticsError::from_diagnostics(vec![
                wrapped.diagnostic().clone(),
            ]));
        }
        let text = format!("{error:#}");
        if text.contains(beskid_queries::UNAVAILABLE_MESSAGE_MARKER) {
            Self::Internal { reason: text }
        } else {
            Self::InvalidRequest { message: text }
        }
    }

    /// The structured diagnostics this error carries, if any.
    pub fn diagnostics(&self) -> Option<&beskid_analysis::services::SemanticDiagnosticsError> {
        match self {
            Self::Diagnostics(bundle) => Some(bundle),
            _ => None,
        }
    }
}

impl From<cranelift_module::ModuleError> for AotError {
    fn from(value: cranelift_module::ModuleError) -> Self {
        Self::ObjectModule { message: value.to_string() }
    }
}

impl From<beskid_execution::NativeExecutionError> for AotError {
    fn from(error: beskid_execution::NativeExecutionError) -> Self {
        Self::InvalidRequest { message: error.to_string() }
    }
}

#[cfg(test)]
mod lowering_classification_tests {
    use super::AotError;

    #[test]
    fn structured_lowering_diagnostics_stay_structured() {
        let diagnostic = beskid_analysis::analysis::diagnostics::make_diagnostic(
            "Main.bd",
            "i32 Main() { return 0; }",
            beskid_analysis::syntax::SpanInfo { start: 0, end: 3, line_col_start: (1, 1), line_col_end: (1, 4) },
            "internal compiler error: the compiler cannot lower this construct yet; please report this",
            "the compiler failed here",
            None,
            Some("E2102".to_string()),
            beskid_analysis::analysis::Severity::Error,
        );
        let bundle = beskid_analysis::services::SemanticDiagnosticsError::from_diagnostics(vec![diagnostic]);
        let error = AotError::from_lowering(anyhow::Error::new(bundle).context("syntax ISLE lowering failed"));
        let bundle = error.diagnostics().expect("diagnostics survive the AOT boundary");
        assert_eq!(bundle.diagnostics()[0].code.as_deref(), Some("E2102"));
        assert!(std::error::Error::source(&error).is_some(), "the bundle is the error's source");
    }

    #[test]
    fn a_stringified_compiler_gap_is_an_internal_error_not_an_invalid_request() {
        let gap = anyhow::anyhow!(
            "semantic query `callable_signature` {}",
            beskid_queries::UNAVAILABLE_MESSAGE_MARKER
        )
        .context("syntax program preparation failed");
        let error = AotError::from_lowering(gap);
        let text = error.to_string();
        assert!(text.starts_with("[E2105] internal compiler error: "), "{text}");
        assert!(!text.contains("E4041"), "{text}");
        let request = AotError::from_lowering(anyhow::anyhow!("Missing entrypoint `Main`"));
        assert!(request.to_string().starts_with("[E4041]"), "{request}");
    }
}
