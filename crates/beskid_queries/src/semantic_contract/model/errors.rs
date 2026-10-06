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
    /// The query whose port is incomplete, for every unavailable (compiler-gap) error. `None` for
    /// an explicit rejection. The module-emission boundary names it in internal error E2101.
    unavailable_query: Option<Arc<str>>,
    /// Present only for [`SemanticError::unavailable_at`]: the site the caller had already
    /// resolved to a key when the query it asked for came back unavailable. A compiler-gap
    /// internal error rendered from this site is a genuine gap, never a user diagnostic (see
    /// `docs/superpowers/specs/2026-09-23-production-semantic-diagnostics-design.md` section 2.3).
    unavailable_site: Option<AstNodeKey>,
    /// Present only for [`SemanticError::generic_binding_conflict`]: the call specialization
    /// authority could not bind one generic parameter because two arguments of the call fix it to
    /// different types. Still an unavailable specialization for every existing consumer; the
    /// legality gate reads this positive description to report E1229 at the call.
    binding_conflict: Option<GenericBindingConflict>,
    /// Present only for [`SemanticError::generic_bound_not_satisfied`]: a `where` bound of the
    /// selected callable that the call's concrete type does not satisfy. Still a rejected
    /// specialization for every existing consumer; the legality gate reads this positive
    /// description to report E1610 at the call.
    bound_violation: Option<GenericBoundViolation>,
}

/// One `where G: Contract` bound a call's concrete type does not satisfy.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericBoundViolation {
    pub type_name: Arc<str>,
    pub contract_name: Arc<str>,
}

/// One generic parameter that a single call binds to two different types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GenericBindingConflict {
    pub parameter: Arc<str>,
    pub first: Arc<str>,
    pub second: Arc<str>,
}

impl SemanticError {
    pub(crate) fn new(message: impl Into<Arc<str>>) -> Self {
        let message = message.into();
        Self {
            diagnostics: Arc::from([Arc::clone(&message)]),
            message,
            unavailable_query: None,
            unavailable_site: None,
            binding_conflict: None,
            bound_violation: None,
        }
    }

    pub(crate) fn from_diagnostics(messages: impl IntoIterator<Item = String>) -> Self {
        let diagnostics = messages.into_iter().map(Arc::<str>::from).collect::<Vec<_>>();
        let message = diagnostics.iter().map(AsRef::as_ref).collect::<Vec<_>>().join("\n");
        Self {
            message: Arc::from(message),
            diagnostics: diagnostics.into(),
            unavailable_query: None,
            unavailable_site: None,
            binding_conflict: None,
            bound_violation: None,
        }
    }

    pub fn unavailable(query: &str) -> Self {
        let message =
            Arc::<str>::from(format!("semantic query `{query}` is unavailable until its AST/Salsa port is complete"));
        Self {
            diagnostics: Arc::from([Arc::clone(&message)]),
            message,
            unavailable_query: Some(Arc::from(query)),
            unavailable_site: None,
            binding_conflict: None,
            bound_violation: None,
        }
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
            unavailable_query: Some(Arc::from(query)),
            unavailable_site: Some(site),
            binding_conflict: None,
            bound_violation: None,
        }
    }

    /// The call specialization authority's own rejection of a call that binds `parameter` to two
    /// different types. It stays an unavailable `call_abi_signature` for every consumer that only
    /// asks whether a specialization exists, and carries the conflict for the legality gate.
    pub(crate) fn generic_binding_conflict(
        parameter: impl Into<Arc<str>>,
        first: impl Into<Arc<str>>,
        second: impl Into<Arc<str>>,
    ) -> Self {
        let conflict =
            GenericBindingConflict { parameter: parameter.into(), first: first.into(), second: second.into() };
        let message = Arc::<str>::from(format!(
            "semantic query `call_abi_signature` is unavailable: generic parameter `{}` is bound to both `{}` and `{}`",
            conflict.parameter, conflict.first, conflict.second
        ));
        Self {
            diagnostics: Arc::from([Arc::clone(&message)]),
            message,
            unavailable_query: Some(Arc::from("call_abi_signature")),
            unavailable_site: None,
            binding_conflict: Some(conflict),
            bound_violation: None,
        }
    }

    /// The contract-witness authority's own rejection of a call whose concrete type does not
    /// satisfy a `where` bound of the selected callable. `message` is the rendered E1610 text the
    /// specialization consumers already show; the legality gate reads the typed violation.
    pub(crate) fn generic_bound_not_satisfied(
        message: impl Into<Arc<str>>,
        type_name: impl Into<Arc<str>>,
        contract_name: impl Into<Arc<str>>,
    ) -> Self {
        let message = message.into();
        Self {
            diagnostics: Arc::from([Arc::clone(&message)]),
            message,
            unavailable_query: None,
            unavailable_site: None,
            binding_conflict: None,
            bound_violation: Some(GenericBoundViolation {
                type_name: type_name.into(),
                contract_name: contract_name.into(),
            }),
        }
    }

    /// The bound violation given to [`SemanticError::generic_bound_not_satisfied`], if any.
    pub fn bound_violation(&self) -> Option<&GenericBoundViolation> {
        self.bound_violation.as_ref()
    }

    /// The generic binding conflict given to [`SemanticError::generic_binding_conflict`], if any.
    pub fn binding_conflict(&self) -> Option<&GenericBindingConflict> {
        self.binding_conflict.as_ref()
    }

    pub fn is_unavailable(&self) -> bool {
        self.unavailable_query.is_some()
    }

    /// The query named by an unavailable (compiler-gap) error.
    pub fn unavailable_query(&self) -> Option<&str> {
        self.unavailable_query.as_deref()
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
