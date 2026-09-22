//! Compile-time conformance witnesses. No value representation or runtime dispatch lives here.

use super::*;
use beskid_analysis::syntax::{
    ContractDefinition, ContractMethodSignature, ContractNode, Expression, FunctionDefinition, MethodDefinition,
    Parameter, Path, PathSegment, Spanned, Type, TypeDefinition, Visibility,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind};

/// Resolve a contract in the same lexical/import namespaces as its source annotation.
/// Keeping this separate from aggregate lookup prevents contracts acquiring a nominal ABI.
fn resolve_contract(db: &dyn Db, key: AstNodeKey, path: &Path) -> Option<AstNodeKey> {
    let (terminal, prefix) = path.segments.split_last()?;
    if !terminal.node.type_args.is_empty() {
        return None;
    }
    let name = terminal.node.name.node.name.as_str();
    let mut units = Vec::new();
    if prefix.is_empty() {
        let syntax = db.syntax_unit(key.unit)?;
        let index = syntax.syntax_index(db);
        let mut scope = module_scope(index, key.node);
        while let Some(current) = scope {
            let matches = index
                .ids_of_kind(NodeKind::ContractDefinition)
                .filter(|node| {
                    module_scope(index, *node) == Some(current)
                        && index
                            .node_at(syntax.expanded_program(db), *node)
                            .and_then(|node| node.of::<ContractDefinition>())
                            .is_some_and(|contract| contract.name.node.name == name)
                })
                .collect::<Vec<_>>();
            if !matches.is_empty() {
                return match matches.as_slice() {
                    [node] => Some(AstNodeKey { node: *node, ..key }),
                    _ => None,
                };
            }
            scope = outer_module_scope(index, current);
        }
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        units.extend(
            registry.imports.get(&(key.unit, key.generation)).into_iter().flatten().map(|import| import.target),
        );
    } else {
        let modules = prefix.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
        units.extend(resolve_qualified_module_unit(db, key, &modules));
        let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
        units.extend(registry.modules.get(&(key.generation, modules.clone())).into_iter().flatten().copied());
        let mut full = modules;
        full.push(name.to_owned());
        units.extend(registry.modules.get(&(key.generation, full)).into_iter().flatten().copied());
    }
    let mut visited = HashSet::new();
    let mut candidates = Vec::new();
    while let Some(unit) = units.pop() {
        if !visited.insert(unit) {
            continue;
        }
        let syntax = db.syntax_unit(unit)?;
        if syntax.generation(db) != key.generation {
            return None;
        }
        let index = syntax.syntax_index(db);
        for node in index.ids_of_kind(NodeKind::ContractDefinition) {
            let contract = index.node_at(syntax.expanded_program(db), node)?.of::<ContractDefinition>()?;
            if contract.name.node.name == name
                && contract.visibility.node == Visibility::Public
                && module_scope(index, node).is_some_and(|scope| index.kind(scope) == Some(NodeKind::Program))
            {
                candidates.push(AstNodeKey { unit, node, ..key });
            }
        }
        units.extend(public_reexport_units(db, unit, key.generation));
    }
    match candidates.as_slice() {
        [candidate] => Some(*candidate),
        _ => None,
    }
}

