//! Spawn targets, entry operands, handle types, entry validation, and stack capture.

use super::super::*;
use super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn spawn_target_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnTarget> {
    with_node(db, syntax, key, |program, index, node| {
        let spawn = node.of::<beskid_analysis::syntax::SpawnExpression>()?;
        let callee = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(spawn.callee.as_ref()),
        )?;
        let callee = AstNodeKey { node: normalized_expression_node(index, callee), ..key };
        let (callee, arguments) = match spawn_entry_operand(program, index, callee) {
            Ok(operand) => operand,
            Err(error) => return Some(Err(error)),
        };
        let captures = if index.kind(callee.node) == Some(beskid_analysis::syntax_query::NodeKind::LambdaExpression) {
            match closure_captures(db, program, index, callee) {
                Ok(captures) => captures.into(),
                Err(error) => return Some(Err(error)),
            }
        } else {
            Arc::from([])
        };
        Some(Ok(SpawnTarget { callee, arguments: arguments.into(), captures }))
    })?
    .transpose()
}

/// Resolve the fiber entry operand and eager arguments for one spawn callee expression.
///
/// `spawn Entry()` and `spawn Entry(args)` unwrap to the `Entry` operand, matching production
/// lowering; the normalized argument expressions are returned in source order. A bare callee
/// has no arguments.
pub(in crate::semantic_contract) fn spawn_entry_operand(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    callee: AstNodeKey,
) -> Result<(AstNodeKey, Vec<AstNodeKey>), SemanticError> {
    let Some(node) = index.node_at(program, callee.node) else {
        return Ok((callee, Vec::new()));
    };
    let Some(call) = node.of::<beskid_analysis::syntax::CallExpression>() else {
        return Ok((callee, Vec::new()));
    };
    let entry = index
        .direct_child_id(program, callee.node, beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()))
        .ok_or_else(|| SemanticError::unavailable("spawn_target"))?;
    let arguments = call
        .args
        .iter()
        .map(|argument| {
            index
                .direct_child_id(program, callee.node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..callee })
                .ok_or_else(|| SemanticError::unavailable("spawn_target"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((AstNodeKey { node: normalized_expression_node(index, entry), ..callee }, arguments))
}

pub(in crate::semantic_contract) fn normalized_expression_node(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    mut node: beskid_analysis::syntax::AstNodeId,
) -> beskid_analysis::syntax::AstNodeId {
    while matches!(
        index.kind(node),
        Some(
            beskid_analysis::syntax_query::NodeKind::Expression
                | beskid_analysis::syntax_query::NodeKind::GroupedExpression
        )
    ) {
        let Some(child) = index.children(node).and_then(|children| children.first()).copied() else {
            break;
        };
        node = child;
    }
    node
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn spawn_handle_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnHandleType> {
    // Source identity is needed to compute ownership, so it cannot depend on
    // ownership legality. Only spawn legality/entry validation authorize emission.
    let Some(target) = spawn_target_tracked(db, syntax, key)? else {
        return Ok(None);
    };
    let declaration =
        layouts::unique_assembled_type_in_module(db, key, &["Concurrency".into(), "Fiber".into()], "Fiber", 1)
            .or_else(|| unique_type_in_unit(db, key.unit, key.generation, "Fiber", 1))
            .ok_or_else(|| SemanticError::unavailable("spawn_handle_type"))?;
    let target = target.callee;
    let identity = if let Some(lambda) = closure_signature(db, target)? {
        generic_source_expression_identity(db, lambda.body)?
    } else {
        let callable =
            resolved_item(db, target)?.ok_or_else(|| SemanticError::unavailable("spawn_handle_type"))?.declaration;
        let callable_syntax =
            db.syntax_unit(callable.unit).ok_or_else(|| SemanticError::unavailable("spawn_handle_type"))?;
        let function = callable_syntax
            .syntax_index(db)
            .node_at(callable_syntax.expanded_program(db), callable.node)
            .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
            .ok_or_else(|| SemanticError::unavailable("spawn_handle_type"))?;
        match function.return_type.as_ref() {
            Some(result) => generic_source_type_identity(db, callable, &result.node)?,
            None => GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT),
        }
    };
    let declaration_syntax =
        db.syntax_unit(declaration.unit).ok_or_else(|| SemanticError::unavailable("spawn_handle_type"))?;
    let parameter = declaration_syntax
        .syntax_index(db)
        .node_at(declaration_syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .and_then(|definition| definition.generics.first())
        .ok_or_else(|| SemanticError::unavailable("spawn_handle_type"))?;
    Ok(Some(SpawnHandleType {
        declaration,
        payload: GenericSubstitution::from_source(parameter.node.name.as_str(), identity.abi_type(), identity),
    }))
}

/// Project a Fiber receiver from the ordinary source-identity authority, including moves and
/// callable returns. This never reconstructs a nominal payload from its erased pointer ABI.
pub(in crate::semantic_contract) fn inferred_spawn_handle(
    db: &dyn Db,
    receiver: AstNodeKey,
) -> Option<SpawnHandleType> {
    let syntax = db.syntax_unit(receiver.unit)?;
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let parent = parent_node(index, receiver.node)?;
    let statement = index.node_at(program, parent)?.of::<beskid_analysis::syntax::LetStatement>()?;
    let initializer =
        index.direct_child_id(program, parent, beskid_analysis::syntax_query::DynNodeRef::from(&statement.value))?;
    let identity = generic_source_expression_identity(
        db,
        AstNodeKey { node: normalized_expression_node(index, initializer), ..receiver },
    )
    .ok()?;
    let GenericSourceTypeIdentity::Nominal { qualified_name, arguments } = identity else {
        return None;
    };
    let declaration =
        layouts::unique_assembled_type_in_module(db, receiver, &["Concurrency".into(), "Fiber".into()], "Fiber", 1)
            .or_else(|| unique_type_in_unit(db, receiver.unit, receiver.generation, "Fiber", 1))?;
    if stable_declaration_identity(db, declaration)? != qualified_name || arguments.len() != 1 {
        return None;
    }
    let target = db.syntax_unit(declaration.unit)?;
    let parameter = target
        .syntax_index(db)
        .node_at(target.expanded_program(db), declaration.node)?
        .of::<beskid_analysis::syntax::TypeDefinition>()?
        .generics
        .first()?;
    let payload = arguments[0].clone();
    Some(SpawnHandleType {
        declaration,
        payload: GenericSubstitution::from_source(parameter.node.name.as_str(), payload.abi_type(), payload),
    })
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn spawn_entry_validation_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnEntryValidation> {
    let Some(legality) = spawn_legality_tracked(db, syntax, key)? else {
        return Ok(None);
    };
    let callable = callable_signature_tracked(db, syntax, legality.target.callee)?;
    let is_legal_entry =
        callable.as_ref().is_some_and(|callable| callable.parameters.len() == legality.target.arguments.len())
            && legality.is_legal();
    Ok(Some(SpawnEntryValidation {
        spawn: key,
        target: legality.target.callee,
        arguments: legality.target.arguments,
        callable,
        is_legal_entry,
        diagnostics: legality.diagnostics,
    }))
}

pub(in crate::semantic_contract) fn spawn_stack_capture(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    lambda: AstNodeKey,
    captures: &[ClosureCapture],
) -> Result<Option<CaptureStorage>, SemanticError> {
    let index = syntax.syntax_index(db);
    if index.kind(lambda.node) != Some(beskid_analysis::syntax_query::NodeKind::LambdaExpression) {
        return Ok(None);
    }
    for path in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::PathExpression) {
        if !is_ancestor(index, lambda.node, path) {
            continue;
        }
        let reference = AstNodeKey { node: path, ..lambda };
        let Some(storage) = capture_storage_tracked(db, syntax, reference)? else {
            continue;
        };
        if storage.class == CaptureStorageClass::StackReference
            && captures.iter().any(|capture| capture.declaration == storage.declaration)
        {
            return Ok(Some(storage));
        }
    }
    Ok(None)
}
