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
    let syntax = db
        .syntax_unit(key.unit)
        .ok_or_else(|| source_expression_unavailable(db, key, "source unit is not registered"))?;
    if !syntax.accepts_key(db, key) {
        return Err(source_expression_unavailable(db, key, "node key belongs to a stale syntax generation"));
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let normalized = normalized_expression_node(index, key.node);
    let normalized_key = AstNodeKey { node: normalized, ..key };
    let node = index
        .node_at(program, normalized)
        .ok_or_else(|| source_expression_unavailable(db, normalized_key, "expression node is absent"))?;

    if node.of::<beskid_analysis::syntax::TryExpression>().is_some() {
        return try_expression_fact(db, normalized_key)?
            .map(|fact| fact.payload_identity)
            .ok_or_else(|| source_expression_unavailable(db, normalized_key, "try expression has no payload fact"));
    }

    if let Some(projection) = super::super::super::layouts::nominal_field_projection(db, normalized_key) {
        return projection.map(|(_, identity)| identity);
    }

    if node.of::<beskid_analysis::syntax::SpawnExpression>().is_some() {
        let handle = spawn_handle_type(db, normalized_key)?
            .ok_or_else(|| source_expression_unavailable(db, normalized_key, "spawn expression has no handle type"))?;
        let qualified_name = stable_declaration_identity(db, handle.declaration).ok_or_else(|| {
            source_expression_unavailable(db, normalized_key, "spawn handle declaration has no stable identity")
        })?;
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
    if node.of::<beskid_analysis::syntax::LambdaExpression>().is_some() {
        return lambda_source_identity(db, program, index, normalized_key);
    }
    if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        if let Ok(Some(access)) = aggregate_field_access(db, normalized_key) {
            return generic_source_aggregate_field_identity(db, &access);
        }
        // A named function item read as a value has the function type of its declaration.
        if let Ok(Some(value)) = function_item_value(db, normalized_key) {
            return function_item_value_identity(db, &value);
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
                    .unwrap_or_else(|| generic_source_field_shape_identity(db, normalized_key, binding.payload));
            }
        }
    }
    if let Some(array) = node.of::<beskid_analysis::syntax::ArrayLiteralExpression>() {
        return generic_source_array_literal_identity(db, program, index, normalized_key, array);
    }
    if let Some(indexed) = node.of::<beskid_analysis::syntax::IndexExpression>() {
        let target = index
            .direct_child_id(
                program,
                normalized,
                beskid_analysis::syntax_query::DynNodeRef::from(indexed.target.as_ref()),
            )
            .ok_or_else(|| source_expression_unavailable(db, normalized_key, "index target node is absent"))?;
        let subscript = index
            .direct_child_id(
                program,
                normalized,
                beskid_analysis::syntax_query::DynNodeRef::from(indexed.index.as_ref()),
            )
            .ok_or_else(|| source_expression_unavailable(db, normalized_key, "index subscript node is absent"))?;
        let subscript_key = AstNodeKey { node: subscript, ..key };
        let subscript_type = abi_type(db, subscript_key)?
            .ok_or_else(|| source_expression_unavailable(db, subscript_key, "index subscript has no ABI type"))?;
        if !primitive_integer(subscript_type) {
            return Err(source_expression_unavailable(db, subscript_key, "index subscript is not a primitive integer"));
        }
        let target = AstNodeKey { node: normalized_expression_node(index, target), ..key };
        let target_node = index
            .node_at(program, target.node)
            .ok_or_else(|| source_expression_unavailable(db, target, "index target node is absent"))?;
        let target_identity = if let Some(path) = target_node.of::<beskid_analysis::syntax::PathExpression>()
            && path.path.node.segments.len() > 1
        {
            // The terminal segment is the existing source-proven projection authority for
            // even a single field. It retains T[] substitutions and rejects private/ambiguous fields.
            let field =
                super::super::super::layouts::path_projection_segment(db, target, path.path.node.segments.len() - 1)
                    .ok_or_else(|| source_expression_unavailable(db, target, "index target projection is unproven"))?;
            super::super::super::layouts::nominal_field_projection(db, field)
                .ok_or_else(|| source_expression_unavailable(db, field, "index target field projection is unproven"))??
                .1
        } else {
            generic_source_expression_identity(db, target)?
        };
        let GenericSourceTypeIdentity::Array(element) = target_identity else {
            return Err(source_expression_unavailable(db, target, "indexed value is not an array"));
        };
        return Ok(*element);
    }
    if node.of::<beskid_analysis::syntax::CallExpression>().is_some() {
        // A primitive numeric conversion (`u8(value % 256_u64)`) is a call ISLE lowers through
        // its own authority, ahead of `call_lowering`, which knows no declaration for `u8`.
        if let Some(conversion) = primitive_numeric_conversion(db, normalized_key)? {
            return Ok(GenericSourceTypeIdentity::Abi(conversion.to));
        }
        match call_lowering(db, normalized_key)? {
            Some(CallLowering::Direct(declaration)) => {
                return generic_source_callable_result_identity(db, normalized_key, declaration);
            }
            // A call through a function value returns that function type's result.
            Some(CallLowering::FunctionValue(_)) => {
                return function_value_call(db, normalized_key)?.map(|call| call.result_identity).ok_or_else(|| {
                    source_expression_unavailable(db, normalized_key, "function value call has no signature")
                });
            }
            _ => {}
        }
    }

    let abi = abi_type(db, normalized_key)?.ok_or_else(|| {
        source_expression_unavailable(db, normalized_key, "expression kind has no source identity or ABI type")
    })?;
    if abi == SemanticTypeId::POINTER {
        return Err(source_expression_unavailable(
            db,
            normalized_key,
            "pointer-ABI expression kind has no source identity authority",
        ));
    }
    Ok(GenericSourceTypeIdentity::Abi(abi))
}

