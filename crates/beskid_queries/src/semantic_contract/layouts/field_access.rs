//! Canonical semantic layout implementation.

use super::super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn aggregate_field_access_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AggregateFieldAccess> {
    with_node(db, syntax, key, |program, index, node| {
        aggregate_field_access_for_environment(db, program, index, key, node, None, None)
    })?
    .transpose()
}

/// Resolve a field access while preserving the complete enclosing specialization.
pub fn aggregate_field_access_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> SemanticQueryResult<AggregateFieldAccess> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    let ambient = enclosing
        .substitutions
        .iter()
        .map(|binding| (binding.parameter.to_string(), AggregateFieldShape::Scalar(binding.argument)))
        .collect::<HashMap<_, _>>();
    with_node(db, syntax, key, |program, index, node| {
        aggregate_field_access_for_environment(db, program, index, key, node, Some(&ambient), Some(enclosing))
    })?
    .transpose()
}

fn aggregate_field_access_for_environment(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Option<Result<AggregateFieldAccess, SemanticError>> {
    if let Some(member) = node.of::<beskid_analysis::syntax::MemberExpression>() {
        let receiver = index.direct_child_id(
            program,
            key.node,
            beskid_analysis::syntax_query::DynNodeRef::from(member.target.as_ref()),
        )?;
        let receiver = AstNodeKey { node: normalized_expression_node(index, receiver), ..key };
        let resolved = applied_call_result_layout(db, receiver, &member.target.node, ambient, enclosing);
        return Some(resolved.and_then(|(declaration, layout)| {
            let field_name = member.member.node.name.as_str();
            let index = layout
                .fields
                .iter()
                .position(|(name, _)| name.as_ref() == field_name)
                .and_then(|index| u32::try_from(index).ok())
                .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
            Ok(AggregateFieldAccess { declaration, receiver, index, layout })
        }));
    }
    let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
    let resolved = match path.path.node.segments.as_slice() {
        [receiver, field] if receiver.node.type_args.is_empty() && field.node.type_args.is_empty() => {
            applied_local_receiver_layout(db, program, index, key, receiver.node.name.node.name.as_str(), ambient).map(
                |(declaration, receiver, layout)| (declaration, receiver, layout, field.node.name.node.name.as_str()),
            )
        }
        [field] if field.node.type_args.is_empty() => applied_method_receiver_layout(db, program, index, key, ambient)
            .map(|(declaration, receiver, layout)| (declaration, receiver, layout, field.node.name.node.name.as_str())),
        _ => return None,
    };
    Some(resolved.and_then(|(declaration, receiver, layout, field_name)| {
        let index = layout
            .fields
            .iter()
            .position(|(name, _)| name.as_ref() == field_name)
            .and_then(|index| u32::try_from(index).ok())
            .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
        Ok(AggregateFieldAccess { declaration, receiver, index, layout })
    }))
}

fn applied_call_result_layout(
    db: &dyn Db,
    receiver: AstNodeKey,
    expression: &beskid_analysis::syntax::Expression,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<(AstNodeKey, AggregateLayoutFact), SemanticError> {
    let beskid_analysis::syntax::Expression::Call(call) = expression else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    let specialization = if let Some(enclosing) = enclosing {
        generic_call_specialization_in_environment(db, receiver, enclosing)?
    } else {
        Some(generic_specialization_instance_for_call(db, receiver)?)
    }
    .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.call_specialization"))?;
    let declaration_syntax = db
        .syntax_unit(specialization.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, specialization.declaration))
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.declaration_syntax"))?;
    let declaration_node = declaration_syntax
        .syntax_index(db)
        .node_at(declaration_syntax.expanded_program(db), specialization.declaration.node)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.declaration_node"))?;
    let function = declaration_node
        .of::<beskid_analysis::syntax::FunctionDefinition>()
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.function"))?;
    let result =
        function.return_type.as_ref().ok_or_else(|| SemanticError::unavailable("aggregate_field_access.result"))?;
    let beskid_analysis::syntax::Expression::Path(callee) = &call.node.callee.node else {
        return Err(SemanticError::unavailable("aggregate_field_access.callee_path"));
    };
    let arguments = explicit_generic_type_argument_syntax(&callee.node.path.node)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.type_arguments"))?;
    if function.generics.len() != arguments.len() {
        return Err(SemanticError::unavailable("aggregate_field_access.type_argument_arity"));
    }
    let substitutions = function
        .generics
        .iter()
        .zip(arguments)
        .map(|(parameter, argument)| (parameter.node.name.as_str(), argument))
        .collect::<HashMap<_, _>>();
    let result = substitute_explicit_type(&result.node, &substitutions)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access.substituted_result"))?;
    let beskid_analysis::syntax::Type::Complex(path) = &result else {
        return Err(SemanticError::unavailable("aggregate_field_access.nominal_result"));
    };
    instantiated_aggregate_layout_for_path(db, receiver, &path.node, ambient)
}

fn applied_local_receiver_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    receiver_name: &str,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(AstNodeKey, AstNodeKey, AggregateLayoutFact), SemanticError> {
    let local = resolve_lexical_declaration(program, index, key.node, receiver_name)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let receiver = AstNodeKey { node: local, ..key };
    let parent = parent_node(index, local).ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    if index.kind(parent) == Some(beskid_analysis::syntax_query::NodeKind::Pattern) {
        return applied_pattern_binding_layout(db, program, index, key, local, ambient)
            .map(|(declaration, layout)| (declaration, receiver, layout));
    }
    let path = explicit_local_complex_type_path(program, index, local)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    instantiated_aggregate_layout_for_path(db, key, path, ambient)
        .map(|(declaration, layout)| (declaration, receiver, layout))
}

fn applied_method_receiver_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(AstNodeKey, AstNodeKey, AggregateLayoutFact), SemanticError> {
    let method =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::MethodDefinition)
            .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let declaration = parent_node(index, method).ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let definition = index
        .node_at(program, declaration)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let declaration = AstNodeKey { node: declaration, ..key };
    let substitutions = if definition.generics.is_empty() {
        None
    } else {
        ambient
            .map(|ambient| {
                definition
                    .generics
                    .iter()
                    .map(|generic| {
                        let name = generic.node.name.clone();
                        ambient
                            .get(name.as_str())
                            .copied()
                            .map(|shape| (name, shape))
                            .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))
                    })
                    .collect::<Result<HashMap<_, _>, SemanticError>>()
            })
            .transpose()?
    };
    aggregate_layout_from_definition(db, program, index, declaration, definition, substitutions.as_ref())
        .map(|layout| (declaration, AstNodeKey { node: method, ..key }, layout))
}

