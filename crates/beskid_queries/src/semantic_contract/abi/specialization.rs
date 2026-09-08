//! Focused ABI semantic implementation.

use super::super::*;

/// Materialize a call-owned specialization with its deterministic emitted declaration identity.
///
/// Keeping this conversion in the semantic layer prevents backend adapters from rebuilding
/// source identity from ABI-only facts.
pub fn generic_call_specialization_instance(
    db: &dyn Db,
    specialization: GenericCallSpecialization,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let declaration_identity = stable_declaration_identity(db, specialization.declaration)
        .ok_or_else(|| SemanticError::unavailable("generic_specialization_identity"))?;
    Ok(Some(GenericSpecializationInstance {
        declaration: specialization.declaration,
        declaration_identity,
        signature: specialization.signature,
        substitutions: specialization.substitutions,
    }))
}

/// Materialize an explicit generic call nested in an already-specialized declaration.
///
/// Source type arguments are recursively interpreted in the enclosing source environment, so
/// applications such as `Empty<MapEntry<TKey, TValue>>()` retain their complete nominal shape.
/// This is deliberately an internal semantic operation rather than a serialized template model:
/// code generation receives one proven concrete instance and never reconstructs source types.
pub fn generic_call_specialization_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(CallLowering::Direct(declaration)) = call_lowering(db, key)? else { return Ok(None) };
    let Some(call_syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    let Some(call) = call_syntax
        .syntax_index(db)
        .node_at(call_syntax.expanded_program(db), key.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
    else {
        return Ok(None);
    };
    let beskid_analysis::syntax::Expression::Path(callee) = &call.callee.node else { return Ok(None) };
    let Some(declaration_syntax) =
        db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))
    else {
        return Ok(None);
    };
    let declaration_node = declaration_syntax
        .syntax_index(db)
        .node_at(declaration_syntax.expanded_program(db), declaration.node)
        .ok_or_else(|| SemanticError::unavailable("generic_specialization_instance"))?;
    let environment = enclosing
        .substitutions
        .iter()
        .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
        .collect::<HashMap<_, _>>();
    let (parameters, return_type, substitutions, is_method) = if let Some(function) =
        declaration_node.of::<beskid_analysis::syntax::FunctionDefinition>()
    {
        let Some(arguments) = explicit_generic_type_argument_syntax(&callee.node.path.node) else {
            return Ok(None);
        };
        if function.generics.len() != arguments.len() {
            return Ok(None);
        }
        let substitutions = function
            .generics
            .iter()
            .zip(arguments)
            .map(|(parameter, argument)| {
                let identity = generic_source_type_identity_with_substitutions(db, key, &argument.node, &environment)?;
                Ok(GenericSubstitution::from_source(parameter.node.name.as_str(), identity.abi_type(), identity))
            })
            .collect::<Result<Vec<_>, SemanticError>>()?;
        (&function.parameters, function.return_type.as_ref(), substitutions, false)
    } else if let Some(method) = declaration_node.of::<beskid_analysis::syntax::MethodDefinition>() {
        let owner = parent_node(declaration_syntax.syntax_index(db), declaration.node)
            .and_then(|owner| {
                declaration_syntax.syntax_index(db).node_at(declaration_syntax.expanded_program(db), owner)
            })
            .and_then(|owner| owner.of::<beskid_analysis::syntax::TypeDefinition>())
            .ok_or_else(|| SemanticError::unavailable("generic_specialization_instance"))?;
        let substitutions = owner
            .generics
            .iter()
            .map(|parameter| {
                let identity = environment
                    .get(parameter.node.name.as_str())
                    .ok_or_else(|| SemanticError::unavailable("generic_specialization_instance"))?;
                Ok(GenericSubstitution::from_source(
                    parameter.node.name.as_str(),
                    identity.abi_type(),
                    (*identity).clone(),
                ))
            })
            .collect::<Result<Vec<_>, SemanticError>>()?;
        (&method.parameters, method.return_type.as_ref(), substitutions, true)
    } else {
        return Ok(None);
    };
    if substitutions.is_empty() {
        return Ok(None);
    }
    let abi_environment = substitutions
        .iter()
        .map(|binding| (binding.parameter.to_string(), binding.argument))
        .collect::<HashMap<_, _>>();
    let mut signature_parameters = parameters
        .iter()
        .map(|parameter| generic_abi_type(db, declaration, &parameter.node.ty.node, &abi_environment))
        .collect::<Result<Vec<_>, _>>()?;
    if is_method {
        signature_parameters.insert(0, SemanticTypeId::POINTER);
    }
    let result = return_type
        .map_or(Ok(SemanticTypeId::UNIT), |result| generic_abi_type(db, declaration, &result.node, &abi_environment))?;
    Ok(Some(GenericSpecializationInstance {
        declaration,
        declaration_identity: stable_declaration_identity(db, declaration)
            .ok_or_else(|| SemanticError::unavailable("generic_specialization_identity"))?,
        signature: ItemSignature { parameters: signature_parameters.into(), result },
        substitutions: substitutions.into(),
    }))
}

