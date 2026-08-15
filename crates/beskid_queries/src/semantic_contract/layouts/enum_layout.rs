//! Canonical semantic layout implementation.

#[cfg(test)]
mod scalar_payload_tests;

use super::super::*;

impl EnumLayoutFact {
    /// Compute the sole target-specific physical authority for ABI-v5 scalar-payload enums.
    ///
    /// Pointer and scalar payloads use distinct union slots so the static pointer map traces every
    /// managed payload without scanning scalar bits.
    pub fn scalar_payload_object_layout(
        &self,
        pointer_width: u8,
        header_size: u64,
        header_alignment: u64,
    ) -> Option<EnumScalarPayloadObjectLayout> {
        #[derive(Clone, Copy)]
        struct StorageClass {
            ty: SemanticTypeId,
            size: u64,
            alignment: u64,
        }

        if header_size < 16 || header_alignment == 0 || !header_alignment.is_power_of_two() {
            return None;
        }
        let mut scalar_storage = None::<StorageClass>;
        let mut pointer_storage = None::<StorageClass>;
        let payloads = self
            .variants
            .iter()
            .map(|variant| match variant.fields.as_ref() {
                [] => Some(None),
                [(_, shape)] => {
                    let ty = match shape {
                        AggregateFieldShape::Scalar(ty) => *ty,
                        AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
                    };
                    let layout = ty.scalar_abi_layout(pointer_width)?;
                    let storage = if layout.is_pointer { &mut pointer_storage } else { &mut scalar_storage };
                    if storage.is_none_or(|current| {
                        layout.size > current.size
                            || (layout.size == current.size && layout.alignment > current.alignment)
                    }) {
                        *storage = Some(StorageClass { ty, size: layout.size, alignment: layout.alignment });
                    }
                    Some(Some((ty, layout.is_pointer)))
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;

        let tag_offset = align_to_layout(header_size, 4)?;
        let mut end = tag_offset.checked_add(4)?;
        let mut object_alignment = header_alignment.max(4);
        let mut storage_fields = Vec::with_capacity(2);
        let scalar_offset = append_storage(
            scalar_storage.map(|storage| (storage.ty, storage.size, storage.alignment)),
            &mut end,
            &mut object_alignment,
            &mut storage_fields,
        )?;
        let pointer_offset = append_storage(
            pointer_storage.map(|storage| (storage.ty, storage.size, storage.alignment)),
            &mut end,
            &mut object_alignment,
            &mut storage_fields,
        )?;
        let variants = payloads
            .into_iter()
            .map(|payload| {
                let (payload_type, payload_offset) = payload
                    .map(|(ty, is_pointer)| (Some(ty), if is_pointer { pointer_offset } else { scalar_offset }))
                    .unwrap_or((None, None));
                EnumScalarPayloadVariantLayout { payload_type, payload_offset }
            })
            .collect::<Vec<_>>();
        Some(EnumScalarPayloadObjectLayout {
            object_size: align_to_layout(end, object_alignment)?,
            object_alignment,
            tag_offset,
            storage_fields: storage_fields.into(),
            pointer_map_offsets: pointer_offset.into_iter().collect::<Vec<_>>().into(),
            variants: variants.into(),
        })
    }
}

fn append_storage(
    storage: Option<(SemanticTypeId, u64, u64)>,
    end: &mut u64,
    object_alignment: &mut u64,
    fields: &mut Vec<(SemanticTypeId, u64)>,
) -> Option<Option<u64>> {
    let Some((ty, size, alignment)) = storage else {
        return Some(None);
    };
    *end = align_to_layout(*end, alignment)?;
    let offset = *end;
    *end = end.checked_add(size)?;
    *object_alignment = (*object_alignment).max(alignment);
    fields.push((ty, offset));
    Some(Some(offset))
}

fn align_to_layout(value: u64, alignment: u64) -> Option<u64> {
    (alignment > 0 && alignment.is_power_of_two()).then_some(())?;
    value.checked_add(alignment - 1).map(|value| value & !(alignment - 1))
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn enum_layout_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumLayoutFact> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(definition) = node.of::<beskid_analysis::syntax::EnumDefinition>() {
            return Some(enum_layout_from_definition(db, program, index, key, definition, None));
        }
        node.of::<beskid_analysis::syntax::EnumConstructorExpression>()
            .map(|constructor| {
                let type_path = contextual_enum_constructor_type_path(program, index, key, constructor)
                    .unwrap_or(&constructor.path.node.type_path.node);
                instantiated_enum_layout_for_path(db, key, type_path)
            })
            .or_else(|| {
                node.of::<beskid_analysis::syntax::TryExpression>().map(|_| {
                    // The layout is available only after the full propagation fact has proven the
                    // Result/error contract. Re-read the parameter annotation solely to instantiate
                    // the existing canonical enum-layout machinery for that exact source path.
                    try_expression_fact_for_node(db, program, index, key, node)?;
                    let (_, declaration) = try_operand_parameter_declaration(program, index, key, node)?;
                    let parameter = parent_node(index, declaration)
                        .and_then(|parent| index.node_at(program, parent))
                        .and_then(|parameter| parameter.of::<beskid_analysis::syntax::Parameter>())
                        .ok_or_else(|| SemanticError::unavailable("try_expression"))?;
                    let beskid_analysis::syntax::Type::Complex(path) = &parameter.ty.node else {
                        return Err(SemanticError::unavailable("try_expression"));
                    };
                    instantiated_enum_layout_for_path(db, key, &path.node)
                })
            })
    })?
    .transpose()
}

/// Return an explicitly applied enum type from the immediate typed context of a genericless
/// constructor. This intentionally declines all inferred, nested, and control-flow contexts.
pub(in crate::semantic_contract) fn contextual_enum_constructor_type_path<'a>(
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    constructor: &beskid_analysis::syntax::EnumConstructorExpression,
) -> Option<&'a beskid_analysis::syntax::Path> {
    let constructor_path = &constructor.path.node.type_path.node;
    let terminal = constructor_path.segments.last()?;
    if !terminal.node.type_args.is_empty() {
        return None;
    }
    let constructor_name = terminal.node.name.node.name.as_str();
    let mut current = parent_node(index, key.node)?;
    while matches!(
        index.kind(current)?,
        beskid_analysis::syntax_query::NodeKind::Expression | beskid_analysis::syntax_query::NodeKind::Statement
    ) {
        current = parent_node(index, current)?;
    }

    let expected = match index.kind(current)? {
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, current)?
            .of::<beskid_analysis::syntax::LetStatement>()?
            .type_annotation
            .as_ref()
            .map(|annotation| &annotation.node),
        beskid_analysis::syntax_query::NodeKind::ReturnStatement => {
            let mut item = parent_node(index, current)?;
            while !matches!(
                index.kind(item)?,
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::MethodDefinition
            ) {
                item = parent_node(index, item)?;
            }
            let item = index.node_at(program, item)?;
            item.of::<beskid_analysis::syntax::FunctionDefinition>()
                .and_then(|function| function.return_type.as_ref().map(|annotation| &annotation.node))
                .or_else(|| {
                    item.of::<beskid_analysis::syntax::MethodDefinition>()
                        .and_then(|method| method.return_type.as_ref().map(|annotation| &annotation.node))
                })
        }
        _ => None,
    }?;
    let beskid_analysis::syntax::Type::Complex(path) = expected else {
        return None;
    };
    let expected_path = &path.node;
    let expected_terminal = expected_path.segments.last()?;
    (expected_terminal.node.name.node.name == constructor_name && !expected_terminal.node.type_args.is_empty())
        .then_some(expected_path)
}

