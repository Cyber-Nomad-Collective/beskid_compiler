//! Managed-reference classification for nodes, aggregate fields, syntax types, and call results.

use super::super::*;
use super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn managed_reference_kind_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ManagedReferenceKind> {
    with_node(db, syntax, key, |program, index, node| {
        let normalized = normalized_expression_node(index, key.node);
        if normalized != key.node {
            return Some(
                managed_reference_kind(db, AstNodeKey { node: normalized, ..key })
                    .and_then(|kind| kind.ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))),
            );
        }
        if node.of::<beskid_analysis::syntax::TryExpression>().is_some() {
            return Some(try_expression_fact(db, key).and_then(|fact| {
                fact.map(|fact| fact.payload_identity.managed_reference_kind())
                    .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))
            }));
        }
        if let Some(projection) = layouts::nominal_field_projection(db, key) {
            return Some(projection.map(|(_, identity)| identity.managed_reference_kind()));
        }
        if let Some(syntax_type) = node.of::<beskid_analysis::syntax::Type>() {
            return Some(managed_reference_kind_for_syntax_type(syntax_type));
        }
        if let Some(parameter) = node.of::<beskid_analysis::syntax::Parameter>() {
            if type_syntax_is_enclosing_generic_parameter_reference(db, key, &parameter.ty.node) {
                return Some(Err(SemanticError::unavailable("managed_reference_kind")));
            }
            return Some(managed_reference_kind_for_syntax_type(&parameter.ty.node));
        }
        if let Some(statement) = node.of::<beskid_analysis::syntax::LetStatement>() {
            if statement.type_annotation.is_none() {
                return Some(
                    index
                        .direct_child_id(
                            program,
                            key.node,
                            beskid_analysis::syntax_query::DynNodeRef::from(&statement.value),
                        )
                        .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))
                        .and_then(|node| managed_reference_kind(db, AstNodeKey { node, ..key }))
                        .and_then(|kind| kind.ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))),
                );
            }
            return Some(
                statement
                    .type_annotation
                    .as_ref()
                    .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))
                    .and_then(|annotation| {
                        if type_syntax_is_enclosing_generic_parameter_reference(db, key, &annotation.node) {
                            Err(SemanticError::unavailable("managed_reference_kind"))
                        } else {
                            managed_reference_kind_for_syntax_type(&annotation.node)
                        }
                    }),
            );
        }
        if node.of::<beskid_analysis::syntax::Identifier>().is_some()
            && let Some(parent) = parent_node(index, key.node)
        {
            if let Some(parameter) =
                index.node_at(program, parent).and_then(|parent| parent.of::<beskid_analysis::syntax::Parameter>())
            {
                if type_syntax_is_enclosing_generic_parameter_reference(db, key, &parameter.ty.node) {
                    return Some(Err(SemanticError::unavailable("managed_reference_kind")));
                }
                return Some(managed_reference_kind_for_syntax_type(&parameter.ty.node));
            }
            if index
                .node_at(program, parent)
                .and_then(|parent| parent.of::<beskid_analysis::syntax::LetStatement>())
                .is_some()
            {
                return Some(
                    managed_reference_kind(db, AstNodeKey { node: parent, ..key })
                        .and_then(|kind| kind.ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))),
                );
            }
        }
        if node.of::<beskid_analysis::syntax::ArrayLiteralExpression>().is_some()
            || node.of::<beskid_analysis::syntax::StructLiteralExpression>().is_some()
            || node.of::<beskid_analysis::syntax::EnumConstructorExpression>().is_some()
            || node.of::<beskid_analysis::syntax::LambdaExpression>().is_some()
        {
            return Some(Ok(ManagedReferenceKind::GcManaged));
        }
        // Module constants are source-restricted to integer literals. Resolve through the same
        // current-unit/import-closure authority as lowering rather than treating their pointer-
        // shaped path syntax as an ambiguous managed reference.
        if node.of::<beskid_analysis::syntax::PathExpression>().is_some()
            && matches!(constant_integer(db, key), Ok(Some(_)))
        {
            return Some(Ok(ManagedReferenceKind::NativeOrScalar));
        }
        if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>()
            && let [segment] = path.path.node.segments.as_slice()
            && segment.node.type_args.is_empty()
            && matches!(constant_integer(db, key), Ok(Some(_)))
        {
            return Some(Ok(ManagedReferenceKind::NativeOrScalar));
        }
        if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>()
            && let [segment] = path.path.node.segments.as_slice()
            && segment.node.type_args.is_empty()
            && let Some(declaration) =
                resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())
            && let Some(binding) = pattern_binding_fact(db, index, key, declaration)
        {
            return Some(binding.map(|binding| binding.managed_reference));
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
            && let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, key)
            && let Some(kind) = managed_reference_kind_for_callable_result(db, key, declaration, None)
        {
            return Some(kind);
        }
        if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
            && let Ok(Some(CallLowering::CorelibService(service))) = call_lowering(db, key)
        {
            return Some(match corelib_service_abi_signature(service).map(|signature| signature.result) {
                Some(SemanticTypeId::STRING) => Ok(ManagedReferenceKind::GcManaged),
                Some(SemanticTypeId::POINTER) | None => Err(SemanticError::unavailable("managed_reference_kind")),
                Some(_) => Ok(ManagedReferenceKind::NativeOrScalar),
            });
        }
        if node.of::<beskid_analysis::syntax::PathExpression>().is_some()
            && let Ok(Some(access)) = aggregate_field_access(db, key)
        {
            return Some(managed_reference_kind_for_aggregate_field(db, &access));
        }
        // Source identity must win over the pointer-shaped ABI fallback. In particular, an
        // explicitly declared `pointer` local is a native pointer, while arrays, functions, and
        // nominal values deliberately share the same target ABI representation.
        if let Ok(identity) = generic_source_expression_identity(db, key) {
            return Some(Ok(identity.managed_reference_kind()));
        }
        if let Ok(Some(semantic)) = node_type(db, key) {
            return Some(match semantic {
                SemanticTypeId::STRING => Ok(ManagedReferenceKind::GcManaged),
                SemanticTypeId::POINTER => Err(SemanticError::unavailable("managed_reference_kind")),
                _ => Ok(ManagedReferenceKind::NativeOrScalar),
            });
        }
        Some(Err(SemanticError::unavailable("managed_reference_kind")))
    })?
    .transpose()
}

