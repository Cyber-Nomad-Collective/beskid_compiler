//! Parse and schema validation errors for Bsol documents.

use thiserror::Error;

use crate::ast::BsolSpan;

/// Failure while parsing or validating a Bsol document.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BsolError {
    #[error("Bsol parse error at line {line}: {message}")]
    ParseAt {
        line: usize,
        message: String,
        start: Option<usize>,
        end: Option<usize>,
    },
    #[error("Bsol parse error: {0}")]
    Parse(String),
    #[error("Bsol schema error at line {line}: {message}")]
    SchemaAt {
        line: usize,
        message: String,
        start: Option<usize>,
        end: Option<usize>,
    },
    #[error("Bsol schema error: {0}")]
    Schema(String),
    #[error("unknown schema profile `{0}`")]
    UnknownProfile(String),
}

impl BsolError {
    pub fn parse_at(span: BsolSpan, message: impl Into<String>) -> Self {
        Self::ParseAt {
            line: span.line,
            message: message.into(),
            start: Some(span.start),
            end: Some(span.end),
        }
    }

    pub fn schema_at(span: BsolSpan, message: impl Into<String>) -> Self {
        Self::SchemaAt {
            line: span.line,
            message: message.into(),
            start: Some(span.start),
            end: Some(span.end),
        }
    }

    pub fn manifest_source_span(&self) -> Option<(usize, usize)> {
        match self {
            Self::ParseAt { start, end, .. } | Self::SchemaAt { start, end, .. } => {
                match (start, end) {
                    (Some(s), Some(e)) if *e > *s => Some((*s, *e)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    pub fn manifest_source_line(&self) -> Option<usize> {
        match self {
            Self::ParseAt { line, .. } | Self::SchemaAt { line, .. } => Some(*line),
            _ => None,
        }
    }
}
