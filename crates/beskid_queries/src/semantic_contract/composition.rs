//! Generation-bound composition statement shape. The adapter validates it against the frozen graph.

use std::sync::Arc;

use beskid_analysis::syntax::{LaunchStatement, WithStatement};
use beskid_analysis::syntax_query::DynNodeRef;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompositionLaunchFact {
    pub site: AstNodeKey,
    pub host: Arc<str>,
}

/// Syntax names a scope; only the assembled frontend snapshot assigns its authoritative ID.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompositionScopeFact {
    pub site: AstNodeKey,
    pub scope_name: Arc<str>,
    pub body: AstNodeKey,
}

pub fn composition_launch(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CompositionLaunchFact> {
    with_registered_syntax(db, key, composition_launch_tracked)
}

pub fn composition_scope(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CompositionScopeFact> {
    with_registered_syntax(db, key, composition_scope_tracked)
}

#[salsa::tracked(persist)]
fn composition_launch_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CompositionLaunchFact> {
    with_node(db, syntax, key, |_program, _index, node| {
        let launch = node.of::<LaunchStatement>()?;
        let host = launch
            .host_path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.as_str())
            .collect::<Vec<_>>()
            .join(".");
        Some(CompositionLaunchFact { site: key, host: Arc::from(host) })
    })
}

#[salsa::tracked(persist)]
fn composition_scope_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CompositionScopeFact> {
    with_node(db, syntax, key, |program, index, node| {
        let with_statement = node.of::<WithStatement>()?;
        let scope_name = &with_statement.scope_name.node.name;
        let body = index.direct_child_id(program, key.node, DynNodeRef::from(&with_statement.body))?;
        Some(CompositionScopeFact {
            site: key,
            scope_name: Arc::from(scope_name.as_str()),
            body: AstNodeKey { node: body, ..key },
        })
    })
}
