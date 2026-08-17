//! Focused call-semantics implementation.

use super::super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn cast_intents_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[CastIntent]>> {
    with_node(db, syntax, key, |program, index, node| cast_intents_for_node(db, program, index, key, node))?.transpose()
}

pub(in crate::semantic_contract) fn cast_intents_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<Arc<[CastIntent]>, SemanticError>> {
    if !expression_fact_target(node.node_kind()) {
        return None;
    }
    let expected = match expected_cast_type(db, program, index, key)? {
        Ok(expected) => expected,
        Err(error) => return Some(Err(error)),
    };
    let actual = literal_fact(db, key)
        .ok()
        .flatten()
        .map(|literal| match literal {
            LiteralFact::Integer(_) => SemanticTypeId::I32,
            LiteralFact::Float(_) => SemanticTypeId::F64,
            LiteralFact::Bool(_) => SemanticTypeId::BOOL,
            LiteralFact::Char(_) => SemanticTypeId::CHAR,
            LiteralFact::String(_) => SemanticTypeId::STRING,
        })
        .or_else(|| {
            abi_type(db, key)
                .ok()
                .flatten()
                .or_else(|| semantic_type_for_node(program, index, key.node, node).and_then(Result::ok))
        })
        .ok_or_else(|| SemanticError::unavailable("cast_intents"));
    let actual = match actual {
        Ok(actual) => actual,
        Err(error) => return Some(Err(error)),
    };
    if actual == expected {
        return Some(Ok(Arc::from([])));
    }
    if primitive_numeric(actual) && primitive_numeric(expected) {
        return Some(Ok(Arc::from([CastIntent { from: actual, to: expected }])));
    }
    Some(Err(SemanticError::unavailable("cast_intents")))
}

/// Resolve the exact explicit constraint that gives an expression a numeric coercion target.
///
/// A typed `let` remains the original source of cast intent.  A direct call contributes the
/// corresponding parameter type only when its declaration or canonical ABI-v5 intrinsic
/// signature is known from generation-safe syntax facts.  This establishes the target before
/// ISLE emits the literal; lowering never guesses a machine-width conversion.
pub(in crate::semantic_contract) fn expected_cast_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    let nearest_call =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::CallExpression);
    if nearest_call
        .and_then(|call_id| index.node_at(program, call_id))
        .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
        .and_then(primitive_numeric_conversion_target)
        .is_some()
    {
        return None;
    }
    if let Some(binary_id) =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::BinaryExpression)
    {
        let operands = index
            .children(binary_id)?
            .iter()
            .copied()
            .filter(|child| index.kind(*child) != Some(beskid_analysis::syntax_query::NodeKind::BinaryOp))
            .collect::<Vec<_>>();
        let operand = operands.iter().copied().find(|operand| is_ancestor(index, *operand, key.node))?;
        let sibling = operands.into_iter().find(|candidate| *candidate != operand)?;
        if is_transparent_binary_operand_path(index, operand, key.node) {
            let sibling_node = index.node_at(program, sibling)?;
            return semantic_type_for_node(program, index, sibling, sibling_node)
                .map(|result| result.map_err(|_| SemanticError::unavailable("cast_intents")));
        }
    }

    if let Some(statement_id) =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::LetStatement)
    {
        let statement = index.node_at(program, statement_id)?.of::<beskid_analysis::syntax::LetStatement>()?;
        let value_id = index
            .children(statement_id)?
            .iter()
            .copied()
            .find(|child| index.kind(*child) == Some(beskid_analysis::syntax_query::NodeKind::Expression))?;
        if !is_ancestor(index, value_id, key.node) {
            return None;
        }
        return Some(
            statement
                .type_annotation
                .as_ref()
                .ok_or_else(|| SemanticError::unavailable("cast_intents"))
                .and_then(|expected| semantic_type_from_syntax(&expected.node)),
        );
    }

    if let Some(call_id) = nearest_call {
        let call = index.node_at(program, call_id)?.of::<beskid_analysis::syntax::CallExpression>()?;
        let argument_index = call.args.iter().position(|argument| {
            index
                .direct_child_id(program, call_id, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                .is_some_and(|argument_id| is_ancestor(index, argument_id, key.node))
        })?;
        let expected = match &call.callee.node {
            beskid_analysis::syntax::Expression::Path(path) => {
                let path = &path.node.path.node;
                let call_key = AstNodeKey { node: call_id, ..key };
                let builtin = beskid_analysis::builtins::builtin_for_path(
                    &path.segments.iter().map(|segment| segment.node.name.node.name.to_string()).collect::<Vec<_>>(),
                );
                if resolve_item_declaration(db, program, index, key, path).is_some() || builtin.is_some() {
                    call_abi_signature(db, call_key)
                        .ok()
                        .flatten()
                        .and_then(|signature| signature.parameters.get(argument_index).copied())
                        .or_else(|| {
                            builtin
                                .and_then(|(_, builtin)| builtin.params.get(argument_index).copied())
                                .and_then(builtin_type_to_semantic)
                        })
                        .or_else(|| {
                            path.segments.last().and_then(|segment| {
                                canonical_intrinsic_parameter_type(segment.node.name.node.name.as_str(), argument_index)
                            })
                        })
                } else if path.segments.len() == 1
                    && resolve_lexical_declaration(
                        program,
                        index,
                        call_id,
                        path.segments[0].node.name.node.name.as_str(),
                    )
                    .is_none()
                {
                    canonical_intrinsic_parameter_type(path.segments[0].node.name.node.name.as_str(), argument_index)
                } else {
                    None
                }
            }
            _ => None,
        };
        return Some(expected.ok_or_else(|| SemanticError::unavailable("cast_intents")));
    }

    let return_id =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::ReturnStatement)?;
    let mut item_id = parent_node(index, return_id)?;
    while !matches!(
        index.kind(item_id)?,
        beskid_analysis::syntax_query::NodeKind::FunctionDefinition
            | beskid_analysis::syntax_query::NodeKind::MethodDefinition
    ) {
        item_id = parent_node(index, item_id)?;
    }
    let item = index.node_at(program, item_id)?;
    let return_type = item
        .of::<beskid_analysis::syntax::FunctionDefinition>()
        .and_then(|function| function.return_type.as_ref().map(|annotation| &annotation.node))
        .or_else(|| {
            item.of::<beskid_analysis::syntax::MethodDefinition>()
                .and_then(|method| method.return_type.as_ref().map(|annotation| &annotation.node))
        })?;
    Some(semantic_type_from_syntax(return_type))
}

