//! Source type identity recovered from expressions, fields, and callable results.

use super::super::super::*;
use super::*;

/// Recover an expression's exact source type shape when current syntax proves it.
/// Pointer ABI alone is deliberately rejected because native pointers, arrays, closures,
/// records, and enums all share that representation.
pub(in crate::semantic_contract) fn generic_source_expression_identity(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let syntax = db.syntax_unit(key.unit).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    if !syntax.accepts_key(db, key) {
        return Err(SemanticError::unavailable("source_expression_type"));
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let normalized = normalized_expression_node(index, key.node);
    let node =
        index.node_at(program, normalized).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let normalized_key = AstNodeKey { node: normalized, ..key };

    if node.of::<beskid_analysis::syntax::TryExpression>().is_some() {
        return try_expression_fact(db, normalized_key)?
            .map(|fact| fact.payload_identity)
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"));
    }

    if let Some(projection) = super::super::super::layouts::nominal_field_projection(db, normalized_key) {
        return projection.map(|(_, identity)| identity);
    }

    if node.of::<beskid_analysis::syntax::SpawnExpression>().is_some() {
        let handle = spawn_handle_type(db, normalized_key)?
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
        let qualified_name = stable_declaration_identity(db, handle.declaration)
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
        return Ok(GenericSourceTypeIdentity::Nominal {
            qualified_name,
            arguments: Arc::from([handle.payload.source_identity().clone()]),
        });
    }

    if let Some(literal) = node.of::<beskid_analysis::syntax::LiteralExpression>() {
        return Ok(GenericSourceTypeIdentity::Abi(semantic_type_for_literal(&literal.literal.node)));
    }
    if let Some(literal) = node.of::<beskid_analysis::syntax::StructLiteralExpression>() {
        return generic_source_path_identity(db, normalized_key, &literal.path.node);
    }
    if let Some(constructor) = node.of::<beskid_analysis::syntax::EnumConstructorExpression>() {
        let path = contextual_enum_constructor_type_path(db, program, index, normalized_key, constructor)
            .unwrap_or_else(|| constructor.path.node.type_path.node.clone());
        return generic_source_path_identity(db, normalized_key, &path);
    }
    if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        if let Ok(Some(access)) = aggregate_field_access(db, normalized_key) {
            return generic_source_aggregate_field_identity(db, &access);
        }
        if let [segment] = path.path.node.segments.as_slice()
            && segment.node.type_args.is_empty()
            && let Some(declaration) =
                resolve_lexical_declaration(program, index, normalized, segment.node.name.node.name.as_str())
        {
            if let Ok(identity) = generic_source_local_identity(db, program, index, normalized_key, declaration) {
                return Ok(identity);
            }
            if let Some(binding) = pattern_binding_fact(db, index, normalized_key, declaration) {
                let binding = binding?;
                return binding
                    .source_identity
                    .map(Ok)
                    .unwrap_or_else(|| generic_source_field_shape_identity(db, binding.payload));
            }
        }
    }
    if let Some(indexed) = node.of::<beskid_analysis::syntax::IndexExpression>() {
        let target = index
            .direct_child_id(
                program,
                normalized,
                beskid_analysis::syntax_query::DynNodeRef::from(indexed.target.as_ref()),
            )
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
        let subscript = index
            .direct_child_id(
                program,
                normalized,
                beskid_analysis::syntax_query::DynNodeRef::from(indexed.index.as_ref()),
            )
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
        let subscript_type = abi_type(db, AstNodeKey { node: subscript, ..key })?
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
        if !primitive_integer(subscript_type) {
            return Err(SemanticError::unavailable("source_expression_type"));
        }
        let target = AstNodeKey { node: normalized_expression_node(index, target), ..key };
        let target_node =
            index.node_at(program, target.node).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
        let target_identity = if let Some(path) = target_node.of::<beskid_analysis::syntax::PathExpression>()
            && path.path.node.segments.len() > 1
        {
            // The terminal segment is the existing source-proven projection authority for
            // even a single field. It retains T[] substitutions and rejects private/ambiguous fields.
            let field =
                super::super::super::layouts::path_projection_segment(db, target, path.path.node.segments.len() - 1)
                    .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            super::super::super::layouts::nominal_field_projection(db, field)
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))??
                .1
        } else {
            generic_source_expression_identity(db, target)?
        };
        let GenericSourceTypeIdentity::Array(element) = target_identity else {
            return Err(SemanticError::unavailable("source_expression_type"));
        };
        return Ok(*element);
    }
    if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
        && let Some(CallLowering::Direct(declaration)) = call_lowering(db, normalized_key)?
    {
        return generic_source_callable_result_identity(db, normalized_key, declaration);
    }

    let abi = abi_type(db, normalized_key)?.ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    if abi == SemanticTypeId::POINTER {
        return Err(SemanticError::unavailable("source_expression_type"));
    }
    Ok(GenericSourceTypeIdentity::Abi(abi))
}

fn generic_source_aggregate_field_identity(
    db: &dyn Db,
    access: &AggregateFieldAccess,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let syntax = db
        .syntax_unit(access.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, access.declaration))
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), access.declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let index = usize::try_from(access.index).map_err(|_| SemanticError::unavailable("source_expression_type"))?;
    let field = definition.fields.get(index).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    if generic_parameter_reference_name(&field.node.ty.node).is_none() {
        return generic_source_type_identity(db, access.declaration, &field.node.ty.node);
    }
    let (_, shape) =
        access.layout.fields.get(index).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    generic_source_field_shape_identity(db, *shape)
}

fn generic_source_field_shape_identity(
    db: &dyn Db,
    shape: AggregateFieldShape,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    match shape {
        AggregateFieldShape::Scalar(semantic) if semantic != SemanticTypeId::POINTER => {
            Ok(GenericSourceTypeIdentity::Abi(semantic))
        }
        AggregateFieldShape::Nominal(declaration) => {
            let qualified_name = stable_declaration_identity(db, declaration)
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            Ok(GenericSourceTypeIdentity::Nominal { qualified_name, arguments: Arc::from([]) })
        }
        AggregateFieldShape::Scalar(_) => Err(SemanticError::unavailable("source_expression_type")),
    }
}

fn generic_source_callable_result_identity(
    db: &dyn Db,
    call: AstNodeKey,
    declaration: AstNodeKey,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    if let Some(signature) = node.of::<beskid_analysis::syntax::ContractMethodSignature>() {
        return signature
            .return_type
            .as_ref()
            .map_or(Ok(GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT)), |result| {
                generic_source_type_identity(db, declaration, &result.node)
            });
    }
    let result = node
        .of::<beskid_analysis::syntax::FunctionDefinition>()
        .and_then(|function| function.return_type.as_ref())
        .or_else(|| {
            node.of::<beskid_analysis::syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref())
        })
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let specialization = generic_specialization_instance_for_call(db, call)?;
    let substitutions = specialization
        .substitutions
        .iter()
        .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
        .collect::<HashMap<_, _>>();
    generic_source_type_identity_with_substitutions(db, declaration, &result.node, &substitutions)
}
