//! Escape analysis for scoped resources and their receiver method chains.

use super::super::*;
use super::*;
use beskid_analysis::syntax::{
    ContractDefinition, ContractNode, FunctionDefinition, MethodDefinition, PrimitiveType, ScopedUseStatement, Type,
    TypeDefinition, Visibility,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

pub(super) fn scoped_resource_escape(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    fact: &ScopedCleanup,
) -> Option<ScopedCleanupDiagnostic> {
    use ScopedCleanupDiagnostic::{ExplicitDispose, ResourceEscapesScope};
    let declaration = index
        .children(fact.binding.node)?
        .iter()
        .copied()
        .find(|child| index.kind(*child) == Some(NodeKind::Identifier))?;
    let callable = fact.callable?;
    let mut visited = HashSet::new();
    if let Some(diagnostic) = scoped_receiver_method_escape(db, fact.dispose?, fact.dispose?, &mut visited) {
        return Some(diagnostic);
    }
    for path_node in index.ids_of_kind(NodeKind::PathExpression).filter(|node| is_ancestor(index, callable.node, *node))
    {
        let path = index.node_at(program, path_node)?.of::<beskid_analysis::syntax::PathExpression>()?;
        let Some(first) = path.path.node.segments.first() else {
            continue;
        };
        if resolve_lexical_declaration(program, index, path_node, &first.node.name.node.name) != Some(declaration) {
            continue;
        }
        if nearest_ancestor(index, path_node, |kind| {
            matches!(kind, NodeKind::LambdaExpression | NodeKind::FunctionDefinition | NodeKind::MethodDefinition)
        }) != Some(callable.node)
        {
            return Some(ResourceEscapesScope);
        }
        // A bare resource value copies ownership into another value/argument/return.
        // Qualified paths may borrow the receiver only for a source-resolved method.
        if path.path.node.segments.len() == 1 {
            return Some(ResourceEscapesScope);
        }
        let path_key = AstNodeKey { node: path_node, ..key };
        let Some(call) = cleanup_path_call(program, index, path_key) else {
            if nominal_local_member_receiver(db, program, index, path_key, &path.path.node).is_some()
                || aggregate_field_access(db, path_key).ok().flatten().is_none()
            {
                return Some(ResourceEscapesScope);
            }
            continue;
        };
        let Some((method, _)) = nominal_local_member_receiver(db, program, index, call, &path.path.node) else {
            return Some(ResourceEscapesScope);
        };
        if Some(method) == fact.dispose {
            return Some(ExplicitDispose);
        }
        if let Some(diagnostic) = scoped_receiver_method_escape(db, method, fact.dispose?, &mut visited) {
            return Some(diagnostic);
        }
    }
    None
}

fn cleanup_path_call(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    path: AstNodeKey,
) -> Option<AstNodeKey> {
    let mut parent = parent_node(index, path.node)?;
    while index.kind(parent) == Some(NodeKind::Expression) {
        parent = parent_node(index, parent)?;
    }
    let call = index.node_at(program, parent)?.of::<beskid_analysis::syntax::CallExpression>()?;
    let callee = index.direct_child_id(program, parent, DynNodeRef::from(call.callee.as_ref()))?;
    (normalized_expression_node(index, callee) == path.node).then_some(AstNodeKey { node: parent, ..path })
}

/// Follow the actual receiver call graph. Receiver values cannot become explicit
/// arguments, returns, or captures, including through sibling method chains.
fn scoped_receiver_method_escape(
    db: &dyn Db,
    method: AstNodeKey,
    dispose: AstNodeKey,
    visited: &mut HashSet<AstNodeKey>,
) -> Option<ScopedCleanupDiagnostic> {
    use ScopedCleanupDiagnostic::{ExplicitDispose, ResourceEscapesScope};
    if !visited.insert(method) {
        return None;
    }
    let Some(syntax) = db.syntax_unit(method.unit) else {
        return Some(ResourceEscapesScope);
    };
    if syntax.generation(db) != method.generation {
        return Some(ResourceEscapesScope);
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    if index.node_at(program, method.node).and_then(|node| node.of::<MethodDefinition>()).is_none() {
        return Some(ResourceEscapesScope);
    }
    for node in index.ids_of_kind(NodeKind::PathExpression).filter(|node| is_ancestor(index, method.node, *node)) {
        let Some(path) =
            index.node_at(program, node).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
        else {
            return Some(ResourceEscapesScope);
        };
        let path_key = AstNodeKey { node, ..method };
        let Some(first) = path.path.node.segments.first() else {
            return Some(ResourceEscapesScope);
        };
        let first_name = first.node.name.node.name.as_str();
        let is_self = matches!(first_name, "self" | "this")
            && resolve_lexical_declaration(program, index, node, first_name).is_none();
        if is_self && path.path.node.segments.len() == 1 {
            return Some(ResourceEscapesScope);
        }
        let call = cleanup_path_call(program, index, path_key);
        let receiver_member = if resolve_lexical_declaration(program, index, node, &first.node.name.node.name).is_none()
        {
            unqualified_enclosing_method_call(program, index, path_key, &path.path.node)
        } else {
            None
        };
        if receiver_member.is_some() && call.is_none() {
            return Some(ResourceEscapesScope);
        }
        let receiver_call = call.and(receiver_member);
        let borrowed_field =
            aggregate_field_access(db, path_key).ok().flatten().is_some_and(|access| access.receiver == method);
        if (is_self || receiver_call.is_some() || borrowed_field)
            && nearest_ancestor(index, node, |kind| {
                matches!(kind, NodeKind::LambdaExpression | NodeKind::MethodDefinition)
            }) != Some(method.node)
        {
            return Some(ResourceEscapesScope);
        }
        if let Some((target, _)) = receiver_call {
            if target == dispose {
                return Some(ExplicitDispose);
            }
            if let Some(diagnostic) = scoped_receiver_method_escape(db, target, dispose, visited) {
                return Some(diagnostic);
            }
        } else if is_self && call.is_some() {
            // Explicit/dynamic receiver paths not proven by the current method resolver
            // cannot be assumed borrowing merely from their spelling.
            return Some(ResourceEscapesScope);
        }
    }
    None
}