/// Derive the concrete declaration environment and ABI shape for one direct generic call.
///
/// This is shared by call facts and module worklist construction so the latter never tries to
/// reconstruct substitutions from mangled ABI types.
pub(in crate::semantic_contract) fn generic_specialization_instance_for_call(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<GenericSpecializationInstance, SemanticError> {
    let Some(CallLowering::Direct(declaration)) = call_lowering(db, key)? else {
        return Err(SemanticError::unavailable("generic_specialization_instance"));
    };
    let declaration_syntax =
        db.syntax_unit(declaration.unit).ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    let declaration_node = declaration_syntax
        .syntax_index(db)
        .node_at(declaration_syntax.expanded_program(db), declaration.node)
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    let declaration_identity = stable_declaration_identity(db, declaration)
        .ok_or_else(|| SemanticError::unavailable("generic_specialization_identity"))?;
    let (parameters, return_type, generic_names, is_method, method_owner) =
        if let Some(function) = declaration_node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            (
                function.parameters.iter().collect::<Vec<_>>(),
                function.return_type.as_ref(),
                function.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>(),
                false,
                None,
            )
        } else if let Some(method) = declaration_node.of::<beskid_analysis::syntax::MethodDefinition>() {
            let owner_node = parent_node(declaration_syntax.syntax_index(db), declaration.node)
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
            let parent = declaration_syntax
                .syntax_index(db)
                .node_at(declaration_syntax.expanded_program(db), owner_node)
                .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
            (
                method.parameters.iter().collect::<Vec<_>>(),
                method.return_type.as_ref(),
                parent.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>(),
                true,
                Some(AstNodeKey { node: owner_node, ..declaration }),
            )
        } else {
            let signature =
                item_abi_signature(db, declaration)?.ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
            return Ok(GenericSpecializationInstance {
                declaration,
                declaration_identity,
                signature,
                substitutions: Arc::from([]),
            });
        };
    if generic_names.is_empty() {
        let signature =
            item_abi_signature(db, declaration)?.ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        return Ok(GenericSpecializationInstance {
            declaration,
            declaration_identity,
            signature,
            substitutions: Arc::from([]),
        });
    }

    let arguments = call_arguments(db, key)?.ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    if arguments.len() != parameters.len() + usize::from(is_method) {
        return Err(SemanticError::unavailable("call_abi_signature"));
    }
    let mut source_substitutions = HashMap::<String, GenericSubstitution>::new();
    let mut substitutions = if let Some(owner) = method_owner {
        let receiver = generic_nominal_method_receiver(db, key)?
            .filter(|receiver| receiver.method == declaration && receiver.owner == owner)
            .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        source_substitutions
            .extend(receiver.substitutions.iter().cloned().map(|binding| (binding.parameter.to_string(), binding)));
        receiver.substitutions.iter().map(|binding| (binding.parameter.to_string(), binding.argument)).collect()
    } else {
        HashMap::new()
    };
    let mut explicit_substitutions_complete = false;
    if let Some(instantiation) = generic_call_instantiation(db, key)?
        && !instantiation.arguments.is_empty()
    {
        if instantiation.arguments.len() != generic_names.len() {
            return Err(SemanticError::unavailable("call_abi_signature"));
        }
        let call_syntax = db.syntax_unit(key.unit).ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        let call_node = call_syntax
            .syntax_index(db)
            .node_at(call_syntax.expanded_program(db), key.node)
            .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
            .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        let beskid_analysis::syntax::Expression::Path(callee) = &call_node.callee.node else {
            return Err(SemanticError::unavailable("call_abi_signature"));
        };
        let source_arguments = explicit_generic_type_argument_syntax(&callee.node.path.node)
            .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        for ((generic, argument), source_argument) in
            generic_names.iter().zip(instantiation.arguments.iter()).zip(source_arguments.iter())
        {
            substitutions.insert((*generic).to_owned(), *argument);
            source_substitutions.insert(
                (*generic).to_owned(),
                GenericSubstitution::from_source(
                    *generic,
                    *argument,
                    generic_source_type_identity(db, key, &source_argument.node)?,
                ),
            );
        }
        explicit_substitutions_complete = true;
    }
    // A bare integer starts at the language default `i32`, but carries no explicit ABI suffix.
    // Keep that distinction while inferring a generic call: a later exact argument can select
    // the binding and the bare literal can inherit it if its magnitude fits.
    let mut provisional_integer_substitutions = HashSet::new();
    for (parameter, argument) in parameters.iter().zip(arguments.iter().copied().skip(usize::from(is_method))) {
        let generic = generic_type_name(&parameter.node.ty.node, &generic_names);
        let bare_integer = generic.is_some() && unsuffixed_integer_literal(db, argument)?;
        let parameter_mentions_generic =
            generic_names.iter().any(|name| type_syntax_mentions_generic_parameter(&parameter.node.ty.node, name));
        let infer_from_actual = parameter_mentions_generic && !bare_integer && !explicit_substitutions_complete;
        let proven_source_identity =
            infer_from_actual.then(|| generic_source_expression_identity(db, argument)).transpose()?;
        let contextual_integer = if bare_integer {
            match generic.and_then(|name| substitutions.get(name)).copied() {
                Some(expected) if integer_literal_fits_abi(db, argument, expected)? => Some(expected),
                Some(_) => None,
                None if integer_literal_fits_abi(db, argument, SemanticTypeId::I32)? => Some(SemanticTypeId::I32),
                None => None,
            }
        } else if unsuffixed_integer_literal(db, argument)? {
            let expected = generic_abi_type(db, declaration, &parameter.node.ty.node, &substitutions)?;
            integer_literal_fits_abi(db, argument, expected)?.then_some(expected)
        } else {
            None
        };
        let explicit_expected = explicit_substitutions_complete
            .then(|| generic_abi_type(db, declaration, &parameter.node.ty.node, &substitutions))
            .transpose()?;
        let actual = if let Some(source) = proven_source_identity.as_ref() {
            Some(source.abi_type())
        } else if contextual_integer.is_some() {
            contextual_integer
        } else {
            match abi_type(db, argument) {
                Ok(Some(abi)) => Some(abi),
                Ok(None) => match node_type(db, argument) {
                    Ok(abi) => abi.or(explicit_expected),
                    Err(error) if error.is_unavailable() => explicit_expected,
                    Err(error) => return Err(error),
                },
                Err(error) if error.is_unavailable() => match node_type(db, argument) {
                    Ok(abi) => abi.or(explicit_expected),
                    Err(error) if error.is_unavailable() => explicit_expected,
                    Err(error) => return Err(error),
                },
                Err(error) => return Err(error),
            }
        }
        .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        if infer_from_actual {
            let source_identity =
                proven_source_identity.as_ref().ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
            infer_source_substitutions(
                db,
                declaration,
                &parameter.node.ty.node,
                source_identity,
                &generic_names,
                &mut substitutions,
                &mut source_substitutions,
            )?;
            continue;
        }
        if let Some(generic) = generic {
            match substitutions.get(generic).copied() {
                None => {
                    substitutions.insert(generic.to_owned(), actual);
                    source_substitutions.insert(generic.to_owned(), GenericSubstitution::inferred(generic, actual));
                    if bare_integer {
                        provisional_integer_substitutions.insert(generic.to_owned());
                    }
                }
                Some(existing) if existing == actual => {}
                Some(existing) if bare_integer && integer_literal_fits_abi(db, argument, existing)? => {}
                Some(_) if provisional_integer_substitutions.remove(generic) => {
                    substitutions.insert(generic.to_owned(), actual);
                    source_substitutions.insert(generic.to_owned(), GenericSubstitution::inferred(generic, actual));
                }
                Some(_) => return Err(SemanticError::unavailable("call_abi_signature")),
            }
        } else if generic_abi_type(db, declaration, &parameter.node.ty.node, &substitutions)? != actual {
            return Err(SemanticError::unavailable("call_abi_signature"));
        }
    }
    if generic_names.iter().any(|generic| {
        !substitutions.contains_key(*generic)
            && !parameters
                .iter()
                .map(|parameter| &parameter.node.ty.node)
                .chain(return_type.iter().map(|return_type| &return_type.node))
                .any(|syntax_type| type_syntax_mentions_generic_parameter(syntax_type, generic))
    }) {
        return Err(SemanticError::unavailable("call_abi_signature"));
    }
    let mut signature_parameters = parameters
        .iter()
        .map(|parameter| generic_abi_type(db, declaration, &parameter.node.ty.node, &substitutions))
        .collect::<Result<Vec<_>, _>>()?;
    if is_method {
        signature_parameters.insert(0, SemanticTypeId::POINTER);
    }
    let result = return_type.map_or(Ok(SemanticTypeId::UNIT), |return_type| {
        generic_abi_type(db, declaration, &return_type.node, &substitutions)
    })?;
    let signature = ItemSignature { parameters: signature_parameters.into(), result };
    let substitutions = generic_names
        .into_iter()
        .filter_map(|parameter| {
            source_substitutions.get(parameter).map(|binding| binding.rebind(parameter)).or_else(|| {
                substitutions.get(parameter).copied().map(|argument| GenericSubstitution::inferred(parameter, argument))
            })
        })
        .collect::<Vec<_>>();
    Ok(GenericSpecializationInstance {
        declaration,
        declaration_identity,
        signature,
        substitutions: substitutions.into(),
    })
}

