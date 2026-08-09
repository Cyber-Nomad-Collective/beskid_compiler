//! Focused semantic-contract implementation cluster.

use super::*;

#[salsa::tracked]
pub(super) fn closure_environment_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureEnvironment> {
    with_node(db, syntax, key, |program, index, node| closure_environment_for_node(program, index, key, node))?
        .transpose()
}

pub(super) fn closure_environment_for_node(
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
    let captures = match closure_captures(program, index, key) {
        Ok(captures) => captures.into(),
        Err(error) => return Some(Err(error)),
    };
    Some(Ok(ClosureEnvironment { parameters: parameters.into(), captures }))
}

#[salsa::tracked]
pub(super) fn closure_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureSignature> {
    with_node(db, syntax, key, |program, index, node| closure_signature_for_node(db, program, index, key, node))?
        .transpose()
}

pub(super) fn closure_signature_for_node(
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
    let environment = match closure_environment_for_node(program, index, key, node) {
        Some(Ok(environment)) => environment,
        Some(Err(error)) => return Some(Err(error)),
        None => return None,
    };
    let fields = environment
        .captures
        .iter()
        .map(|capture| {
            local_declaration_type(program, index, capture.declaration.node)
                .unwrap_or_else(|| Err(SemanticError::unavailable("closure_signature")))
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

#[salsa::tracked]
pub(super) fn closure_call_target_tracked(
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

pub(super) fn closure_captures(
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
        let Some(declaration) = resolve_lexical_declaration(
            program,
            index,
            path_id,
            path.path.node.segments.first().map(|segment| segment.node.name.node.name.as_str()).unwrap_or_default(),
        ) else {
            continue;
        };
        if path.path.node.segments.len() != 1 || is_ancestor(index, lambda.node, declaration) {
            continue;
        }
        let declaration = AstNodeKey { node: declaration, ..lambda };
        if captures.iter().any(|capture| capture.declaration == declaration) {
            continue;
        }
        let Some(slot) = local_slot_for_declaration(index, declaration) else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let Some(span) = node.span() else {
            return Err(SemanticError::unavailable("closure_environment"));
        };
        let class = capture_storage_class(program, index, declaration)?;
        captures.push(ClosureCapture { declaration, slot: slot?, class, span });
    }
    Ok(captures)
}

#[salsa::tracked]
pub(super) fn capture_storage_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CaptureStorage> {
    with_node(db, syntax, key, |program, index, node| capture_storage_for_node(program, index, key, node))?.transpose()
}

pub(super) fn capture_storage_for_node(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<CaptureStorage, SemanticError>> {
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let [segment] = path.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let declaration = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())?;
    let declaration = AstNodeKey { node: declaration, ..key };
    let span = node.span()?;
    Some(capture_storage_class(program, index, declaration).map(|class| CaptureStorage { declaration, class, span }))
}

pub(super) fn capture_storage_class(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
) -> Result<CaptureStorageClass, SemanticError> {
    let parent = parent_node(index, declaration.node).ok_or_else(|| SemanticError::unavailable("capture_storage"))?;
    let mutable = index
        .node_at(program, parent)
        .and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())
        .is_some_and(|binding| binding.mutable);
    let semantic_type = local_declaration_type(program, index, declaration.node)
        .unwrap_or_else(|| Err(SemanticError::unavailable("capture_storage")))?;
    Ok(if mutable || semantic_type == SemanticTypeId::POINTER {
        CaptureStorageClass::StackReference
    } else {
        CaptureStorageClass::TransferableValue
    })
}

#[salsa::tracked]
pub(super) fn callable_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |program, index, node| callable_signature_for_node(db, program, index, key, node))?
        .transpose()
}

pub(super) fn callable_signature_for_node(
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
                    |ty| semantic_type_from_syntax(&ty.node),
                )
            })
            .collect::<Result<Vec<_>, _>>();
        let result = semantic_type_for_expression(program, index, key.node, &lambda.body.node);
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

pub(super) fn callable_signature_for_path(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<Result<ItemSignature, SemanticError>> {
    let declaration = resolve_item_declaration(db, program, index, key, path)?;
    let declaration = index.node_at(program, declaration.node)?;
    item_signature_for_node(declaration)
}

#[salsa::tracked]
pub(super) fn spawn_target_tracked(db: &dyn Db, syntax: SyntaxUnitInput, key: AstNodeKey) -> SemanticQueryResult<SpawnTarget> {
    with_node(db, syntax, key, |program, index, node| {
        let spawn = node.of::<beskid_analysis::syntax::SpawnExpression>()?;
        let callee = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(spawn.callee.as_ref()),
        )?;
        let callee = AstNodeKey { node: normalized_expression_node(index, callee), ..key };
        let callee = match spawn_entry_operand(program, index, callee) {
            Ok(callee) => callee,
            Err(error) => return Some(Err(error)),
        };
        let captures = if index.kind(callee.node) == Some(beskid_analysis::syntax_query::NodeKind::LambdaExpression) {
            match closure_captures(program, index, callee) {
                Ok(captures) => captures.into(),
                Err(error) => return Some(Err(error)),
            }
        } else {
            Arc::from([])
        };
        Some(Ok(SpawnTarget { callee, captures }))
    })?
    .transpose()
}