fn managed_reference_kind_for_aggregate_field(
    db: &dyn Db,
    access: &AggregateFieldAccess,
) -> Result<ManagedReferenceKind, SemanticError> {
    let syntax = db
        .syntax_unit(access.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, access.declaration))
        .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), access.declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))?;
    let field = definition
        .fields
        .get(usize::try_from(access.index).map_err(|_| SemanticError::unavailable("managed_reference_kind"))?)
        .ok_or_else(|| SemanticError::unavailable("managed_reference_kind"))?;
    if generic_parameter_reference_name(&field.node.ty.node).is_some() {
        return match access.layout.fields.get(usize::try_from(access.index).unwrap_or(usize::MAX)) {
            Some((_, AggregateFieldShape::Nominal(_)))
            | Some((_, AggregateFieldShape::Scalar(SemanticTypeId::STRING))) => Ok(ManagedReferenceKind::GcManaged),
            Some((_, AggregateFieldShape::Scalar(SemanticTypeId::POINTER))) => {
                Err(SemanticError::unavailable("managed_reference_kind"))
            }
            Some((_, AggregateFieldShape::Scalar(_))) => Ok(ManagedReferenceKind::NativeOrScalar),
            None => Err(SemanticError::unavailable("managed_reference_kind")),
        };
    }
    managed_reference_kind_for_syntax_type(&field.node.ty.node)
}

pub(in crate::semantic_contract) fn managed_reference_kind_for_syntax_type(
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<ManagedReferenceKind, SemanticError> {
    Ok(match syntax_type {
        beskid_analysis::syntax::Type::Primitive(primitive)
            if primitive.node == beskid_analysis::syntax::PrimitiveType::String =>
        {
            ManagedReferenceKind::GcManaged
        }
        beskid_analysis::syntax::Type::Primitive(_) => ManagedReferenceKind::NativeOrScalar,
        beskid_analysis::syntax::Type::Array(_) | beskid_analysis::syntax::Type::Function { .. } => {
            ManagedReferenceKind::GcManaged
        }
        beskid_analysis::syntax::Type::Complex(_) => ManagedReferenceKind::GcManaged,
        // `This` stands for whatever concrete type conforms; treated the same as an
        // unresolved-generic `Complex` reference (conservative GC-managed default).
        beskid_analysis::syntax::Type::This => ManagedReferenceKind::GcManaged,
        beskid_analysis::syntax::Type::Associated { .. } => {
            return Err(SemanticError::unavailable("managed_reference_kind"));
        }
    })
}

/// Preserve source result ownership through the already selected generic call environment.
pub fn specialized_call_result_managed_reference_kind(
    db: &dyn Db,
    use_key: AstNodeKey,
    instance: &GenericSpecializationInstance,
) -> SemanticQueryResult<ManagedReferenceKind> {
    if !db.syntax_unit(use_key.unit).is_some_and(|syntax| syntax.accepts_key(db, use_key)) {
        return Ok(None);
    }
    managed_reference_kind_for_callable_result(db, use_key, instance.declaration, Some(instance)).transpose()
}

fn managed_reference_kind_for_callable_result(
    db: &dyn Db,
    use_key: AstNodeKey,
    declaration: AstNodeKey,
    instance: Option<&GenericSpecializationInstance>,
) -> Option<Result<ManagedReferenceKind, SemanticError>> {
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?;
    let return_type = node
        .of::<beskid_analysis::syntax::FunctionDefinition>()
        .and_then(|function| function.return_type.as_ref())
        .or_else(|| {
            node.of::<beskid_analysis::syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref())
        })
        .or_else(|| {
            node.of::<beskid_analysis::syntax::ContractMethodSignature>().and_then(|method| method.return_type.as_ref())
        });
    let Some(return_type) = return_type else {
        return Some(Ok(ManagedReferenceKind::NativeOrScalar));
    };
    Some(match &return_type.node {
        beskid_analysis::syntax::Type::Primitive(_)
        | beskid_analysis::syntax::Type::Array(_)
        | beskid_analysis::syntax::Type::Function { .. } => managed_reference_kind_for_syntax_type(&return_type.node),
        beskid_analysis::syntax::Type::Complex(_) => {
            let selected = instance.cloned().or_else(|| generic_specialization_instance_for_call(db, use_key).ok());
            if let Some(parameter) = generic_parameter_reference_name(&return_type.node)
                && let Some(instance) = selected
                && let Some(binding) =
                    instance.substitutions.iter().find(|binding| binding.parameter.as_ref() == parameter)
            {
                Ok(binding.source_identity().managed_reference_kind())
            } else {
                Ok(ManagedReferenceKind::GcManaged)
            }
        }
        beskid_analysis::syntax::Type::Associated { .. } => Err(SemanticError::unavailable("managed_reference_kind")),
        beskid_analysis::syntax::Type::This => Ok(ManagedReferenceKind::GcManaged),
    })
}