fn infer_source_substitutions(
    db: &dyn Db,
    declaration: AstNodeKey,
    parameter: &beskid_analysis::syntax::Type,
    actual: &GenericSourceTypeIdentity,
    generic_names: &[&str],
    abi_substitutions: &mut HashMap<String, SemanticTypeId>,
    source_substitutions: &mut HashMap<String, GenericSubstitution>,
) -> Result<(), SemanticError> {
    use beskid_analysis::syntax::Type;

    if let Some(generic) = generic_parameter_reference_name(parameter).filter(|name| generic_names.contains(name)) {
        let abi = actual.abi_type();
        if let Some(existing) = source_substitutions.get(generic) {
            if existing.argument != abi || existing.source_identity() != actual {
                return Err(SemanticError::unavailable("call_abi_signature"));
            }
            return Ok(());
        }
        if abi_substitutions.get(generic).is_some_and(|existing| *existing != abi) {
            return Err(SemanticError::unavailable("call_abi_signature"));
        }
        abi_substitutions.insert(generic.to_owned(), abi);
        source_substitutions.insert(generic.to_owned(), GenericSubstitution::from_source(generic, abi, actual.clone()));
        return Ok(());
    }

    match (parameter, actual) {
        (Type::Complex(path), GenericSourceTypeIdentity::Nominal { qualified_name, arguments }) => {
            let expected_declaration = resolve_type_declaration(db, declaration, &path.node)
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
            let expected_name = stable_declaration_identity(db, expected_declaration)
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
            if expected_name != *qualified_name {
                return Err(SemanticError::unavailable("call_abi_signature"));
            }
            let expected_arguments =
                path.node.segments.iter().flat_map(|segment| segment.node.type_args.iter()).collect::<Vec<_>>();
            if expected_arguments.len() != arguments.len() {
                return Err(SemanticError::unavailable("call_abi_signature"));
            }
            for (expected, actual) in expected_arguments.into_iter().zip(arguments.iter()) {
                infer_source_substitutions(
                    db,
                    declaration,
                    &expected.node,
                    actual,
                    generic_names,
                    abi_substitutions,
                    source_substitutions,
                )?;
            }
            Ok(())
        }
        (Type::Array(expected), GenericSourceTypeIdentity::Array(actual)) => infer_source_substitutions(
            db,
            declaration,
            &expected.node,
            actual,
            generic_names,
            abi_substitutions,
            source_substitutions,
        ),
        (
            Type::Function { parameters: expected_parameters, return_type: expected_result },
            GenericSourceTypeIdentity::Function { parameters: actual_parameters, result: actual_result },
        ) if expected_parameters.len() == actual_parameters.len() => {
            for (expected, actual) in expected_parameters.iter().zip(actual_parameters.iter()) {
                infer_source_substitutions(
                    db,
                    declaration,
                    &expected.node,
                    actual,
                    generic_names,
                    abi_substitutions,
                    source_substitutions,
                )?;
            }
            infer_source_substitutions(
                db,
                declaration,
                &expected_result.node,
                actual_result,
                generic_names,
                abi_substitutions,
                source_substitutions,
            )
        }
        _ => {
            let expected = generic_source_type_identity(db, declaration, parameter)?;
            (expected == *actual).then_some(()).ok_or_else(|| SemanticError::unavailable("call_abi_signature"))
        }
    }
}

