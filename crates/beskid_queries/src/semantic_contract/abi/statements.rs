//! Exact ABI facts for non-generic statement results and storage boundaries.

use super::super::*;

pub(in crate::semantic_contract) fn statement_abi_type_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<SemanticTypeId, SemanticError>> {
    if let Some(binding) = node.of::<beskid_analysis::syntax::LetStatement>() {
        let declared = binding.type_annotation.as_ref().map_or_else(
            || inferred_local_storage_type(db, program, index, key),
            |ty| abi_type_from_syntax(db, key, &ty.node),
        );
        return Some(declared.and_then(|storage| {
            let value = direct_expression_key(program, index, key, &binding.value)?;
            require_exact_value_type(db, value, storage)
        }));
    }

    if let Some(statement) = node.of::<beskid_analysis::syntax::ReturnStatement>() {
        return Some(return_statement_abi_type(db, program, index, key, statement));
    }

    if let Some(statement) = node.of::<beskid_analysis::syntax::ExpressionStatement>() {
        return Some(
            direct_expression_key(program, index, key, &statement.expression)
                .and_then(|expression| required_value_type(db, expression)),
        );
    }

    if let Some(assignment) = node.of::<beskid_analysis::syntax::AssignExpression>() {
        return Some(assignment_abi_type(db, program, index, key, assignment));
    }

    None
}

fn inferred_local_storage_type(
    db: &dyn Db,
    _program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Result<SemanticTypeId, SemanticError> {
    let declaration = index
        .children(key.node)
        .and_then(|children| {
            children
                .iter()
                .copied()
                .find(|child| index.kind(*child) == Some(beskid_analysis::syntax_query::NodeKind::Identifier))
        })
        .map(|node| AstNodeKey { node, ..key })
        .ok_or_else(|| SemanticError::unavailable("value_abi_type"))?;
    match abi_type(db, declaration) {
        Ok(Some(storage)) => Ok(storage),
        Ok(None) => node_type(db, declaration)?.ok_or_else(|| SemanticError::unavailable("value_abi_type")),
        Err(error) if error.is_unavailable() => {
            node_type(db, declaration)?.ok_or_else(|| SemanticError::unavailable("value_abi_type"))
        }
        Err(error) => Err(error),
    }
}

fn return_statement_abi_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    statement: &beskid_analysis::syntax::ReturnStatement,
) -> Result<SemanticTypeId, SemanticError> {
    let item = nearest_ancestor(index, key.node, |kind| {
        matches!(
            kind,
            beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                | beskid_analysis::syntax_query::NodeKind::MethodDefinition
                | beskid_analysis::syntax_query::NodeKind::TestDefinition
        )
    })
    .ok_or_else(|| SemanticError::unavailable("value_abi_type"))?;
    let signature = item_abi_signature(db, AstNodeKey { node: item, ..key })?
        .ok_or_else(|| SemanticError::unavailable("value_abi_type"))?;
    match &statement.value {
        Some(value) => {
            let value = direct_expression_key(program, index, key, value)?;
            require_exact_value_type(db, value, signature.result)
        }
        None if signature.result == SemanticTypeId::UNIT => Ok(SemanticTypeId::UNIT),
        None => Err(SemanticError::unavailable("value_abi_type")),
    }
}

fn assignment_abi_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    assignment: &beskid_analysis::syntax::AssignExpression,
) -> Result<SemanticTypeId, SemanticError> {
    if !matches!(assignment.op.node, beskid_analysis::syntax::AssignOp::Assign) {
        return Err(SemanticError::unavailable("value_abi_type"));
    }
    let storage = match mutable_local_assignment(db, key)? {
        Some(write) => abi_type(db, write.declaration)?.ok_or_else(|| SemanticError::unavailable("value_abi_type"))?,
        None => array_index_element_abi_type(db, key)?.ok_or_else(|| SemanticError::unavailable("value_abi_type"))?,
    };
    let value = direct_expression_key(program, index, key, assignment.value.as_ref())?;
    require_exact_value_type(db, value, storage)
}

fn direct_expression_key(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    parent: AstNodeKey,
    expression: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
) -> Result<AstNodeKey, SemanticError> {
    index
        .direct_child_id(program, parent.node, beskid_analysis::syntax_query::DynNodeRef::from(expression))
        .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..parent })
        .ok_or_else(|| SemanticError::unavailable("value_abi_type"))
}

fn required_value_type(db: &dyn Db, key: AstNodeKey) -> Result<SemanticTypeId, SemanticError> {
    value_abi_type(db, key)?.ok_or_else(|| SemanticError::unavailable("value_abi_type"))
}

fn require_exact_value_type(
    db: &dyn Db,
    value: AstNodeKey,
    expected: SemanticTypeId,
) -> Result<SemanticTypeId, SemanticError> {
    (required_value_type(db, value)? == expected)
        .then_some(expected)
        .ok_or_else(|| SemanticError::unavailable("value_abi_type"))
}