/// A binary comparison constrains only its own operand expression, not a nested call argument.
/// For example, `object == NativePointer(0)` must retain `NativePointer`'s `word` parameter
/// intent for `0`; the outer pointer comparison has no authority to coerce that argument.
pub(in crate::semantic_contract) fn is_transparent_binary_operand_path(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    operand: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> bool {
    use beskid_analysis::syntax_query::NodeKind;

    let mut current = node;
    while current != operand {
        let Some(parent) = parent_node(index, current) else {
            return false;
        };
        if !matches!(
            index.kind(parent),
            Some(NodeKind::Expression | NodeKind::LiteralExpression | NodeKind::GroupedExpression)
        ) {
            return false;
        }
        current = parent;
    }
    true
}

/// ABI-v5 intrinsic signatures are target-independent.  Selecting a supported target merely
/// accesses the generated canonical manifest; codegen still requires its non-forgeable runtime
/// capability before it can import any of these symbols.
pub(in crate::semantic_contract) fn canonical_intrinsic_parameter_type(
    name: &str,
    argument_index: usize,
) -> Option<SemanticTypeId> {
    let target = TargetMetadata::supported().into_iter().next()?;
    let manifest = AbiManifestV5::canonical_runtime(target);
    let intrinsic = manifest.intrinsic_metadata(name)?;
    abi_semantic_type(*intrinsic.params.get(argument_index)?)
}

pub(in crate::semantic_contract) fn abi_semantic_type(ty: AbiType) -> Option<SemanticTypeId> {
    Some(match ty {
        AbiType::Void => return None,
        AbiType::Pointer => SemanticTypeId::POINTER,
        AbiType::USize => SemanticTypeId::WORD,
        AbiType::I8 | AbiType::U8 => SemanticTypeId::U8,
        AbiType::I32 => SemanticTypeId::I32,
        AbiType::I64 => SemanticTypeId::I64,
        AbiType::F64 => SemanticTypeId::F64,
        _ => return None,
    })
}

pub(in crate::semantic_contract) fn primitive_numeric(semantic_type: SemanticTypeId) -> bool {
    matches!(
        semantic_type,
        SemanticTypeId::I32 | SemanticTypeId::I64 | SemanticTypeId::U8 | SemanticTypeId::WORD | SemanticTypeId::F64
    )
}

pub(in crate::semantic_contract) fn primitive_integer(semantic_type: SemanticTypeId) -> bool {
    matches!(semantic_type, SemanticTypeId::I32 | SemanticTypeId::I64 | SemanticTypeId::U8 | SemanticTypeId::WORD)
}

pub(in crate::semantic_contract) fn expression_fact_target(kind: beskid_analysis::syntax_query::NodeKind) -> bool {
    use beskid_analysis::syntax_query::NodeKind;

    matches!(
        kind,
        NodeKind::Expression
            | NodeKind::AssignExpression
            | NodeKind::BinaryExpression
            | NodeKind::UnaryExpression
            | NodeKind::CallExpression
            | NodeKind::MemberExpression
            | NodeKind::LiteralExpression
            | NodeKind::PathExpression
            | NodeKind::StructLiteralExpression
            | NodeKind::IndexExpression
            | NodeKind::ArrayLiteralExpression
            | NodeKind::CodeStringLiteral
            | NodeKind::EnumConstructorExpression
            | NodeKind::BlockExpression
            | NodeKind::GroupedExpression
            | NodeKind::TryExpression
            | NodeKind::SpawnExpression
            | NodeKind::LambdaExpression
            | NodeKind::MatchExpression
            | NodeKind::Literal
    )
}
