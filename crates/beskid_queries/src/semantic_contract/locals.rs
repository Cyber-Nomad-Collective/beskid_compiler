//! Focused semantic-contract implementation cluster.

use super::*;

#[salsa::tracked]
pub(super) fn resolved_local_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ResolvedLocal> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        let [segment] = path.path.node.segments.as_slice() else {
            return None;
        };
        if !segment.node.type_args.is_empty() {
            return None;
        }
        let declaration = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())?;
        Some(ResolvedLocal { declaration: AstNodeKey { node: declaration, ..key } })
    })
}

/// Resolve an unshadowed, same-module integer constant to its declared value.
/// Constants have no storage slot: generated ISLE consumes this fact as an immediate.
#[salsa::tracked]
pub(super) fn constant_integer_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<i64> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        let [segment] = path.path.node.segments.as_slice() else {
            return None;
        };
        if !segment.node.type_args.is_empty()
            || resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str()).is_some()
        {
            return None;
        }
        if let Some(declaration) =
            resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())
        {
            let parent = parent_node(index, declaration)?;
            if index.kind(parent) != Some(beskid_analysis::syntax_query::NodeKind::ConstantDefinition) {
                return None;
            }
        }
        program.node.items.iter().find_map(|item| {
            let beskid_analysis::syntax::Node::ConstantDefinition(constant) = &item.node else {
                return None;
            };
            (constant.node.name.node.name == segment.node.name.node.name).then(|| match &constant.node.value.node {
                beskid_analysis::syntax::Literal::Integer(value) => {
                    integer_literal_u64(value).and_then(|value| i64::try_from(value).ok())
                }
                _ => None,
            })?
        })
    })
}

#[salsa::tracked]
pub(super) fn local_slot_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<LocalSlot> {
    with_node(db, syntax, key, |_program, index, node| {
        node.of::<beskid_analysis::syntax::Identifier>()?;
        local_slot_for_declaration(index, key)
    })?
    .transpose()
}

#[salsa::tracked]
pub(super) fn mutable_local_assignment_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<MutableLocalAssignment> {
    with_node(db, syntax, key, |program, index, node| {
        let assignment = node.of::<beskid_analysis::syntax::AssignExpression>()?;
        if !matches!(assignment.op.node, beskid_analysis::syntax::AssignOp::Assign) {
            return None;
        }
        let beskid_analysis::syntax::Expression::Path(path) = &assignment.target.node else {
            return None;
        };
        let [segment] = path.node.path.node.segments.as_slice() else {
            return None;
        };
        if !segment.node.type_args.is_empty() {
            return None;
        }
        let declaration = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())?;
        if !local_declaration_is_mutable(program, index, declaration) {
            return Some(Err(SemanticError::unavailable("mutable_local_assignment")));
        }
        let declaration = AstNodeKey { node: declaration, ..key };
        let slot = local_slot_for_declaration(index, declaration)?;
        Some(slot.map(|slot| MutableLocalAssignment { declaration, slot }))
    })?
    .transpose()
}

pub(super) fn local_slot_for_declaration(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<Result<LocalSlot, SemanticError>> {
    let owner = local_declaration_owner(index, key.node)?;
    let slot = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::Identifier)
        .filter(|declaration| local_declaration_owner(index, *declaration) == Some(owner))
        .position(|declaration| declaration == key.node)?;
    Some(
        u32::try_from(slot)
            .map(|index| LocalSlot { owner: AstNodeKey { node: owner, ..key }, index })
            .map_err(|_| SemanticError::unavailable("local_slot")),
    )
}

pub(super) fn local_declaration_owner(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let parent = parent_node(index, declaration)?;
    if !matches!(
        index.kind(parent)?,
        beskid_analysis::syntax_query::NodeKind::Parameter
            | beskid_analysis::syntax_query::NodeKind::LetStatement
            | beskid_analysis::syntax_query::NodeKind::LambdaParameter
            | beskid_analysis::syntax_query::NodeKind::ForStatement
            | beskid_analysis::syntax_query::NodeKind::Pattern
    ) {
        return None;
    }
    nearest_ancestor(index, parent, |kind| {
        matches!(
            kind,
            beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                | beskid_analysis::syntax_query::NodeKind::MethodDefinition
                | beskid_analysis::syntax_query::NodeKind::TestDefinition
                | beskid_analysis::syntax_query::NodeKind::LambdaExpression
        )
    })
}