/// Whether a source expression is a bare integer literal without an ABI suffix.
///
/// This follows only singleton syntax wrappers, so compound arithmetic, casts, calls, and arrays
/// never inherit an ABI representation from a surrounding call.
pub(in crate::semantic_contract) fn unsuffixed_integer_literal(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<bool, SemanticError> {
    Ok(integer_literal_text(db, key)?.is_some())
}

/// Prove that one bare integer literal fits the ABI representation selected elsewhere in its
/// generic call. Explicitly suffixed literals never reach this helper.
pub(in crate::semantic_contract) fn integer_literal_fits_abi(
    db: &dyn Db,
    key: AstNodeKey,
    expected: SemanticTypeId,
) -> Result<bool, SemanticError> {
    let Some(text) = integer_literal_text(db, key)? else {
        return Ok(false);
    };
    if let Some(magnitude) = text.strip_prefix('-').and_then(integer_literal_u64) {
        return Ok(match expected {
            SemanticTypeId::I32 => magnitude <= (i32::MAX as u64) + 1,
            SemanticTypeId::I64 => magnitude <= (i64::MAX as u64) + 1,
            SemanticTypeId::U32 | SemanticTypeId::U8 | SemanticTypeId::WORD => false,
            _ => false,
        });
    }
    let value = integer_literal_u64(&text);
    Ok(match expected {
        SemanticTypeId::I32 => value.is_some_and(|value| i32::try_from(value).is_ok()),
        SemanticTypeId::I64 => value.is_some_and(|value| i64::try_from(value).is_ok()),
        SemanticTypeId::U32 => value.is_some_and(|value| u32::try_from(value).is_ok()),
        SemanticTypeId::U8 => value.is_some_and(|value| u8::try_from(value).is_ok()),
        SemanticTypeId::WORD => value.is_some(),
        _ => false,
    })
}

fn contextual_integer_operand_fits_abi(
    db: &dyn Db,
    key: AstNodeKey,
    expected: SemanticTypeId,
) -> Result<bool, SemanticError> {
    if integer_literal_text(db, key)?.is_some() {
        return integer_literal_fits_abi(db, key, expected);
    }
    let Some(value) = contextual_constant_integer(db, key)? else {
        return Ok(false);
    };
    Ok(match expected {
        SemanticTypeId::I32 => i32::try_from(value).is_ok(),
        SemanticTypeId::I64 => true,
        SemanticTypeId::U32 => u32::try_from(value).is_ok(),
        SemanticTypeId::U8 => u8::try_from(value).is_ok(),
        SemanticTypeId::WORD => u64::try_from(value).is_ok(),
        _ => false,
    })
}

pub(in crate::semantic_contract) fn integer_literal_text(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<Option<Arc<str>>, SemanticError> {
    let Some(literal) = literal_fact(db, key)? else {
        let Some(children) = child_nodes(db, key)? else {
            return Ok(None);
        };
        if node_kind(db, key)? == Some(IndexedNodeKind::UnaryExpression)
            && operator_fact(db, key)? == Some(OperatorFact::Neg)
        {
            let [_, operand] = children.as_ref() else {
                return Ok(None);
            };
            return Ok(integer_literal_text(db, *operand)?.map(|text| Arc::from(format!("-{text}"))));
        }
        let [child] = children.as_ref() else {
            return Ok(None);
        };
        return integer_literal_text(db, *child);
    };
    match literal {
        LiteralFact::Integer(text) if !integer_has_explicit_abi_suffix(&text) => Ok(Some(text)),
        _ => Ok(None),
    }
}

pub(in crate::semantic_contract) fn contextual_constant_integer(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<Option<i64>, SemanticError> {
    if let Some(value) = constant_integer(db, key)? {
        return Ok(Some(value));
    }
    let Some(children) = child_nodes(db, key)? else {
        return Ok(None);
    };
    let [child] = children.as_ref() else {
        return Ok(None);
    };
    contextual_constant_integer(db, *child)
}

pub(in crate::semantic_contract) fn integer_has_explicit_abi_suffix(text: &str) -> bool {
    matches!(text.rsplit_once('_').map(|(_, suffix)| suffix), Some("i32" | "i64" | "u32" | "u8"))
}

/// Parse a source integer's magnitude while preserving a hexadecimal word-sized bit pattern.
///
/// The caller is responsible for ABI suffix handling. Negative values are deliberately excluded:
/// this helper is used only for selecting and checking unsigned ABI representations.
pub(in crate::semantic_contract) fn integer_literal_u64(text: &str) -> Option<u64> {
    match text.strip_prefix("0x") {
        Some(digits) => u64::from_str_radix(&digits.replace('_', ""), 16).ok(),
        None => text.replace('_', "").parse::<u64>().ok(),
    }
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn call_argument_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |program, index, _node| {
        Some((|| {
            if integer_literal_text(db, key)?.is_none() {
                return Err(SemanticError::unavailable("call_argument_abi_type"));
            }
            let mut current = key.node;
            while let Some(parent) = index.metadata_for(key.generation, current).and_then(|meta| meta.parent) {
                let parent_key = AstNodeKey { node: parent, ..key };
                if index.kind(parent) == Some(beskid_analysis::syntax_query::NodeKind::CallExpression) {
                    if index
                        .node_at(program, parent)
                        .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
                        .and_then(primitive_numeric_conversion_target)
                        .is_some()
                    {
                        return Err(SemanticError::unavailable("call_argument_abi_type"));
                    }
                    let arguments = call_arguments(db, parent_key)?
                        .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"))?;
                    let argument_index = arguments.iter().position(|argument| {
                        is_transparent_call_integer_path(index, key.generation, argument.node, key.node)
                    });
                    let Some(argument_index) = argument_index else {
                        current = parent;
                        continue;
                    };
                    let signature = call_abi_signature(db, parent_key)?
                        .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"))?;
                    let expected = signature
                        .parameters
                        .get(argument_index)
                        .copied()
                        .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"));
                    return expected.and_then(|expected| {
                        (primitive_integer(expected) && integer_literal_fits_abi(db, key, expected)?)
                            .then_some(expected)
                            .ok_or_else(|| SemanticError::unavailable("call_argument_abi_type"))
                    });
                }
                current = parent;
            }
            Err(SemanticError::unavailable("call_argument_abi_type"))
        })())
    })?
    .transpose()
}

/// Whether a proven integer literal reaches its call-argument boundary through singleton syntax
/// wrappers only. Unary negation is admitted because [`integer_literal_text`] has already proved
/// the whole subtree is one literal; binary arithmetic and nested calls remain non-transparent.
fn is_transparent_call_integer_path(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    generation: SyntaxGenerationId,
    argument: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> bool {
    use beskid_analysis::syntax_query::NodeKind;

    let mut current = node;
    while current != argument {
        let Some(parent) = index.metadata_for(generation, current).and_then(|meta| meta.parent) else {
            return false;
        };
        if !matches!(
            index.kind(parent),
            Some(
                NodeKind::Expression
                    | NodeKind::LiteralExpression
                    | NodeKind::GroupedExpression
                    | NodeKind::UnaryExpression
            )
        ) {
            return false;
        }
        current = parent;
    }
    true
}

/// Contextual ABI for an unsuffixed integer literal used directly as one operand of a
/// homogeneous primitive-integer binary expression. This is representation selection, not
/// widening: the sibling must already prove the exact ABI type.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn binary_operand_abi_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    with_node(db, syntax, key, |_program, index, _node| {
        Some((|| {
            if integer_literal_text(db, key)?.is_none() && contextual_constant_integer(db, key)?.is_none() {
                return Err(SemanticError::unavailable("binary_operand_abi_type"));
            }
            let mut parent = index.metadata_for(key.generation, key.node).and_then(|meta| meta.parent);
            while parent
                .is_some_and(|node| index.kind(node) != Some(beskid_analysis::syntax_query::NodeKind::BinaryExpression))
            {
                parent = parent.and_then(|node| index.metadata_for(key.generation, node).and_then(|meta| meta.parent));
            }
            let parent = parent.ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            let mut branch = key.node;
            while index.metadata_for(key.generation, branch).and_then(|meta| meta.parent) != Some(parent) {
                branch = index
                    .metadata_for(key.generation, branch)
                    .and_then(|meta| meta.parent)
                    .ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            }
            if !is_transparent_binary_operand_path(index, branch, key.node) {
                return Err(SemanticError::unavailable("binary_operand_abi_type"));
            }
            let children =
                index.children(parent).ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            let sibling = children
                .iter()
                .copied()
                .filter(|child| *child != branch)
                .find(|child| index.kind(*child) != Some(beskid_analysis::syntax_query::NodeKind::BinaryOp))
                .ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            let expected = abi_type(db, AstNodeKey { node: sibling, ..key })?
                .ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))?;
            (primitive_integer(expected) && contextual_integer_operand_fits_abi(db, key, expected)?)
                .then_some(expected)
                .ok_or_else(|| SemanticError::unavailable("binary_operand_abi_type"))
        })())
    })?
    .transpose()
}

