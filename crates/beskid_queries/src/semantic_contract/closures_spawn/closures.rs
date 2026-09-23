//! Closure environments, signatures, call targets, captures, and callable signatures.

use super::super::*;
use super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn closure_environment_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureEnvironment> {
    with_node(db, syntax, key, |program, index, node| closure_environment_for_node(db, program, index, key, node))?
        .transpose()
}

pub(in crate::semantic_contract) fn closure_environment_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<ClosureEnvironment, SemanticError>> {
    let lambda = node.of::<beskid_analysis::syntax::LambdaExpression>()?;
    let parameters = match lambda
        .parameters
        .iter()
        .map(|parameter| {
            index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(parameter))
                .ok_or_else(|| SemanticError::unavailable("closure_environment"))
                .and_then(|parameter| {
                    index
                        .children(parameter)
                        .and_then(|children| {
                            children.iter().copied().find(|child| {
                                index.kind(*child) == Some(beskid_analysis::syntax_query::NodeKind::Identifier)
                            })
                        })
                        .map(|node| AstNodeKey { node, ..key })
                        .ok_or_else(|| SemanticError::unavailable("closure_environment"))
                })
        })
        .collect::<Result<Vec<_>, _>>()
    {
        Ok(parameters) => parameters,
        Err(error) => return Some(Err(error)),
    };
    let captures = match closure_captures(db, program, index, key) {
        Ok(captures) => captures.into(),
        Err(error) => return Some(Err(error)),
    };
    Some(Ok(ClosureEnvironment { parameters: parameters.into(), captures }))
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn closure_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureSignature> {
    with_node(db, syntax, key, |program, index, node| closure_signature_for_node(db, program, index, key, node))?
        .transpose()
}

pub(in crate::semantic_contract) fn closure_signature_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<ClosureSignature, SemanticError>> {
    let lambda = node.of::<beskid_analysis::syntax::LambdaExpression>()?;
    let body = index
        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(lambda.body.as_ref()))
        .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })?;
    let callable = match callable_signature_for_node(db, program, index, key, node) {
        Some(Ok(callable)) => callable,
        Some(Err(error)) => return Some(Err(error)),
        None => return None,
    };
    let environment = match closure_environment_for_node(db, program, index, key, node) {
        Some(Ok(environment)) => environment,
        Some(Err(error)) => return Some(Err(error)),
        None => return None,
    };
    let fields = environment
        .captures
        .iter()
        .map(|capture| {
            abi_type(db, capture.declaration)
                .and_then(|ty| ty.ok_or_else(|| SemanticError::unavailable("closure_signature")))
                .map(|abi_type| ClosureEnvironmentField { capture: *capture, abi_type })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|mut fields| {
            fields.sort_by_key(|field| {
                (field.capture.slot.owner.node.0, field.capture.slot.index, field.capture.declaration.node.0)
            });
            fields
        });
    Some(fields.map(|fields| ClosureSignature {
        lambda: key,
        body,
        callable,
        environment: ClosureEnvironmentAbiShape {
            fields: fields.into(),
            pointer_map: ClosurePointerMapRequirement::RuntimeDescriptorRequired,
        },
        lowering: ClosureLoweringStatus::NotLowered,
        allocation: ClosureAllocationStatus::NotAllocated,
    }))
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn closure_call_target_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureCallTarget> {
    let lambda = with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let callee = index
            .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()))
            .map(|node| normalized_expression_node(index, node))?;
        let callee_node = index.node_at(program, callee)?;
        if callee_node.of::<beskid_analysis::syntax::LambdaExpression>().is_some() {
            return Some(AstNodeKey { node: callee, ..key });
        }

        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        let [segment] = path.node.path.node.segments.as_slice() else {
            return None;
        };
        if !segment.node.type_args.is_empty() {
            return None;
        }
        let declaration = resolve_lexical_declaration(program, index, callee, segment.node.name.node.name.as_str())?;
        let binding = parent_node(index, declaration)
            .and_then(|parent| index.node_at(program, parent)?.of::<beskid_analysis::syntax::LetStatement>())?;
        if !expression_is_lambda(&binding.value.node) {
            return None;
        }
        index
            .direct_child_id(
                program,
                parent_node(index, declaration)?,
                beskid_analysis::syntax_query::DynNodeRef::from(&binding.value),
            )
            .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
    })?;
    let Some(lambda) = lambda else {
        return Ok(None);
    };
    let Some(signature) = closure_signature_tracked(db, syntax, lambda)? else {
        return Ok(None);
    };
    Ok(Some(ClosureCallTarget {
        call: key,
        lambda: signature.lambda,
        body: signature.body,
        callable: signature.callable,
    }))
}