pub(in crate::semantic_contract) fn instantiated_enum_layout_for_path(
    db: &dyn Db,
    use_key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Result<EnumLayoutFact, SemanticError> {
    let declaration =
        resolve_type_declaration(db, use_key, path).ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.generation(db) == declaration.generation)
        .ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let definition = index
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    if definition.generics.is_empty() {
        return enum_layout_from_definition(db, program, index, declaration, definition, None);
    }
    let substitutions = enum_layout_substitutions(db, use_key, definition, path)?;
    enum_layout_from_definition(db, program, index, declaration, definition, Some(&substitutions))
}

pub(in crate::semantic_contract) fn enum_layout_substitutions(
    db: &dyn Db,
    use_key: AstNodeKey,
    definition: &beskid_analysis::syntax::EnumDefinition,
    path: &beskid_analysis::syntax::Path,
) -> Result<HashMap<String, AggregateFieldShape>, SemanticError> {
    let (terminal, module_path) =
        path.segments.split_last().ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    if module_path.iter().any(|segment| !segment.node.type_args.is_empty())
        || terminal.node.type_args.len() != definition.generics.len()
        || definition.generics.is_empty()
    {
        return Err(SemanticError::unavailable("enum_layout"));
    }
    definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .map(|(generic, argument)| {
            aggregate_shape_from_applied_type(db, use_key, &argument.node)
                .or_else(|error| {
                    if type_syntax_is_generic_parameter_reference(&argument.node, generic.node.name.as_str()) {
                        return Ok(AggregateFieldShape::Scalar(SemanticTypeId::POINTER));
                    }
                    Err(error)
                })
                .map(|shape| (generic.node.name.clone(), shape))
        })
        .collect()
}