/// ABI signature for a soft runtime builtin reached as [`CallLowering::Dynamic`].
///
/// Only names with a dispatch route receive a signature; this never grants Corelib-service or
/// canonical-runtime intrinsic authority.
pub(in crate::semantic_contract) fn dispatch_builtin_abi_signature(
    db: &dyn Db,
    key: AstNodeKey,
) -> Option<ItemSignature> {
    let symbol = dispatch_builtin_symbol(db, key).ok().flatten()?;
    let (_, spec) = beskid_analysis::builtins::builtin_specs()
        .iter()
        .enumerate()
        .find(|(_, candidate)| candidate.runtime_symbol == symbol.0)?;
    let parameters = spec.params.iter().copied().map(builtin_type_to_semantic).collect::<Option<Vec<_>>>()?;
    let result = builtin_type_to_semantic(spec.returns)?;
    Some(ItemSignature { parameters: parameters.into(), result })
}

pub(in crate::semantic_contract) fn builtin_type_to_semantic(
    ty: beskid_analysis::builtins::BuiltinType,
) -> Option<SemanticTypeId> {
    use beskid_analysis::builtins::BuiltinType;
    Some(match ty {
        BuiltinType::String => SemanticTypeId::STRING,
        BuiltinType::Ptr => SemanticTypeId::POINTER,
        BuiltinType::Usize => SemanticTypeId::WORD,
        BuiltinType::U64 => SemanticTypeId::I64,
        BuiltinType::U32 => SemanticTypeId::U32,
        BuiltinType::I32 => SemanticTypeId::I32,
        BuiltinType::F64 => SemanticTypeId::F64,
        BuiltinType::Unit => SemanticTypeId::UNIT,
        BuiltinType::Never => SemanticTypeId::NEVER,
    })
}
/// ABI facts for compiler-embedded Corelib service facades. These are deliberately available
/// only after [`CallLowering::CorelibService`] has proved the current source corpus; user source
/// that merely spells one of these names remains unauthorized and receives no import signature.
pub(in crate::semantic_contract) fn corelib_service_abi_signature(service: CorelibService) -> Option<ItemSignature> {
    let adapted = match service.name {
        "__panic_str" => Some((vec![SemanticTypeId::STRING], SemanticTypeId::NEVER)),
        "__str_from_bytes_utf8" => Some((vec![SemanticTypeId::POINTER], SemanticTypeId::STRING)),
        "__syscall_write" => Some((vec![SemanticTypeId::I64, SemanticTypeId::STRING], SemanticTypeId::I64)),
        "__syscall_write_bytes" => Some((vec![SemanticTypeId::I64, SemanticTypeId::POINTER], SemanticTypeId::I64)),
        _ => None,
    };
    if let Some((parameters, result)) = adapted {
        return Some(ItemSignature { parameters: parameters.into(), result });
    }
    let abi = beskid_abi::runtime_source::canonical_corelib_service_abi(service)?;
    let parameters = abi.parameters.into_iter().map(corelib_service_abi_type).collect::<Option<Vec<_>>>()?;
    let result = corelib_service_abi_type(abi.result)?;
    Some(ItemSignature { parameters: parameters.into(), result })
}

