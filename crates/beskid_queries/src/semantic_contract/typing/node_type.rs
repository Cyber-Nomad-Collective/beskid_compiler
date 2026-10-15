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
        if node.of::<beskid_analysis::syntax::MemberExpression>().is_some() {
            return Some(abi_type(db, key).and_then(|ty| ty.ok_or_else(|| SemanticError::unavailable("node_type"))));
        }
        if node.of::<beskid_analysis::syntax::ClifBlockExpression>().is_some() {
            return Some(clif_block_context_type(db, key));
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some() {
            if is_transfer_class_intrinsic(program, index, key).is_some() {
                return Some(Ok(SemanticTypeId::I64));
            }
            if let Some(exchange) = exchange_call_for_node(program, index, key) {
                return Some(node_type(db, exchange.place).and_then(|ty| ty.ok_or_else(|| SemanticError::unavailable("node_type"))));
            }
            match primitive_numeric_conversion(db, key) {
                Ok(Some(conversion)) => return Some(Ok(conversion.to)),
                Ok(None) => (),
                Err(error) => return Some(Err(error)),
            }
            match call_lowering(db, key) {
                Ok(Some(
                    CallLowering::Direct(_)
                    | CallLowering::Runtime(_)
                    | CallLowering::NativeModCallback(_)
                    | CallLowering::FunctionValue(_),
                )) => (),
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
            && !block.block.node.may_fall_through()
        {
            return Some(Ok(SemanticTypeId::NEVER));
        }
        // A module constant use behaves as its declared literal, so it has that literal's
        // natural type (`const LIMIT = 0xFFFFFFFFFFFFFFFE;` is a `word`, never an `i32`).
        if node.of::<beskid_analysis::syntax::PathExpression>().is_some()
            && let Ok(Some(text)) = constant_integer_literal(db, key)
        {
            return Some(Ok(integer_literal_semantic_type(&text)));
        }
        if node.of::<beskid_analysis::syntax::MatchExpression>().is_some() {
            if let Ok(Some(fact)) = enum_match(db, key) {
                return Some(match_arms_result_semantic_type(db, &fact.arms));
            }
            if let Ok(Some(fact)) = scalar_match(db, key) {
                return Some(match_arms_result_semantic_type(db, &fact.arms));
            }
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

/// The joined result type of a match's arm bodies; `never` arms do not constrain it.
pub(in crate::semantic_contract) fn match_arms_result_semantic_type(
    db: &dyn Db,
    arms: &[EnumMatchArmFact],
) -> Result<SemanticTypeId, SemanticError> {
    let mut result = None;
    for arm in arms {
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

/// A `clif { ... }` block has no intrinsic type: it takes its [`expected_type`]. Expected
/// types are derived only from the block's context, never from the block itself, so this never
/// recurses into the block. A block with no typed context stays unavailable.
pub(in crate::semantic_contract) fn clif_block_context_type(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<SemanticTypeId, SemanticError> {
    expected_type(db, key)?.map(|expected| expected.abi_type()).ok_or_else(|| SemanticError::unavailable("node_type"))
}
