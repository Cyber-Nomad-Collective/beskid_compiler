//! Fresh-ownership acquisition evidence for scoped resources.

use super::super::*;
use super::*;
use beskid_analysis::syntax::{
    ContractDefinition, ContractNode, FunctionDefinition, MethodDefinition, PrimitiveType, ScopedUseStatement, Type,
    TypeDefinition, Visibility,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

/// Acquiring an alias is not ownership transfer. Follow only exact fresh constructors and
/// source-resolved factories whose every return is recursively fresh. A recursive component
/// must have a concrete construction leaf; a cycle alone supplies no ownership evidence.
pub(super) fn scoped_acquisition(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    fact: &ScopedCleanup,
    resource: AstNodeKey,
) -> Option<ScopedAcquisition> {
    let binding = index.node_at(program, fact.binding.node)?.of::<beskid_analysis::syntax::LetStatement>()?;
    let node = index.direct_child_id(program, fact.binding.node, DynNodeRef::from(&binding.value))?;
    let expression = AstNodeKey { node: normalized_expression_node(index, node), ..fact.binding };
    let mut constructed = false;
    if !fresh_cleanup_expression(db, expression, resource, None, &mut HashSet::new(), &mut constructed) || !constructed
    {
        return None;
    }
    match index.kind(expression.node)? {
        NodeKind::StructLiteralExpression => Some(ScopedAcquisition::FreshConstruction(expression)),
        NodeKind::CallExpression => match call_lowering(db, expression).ok()?? {
            CallLowering::Direct(factory) => Some(ScopedAcquisition::FreshFactory(factory)),
            _ => None,
        },
        NodeKind::TryExpression => Some(ScopedAcquisition::FreshTry(expression)),
        _ => None,
    }
}

/// A validated `?` supplies the exact Result identity. Only its Ok payload can
/// acquire ownership; Error exits before there is a scoped resource to dispose.
struct FreshResultSuccess {
    identity: GenericSourceTypeIdentity,
    declaration: AstNodeKey,
}

fn fresh_cleanup_expression(
    db: &dyn Db,
    expression: AstNodeKey,
    resource: AstNodeKey,
    result: Option<&FreshResultSuccess>,
    visited: &mut HashSet<AstNodeKey>,
    constructed: &mut bool,
) -> bool {
    let Some(syntax) = db.syntax_unit(expression.unit).filter(|syntax| syntax.accepts_key(db, expression)) else {
        return false;
    };
    let index = syntax.syntax_index(db);
    let expression = AstNodeKey { node: normalized_expression_node(index, expression.node), ..expression };
    if let Some(result) = result {
        if generic_source_expression_identity(db, expression).ok().as_ref() != Some(&result.identity) {
            return false;
        }
        if index.kind(expression.node) == Some(NodeKind::EnumConstructorExpression) {
            let Some(constructor) = enum_constructor(db, expression).ok().flatten() else {
                return false;
            };
            if constructor.declaration != result.declaration {
                return false;
            }
            return match (constructor.variant_index, constructor.payloads.as_ref()) {
                (0, [payload]) => fresh_cleanup_expression(db, *payload, resource, None, visited, constructed),
                // A propagated error is not evidence of fresh ownership. The caller
                // still requires at least one concrete successful construction leaf.
                (1, [_]) => true,
                _ => false,
            };
        }
    } else if index.kind(expression.node) == Some(NodeKind::TryExpression) {
        let Some(fact) = try_expression_fact(db, expression).ok().flatten() else {
            return false;
        };
        if contracts::concrete_declaration(db, expression, &fact.payload_identity) != Some(resource) {
            return false;
        }
        let Ok(identity) = generic_source_expression_identity(db, fact.operand) else {
            return false;
        };
        let Ok((declaration, _)) = layouts::enum_layout_for_source_identity(db, fact.operand, &identity) else {
            return false;
        };
        return fresh_cleanup_expression(
            db,
            fact.operand,
            resource,
            Some(&FreshResultSuccess { identity, declaration }),
            visited,
            constructed,
        );
    } else if index.kind(expression.node) == Some(NodeKind::StructLiteralExpression) {
        let fresh = aggregate_literal_declaration(db, expression).ok().flatten() == Some(resource);
        *constructed |= fresh;
        return fresh;
    }
    let Some(CallLowering::Direct(factory)) = call_lowering(db, expression).ok().flatten() else {
        return false;
    };
    if !visited.insert(factory) {
        // Preserve the existing direct-factory policy, but do not infer a fresh
        // Result success through a cyclic fallible factory.
        return result.is_none();
    }
    let Some(syntax) = db.syntax_unit(factory.unit).filter(|syntax| syntax.accepts_key(db, factory)) else {
        return false;
    };
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let Some(function) = index.node_at(program, factory.node).and_then(|node| node.of::<FunctionDefinition>()) else {
        return false;
    };
    if !function.generics.is_empty() {
        return false;
    }
    let returns = index
        .ids_of_kind(NodeKind::ReturnStatement)
        .filter(|node| {
            is_ancestor(index, factory.node, *node)
                && nearest_ancestor(index, *node, |kind| {
                    matches!(
                        kind,
                        NodeKind::FunctionDefinition | NodeKind::LambdaExpression | NodeKind::MethodDefinition
                    )
                }) == Some(factory.node)
        })
        .collect::<Vec<_>>();
    if returns.is_empty() {
        return false;
    }
    let fresh = returns.into_iter().all(|node| {
        let value = index
            .node_at(program, node)
            .and_then(|node| node.of::<beskid_analysis::syntax::ReturnStatement>())
            .and_then(|ret| ret.value.as_ref());
        let Some(value) = value else {
            return false;
        };
        let Some(value) = index.direct_child_id(program, node, DynNodeRef::from(value)) else {
            return false;
        };
        fresh_cleanup_expression(db, AstNodeKey { node: value, ..factory }, resource, result, visited, constructed)
    });
    if result.is_some() {
        visited.remove(&factory);
    }
    fresh
}