fn corelib_service_abi_type(ty: beskid_abi::runtime_source::CorelibServiceAbiType) -> Option<SemanticTypeId> {
    use beskid_abi::runtime_source::CorelibServiceAbiType;
    Some(match ty {
        CorelibServiceAbiType::Pointer => SemanticTypeId::POINTER,
        CorelibServiceAbiType::String => SemanticTypeId::STRING,
        CorelibServiceAbiType::Usize => SemanticTypeId::WORD,
        CorelibServiceAbiType::I64 => SemanticTypeId::I64,
        CorelibServiceAbiType::I32 => SemanticTypeId::I32,
        CorelibServiceAbiType::U32 => SemanticTypeId::U32,
        CorelibServiceAbiType::U8 => SemanticTypeId::U8,
        CorelibServiceAbiType::F64 => SemanticTypeId::F64,
        CorelibServiceAbiType::Void => SemanticTypeId::UNIT,
        CorelibServiceAbiType::Never => SemanticTypeId::NEVER,
    })
}

pub(in crate::semantic_contract) fn generic_abi_type(
    db: &dyn Db,
    declaration: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
    substitutions: &HashMap<String, SemanticTypeId>,
) -> Result<SemanticTypeId, SemanticError> {
    if let beskid_analysis::syntax::Type::Complex(path) = syntax_type
        && path.node.segments.iter().any(|segment| {
            segment.node.type_args.iter().any(|argument| {
                type_syntax_mentions_generic_parameter(&argument.node, "T")
                    || substitutions.keys().any(|name| type_syntax_mentions_generic_parameter(&argument.node, name))
            })
        })
    {
        return Ok(SemanticTypeId::POINTER);
    }
    let generic = match syntax_type {
        beskid_analysis::syntax::Type::Complex(path) => {
            let [segment] = path.node.segments.as_slice() else {
                return abi_type_from_syntax(db, declaration, syntax_type);
            };
            segment.node.type_args.is_empty().then_some(segment.node.name.node.name.as_str())
        }
        _ => None,
    };
    generic
        .and_then(|name| substitutions.get(name).copied())
        .map(Ok)
        .unwrap_or_else(|| abi_type_from_syntax(db, declaration, syntax_type))
}