pub(super) fn contract_parameter_declarations(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Vec<(AstNodeKey, u32, AstNodeKey)> {
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return Vec::new();
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(node) = index.node_at(program, declaration.node) else {
        return Vec::new();
    };
    let parameters = if let Some(function) = node.of::<FunctionDefinition>() {
        &function.parameters
    } else if let Some(method) = node.of::<MethodDefinition>() {
        &method.parameters
    } else {
        return Vec::new();
    };
    parameters
        .iter()
        .enumerate()
        .filter_map(|(position, parameter)| {
            let node = index.direct_child_id(program, declaration.node, DynNodeRef::from(parameter))?;
            let parameter_key = AstNodeKey { node, ..declaration };
            // Lexical generic bindings shadow outer contracts, including owner generics on
            // methods. Classify the declared parameter before consulting contract namespaces.
            if type_syntax_is_enclosing_generic_parameter_reference(db, parameter_key, &parameter.node.ty.node) {
                return None;
            }
            let Type::Complex(path) = &parameter.node.ty.node else {
                return None;
            };
            let contract = resolve_contract(db, declaration, &path.node)?;
            Some((parameter_key, u32::try_from(position).ok()?, contract))
        })
        .collect()
}

fn contract_methods(
    db: &dyn Db,
    contract: AstNodeKey,
    active: &mut HashSet<AstNodeKey>,
) -> Result<Vec<AstNodeKey>, SemanticError> {
    if !active.insert(contract) {
        return Err(SemanticError::new("cyclic contract embedding"));
    }
    let syntax = db
        .syntax_unit(contract.unit)
        .filter(|syntax| syntax.accepts_key(db, contract))
        .ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let definition = index
        .node_at(program, contract.node)
        .and_then(|node| node.of::<ContractDefinition>())
        .ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
    let mut methods = Vec::new();
    for item in &definition.items {
        match &item.node {
            ContractNode::MethodSignature(signature) => {
                let node = index
                    .ids_of_kind(NodeKind::ContractMethodSignature)
                    .find(|node| {
                        index
                            .node_at(program, *node)
                            .and_then(|node| node.of::<ContractMethodSignature>())
                            .is_some_and(|candidate| std::ptr::eq(candidate, &signature.node))
                    })
                    .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
                methods.push(AstNodeKey { node, ..contract });
            }
            ContractNode::Embedding(embedding) => {
                let path = Path {
                    segments: vec![Spanned::new(
                        PathSegment { name: embedding.node.name.clone(), type_args: Vec::new() },
                        embedding.span,
                    )],
                };
                let embedded = resolve_contract(db, contract, &path).ok_or_else(|| {
                    SemanticError::new(format!("unknown embedded contract {}", embedding.node.name.node.name))
                })?;
                methods.extend(contract_methods(db, embedded, active)?);
            }
        }
    }
    active.remove(&contract);
    let mut seen = HashSet::new();
    methods.retain(|method| seen.insert(*method));
    Ok(methods)
}

pub(super) fn contract_member_receiver(
    db: &dyn Db,
    program: &Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &Path,
) -> Option<Result<(AstNodeKey, AstNodeKey), SemanticError>> {
    let [receiver, member] = path.segments.as_slice() else {
        return None;
    };
    if !receiver.node.type_args.is_empty() || !member.node.type_args.is_empty() {
        return None;
    }
    let identifier = resolve_lexical_declaration(program, index, key.node, &receiver.node.name.node.name)?;
    let parameter_node = parent_node(index, identifier)?;
    let parameter = index.node_at(program, parameter_node)?.of::<Parameter>()?;
    if type_syntax_is_enclosing_generic_parameter_reference(
        db,
        AstNodeKey { node: parameter_node, ..key },
        &parameter.ty.node,
    ) {
        return None;
    }
    let Type::Complex(annotation) = &parameter.ty.node else {
        return None;
    };
    let contract = resolve_contract(db, key, &annotation.node)?;
    let result = contract_methods(db, contract, &mut HashSet::new()).and_then(|methods| {
        let candidates = methods
            .into_iter()
            .filter(|method| {
                let Some(syntax) = db.syntax_unit(method.unit) else {
                    return false;
                };
                syntax
                    .syntax_index(db)
                    .node_at(syntax.expanded_program(db), method.node)
                    .and_then(|node| node.of::<ContractMethodSignature>())
                    .is_some_and(|signature| signature.name.node.name == member.node.name.node.name)
            })
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [method] => Ok((*method, AstNodeKey { node: identifier, ..key })),
            _ => Err(SemanticError::new(format!("contract member is not visible: {}", member.node.name.node.name))),
        }
    });
    Some(result)
}

