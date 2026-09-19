//! Focused semantic-contract implementation cluster.

use super::*;

#[salsa::tracked(persist)]
pub(super) fn closure_environment_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ClosureEnvironment> {
    with_node(db, syntax, key, |program, index, node| closure_environment_for_node(db, program, index, key, node))?
        .transpose()
}

pub(super) fn closure_environment_for_node(
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
        let class = capture_storage_class(db, program, index, declaration)?;
        captures.push(ClosureCapture { declaration, slot: slot?, class, span });
    }
    Ok(captures)
}

#[salsa::tracked(persist)]
pub(super) fn capture_storage_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CaptureStorage> {
    with_node(db, syntax, key, |program, index, node| capture_storage_for_node(db, program, index, key, node))?
        .transpose()
}

pub(super) fn capture_storage_for_node(
    db: &dyn Db,
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
    Some(capture_storage_class(db, program, index, declaration).map(|class| CaptureStorage {
        declaration,
        class,
        span,
    }))
}

pub(super) fn capture_storage_class(
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

pub(super) fn callable_signature_for_path(
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

#[salsa::tracked(persist)]
pub(super) fn spawn_target_tracked(
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
        let callee = match spawn_entry_operand(program, index, callee) {
            Ok(callee) => callee,
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

#[salsa::tracked(persist)]
pub(super) fn spawn_handle_type_tracked(
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
pub(super) fn inferred_spawn_handle(db: &dyn Db, receiver: AstNodeKey) -> Option<SpawnHandleType> {
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
pub(super) fn spawn_legality_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SpawnLegality> {
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
    let diagnostics = match capture {
        None => containing_fiber_callable(index, key.node)
            .map(|node| callable_fiber_ownership_tracked(db, syntax, AstNodeKey { node, ..key }))
            .transpose()?
            .flatten()
            .map_or_else(|| Arc::from([]), |fact| fact.diagnostics),
        Some(capture) => Arc::from([SpawnDiagnostic {
            kind: SpawnDiagnosticKind::StackReferenceEscapesSpawn,
            span: capture.span,
            capture: Some(capture),
        }]),
    };
    Ok(Some(SpawnLegality { target, result: Some(signature.result), span, diagnostics }))
}

fn containing_fiber_callable(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    use beskid_analysis::syntax_query::NodeKind;
    nearest_ancestor(index, node, |kind| {
        matches!(
            kind,
            NodeKind::FunctionDefinition
                | NodeKind::MethodDefinition
                | NodeKind::TestDefinition
                | NodeKind::LambdaExpression
        )
    })
}

#[salsa::tracked(persist)]
pub(super) fn callable_fiber_ownership_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<FiberOwnership> {
    use beskid_analysis::syntax_query::NodeKind;
    with_node(db, syntax, key, |program, index, _node| {
        if !matches!(
            index.kind(key.node),
            Some(
                NodeKind::FunctionDefinition
                    | NodeKind::MethodDefinition
                    | NodeKind::TestDefinition
                    | NodeKind::LambdaExpression
            )
        ) {
            return None;
        }
        Some(FiberOwnership { callable: key, diagnostics: spawn_handle_diagnostics(db, program, index, key).into() })
    })
}

/// A nominal Fiber annotation is evidence; a pointer-shaped ABI is not.
fn fiber_type_declaration(db: &dyn Db, key: AstNodeKey, ty: &beskid_analysis::syntax::Type) -> Option<AstNodeKey> {
    let beskid_analysis::syntax::Type::Complex(path) = ty else {
        return None;
    };
    let actual = resolve_type_declaration(db, key, &path.node)?;
    let fiber = layouts::unique_assembled_type_in_module(db, key, &["Concurrency".into(), "Fiber".into()], "Fiber", 1)
        .or_else(|| unique_type_in_unit(db, key.unit, key.generation, "Fiber", 1))?;
    (actual == fiber).then_some(actual)
}

/// Seed ownership from declared parameters/locals, spawn, or resolved callable result identity.
/// Ordinary source identity preserves method receiver and generic substitutions, without
/// deriving nominal ownership from a pointer-shaped ABI or querying ownership legality.
fn fiber_owner_seed(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    use beskid_analysis::syntax::{LetStatement, Parameter};
    let node = index.node_at(program, key.node)?;
    if let Some(parameter) = node.of::<Parameter>() {
        fiber_type_declaration(db, key, &parameter.ty.node)?;
        return index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(&parameter.name),
        );
    }
    let binding = node.of::<LetStatement>()?;
    let initializer =
        index.direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&binding.value))?;
    let initializer = normalized_expression_node(index, initializer);
    let is_fiber = if index.kind(initializer) == Some(beskid_analysis::syntax_query::NodeKind::SpawnExpression) {
        true
    } else if let Some(annotation) = &binding.type_annotation {
        fiber_type_declaration(db, key, &annotation.node).is_some()
    } else if index.kind(initializer) == Some(beskid_analysis::syntax_query::NodeKind::CallExpression) {
        let GenericSourceTypeIdentity::Nominal { qualified_name, arguments } =
            generic_source_expression_identity(db, AstNodeKey { node: initializer, ..key }).ok()?
        else {
            return None;
        };
        let fiber =
            layouts::unique_assembled_type_in_module(db, key, &["Concurrency".into(), "Fiber".into()], "Fiber", 1)
                .or_else(|| unique_type_in_unit(db, key.unit, key.generation, "Fiber", 1))?;
        stable_declaration_identity(db, fiber)? == qualified_name && arguments.len() == 1
    } else {
        false
    };
    is_fiber
        .then(|| {
            index.direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&binding.name))
        })
        .flatten()
}

/// Handle ownership belongs to one generation-bound callable fact, projected into spawn legality.
/// Resolve terminal uses by declaration identity, so a shadowing local never consumes its parent.
fn spawn_handle_diagnostics(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Vec<SpawnDiagnostic> {
    use beskid_analysis::{
        syntax::{LetStatement, PathExpression},
        syntax_query::NodeKind,
    };
    type BranchPath = Vec<(beskid_analysis::syntax::AstNodeId, beskid_analysis::syntax::AstNodeId)>;
    let mut owners = HashMap::<_, Vec<BranchPath>>::new();
    for kind in [NodeKind::Parameter, NodeKind::LetStatement] {
        for node in index.ids_of_kind(kind).filter(|node| containing_fiber_callable(index, *node) == Some(key.node)) {
            if let Some(owner) = fiber_owner_seed(db, program, index, AstNodeKey { node, ..key }) {
                owners.insert(owner, Vec::new());
            }
        }
    }
    let mut diagnostics = Vec::new();
    for spawn in index
        .ids_of_kind(NodeKind::SpawnExpression)
        .filter(|node| containing_fiber_callable(index, *node) == Some(key.node))
    {
        let mut owner = spawn;
        while let Some(parent) = parent_node(index, owner) {
            match index.kind(parent) {
                Some(NodeKind::Expression | NodeKind::GroupedExpression) => owner = parent,
                Some(NodeKind::ExpressionStatement) => {
                    if let Some(span) = index.node_at(program, spawn).and_then(|node| node.span()) {
                        diagnostics.push(SpawnDiagnostic {
                            kind: SpawnDiagnosticKind::DiscardedHandle,
                            span,
                            capture: None,
                        });
                    }
                    break;
                }
                _ => break,
            }
        }
    }
    let mut uses = index
        .ids_of_kind(NodeKind::PathExpression)
        .filter_map(|use_id| {
            if !is_ancestor(index, key.node, use_id) {
                return None;
            }
            let node = index.node_at(program, use_id)?;
            let path = node.of::<PathExpression>()?;
            let [receiver, rest @ ..] = path.path.node.segments.as_slice() else {
                return None;
            };
            let local = resolve_lexical_declaration(program, index, use_id, &receiver.node.name.node.name)?;
            let method = rest.first().map(|segment| segment.node.name.node.name.as_str());
            Some((node.span()?, use_id, local, method))
        })
        .collect::<Vec<_>>();
    uses.sort_by_key(|(span, ..)| span.start);
    for (span, use_id, local, method) in uses {
        let Some(consumed) = owners.get_mut(&local) else {
            continue;
        };
        let mut branches = BranchPath::new();
        let mut repeated_move = false;
        let mut child = use_id;
        while let Some(parent) = parent_node(index, child) {
            if index.kind(parent) == Some(NodeKind::LambdaExpression) && !is_ancestor(index, parent, local) {
                // Ordinary closures are repeatable; they cannot own a consuming
                // Fiber capture. A lambda directly transferred to spawn has one
                // invocation and is checked as that one-shot ownership transfer.
                let mut container = parent;
                while let Some(wrapper) = parent_node(index, container) {
                    if matches!(index.kind(wrapper), Some(NodeKind::Expression | NodeKind::GroupedExpression)) {
                        container = wrapper;
                    } else {
                        repeated_move |= index.kind(wrapper) != Some(NodeKind::SpawnExpression);
                        break;
                    }
                }
            }
            if index.kind(parent) == Some(NodeKind::IfStatement)
                && matches!(index.kind(child), Some(NodeKind::Block | NodeKind::ElseBranch))
            {
                branches.push((parent, child));
            }
            if matches!(index.kind(parent), Some(NodeKind::WhileStatement | NodeKind::ForStatement)) {
                let loop_span = index.node_at(program, parent).and_then(|node| node.span());
                let declaration_span = index.node_at(program, local).and_then(|node| node.span());
                if let (Some(loop_span), Some(declaration_span)) = (loop_span, declaration_span) {
                    repeated_move |= declaration_span.start < loop_span.start;
                }
            }
            child = parent;
        }
        let consumed_on_this_path = consumed.iter().any(|prior| {
            !prior.iter().any(|(condition, arm)| {
                branches.iter().any(|(other, other_arm)| condition == other && arm != other_arm)
            })
        });
        if consumed_on_this_path || (repeated_move && method != Some("Cancel")) {
            diagnostics.push(SpawnDiagnostic { kind: SpawnDiagnosticKind::UseAfterMove, span, capture: None });
            continue;
        }
        if method == Some("Cancel") {
            continue;
        }
        consumed.push(branches);
        // A local assignment transfers the join capability to the new declaration.
        // Passing/returning the value consumes this owner without manufacturing a copy.
        if method.is_none() {
            let mut container = use_id;
            while let Some(parent) = parent_node(index, container) {
                match index.kind(parent) {
                    Some(NodeKind::Expression | NodeKind::GroupedExpression) => container = parent,
                    Some(NodeKind::LetStatement) => {
                        if let Some(binding) = index.node_at(program, parent).and_then(|node| node.of::<LetStatement>())
                            && let Some(alias) = index.direct_child_id(
                                program,
                                parent,
                                beskid_analysis::syntax_query::DynNodeRef::from(&binding.name),
                            )
                        {
                            owners.insert(alias, Vec::new());
                        }
                        break;
                    }
                    _ => break,
                }
            }
        }
    }
    diagnostics
}

#[salsa::tracked(persist)]
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

#[salsa::tracked(persist)]
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

#[salsa::tracked(persist)]
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