pub(in crate::semantic_contract) fn generic_type_name<'a>(
    syntax_type: &'a beskid_analysis::syntax::Type,
    generics: &[&str],
) -> Option<&'a str> {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return None;
    };
    let [segment] = path.node.segments.as_slice() else {
        return None;
    };
    let name = segment.node.name.node.name.as_str();
    segment.node.type_args.is_empty().then_some(name).filter(|name| generics.contains(name))
}

pub(in crate::semantic_contract) fn type_syntax_mentions_generic_parameter(
    syntax_type: &beskid_analysis::syntax::Type,
    parameter: &str,
) -> bool {
    match syntax_type {
        beskid_analysis::syntax::Type::Primitive(_) => false,
        beskid_analysis::syntax::Type::Complex(path) => path.node.segments.iter().any(|segment| {
            segment.node.name.node.name == parameter
                || segment
                    .node
                    .type_args
                    .iter()
                    .any(|argument| type_syntax_mentions_generic_parameter(&argument.node, parameter))
        }),
        beskid_analysis::syntax::Type::Associated { contract, .. } => contract.node.segments.iter().any(|segment| {
            segment.node.name.node.name == parameter
                || segment
                    .node
                    .type_args
                    .iter()
                    .any(|argument| type_syntax_mentions_generic_parameter(&argument.node, parameter))
        }),
        beskid_analysis::syntax::Type::Array(element) => {
            type_syntax_mentions_generic_parameter(&element.node, parameter)
        }
        beskid_analysis::syntax::Type::Function { return_type, parameters } => {
            type_syntax_mentions_generic_parameter(&return_type.node, parameter)
                || parameters
                    .iter()
                    .any(|parameter_type| type_syntax_mentions_generic_parameter(&parameter_type.node, parameter))
        }
    }
}

