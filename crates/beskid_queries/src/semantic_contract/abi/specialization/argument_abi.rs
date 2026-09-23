//! Contextual integer-literal ABI fit and call-argument and binary-operand ABI facts.

use super::super::super::layouts::unique_assembled_type_in_module;
use super::super::super::*;
use super::*;

/// Whether a source expression is a bare integer literal without an ABI suffix.
///
/// This follows only singleton syntax wrappers, so compound arithmetic, casts, calls, and arrays
/// never inherit an ABI representation from a surrounding call.
pub(in crate::semantic_contract) fn unsuffixed_integer_literal(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<bool, SemanticError> {
    Ok(integer_literal_text(db, key)?.is_some())
}

/// Prove that one bare integer literal fits the ABI representation selected elsewhere in its
/// generic call. Explicitly suffixed literals never reach this helper.
pub(in crate::semantic_contract) fn integer_literal_fits_abi(
    db: &dyn Db,
    key: AstNodeKey,
    expected: SemanticTypeId,
) -> Result<bool, SemanticError> {
    let Some(text) = integer_literal_text(db, key)? else {
        return Ok(false);
    };
    if let Some(magnitude) = text.strip_prefix('-').and_then(integer_literal_u64) {
        return Ok(match expected {
            SemanticTypeId::I32 => magnitude <= (i32::MAX as u64) + 1,
            SemanticTypeId::I64 => magnitude <= (i64::MAX as u64) + 1,
            SemanticTypeId::U32 | SemanticTypeId::U8 | SemanticTypeId::WORD => false,
            _ => false,
        });
    }
    let value = integer_literal_u64(&text);
    Ok(match expected {
        SemanticTypeId::I32 => value.is_some_and(|value| i32::try_from(value).is_ok()),
        SemanticTypeId::I64 => value.is_some_and(|value| i64::try_from(value).is_ok()),
        SemanticTypeId::U32 => value.is_some_and(|value| u32::try_from(value).is_ok()),
        SemanticTypeId::U8 => value.is_some_and(|value| u8::try_from(value).is_ok()),
        SemanticTypeId::WORD => value.is_some(),
        _ => false,
    })
}

fn contextual_integer_operand_fits_abi(
    db: &dyn Db,
    key: AstNodeKey,
    expected: SemanticTypeId,
) -> Result<bool, SemanticError> {
    if integer_literal_text(db, key)?.is_some() {
        return integer_literal_fits_abi(db, key, expected);
    }
    let Some(value) = contextual_constant_integer(db, key)? else {
        return Ok(false);
    };
    Ok(match expected {
        SemanticTypeId::I32 => i32::try_from(value).is_ok(),
        SemanticTypeId::I64 => true,
        SemanticTypeId::U32 => u32::try_from(value).is_ok(),
        SemanticTypeId::U8 => u8::try_from(value).is_ok(),
        SemanticTypeId::WORD => u64::try_from(value).is_ok(),
        _ => false,
    })
}

pub(in crate::semantic_contract) fn integer_literal_text(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<Option<Arc<str>>, SemanticError> {
    let Some(literal) = literal_fact(db, key)? else {
        let Some(children) = child_nodes(db, key)? else {
            return Ok(None);
        };
        if node_kind(db, key)? == Some(IndexedNodeKind::UnaryExpression)
            && operator_fact(db, key)? == Some(OperatorFact::Neg)
        {
            let [_, operand] = children.as_ref() else {
                return Ok(None);
            };
            return Ok(integer_literal_text(db, *operand)?.map(|text| Arc::from(format!("-{text}"))));
        }
        let [child] = children.as_ref() else {
            return Ok(None);
        };
        return integer_literal_text(db, *child);
    };
    match literal {
        LiteralFact::Integer(text) if !integer_has_explicit_abi_suffix(&text) => Ok(Some(text)),
        _ => Ok(None),
    }
}

pub(in crate::semantic_contract) fn contextual_constant_integer(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<Option<i64>, SemanticError> {
    if let Some(value) = constant_integer(db, key)? {
        return Ok(Some(value));
    }
    let Some(children) = child_nodes(db, key)? else {
        return Ok(None);
    };
    let [child] = children.as_ref() else {
        return Ok(None);
    };
    contextual_constant_integer(db, *child)
}

pub(in crate::semantic_contract) fn integer_has_explicit_abi_suffix(text: &str) -> bool {
    matches!(text.rsplit_once('_').map(|(_, suffix)| suffix), Some("i32" | "i64" | "u32" | "u8"))
}

/// Parse a source integer's magnitude while preserving a hexadecimal word-sized bit pattern.
///
/// The caller is responsible for ABI suffix handling. Negative values are deliberately excluded:
/// this helper is used only for selecting and checking unsigned ABI representations.
pub(in crate::semantic_contract) fn integer_literal_u64(text: &str) -> Option<u64> {
    match text.strip_prefix("0x") {
        Some(digits) => u64::from_str_radix(&digits.replace('_', ""), 16).ok(),
        None => text.replace('_', "").parse::<u64>().ok(),
    }
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn call_argument_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, _node| {
        Some((|| {
            if integer_literal_text(db, key)?.is_none() {
                return Err(SemanticError::unavailable("call_argument_abi_type"));
            }
            let mut current = key.node;
            while let Some(parent) = index.metadata_for(key.generation, current).and_then(|meta| meta.parent) {
                let parent_key = AstNodeKey { node: parent, ..key };
                if index.kind(parent) == Some(beskid_analysis::syntax_query::NodeKind::CallExpression) {
                    if index
                        .node_at(program, parent)
                        .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
                        .and_then(primitive_numeric_conversion_target)
                        .is_some()
                    {
                        return Err(SemanticError::unavailable("call_argument_abi_type"));
                    }
                    let arguments = call_arguments(db, parent_key)?
                        .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"))?;
                    let argument_index = arguments.iter().position(|argument| {
                        is_transparent_call_integer_path(index, key.generation, argument.node, key.node)
                    });
                    let Some(argument_index) = argument_index else {
                        current = parent;
                        continue;
                    };
                    let signature = call_abi_signature(db, parent_key)?
                        .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"))?;
                    let expected = signature
                        .parameters
                        .get(argument_index)
                        .copied()
                        .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"));
                    return expected.and_then(|expected| {
                        (primitive_integer(expected) && integer_literal_fits_abi(db, key, expected)?)
                            .then_some(expected)
                            .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"))
                    });
                }
                current = parent;
            }
            Err(SemanticError::unavailable("call_argument_abi_type"))
        })())
    })?
    .transpose()
}

/// Whether a proven integer literal reaches its call-argument boundary through singleton syntax
/// wrappers only. Unary negation is admitted because [`integer_literal_text`] has already proved
/// the whole subtree is one literal; binary arithmetic and nested calls remain non-transparent.
fn is_transparent_call_integer_path(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    generation: SyntaxGenerationId,
    argument: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> bool {
    use beskid_analysis::syntax_query::NodeKind;

    let mut current = node;
    while current != argument {
        let Some(parent) = index.metadata_for(generation, current).and_then(|meta| meta.parent) else {
            return false;
        };
        if !matches!(
            index.kind(parent),
            Some(
                NodeKind::Expression
                    | NodeKind::LiteralExpression
                    | NodeKind::GroupedExpression
                    | NodeKind::UnaryExpression
            )
        ) {
            return false;
        }
        current = parent;
    }
    true
}

/// Contextual ABI for an unsuffixed integer literal used directly as one operand of a
/// homogeneous primitive-integer binary expression. This is representation selection, not
/// widening: the sibling must already prove the exact ABI type.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn binary_operand_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |_program, index, _node| {
        Some((|| {
            if integer_literal_text(db, key)?.is_none() && contextual_constant_integer(db, key)?.is_none() {
                return Err(SemanticError::unavailable("binary_operand_abi_type"));
            }
            let mut parent = index.metadata_for(key.generation, key.node).and_then(|meta| meta.parent);
            while parent
                .is_some_and(|node| index.kind(node) != Some(beskid_analysis::syntax_query::NodeKind::BinaryExpression))
            {
                parent = parent.and_then(|node| index.metadata_for(key.generation, node).and_then(|meta| meta.parent));
            }
            let parent = parent.ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            let mut branch = key.node;
            while index.metadata_for(key.generation, branch).and_then(|meta| meta.parent) != Some(parent) {
                branch = index
                    .metadata_for(key.generation, branch)
                    .and_then(|meta| meta.parent)
                    .ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            }
            if !is_transparent_binary_operand_path(index, branch, key.node) {
                return Err(SemanticError::unavailable("binary_operand_abi_type"));
            }
            let children =
                index.children(parent).ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            let sibling = children
                .iter()
                .copied()
                .filter(|child| *child != branch)
                .find(|child| index.kind(*child) != Some(beskid_analysis::syntax_query::NodeKind::BinaryOp))
                .ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            let sibling = AstNodeKey { node: normalized_expression_node(index, sibling), ..key };
            if contextual_constant_integer(db, key)?.is_some() && integer_literal_text(db, sibling)?.is_some() {
                return Err(SemanticError::unavailable("binary_operand_abi_type"));
            }
            let expected =
                abi_type(db, sibling)?.ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            (primitive_integer(expected) && contextual_integer_operand_fits_abi(db, key, expected)?)
                .then_some(expected)
                .ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))
        })())
    })?
    .transpose()
}