pub(in crate::semantic_contract) fn aggregate_shape_from_applied_type(
    db: &dyn Db,
    use_key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<AggregateFieldShape, SemanticError> {
    match syntax_type {
        beskid_analysis::syntax::Type::Primitive(_) => {
            Ok(AggregateFieldShape::Scalar(semantic_type_from_syntax(syntax_type)?))
        }
        beskid_analysis::syntax::Type::Complex(path) => resolve_type_declaration(db, use_key, &path.node)
            .map(AggregateFieldShape::Nominal)
            .ok_or_else(|| SemanticError::unavailable("enum_layout")),
        beskid_analysis::syntax::Type::Array(_) => Ok(AggregateFieldShape::Scalar(SemanticTypeId::POINTER)),
        beskid_analysis::syntax::Type::Function { .. } => Err(SemanticError::unavailable("enum_layout")),
    }
}

pub(in crate::semantic_contract) fn enum_layout_from_definition(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
    definition: &beskid_analysis::syntax::EnumDefinition,
    substitutions: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<EnumLayoutFact, SemanticError> {
    if definition.generics.is_empty() != substitutions.is_none() {
        return Err(SemanticError::unavailable("enum_layout"));
    }
    definition
        .variants
        .iter()
        .map(|variant| {
            variant
                .node
                .fields
                .iter()
                .map(|field| enum_field_layout(db, program, index, declaration, field, substitutions))
                .collect::<Result<Vec<_>, SemanticError>>()
                .map(|fields| EnumVariantLayoutFact {
                    name: Arc::from(variant.node.name.node.name.as_str()),
                    fields: fields.into(),
                })
        })
        .collect::<Result<Vec<_>, SemanticError>>()
        .map(|variants| EnumLayoutFact { variants: variants.into() })
}

pub(in crate::semantic_contract) fn enum_field_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: AstNodeKey,
    field: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Field>,
    substitutions: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(Arc<str>, AggregateFieldShape), SemanticError> {
    if field.node.kind != beskid_analysis::syntax::FieldKind::Value {
        return Err(SemanticError::unavailable("enum_layout"));
    }
    let substituted = match (&field.node.ty.node, substitutions) {
        (beskid_analysis::syntax::Type::Complex(path), Some(substitutions)) => {
            let [segment] = path.node.segments.as_slice() else {
                return Err(SemanticError::unavailable("enum_layout"));
            };
            segment
                .node
                .type_args
                .is_empty()
                .then(|| substitutions.get(segment.node.name.node.name.as_str()).copied())
                .flatten()
        }
        _ => None,
    };
    substituted
        .map(|shape| (Arc::from(field.node.name.node.name.as_str()), shape))
        .map(Ok)
        .unwrap_or_else(|| aggregate_field_layout(db, program, index, declaration, field))
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn enum_constructor_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumConstructorFact> {
    with_node(db, syntax, key, |program, index, node| {
        let constructor = node.of::<beskid_analysis::syntax::EnumConstructorExpression>()?;
        let type_path = contextual_enum_constructor_type_path(program, index, key, constructor)
            .unwrap_or(&constructor.path.node.type_path.node);
        let declaration =
            resolve_type_declaration(db, key, type_path).ok_or_else(|| SemanticError::unavailable("enum_constructor"));
        let declaration = match declaration {
            Ok(declaration) => declaration,
            Err(error) => return Some(Err(error)),
        };
        let layout = match enum_layout(db, key) {
            Ok(Some(layout)) => layout,
            Ok(None) | Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        let variant_name = constructor.path.node.variant.node.name.as_str();
        let Some(variant_index) = layout.variants.iter().position(|variant| variant.name.as_ref() == variant_name)
        else {
            return Some(Err(SemanticError::unavailable("enum_constructor")));
        };
        let variant = &layout.variants[variant_index];
        if variant.fields.len() != constructor.args.len() || variant.fields.len() > 1 {
            return Some(Err(SemanticError::unavailable("enum_constructor")));
        }
        let payload = constructor
            .args
            .first()
            .map(|argument| {
                index
                    .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                    .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
                    .ok_or_else(|| SemanticError::unavailable("enum_constructor"))
            })
            .transpose();
        let variant_index = match u32::try_from(variant_index) {
            Ok(variant_index) => variant_index,
            Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        Some(payload.map(|payload| EnumConstructorFact { declaration, variant_index, payload }))
    })?
    .transpose()
}

#[salsa::tracked]
pub(in crate::semantic_contract) fn enum_match_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumMatchFact> {
    with_node(db, syntax, key, |program, index, node| {
        let expression = node.of::<beskid_analysis::syntax::MatchExpression>()?;
        let (declaration, layout) = match enum_match_scrutinee_layout(db, program, index, key, expression) {
            Some(Ok(fact)) => fact,
            Some(Err(error)) => return Some(Err(error)),
            None => return Some(Err(SemanticError::unavailable("enum_match"))),
        };
        let mut arms = Vec::with_capacity(expression.arms.len());
        for arm in &expression.arms {
            if arm.node.guard.is_some() {
                return Some(Err(SemanticError::unavailable("enum_match")));
            }
            let arm_node = index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(arm))
                .ok_or_else(|| SemanticError::unavailable("enum_match"));
            let arm_node = match arm_node {
                Ok(arm_node) => arm_node,
                Err(error) => return Some(Err(error)),
            };
            let body = index
                .direct_child_id(program, arm_node, beskid_analysis::syntax_query::DynNodeRef::from(&arm.node.value))
                .map(|body| AstNodeKey { node: normalized_expression_node(index, body), ..key })
                .ok_or_else(|| SemanticError::unavailable("enum_match"));
            let body = match body {
                Ok(body) => body,
                Err(error) => return Some(Err(error)),
            };
            let arm_fact = match &arm.node.pattern.node {
                beskid_analysis::syntax::Pattern::Wildcard => Ok((None, None)),
                beskid_analysis::syntax::Pattern::Enum(pattern) => {
                    if !enum_pattern_targets_declaration(db, declaration, &pattern.node.path.node.type_path.node) {
                        return Some(Err(SemanticError::unavailable("enum_match")));
                    }
                    let name = pattern.node.path.node.variant.node.name.as_str();
                    let Some((variant_index, variant)) =
                        layout.variants.iter().enumerate().find(|(_, variant)| variant.name.as_ref() == name)
                    else {
                        return Some(Err(SemanticError::unavailable("enum_match")));
                    };
                    if variant.fields.len() != pattern.node.items.len() || variant.fields.len() > 1 {
                        return Some(Err(SemanticError::unavailable("enum_match")));
                    }
                    let binding = match pattern.node.items.as_slice() {
                        [] => None,
                        [item] if matches!(item.node, beskid_analysis::syntax::Pattern::Wildcard) => None,
                        [item] if matches!(item.node, beskid_analysis::syntax::Pattern::Identifier(_)) => {
                            let Some(pattern_node) = index
                                .direct_child_id(
                                    program,
                                    arm_node,
                                    beskid_analysis::syntax_query::DynNodeRef::from(&arm.node.pattern),
                                )
                                .and_then(|node| {
                                    index.direct_child_id(
                                        program,
                                        node,
                                        beskid_analysis::syntax_query::DynNodeRef::from(pattern),
                                    )
                                })
                                .and_then(|node| {
                                    index.direct_child_id(
                                        program,
                                        node,
                                        beskid_analysis::syntax_query::DynNodeRef::from(item),
                                    )
                                })
                                .and_then(|node| index.children(node)?.first().copied())
                            else {
                                return Some(Err(SemanticError::unavailable("enum_match")));
                            };
                            Some(EnumMatchBindingFact {
                                declaration: AstNodeKey { node: pattern_node, ..key },
                                payload: variant.fields[0].1,
                            })
                        }
                        _ => return Some(Err(SemanticError::unavailable("enum_match"))),
                    };
                    let Ok(variant_index) = u32::try_from(variant_index) else {
                        return Some(Err(SemanticError::unavailable("enum_match")));
                    };
                    Ok((Some(variant_index), binding))
                }
                _ => Err(SemanticError::unavailable("enum_match")),
            };
            let (variant_index, binding) = match arm_fact {
                Ok(fact) => fact,
                Err(error) => return Some(Err(error)),
            };
            arms.push(EnumMatchArmFact { variant_index, body, binding });
        }
        Some(Ok(EnumMatchFact { declaration, layout, arms: arms.into() }))
    })?
    .transpose()
}

pub(in crate::semantic_contract) fn enum_pattern_targets_declaration(
    db: &dyn Db,
    declaration: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let Some((terminal, module_path)) = path.segments.split_last() else {
        return false;
    };
    if !terminal.node.type_args.is_empty() || module_path.iter().any(|segment| !segment.node.type_args.is_empty()) {
        return false;
    }
    let Some(syntax) =
        db.syntax_unit(declaration.unit).filter(|syntax| syntax.generation(db) == declaration.generation)
    else {
        return false;
    };
    let program = syntax.expanded_program(db);
    syntax
        .syntax_index(db)
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .is_some_and(|definition| definition.name.node.name == terminal.node.name.node.name)
}

/// Resolve the intentionally narrow generic-match surface: an unqualified local path whose
/// declaration is a parameter or a `let` with an explicit complex type annotation. Inferred,
/// chained, and computed scrutinees remain unavailable rather than reviving HIR reconstruction.
pub(in crate::semantic_contract) fn enum_match_scrutinee_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
) -> Option<Result<(AstNodeKey, EnumLayoutFact), SemanticError>> {
    if let beskid_analysis::syntax::Expression::EnumConstructor(constructor) = &expression.scrutinee.node {
        let declaration = resolve_type_declaration(db, key, &constructor.node.path.node.type_path.node)?;
        return Some(
            enum_layout(db, declaration)
                .and_then(|layout| layout.ok_or_else(|| SemanticError::unavailable("enum_match")))
                .map(|layout| (declaration, layout)),
        );
    }
    let beskid_analysis::syntax::Expression::Path(path) = &expression.scrutinee.node else {
        return None;
    };
    if path.node.path.node.segments.len() == 2 {
        let scrutinee = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(expression.scrutinee.as_ref()),
        )?;
        let scrutinee = normalized_expression_node(index, scrutinee);
        let access = aggregate_field_access(db, AstNodeKey { node: scrutinee, ..key }).ok().flatten()?;
        let layout = aggregate_layout(db, access.declaration).ok().flatten()?;
        let field = layout.fields.get(usize::try_from(access.index).ok()?)?;
        let AggregateFieldShape::Nominal(declaration) = field.1 else {
            return None;
        };
        return Some(
            enum_layout(db, declaration)
                .and_then(|layout| layout.ok_or_else(|| SemanticError::unavailable("enum_match")))
                .map(|layout| (declaration, layout)),
        );
    }
    let [segment] = path.node.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let local = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())?;
    let parent = parent_node(index, local)?;
    if index.kind(parent)? == beskid_analysis::syntax_query::NodeKind::Pattern {
        let binding = match pattern_binding_fact(db, index, key, local)? {
            Ok(binding) => binding,
            Err(error) => return Some(Err(error)),
        };
        let AggregateFieldShape::Nominal(declaration) = binding.payload else {
            return Some(Err(SemanticError::unavailable("enum_match")));
        };
        return Some(
            enum_layout(db, declaration)
                .and_then(|layout| layout.ok_or_else(|| SemanticError::unavailable("enum_match")))
                .map(|layout| (declaration, layout)),
        );
    }
    let annotation = match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::Parameter => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::Parameter>()
            .map(|parameter| &parameter.ty.node),
        beskid_analysis::syntax_query::NodeKind::LetStatement => index
            .node_at(program, parent)?
            .of::<beskid_analysis::syntax::LetStatement>()
            .and_then(|statement| statement.type_annotation.as_ref())
            .map(|annotation| &annotation.node),
        _ => None,
    }?;
    let beskid_analysis::syntax::Type::Complex(path) = annotation else {
        return None;
    };
    let declaration = resolve_type_declaration(db, key, &path.node)?;
    Some(instantiated_enum_layout_for_path(db, key, &path.node).map(|layout| (declaration, layout)))
}
