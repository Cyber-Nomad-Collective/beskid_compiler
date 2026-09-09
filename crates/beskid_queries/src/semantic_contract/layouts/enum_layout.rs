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
                    // `unit` is zero-sized: it occupies no storage slot, so a variant whose sole
                    // payload is `unit` is laid out as if it had no payload.
                    if ty == SemanticTypeId::UNIT {
                        Some(None)
                    } else {
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
                if let Some(type_path) = contextual_enum_constructor_type_path(program, index, key, constructor) {
                    return instantiated_enum_layout_for_path(db, key, type_path);
                }
                match instantiated_enum_layout_for_path(db, key, &constructor.path.node.type_path.node) {
                    Ok(layout) => Ok(layout),
                    Err(error) if error.is_unavailable() => {
                        enum_layout_for_call_argument_constructor(db, program, index, key).map(Ok).unwrap_or(Err(error))
                    }
                    Err(error) => Err(error),
                }
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

/// Resolve the enum declaration and instantiation context for a constructor used as a direct
/// argument to a generic call with explicit type arguments.
///
/// Returns the enum declaration, the parameter's nominal type path (e.g. `Result<TValue, TError>`),
/// and a map from callee generic name to its explicit call-site type argument. This closes the gap
/// left by [`contextual_enum_constructor_type_path`], which only proves let/return annotations and
/// falls back to the bare generic enum path (uninstantiable) for call arguments.
pub(in crate::semantic_contract) struct CallArgumentEnumContext<'a> {
    pub call_key: AstNodeKey,
    pub enum_declaration: AstNodeKey,
    pub parameter_path: &'a beskid_analysis::syntax::Path,
    pub callee_generic_args: HashMap<&'a str, &'a beskid_analysis::syntax::Type>,
}

pub(in crate::semantic_contract) fn resolve_call_argument_enum_context<'a>(
    db: &'a dyn Db,
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &'a beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<CallArgumentEnumContext<'a>> {
    let call_node =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::CallExpression)?;
    let call = index.node_at(program, call_node)?.of::<beskid_analysis::syntax::CallExpression>()?;
    let beskid_analysis::syntax::Expression::Path(callee) = &call.callee.node else {
        return None;
    };
    let path = &callee.node.path.node;
    let type_argument_syntax = explicit_generic_type_argument_syntax(path)?;
    let call_key = AstNodeKey { node: call_node, ..key };
    let declaration = resolve_item_declaration_candidate(db, program, index, call_key, path)?;
    let declaration_syntax =
        db.syntax_unit(declaration.unit).filter(|syntax| syntax.generation(db) == declaration.generation)?;
    let declaration_program = declaration_syntax.expanded_program(db);
    let declaration_index = declaration_syntax.syntax_index(db);
    let function = declaration_index
        .node_at(declaration_program, declaration.node)?
        .of::<beskid_analysis::syntax::FunctionDefinition>()?;
    if function.generics.len() != type_argument_syntax.len() {
        return None;
    }
    let callee_generic_args: HashMap<&str, &beskid_analysis::syntax::Type> = function
        .generics
        .iter()
        .zip(type_argument_syntax.iter())
        .map(|(generic, argument)| (generic.node.name.as_str(), &argument.node))
        .collect();
    let arguments = call_arguments(db, call_key).ok().flatten()?;
    let argument_index = arguments.iter().position(|argument| {
        let mut descendant = key.node;
        loop {
            if descendant == argument.node {
                return true;
            }
            match index.metadata_for(key.generation, descendant).and_then(|meta| meta.parent) {
                Some(parent) => descendant = parent,
                None => return false,
            }
        }
    })?;
    let parameter = function.parameters.get(argument_index)?;
    let beskid_analysis::syntax::Type::Complex(parameter_path) = &parameter.node.ty.node else {
        return None;
    };
    let enum_declaration = resolve_type_declaration(db, call_key, &parameter_path.node)?;
    Some(CallArgumentEnumContext {
        call_key,
        enum_declaration,
        parameter_path: &parameter_path.node,
        callee_generic_args,
    })
}

