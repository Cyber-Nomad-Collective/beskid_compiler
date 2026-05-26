use beskid_analysis::syntax::SpanInfo;
use beskid_analysis::types::TypeId;

/// Recoverable lowering or CLIF verification failure; map with [`crate::codegen_error_to_diagnostic`].
#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    #[error("unsupported node for kickoff lowering: {node}")]
    UnsupportedNode {
        span: SpanInfo,
        node: &'static str,
    },
    #[error("unsupported feature: {_0}")]
    UnsupportedFeature(&'static str),
    #[error("missing symbol: {_0}")]
    MissingSymbol(&'static str),
    #[error("missing resolved value entry")]
    MissingResolvedValue { span: SpanInfo },
    #[error("missing local type information")]
    MissingLocalType { span: SpanInfo },
    #[error("invalid local binding for kickoff lowering")]
    InvalidLocalBinding { span: SpanInfo },
    #[error("missing expression type information")]
    MissingExpressionType { span: SpanInfo },
    #[error("missing cast intent for numeric mismatch (expected {expected:?}, actual {actual:?})")]
    MissingCastIntent {
        span: SpanInfo,
        expected: TypeId,
        actual: TypeId,
    },
    #[error("invalid cast intent: {message}")]
    InvalidCastIntent { span: SpanInfo, message: String },
    #[error("type mismatch during codegen (expected {expected:?}, actual {actual:?})")]
    TypeMismatch {
        span: SpanInfo,
        expected: TypeId,
        actual: TypeId,
    },
    #[error("CLIF verification failed for `{function}`: {message}")]
    VerificationFailed { function: String, message: String },
    #[error("invalid export: {message}")]
    InvalidExport { span: SpanInfo, message: String },
    #[error("ineligible serialize mapping from `{src_name}` to `{dst_name}`")]
    IneligibleSerializeMapping {
        span: SpanInfo,
        src_name: String,
        dst_name: String,
    },
}
