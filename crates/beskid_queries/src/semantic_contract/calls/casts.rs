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
    let expected = match expected_cast_type(db, key)? {
        Ok(expected) => expected,
        Err(error) => return Some(Err(error)),
    };
    let actual = literal_fact(db, key)
        .ok()
        .flatten()
        .map(|literal| match literal {
            LiteralFact::Integer(ref value) => {
                semantic_type_for_literal(&beskid_analysis::syntax::Literal::Integer(value.to_string()))
            }
            LiteralFact::Float(text) if text.ends_with("_f32") => SemanticTypeId::F32,
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

/// The numeric coercion target of an expression: the ABI of its [`expected_type`].
///
/// A typed `let`, a callee parameter, a declared return, an enum payload, a struct field, an
/// expected array element, a forwarded match arm or block result, and a binary operand's peer
/// all establish the target before ISLE emits the value; lowering never guesses a machine-width
/// conversion. A discarded expression-statement value has no coercion target, and a position
/// with no expected type has no cast fact.
pub(in crate::semantic_contract) fn expected_cast_type(
    db: &dyn Db,
    key: AstNodeKey,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    match expected_type(db, key) {
        Ok(Some(expected)) => match expected.context {
            ExpectedTypeContext::ExpressionStatement { .. } => None,
            _ => Some(Ok(expected.abi_type())),
        },
        Ok(None) => None,
        Err(error) if error.is_unavailable() => None,
        Err(error) => Some(Err(error)),
    }
}

pub(in crate::semantic_contract) fn primitive_numeric(semantic_type: SemanticTypeId) -> bool {
    matches!(
        semantic_type,
        SemanticTypeId::I8
            | SemanticTypeId::I16
            | SemanticTypeId::U16
            | SemanticTypeId::U64
            | SemanticTypeId::I32
            | SemanticTypeId::I64
            | SemanticTypeId::U32
            | SemanticTypeId::U8
            | SemanticTypeId::WORD
            | SemanticTypeId::F32
            | SemanticTypeId::F64
    )
}

pub(in crate::semantic_contract) fn primitive_integer(semantic_type: SemanticTypeId) -> bool {
    matches!(
        semantic_type,
        SemanticTypeId::I8
            | SemanticTypeId::I16
            | SemanticTypeId::U16
            | SemanticTypeId::U64
            | SemanticTypeId::I32
            | SemanticTypeId::I64
            | SemanticTypeId::U32
            | SemanticTypeId::U8
            | SemanticTypeId::WORD
    )
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
