//! Dead growth of a mutable array parameter (`BSP-REQ-35580A7D7B75`, `DeadCollectionGrowth`).
//!
//! Parameters pass by value. A canonical growth operation (`Core.Collections.Array.Append`)
//! publishes the grown handle into its owner slot, so growth on a `mut T[]` *local* is visible to
//! every later read of that local. Growth on a `mut T[]` *parameter* updates only the callee's
//! copy: unless the body publishes the parameter handle again (returns it, passes it on, stores
//! it), the caller silently keeps the ungrown array. This fact identifies exactly that shape and
//! nothing else; every unclear read of the parameter counts as publication so the fact never
//! fires on legal code.

use super::*;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};

/// One discarded canonical growth call whose `mut T[]` parameter owner is never published.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DeadCollectionGrowth {
    /// The discarded growth call (the diagnostic site).
    pub call: AstNodeKey,
    /// The declaration identifier of the `mut T[]` parameter that owns the growth.
    pub parameter: AstNodeKey,
}

/// Canonical `Core.Collections.Array` function names whose lowering can reallocate their first
/// argument and that return the grown handle.
const GROWTH_OPERATIONS: &[&str] = &["Append"];

/// Cheap syntactic pre-filter: does this call name a growth operation by its last path segment?
/// Authority still comes from [`collection_operation`]; this only avoids querying every call.
pub fn is_growth_call_candidate(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    call: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let Some(call) = index.node_at(program, call).and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
    else {
        return false;
    };
    let mut callee = &call.callee.node;
    while let beskid_analysis::syntax::Expression::Grouped(inner) = callee {
        callee = &inner.node.expr.node;
    }
    match callee {
        beskid_analysis::syntax::Expression::Path(path) => path
            .node
            .path
            .node
            .segments
            .last()
            .is_some_and(|segment| GROWTH_OPERATIONS.contains(&segment.node.name.node.name.as_str())),
        _ => false,
    }
}

/// Report a discarded canonical growth call whose owner argument is a `mut T[]` parameter of the
/// enclosing body and whose body never publishes that parameter handle.
///
/// The fact exists only when all of the following hold:
/// - the call is a canonical growth operation with a local mutation owner;
/// - that owner declaration is a `Parameter` of the same function or method body as the call;
/// - the call is an expression statement (its returned handle is discarded);
/// - no read of the parameter in the body is in a publishing position. Only the owner argument
///   of a growth operation, the target of an index expression, and the target of an assignment
///   are non-publishing; everything else (call arguments, `return`, binding or assignment
///   sources, field initializers, member access, captures) suppresses the fact.
///
/// Non-call nodes, non-canonical lookalikes, local or aggregate-field owners, stale generations,
/// and calls the existing lowering already rejects for an unproven owner contain no fact.
pub fn dead_collection_growth(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<DeadCollectionGrowth> {
    with_registered_syntax(db, key, dead_collection_growth_tracked)
}

#[salsa::tracked(persist)]
fn dead_collection_growth_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<DeadCollectionGrowth> {
    with_node(db, syntax, key, |program, index, node| {
        node.of::<beskid_analysis::syntax::CallExpression>()?;
        dead_collection_growth_for_call(db, program, index, key)
    })?
    .transpose()
}

fn dead_collection_growth_for_call(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
) -> Option<Result<DeadCollectionGrowth, SemanticError>> {
    if !is_growth_call_candidate(program, index, key.node) {
        return None;
    }
    // An unproven (non-`mut`) owner is already rejected by lowering; it is not this fact.
    let Ok(Some(CollectionOperation::Append { owner: CollectionMutationOwner::Local(_) })) =
        collection_operation(db, key)
    else {
        return None;
    };
    let arguments = match call_arguments(db, key) {
        Ok(Some(arguments)) => arguments,
        Ok(None) => return None,
        Err(error) => return Some(Err(error)),
    };
    let owner = *arguments.first()?;
    let owner = AstNodeKey { node: normalized_expression_node(index, owner.node), ..owner };
    let declaration = match resolved_local(db, owner) {
        Ok(Some(resolved)) => resolved.declaration,
        Ok(None) => return None,
        Err(error) => return Some(Err(error)),
    };
    if declaration.unit != key.unit || declaration.generation != key.generation {
        return None;
    }
    if index.kind(parent_node(index, declaration.node)?)? != NodeKind::Parameter {
        return None;
    }
    let callable = super::locals::enclosing_executable_callable(index, key.node)?;
    if !matches!(index.kind(callable)?, NodeKind::FunctionDefinition | NodeKind::MethodDefinition)
        || local_declaration_owner(index, declaration.node) != Some(callable)
    {
        return None;
    }
    if !call_result_is_discarded(index, key.node) {
        return None;
    }
    let name = index
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::Identifier>())
        .map(|identifier| identifier.name.clone())?;
    for path_node in
        index.ids_of_kind(NodeKind::PathExpression).filter(|node| *node != callable && is_ancestor(index, callable, *node))
    {
        let path = index.node_at(program, path_node).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>());
        let Some(path) = path else {
            continue;
        };
        let Some(first) = path.path.node.segments.first() else {
            continue;
        };
        if first.node.name.node.name != name
            || resolve_lexical_declaration(program, index, path_node, &name) != Some(declaration.node)
        {
            continue;
        }
        if path.path.node.segments.len() != 1 {
            return None;
        }
        match parameter_read_publishes(db, index, key, path_node) {
            Ok(false) => continue,
            Ok(true) => return None,
            Err(error) => return Some(Err(error)),
        }
    }
    Some(Ok(DeadCollectionGrowth { call: key, parameter: declaration }))
}