/// Instantiate the enum layout for a constructor used as a direct argument to a generic call
/// with explicit type arguments, using the call parameter's instantiated nominal type.
pub(in crate::semantic_contract) fn enum_layout_for_call_argument_constructor(
    db: &dyn Db,
    _program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    _index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<EnumLayoutFact> {
    let context = resolve_call_argument_enum_context(db, _program, _index, key)?;
    let enum_declaration = context.enum_declaration;
    let enum_syntax =
        db.syntax_unit(enum_declaration.unit).filter(|syntax| syntax.generation(db) == enum_declaration.generation)?;
    let enum_program = enum_syntax.expanded_program(db);
    let enum_index = enum_syntax.syntax_index(db);
    let enum_definition =
        enum_index.node_at(enum_program, enum_declaration.node)?.of::<beskid_analysis::syntax::EnumDefinition>()?;
    if enum_definition.generics.is_empty() {
        return enum_layout_from_definition(db, enum_program, enum_index, enum_declaration, enum_definition, None).ok();
    }
    let parameter_terminal = context.parameter_path.segments.last()?;
    if parameter_terminal.node.type_args.len() != enum_definition.generics.len() {
        return None;
    }
    let mut substitutions = HashMap::new();
    for (enum_generic, parameter_arg) in enum_definition.generics.iter().zip(parameter_terminal.node.type_args.iter()) {
        let concrete_arg = generic_parameter_reference_name(&parameter_arg.node)
            .and_then(|name| context.callee_generic_args.get(name).copied())
            .unwrap_or(&parameter_arg.node);
        let shape = aggregate_shape_from_applied_type(db, context.call_key, concrete_arg)
            .or_else(|error| {
                if let Some(name) = generic_parameter_reference_name(concrete_arg)
                    && enum_definition.generics.iter().any(|generic| generic.node.name.as_str() == name)
                {
                    return Ok(AggregateFieldShape::Scalar(SemanticTypeId::POINTER));
                }
                Err(error)
            })
            .ok()?;
        substitutions.insert(enum_generic.node.name.clone(), shape);
    }
    enum_layout_from_definition(db, enum_program, enum_index, enum_declaration, enum_definition, Some(&substitutions))
        .ok()
}

/// Instantiate the enum layout for a constructor inside a specialized generic function body.
///
/// The constructor's enum type is taken from the contextual return/let annotation (e.g.
/// `Result<TNext, TError>`); its generic-parameter type arguments are resolved through the
/// enclosing function's specialization substitutions (e.g. `TNext -> i64`, `TError -> string`).
/// This closes the gap left by [`instantiated_enum_layout_for_path`], which can only resolve
/// concrete or enum-generic type arguments, not the enclosing function's generic parameters.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_layout_for_specialized_constructor_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    enclosing_substitutions: std::sync::Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<EnumLayoutFact> {
    with_node(db, syntax, key, |program, index, node| {
        let type_path = if let Some(constructor) = node.of::<beskid_analysis::syntax::EnumConstructorExpression>() {
            contextual_enum_constructor_type_path(program, index, key, constructor)
                .unwrap_or(&constructor.path.node.type_path.node)
        } else if let Some(match_expression) = node.of::<beskid_analysis::syntax::MatchExpression>() {
            specialized_match_scrutinee_type_path(db, program, index, key, match_expression)?
        } else {
            return None;
        };
        Some(specialized_enum_layout_from_type_path(db, key, type_path, &enclosing_substitutions))
    })?
    .transpose()
}

/// Resolve the enum type path of a match scrutinee that names a parameter or local whose declared
/// type carries the enclosing function's generic parameters (e.g. `Result<TValue, TError>`).
fn specialized_match_scrutinee_type_path<'a>(
    db: &dyn Db,
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &'a beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    match_expression: &beskid_analysis::syntax::MatchExpression,
) -> Option<&'a beskid_analysis::syntax::Path> {
    let beskid_analysis::syntax::Expression::Path(path) = &match_expression.scrutinee.node else {
        return None;
    };
    let [segment] = path.node.path.node.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let local = resolve_lexical_declaration(program, index, key.node, segment.node.name.node.name.as_str())?;
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
    let _ = db;
    let beskid_analysis::syntax::Type::Complex(path) = annotation else {
        return None;
    };
    Some(&path.node)
}

