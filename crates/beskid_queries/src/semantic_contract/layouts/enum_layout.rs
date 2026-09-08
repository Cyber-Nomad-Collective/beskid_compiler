//! Canonical semantic layout implementation.

#[cfg(test)]
mod scalar_payload_tests;

use super::super::*;
use crate::semantic_contract::typing::managed_reference_kind_for_syntax_type;

impl EnumLayoutFact {
    /// Compute the sole target-specific physical authority for ABI-v5 enum payloads.
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
        let mut storage_by_position = Vec::<(Option<StorageClass>, Option<StorageClass>)>::new();
        let payloads = self
            .variants
            .iter()
            .map(|variant| {
                variant
                    .fields
                    .iter()
                    .enumerate()
                    .map(|(position, (_, shape))| {
                        if matches!(shape, AggregateFieldShape::Scalar(SemanticTypeId::UNIT)) {
                            return Some(None);
                        }
                        let ty = match shape {
                            AggregateFieldShape::Scalar(ty) => *ty,
                            AggregateFieldShape::Nominal(_) => SemanticTypeId::POINTER,
                        };
                        let layout = ty.scalar_abi_layout(pointer_width)?;
                        if storage_by_position.len() <= position {
                            storage_by_position.resize(position + 1, (None, None));
                        }
                        let (scalar_storage, pointer_storage) = &mut storage_by_position[position];
                        let storage = if layout.is_pointer { pointer_storage } else { scalar_storage };
                        if storage.is_none_or(|current| {
                            layout.size > current.size
                                || (layout.size == current.size && layout.alignment > current.alignment)
                        }) {
                            *storage = Some(StorageClass { ty, size: layout.size, alignment: layout.alignment });
                        }
                        Some(Some((ty, layout.is_pointer)))
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .collect::<Option<Vec<_>>>()?;

        let tag_offset = align_to_layout(header_size, 4)?;
        let mut end = tag_offset.checked_add(4)?;
        let mut object_alignment = header_alignment.max(4);
        let mut storage_fields = Vec::with_capacity(storage_by_position.len() * 2);
        let mut offsets_by_position = Vec::with_capacity(storage_by_position.len());
        let mut pointer_map_offsets = Vec::new();
        for (scalar_storage, pointer_storage) in storage_by_position {
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
            pointer_map_offsets.extend(pointer_offset);
            offsets_by_position.push((scalar_offset, pointer_offset));
        }
        let variants = payloads
            .into_iter()
            .map(|payloads| {
                let payload_fields = payloads
                    .into_iter()
                    .enumerate()
                    .map(|(position, payload)| {
                        let Some((ty, is_pointer)) = payload else {
                            return Some(None);
                        };
                        let (scalar_offset, pointer_offset) = offsets_by_position[position];
                        let offset = if is_pointer { pointer_offset? } else { scalar_offset? };
                        Some(Some((ty, offset)))
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(EnumScalarPayloadVariantLayout { payload_fields: payload_fields.into() })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(EnumScalarPayloadObjectLayout {
            object_size: align_to_layout(end, object_alignment)?,
            object_alignment,
            tag_offset,
            storage_fields: storage_fields.into(),
            pointer_map_offsets: pointer_map_offsets.into(),
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

#[salsa::tracked(persist)]
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
                let type_path = contextual_enum_constructor_type_path(db, program, index, key, constructor)
                    .unwrap_or_else(|| constructor.path.node.type_path.node.clone());
                instantiated_enum_layout_for_path(db, key, &type_path)
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
pub(in crate::semantic_contract) fn contextual_enum_constructor_type_path(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    constructor: &beskid_analysis::syntax::EnumConstructorExpression,
) -> Option<beskid_analysis::syntax::Path> {
    let constructor_path = &constructor.path.node.type_path.node;
    let terminal = constructor_path.segments.last()?;
    if !terminal.node.type_args.is_empty() {
        return None;
    }
    let constructor_name = terminal.node.name.node.name.as_str();
    let mut current = parent_node(index, key.node)?;
    while matches!(
        index.kind(current)?,
        beskid_analysis::syntax_query::NodeKind::Expression
            | beskid_analysis::syntax_query::NodeKind::Statement
            | beskid_analysis::syntax_query::NodeKind::MatchArm
            | beskid_analysis::syntax_query::NodeKind::MatchExpression
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
        beskid_analysis::syntax_query::NodeKind::AssignExpression => {
            let assignment = mutable_local_assignment(db, AstNodeKey { node: current, ..key }).ok().flatten()?;
            explicit_local_declaration_type(program, index, assignment.declaration.node)
        }
        beskid_analysis::syntax_query::NodeKind::CallExpression => {
            return expected_explicit_call_argument_type(db, program, index, key).and_then(|expected| {
                let beskid_analysis::syntax::Type::Complex(path) = expected else { return None };
                let expected_path = path.node;
                let expected_terminal = expected_path.segments.last()?;
                (expected_terminal.node.name.node.name == constructor_name
                    && !expected_terminal.node.type_args.is_empty())
                .then_some(expected_path)
            });
        }
        _ => None,
    }?;
    let beskid_analysis::syntax::Type::Complex(path) = expected else {
        return None;
    };
    let expected_path = &path.node;
    let expected_terminal = expected_path.segments.last()?;
    (expected_terminal.node.name.node.name == constructor_name && !expected_terminal.node.type_args.is_empty())
        .then_some(expected_path.clone())
}

fn explicit_local_declaration_type<'a>(
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<&'a beskid_analysis::syntax::Type> {
    let declaration = index.node_at(program, parent_node(index, declaration)?)?;
    declaration
        .of::<beskid_analysis::syntax::LetStatement>()
        .and_then(|statement| statement.type_annotation.as_ref().map(|annotation| &annotation.node))
        .or_else(|| declaration.of::<beskid_analysis::syntax::Parameter>().map(|parameter| &parameter.ty.node))
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
        beskid_analysis::syntax::Type::Associated { .. } => Err(SemanticError::unavailable("enum_layout")),
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

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_constructor_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumConstructorFact> {
    with_node(db, syntax, key, |program, index, node| {
        let constructor = node.of::<beskid_analysis::syntax::EnumConstructorExpression>()?;
        let type_path = contextual_enum_constructor_type_path(db, program, index, key, constructor)
            .unwrap_or_else(|| constructor.path.node.type_path.node.clone());
        let declaration =
            resolve_type_declaration(db, key, &type_path).ok_or_else(|| SemanticError::unavailable("enum_constructor"));
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
        if variant.fields.len() != constructor.args.len() {
            return Some(Err(SemanticError::unavailable("enum_constructor")));
        }
        let payloads = constructor
            .args
            .iter()
            .map(|argument| {
                index
                    .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                    .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
                    .ok_or_else(|| SemanticError::unavailable("enum_constructor"))
            })
            .collect::<Result<Vec<_>, _>>();
        let variant_index = match u32::try_from(variant_index) {
            Ok(variant_index) => variant_index,
            Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        Some(payloads.map(|payloads| EnumConstructorFact { declaration, variant_index, payloads: payloads.into() }))
    })?
    .transpose()
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_constructor_template_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<EnumConstructorTemplate> {
    with_node(db, syntax, key, |program, index, node| {
        let constructor = node.of::<beskid_analysis::syntax::EnumConstructorExpression>()?;
        let contextual = contextual_enum_constructor_type_path(db, program, index, key, constructor)?;
        let declaration = resolve_type_declaration(db, key, &contextual)?;
        let declaration_syntax = db.syntax_unit(declaration.unit)?;
        let definition = declaration_syntax
            .syntax_index(db)
            .node_at(declaration_syntax.expanded_program(db), declaration.node)?
            .of::<beskid_analysis::syntax::EnumDefinition>()?;
        let terminal = contextual.segments.last()?;
        if definition.generics.is_empty() || terminal.node.type_args.len() != definition.generics.len() {
            return Some(Err(SemanticError::unavailable("enum_constructor_template")));
        }

        let owner = nearest_ancestor(index, key.node, |kind| {
            matches!(
                kind,
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::MethodDefinition
            )
        })?;
        let owner_node = index.node_at(program, owner)?;
        let generic_names = if let Some(function) = owner_node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            function.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>()
        } else {
            let type_owner = parent_node(index, owner)?;
            index
                .node_at(program, type_owner)?
                .of::<beskid_analysis::syntax::TypeDefinition>()?
                .generics
                .iter()
                .map(|generic| generic.node.name.as_str())
                .collect::<Vec<_>>()
        };
        let arguments = terminal
            .node
            .type_args
            .iter()
            .map(|argument| {
                aggregate_shape_from_applied_type(db, key, &argument.node)
                    .map(EnumLayoutTemplateArgument::Concrete)
                    .or_else(|error| {
                        let parameter = generic_parameter_reference_name(&argument.node)
                            .filter(|parameter| generic_names.contains(parameter))
                            .ok_or(error)?;
                        Ok(EnumLayoutTemplateArgument::EnclosingParameter(Arc::from(parameter)))
                    })
            })
            .collect::<Result<Vec<_>, SemanticError>>();
        let arguments = match arguments {
            Ok(arguments)
                if arguments
                    .iter()
                    .any(|argument| matches!(argument, EnumLayoutTemplateArgument::EnclosingParameter(_))) =>
            {
                arguments
            }
            Ok(_) => return None,
            Err(error) => return Some(Err(error)),
        };
        let variant_name = constructor.path.node.variant.node.name.as_str();
        let Some(variant_index) = definition
            .variants
            .iter()
            .position(|variant| variant.node.name.node.name == variant_name)
            .and_then(|index| u32::try_from(index).ok())
        else {
            return Some(Err(SemanticError::unavailable("enum_constructor_template")));
        };
        let variant = &definition.variants[usize::try_from(variant_index).ok()?];
        if variant.node.fields.len() != constructor.args.len() {
            return Some(Err(SemanticError::unavailable("enum_constructor_template")));
        }
        let payloads = constructor
            .args
            .iter()
            .map(|argument| {
                index
                    .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                    .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
                    .ok_or_else(|| SemanticError::unavailable("enum_constructor_template"))
            })
            .collect::<Result<Vec<_>, _>>();
        let payloads = match payloads {
            Ok(payloads) => payloads,
            Err(error) => return Some(Err(error)),
        };
        let parameters =
            definition.generics.iter().map(|generic| Arc::<str>::from(generic.node.name.as_str())).collect::<Vec<_>>();
        Some(Ok(EnumConstructorTemplate {
            constructor: EnumConstructorFact { declaration, variant_index, payloads: payloads.into() },
            parameters: parameters.into(),
            arguments: arguments.into(),
        }))
    })?
    .transpose()
}

pub fn enum_constructor_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<EnumConstructorSpecialization> {
    let Some(template) = enum_constructor_template(db, key)? else {
        return Ok(None);
    };
    let substitutions = template
        .parameters
        .iter()
        .zip(template.arguments.iter())
        .map(|(parameter, argument)| {
            let shape = match argument {
                EnumLayoutTemplateArgument::Concrete(shape) => *shape,
                EnumLayoutTemplateArgument::EnclosingParameter(name) => {
                    let semantic = enclosing
                        .iter()
                        .find(|binding| binding.parameter == *name)
                        .map(|binding| binding.argument)
                        .ok_or_else(|| SemanticError::unavailable("enum_constructor_specialization"))?;
                    AggregateFieldShape::Scalar(semantic)
                }
            };
            Ok((parameter.to_string(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    let syntax = db
        .syntax_unit(template.constructor.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, template.constructor.declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_constructor_specialization"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), template.constructor.declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_constructor_specialization"))?;
    let layout = enum_layout_from_definition(
        db,
        syntax.expanded_program(db),
        syntax.syntax_index(db),
        template.constructor.declaration,
        definition,
        Some(&substitutions),
    )?;
    Ok(Some(EnumConstructorSpecialization { constructor: template.constructor, layout }))
}

#[salsa::tracked(persist)]
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
        let environment = match enum_match_ownership_environment(db, program, index, key, expression, declaration, None)
        {
            Ok(environment) => environment,
            Err(error) => return Some(Err(error)),
        };
        Some(materialize_enum_match(db, program, index, key, expression, declaration, layout, &environment))
    })?
    .transpose()
}

/// Materialize a generic match through the immutable specialization of its enclosing item.
///
/// The ordinary `enum_match` fact remains target-neutral and unavailable when a generic payload's
/// ownership cannot be proven. This explicit path applies the call-derived substitutions before
/// constructing layouts or bindings, so pointer-shaped nominal/native identities are never guessed.
pub fn enum_match_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<EnumMatchFact> {
    let syntax = db
        .syntax_unit(key.unit)
        .filter(|syntax| syntax.accepts_key(db, key))
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let expression = index
        .node_at(program, key.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::MatchExpression>())
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let (declaration, layout) =
        enum_match_scrutinee_layout_in_environment(db, program, index, key, expression, &enclosing)?
            .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let environment =
        enum_match_ownership_environment(db, program, index, key, expression, declaration, Some(&enclosing))?;
    materialize_enum_match(db, program, index, key, expression, declaration, layout, &environment).map(Some)
}

fn materialize_enum_match(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
    declaration: AstNodeKey,
    layout: EnumLayoutFact,
    environment: &HashMap<String, ManagedReferenceKind>,
) -> Result<EnumMatchFact, SemanticError> {
    let mut arms = Vec::with_capacity(expression.arms.len());
    for arm in &expression.arms {
        if arm.node.guard.is_some() {
            return Err(SemanticError::unavailable("enum_match"));
        }
        let arm_node = index
            .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(arm))
            .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
        let body = index
            .direct_child_id(program, arm_node, beskid_analysis::syntax_query::DynNodeRef::from(&arm.node.value))
            .map(|body| AstNodeKey { node: normalized_expression_node(index, body), ..key })
            .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
        let pattern_node = index
            .direct_child_id(program, arm_node, beskid_analysis::syntax_query::DynNodeRef::from(&arm.node.pattern))
            .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
        let pattern = materialize_match_pattern(
            db,
            program,
            index,
            key,
            pattern_node,
            &arm.node.pattern,
            MatchPatternExpectation::NominalEnum { declaration, layout: layout.clone() },
            environment,
        )?;
        arms.push(EnumMatchArmFact { pattern, body });
    }
    Ok(EnumMatchFact { declaration, layout, arms: arms.into() })
}

#[derive(Clone)]
enum MatchPatternExpectation {
    Scalar { semantic_type: SemanticTypeId, managed_reference: ManagedReferenceKind },
    Nominal(AstNodeKey),
    NominalEnum { declaration: AstNodeKey, layout: EnumLayoutFact },
}

impl MatchPatternExpectation {
    fn binding_shape(&self) -> AggregateFieldShape {
        match self {
            Self::Scalar { semantic_type, .. } => AggregateFieldShape::Scalar(*semantic_type),
            Self::Nominal(declaration) | Self::NominalEnum { declaration, .. } => {
                AggregateFieldShape::Nominal(*declaration)
            }
        }
    }

    fn managed_reference(&self) -> ManagedReferenceKind {
        match self {
            Self::Scalar { managed_reference, .. } => *managed_reference,
            Self::Nominal(_) | Self::NominalEnum { .. } => ManagedReferenceKind::GcManaged,
        }
    }
}

fn materialize_match_pattern(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    pattern_node: beskid_analysis::syntax::AstNodeId,
    pattern: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Pattern>,
    expected: MatchPatternExpectation,
    environment: &HashMap<String, ManagedReferenceKind>,
) -> Result<EnumMatchPatternFact, SemanticError> {
    match &pattern.node {
        beskid_analysis::syntax::Pattern::Wildcard => Ok(EnumMatchPatternFact::Wildcard),
        beskid_analysis::syntax::Pattern::Identifier(identifier) => {
            if matches!(expected, MatchPatternExpectation::Scalar { semantic_type: SemanticTypeId::UNIT, .. }) {
                return Err(SemanticError::unavailable("enum_match"));
            }
            let declaration = index
                .direct_child_id(program, pattern_node, beskid_analysis::syntax_query::DynNodeRef::from(identifier))
                .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
            Ok(EnumMatchPatternFact::Binding(EnumMatchBindingFact {
                declaration: AstNodeKey { node: declaration, ..key },
                payload: expected.binding_shape(),
                managed_reference: expected.managed_reference(),
            }))
        }
        beskid_analysis::syntax::Pattern::Literal(literal) => {
            let literal_node = index
                .direct_child_id(program, pattern_node, beskid_analysis::syntax_query::DynNodeRef::from(literal))
                .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
            let literal_key = AstNodeKey { node: literal_node, ..key };
            match &literal.node {
                beskid_analysis::syntax::Literal::Unit
                    if matches!(
                        expected,
                        MatchPatternExpectation::Scalar { semantic_type: SemanticTypeId::UNIT, .. }
                    ) =>
                {
                    Ok(EnumMatchPatternFact::UnitLiteral { literal: literal_key })
                }
                beskid_analysis::syntax::Literal::Integer(value) => materialize_scalar_literal(
                    literal_key,
                    &expected,
                    semantic_type_for_literal(&literal.node),
                    LiteralFact::Integer(Arc::from(value.as_str())),
                ),
                beskid_analysis::syntax::Literal::Bool(value) => {
                    materialize_scalar_literal(literal_key, &expected, SemanticTypeId::BOOL, LiteralFact::Bool(*value))
                }
                beskid_analysis::syntax::Literal::Char(value) => materialize_scalar_literal(
                    literal_key,
                    &expected,
                    SemanticTypeId::CHAR,
                    LiteralFact::Char(Arc::from(value.as_str())),
                ),
                beskid_analysis::syntax::Literal::Float(_)
                | beskid_analysis::syntax::Literal::String(_)
                | beskid_analysis::syntax::Literal::Unit => Err(SemanticError::unavailable("enum_match")),
            }
        }
        beskid_analysis::syntax::Pattern::Enum(enum_pattern) => {
            let (declaration, layout) = match expected {
                MatchPatternExpectation::NominalEnum { declaration, layout } => (declaration, layout),
                MatchPatternExpectation::Nominal(declaration) => {
                    let layout =
                        enum_layout(db, declaration)?.ok_or_else(|| SemanticError::unavailable("enum_match"))?;
                    (declaration, layout)
                }
                MatchPatternExpectation::Scalar { .. } => return Err(SemanticError::unavailable("enum_match")),
            };
            materialize_enum_pattern(
                db,
                program,
                index,
                key,
                pattern_node,
                enum_pattern,
                (declaration, layout),
                environment,
            )
        }
    }
}

fn materialize_scalar_literal(
    literal: AstNodeKey,
    expected: &MatchPatternExpectation,
    semantic_type: SemanticTypeId,
    value: LiteralFact,
) -> Result<EnumMatchPatternFact, SemanticError> {
    if !matches!(expected, MatchPatternExpectation::Scalar { semantic_type: expected, .. } if *expected == semantic_type)
    {
        return Err(SemanticError::unavailable("enum_match"));
    }
    Ok(EnumMatchPatternFact::ScalarLiteral(EnumMatchScalarLiteralFact { literal, semantic_type, value }))
}

fn materialize_enum_pattern(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    pattern_node: beskid_analysis::syntax::AstNodeId,
    pattern: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::EnumPattern>,
    applied_enum: (AstNodeKey, EnumLayoutFact),
    environment: &HashMap<String, ManagedReferenceKind>,
) -> Result<EnumMatchPatternFact, SemanticError> {
    let (declaration, layout) = applied_enum;
    if !enum_pattern_targets_declaration(db, declaration, &pattern.node.path.node.type_path.node) {
        return Err(SemanticError::unavailable("enum_match"));
    }
    let name = pattern.node.path.node.variant.node.name.as_str();
    let (variant_index, variant) = layout
        .variants
        .iter()
        .enumerate()
        .find(|(_, variant)| variant.name.as_ref() == name)
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if variant.fields.len() != pattern.node.items.len() {
        return Err(SemanticError::unavailable("enum_match"));
    }
    let enum_pattern_node = index
        .direct_child_id(program, pattern_node, beskid_analysis::syntax_query::DynNodeRef::from(pattern))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let items = pattern
        .node
        .items
        .iter()
        .zip(variant.fields.iter())
        .enumerate()
        .map(|(field_index, (item, (_, shape)))| {
            let item_node = index
                .direct_child_id(program, enum_pattern_node, beskid_analysis::syntax_query::DynNodeRef::from(item))
                .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
            let expected = match shape {
                AggregateFieldShape::Scalar(semantic_type) => MatchPatternExpectation::Scalar {
                    semantic_type: *semantic_type,
                    managed_reference: enum_variant_field_managed_reference(
                        db,
                        declaration,
                        variant_index,
                        field_index,
                        *shape,
                        environment,
                    )?,
                },
                AggregateFieldShape::Nominal(declaration) => MatchPatternExpectation::Nominal(*declaration),
            };
            materialize_match_pattern(db, program, index, key, item_node, item, expected, environment)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EnumMatchPatternFact::Enum(EnumMatchVariantPatternFact {
        declaration,
        layout,
        variant_index: u32::try_from(variant_index).map_err(|_| SemanticError::unavailable("enum_match"))?,
        items: items.into(),
    }))
}

fn enum_variant_field_managed_reference(
    db: &dyn Db,
    declaration: AstNodeKey,
    variant_index: usize,
    field_index: usize,
    applied_shape: AggregateFieldShape,
    environment: &HashMap<String, ManagedReferenceKind>,
) -> Result<ManagedReferenceKind, SemanticError> {
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let field = definition
        .variants
        .get(variant_index)
        .and_then(|variant| variant.node.fields.get(field_index))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;

    if let Some(parameter) = generic_parameter_reference_name(&field.node.ty.node) {
        if let Some(managed_reference) = environment.get(parameter) {
            return Ok(*managed_reference);
        }
        return match applied_shape {
            AggregateFieldShape::Nominal(_) | AggregateFieldShape::Scalar(SemanticTypeId::STRING) => {
                Ok(ManagedReferenceKind::GcManaged)
            }
            AggregateFieldShape::Scalar(SemanticTypeId::POINTER) => Err(SemanticError::unavailable("enum_match")),
            AggregateFieldShape::Scalar(_) => Ok(ManagedReferenceKind::NativeOrScalar),
        };
    }
    managed_reference_kind_for_syntax_type(&field.node.ty.node)
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
    if matches!(expression.scrutinee.node, beskid_analysis::syntax::Expression::Call(_)) {
        let scrutinee = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(expression.scrutinee.as_ref()),
        )?;
        let call = AstNodeKey { node: normalized_expression_node(index, scrutinee), ..key };
        return Some(enum_layout_for_direct_call_result(db, call));
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
        let field = access.layout.fields.get(usize::try_from(access.index).ok()?)?;
        let AggregateFieldShape::Nominal(declaration) = field.1 else {
            return None;
        };
        return Some(
            enum_layout(db, declaration)
                .and_then(|layout| layout.ok_or_else(|| SemanticError::unavailable("enum_match")))
                .map(|layout| (declaration, layout)),
        );
    }
    let local = enum_match_scrutinee_local_declaration(program, index, key, expression)?;
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
    let path = enum_match_scrutinee_explicit_type_path(program, index, key, expression)?;
    let declaration = resolve_type_declaration(db, key, &path)?;
    Some(instantiated_enum_layout_for_path(db, key, &path).map(|layout| (declaration, layout)))
}

/// Retain the source-level applied type of a local match scrutinee.
///
/// Enum layout intentionally erases source identities down to ABI shapes. Pattern binding also
/// needs the applied type to distinguish a managed array/nominal from an unmanaged native pointer,
/// so this helper is the single syntax-backed seam used by both layout and ownership projection.
fn enum_match_scrutinee_explicit_type_path(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
) -> Option<beskid_analysis::syntax::Path> {
    let local = enum_match_scrutinee_local_declaration(program, index, key, expression)?;
    let parent = parent_node(index, local)?;
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
    Some(path.node.clone())
}

fn enum_match_scrutinee_local_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let beskid_analysis::syntax::Expression::Path(path) = &expression.scrutinee.node else {
        return None;
    };
    let [segment] = path.node.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())
}

/// Project an applied generic enum type into the ownership facts needed by pattern bindings.
///
/// Physical enum layouts collapse arrays, functions, nominals, and native pointers to pointer
/// shapes. This environment preserves the source distinction without adding a second layout path.
/// If an applied argument still names an enclosing generic, only an explicit call specialization
/// may provide its ownership; missing substitutions fail closed.
fn enum_match_ownership_environment(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
    declaration: AstNodeKey,
    enclosing: Option<&[GenericSubstitution]>,
) -> Result<HashMap<String, ManagedReferenceKind>, SemanticError> {
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if definition.generics.is_empty() {
        return Ok(HashMap::new());
    }
    let Some(path) = enum_match_scrutinee_explicit_type_path(program, index, key, expression) else {
        // Other supported scrutinee shapes retain their existing conservative field inference.
        // In particular, pointer-shaped generic fields remain unavailable rather than guessed.
        return Ok(HashMap::new());
    };
    let terminal = path.segments.last().ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if terminal.node.type_args.len() != definition.generics.len() {
        return Err(SemanticError::unavailable("enum_match"));
    }
    definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .map(|(parameter, argument)| {
            let ownership = if let Some(name) = generic_parameter_reference_name(&argument.node) {
                if let Some(binding) =
                    enclosing.and_then(|bindings| bindings.iter().find(|binding| binding.parameter.as_ref() == name))
                {
                    binding.managed_reference_kind()
                } else if aggregate_shape_from_applied_type(db, key, &argument.node).is_ok() {
                    // A one-segment nominal path has the same syntax shape as a generic parameter.
                    // A resolvable applied nominal is concrete and therefore managed.
                    managed_reference_kind_for_syntax_type(&argument.node)?
                } else {
                    return Err(SemanticError::unavailable("enum_match"));
                }
            } else {
                managed_reference_kind_for_syntax_type(&argument.node)?
            };
            Ok((parameter.node.name.clone(), ownership))
        })
        .collect()
}

fn enum_match_scrutinee_layout_in_environment(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    expression: &beskid_analysis::syntax::MatchExpression,
    enclosing: &[GenericSubstitution],
) -> Result<Option<(AstNodeKey, EnumLayoutFact)>, SemanticError> {
    let Some(path) = enum_match_scrutinee_explicit_type_path(program, index, key, expression) else {
        return Ok(None);
    };
    let declaration = resolve_type_declaration(db, key, &path)
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let layout = instantiated_enum_layout_for_path_in_environment(db, key, &path, enclosing)?;
    Ok(Some((declaration, layout)))
}

fn instantiated_enum_layout_for_path_in_environment(
    db: &dyn Db,
    use_key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
    enclosing: &[GenericSubstitution],
) -> Result<EnumLayoutFact, SemanticError> {
    let declaration = resolve_type_declaration(db, use_key, path)
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let definition = index
        .node_at(program, declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    if definition.generics.is_empty() {
        return enum_layout_from_definition(db, program, index, declaration, definition, None);
    }
    let terminal = path.segments.last().ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))?;
    if terminal.node.type_args.len() != definition.generics.len() {
        return Err(SemanticError::unavailable("enum_match_specialization"));
    }
    let substitutions = definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .map(|(parameter, argument)| {
            let shape = aggregate_shape_from_applied_type(db, use_key, &argument.node).or_else(|error| {
                let name = generic_parameter_reference_name(&argument.node).ok_or(error)?;
                enclosing
                    .iter()
                    .find(|binding| binding.parameter.as_ref() == name)
                    .map(|binding| AggregateFieldShape::Scalar(binding.argument))
                    .ok_or_else(|| SemanticError::unavailable("enum_match_specialization"))
            })?;
            Ok((parameter.node.name.clone(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    enum_layout_from_definition(db, program, index, declaration, definition, Some(&substitutions))
}

/// Preserve nominal enum provenance for a direct call used as a `match` scrutinee.
///
/// The concrete call specialization remains the authority for generic method-owner bindings;
/// this only projects those bindings through the declaration's return type into the existing
/// enum layout representation.
fn enum_layout_for_direct_call_result(
    db: &dyn Db,
    call: AstNodeKey,
) -> Result<(AstNodeKey, EnumLayoutFact), SemanticError> {
    let instance = generic_specialization_instance_for_call(db, call)?;
    let callable_syntax = db
        .syntax_unit(instance.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, instance.declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let callable = callable_syntax
        .syntax_index(db)
        .node_at(callable_syntax.expanded_program(db), instance.declaration.node)
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let return_type = callable
        .of::<beskid_analysis::syntax::FunctionDefinition>()
        .and_then(|function| function.return_type.as_ref())
        .or_else(|| {
            callable.of::<beskid_analysis::syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref())
        })
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let beskid_analysis::syntax::Type::Complex(path) = &return_type.node else {
        return Err(SemanticError::unavailable("enum_match"));
    };
    let declaration = resolve_type_declaration(db, instance.declaration, &path.node)
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let enum_syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    let definition = enum_syntax
        .syntax_index(db)
        .node_at(enum_syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if definition.generics.is_empty() {
        return enum_layout_from_definition(
            db,
            enum_syntax.expanded_program(db),
            enum_syntax.syntax_index(db),
            declaration,
            definition,
            None,
        )
        .map(|layout| (declaration, layout));
    }
    let terminal = path.node.segments.last().ok_or_else(|| SemanticError::unavailable("enum_match"))?;
    if terminal.node.type_args.len() != definition.generics.len() {
        return Err(SemanticError::unavailable("enum_match"));
    }
    let callable_substitutions = instance
        .substitutions
        .iter()
        .map(|substitution| (substitution.parameter.as_ref(), substitution.argument))
        .collect::<HashMap<_, _>>();
    let substitutions = definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .map(|(generic, argument)| {
            let shape = specialized_call_result_argument_shape(
                db,
                instance.declaration,
                &argument.node,
                &callable_substitutions,
            )?;
            Ok((generic.node.name.clone(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    enum_layout_from_definition(
        db,
        enum_syntax.expanded_program(db),
        enum_syntax.syntax_index(db),
        declaration,
        definition,
        Some(&substitutions),
    )
    .map(|layout| (declaration, layout))
}

fn specialized_call_result_argument_shape(
    db: &dyn Db,
    declaration: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
    substitutions: &HashMap<&str, SemanticTypeId>,
) -> Result<AggregateFieldShape, SemanticError> {
    if let beskid_analysis::syntax::Type::Complex(path) = syntax_type
        && let [segment] = path.node.segments.as_slice()
        && segment.node.type_args.is_empty()
        && let Some(argument) = substitutions.get(segment.node.name.node.name.as_str())
    {
        return Ok(AggregateFieldShape::Scalar(*argument));
    }
    aggregate_shape_from_applied_type(db, declaration, syntax_type)
}