/// Source interpretation in an immutable enclosing specialization. Contract witnesses are
/// looked up by parameter identity; generic names keep their existing source substitution map.
pub(super) fn specialized_source_expression_identity(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let Some(enclosing) = enclosing else {
        return generic_source_expression_identity(db, key);
    };
    let syntax = db
        .syntax_unit(key.unit)
        .filter(|syntax| syntax.accepts_key(db, key))
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let key = AstNodeKey { node: normalized_expression_node(index, key.node), ..key };
    let node = index.node_at(program, key.node).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let environment =
        enclosing.substitutions.iter().map(|binding| (binding.parameter.as_ref(), binding.source_identity())).collect();
    if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>()
        && let [segment] = path.path.node.segments.as_slice()
        && let Some(identifier) = resolve_lexical_declaration(program, index, key.node, &segment.node.name.node.name)
        && let Some(parent) = parent_node(index, identifier)
    {
        let declaration = AstNodeKey { node: parent, ..key };
        if let Some(witness) = enclosing.contract_witnesses.iter().find(|witness| witness.parameter == declaration) {
            return Ok(witness.source_identity.clone());
        }
        if let Some(parameter) = index.node_at(program, parent).and_then(|node| node.of::<Parameter>()) {
            return generic_source_type_identity_with_substitutions(db, declaration, &parameter.ty.node, &environment);
        }
        if let Some(local) =
            index.node_at(program, parent).and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())
        {
            if let Some(annotation) = &local.type_annotation {
                return generic_source_type_identity_with_substitutions(
                    db,
                    declaration,
                    &annotation.node,
                    &environment,
                );
            }
            let initializer = index
                .direct_child_id(program, parent, DynNodeRef::from(&local.value))
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            return specialized_source_expression_identity(
                db,
                AstNodeKey { node: initializer, ..key },
                Some(enclosing),
            );
        }
    }
    if let Some(literal) = node.of::<beskid_analysis::syntax::StructLiteralExpression>() {
        return generic_source_type_identity_with_substitutions(
            db,
            key,
            &Type::Complex(literal.path.clone()),
            &environment,
        );
    }
    if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
        && let Some(result) = specialized_corelib_value_service_result(db, key, enclosing)?
    {
        return Ok(result.source_identity().clone());
    }
    if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
        && let Some(instance) = generic_call_specialization_in_environment(db, key, enclosing)?
    {
        let target = db
            .syntax_unit(instance.declaration.unit)
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
        let target_node = target
            .syntax_index(db)
            .node_at(target.expanded_program(db), instance.declaration.node)
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
        let result = target_node
            .of::<FunctionDefinition>()
            .and_then(|item| item.return_type.as_ref())
            .or_else(|| target_node.of::<MethodDefinition>().and_then(|item| item.return_type.as_ref()));
        let Some(result) = result else {
            return Ok(GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT));
        };
        let substitutions = instance
            .substitutions
            .iter()
            .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
            .collect();
        return generic_source_type_identity_with_substitutions(db, instance.declaration, &result.node, &substitutions);
    }
    generic_source_expression_identity(db, key)
}

pub(super) fn concrete_declaration(
    db: &dyn Db,
    key: AstNodeKey,
    source: &GenericSourceTypeIdentity,
) -> Option<AstNodeKey> {
    let GenericSourceTypeIdentity::Nominal { qualified_name, arguments } = source else {
        return None;
    };
    let mut units = HashSet::from([key.unit]);
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    units.extend(
        registry
            .modules
            .iter()
            .filter(|((generation, _), _)| *generation == key.generation)
            .flat_map(|(_, units)| units.iter().copied()),
    );
    drop(registry);
    let mut candidates = Vec::new();
    for unit in units {
        let syntax = db.syntax_unit(unit)?;
        if syntax.generation(db) != key.generation {
            continue;
        }
        for node in syntax.syntax_index(db).ids_of_kind(NodeKind::TypeDefinition) {
            let declaration = AstNodeKey { unit, node, ..key };
            let definition =
                syntax.syntax_index(db).node_at(syntax.expanded_program(db), node)?.of::<TypeDefinition>()?;
            if definition.generics.len() == arguments.len()
                && stable_declaration_identity(db, declaration).as_ref() == Some(qualified_name)
            {
                candidates.push(declaration);
            }
        }
    }
    match candidates.as_slice() {
        [candidate] => Some(*candidate),
        _ => None,
    }
}