fn generic_source_aggregate_field_identity(
    db: &dyn Db,
    access: &AggregateFieldAccess,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let gap = |reason: &str| source_expression_unavailable(db, access.receiver, reason);
    let syntax = db
        .syntax_unit(access.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, access.declaration))
        .ok_or_else(|| gap("field owner declaration is stale"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), access.declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| gap("field owner is not a type definition"))?;
    let index = usize::try_from(access.index).map_err(|_| gap("field index exceeds the host range"))?;
    let field = definition.fields.get(index).ok_or_else(|| gap("field index is outside the declaration"))?;
    if generic_parameter_reference_name(&field.node.ty.node).is_none() {
        return generic_source_type_identity(db, access.declaration, &field.node.ty.node);
    }
    let (_, shape) = access.layout.fields.get(index).ok_or_else(|| gap("field index is outside the applied layout"))?;
    generic_source_field_shape_identity(db, access.receiver, *shape)
}

fn generic_source_field_shape_identity(
    db: &dyn Db,
    site: AstNodeKey,
    shape: AggregateFieldShape,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    match shape {
        AggregateFieldShape::Scalar(semantic) if semantic != SemanticTypeId::POINTER => {
            Ok(GenericSourceTypeIdentity::Abi(semantic))
        }
        AggregateFieldShape::Nominal(declaration) => {
            let qualified_name = stable_declaration_identity(db, declaration).ok_or_else(|| {
                source_expression_unavailable(db, site, "field shape declaration has no stable identity")
            })?;
            Ok(GenericSourceTypeIdentity::Nominal { qualified_name, arguments: Arc::from([]) })
        }
        AggregateFieldShape::Scalar(_) | AggregateFieldShape::ManagedReference(_) => Err(
            source_expression_unavailable(db, site, "pointer or managed-reference field shape has no source identity"),
        ),
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
        .ok_or_else(|| source_expression_unavailable(db, call, "callee declaration is stale"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .ok_or_else(|| source_expression_unavailable(db, call, "callee declaration node is absent"))?;
    if let Some(signature) = node.of::<beskid_analysis::syntax::ContractMethodSignature>() {
        if let Some(result) = signature.return_type.as_ref()
            && !result_mentions_callable_generics(db, declaration, &result.node, true)
        {
            return generic_source_type_identity(db, declaration, &result.node);
        }
        let specialization = generic_specialization_instance_for_call(db, call)?;
        let substitutions = specialization
            .substitutions
            .iter()
            .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
            .collect();
        return signature
            .return_type
            .as_ref()
            .map_or(Ok(GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT)), |result| {
                generic_source_type_identity_with_substitutions(db, declaration, &result.node, &substitutions)
            });
    }
    let result = if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        function.return_type.as_ref()
    } else if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        method.return_type.as_ref()
    } else {
        return Err(source_expression_unavailable(db, call, "callee is not a function, method, or contract signature"));
    };
    // An omitted return type is `unit`, exactly as the contract-signature branch above.
    let Some(result) = result else {
        return Ok(GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT));
    };
    if !result_mentions_callable_generics(db, declaration, &result.node, false) {
        return generic_source_type_identity(db, declaration, &result.node);
    }
    let specialization = generic_specialization_instance_for_call(db, call)?;
    let substitutions = specialization
        .substitutions
        .iter()
        .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
        .collect::<HashMap<_, _>>();
    generic_source_type_identity_with_substitutions(db, declaration, &result.node, &substitutions)
}

