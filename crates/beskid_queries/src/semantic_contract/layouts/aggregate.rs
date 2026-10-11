//! Canonical semantic layout implementation.

use super::{super::*, explicit_local_declaration_type};

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn aggregate_layout_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateLayoutFact> {
    if crate::runtime_managed_opaque_kind(db, key)
        .is_some_and(|kind| kind != crate::RuntimeManagedOpaqueKind::SerializationExtras)
    {
        return Err(SemanticError::unavailable("aggregate_layout.sealed_runtime_opaque"));
    }
    with_node(db, syntax, key, |program, index, node| {
        let definition = node.of::<beskid_analysis::syntax::TypeDefinition>()?;
        Some(aggregate_layout_from_definition(db, program, index, key, definition, None))
    })?
    .transpose()
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn event_field_layout_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EventFieldLayoutFact> {
    with_node(db, syntax, key, |program, index, node| {
        let field = node.of::<beskid_analysis::syntax::Field>()?;
        if field.kind != beskid_analysis::syntax::FieldKind::Event {
            return None;
        }
        let owner = find_event_owner(index, key.node)?;
        let owner_key = AstNodeKey { node: owner, ..key };
        let definition = index.node_at(program, owner)?.of::<beskid_analysis::syntax::TypeDefinition>()?;
        let field_ids = index.children(owner)?;
        let matching_field = field_ids.iter().copied().find(|id| {
            index
                .node_at(program, *id)
                .and_then(|node| node.of::<beskid_analysis::syntax::Field>())
                .is_some_and(|candidate| candidate.name.node.name == field.name.node.name)
        })?;

        // ABI-v5's supported targets all use 64-bit pointers and the managed-object
        // header is two words. Keep value-field offsets identical to aggregate_layout,
        // then append pointer-sized event slots in declaration order.
        let mut size = 16_u64;
        let mut alignment = 8_u64;
        for source_field in
            definition.fields.iter().filter(|field| field.node.kind == beskid_analysis::syntax::FieldKind::Value)
        {
            let (_, shape) = aggregate_field_layout(db, program, index, owner_key, source_field).ok()?;
            let abi_type = match shape {
                AggregateFieldShape::Scalar(abi_type) | AggregateFieldShape::ManagedReference(abi_type) => abi_type,
                AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
            };
            let layout = abi_type.scalar_abi_layout(64)?;
            size = align_layout(size, layout.alignment)?;
            size = size.checked_add(layout.size)?;
            alignment = alignment.max(layout.alignment);
        }
        size = align_layout(size, alignment)?;
        let injected_count = definition
            .fields
            .iter()
            .filter(|field| field.node.kind == beskid_analysis::syntax::FieldKind::Injected)
            .count();
        size = size.checked_add(u64::try_from(injected_count).ok()?.checked_mul(8)?)?;
        let event_index = field_ids
            .iter()
            .copied()
            .take_while(|id| *id != matching_field)
            .filter(|id| {
                index
                    .node_at(program, *id)
                    .and_then(|node| node.of::<beskid_analysis::syntax::Field>())
                    .is_some_and(|candidate| candidate.kind == beskid_analysis::syntax::FieldKind::Event)
            })
            .count();
        let slot_offset = size.checked_add(u64::try_from(event_index).ok()?.checked_mul(8)?)?;
        Some(Ok(EventFieldLayoutFact {
            owner_type: owner_key,
            field: AstNodeKey { node: matching_field, ..key },
            slot_offset: u32::try_from(slot_offset).ok()?,
            capacity: u32::try_from(field.event_capacity.unwrap_or_default()).ok()?,
        }))
    })?
    .transpose()
}

fn find_event_owner(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    mut node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    while let Some(parent) = parent_node(index, node) {
        if index.kind(parent)? == beskid_analysis::syntax_query::NodeKind::TypeDefinition {
            return Some(parent);
        }
        node = parent;
    }
    None
}

fn align_layout(value: u64, alignment: u64) -> Option<u64> {
    value.checked_add(alignment.checked_sub(1)?).map(|value| value & !(alignment - 1))
}

pub(in crate::semantic_contract) fn aggregate_layout_from_definition(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
    definition: &beskid_analysis::syntax::TypeDefinition,
    substitutions: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<AggregateLayoutFact, SemanticError> {
    if definition.generics.is_empty() && substitutions.is_some() {
        return Err(SemanticError::unavailable("aggregate_layout"));
    }
    definition
        .fields
        .iter()
        // Events are dispatch metadata, not ABI-v5 aggregate storage. Keeping them
        // out of this value-field layout preserves the exact physical indices used by
        // struct literals and direct value projections; a projection of an event itself
        // still has no aggregate-field fact and therefore fails closed.
        .filter(|field| field.node.kind == beskid_analysis::syntax::FieldKind::Value)
        .map(|field| {
            let substituted = substitutions.and_then(|substitutions| {
                generic_parameter_reference_name(&field.node.ty.node)
                    .and_then(|parameter| substitutions.get(parameter))
                    .copied()
            });
            substituted
                .map(|shape| (Arc::from(field.node.name.node.name.as_str()), shape))
                .map(Ok)
                .unwrap_or_else(|| aggregate_field_layout(db, program, index, declaration, field))
        })
        .collect::<Result<Vec<_>, SemanticError>>()
        .map(|fields| AggregateLayoutFact { fields: fields.into() })
}

pub(in crate::semantic_contract) fn instantiated_aggregate_layout_for_path(
    db: &dyn Db,
    use_key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(AstNodeKey, AggregateLayoutFact), SemanticError> {
    let declaration =
        resolve_type_declaration(db, use_key, path).ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?;
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let definition = index
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?;
    if definition.generics.is_empty() {
        return aggregate_layout_from_definition(db, program, index, declaration, definition, None)
            .map(|layout| (declaration, layout));
    }
    let terminal = path.segments.last().ok_or_else(|| SemanticError::unavailable("aggregate_layout"))?;
    if path.segments[..path.segments.len() - 1].iter().any(|segment| !segment.node.type_args.is_empty())
        || terminal.node.type_args.len() != definition.generics.len()
    {
        return Err(SemanticError::unavailable("aggregate_layout"));
    }
    // Materialize only substitutions that can change this declaration's physical field layout.
    // A source-proven enclosing generic remains part of the nominal application identity even
    // when the declaration is phantom over it, but it must not make an otherwise concrete layout
    // unavailable. Generic parameters used directly as fields still require an exact shape.
    let layout_parameters = definition
        .fields
        .iter()
        .filter_map(|field| generic_parameter_reference_name(&field.node.ty.node))
        .collect::<HashSet<_>>();
    let substitutions = definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .filter_map(|(generic, argument)| {
            let parameter = generic.node.name.as_str();
            match applied_aggregate_shape(db, use_key, &argument.node, ambient) {
                Ok(shape) => Some(Ok((generic.node.name.clone(), shape))),
                Err(_)
                    if !layout_parameters.contains(parameter)
                        && type_syntax_is_enclosing_generic_parameter_reference(db, use_key, &argument.node) =>
                {
                    None
                }
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    aggregate_layout_from_definition(db, program, index, declaration, definition, Some(&substitutions))
        .map(|layout| (declaration, layout))
}

pub(in crate::semantic_contract) fn applied_aggregate_shape(
    db: &dyn Db,
    use_key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<AggregateFieldShape, SemanticError> {
    if let Some(parameter) = generic_parameter_reference_name(syntax_type)
        && let Some(shape) = ambient.and_then(|substitutions| substitutions.get(parameter))
    {
        return Ok(*shape);
    }
    aggregate_shape_from_applied_type(db, use_key, syntax_type)
        .map_err(|_| SemanticError::unavailable("aggregate_layout"))
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn aggregate_literal_layout_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateLayoutFact> {
    with_node(db, syntax, key, |_, _, node| {
        let literal = node.of::<beskid_analysis::syntax::StructLiteralExpression>()?;
        Some(instantiated_aggregate_layout_for_path(db, key, &literal.path.node, None).and_then(
            |(declaration, layout)| {
                validate_aggregate_literal_visibility(db, key, declaration)?;
                Ok(layout)
            },
        ))
    })?
    .transpose()
}

pub fn aggregate_literal_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<AggregateLayoutFact> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    with_node(db, syntax, key, |_, _, node| {
        let literal = node.of::<beskid_analysis::syntax::StructLiteralExpression>()?;
        let ambient = enclosing
            .iter()
            .map(|binding| {
                crate::semantic_contract::layouts::aggregate_shape_for_binding(db, key, binding)
                    .map(|shape| (binding.parameter.to_string(), shape))
            })
            .collect::<Result<HashMap<_, _>, SemanticError>>();
        let ambient = match ambient {
            Ok(ambient) => ambient,
            Err(error) => return Some(Err(error)),
        };
        Some(instantiated_aggregate_layout_for_path(db, key, &literal.path.node, Some(&ambient)).and_then(
            |(declaration, layout)| {
                validate_aggregate_literal_visibility(db, key, declaration)?;
                Ok(layout)
            },
        ))
    })?
    .transpose()
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn aggregate_literal_declaration_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_node(db, syntax, key, |program, index, node| {
        let literal = node.of::<beskid_analysis::syntax::StructLiteralExpression>()?;
        let declaration = resolve_nominal_layout_declaration(db, program, index, key, &literal.path.node)?;
        Some(validate_aggregate_literal_visibility(db, key, declaration).map(|()| declaration))
    })?
    .transpose()
}

/// A struct literal is legal only where every value field it must supply is accessible: a
/// field without `pub` is private to the source unit that declares the type, so a type with any
/// private value field is constructible only inside its declaring unit (or by an exact canonical
/// admission). Sealed runtime opaques and canonical process resources are never literal-built
/// outside their own authority.
fn validate_aggregate_literal_visibility(
    db: &dyn Db,
    key: AstNodeKey,
    declaration: AstNodeKey,
) -> Result<(), SemanticError> {
    if let Some(kind) = crate::runtime_managed_opaque_kind(db, declaration) {
        if kind != crate::RuntimeManagedOpaqueKind::SerializationExtras
            || !sealed_extras_factory_site(db, key, declaration)
        {
            return Err(SemanticError::unavailable("aggregate_literal.sealed_runtime_opaque"));
        }
    }
    if !inaccessible_literal_fields(db, key, declaration)?.is_empty() {
        return Err(SemanticError::unavailable("aggregate_literal.visibility"));
    }
    Ok(())
}

/// The value fields of `declaration` that a struct literal at `key` cannot supply, in declaration
/// order. Legality uses this same authority to issue the coded E1211 diagnostic first; lowering
/// fails closed on any non-empty answer.
pub(in crate::semantic_contract) fn inaccessible_literal_fields(
    db: &dyn Db,
    key: AstNodeKey,
    declaration: AstNodeKey,
) -> Result<Vec<Arc<str>>, SemanticError> {
    if crate::contribution_attribution::contribution_attributed_unit(db, key) == declaration.unit {
        return Ok(Vec::new());
    }
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("aggregate_literal.visibility"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("aggregate_literal.visibility"))?;
    if crate::process_source_authority::canonical_process_resource_kind(db, declaration).is_some() {
        return Ok(vec![Arc::from("token")]);
    }
    Ok(definition
        .fields
        .iter()
        .filter(|field| field.node.kind == beskid_analysis::syntax::FieldKind::Value)
        .filter(|field| {
            super::field_access::field_inaccessible(
                db,
                key,
                declaration,
                definition,
                field,
                super::field_access::PrivateFieldUse::Construct,
            )
        })
        .map(|field| Arc::from(field.node.name.node.name.as_str()))
        .collect())
}

/// Derive the element ABI of an empty array literal from its [`expected_type`].
///
/// An empty literal carries no element expression from which to infer a representation, so the
/// expected array type of its position (a declared `T[]` local, callee parameter, enum payload,
/// struct field, return type, outer array element or match arm) is the sole authority. An
/// assignment is a store into existing storage, not an initialization context, and grants no
/// allocation plan. A position with no expected type, or whose expected type is not an array,
/// stays unavailable; a non-empty literal has no fact here.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn empty_array_literal_element_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |_program, _index, node| {
        let array = node.of::<beskid_analysis::syntax::ArrayLiteralExpression>()?;
        if !array.elements.is_empty() {
            return None;
        }
        Some((|| {
            let unavailable = || SemanticError::unavailable("empty_array_literal_element_abi_type");
            let expected = expected_type(db, key)?.ok_or_else(unavailable)?;
            if matches!(expected.context, ExpectedTypeContext::Assignment { .. }) {
                return Err(unavailable());
            }
            expected.array_element_abi_type().ok_or_else(unavailable)
        })())
    })?
    .transpose()
}

/// Return the element ABI for an indexed, explicitly declared array or proven nominal field.
///
/// Array literals own allocation metadata, but an index operation may address an array supplied
/// by a parameter or constructed by a runtime intrinsic. In that case the declaration's `T[]`
/// syntax or the nominal field's exact substituted source identity owns the element representation.
/// Unproven, string, and stale targets deliberately remain unavailable.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn array_index_element_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    Ok(match array_index_element_template_tracked(db, syntax, key)? {
        Some(ArrayIndexElementTemplate::Concrete(element)) => Some(element),
        Some(ArrayIndexElementTemplate::EnclosingParameter(_)) => {
            return Err(SemanticError::unavailable("array_index_element_abi_type"));
        }
        None => None,
    })
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn array_index_element_template_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ArrayIndexElementTemplate> {
    with_node(db, syntax, key, |program, index, node| {
        let (index_node, indexed) = if let Some(indexed) = node.of::<beskid_analysis::syntax::IndexExpression>() {
            (key.node, indexed)
        } else {
            let assignment = node.of::<beskid_analysis::syntax::AssignExpression>()?;
            if assignment.op.node != beskid_analysis::syntax::AssignOp::Assign {
                return Some(Err(SemanticError::unavailable("array_index_element_abi_type")));
            }
            let target = index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(assignment.target.as_ref()),
                )
                .map(|target| normalized_expression_node(index, target))?;
            (target, index.node_at(program, target)?.of::<beskid_analysis::syntax::IndexExpression>()?)
        };
        let target = index
            .direct_child_id(
                program,
                index_node,
                beskid_analysis::syntax_query::DynNodeRef::from(indexed.target.as_ref()),
            )
            .map(|target| normalized_expression_node(index, target))?;
        let target_node = index.node_at(program, target)?;
        if target_node.of::<beskid_analysis::syntax::MemberExpression>().is_some()
            || target_node
                .of::<beskid_analysis::syntax::PathExpression>()
                .is_some_and(|path| path.path.node.segments.len() > 1)
        {
            // The source query proves the field declaration, substitutions and integer index;
            // an erased array pointer is never sufficient to authorize element storage.
            return Some(
                generic_source_expression_identity(db, AstNodeKey { node: index_node, ..key })
                    .map(|element| ArrayIndexElementTemplate::Concrete(element.abi_type())),
            );
        }
        let target = target_node.of::<beskid_analysis::syntax::PathExpression>()?;
        let [segment] = target.path.node.segments.as_slice() else {
            return Some(Err(SemanticError::unavailable("array_index_element_abi_type")));
        };
        if !segment.node.type_args.is_empty() {
            return Some(Err(SemanticError::unavailable("array_index_element_abi_type")));
        }
        let declaration =
            resolve_lexical_declaration(program, index, index_node, segment.node.name.node.name.as_str())?;
        let parent = parent_node(index, declaration)?;
        if index.kind(parent) == Some(beskid_analysis::syntax_query::NodeKind::Pattern) {
            return Some(
                generic_source_expression_identity(db, AstNodeKey { node: index_node, ..key })
                    .map(|element| ArrayIndexElementTemplate::Concrete(element.abi_type())),
            );
        }
        let array_type = match index.kind(parent)? {
            beskid_analysis::syntax_query::NodeKind::Parameter => index
                .node_at(program, parent)?
                .of::<beskid_analysis::syntax::Parameter>()
                .map(|parameter| &parameter.ty.node),
            beskid_analysis::syntax_query::NodeKind::LetStatement => index
                .node_at(program, parent)?
                .of::<beskid_analysis::syntax::LetStatement>()
                .and_then(|statement| statement.type_annotation.as_ref().map(|annotation| &annotation.node)),
            _ => None,
        }
        .ok_or_else(|| SemanticError::unavailable("array_index_element_abi_type"));
        Some(array_type.and_then(|array_type| {
            let beskid_analysis::syntax::Type::Array(element) = array_type else {
                return Err(SemanticError::unavailable("array_index_element_abi_type"));
            };
            // A single-segment element type is a template parameter only when the enclosing item
            // declares it; `Chunk[]` names a concrete nominal element, not an erased `T[]`.
            if type_syntax_is_enclosing_generic_parameter_reference(
                db,
                AstNodeKey { node: index_node, ..key },
                &element.node,
            ) && let Some(parameter) = generic_parameter_reference_name(&element.node)
            {
                return Ok(ArrayIndexElementTemplate::EnclosingParameter(Arc::from(parameter)));
            }
            abi_type_from_syntax(db, AstNodeKey { node: declaration, ..key }, &element.node)
                .map(ArrayIndexElementTemplate::Concrete)
        }))
    })?
    .transpose()
}

/// Resolve an indexed array element through one exact enclosing generic specialization.
pub fn array_index_element_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<SemanticTypeId> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Ok(None) };
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    let Some(template) = array_index_element_template_tracked(db, syntax, key)? else { return Ok(None) };
    match template {
        ArrayIndexElementTemplate::Concrete(element) => Ok(Some(element)),
        ArrayIndexElementTemplate::EnclosingParameter(parameter) => Ok(enclosing
            .iter()
            .find(|binding| binding.parameter.as_ref() == parameter.as_ref())
            .map(|binding| binding.argument)),
    }
}