fn applied_pattern_binding_layout(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
    ambient: Option<&HashMap<String, AggregateFieldShape>>,
) -> Result<(AstNodeKey, AggregateLayoutFact), SemanticError> {
    let arm_id = nearest_ancestor(index, declaration, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchArm)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let match_id =
        nearest_ancestor(index, arm_id, |kind| kind == beskid_analysis::syntax_query::NodeKind::MatchExpression)
            .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let expression = index
        .node_at(program, match_id)
        .and_then(|node| node.of::<beskid_analysis::syntax::MatchExpression>())
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let arm = expression
        .arms
        .iter()
        .find(|arm| {
            index.direct_child_id(program, match_id, beskid_analysis::syntax_query::DynNodeRef::from(*arm))
                == Some(arm_id)
        })
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let beskid_analysis::syntax::Pattern::Enum(pattern) = &arm.node.pattern.node else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    let scrutinee_path = match &expression.scrutinee.node {
        beskid_analysis::syntax::Expression::Path(path) => &path.node.path.node,
        _ => return Err(SemanticError::unavailable("aggregate_field_access")),
    };
    let [scrutinee] = scrutinee_path.segments.as_slice() else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    let scrutinee_local = resolve_lexical_declaration(program, index, match_id, scrutinee.node.name.node.name.as_str())
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let scrutinee_type = explicit_local_complex_type_path(program, index, scrutinee_local)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let enum_declaration = resolve_type_declaration(db, key, scrutinee_type)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let enum_syntax = db
        .syntax_unit(enum_declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, enum_declaration))
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let enum_definition = enum_syntax
        .syntax_index(db)
        .node_at(enum_syntax.expanded_program(db), enum_declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::EnumDefinition>())
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let terminal =
        scrutinee_type.segments.last().ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    if terminal.node.type_args.len() != enum_definition.generics.len() {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    }
    let enum_substitutions = enum_definition
        .generics
        .iter()
        .zip(terminal.node.type_args.iter())
        .map(|(generic, argument)| {
            applied_aggregate_shape(db, key, &argument.node, ambient).map(|shape| (generic.node.name.clone(), shape))
        })
        .collect::<Result<HashMap<_, _>, SemanticError>>()?;
    let variant_name = pattern.node.path.node.variant.node.name.as_str();
    let variant = enum_definition
        .variants
        .iter()
        .find(|variant| variant.node.name.node.name == variant_name)
        .ok_or_else(|| SemanticError::unavailable("aggregate_field_access"))?;
    let [field] = variant.node.fields.as_slice() else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    let beskid_analysis::syntax::Type::Complex(payload) = &field.node.ty.node else {
        return Err(SemanticError::unavailable("aggregate_field_access"));
    };
    instantiated_aggregate_layout_for_path(db, enum_declaration, &payload.node, Some(&enum_substitutions))
}

fn explicit_local_complex_type_path<'a>(
    program: &'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Option<&'a beskid_analysis::syntax::Path> {
    let parent = parent_node(index, declaration)?;
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
    Some(&path.node)
}

/// Resolve the smallest generation-safe member receiver: an unqualified local with an explicit
/// nominal parameter or let annotation. Calls, inferred locals, and chained receivers remain
/// unavailable rather than reconstructing retired HIR type information.
pub(in crate::semantic_contract) fn nominal_local_receiver_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    receiver_name: &str,
) -> Option<(AstNodeKey, AstNodeKey)> {
    let local = resolve_lexical_declaration(program, index, key.node, receiver_name)?;
    let receiver = AstNodeKey { node: local, ..key };
    let parent = parent_node(index, local)?;
    if index.kind(parent) == Some(beskid_analysis::syntax_query::NodeKind::Pattern) {
        let binding = pattern_binding_fact(db, index, key, local).and_then(Result::ok)?;
        let AggregateFieldShape::Nominal(declaration) = binding.payload else {
            return None;
        };
        return Some((declaration, receiver));
    }
    let path = explicit_local_complex_type_path(program, index, local)?;
    resolve_type_declaration(db, key, path).map(|declaration| (declaration, receiver))
}
