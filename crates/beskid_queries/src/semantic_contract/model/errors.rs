//! Semantic query errors and legality findings.

use beskid_abi::{abi_v5::AbiType, runtime_source::RuntimeIntrinsicCapability};
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::db::Db;
use crate::inputs::ProjectSession;

use super::super::queries::{node_kind, node_span};
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
#[error("{message}")]
pub struct SemanticError {
    message: Arc<str>,
    diagnostics: Arc<[Arc<str>]>,
    unavailable: bool,
    /// Present only for [`SemanticError::unavailable_at`]: the site the caller had already
    /// resolved to a key when the query it asked for came back unavailable. A compiler-gap
    /// internal error rendered from this site is a genuine gap, never a user diagnostic (see
    /// `docs/superpowers/specs/2026-09-23-production-semantic-diagnostics-design.md` section 2.3).
    unavailable_site: Option<AstNodeKey>,
}

impl SemanticError {
    pub(crate) fn new(message: impl Into<Arc<str>>) -> Self {
        let message = message.into();
        Self { diagnostics: Arc::from([Arc::clone(&message)]), message, unavailable: false, unavailable_site: None }
    }

    pub(crate) fn from_diagnostics(messages: impl IntoIterator<Item = String>) -> Self {
        let diagnostics = messages.into_iter().map(Arc::<str>::from).collect::<Vec<_>>();
        let message = diagnostics.iter().map(AsRef::as_ref).collect::<Vec<_>>().join("\n");
        Self {
            message: Arc::from(message),
            diagnostics: diagnostics.into(),
            unavailable: false,
            unavailable_site: None,
        }
    }

    pub fn unavailable(query: &str) -> Self {
        let message =
            Arc::<str>::from(format!("semantic query `{query}` is unavailable until its AST/Salsa port is complete"));
        Self { diagnostics: Arc::from([Arc::clone(&message)]), message, unavailable: true, unavailable_site: None }
    }

    /// Same meaning as [`SemanticError::unavailable`], plus the generation-bound site the caller
    /// already had in hand. A compiler-gap boundary (`beskid_codegen::module_emission::contracts`)
    /// uses this site to render an internal error at the offending construct instead of an
    /// unsited "unavailable" message.
    pub fn unavailable_at(query: &str, site: AstNodeKey) -> Self {
        let message =
            Arc::<str>::from(format!("semantic query `{query}` is unavailable until its AST/Salsa port is complete"));
        Self {
            diagnostics: Arc::from([Arc::clone(&message)]),
            message,
            unavailable: true,
            unavailable_site: Some(site),
        }
    }

    pub fn is_unavailable(&self) -> bool {
        self.unavailable
    }

    /// The site given to [`SemanticError::unavailable_at`], if any.
    pub fn unavailable_site(&self) -> Option<AstNodeKey> {
        self.unavailable_site
    }

    pub fn diagnostics(&self) -> &[Arc<str>] {
        &self.diagnostics
    }
}

pub type SemanticQueryResult<T> = Result<Option<T>, SemanticError>;

/// One legality finding: a positive description of a user-facing semantic error located at a
/// generation-bound source site, produced by a `beskid_queries::semantic_contract::legality`
/// fact. `kind` owns the diagnostic code, message, label, and help text
/// (`beskid_analysis::analysis::SemanticIssueKind`, `BSP-REQ-072159A73908`); the legality fact is
/// the *detection* authority, `kind` is the *code* authority (see the design doc's ownership
/// table, section 2.6). `related` names optional secondary sites (for example the earlier call
/// argument a generic parameter conflict was first bound from).
#[derive(Debug, Clone)]
pub struct SemanticFinding {
    pub kind: beskid_analysis::analysis::SemanticIssueKind,
    pub site: AstNodeKey,
    pub related: Vec<(AstNodeKey, &'static str)>,
}
