//! Tracked node typing and enum-match result types.

use super::super::*;
use super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn node_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, node| {
        if node.of::<beskid_analysis::syntax::TryExpression>().is_some()
            || matches!(
                node.of::<beskid_analysis::syntax::Expression>(),
                Some(beskid_analysis::syntax::Expression::Try(_))
            )
        {
            return Some(abi_type(db, key).and_then(|ty| ty.ok_or_else(|| SemanticError::unavailable("node_type"))));
        }
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return Some(abi_type_for_binary_expression(db, program, index, key, binary));
        }
        if node.of::<beskid_analysis::syntax::ClifBlockExpression>().is_some() {
            return Some(clif_block_context_type(db, program, index, key));
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some() {
            match primitive_numeric_conversion(db, key) {
                Ok(Some(conversion)) => return Some(Ok(conversion.to)),
                Ok(None) => (),
                Err(error) => return Some(Err(error)),
            }
            match call_lowering(db, key) {
                Ok(Some(CallLowering::Direct(_) | CallLowering::Runtime(_) | CallLowering::NativeModCallback(_))) => (),
                Ok(Some(_) | None) => return Some(Err(SemanticError::unavailable("node_type"))),
                Err(error) => return Some(Err(error)),
            };
            return Some(
                call_abi_signature(db, key)
                    .and_then(|signature| signature.ok_or_else(|| SemanticError::unavailable("node_type")))
                    .map(|signature| signature.result),
            );
        }
        if let Some(binding_type) = pattern_binding_semantic_type(db, program, index, key, node) {
            return Some(binding_type);
        }
        // A block expression whose statements cannot fall through (for example a match arm
        // ending in `return`) never produces a value. The same syntax-only control-flow fact
        // that proves function bodies terminate is the authority; value blocks stay untyped here.
        if let Some(block) = node.of::<beskid_analysis::syntax::BlockExpression>()
            && !block_may_fall_through(&block.block.node)
        {
            return Some(Ok(SemanticTypeId::NEVER));
        }
        if node.of::<beskid_analysis::syntax::PathExpression>().is_some()
            && matches!(constant_integer(db, key), Ok(Some(_)))
        {
            return Some(Ok(SemanticTypeId::I32));
        }
        if node.of::<beskid_analysis::syntax::MatchExpression>().is_some() && matches!(enum_match(db, key), Ok(Some(_)))
        {
            return Some(enum_match_result_semantic_type(db, key));
        }
        if let Some(beskid_analysis::syntax::Expression::Call(call)) = node.of::<beskid_analysis::syntax::Expression>()
        {
            let call = index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(call))
                .map(|node| AstNodeKey { node, ..key })
                .ok_or_else(|| SemanticError::unavailable("node_type"));
            return Some(call.and_then(|call| {
                // The normalized call owns both primitive conversion and ordinary call typing.
                node_type(db, call)?.ok_or_else(|| SemanticError::unavailable("node_type"))
            }));
        }
        semantic_type_for_node(program, index, key.node, node)
    })?
    .transpose()
}

pub(in crate::semantic_contract) fn enum_match_result_semantic_type(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<SemanticTypeId, SemanticError> {
    let fact = enum_match(db, key)?.ok_or_else(|| SemanticError::unavailable("node_type"))?;
    let mut result = None;
    for arm in fact.arms.iter() {
        let contextual = match contextual_integer_literal_abi_type(db, arm.body) {
            Ok(contextual) => contextual,
            Err(error) if error.is_unavailable() => None,
            Err(error) => return Err(error),
        };
        let arm_type = match contextual {
            Some(contextual) => contextual,
            None => node_type(db, arm.body)?.ok_or_else(|| SemanticError::unavailable("node_type"))?,
        };
        result = join_match_arm_type(result, arm_type)?;
    }
    result.ok_or_else(|| SemanticError::unavailable("node_type"))
}

/// A `clif { ... }` block has no intrinsic type: it takes the type of its syntactic context.
/// Only contexts that do not depend on the block's own type are consulted, so this never
/// recurses into the block: a typed `let`, the enclosing function's return type, the target of
/// an assignment, a parameter of a non-generic callee, or `unit` for an expression statement.
pub(in crate::semantic_contract) fn clif_block_context_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax_query::NodeKind;

    let unavailable = || SemanticError::unavailable("node_type");
    let mut child = key.node;
    let mut parent = parent_node(index, child).ok_or_else(unavailable)?;
    while matches!(index.kind(parent), Some(NodeKind::Expression | NodeKind::GroupedExpression)) {
        child = parent;
        parent = parent_node(index, child).ok_or_else(unavailable)?;
    }
    let parent_key = AstNodeKey { node: parent, ..key };
    let node = index.node_at(program, parent).ok_or_else(unavailable)?;
    match index.kind(parent) {
        Some(NodeKind::ExpressionStatement) => Ok(SemanticTypeId::UNIT),
        Some(NodeKind::LetStatement) => {
            let binding = node.of::<beskid_analysis::syntax::LetStatement>().ok_or_else(unavailable)?;
            let annotation = binding.type_annotation.as_ref().ok_or_else(unavailable)?;
            abi_type_from_syntax(db, parent_key, &annotation.node)
        }
        Some(NodeKind::ReturnStatement) => {
            let owner = nearest_ancestor(index, parent, |kind| {
                matches!(kind, NodeKind::FunctionDefinition | NodeKind::MethodDefinition | NodeKind::LambdaExpression)
            })
            .ok_or_else(unavailable)?;
            if index.kind(owner) == Some(NodeKind::LambdaExpression) {
                return Err(unavailable());
            }
            Ok(item_abi_signature(db, AstNodeKey { node: owner, ..key })?.ok_or_else(unavailable)?.result)
        }
        Some(NodeKind::AssignExpression) => {
            let assignment = node.of::<beskid_analysis::syntax::AssignExpression>().ok_or_else(unavailable)?;
            let target = index
                .direct_child_id(program, parent, beskid_analysis::syntax_query::DynNodeRef::from(assignment.target.as_ref()))
                .ok_or_else(unavailable)?;
            if target == child {
                return Err(unavailable());
            }
            let target = AstNodeKey { node: normalized_expression_node(index, target), ..key };
            if index.kind(target.node) == Some(NodeKind::IndexExpression) {
                return array_index_element_abi_type(db, target)?.ok_or_else(unavailable);
            }
            node_type(db, target)?.ok_or_else(unavailable)
        }
        Some(NodeKind::CallExpression) => {
            let arguments = call_arguments(db, parent_key)?.ok_or_else(unavailable)?;
            let position = arguments
                .iter()
                .position(|argument| argument.node == child || normalized_expression_node(index, argument.node) == key.node)
                .ok_or_else(unavailable)?;
            let signature = match call_lowering(db, parent_key)? {
                Some(CallLowering::Direct(declaration)) => item_abi_signature(db, declaration)?,
                Some(CallLowering::Dynamic) | None => None,
                Some(_) => call_abi_signature(db, parent_key)?,
            }
            .ok_or_else(unavailable)?;
            let parameter = if signature.parameters.len() == arguments.len() {
                position
            } else if signature.parameters.len() + 1 == arguments.len() {
                position.checked_sub(1).ok_or_else(unavailable)?
            } else {
                return Err(unavailable());
            };
            signature.parameters.get(parameter).copied().ok_or_else(unavailable)
        }
        _ => Err(unavailable()),
    }
}