pub(super) fn local_declaration_is_mutable(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let Some(parent) = parent_node(index, declaration) else {
        return false;
    };
    let Some(node) = index.node_at(program, parent) else {
        return false;
    };
    node.of::<beskid_analysis::syntax::LetStatement>().is_some_and(|binding| binding.mutable)
        || node.of::<beskid_analysis::syntax::Parameter>().is_some_and(|parameter| parameter.mutable)
}

pub(super) fn resolve_lexical_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    name: &str,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut best: Option<(usize, u32, beskid_analysis::syntax::AstNodeId)> = None;
    for declaration in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::Identifier) {
        let Some(identifier) =
            index.node_at(program, declaration).and_then(|node| node.of::<beskid_analysis::syntax::Identifier>())
        else {
            continue;
        };
        if identifier.name != name {
            continue;
        }
        let Some(scope) = local_declaration_scope(index, declaration, reference) else {
            continue;
        };
        let Some(distance) = ancestor_distance(index, scope, reference) else {
            continue;
        };
        let rank = (distance, u32::MAX - declaration.0, declaration);
        if best.is_none_or(|current| rank < current) {
            best = Some(rank);
        }
    }
    best.map(|(_, _, declaration)| declaration)
}

pub(super) fn local_declaration_scope(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
    reference: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let parent = parent_node(index, declaration)?;
    match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::LetStatement => {
            if declaration.0 >= reference.0 || is_ancestor(index, parent, reference) {
                return None;
            }
            nearest_ancestor(index, parent, |kind| {
                matches!(
                    kind,
                    beskid_analysis::syntax_query::NodeKind::Block
                        | beskid_analysis::syntax_query::NodeKind::TestDefinition
                )
            })
            .filter(|scope| is_ancestor(index, *scope, reference))
        }
        beskid_analysis::syntax_query::NodeKind::Parameter => nearest_ancestor(index, parent, |kind| {
            matches!(
                kind,
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::MethodDefinition
            )
        })
        .filter(|scope| is_ancestor(index, *scope, reference)),
        beskid_analysis::syntax_query::NodeKind::LambdaParameter => {
            nearest_ancestor(index, parent, |kind| kind == beskid_analysis::syntax_query::NodeKind::LambdaExpression)
                .filter(|scope| is_ancestor(index, *scope, reference))
        }
        beskid_analysis::syntax_query::NodeKind::ForStatement => index
            .children(parent)?
            .iter()
            .copied()
            .find(|child| index.kind(*child) == Some(beskid_analysis::syntax_query::NodeKind::Block))
            .filter(|scope| is_ancestor(index, *scope, reference)),
        beskid_analysis::syntax_query::NodeKind::Pattern => {
            nearest_ancestor(index, parent, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchArm)
                .filter(|scope| is_ancestor(index, *scope, reference))
        }
        _ => None,
    }
}

pub(super) fn parent_node(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    index.metadata().get(node.0 as usize)?.parent
}

pub(super) fn nearest_ancestor(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
    predicate: impl Fn(beskid_analysis::syntax_query::NodeKind) -> bool,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if index.kind(candidate).is_some_and(&predicate) {
            return Some(candidate);
        }
        current = parent_node(index, candidate);
    }
    None
}

pub(super) fn is_ancestor(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    ancestor: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> bool {
    ancestor_distance(index, ancestor, node).is_some()
}

pub(super) fn ancestor_distance(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    ancestor: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<usize> {
    let mut current = Some(node);
    let mut distance = 0usize;
    while let Some(candidate) = current {
        if candidate == ancestor {
            return Some(distance);
        }
        current = parent_node(index, candidate);
        distance += 1;
    }
    None
}