/// Capture local values and source-resolved nominal method receivers using the
/// same declaration identity as ordinary method calls. Unknown qualified paths
/// are not evidence of a captured receiver.
fn closure_capture_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    if let [segment] = path.segments.as_slice() {
        if !segment.node.type_args.is_empty() {
            return None;
        }
        return resolve_lexical_declaration(program, index, key.node, &segment.node.name.node.name)
            .map(|node| AstNodeKey { node, ..key });
    }
    nominal_local_member_receiver(db, program, index, key, path).map(|(_, receiver)| receiver)
}

pub(in crate::semantic_contract) fn closure_captures(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    lambda: AstNodeKey,
) -> Result<Vec<ClosureCapture>, SemanticError> {
    let mut captures: Vec<ClosureCapture> = Vec::new();
    for path_id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::PathExpression) {
        if !is_ancestor(index, lambda.node, path_id) {
            continue;
        }
        let Some(node) = index.node_at(program, path_id) else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let Some(declaration) =
            closure_capture_declaration(db, program, index, AstNodeKey { node: path_id, ..lambda }, &path.path.node)
        else {
            continue;
        };
        if is_ancestor(index, lambda.node, declaration.node) {
            continue;
        }
        if captures.iter().any(|capture| capture.declaration == declaration) {
            continue;
        }
        let Some(slot) = local_slot_for_declaration(index, declaration) else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let Some(span) = node.span() else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let class = capture_storage_class(db, program, index, declaration)?;
        captures.push(ClosureCapture { declaration, slot: slot?, class, span });
    }
    Ok(captures)
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn capture_storage_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CaptureStorage> {
    with_node(db, syntax, key, |program, index, node| capture_storage_for_node(db, program, index, key, node))?
        .transpose()
}

pub(in crate::semantic_contract) fn capture_storage_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<CaptureStorage, SemanticError>> {
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let declaration = closure_capture_declaration(db, program, index, key, &path.path.node)?;
    let span = node.span()?;
    Some(capture_storage_class(db, program, index, declaration).map(|class| CaptureStorage {
        declaration,
        class,
        span,
    }))
}

pub(in crate::semantic_contract) fn capture_storage_class(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
) -> Result<CaptureStorageClass, SemanticError> {
    let parent = parent_node(index, declaration.node).ok_or_else(|| SemanticError::unavailable("capture_storage"))?;
    let mutable = index
        .node_at(program, parent)
        .and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())
        .is_some_and(|binding| binding.mutable);
    let semantic_type = abi_type(db, declaration)?.ok_or_else(|| SemanticError::unavailable("capture_storage"))?;
    // ABI pointers alone carry no ownership provenance. The existing source-type
    // query distinguishes native pointers from managed arrays and nominal values.
    let managed = managed_reference_kind(db, declaration).ok().flatten() == Some(ManagedReferenceKind::GcManaged);
    Ok(if mutable || (semantic_type == SemanticTypeId::POINTER && !managed) {
        CaptureStorageClass::StackReference
    } else {
        CaptureStorageClass::TransferableValue
    })
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn callable_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |program, index, node| callable_signature_for_node(db, program, index, key, node))?
        .transpose()
}

pub(in crate::semantic_contract) fn callable_signature_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<ItemSignature, SemanticError>> {
    if let Some(signature) = item_signature_for_node(node) {
        return Some(signature);
    }
    if let Some(lambda) = node.of::<beskid_analysis::syntax::LambdaExpression>() {
        let parameters = lambda
            .parameters
            .iter()
            .map(|parameter| {
                parameter.node.ty.as_ref().map_or_else(
                    || Err(SemanticError::unavailable("callable_signature")),
                    |ty| abi_type_from_syntax(db, key, &ty.node),
                )
            })
            .collect::<Result<Vec<_>, _>>();
        let result = index
            .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(lambda.body.as_ref()))
            .ok_or_else(|| SemanticError::unavailable("callable_signature"))
            .and_then(|node| {
                value_abi_type(db, AstNodeKey { node: normalized_expression_node(index, node), ..key })?
                    .ok_or_else(|| SemanticError::unavailable("callable_signature"))
            });
        return Some(
            parameters
                .and_then(|parameters| result.map(|result| ItemSignature { parameters: parameters.into(), result })),
        );
    }
    if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        return callable_signature_for_path(db, program, index, key, &path.path.node);
    }
    if let Some(call) = node.of::<beskid_analysis::syntax::CallExpression>()
        && let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node
    {
        return callable_signature_for_path(db, program, index, key, &path.node.path.node);
    }
    None
}

pub(in crate::semantic_contract) fn callable_signature_for_path(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<Result<ItemSignature, SemanticError>> {
    let declaration = resolve_item_declaration(db, program, index, key, path)?;
    Some(
        item_abi_signature(db, declaration)
            .and_then(|signature| signature.ok_or_else(|| SemanticError::unavailable("callable_signature"))),
    )
}