/// Classify an indexed element whose declared element type is one of the enclosing item's
/// generic parameters, through the exact enclosing specialization's source binding.
///
/// `values[index]` in `Copy<T>` specialized with `T = Item` has a pointer-shaped element ABI
/// that alone cannot distinguish a traced reference from a native pointer; the binding retains
/// the source identity that does. A concrete declared element keeps its own authority and
/// receives no fact here.
pub fn array_index_element_managed_reference_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<ManagedReferenceKind> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Ok(None) };
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    match array_index_element_template_tracked(db, syntax, key)? {
        Some(ArrayIndexElementTemplate::EnclosingParameter(parameter)) => Ok(enclosing
            .iter()
            .find(|binding| binding.parameter.as_ref() == parameter.as_ref())
            .map(|binding| binding.managed_reference_kind())),
        Some(ArrayIndexElementTemplate::Concrete(_)) | None => Ok(None),
    }
}

/// Physical layout remains source-derived, but only this exact immutable
/// canonical private constructor may allocate the sealed ExtrasBinding.
fn sealed_extras_factory_site(db: &dyn Db, key: AstNodeKey, declaration: AstNodeKey) -> bool {
    if key.unit != declaration.unit
        || key.generation != declaration.generation
        || crate::canonical_corelib_source_path(db, key).as_deref()
            != Some(beskid_abi::runtime_source::CANONICAL_SERIALIZATION_DESCRIPTORS_SOURCE_PATH)
    {
        return false;
    }
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else { return false };
    let Some(expected) = beskid_abi::runtime_source::canonical_corelib_service_sources()
        .into_iter()
        .find(|unit| unit.logical_path == beskid_abi::runtime_source::CANONICAL_SERIALIZATION_DESCRIPTORS_SOURCE_PATH)
    else {
        return false;
    };
    let Ok(parsed) =
        beskid_analysis::services::parse_program_with_source_name("canonical-descriptors", &expected.source)
    else {
        return false;
    };
    if &parsed != syntax.expanded_program(db).as_ref() {
        return false;
    }
    let index = syntax.syntax_index(db);
    let Some(owner) =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
    else {
        return false;
    };
    index
        .node_at(syntax.expanded_program(db), owner)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .is_some_and(|function| {
            function.name.node.name == "AttachCompiledExtras"
                && function.visibility.node == beskid_analysis::syntax::Visibility::Private
        })
}