pub(in crate::semantic_contract) fn generic_parameter_reference_name(
    syntax_type: &beskid_analysis::syntax::Type,
) -> Option<&str> {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return None;
    };
    let [segment] = path.node.segments.as_slice() else {
        return None;
    };
    segment.node.type_args.is_empty().then_some(segment.node.name.node.name.as_str())
}

/// Materialize a generic declaration with an already-proven immutable environment.
///
/// The caller must obtain `substitutions` from a source call fact or an enclosing instance;
/// this function checks arity and derives the ABI directly from the declaration syntax.
pub fn generic_specialization_instance(
    db: &dyn Db,
    declaration: AstNodeKey,
    substitutions: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(syntax) = db.syntax_unit(declaration.unit) else { return Ok(None) };
    if !syntax.accepts_key(db, declaration) {
        return Ok(None);
    }
    let Some(function) = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
    else {
        return Ok(None);
    };
    if substitutions.len() > function.generics.len() {
        return Ok(None);
    }
    let mut environment = HashMap::with_capacity(substitutions.len());
    for binding in substitutions.iter() {
        if !function.generics.iter().any(|generic| generic.node.name.as_str() == binding.parameter.as_ref())
            || environment.insert(binding.parameter.to_string(), binding.argument).is_some()
        {
            return Ok(None);
        }
    }
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| generic_abi_type(db, declaration, &parameter.node.ty.node, &environment))
        .collect::<Result<Vec<_>, _>>()?;
    let result = function.return_type.as_ref().map_or(Ok(SemanticTypeId::UNIT), |return_type| {
        generic_abi_type(db, declaration, &return_type.node, &environment)
    })?;
    Ok(Some(GenericSpecializationInstance {
        declaration,
        declaration_identity: stable_declaration_identity(db, declaration)
            .ok_or_else(|| SemanticError::unavailable("generic_specialization_identity"))?,
        signature: ItemSignature { parameters: parameters.into(), result },
        substitutions,
    }))
}

pub(in crate::semantic_contract) fn abi_signature_from_syntax(
    db: &dyn Db,
    key: AstNodeKey,
    parameters: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>],
    return_type: Option<&beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>>,
) -> Result<ItemSignature, SemanticError> {
    let parameters = parameters
        .iter()
        .map(|parameter| item_abi_type_from_syntax(db, key, &parameter.node.ty.node))
        .collect::<Result<Vec<_>, _>>()?;
    let result = return_type
        .map_or(Ok(SemanticTypeId::UNIT), |return_type| item_abi_type_from_syntax(db, key, &return_type.node))?;
    Ok(ItemSignature { parameters: parameters.into(), result })
}

/// Resolve one declaration ABI type without broadening ordinary source lookup.
///
/// A fully qualified nominal envelope can appear in a public signature without a matching `use`
/// declaration (`Console.ConsoleSize` or
/// `Core.Results.Result<i64, Core.Syscall.SyscallError>`). Its outer declaration is nevertheless
/// exact assembly authority, and ABI v5 passes every nominal aggregate by pointer regardless of
/// its payload arguments. Keep this fallback item-local: general type resolution, completion,
/// enum facts, and ABI-varying bare generics remain closed.
pub(in crate::semantic_contract) fn item_abi_type_from_syntax(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<SemanticTypeId, SemanticError> {
    abi_type_from_syntax(db, key, syntax_type)
        .or_else(|error| exact_assembled_nominal_envelope(db, key, syntax_type).ok_or(error))
}

pub(in crate::semantic_contract) fn exact_assembled_nominal_envelope(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Option<SemanticTypeId> {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return None;
    };
    let (nominal, module_path) = path.node.segments.split_last()?;
    if module_path.is_empty() || module_path.iter().any(|segment| !segment.node.type_args.is_empty()) {
        return None;
    }
    let module_path = module_path.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
    let target = {
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        let [target] = registry.modules.get(&(key.generation, module_path))?.as_slice() else {
            return None;
        };
        *target
    };
    unique_exported_type_in_unit(
        db,
        target,
        key.generation,
        &nominal.node.name.node.name,
        nominal.node.type_args.len(),
    )?;
    Some(SemanticTypeId::POINTER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_authorized_corelib_service_has_an_exact_source_level_abi_signature() {
        let target = beskid_abi::abi_v5::TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
            .expect("linux target");
        let manifest = beskid_abi::abi_v5::AbiManifestV5::canonical_runtime(target);
        let capability = beskid_abi::runtime_source::canonical_corelib_service_capability(&manifest)
            .expect("Corelib service capability");

        for service in capability.services() {
            let signature = corelib_service_abi_signature(*service).unwrap_or_else(|| {
                panic!(
                    "authorized service {} in {} has no source-level ABI signature",
                    service.name, service.source_path
                )
            });
            if service.name == "__str_from_bytes_utf8" {
                assert_eq!(
                    signature.result,
                    SemanticTypeId::STRING,
                    "the native pointer result must retain the source-level string type"
                );
            }
        }
    }
}
