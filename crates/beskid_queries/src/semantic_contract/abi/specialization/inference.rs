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
        return Err(SemanticError::unavailable_at("generic_specialization_instance", key));
    };
    let declaration_syntax =
        db.syntax_unit(declaration.unit).ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
    let declaration_node = declaration_syntax
        .syntax_index(db)
        .node_at(declaration_syntax.expanded_program(db), declaration.node)
        .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
    let declaration_identity = stable_declaration_identity(db, declaration)
        .ok_or_else(|| SemanticError::unavailable_at("generic_specialization_identity", key))?;
    let (parameters, return_type, generic_names, is_method, method_owner) = if let Some(function) =
        declaration_node.of::<beskid_analysis::syntax::FunctionDefinition>()
    {
        (
            function.parameters.iter().collect::<Vec<_>>(),
            function.return_type.as_ref(),
            function.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>(),
            false,
            None,
        )
    } else if let Some(method) = declaration_node.of::<beskid_analysis::syntax::ContractMethodSignature>() {
        let generic_names = generic_callable_parameters(db, declaration).map(|(names, _)| names).unwrap_or_default();
        (method.parameters.iter().collect::<Vec<_>>(), method.return_type.as_ref(), generic_names, false, None)
    } else if let Some(method) = declaration_node.of::<beskid_analysis::syntax::MethodDefinition>() {
        // The owner may live in another unit (`impl<T> List<T>` extending a Foundation type).
        let owner = method_owner_declaration(db, declaration)
            .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        let owner_syntax = db
            .syntax_unit(owner.unit)
            .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        let parent = owner_syntax
            .syntax_index(db)
            .node_at(owner_syntax.expanded_program(db), owner.node)
            .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
            .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        (
            method.parameters.iter().collect::<Vec<_>>(),
            method.return_type.as_ref(),
            generic_callable_parameters(db, declaration)
                .map(|(names, _)| names)
                .unwrap_or_else(|| parent.generics.iter().map(|generic| generic.node.name.as_str()).collect()),
            true,
            Some(owner),
        )
    } else {
        let signature = item_abi_signature(db, declaration)?
            .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        return Ok(GenericSpecializationInstance {
            declaration,
            declaration_identity,
            signature,
            substitutions: Arc::from([]),
            contract_witnesses: Arc::from([]),
        });
    };
    if generic_names.is_empty() && contract_parameter_declarations(db, declaration).is_empty() {
        let signature = item_abi_signature(db, declaration)?
            .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        return Ok(GenericSpecializationInstance {
            declaration,
            declaration_identity,
            signature,
            substitutions: Arc::from([]),
            contract_witnesses: Arc::from([]),
        });
    }

    let arguments = call_arguments(db, key)?.ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
    let argument_parameter_types = arguments
        .len()
        .checked_sub(usize::from(is_method))
        .and_then(|count| argument_parameter_types(&parameters, count))
        .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
    // Contract-typed parameters take their identity from the argument, not from inference.
    // Their witnesses (and every where-bound witness) are issued only after the substitution
    // environment is complete, so applied bounds see the exact substituted contract.
    let contract_positions = contract_parameter_declarations(db, declaration)
        .into_iter()
        .map(|(_, position, _)| position)
        .collect::<Vec<_>>();
    let mut source_substitutions = HashMap::<String, GenericSubstitution>::new();
    let mut substitutions = if let Some(owner) =
        method_owner.filter(|_| generic_callable_owner_count(db, declaration) > 0)
    {
        // An unqualified sibling call uses the same implicit receiver, not an independently
        // annotated local receiver. Reuse bindings only when that source fact proves the exact
        // enclosing method and owner; equal generic parameter names alone are not authority.
        let bindings = if let Some(enclosing) = enclosing
            && implicit_method_receiver(db, arguments[0])? == Some(enclosing.declaration)
            && enclosing.declaration.unit == owner.unit
            && parent_node(declaration_syntax.syntax_index(db), enclosing.declaration.node) == Some(owner.node)
        {
            enclosing.substitutions.clone()
        } else if let Some(bindings) =
            enclosing.and_then(|enclosing| specialized_method_receiver_bindings(db, key, arguments[0], owner, enclosing))
        {
            bindings
        } else {
            generic_nominal_method_receiver(db, key)?
                .filter(|receiver| receiver.method == declaration && receiver.owner == owner)
                .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?
                .substitutions
        };
        source_substitutions.extend(bindings.iter().cloned().map(|binding| (binding.parameter.to_string(), binding)));
        bindings.iter().map(|binding| (binding.parameter.to_string(), binding.argument)).collect()
    } else {
        HashMap::new()
    };
    let mut explicit_substitutions_complete = false;
    // Explicit type arguments on a method call bind the method's own type parameters only, all
    // of them and in declaration order; the owner's come from the receiver above.
    if is_method && let Some(arguments) = call_explicit_member_type_arguments(db, key) {
        let own = &generic_names[generic_callable_owner_count(db, declaration).min(generic_names.len())..];
        if own.len() != arguments.len() {
            return Err(SemanticError::unavailable_at("call_abi_signature", key));
        }
        let environment = enclosing
            .map(|enclosing| {
                enclosing
                    .substitutions
                    .iter()
                    .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        for (generic, argument) in own.iter().zip(arguments.iter()) {
            let source = generic_source_type_identity_with_substitutions(db, key, &argument.node, &environment)?;
            substitutions.insert((*generic).to_owned(), source.abi_type());
            source_substitutions
                .insert((*generic).to_owned(), GenericSubstitution::from_source(*generic, source.abi_type(), source));
        }
        explicit_substitutions_complete = true;
    }
    if !is_method && let Some(enclosing) = enclosing {
        let syntax =
            db.syntax_unit(key.unit).ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        let call = syntax
            .syntax_index(db)
            .node_at(syntax.expanded_program(db), key.node)
            .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
            .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        if let beskid_analysis::syntax::Expression::Path(callee) = &call.callee.node
            && let Some(arguments) = explicit_generic_type_argument_syntax(&callee.node.path.node)
        {
            if generic_names.len() != arguments.len() {
                return Err(SemanticError::unavailable_at("call_abi_signature", key));
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
        && !is_method
        && let Some(instantiation) = generic_call_instantiation(db, key)?
        && !instantiation.arguments.is_empty()
    {
        if instantiation.arguments.len() != generic_names.len() {
            return Err(SemanticError::unavailable_at("call_abi_signature", key));
        }
        let call_syntax =
            db.syntax_unit(key.unit).ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        let call_node = call_syntax
            .syntax_index(db)
            .node_at(call_syntax.expanded_program(db), key.node)
            .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
            .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        let beskid_analysis::syntax::Expression::Path(callee) = &call_node.callee.node else {
            return Err(SemanticError::unavailable_at("call_abi_signature", key));
        };
        let source_arguments = explicit_generic_type_argument_syntax(&callee.node.path.node)
            .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
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
    for (position, (parameter_type, argument)) in
        argument_parameter_types.iter().copied().zip(arguments.iter().copied().skip(usize::from(is_method))).enumerate()
    {
        if contract_positions.iter().any(|candidate| *candidate as usize == position) {
            continue;
        }
        let generic = generic_type_name(parameter_type, &generic_names);
        let bare_integer = generic.is_some() && unsuffixed_integer_literal(db, argument)?;
        let parameter_mentions_generic =
            generic_names.iter().any(|name| type_syntax_mentions_generic_parameter(parameter_type, name));
        // A parameter whose every generic is already bound (by the receiver, as in
        // `output.Send(Sequenced::Item(next, item))` on a `Port<Sequenced<T>>`) has a known expected
        // type. A proven argument identity is still checked against it; an argument whose identity
        // only that expected type proves (a contextual generic enum constructor) takes it instead.
        let parameter_already_bound = generic_names
            .iter()
            .filter(|name| type_syntax_mentions_generic_parameter(parameter_type, name))
            .all(|name| source_substitutions.contains_key(*name));
        let proven_source_identity = if parameter_mentions_generic && !bare_integer && !explicit_substitutions_complete
        {
            match specialized_source_expression_identity(db, argument, enclosing) {
                Ok(identity) => Some(identity),
                Err(error) if parameter_already_bound && error.is_unavailable() => None,
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        let infer_from_actual = proven_source_identity.is_some();
        let contextual_integer = if bare_integer {
            match generic.and_then(|name| substitutions.get(name)).copied() {
                Some(expected) if integer_literal_fits_abi(db, argument, expected)? => Some(expected),
                Some(_) => None,
                None if integer_literal_fits_abi(db, argument, SemanticTypeId::I32)? => Some(SemanticTypeId::I32),
                None => None,
            }
        } else if unsuffixed_integer_literal(db, argument)? {
            let expected = generic_abi_type(db, declaration, parameter_type, &substitutions)?;
            integer_literal_fits_abi(db, argument, expected)?.then_some(expected)
        } else {
            None
        };
        let known_expected =
            explicit_substitutions_complete || (parameter_mentions_generic && parameter_already_bound);
        let explicit_expected = known_expected
            .then(|| generic_abi_type(db, declaration, parameter_type, &substitutions))
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
        .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
        if infer_from_actual {
            let source_identity = proven_source_identity
                .as_ref()
                .ok_or_else(|| SemanticError::unavailable_at("call_abi_signature", key))?;
            infer_source_substitutions(
                db,
                declaration,
                parameter_type,
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
                Some(existing) => {
                    return Err(SemanticError::generic_binding_conflict(
                        generic,
                        existing.display_name(),
                        actual.display_name(),
                    ));
                }
            }
        } else if generic_abi_type(db, declaration, parameter_type, &substitutions)? != actual {
            return Err(SemanticError::unavailable_at("call_abi_signature", key));
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
        return Err(SemanticError::unavailable_at("call_abi_signature", key));
    }
    let substitutions = generic_names
        .iter()
        .filter_map(|parameter| {
            source_substitutions.get(*parameter).map(|binding| binding.rebind(*parameter)).or_else(|| {
                substitutions
                    .get(*parameter)
                    .copied()
                    .map(|argument| GenericSubstitution::inferred(*parameter, argument))
            })
        })
        .collect::<Vec<_>>();
    // `where T: Contract<...>` (Gap 3, task 2.3/2.7): every bound is checked against its exact
    // applied contract in the complete substitution environment before monomorphization. ISLE
    // never sees a call that fails this check, since `DirectCallee::SpecializedItem` is only
    // minted from a successful `Ok(GenericSpecializationInstance)`.
    let contract_witnesses =
        contract_witnesses_for_call(db, declaration, &arguments, is_method, enclosing, &substitutions)?;
    let abi_substitutions = substitutions
        .iter()
        .map(|binding| (binding.parameter.to_string(), binding.argument))
        .collect::<HashMap<_, _>>();
    let mut signature_parameters = parameters
        .iter()
        .enumerate()
        .map(|(position, parameter)| {
            if let Some(witness) = contract_witnesses.iter().find(|witness| witness.position as usize == position) {
                Ok(witness.source_identity.abi_type())
            } else {
                generic_abi_type(db, declaration, &parameter.node.ty.node, &abi_substitutions)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    if is_method {
        signature_parameters.insert(0, SemanticTypeId::POINTER);
    }
    let result = return_type.map_or(Ok(SemanticTypeId::UNIT), |return_type| {
        generic_abi_type(db, declaration, &return_type.node, &abi_substitutions)
    })?;
    let signature = ItemSignature { parameters: signature_parameters.into(), result };
    Ok(GenericSpecializationInstance {
        declaration,
        declaration_identity,
        signature,
        substitutions: substitutions.into(),
        contract_witnesses,
    })
}

/// Owner generic bindings of a method receiver inside a specialized body. `Bag<T> bag` in
/// `Single<T>` is typed by the enclosing parameters, so only its identity in the enclosing
/// environment can bind the owner's generics; no target-neutral annotation fact can. `None` when
/// the receiver has no nominal identity of exactly the owner type in that environment, so the
/// target-neutral receiver authority still decides.
pub(in crate::semantic_contract) fn specialized_method_receiver_bindings(
    db: &dyn Db,
    key: AstNodeKey,
    receiver: AstNodeKey,
    owner: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> Option<Arc<[GenericSubstitution]>> {
    // A path callee (`bag.Put`) carries the receiver as its root segment: the receiver argument is
    // the callee path itself, so its root local is the value whose identity binds the owner.
    let syntax = db.syntax_unit(receiver.unit).filter(|syntax| syntax.accepts_key(db, receiver))?;
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let identity = if let Some(path) = index
        .node_at(program, receiver.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
        && let [root, _] = path.path.node.segments.as_slice()
        && root.node.type_args.is_empty()
        && let Some(identifier) = resolve_lexical_declaration(program, index, receiver.node, &root.node.name.node.name)
    {
        super::super::super::contracts::specialized_local_declaration_identity(
            db,
            AstNodeKey { node: identifier, ..receiver },
            enclosing,
        )?
        .ok()?
    } else {
        specialized_source_expression_identity(db, receiver, Some(enclosing)).ok()?
    };
    let GenericSourceTypeIdentity::Nominal { arguments, .. } = &identity else { return None };
    (super::super::super::contracts::concrete_declaration(db, key, &identity) == Some(owner)).then_some(())?;
    let owner_syntax = db.syntax_unit(owner.unit).filter(|syntax| syntax.accepts_key(db, owner))?;
    let owner_definition = owner_syntax
        .syntax_index(db)
        .node_at(owner_syntax.expanded_program(db), owner.node)?
        .of::<beskid_analysis::syntax::TypeDefinition>()?;
    (owner_definition.generics.len() == arguments.len()).then_some(())?;
    Some(
        owner_definition
            .generics
            .iter()
            .zip(arguments.iter())
            .map(|(generic, argument)| {
                GenericSubstitution::from_source(generic.node.name.as_str(), argument.abi_type(), argument.clone())
            })
            .collect::<Vec<_>>()
            .into(),
    )
}

pub(in crate::semantic_contract) fn infer_source_substitutions(
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
                return Err(SemanticError::generic_binding_conflict(
                    generic,
                    existing.source_identity().display_name(),
                    actual.display_name(),
                ));
            }
            return Ok(());
        }
        if let Some(existing) = abi_substitutions.get(generic).filter(|existing| **existing != abi) {
            return Err(SemanticError::generic_binding_conflict(
                generic,
                existing.display_name(),
                actual.display_name(),
            ));
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

/// The declared type each call argument is checked against. A trailing `bulk T[]` parameter takes
/// every remaining argument, zero or more, as a `T`; the callee still receives one `T[]`.
fn argument_parameter_types<'a>(
    parameters: &[&'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>],
    argument_count: usize,
) -> Option<Vec<&'a beskid_analysis::syntax::Type>> {
    match parameters.split_last() {
        Some((last, fixed)) if last.node.bulk => {
            let beskid_analysis::syntax::Type::Array(element) = &last.node.ty.node else {
                return None;
            };
            let trailing = argument_count.checked_sub(fixed.len())?;
            Some(
                fixed
                    .iter()
                    .map(|parameter| &parameter.node.ty.node)
                    .chain(std::iter::repeat_n(&element.node, trailing))
                    .collect(),
            )
        }
        _ => (argument_count == parameters.len())
            .then(|| parameters.iter().map(|parameter| &parameter.node.ty.node).collect()),
    }
}

/// The type arguments that instantiate a generic function item read as a value.
///
/// Explicit type arguments (`Identity<i64>`) bind every generic parameter in declaration order
/// and are interpreted in the reading site's scope. Without them, the expected function type of
/// the value's position binds the generics: every declared parameter type is unified with the
/// expected parameter at its position and the declared result with the expected result, with the
/// same structural unification a generic call uses for its arguments. A generic parameter bound
/// twice to different types is a binding conflict (E1229); a generic parameter left unbound, or
/// no usable expected function type, is unavailable. `where` bounds are proven in the complete
/// environment exactly as at a call. A function with contract-typed parameters has no value
/// form in this release.
pub(in crate::semantic_contract) fn function_value_specialization(
    db: &dyn Db,
    site: AstNodeKey,
    declaration: AstNodeKey,
    function: &beskid_analysis::syntax::FunctionDefinition,
    explicit: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>],
    expected: Option<(&[GenericSourceTypeIdentity], &GenericSourceTypeIdentity)>,
) -> Result<GenericSpecializationInstance, SemanticError> {
    let unavailable = || SemanticError::unavailable_at("function_item_value", site);
    if !contract_parameter_declarations(db, declaration).is_empty() {
        return Err(unavailable());
    }
    let generic_names = function.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>();
    let mut abi_substitutions = HashMap::<String, SemanticTypeId>::new();
    let mut source_substitutions = HashMap::<String, GenericSubstitution>::new();
    if !explicit.is_empty() {
        if explicit.len() != generic_names.len() {
            return Err(unavailable());
        }
        for (generic, argument) in generic_names.iter().zip(explicit) {
            let source = generic_source_type_identity(db, site, &argument.node)?;
            abi_substitutions.insert((*generic).to_owned(), source.abi_type());
            source_substitutions
                .insert((*generic).to_owned(), GenericSubstitution::from_source(*generic, source.abi_type(), source));
        }
    } else {
        let (parameters, result) = expected.ok_or_else(unavailable)?;
        if parameters.len() != function.parameters.len() || function.parameters.iter().any(|parameter| parameter.node.bulk)
        {
            return Err(unavailable());
        }
        for (parameter, actual) in function.parameters.iter().zip(parameters) {
            infer_source_substitutions(
                db,
                declaration,
                &parameter.node.ty.node,
                actual,
                &generic_names,
                &mut abi_substitutions,
                &mut source_substitutions,
            )?;
        }
        match function.return_type.as_ref() {
            Some(return_type) => infer_source_substitutions(
                db,
                declaration,
                &return_type.node,
                result,
                &generic_names,
                &mut abi_substitutions,
                &mut source_substitutions,
            )?,
            None if *result == GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT) => {}
            None => return Err(unavailable()),
        }
    }
    let substitutions = generic_names
        .iter()
        .map(|generic| source_substitutions.get(*generic).map(|binding| binding.rebind(*generic)))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(unavailable)?;
    let mut instance = generic_specialization_instance(db, declaration, substitutions.clone().into())?
        .ok_or_else(unavailable)?;
    instance.contract_witnesses = contract_witnesses_for_call(db, declaration, &[], false, None, &substitutions)?;
    Ok(instance)
}
