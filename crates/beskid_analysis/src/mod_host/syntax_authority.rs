//! Closed current-generation syntax services borrowed by a native invocation.
use crate::{
    syntax::{AstNodeId, SpanInfo, SyntaxGenerationId},
    syntax_query::NodeKind,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModSyntaxNodeRef {
    pub source_unit: PathBuf,
    pub invocation_issuer: u64,
    pub generation: SyntaxGenerationId,
    pub node: AstNodeId,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModSyntaxBounds {
    pub max_nodes: u32,
    pub max_depth: u32,
}
impl ModSyntaxBounds {
    pub fn checked(self) -> Result<Self, super::ModSemanticError> {
        if self.max_nodes == 0 || self.max_nodes > 1_000_000 || self.max_depth == 0 || self.max_depth > 1024 {
            return Err(super::ModSemanticError("syntax query bounds outside supported work policy".into()));
        }
        Ok(self)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
pub enum ModSyntaxRequest {
    Root,
    Children {
        node: ModSyntaxNodeRef,
    },
    Parent {
        node: ModSyntaxNodeRef,
    },
    Ancestors {
        node: ModSyntaxNodeRef,
        bounds: ModSyntaxBounds,
    },
    Descendants {
        node: ModSyntaxNodeRef,
        bounds: ModSyntaxBounds,
    },
    Span {
        node: ModSyntaxNodeRef,
    },
    OfKind {
        node: ModSyntaxNodeRef,
        bounds: ModSyntaxBounds,
        kind: NodeKind,
    },
    FindFirst {
        node: ModSyntaxNodeRef,
        bounds: ModSyntaxBounds,
        kind: NodeKind,
    },
    /// Exact Rust AST projection. Worker correspondence maps it through canonical SDK schema.
    Project {
        node: ModSyntaxNodeRef,
        kind: NodeKind,
    },
    Replace {
        root: ModSyntaxNodeRef,
        target: ModSyntaxNodeRef,
        replacement: ModSyntaxNodeRef,
        bounds: ModSyntaxBounds,
    },
    ReplaceProjection {
        root: ModSyntaxNodeRef,
        target: ModSyntaxNodeRef,
        kind: NodeKind,
        value: serde_json::Value,
        bounds: ModSyntaxBounds,
    },
    Remove {
        root: ModSyntaxNodeRef,
        target: ModSyntaxNodeRef,
        bounds: ModSyntaxBounds,
    },
    InsertBefore {
        root: ModSyntaxNodeRef,
        anchor: ModSyntaxNodeRef,
        node: ModSyntaxNodeRef,
        bounds: ModSyntaxBounds,
    },
    InsertAfter {
        root: ModSyntaxNodeRef,
        anchor: ModSyntaxNodeRef,
        node: ModSyntaxNodeRef,
        bounds: ModSyntaxBounds,
    },
    Apply {
        root: ModSyntaxNodeRef,
        bounds: ModSyntaxBounds,
    },
}
#[derive(Debug, Clone, Serialize)]
pub enum ModSyntaxResponse {
    Node(Option<ModSyntaxNodeRef>),
    Nodes(Vec<ModSyntaxNodeRef>),
    Span(Option<SpanInfo>),
    Projection { kind: NodeKind, value: Option<serde_json::Value> },
}
#[derive(Debug, Clone)]
pub struct ModAppliedSyntax {
    pub source_unit: PathBuf,
    pub previous_generation: SyntaxGenerationId,
    pub program: crate::syntax::Spanned<crate::syntax::Program>,
}
pub trait ModSyntaxAuthority {
    fn take_applied_syntax(&self, invocation_issuer: u64) -> Result<Vec<ModAppliedSyntax>, super::ModSemanticError>;
    /// Issuer is assigned by the scoped transport, never accepted from native input.
    fn syntax_query(
        &self,
        invocation_issuer: u64,
        request: &ModSyntaxRequest,
    ) -> Result<ModSyntaxResponse, super::ModSemanticError>;
}
