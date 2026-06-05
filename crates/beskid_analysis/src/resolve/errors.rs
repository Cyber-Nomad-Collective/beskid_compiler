//! [`ResolveError`] and [`ResolveWarning`] for the [`crate::resolve::Resolver`] pass.

use std::fmt;

use crate::syntax::SpanInfo;

fn span_loc(span: SpanInfo) -> String {
    format!(
        "{}:{}-{}:{}",
        span.line_col_start.0, span.line_col_start.1, span.line_col_end.0, span.line_col_end.1
    )
}

/// Failed binding of a path, type, module segment, or visibility; returned as a batch from [`crate::resolve::Resolver::resolve_program`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    DuplicateItem {
        name: String,
        span: SpanInfo,
        previous: SpanInfo,
    },
    DuplicateSymbol {
        symbol: String,
        span: SpanInfo,
        previous: SpanInfo,
    },
    DuplicateLocal {
        name: String,
        span: SpanInfo,
        previous: SpanInfo,
    },
    UnknownValue {
        name: String,
        span: SpanInfo,
    },
    UnknownType {
        name: String,
        span: SpanInfo,
    },
    UnknownModulePath {
        path: String,
        span: SpanInfo,
    },
    UnknownValueInModule {
        module_path: String,
        name: String,
        span: SpanInfo,
    },
    UnknownTypeInModule {
        module_path: String,
        name: String,
        span: SpanInfo,
    },
    InvalidConformanceTarget {
        name: String,
        span: SpanInfo,
    },
    PrivateItemInModule {
        module_path: String,
        name: String,
        span: SpanInfo,
    },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::DuplicateItem {
                name,
                span,
                previous,
            } => write!(
                f,
                "duplicate item `{name}` at {} (previous at {})",
                span_loc(*span),
                span_loc(*previous)
            ),
            ResolveError::DuplicateLocal {
                name,
                span,
                previous,
            } => write!(
                f,
                "duplicate local `{name}` at {} (previous at {})",
                span_loc(*span),
                span_loc(*previous)
            ),
            ResolveError::DuplicateSymbol {
                symbol,
                span,
                previous,
            } => write!(
                f,
                "duplicate symbol `{symbol}` at {} (previous at {})",
                span_loc(*span),
                span_loc(*previous)
            ),
            ResolveError::UnknownValue { name, span } => {
                write!(f, "unknown value `{name}` at {}", span_loc(*span))
            }
            ResolveError::UnknownType { name, span } => {
                write!(f, "unknown type `{name}` at {}", span_loc(*span))
            }
            ResolveError::UnknownModulePath { path, span } => {
                write!(f, "unknown module path `{path}` at {}", span_loc(*span))
            }
            ResolveError::UnknownValueInModule {
                module_path,
                name,
                span,
            } => write!(
                f,
                "unknown value `{name}` in module `{module_path}` at {}",
                span_loc(*span)
            ),
            ResolveError::UnknownTypeInModule {
                module_path,
                name,
                span,
            } => write!(
                f,
                "unknown type `{name}` in module `{module_path}` at {}",
                span_loc(*span)
            ),
            ResolveError::InvalidConformanceTarget { name, span } => write!(
                f,
                "invalid conformance target `{name}` at {}",
                span_loc(*span)
            ),
            ResolveError::PrivateItemInModule {
                module_path,
                name,
                span,
            } => write!(
                f,
                "private item `{name}` in module `{module_path}` at {}",
                span_loc(*span)
            ),
        }
    }
}

/// Non-fatal issues such as shadowed locals (kept separate from hard errors).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveWarning {
    ShadowedLocal {
        name: String,
        span: SpanInfo,
        previous: SpanInfo,
    },
}

/// `Ok` only when the resolver collected zero [`ResolveError`] (warnings may still be present).
pub type ResolveResult<T> = Result<T, Vec<ResolveError>>;
