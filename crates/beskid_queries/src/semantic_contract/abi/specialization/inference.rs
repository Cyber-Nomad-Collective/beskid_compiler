//! Call specialization in a source environment and source substitution inference.

use super::super::super::layouts::unique_assembled_type_in_module;
use super::super::super::*;
use super::*;

pub(super) fn specialization_for_call_in_environment(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
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
            let owner_node = method_owner_node(
                declaration_syntax.expanded_program(db),
                declaration_syntax.syntax_index(db),
                declaration.node,
            )
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
                contract_witnesses: Arc::from([]),
            });
        };
    if generic_names.is_empty() && contract_parameter_declarations(db, declaration).is_empty() {
        let signature =
            item_abi_signature(db, declaration)?.ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        return Ok(GenericSpecializationInstance {
            declaration,
            declaration_identity,
            signature,
            substitutions: Arc::from([]),
            contract_witnesses: Arc::from([]),
        });
    }

    let arguments = call_arguments(db, key)?.ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
    if arguments.len() != parameters.len() + usize::from(is_method) {
        return Err(SemanticError::unavailable("call_abi_signature"));
    }
    let contract_witnesses = contract_witnesses_for_call(db, declaration, &arguments, is_method, enclosing)?;
    let mut source_substitutions = HashMap::<String, GenericSubstitution>::new();
    let mut substitutions = if let Some(owner) = method_owner.filter(|_| !generic_names.is_empty()) {
        // An unqualified sibling call uses the same implicit receiver, not an independently
        // annotated local receiver. Reuse bindings only when that source fact proves the exact
        // enclosing method and owner; equal generic parameter names alone are not authority.
        let bindings = if let Some(enclosing) = enclosing
            && implicit_method_receiver(db, arguments[0])? == Some(enclosing.declaration)
            && enclosing.declaration.unit == owner.unit
            && parent_node(declaration_syntax.syntax_index(db), enclosing.declaration.node) == Some(owner.node)
        {
            enclosing.substitutions.clone()
        } else {
            generic_nominal_method_receiver(db, key)?
                .filter(|receiver| receiver.method == declaration && receiver.owner == owner)
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?
                .substitutions
        };
        source_substitutions.extend(bindings.iter().cloned().map(|binding| (binding.parameter.to_string(), binding)));
        bindings.iter().map(|binding| (binding.parameter.to_string(), binding.argument)).collect()
    } else {
        HashMap::new()
    };
    let mut explicit_substitutions_complete = false;
    if let Some(enclosing) = enclosing {
        let syntax = db.syntax_unit(key.unit).ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        let call = syntax
            .syntax_index(db)
            .node_at(syntax.expanded_program(db), key.node)
            .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
            .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
        if let beskid_analysis::syntax::Expression::Path(callee) = &call.callee.node
            && let Some(arguments) = explicit_generic_type_argument_syntax(&callee.node.path.node)
        {
            if generic_names.len() != arguments.len() {
                return Err(SemanticError::unavailable("call_abi_signature"));
            }
            let environment = enclosing
                .substitutions
                .iter()
                .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
                .collect();
            for (generic, argument) in generic_names.iter().zip(arguments) {
                let source = generic_source_type_identity_with_substitutions(db, key, &argument.node, &environment)?;
                substitutions.insert((*generic).to_owned(), source.abi_type());
                source_substitutions.insert(
                    (*generic).to_owned(),
                    GenericSubstitution::from_source(*generic, source.abi_type(), source),
                );
            }
            explicit_substitutions_complete = true;
        }
    }
    if !explicit_substitutions_complete
        && let Some(instantiation) = generic_call_instantiation(db, key)?
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
    for (position, (parameter, argument)) in
        parameters.iter().zip(arguments.iter().copied().skip(usize::from(is_method))).enumerate()
    {
        if contract_witnesses.iter().any(|witness| witness.position as usize == position) {
            continue;
        }
        let generic = generic_type_name(&parameter.node.ty.node, &generic_names);
        let bare_integer = generic.is_some() && unsuffixed_integer_literal(db, argument)?;
        let parameter_mentions_generic =
            generic_names.iter().any(|name| type_syntax_mentions_generic_parameter(&parameter.node.ty.node, name));
        let infer_from_actual = parameter_mentions_generic && !bare_integer && !explicit_substitutions_complete;
        let proven_source_identity =
            infer_from_actual.then(|| specialized_source_expression_identity(db, argument, enclosing)).transpose()?;
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
        } else if enclosing.is_some()
            && let Ok(source) = specialized_source_expression_identity(db, argument, enclosing)
        {
            Some(source.abi_type())
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
    // `where T: Contract` (Gap 3, task 2.3/2.7): reject the call before monomorphization if an
    // inferred/explicit generic argument does not conform to its bound. ISLE never sees a call
    // that fails this check, since `DirectCallee::SpecializedItem` is only minted from a
    // successful `Ok(GenericSpecializationInstance)`.
    if let Some(function) = declaration_node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        for bound in &function.where_bounds {
            let Some(contract) = contracts::resolve_contract(db, declaration, &bound.contract.node) else {
                return Err(SemanticError::new(format!(
                    "unknown contract `{}` in where clause",
                    bound.contract.node.segments.last().map(|s| s.node.name.node.name.as_str()).unwrap_or("?")
                )));
            };
            let Some(argument_type_id) = substitutions.get(bound.parameter.node.name.as_str()).copied() else {
                continue;
            };
            let source_identity = source_substitutions
                .get(bound.parameter.node.name.as_str())
                .map(|binding| binding.source_identity().clone())
                .unwrap_or(GenericSourceTypeIdentity::Abi(argument_type_id));
            let Some(concrete) = contracts::concrete_declaration(db, declaration, &source_identity) else {
                return Err(SemanticError::new(format!(
                    "generic bound not satisfied: `{}` has no concrete conforming type for `where {}: {}`",
                    bound.parameter.node.name,
                    bound.parameter.node.name,
                    bound.contract.node.segments.last().map(|s| s.node.name.node.name.as_str()).unwrap_or("?")
                )));
            };
            let concrete_syntax =
                db.syntax_unit(concrete.unit).ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
            let concrete_definition = concrete_syntax
                .syntax_index(db)
                .node_at(concrete_syntax.expanded_program(db), concrete.node)
                .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"))?;
            if !contracts::type_declaration_conforms_to_contract(db, concrete, concrete_definition, contract) {
                return Err(SemanticError::new(format!(
                    "generic bound not satisfied: `{}` does not conform to `{}`",
                    concrete_definition.name.node.name,
                    bound.contract.node.segments.last().map(|s| s.node.name.node.name.as_str()).unwrap_or("?")
                )));
            }
        }
    }
    let mut signature_parameters = parameters
        .iter()
        .enumerate()
        .map(|(position, parameter)| {
            if let Some(witness) = contract_witnesses.iter().find(|witness| witness.position as usize == position) {
                Ok(witness.source_identity.abi_type())
            } else {
                generic_abi_type(db, declaration, &parameter.node.ty.node, &substitutions)
            }
        })
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
        contract_witnesses,
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