/// Compute the enum layout for a type path whose generic type arguments may reference the
/// enclosing function's generic parameters, resolved through the enclosing specialization.
fn specialized_enum_layout_from_type_path(
    db: &dyn Db,
    key: AstNodeKey,
    type_path: &beskid_analysis::syntax::Path,
    enclosing_substitutions: &std::sync::Arc<[GenericSubstitution]>,
) -> Result<EnumLayoutFact, SemanticError> {
    let declaration =
        resolve_type_declaration(db, key, type_path).ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    let enum_syntax = db
        .syntax_unit(declaration.unit)
        .filter(|s| s.generation(db) == declaration.generation)
        .ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    let enum_program = enum_syntax.expanded_program(db);
    let enum_index = enum_syntax.syntax_index(db);
    let enum_definition = enum_index
        .node_at(enum_program, declaration.node)
        .ok_or_else(|| SemanticError::unavailable("enum_layout"))?
        .of::<beskid_analysis::syntax::EnumDefinition>()
        .ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    if enum_definition.generics.is_empty() {
        return enum_layout_from_definition(db, enum_program, enum_index, declaration, enum_definition, None);
    }
    let terminal = type_path.segments.last().ok_or_else(|| SemanticError::unavailable("enum_layout"))?;
    if terminal.node.type_args.len() != enum_definition.generics.len() {
        return Err(SemanticError::unavailable("enum_layout"));
    }
    let enclosing_map: HashMap<&str, SemanticTypeId> =
        enclosing_substitutions.iter().map(|binding| (binding.parameter.as_ref(), binding.argument)).collect();
    let mut substitutions = HashMap::new();
    for (enum_generic, type_arg) in enum_definition.generics.iter().zip(terminal.node.type_args.iter()) {
        let shape = if let Some(name) = generic_parameter_reference_name(&type_arg.node)
            && let Some(&semantic) = enclosing_map.get(name)
        {
            if matches!(semantic, SemanticTypeId::POINTER | SemanticTypeId::STRING | SemanticTypeId::WORD) {
                AggregateFieldShape::Scalar(SemanticTypeId::POINTER)
            } else {
                AggregateFieldShape::Scalar(semantic)
            }
        } else {
            aggregate_shape_from_applied_type(db, key, &type_arg.node).or_else(|error| {
                if type_syntax_is_generic_parameter_reference(&type_arg.node, enum_generic.node.name.as_str()) {
                    Ok(AggregateFieldShape::Scalar(SemanticTypeId::POINTER))
                } else {
                    Err(error)
                }
            })?
        };
        substitutions.insert(enum_generic.node.name.clone(), shape);
    }
    enum_layout_from_definition(db, enum_program, enum_index, declaration, enum_definition, Some(&substitutions))
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
        beskid_analysis::syntax::Type::This_ => Err(SemanticError::unavailable("enum_layout")),
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
        let type_path = contextual_enum_constructor_type_path(program, index, key, constructor)
            .unwrap_or(&constructor.path.node.type_path.node);
        let declaration = resolve_type_declaration(db, key, type_path)
            .or_else(|| {
                resolve_call_argument_enum_context(db, program, index, key).map(|context| context.enum_declaration)
            })
            .ok_or_else(|| SemanticError::unavailable("enum_constructor"));
        let declaration = match declaration {
            Ok(declaration) => declaration,
            Err(error) => return Some(Err(error)),
        };
        let layout = match enum_layout(db, key) {
            Ok(Some(layout)) => layout,
            Ok(None) | Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        Some(enum_constructor_fact_from_layout(db, program, index, key, declaration, &layout, constructor))
    })?
    .transpose()
}