/// Resolve the fiber entry operand for one spawn callee expression.
///
/// Empty-arg `spawn Entry()` sugar unwraps to `Entry`, matching production lowering. Call
/// expressions that still carry arguments remain the CallExpression node so legality can reject
/// them without inventing a trampoline.
pub(super) fn spawn_entry_operand(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    callee: AstNodeKey,
) -> Result<AstNodeKey, SemanticError> {
    let Some(node) = index.node_at(program, callee.node) else {
        return Ok(callee);
    };
    let Some(call) = node.of::<beskid_analysis::syntax::CallExpression>() else {
        return Ok(callee);
    };
    if !call.args.is_empty() {
        return Ok(callee);
    }
    let entry = index
        .direct_child_id(program, callee.node, beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()))
        .ok_or_else(|| SemanticError::unavailable("spawn_target"))?;
    Ok(AstNodeKey { node: normalized_expression_node(index, entry), ..callee })
}

pub(super) fn normalized_expression_node(
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

#[salsa::tracked]
pub(super) fn spawn_legality_tracked(db: &dyn Db, syntax: SyntaxUnitInput, key: AstNodeKey) -> SemanticQueryResult<SpawnLegality> {
    let target = spawn_target_tracked(db, syntax, key)?;
    let Some(target) = target else {
        return Ok(None);
    };
    let span = node_span_tracked(db, syntax, key)?.ok_or_else(|| SemanticError::unavailable("spawn_legality"))?;
    let index = syntax.syntax_index(db);
    if index.kind(target.callee.node) == Some(beskid_analysis::syntax_query::NodeKind::CallExpression) {
        // Non-empty `spawn Entry(args)` left the CallExpression in place; fail closed before
        // signature lookup so parameterized callees are not misdiagnosed as TargetRequiresArguments.
        return Ok(Some(SpawnLegality {
            target,
            result: None,
            span,
            diagnostics: Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::CalleeArgumentsUnsupported,
                span,
                capture: None,
            }]),
        }));
    }
    let signature = callable_signature_tracked(db, syntax, target.callee)?;
    let Some(signature) = signature else {
        return Ok(Some(SpawnLegality {
            target,
            result: None,
            span,
            diagnostics: Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::TargetNotCallable,
                span,
                capture: None,
            }]),
        }));
    };

    if !signature.parameters.is_empty() {
        return Ok(Some(SpawnLegality {
            target,
            result: Some(signature.result),
            span,
            diagnostics: Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::TargetRequiresArguments,
                span,
                capture: None,
            }]),
        }));
    }

    let capture = spawn_stack_capture(db, syntax, target.callee, &target.captures)?;
    let diagnostics = capture.map_or_else(
        || Arc::from([]),
        |capture| {
            Arc::from([SpawnDiagnostic {
                kind: SpawnDiagnosticKind::StackReferenceEscapesSpawn,
                span: capture.span,
                capture: Some(capture),
            }])
        },
    );
    Ok(Some(SpawnLegality { target, result: Some(signature.result), span, diagnostics }))
}

#[salsa::tracked]
pub(super) fn spawn_entry_validation_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnEntryValidation> {
    let Some(legality) = spawn_legality_tracked(db, syntax, key)? else {
        return Ok(None);
    };
    let callable = callable_signature_tracked(db, syntax, legality.target.callee)?;
    let is_zero_argument_entry =
        callable.as_ref().is_some_and(|callable| callable.parameters.is_empty()) && legality.is_legal();
    Ok(Some(SpawnEntryValidation {
        spawn: key,
        target: legality.target.callee,
        callable,
        is_zero_argument_entry,
        diagnostics: legality.diagnostics,
    }))
}

pub(super) fn spawn_stack_capture(
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

#[salsa::tracked]
pub(super) fn runtime_intrinsic_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<RuntimeIntrinsic> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return Some(Err(SemanticError::unavailable("runtime_intrinsic")));
        };
        if path.node.path.node.segments.len() == 1
            && resolve_lexical_declaration(
                program,
                index,
                key.node,
                path.node.path.node.segments[0].node.name.node.name.as_str(),
            )
            .is_some()
        {
            return Some(Err(SemanticError::unavailable("runtime_intrinsic")));
        }
        let segments =
            path.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
        beskid_analysis::builtins::builtin_for_path(&segments)
            .map(|(index, _)| {
                u32::try_from(index).map(RuntimeIntrinsic).map_err(|_| SemanticError::unavailable("runtime_intrinsic"))
            })
            .or_else(|| Some(Err(SemanticError::unavailable("runtime_intrinsic"))))
    })?
    .transpose()
}

#[salsa::tracked]
pub(super) fn runtime_intrinsic_name_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<RuntimeIntrinsicName> {
    with_node(db, syntax, key, |_program, _index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        if path.node.path.node.segments.len() != 1 {
            return None;
        }
        Some(Ok(RuntimeIntrinsicName(Arc::from(path.node.path.node.segments[0].node.name.node.name.as_str()))))
    })?
    .transpose()
}

