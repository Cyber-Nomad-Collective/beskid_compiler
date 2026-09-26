//! Syntax-authoritative event operation facts.

use super::*;
use beskid_analysis::syntax_query::DynNodeRef;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn event_operation_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EventOperationFact> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(assignment) = node.of::<beskid_analysis::syntax::AssignExpression>() {
            let operation = match assignment.op.node {
                beskid_analysis::syntax::AssignOp::AddAssign => EventOperationKind::Subscribe,
                beskid_analysis::syntax::AssignOp::SubAssign => EventOperationKind::UnsubscribeFirst,
                beskid_analysis::syntax::AssignOp::Assign => return None,
            };
            let target = normalized_expression_node(
                index,
                index.direct_child_id(program, key.node, DynNodeRef::from(assignment.target.as_ref()))?,
            );
            let value = normalized_expression_node(
                index,
                index.direct_child_id(program, key.node, DynNodeRef::from(assignment.value.as_ref()))?,
            );
            return event_fact_for_target(
                db,
                program,
                index,
                key,
                target,
                operation,
                Some(AstNodeKey { node: value, ..key }),
                Arc::from([]),
            );
        }
        if let Some(call) = node.of::<beskid_analysis::syntax::CallExpression>() {
            let callee = normalized_expression_node(
                index,
                index.direct_child_id(program, key.node, DynNodeRef::from(call.callee.as_ref()))?,
            );
            let arguments = call
                .args
                .iter()
                .map(|argument| {
                    index
                        .direct_child_id(program, key.node, DynNodeRef::from(argument))
                        .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
                })
                .collect::<Option<Vec<_>>>()?;
            return event_fact_for_target(
                db,
                program,
                index,
                key,
                callee,
                EventOperationKind::Raise,
                None,
                arguments.into(),
            );
        }
        None
    })?
    .transpose()
}

fn event_fact_for_target(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    operation_node: AstNodeKey,
    target: beskid_analysis::syntax::AstNodeId,
    operation: EventOperationKind,
    handler: Option<AstNodeKey>,
    arguments: Arc<[AstNodeKey]>,
) -> Option<Result<EventOperationFact, SemanticError>> {
    let target_node = index.node_at(program, target)?;
    let receiver = field_access_receiver(db, program, index, operation_node, target_node, None, None)?;
    let receiver = match receiver {
        Ok(receiver) => receiver,
        Err(error) => return Some(Err(error)),
    };
    let declaration_syntax =
        db.syntax_unit(receiver.declaration.unit).filter(|syntax| syntax.accepts_key(db, receiver.declaration))?;
    let declaration_program = declaration_syntax.expanded_program(db);
    let declaration_index = declaration_syntax.syntax_index(db);
    let definition = declaration_index
        .node_at(declaration_program, receiver.declaration.node)?
        .of::<beskid_analysis::syntax::TypeDefinition>()?;
    let field_position = definition.fields.iter().position(|field| {
        field.node.kind == beskid_analysis::syntax::FieldKind::Event && field.node.name.node.name == receiver.field_name
    })?;
    let field_node = declaration_index
        .children(receiver.declaration.node)?
        .iter()
        .copied()
        .filter(|id| {
            declaration_index
                .node_at(declaration_program, *id)
                .and_then(|node| node.of::<beskid_analysis::syntax::Field>())
                .is_some_and(|field| field.kind == beskid_analysis::syntax::FieldKind::Event)
        })
        .nth(field_position)?;
    let field_key = AstNodeKey { node: field_node, ..receiver.declaration };
    let layout = match event_field_layout(db, field_key) {
        Ok(Some(layout)) => layout,
        Ok(None) => return None,
        Err(error) => return Some(Err(error)),
    };
    let field = declaration_index.node_at(declaration_program, field_node)?.of::<beskid_analysis::syntax::Field>()?;
    let (parameters, result) = match &field.ty.node {
        beskid_analysis::syntax::Type::Function { parameters, return_type } => (
            parameters
                .iter()
                .map(|parameter| abi_type_from_syntax(db, field_key, &parameter.node))
                .collect::<Result<Vec<_>, _>>(),
            abi_type_from_syntax(db, field_key, &return_type.node),
        ),
        _ => return None,
    };
    let signature = match (parameters, result) {
        (Ok(parameters), Ok(result)) => ItemSignature { parameters: parameters.into(), result },
        (Err(error), _) | (_, Err(error)) => return Some(Err(error)),
    };
    if operation == EventOperationKind::Raise {
        if arguments.len() != signature.parameters.len() {
            return None;
        }
        for (argument, expected) in arguments.iter().zip(signature.parameters.iter()) {
            if value_abi_type(db, *argument).ok().flatten() != Some(*expected) {
                return None;
            }
        }
    }
    Some(Ok(EventOperationFact {
        operation,
        operation_node,
        receiver: receiver.receiver,
        declaration: layout.owner_type,
        field: layout.field,
        slot_offset: layout.slot_offset,
        capacity: layout.capacity,
        handler,
        arguments,
        delegate_signature: Some(signature),
    }))
}