/// Whether a callee's declared result depends on the call's substitutions. A result that names
/// none of the callee's generic parameters (nor `This`, for a contract member) has one identity
/// at every call, so it is proven from the declaration alone. That is the only authority inside
/// an unspecialized generic body, where a call passing the enclosing generic parameter
/// (`Check<R>(reader)`) has no concrete specialization.
pub(in crate::semantic_contract) fn result_mentions_callable_generics(
    db: &dyn Db,
    declaration: AstNodeKey,
    result: &beskid_analysis::syntax::Type,
    contract_member: bool,
) -> bool {
    let generics = generic_callable_parameters(db, declaration).map(|(names, _)| names).unwrap_or_default();
    generics.iter().any(|parameter| type_syntax_mentions_generic_parameter(result, parameter))
        || (contract_member && type_syntax_mentions_this(result))
}

fn type_syntax_mentions_this(syntax_type: &beskid_analysis::syntax::Type) -> bool {
    use beskid_analysis::syntax::Type;
    match syntax_type {
        Type::This => true,
        Type::Primitive(_) => false,
        Type::Complex(path) => path
            .node
            .segments
            .iter()
            .any(|segment| segment.node.type_args.iter().any(|argument| type_syntax_mentions_this(&argument.node))),
        Type::Associated { .. } => true,
        Type::Array(element) => type_syntax_mentions_this(&element.node),
        Type::Function { return_type, parameters } => {
            type_syntax_mentions_this(&return_type.node)
                || parameters.iter().any(|parameter| type_syntax_mentions_this(&parameter.node))
        }
    }
}

/// Source identity of an array literal `[e0, e1, ...]`.
///
/// A literal written directly into a declared position (a struct-literal field, an annotated
/// `let`, or the `return` of a non-generic callable) takes that declared array type: the
/// typechecker has already proven the elements assignable to it, and the declared type is the
/// only authority for an empty literal. Otherwise a non-empty literal is `E[]` where every element
/// proves the same source identity `E`. An empty literal without a declared position, or
/// elements with differing identities, stay unavailable.
fn generic_source_array_literal_identity(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    array: &beskid_analysis::syntax::ArrayLiteralExpression,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    if let Some(declared) = array_literal_declared_identity(db, key)? {
        let GenericSourceTypeIdentity::Array(_) = declared else {
            return Err(source_expression_unavailable(db, key, "array literal is written into a non-array position"));
        };
        return Ok(declared);
    }
    let mut element_identity = None;
    for element in &array.elements {
        let element = index
            .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(element))
            .ok_or_else(|| source_expression_unavailable(db, key, "array literal element node is absent"))?;
        let element = AstNodeKey { node: normalized_expression_node(index, element), ..key };
        if index
            .node_at(program, element.node)
            .and_then(|node| node.of::<beskid_analysis::syntax::LiteralExpression>())
            .is_some_and(|literal| contextual_numeric_literal(&literal.literal.node))
        {
            // An unsuffixed numeric literal takes its width from the declared position, which
            // this literal does not have; its default width is not the array's element type.
            return Err(source_expression_unavailable(
                db,
                element,
                "unsuffixed numeric array element has no declared element type",
            ));
        }
        let identity = generic_source_expression_identity(db, element)?;
        match &element_identity {
            None => element_identity = Some(identity),
            Some(first) if *first == identity => {}
            Some(_) => {
                return Err(source_expression_unavailable(
                    db,
                    key,
                    "array literal elements have different source identities and no declared position",
                ));
            }
        }
    }
    let element = element_identity.ok_or_else(|| {
        source_expression_unavailable(db, key, "empty array literal has no declared element type at this position")
    })?;
    Ok(GenericSourceTypeIdentity::Array(Box::new(element)))
}

/// An integer or float literal without an explicit width suffix is typed by its context.
fn contextual_numeric_literal(literal: &beskid_analysis::syntax::Literal) -> bool {
    match literal {
        beskid_analysis::syntax::Literal::Integer(value) => {
            !["_i8", "_i16", "_i32", "_i64", "_u8", "_u16", "_u32", "_u64"].iter().any(|suffix| value.ends_with(suffix))
        }
        beskid_analysis::syntax::Literal::Float(value) => !value.ends_with("_f32") && !value.ends_with("_f64"),
        _ => false,
    }
}

/// The declared array type of an array literal's position, from its [`expected_type`], for the
/// direct declaration contexts only: an annotated local, a struct field and a declared return.
/// A call argument's identity feeds that call's own argument typing, so it is never taken from
/// the parameter it is checked against; it and every other position yield `None`, and a non-empty
/// literal then proves its identity from its elements.
fn array_literal_declared_identity(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<Option<GenericSourceTypeIdentity>, SemanticError> {
    match expected_type(db, key) {
        Ok(Some(expected)) => Ok(match expected.context {
            ExpectedTypeContext::DeclaredLocal { .. }
            | ExpectedTypeContext::StructField { .. }
            | ExpectedTypeContext::Return { .. } => Some(expected.identity().clone()),
            _ => None,
        }),
        Ok(None) => Ok(None),
        Err(error) if error.is_unavailable() => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod array_literal_tests;
