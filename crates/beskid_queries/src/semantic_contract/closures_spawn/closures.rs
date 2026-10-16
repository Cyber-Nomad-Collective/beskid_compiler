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
        // A mutable binding can be reassigned to another function value, so its calls go
        // through the closure record it holds at the call site, never its initializer.
        if binding.mutable || !expression_is_lambda(&binding.value.node) {
            return None;
        }
        let lambda = index
            .direct_child_id(
                program,
                parent_node(index, declaration)?,
                beskid_analysis::syntax_query::DynNodeRef::from(&binding.value),
            )
            .map(|node| normalized_expression_node(index, node))?;
        // A block body's `return` leaves the lambda, so it is called as a function, not inlined.
        (!lambda_has_statement_body(program, index, lambda)).then_some(AstNodeKey { node: lambda, ..key })
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
    // A projection from a local (`config.width`, `knobs.depth.inner`) captures the local that
    // roots it, exactly as the field projection resolves its receiver; a root that is a module
    // or type name resolves no lexical declaration and is not a capture. A call callee
    // (`child.Join()`) is captured only through its resolved member receiver below.
    if !path_is_call_callee(program, index, key.node)
        && let Some(root) = path.segments.first()
        && root.node.type_args.is_empty()
        && let Some(declaration) = resolve_lexical_declaration(program, index, key.node, &root.node.name.node.name)
    {
        return Some(AstNodeKey { node: declaration, ..key });
    }
    nominal_local_member_receiver(db, program, index, key, path).map(|(_, receiver)| receiver).or_else(|| {
        // A resolved contract member receiver is captured like a nominal one: a contract-bounded
        // generic parameter (`combiner.Combine(acc, value)` with `where C: Associative<T>`) or a
        // contract-typed one. An unresolved member still captures nothing.
        crate::semantic_contract::contracts::contract_member_receiver(db, program, index, key, path)?
            .ok()
            .map(|(_, receiver)| receiver)
    })
}

/// Whether the path expression `path` is the callee of the call expression above it.
fn path_is_call_callee(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    path: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let Some(mut parent) = parent_node(index, path) else { return false };
    while matches!(
        index.kind(parent),
        Some(
            beskid_analysis::syntax_query::NodeKind::Expression
                | beskid_analysis::syntax_query::NodeKind::GroupedExpression
        )
    ) {
        let Some(next) = parent_node(index, parent) else { return false };
        parent = next;
    }
    index
        .node_at(program, parent)
        .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
        .and_then(|call| {
            index.direct_child_id(program, parent, beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()))
        })
        .is_some_and(|callee| normalized_expression_node(index, callee) == path)
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
    let semantic_type = match abi_type(db, declaration) {
        Ok(Some(semantic_type)) => semantic_type,
        // An immutable value of a generic body whose type names the body's type parameters has
        // no context-free ABI. It is captured by value; each specialization re-checks the
        // instance type (`specialized_closure_signature` rejects a native pointer).
        result @ (Ok(None) | Err(_)) => {
            let generic_value = !mutable
                && result.as_ref().err().is_none_or(SemanticError::is_unavailable)
                && declared_in_generic_callable(db, program, index, declaration);
            if generic_value {
                return Ok(CaptureStorageClass::TransferableValue);
            }
            return Err(result.err().unwrap_or_else(|| SemanticError::unavailable("capture_storage")));
        }
    };
    // ABI pointers alone carry no ownership provenance. The existing source-type
    // query distinguishes native pointers from managed arrays and nominal values.
    let managed = managed_reference_kind(db, declaration).ok().flatten() == Some(ManagedReferenceKind::GcManaged);
    Ok(if mutable || (semantic_type == SemanticTypeId::POINTER && !managed) {
        CaptureStorageClass::StackReference
    } else {
        CaptureStorageClass::TransferableValue
    })
}

/// Whether `declaration` belongs to a callable with type parameters of its own or of its owner.
fn declared_in_generic_callable(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
) -> bool {
    let Some(callable) = nearest_ancestor(index, declaration.node, |kind| {
        matches!(
            kind,
            beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                | beskid_analysis::syntax_query::NodeKind::MethodDefinition
        )
    }) else {
        return false;
    };
    let Some(node) = index.node_at(program, callable) else { return false };
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return !function.generics.is_empty();
    }
    let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() else { return false };
    if !method.generics.is_empty() {
        return true;
    }
    crate::semantic_contract::locals::method_owner_declaration(db, AstNodeKey { node: callable, ..declaration })
        .and_then(|owner| {
            let syntax = db.syntax_unit(owner.unit).filter(|syntax| syntax.accepts_key(db, owner))?;
            syntax
                .syntax_index(db)
                .node_at(syntax.expanded_program(db), owner.node)?
                .of::<beskid_analysis::syntax::TypeDefinition>()
                .map(|definition| !definition.generics.is_empty())
        })
        .unwrap_or(false)
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
    if node.of::<beskid_analysis::syntax::LambdaExpression>().is_some() {
        // The lambda's function type is its source identity: typed parameters and an expression
        // body synthesize, untyped parameters and a block body check against the expected type.
        return Some(lambda_source_identity(db, program, index, key).and_then(|identity| match identity {
            GenericSourceTypeIdentity::Function { parameters, result } => Ok(ItemSignature {
                parameters: parameters.iter().map(GenericSourceTypeIdentity::abi_type).collect(),
                result: result.abi_type(),
            }),
            GenericSourceTypeIdentity::Abi(_)
            | GenericSourceTypeIdentity::Nominal { .. }
            | GenericSourceTypeIdentity::Array(_) => Err(SemanticError::unavailable("callable_signature")),
        }));
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