/// Compute the enum constructor fact given the enum declaration and layout.
pub(in crate::semantic_contract) fn enum_constructor_fact_from_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: AstNodeKey,
    layout: &EnumLayoutFact,
    constructor: &beskid_analysis::syntax::EnumConstructorExpression,
) -> Result<EnumConstructorFact, SemanticError> {
    let _ = db;
    let variant_name = constructor.path.node.variant.node.name.as_str();
    let Some(variant_index) = layout.variants.iter().position(|variant| variant.name.as_ref() == variant_name) else {
        return Err(SemanticError::unavailable("enum_constructor"));
    };
    let variant = &layout.variants[variant_index];
    if variant.fields.len() != constructor.args.len() || variant.fields.len() > 1 {
        return Err(SemanticError::unavailable("enum_constructor"));
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
        .transpose()?;
    let variant_index = u32::try_from(variant_index).map_err(|_| SemanticError::unavailable("enum_constructor"))?;
    Ok(EnumConstructorFact { declaration, variant_index, payload })
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_constructor_for_specialized_body_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    enclosing_substitutions: std::sync::Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<EnumConstructorFact> {
    with_node(db, syntax, key, |program, index, node| {
        let constructor = node.of::<beskid_analysis::syntax::EnumConstructorExpression>()?;
        let type_path = contextual_enum_constructor_type_path(program, index, key, constructor)
            .unwrap_or(&constructor.path.node.type_path.node);
        let declaration = match resolve_type_declaration(db, key, type_path) {
            Some(declaration) => declaration,
            None => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        let layout = match specialized_enum_layout_from_type_path(db, key, type_path, &enclosing_substitutions) {
            Ok(layout) => layout,
            Err(_) => return Some(Err(SemanticError::unavailable("enum_constructor"))),
        };
        Some(enum_constructor_fact_from_layout(db, program, index, key, declaration, &layout, constructor))
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
        let contextual = contextual_enum_constructor_type_path(program, index, key, constructor)?;
        let declaration = resolve_type_declaration(db, key, contextual)?;
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
        if variant.node.fields.len() != constructor.args.len() || variant.node.fields.len() > 1 {
            return Some(Err(SemanticError::unavailable("enum_constructor_template")));
        }
        let payload = constructor.args.first().and_then(|argument| {
            index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
                .map(|node| AstNodeKey { node: normalized_expression_node(index, node), ..key })
        });
        let parameters =
            definition.generics.iter().map(|generic| Arc::<str>::from(generic.node.name.as_str())).collect::<Vec<_>>();
        Some(Ok(EnumConstructorTemplate {
            constructor: EnumConstructorFact { declaration, variant_index, payload },
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
        match enum_match_arms_from_layout(db, program, index, key, declaration, &layout, expression) {
            Ok(arms) => Some(Ok(EnumMatchFact { declaration, layout, arms: arms.into() })),
            Err(error) => Some(Err(error)),
        }
    })?
    .transpose()
}

/// Compute the match arms for a match expression given its enum declaration and layout.
pub(in crate::semantic_contract) fn enum_match_arms_from_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: AstNodeKey,
    layout: &EnumLayoutFact,
    expression: &beskid_analysis::syntax::MatchExpression,
) -> Result<Vec<EnumMatchArmFact>, SemanticError> {
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
        let (variant_index, binding) = match &arm.node.pattern.node {
            beskid_analysis::syntax::Pattern::Wildcard => (None, None),
            beskid_analysis::syntax::Pattern::Enum(pattern) => {
                if !enum_pattern_targets_declaration(db, declaration, &pattern.node.path.node.type_path.node) {
                    return Err(SemanticError::unavailable("enum_match"));
                }
                let name = pattern.node.path.node.variant.node.name.as_str();
                let Some((variant_index, variant)) =
                    layout.variants.iter().enumerate().find(|(_, variant)| variant.name.as_ref() == name)
                else {
                    return Err(SemanticError::unavailable("enum_match"));
                };
                if variant.fields.len() != pattern.node.items.len() || variant.fields.len() > 1 {
                    return Err(SemanticError::unavailable("enum_match"));
                }
                let binding = match pattern.node.items.as_slice() {
                    [] => None,
                    [item] if matches!(item.node, beskid_analysis::syntax::Pattern::Wildcard) => None,
                    [item]
                        if matches!(
                            item.node,
                            beskid_analysis::syntax::Pattern::Literal(ref literal)
                                if matches!(literal.node, beskid_analysis::syntax::Literal::Unit)
                        ) =>
                    {
                        if !matches!(variant.fields[0].1, AggregateFieldShape::Scalar(SemanticTypeId::UNIT)) {
                            return Err(SemanticError::unavailable("enum_match"));
                        }
                        None
                    }
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
                            return Err(SemanticError::unavailable("enum_match"));
                        };
                        Some(EnumMatchBindingFact {
                            declaration: AstNodeKey { node: pattern_node, ..key },
                            payload: variant.fields[0].1,
                        })
                    }
                    _ => return Err(SemanticError::unavailable("enum_match")),
                };
                let variant_index =
                    u32::try_from(variant_index).map_err(|_| SemanticError::unavailable("enum_match"))?;
                (Some(variant_index), binding)
            }
            _ => return Err(SemanticError::unavailable("enum_match")),
        };
        arms.push(EnumMatchArmFact { variant_index, body, binding });
    }
    Ok(arms)
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn enum_match_for_specialized_body_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    enclosing_substitutions: std::sync::Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<EnumMatchFact> {
    with_node(db, syntax, key, |program, index, node| {
        let expression = node.of::<beskid_analysis::syntax::MatchExpression>()?;
        let type_path = specialized_match_scrutinee_type_path(db, program, index, key, expression)?;
        let declaration = match resolve_type_declaration(db, key, type_path) {
            Some(declaration) => declaration,
            None => return Some(Err(SemanticError::unavailable("enum_match"))),
        };
        let layout = match specialized_enum_layout_from_type_path(db, key, type_path, &enclosing_substitutions) {
            Ok(layout) => layout,
            Err(_) => return Some(Err(SemanticError::unavailable("enum_match"))),
        };
        match enum_match_arms_from_layout(db, program, index, key, declaration, &layout, expression) {
            Ok(arms) => Some(Ok(EnumMatchFact { declaration, layout, arms: arms.into() })),
            Err(error) => Some(Err(error)),
        }
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