fn contract_includes(
    db: &dyn Db,
    candidate: AstNodeKey,
    required: AstNodeKey,
    active: &mut HashSet<AstNodeKey>,
) -> bool {
    if candidate == required {
        return true;
    }
    if !active.insert(candidate) {
        return false;
    }
    let Some(syntax) = db.syntax_unit(candidate.unit) else {
        return false;
    };
    let Some(definition) = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), candidate.node)
        .and_then(|node| node.of::<ContractDefinition>())
    else {
        return false;
    };
    definition.items.iter().any(|item| {
        let ContractNode::Embedding(embedding) = &item.node else {
            return false;
        };
        let path = Path {
            segments: vec![Spanned::new(
                PathSegment { name: embedding.node.name.clone(), type_args: Vec::new() },
                embedding.span,
            )],
        };
        resolve_contract(db, candidate, &path).is_some_and(|embedded| contract_includes(db, embedded, required, active))
    })
}

pub(super) fn contract_witnesses_for_call(
    db: &dyn Db,
    declaration: AstNodeKey,
    arguments: &[AstNodeKey],
    is_method: bool,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<Arc<[ContractParameterWitness]>, SemanticError> {
    contract_parameter_declarations(db, declaration)
        .into_iter()
        .map(|(parameter, position, contract)| {
            let argument = *arguments
                .get(position as usize + usize::from(is_method))
                .ok_or_else(|| SemanticError::unavailable("contract_argument"))?;
            let source_identity = specialized_source_expression_identity(db, argument, enclosing)?;
            let concrete = concrete_declaration(db, argument, &source_identity)
                .ok_or_else(|| SemanticError::new("contract argument has no concrete conforming type"))?;
            let syntax =
                db.syntax_unit(concrete.unit).ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
            let definition = syntax
                .syntax_index(db)
                .node_at(syntax.expanded_program(db), concrete.node)
                .and_then(|node| node.of::<TypeDefinition>())
                .ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
            if !definition
                .conformances
                .iter()
                .filter_map(|path| resolve_contract(db, concrete, &path.node))
                .any(|candidate| contract_includes(db, candidate, contract, &mut HashSet::new()))
            {
                return Err(SemanticError::new(format!(
                    "missing contract conformance for {}",
                    definition.name.node.name
                )));
            }
            let GenericSourceTypeIdentity::Nominal { arguments, .. } = &source_identity else { unreachable!() };
            let substitutions = definition
                .generics
                .iter()
                .zip(arguments.iter())
                .map(|(parameter, argument)| (parameter.node.name.as_str(), argument))
                .collect::<HashMap<_, _>>();
            let mut methods = Vec::new();
            for method in contract_methods(db, contract, &mut HashSet::new())? {
                let expected_syntax =
                    db.syntax_unit(method.unit).ok_or_else(|| SemanticError::unavailable("contract_method"))?;
                let expected = expected_syntax
                    .syntax_index(db)
                    .node_at(expected_syntax.expanded_program(db), method.node)
                    .and_then(|node| node.of::<ContractMethodSignature>())
                    .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
                let implementation = unique_nominal_method_declaration(db, concrete, &expected.name.node.name)
                    .ok_or_else(|| {
                        SemanticError::new(format!(
                            "missing contract method implementation: {}",
                            expected.name.node.name
                        ))
                    })?;
                let actual = syntax
                    .syntax_index(db)
                    .node_at(syntax.expanded_program(db), implementation.node)
                    .and_then(|node| node.of::<MethodDefinition>())
                    .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
                if actual.visibility.node != Visibility::Public {
                    return Err(SemanticError::new(format!(
                        "contract method implementation is private: {}",
                        expected.name.node.name
                    )));
                }
                let expected_types = expected
                    .parameters
                    .iter()
                    .map(|parameter| &parameter.node.ty.node)
                    .chain(expected.return_type.iter().map(|result| &result.node))
                    .map(|ty| generic_source_type_identity(db, method, ty))
                    .collect::<Result<Vec<_>, _>>()?;
                let actual_types = actual
                    .parameters
                    .iter()
                    .map(|parameter| &parameter.node.ty.node)
                    .chain(actual.return_type.iter().map(|result| &result.node))
                    .map(|ty| generic_source_type_identity_with_substitutions(db, implementation, ty, &substitutions))
                    .collect::<Result<Vec<_>, _>>()?;
                if expected.parameters.len() != actual.parameters.len() || expected_types != actual_types {
                    return Err(SemanticError::new(format!(
                        "contract implementation signature mismatch: {}",
                        expected.name.node.name
                    )));
                }
                methods.push((method, implementation));
            }
            Ok(ContractParameterWitness {
                parameter,
                position,
                contract,
                concrete,
                source_identity,
                methods: methods.into(),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Arc::from)
}

pub(super) fn contract_method_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: &GenericSpecializationInstance,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(call) =
        index.node_at(program, key.node).and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
    else {
        return Ok(None);
    };
    let Expression::Path(path) = &call.callee.node else {
        return Ok(None);
    };
    let Some(receiver) = contract_member_receiver(db, program, index, key, &path.node.path.node) else {
        return Ok(None);
    };
    let (method, receiver) = receiver?;
    let parameter = AstNodeKey {
        node: parent_node(index, receiver.node).ok_or_else(|| SemanticError::unavailable("contract_parameter"))?,
        ..receiver
    };
    let witness = enclosing
        .contract_witnesses
        .iter()
        .find(|witness| witness.parameter == parameter)
        .ok_or_else(|| SemanticError::unavailable("contract_witness"))?;
    let declaration = witness
        .methods
        .iter()
        .find_map(|(signature, implementation)| (*signature == method).then_some(*implementation))
        .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
    let target = db.syntax_unit(declaration.unit).ok_or_else(|| SemanticError::unavailable("contract_method"))?;
    let definition = target
        .syntax_index(db)
        .node_at(target.expanded_program(db), witness.concrete.node)
        .and_then(|node| node.of::<TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
    let GenericSourceTypeIdentity::Nominal { arguments, .. } = &witness.source_identity else {
        return Err(SemanticError::unavailable("contract_witness"));
    };
    let substitutions = definition
        .generics
        .iter()
        .zip(arguments.iter())
        .map(|(parameter, argument)| {
            GenericSubstitution::from_source(parameter.node.name.as_str(), argument.abi_type(), argument.clone())
        })
        .collect::<Vec<_>>();
    let environment = substitutions.iter().map(|binding| (binding.parameter.to_string(), binding.argument)).collect();
    let implementation = target
        .syntax_index(db)
        .node_at(target.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<MethodDefinition>())
        .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
    let mut parameters = vec![SemanticTypeId::POINTER];
    parameters.extend(
        implementation
            .parameters
            .iter()
            .map(|parameter| generic_abi_type(db, declaration, &parameter.node.ty.node, &environment))
            .collect::<Result<Vec<_>, _>>()?,
    );
    let result = implementation
        .return_type
        .as_ref()
        .map_or(Ok(SemanticTypeId::UNIT), |ty| generic_abi_type(db, declaration, &ty.node, &environment))?;
    Ok(Some(GenericSpecializationInstance {
        declaration,
        declaration_identity: stable_declaration_identity(db, declaration)
            .ok_or_else(|| SemanticError::unavailable("contract_method"))?,
        signature: ItemSignature { parameters: parameters.into(), result },
        substitutions: substitutions.into(),
        contract_witnesses: Arc::from([]),
    }))
}