/// Walk out of transparent wrappers; the call is discarded only when it is the whole expression
/// of an `ExpressionStatement`.
fn call_result_is_discarded(index: &SyntaxIndex, call: beskid_analysis::syntax::AstNodeId) -> bool {
    let mut node = call;
    while let Some(parent) = parent_node(index, node) {
        match index.kind(parent) {
            Some(NodeKind::Expression | NodeKind::GroupedExpression) => node = parent,
            Some(NodeKind::ExpressionStatement) => return true,
            _ => return false,
        }
    }
    false
}

/// Classify one single-segment read of the parameter. `false` means the read cannot make the
/// grown handle visible outside the body; `true` means it can, or its effect is unknown.
fn parameter_read_publishes(
    db: &dyn Db,
    index: &SyntaxIndex,
    key: AstNodeKey,
    path_node: beskid_analysis::syntax::AstNodeId,
) -> Result<bool, SemanticError> {
    let mut node = path_node;
    while let Some(parent) = parent_node(index, node) {
        if matches!(index.kind(parent), Some(NodeKind::Expression | NodeKind::GroupedExpression)) {
            node = parent;
        } else {
            break;
        }
    }
    let Some(parent) = parent_node(index, node) else {
        return Ok(true);
    };
    let first_child = index.children(parent).and_then(|children| children.first().copied());
    match index.kind(parent) {
        // `values[i]` and `values[i] = v` read or write elements through the current handle.
        Some(NodeKind::IndexExpression) => Ok(first_child != Some(node)),
        // `values = ...` overwrites the callee copy; it is a write, not a publication.
        Some(NodeKind::AssignExpression) => Ok(first_child != Some(node)),
        // The owner argument of another canonical growth operation publishes only into the
        // same callee slot. Any other argument position hands the handle to a callee.
        Some(NodeKind::CallExpression) => {
            let call = AstNodeKey { node: parent, ..key };
            if !matches!(
                collection_operation(db, call),
                Ok(Some(CollectionOperation::Append { owner: CollectionMutationOwner::Local(_) }))
            ) {
                return Ok(true);
            }
            let arguments = call_arguments(db, call)?.ok_or_else(|| SemanticError::unavailable("dead_collection_growth"))?;
            let owner = arguments.first().map(|owner| normalized_expression_node(index, owner.node));
            Ok(owner != Some(normalized_expression_node(index, path_node)))
        }
        _ => Ok(true),
    }
}
