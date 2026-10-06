//! Compile-time conformance witnesses. No value representation or runtime dispatch lives here.

use super::*;
use beskid_analysis::syntax::{
    ContractDefinition, ContractMethodSignature, ContractNode, EnumDefinition, Expression, FunctionDefinition,
    ImplBlock, MethodDefinition, Parameter, Path, PathSegment, Spanned, Type, TypeDefinition, Visibility,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind};

/// An applied conformance issued from a current registered source declaration.
/// Neither argument ABI IDs nor a contract leaf name can construct this capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedContractIdentity {
    owner: AstNodeKey,
    declaration: AstNodeKey,
    arguments: Arc<[GenericSourceTypeIdentity]>,
}
impl AppliedContractIdentity {
    pub fn declaration(&self) -> AstNodeKey {
        self.declaration
    }
    pub fn argument_count(&self) -> usize {
        self.arguments.len()
    }
    pub(crate) fn arguments(&self) -> &[GenericSourceTypeIdentity] {
        &self.arguments
    }
}

/// Compare a private applied argument with an exact current concrete declaration.
/// A caller cannot replace this nominal check with the argument's native ABI ID.
pub fn applied_contract_argument_is_type(
    db: &dyn Db,
    application: &AppliedContractIdentity,
    ordinal: usize,
    declaration: AstNodeKey,
) -> Result<bool, SemanticError> {
    let owner = db
        .syntax_unit(application.owner.unit)
        .filter(|syntax| syntax.accepts_key(db, application.owner))
        .ok_or_else(|| SemanticError::new("applied contract belongs to a stale owner"))?;
    let target = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration) && syntax.project(db) == owner.project(db))
        .ok_or_else(|| SemanticError::new("applied argument target is stale or foreign"))?;
    if type_applied_contract_implementation_registered(db, owner, application.owner, application)?.is_none() {
        return Err(SemanticError::new("applied contract has no current implementation witness"));
    }
    let node = target
        .syntax_index(db)
        .node_at(target.expanded_program(db), declaration.node)
        .ok_or_else(|| SemanticError::new("applied argument target is not a nominal type"))?;
    let generic_count = node
        .of::<TypeDefinition>()
        .map(|definition| definition.generics.len())
        .or_else(|| node.of::<EnumDefinition>().map(|definition| definition.generics.len()))
        .ok_or_else(|| SemanticError::new("applied argument target is not a nominal type"))?;
    if generic_count != 0 {
        return Err(SemanticError::new("applied argument requires a concrete target instantiation"));
    }
    let identity = stable_declaration_identity(db, declaration)
        .ok_or_else(|| SemanticError::unavailable("applied_argument.declaration_identity"))?;
    let argument = application
        .arguments
        .get(ordinal)
        .ok_or_else(|| SemanticError::new("applied argument ordinal is outside the contract"))?;
    Ok(matches!(argument, GenericSourceTypeIdentity::Nominal { qualified_name, arguments }
        if *qualified_name == identity && arguments.is_empty()))
}

fn application_from_path(
    db: &dyn Db,
    owner: AstNodeKey,
    context: AstNodeKey,
    path: &Path,
) -> Result<AppliedContractIdentity, SemanticError> {
    application_from_path_with_arguments(db, owner, context, path, &HashMap::new())
}

fn application_from_path_with_arguments(
    db: &dyn Db,
    owner: AstNodeKey,
    context: AstNodeKey,
    path: &Path,
    substitutions: &HashMap<&str, &GenericSourceTypeIdentity>,
) -> Result<AppliedContractIdentity, SemanticError> {
    let terminal = path.segments.last().ok_or_else(|| SemanticError::unavailable("contract_application.empty"))?;
    let mut declaration_path = path.clone();
    declaration_path.segments.last_mut().unwrap().node.type_args.clear();
    let declaration = resolve_contract(db, context, &declaration_path)
        .ok_or_else(|| SemanticError::unavailable("contract_application.declaration"))?;
    let syntax =
        db.syntax_unit(declaration.unit).ok_or_else(|| SemanticError::unavailable("contract_application.owner"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<ContractDefinition>())
        .ok_or_else(|| SemanticError::unavailable("contract_application.kind"))?;
    if definition.generics.len() != terminal.node.type_args.len() {
        return Err(SemanticError::new("applied contract argument count differs from its declaration"));
    }
    let arguments = terminal
        .node
        .type_args
        .iter()
        .map(|argument| generic_source_type_identity_with_substitutions(db, context, &argument.node, substitutions))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AppliedContractIdentity { owner, declaration, arguments: arguments.into() })
}

pub(super) fn type_contract_applications_registered(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AppliedContractIdentity]>> {
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(node) = index.node_at(program, key.node) else {
        return Ok(None);
    };
    let mut applications = Vec::new();
    if let Some(definition) = node.of::<TypeDefinition>() {
        for path in &definition.conformances {
            applications.push(application_from_path(db, key, key, &path.node)?);
        }
    } else if node.of::<EnumDefinition>().is_none() {
        return Ok(None);
    }
    for node in index.ids_of_kind(NodeKind::ImplBlock) {
        let implementation = AstNodeKey { node, ..key };
        let block = index
            .node_at(program, node)
            .and_then(|node| node.of::<ImplBlock>())
            .ok_or_else(|| SemanticError::unavailable("contract_application.impl"))?;
        let Type::Complex(receiver) = &block.receiver_type.node else {
            continue;
        };
        if layouts::resolve_type_declaration(db, implementation, &receiver.node) != Some(key) {
            continue;
        }
        for path in &block.conformances {
            applications.push(application_from_path(db, key, implementation, &path.node)?);
        }
    }
    let mut unique = Vec::new();
    for application in applications {
        if !unique.contains(&application) {
            unique.push(application);
        }
    }
    Ok(Some(unique.into()))
}

pub(super) fn type_applied_contract_implementation_registered(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    concrete: AstNodeKey,
    application: &AppliedContractIdentity,
) -> SemanticQueryResult<Arc<[(AstNodeKey, AstNodeKey)]>> {
    if !syntax.accepts_key(db, concrete) {
        return Ok(None);
    }
    if application.owner != concrete {
        return Err(SemanticError::new("applied contract was issued to another source owner"));
    }
    let current = type_contract_applications_registered(db, syntax, concrete)?
        .ok_or_else(|| SemanticError::unavailable("contract_application.owner"))?;
    if !current.contains(application) {
        return Err(SemanticError::new("applied contract no longer belongs to this source generation"));
    }
    let contract_syntax = db
        .syntax_unit(application.declaration.unit)
        .filter(|owner| owner.accepts_key(db, application.declaration) && owner.project(db) == syntax.project(db))
        .ok_or_else(|| SemanticError::unavailable("contract_application.project"))?;
    let contract = contract_syntax
        .syntax_index(db)
        .node_at(contract_syntax.expanded_program(db), application.declaration.node)
        .and_then(|node| node.of::<ContractDefinition>())
        .ok_or_else(|| SemanticError::unavailable("contract_application.kind"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), concrete.node)
        .ok_or_else(|| SemanticError::unavailable("contract_application.owner"))?;
    let generic_count = node
        .of::<TypeDefinition>()
        .map(|definition| definition.generics.len())
        .or_else(|| node.of::<EnumDefinition>().map(|definition| definition.generics.len()))
        .ok_or_else(|| SemanticError::unavailable("contract_application.owner"))?;
    if generic_count != 0 {
        return Err(SemanticError::unavailable("contract_application.unapplied_receiver"));
    }
    let expected = contract
        .generics
        .iter()
        .zip(application.arguments.iter())
        .map(|(parameter, argument)| (parameter.node.name.as_str(), argument))
        .collect::<HashMap<_, _>>();
    validated_contract_methods_with_expected(db, concrete, application.declaration, &HashMap::new(), &expected)
        .map(Some)
}

#[salsa::tracked(persist)]
pub(super) fn type_contract_implementation_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    concrete: AstNodeKey,
    contract: AstNodeKey,
) -> SemanticQueryResult<Arc<[(AstNodeKey, AstNodeKey)]>> {
    if !syntax.accepts_key(db, concrete) || concrete.generation != contract.generation {
        return Ok(None);
    }
    let Some(contract_syntax) = db
        .syntax_unit(contract.unit)
        .filter(|owner| owner.accepts_key(db, contract) && owner.project(db) == syntax.project(db))
    else {
        return Ok(None);
    };
    if contract_syntax.syntax_index(db).kind(contract.node) != Some(NodeKind::ContractDefinition) {
        return Ok(None);
    }
    let Some(definition) = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), concrete.node)
        .and_then(|node| node.of::<TypeDefinition>())
    else {
        return Ok(None);
    };
    if !definition.generics.is_empty() {
        return Err(SemanticError::unavailable("type_contract.unapplied_generic"));
    }
    if !type_declaration_conforms_to_contract(db, concrete, definition, contract) {
        return Err(SemanticError::new("type does not conform to the requested contract"));
    }
    validated_contract_methods(db, concrete, contract, &HashMap::new()).map(Some)
}

/// Exact declared conformance identities for native adapter discovery. This is
/// declaration discovery, not proof that a particular method implements the contract.
#[salsa::tracked(persist)]
pub(super) fn type_contract_declarations_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    Ok(type_contract_applications_registered(db, syntax, key)?.map(|applications| {
        let mut declarations = Vec::new();
        for application in applications.iter() {
            if !declarations.contains(&application.declaration) {
                declarations.push(application.declaration);
            }
        }
        declarations.into()
    }))
}

/// Resolve a contract in the same lexical/import namespaces as its source annotation.
/// Keeping this separate from aggregate lookup prevents contracts acquiring a nominal ABI.
pub(in crate::semantic_contract) fn resolve_contract(db: &dyn Db, key: AstNodeKey, path: &Path) -> Option<AstNodeKey> {
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
        units.extend(registry.visible_module_units(key.unit, key.generation, &modules).into_iter().flatten().copied());
        let mut full = modules;
        full.push(name.to_owned());
        units.extend(registry.visible_module_units(key.unit, key.generation, &full).into_iter().flatten().copied());
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
            // Associated-type declarations are not methods and carry no dispatchable node.
            ContractNode::AssociatedType(_) => {}
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
    // A receiver typed by a bounded lexical generic (`T value` with `where T: Contract<E>`)
    // keeps that generic: its members are exactly the bound contracts' members.
    if generic_parameter_reference_name(&parameter.ty.node).is_some()
        && let Some(callable) = parent_node(index, parameter_node)
    {
        let parameter_key = AstNodeKey { node: parameter_node, ..key };
        let bounds = match declared_where_bounds(db, AstNodeKey { node: callable, ..key }) {
            Ok(bounds) => bounds,
            Err(error) => return Some(Err(error)),
        };
        let bounds = bounds
            .into_iter()
            .filter(|bound| bound.parameters.iter().any(|(candidate, _)| *candidate == parameter_key))
            .collect::<Vec<_>>();
        if !bounds.is_empty() {
            return Some(
                bounded_member_method(db, &bounds, &member.node.name.node.name)
                    .map(|method| (method, AstNodeKey { node: identifier, ..key })),
            );
        }
    }
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

/// The unique member named `member` among the contracts bounding one generic receiver.
fn bounded_member_method(
    db: &dyn Db,
    bounds: &[DeclaredWhereBound],
    member: &str,
) -> Result<AstNodeKey, SemanticError> {
    let mut candidates = Vec::new();
    for bound in bounds {
        for method in contract_methods(db, bound.contract, &mut HashSet::new())? {
            let named = db.syntax_unit(method.unit).is_some_and(|syntax| {
                syntax
                    .syntax_index(db)
                    .node_at(syntax.expanded_program(db), method.node)
                    .and_then(|node| node.of::<ContractMethodSignature>())
                    .is_some_and(|signature| signature.name.node.name == member)
            });
            if named && !candidates.contains(&method) {
                candidates.push(method);
            }
        }
    }
    match candidates.as_slice() {
        [method] => Ok(*method),
        _ => Err(SemanticError::new(format!("contract member is not visible: {member}"))),
    }
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

/// Resolve a nominal source identity to its exact `type` declaration. Contract conformance and
/// nominal method/field authorities are defined over types only, so an enum never resolves here.
pub(super) fn concrete_declaration(
    db: &dyn Db,
    key: AstNodeKey,
    source: &GenericSourceTypeIdentity,
) -> Option<AstNodeKey> {
    concrete_nominal_declaration_of_kinds(db, key, source, false)
}

/// Resolve a nominal source identity to its exact `type` or `enum` declaration. Layout and
/// ownership projection of a generic substitution needs both: an applied enum argument (for
/// example the error of `Result<T, SystemError>`) is a nominal payload like any type.
pub(super) fn concrete_type_or_enum_declaration(
    db: &dyn Db,
    key: AstNodeKey,
    source: &GenericSourceTypeIdentity,
) -> Option<AstNodeKey> {
    concrete_nominal_declaration_of_kinds(db, key, source, true)
}

fn concrete_nominal_declaration_of_kinds(
    db: &dyn Db,
    key: AstNodeKey,
    source: &GenericSourceTypeIdentity,
    include_enums: bool,
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
        let index = syntax.syntax_index(db);
        let program = syntax.expanded_program(db);
        let enums = index.ids_of_kind(NodeKind::EnumDefinition).filter(|_| include_enums);
        for node in index.ids_of_kind(NodeKind::TypeDefinition).chain(enums) {
            let declaration = AstNodeKey { unit, node, ..key };
            let Some(definition) = index.node_at(program, node) else {
                continue;
            };
            let Some(arity) = definition
                .of::<TypeDefinition>()
                .map(|definition| definition.generics.len())
                .or_else(|| definition.of::<EnumDefinition>().map(|definition| definition.generics.len()))
            else {
                continue;
            };
            if arity == arguments.len() && stable_declaration_identity(db, declaration).as_ref() == Some(qualified_name)
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

/// `impl` blocks in `concrete`'s own unit whose receiver names `type_name`. Same-unit only,
/// matching design.md's `impl T : Contract` "closed/in-module form".
fn impl_blocks_for_type(db: &dyn Db, concrete: AstNodeKey) -> Vec<AstNodeKey> {
    let Some(syntax) = db.syntax_unit(concrete.unit) else {
        return Vec::new();
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    index
        .ids_of_kind(NodeKind::ImplBlock)
        .filter_map(|node| {
            let impl_block = index.node_at(program, node)?.of::<ImplBlock>()?;
            let Type::Complex(receiver_path) = &impl_block.receiver_type.node else {
                return None;
            };
            let implementation = AstNodeKey { node, ..concrete };
            (layouts::resolve_type_declaration(db, implementation, &receiver_path.node) == Some(concrete))
                .then_some(implementation)
        })
        .collect()
}

/// Whether `concrete` (a `TypeDefinition`, already fetched as `definition`) conforms to
/// `contract`, checking both `type X : Contract { }` and `impl X : Contract { }` (Gap 2, task
/// 2.5 -- one fact, two syntax sources; reused by call-site contract witnesses and by
/// `where T: Contract` bound checking).
pub(in crate::semantic_contract) fn type_declaration_conforms_to_contract(
    db: &dyn Db,
    concrete: AstNodeKey,
    definition: &TypeDefinition,
    contract: AstNodeKey,
) -> bool {
    let conforms_via_type_conformance = definition
        .conformances
        .iter()
        .filter_map(|path| resolve_contract(db, concrete, &path.node))
        .any(|candidate| contract_includes(db, candidate, contract, &mut HashSet::new()));
    if conforms_via_type_conformance {
        return true;
    }
    impl_blocks_for_type(db, concrete).into_iter().any(|impl_key| {
        let Some(impl_syntax) = db.syntax_unit(impl_key.unit) else {
            return false;
        };
        let Some(impl_block) = impl_syntax
            .syntax_index(db)
            .node_at(impl_syntax.expanded_program(db), impl_key.node)
            .and_then(|node| node.of::<ImplBlock>())
        else {
            return false;
        };
        impl_block
            .conformances
            .iter()
            .filter_map(|path| resolve_contract(db, impl_key, &path.node))
            .any(|candidate| contract_includes(db, candidate, contract, &mut HashSet::new()))
    })
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

fn validated_contract_methods(
    db: &dyn Db,
    concrete: AstNodeKey,
    contract: AstNodeKey,
    substitutions: &HashMap<&str, &GenericSourceTypeIdentity>,
) -> Result<Arc<[(AstNodeKey, AstNodeKey)]>, SemanticError> {
    validated_contract_methods_with_expected(db, concrete, contract, substitutions, &HashMap::new())
}

fn validated_contract_methods_with_expected(
    db: &dyn Db,
    concrete: AstNodeKey,
    contract: AstNodeKey,
    substitutions: &HashMap<&str, &GenericSourceTypeIdentity>,
    expected_substitutions: &HashMap<&str, &GenericSourceTypeIdentity>,
) -> Result<Arc<[(AstNodeKey, AstNodeKey)]>, SemanticError> {
    let contract_syntax =
        db.syntax_unit(contract.unit).ok_or_else(|| SemanticError::unavailable("contract_method.owner"))?;
    let contract_definition = contract_syntax
        .syntax_index(db)
        .node_at(contract_syntax.expanded_program(db), contract.node)
        .and_then(|node| node.of::<ContractDefinition>())
        .ok_or_else(|| SemanticError::unavailable("contract_method.owner"))?;
    let syntax = db
        .syntax_unit(concrete.unit)
        .filter(|syntax| syntax.accepts_key(db, concrete))
        .ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
    let mut methods = Vec::new();
    for method in contract_methods(db, contract, &mut HashSet::new())? {
        let expected_syntax =
            db.syntax_unit(method.unit).ok_or_else(|| SemanticError::unavailable("contract_method"))?;
        let expected = expected_syntax
            .syntax_index(db)
            .node_at(expected_syntax.expanded_program(db), method.node)
            .and_then(|node| node.of::<ContractMethodSignature>())
            .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
        let implementation =
            unique_nominal_method_declaration(db, concrete, &expected.name.node.name).ok_or_else(|| {
                SemanticError::new(format!("missing contract method implementation: {}", expected.name.node.name))
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
            .map(|ty| {
                let direct = contract_definition.items.iter().any(|item| match &item.node {
                    ContractNode::MethodSignature(signature) => std::ptr::eq(&signature.node, expected),
                    _ => false,
                });
                if direct {
                    generic_source_type_identity_with_substitutions(db, method, ty, expected_substitutions)
                } else {
                    generic_source_type_identity(db, method, ty)
                }
            })
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
    Ok(methods.into())
}

/// Why one non-generic implementor fails one non-generic contract, classified for the `check`
/// gate (E1601, E1602), read through a typed outcome instead of an opaque rejection message.
/// These are the contract satisfaction checks of the language specification (presence by name,
/// then closed signature identity), which the legacy `TypeChecker` also makes. The `pub`
/// requirement of `validated_contract_methods` is a codegen witness obligation for contract-typed
/// dispatch, not a conformance obligation, so a non-`pub` inline method satisfies this check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::semantic_contract) enum ContractConformanceFailure {
    /// The implementor declares no method of the contract method's name (E1601). Presence is by
    /// name only, as the contract satisfaction algorithm states; visibility is not a conformance
    /// obligation of `check`.
    MissingMethod { method_name: String, expected: String },
    /// The implementor's method has a different closed signature (E1602).
    SignatureMismatch { method_name: String, expected: String, actual: String },
}

/// Classify the first contract method of `contract` that `concrete` (a non-generic `type`
/// declaration) fails to implement. Only closed signatures are compared: a contract method that
/// mentions `This`, an associated type, or a contract generic keeps the specialization
/// authority (`type_applied_contract_implementation`) and is never a finding here. `Ok(None)`
/// means every comparable method is implemented; `Err` is a compiler gap (unregistered or cyclic
/// declarations), never a user error.
pub(in crate::semantic_contract) fn contract_conformance_failure(
    db: &dyn Db,
    concrete: AstNodeKey,
    contract: AstNodeKey,
) -> Result<Option<ContractConformanceFailure>, SemanticError> {
    let syntax = db
        .syntax_unit(concrete.unit)
        .filter(|syntax| syntax.accepts_key(db, concrete))
        .ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
    let contract_syntax =
        db.syntax_unit(contract.unit).ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
    let contract_generics = contract_syntax
        .syntax_index(db)
        .node_at(contract_syntax.expanded_program(db), contract.node)
        .and_then(|node| node.of::<ContractDefinition>())
        .map(|definition| definition.generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>())
        .ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
    for method in contract_methods(db, contract, &mut HashSet::new())? {
        let expected_syntax =
            db.syntax_unit(method.unit).ok_or_else(|| SemanticError::unavailable("contract_method"))?;
        let expected = expected_syntax
            .syntax_index(db)
            .node_at(expected_syntax.expanded_program(db), method.node)
            .and_then(|node| node.of::<ContractMethodSignature>())
            .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
        let method_name = expected.name.node.name.clone();
        let expected_rendered = render_signature_syntax(&expected.parameters, expected.return_type.as_ref());
        let Some(implementation) = unique_nominal_method_declaration(db, concrete, &method_name) else {
            return Ok(Some(ContractConformanceFailure::MissingMethod { method_name, expected: expected_rendered }));
        };
        let actual = syntax
            .syntax_index(db)
            .node_at(syntax.expanded_program(db), implementation.node)
            .and_then(|node| node.of::<MethodDefinition>())
            .ok_or_else(|| SemanticError::unavailable("contract_method"))?;
        let open = expected
            .parameters
            .iter()
            .map(|parameter| &parameter.node.ty.node)
            .chain(expected.return_type.iter().map(|result| &result.node))
            .any(|ty| type_syntax_is_open(ty, &contract_generics));
        if open {
            continue;
        }
        let Ok(expected_types) = closed_signature_identities(db, method, &expected.parameters, expected.return_type.as_ref())
        else {
            continue;
        };
        let Ok(actual_types) =
            closed_signature_identities(db, implementation, &actual.parameters, actual.return_type.as_ref())
        else {
            continue;
        };
        if expected.parameters.len() != actual.parameters.len() || expected_types != actual_types {
            return Ok(Some(ContractConformanceFailure::SignatureMismatch {
                method_name,
                expected: expected_rendered,
                actual: render_signature_syntax(&actual.parameters, actual.return_type.as_ref()),
            }));
        }
    }
    Ok(None)
}

/// Parameter and result identities of one closed signature; a missing result is `unit`.
fn closed_signature_identities(
    db: &dyn Db,
    key: AstNodeKey,
    parameters: &[Spanned<Parameter>],
    result: Option<&Spanned<Type>>,
) -> Result<Vec<GenericSourceTypeIdentity>, SemanticError> {
    let mut identities = parameters
        .iter()
        .map(|parameter| generic_source_type_identity(db, key, &parameter.node.ty.node))
        .collect::<Result<Vec<_>, _>>()?;
    identities.push(match result {
        Some(result) => generic_source_type_identity(db, key, &result.node)?,
        None => GenericSourceTypeIdentity::Abi(SemanticTypeId::UNIT),
    });
    Ok(identities)
}

/// Whether a contract signature type mentions `This`, an associated type, or one of the
/// contract's own generic parameters, at any depth.
fn type_syntax_is_open(ty: &Type, contract_generics: &[String]) -> bool {
    match ty {
        Type::Primitive(_) => false,
        Type::This | Type::Associated { .. } => true,
        Type::Array(inner) => type_syntax_is_open(&inner.node, contract_generics),
        Type::Function { return_type, parameters } => {
            type_syntax_is_open(&return_type.node, contract_generics)
                || parameters.iter().any(|parameter| type_syntax_is_open(&parameter.node, contract_generics))
        }
        Type::Complex(path) => {
            let bare = path.node.segments.len() == 1 && path.node.segments[0].node.type_args.is_empty();
            (bare && contract_generics.iter().any(|generic| *generic == path.node.segments[0].node.name.node.name))
                || path.node.segments.iter().any(|segment| {
                    segment.node.type_args.iter().any(|argument| type_syntax_is_open(&argument.node, contract_generics))
                })
        }
    }
}

/// Source spelling of one type, for diagnostics (`i64`, `Pair<i64>[]`, `(i64) -> bool`).
pub(in crate::semantic_contract) fn render_type_syntax(ty: &Type) -> String {
    match ty {
        Type::Primitive(primitive) => format!("{:?}", primitive.node).to_lowercase(),
        Type::This => "This".to_string(),
        Type::Associated { contract, name } => format!("{}::{}", render_path_syntax(&contract.node), name.node.name),
        Type::Array(inner) => format!("{}[]", render_type_syntax(&inner.node)),
        Type::Function { return_type, parameters } => format!(
            "({}) -> {}",
            parameters.iter().map(|parameter| render_type_syntax(&parameter.node)).collect::<Vec<_>>().join(", "),
            render_type_syntax(&return_type.node)
        ),
        Type::Complex(path) => render_path_syntax(&path.node),
    }
}

fn render_path_syntax(path: &Path) -> String {
    path.segments
        .iter()
        .map(|segment| {
            if segment.node.type_args.is_empty() {
                segment.node.name.node.name.clone()
            } else {
                format!(
                    "{}<{}>",
                    segment.node.name.node.name,
                    segment.node.type_args.iter().map(|argument| render_type_syntax(&argument.node)).collect::<Vec<_>>().join(", ")
                )
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// `(params) -> result` spelling of one declared signature; a missing result is `unit`.
pub(in crate::semantic_contract) fn render_signature_syntax(
    parameters: &[Spanned<Parameter>],
    result: Option<&Spanned<Type>>,
) -> String {
    format!(
        "({}) -> {}",
        parameters.iter().map(|parameter| render_type_syntax(&parameter.node.ty.node)).collect::<Vec<_>>().join(", "),
        result.map_or_else(|| "unit".to_string(), |result| render_type_syntax(&result.node))
    )
}

/// Nesting limit for proving `impl ... where` bounds of a selected implementation.
const WHERE_BOUND_DEPTH_LIMIT: usize = 32;

/// One `where G: Contract<...>` bound of a callable declaration and the value parameters typed
/// directly by the lexical generic `G`. Applied contract arguments remain source syntax here;
/// they are interpreted only in one concrete call's complete substitution environment.
#[derive(Debug, Clone)]
pub(super) struct DeclaredWhereBound {
    pub(super) generic: String,
    pub(super) path: Path,
    pub(super) contract: AstNodeKey,
    pub(super) parameters: Vec<(AstNodeKey, u32)>,
}

fn bound_contract_name(path: &Path) -> String {
    path.segments.last().map(|segment| segment.node.name.node.name.clone()).unwrap_or_default()
}

fn generic_bound_not_satisfied(type_name: String, contract: &Path) -> SemanticError {
    let contract_name = bound_contract_name(contract);
    let issue = beskid_analysis::analysis::SemanticIssueKind::GenericBoundNotSatisfied {
        type_name: type_name.clone(),
        contract_name: contract_name.clone(),
    };
    SemanticError::generic_bound_not_satisfied(
        format!("{}: {}", issue.code(), issue.message()),
        type_name,
        contract_name,
    )
}

/// Resolve the contract declaration named by an applied bound path (`Serializable<E>` names
/// `Serializable`) and require its declared generic arity. Arguments are not interpreted here.
fn bound_contract_declaration(db: &dyn Db, context: AstNodeKey, path: &Path) -> Result<AstNodeKey, SemanticError> {
    let terminal = path.segments.last().ok_or_else(|| SemanticError::unavailable("where_bound.contract"))?;
    let mut declaration_path = path.clone();
    if let Some(segment) = declaration_path.segments.last_mut() {
        segment.node.type_args.clear();
    }
    let contract = resolve_contract(db, context, &declaration_path).ok_or_else(|| {
        SemanticError::new(format!("unknown contract `{}` in where clause", terminal.node.name.node.name))
    })?;
    let syntax = db
        .syntax_unit(contract.unit)
        .filter(|syntax| syntax.accepts_key(db, contract))
        .ok_or_else(|| SemanticError::unavailable("where_bound.contract"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), contract.node)
        .and_then(|node| node.of::<ContractDefinition>())
        .ok_or_else(|| SemanticError::unavailable("where_bound.contract"))?;
    if definition.generics.len() != terminal.node.type_args.len() {
        return Err(SemanticError::new(format!(
            "where clause applies contract `{}` with {} argument(s) but it declares {}",
            terminal.node.name.node.name,
            terminal.node.type_args.len(),
            definition.generics.len()
        )));
    }
    Ok(contract)
}

/// Interpret the applied arguments of a contract path in an exact source environment.
fn applied_contract_arguments(
    db: &dyn Db,
    context: AstNodeKey,
    path: &Path,
    environment: &HashMap<&str, &GenericSourceTypeIdentity>,
) -> Result<Arc<[GenericSourceTypeIdentity]>, SemanticError> {
    path.segments
        .last()
        .ok_or_else(|| SemanticError::unavailable("where_bound.contract"))?
        .node
        .type_args
        .iter()
        .map(|argument| generic_source_type_identity_with_substitutions(db, context, &argument.node, environment))
        .collect::<Result<Vec<_>, _>>()
        .map(Arc::from)
}

/// Where-bounds that constrain `declaration`'s lexical generic parameters: a function's own
/// `where` clause, or the enclosing `impl<...> ... where` clause of an implementation method.
pub(super) fn declared_where_bounds(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Result<Vec<DeclaredWhereBound>, SemanticError> {
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return Ok(Vec::new());
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(node) = index.node_at(program, declaration.node) else {
        return Ok(Vec::new());
    };
    let (parameters, bounds, scope) = if let Some(function) = node.of::<FunctionDefinition>() {
        (
            function.parameters.as_slice(),
            function.where_bounds.as_slice(),
            function.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>(),
        )
    } else if let Some(method) = node.of::<MethodDefinition>() {
        let Some(block) = nearest_ancestor(index, declaration.node, |kind| kind == NodeKind::ImplBlock)
            .and_then(|parent| index.node_at(program, parent))
            .and_then(|node| node.of::<ImplBlock>())
        else {
            return Ok(Vec::new());
        };
        (
            method.parameters.as_slice(),
            block.where_bounds.as_slice(),
            block.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>(),
        )
    } else {
        return Ok(Vec::new());
    };
    bounds
        .iter()
        .map(|bound| {
            let generic = bound.parameter.node.name.as_str();
            let contract = bound_contract_declaration(db, declaration, &bound.contract.node)?;
            let parameters = parameters
                .iter()
                .enumerate()
                .filter_map(|(position, parameter)| {
                    if generic_parameter_reference_name(&parameter.node.ty.node) != Some(generic) {
                        return None;
                    }
                    let node = index.direct_child_id(program, declaration.node, DynNodeRef::from(parameter))?;
                    let parameter_key = AstNodeKey { node, ..declaration };
                    let lexical = scope.contains(&generic)
                        || type_syntax_is_enclosing_generic_parameter_reference(
                            db,
                            parameter_key,
                            &parameter.node.ty.node,
                        );
                    lexical.then_some((parameter_key, u32::try_from(position).ok()?))
                })
                .collect();
            Ok(DeclaredWhereBound {
                generic: generic.to_owned(),
                path: bound.contract.node.clone(),
                contract,
                parameters,
            })
        })
        .collect()
}

/// Bind bare `impl<...>` parameters named inside `syntax` to the matching parts of `actual`.
/// Everything else is proven afterwards by exact identity comparison of the substituted syntax.
fn bind_implementation_generics(
    syntax: &Type,
    actual: &GenericSourceTypeIdentity,
    names: &[&str],
    bindings: &mut Vec<(String, GenericSourceTypeIdentity)>,
) -> bool {
    if let Some(name) = generic_parameter_reference_name(syntax).filter(|name| names.contains(name)) {
        return match bindings.iter().find(|(bound, _)| bound == name) {
            Some((_, existing)) => existing == actual,
            None => {
                bindings.push((name.to_owned(), actual.clone()));
                true
            }
        };
    }
    match (syntax, actual) {
        (Type::Complex(path), GenericSourceTypeIdentity::Nominal { arguments, .. }) => {
            let expected =
                path.node.segments.iter().flat_map(|segment| segment.node.type_args.iter()).collect::<Vec<_>>();
            expected.len() == arguments.len()
                && expected
                    .into_iter()
                    .zip(arguments.iter())
                    .all(|(expected, actual)| bind_implementation_generics(&expected.node, actual, names, bindings))
        }
        (Type::Array(element), GenericSourceTypeIdentity::Array(actual)) => {
            bind_implementation_generics(&element.node, actual, names, bindings)
        }
        _ => true,
    }
}

/// Whether one written conformance path is exactly `contract<arguments>` in `environment`.
/// A non-generic required contract may also be reached through contract embedding.
fn conformance_path_matches(
    db: &dyn Db,
    context: AstNodeKey,
    path: &Path,
    environment: &HashMap<&str, &GenericSourceTypeIdentity>,
    contract: AstNodeKey,
    arguments: &[GenericSourceTypeIdentity],
) -> bool {
    let Some(terminal) = path.segments.last() else {
        return false;
    };
    let mut declaration_path = path.clone();
    if let Some(segment) = declaration_path.segments.last_mut() {
        segment.node.type_args.clear();
    }
    let Some(candidate) = resolve_contract(db, context, &declaration_path) else {
        return false;
    };
    if candidate == contract {
        return terminal.node.type_args.len() == arguments.len()
            && terminal.node.type_args.iter().zip(arguments).all(|(argument, expected)| {
                generic_source_type_identity_with_substitutions(db, context, &argument.node, environment)
                    .is_ok_and(|actual| actual == *expected)
            });
    }
    arguments.is_empty() && contract_includes(db, candidate, contract, &mut HashSet::new())
}

fn environment_map(bindings: &[(String, GenericSourceTypeIdentity)]) -> HashMap<&str, &GenericSourceTypeIdentity> {
    bindings.iter().map(|(name, identity)| (name.as_str(), identity)).collect()
}

fn environment_substitutions(bindings: Vec<(String, GenericSourceTypeIdentity)>) -> Vec<GenericSubstitution> {
    bindings
        .into_iter()
        .map(|(name, identity)| GenericSubstitution::from_source(name, identity.abi_type(), identity))
        .collect()
}

/// Prove `concrete<source_arguments> : contract<arguments>` against the exact applied
/// conformance written on the type or on one of its `impl` blocks, including that block's own
/// where-bounds. Returns the selected implementation's generic environment in declaration order.
fn applied_conformance_environment(
    db: &dyn Db,
    concrete: AstNodeKey,
    definition: &TypeDefinition,
    source_arguments: &[GenericSourceTypeIdentity],
    contract: AstNodeKey,
    arguments: &[GenericSourceTypeIdentity],
    depth: usize,
) -> Result<Option<Vec<GenericSubstitution>>, SemanticError> {
    if depth > WHERE_BOUND_DEPTH_LIMIT {
        return Err(SemanticError::new("where-bound conformance nesting exceeds its limit"));
    }
    if definition.generics.len() != source_arguments.len() {
        return Ok(None);
    }
    let receiver_environment = definition
        .generics
        .iter()
        .zip(source_arguments)
        .map(|(generic, argument)| (generic.node.name.clone(), argument.clone()))
        .collect::<Vec<_>>();
    if definition.conformances.iter().any(|path| {
        conformance_path_matches(db, concrete, &path.node, &environment_map(&receiver_environment), contract, arguments)
    }) {
        return Ok(Some(environment_substitutions(receiver_environment)));
    }
    let receiver_identity = GenericSourceTypeIdentity::Nominal {
        qualified_name: stable_declaration_identity(db, concrete)
            .ok_or_else(|| SemanticError::unavailable("contract_conformance.receiver_identity"))?,
        arguments: source_arguments.to_vec().into(),
    };
    for implementation in impl_blocks_for_type(db, concrete) {
        let syntax = db
            .syntax_unit(implementation.unit)
            .filter(|syntax| syntax.accepts_key(db, implementation))
            .ok_or_else(|| SemanticError::unavailable("contract_conformance.impl"))?;
        let block = syntax
            .syntax_index(db)
            .node_at(syntax.expanded_program(db), implementation.node)
            .and_then(|node| node.of::<ImplBlock>())
            .ok_or_else(|| SemanticError::unavailable("contract_conformance.impl"))?;
        let names = block.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>();
        if names.iter().enumerate().any(|(position, name)| names[..position].contains(name)) {
            return Err(SemanticError::new("implementation generic scope is ambiguous"));
        }
        if names.is_empty() {
            // `impl Box<T> : Contract<T>` reuses the receiver's own generic names.
            let environment = environment_map(&receiver_environment);
            if block
                .conformances
                .iter()
                .any(|path| conformance_path_matches(db, implementation, &path.node, &environment, contract, arguments))
                && implementation_bounds_hold(db, implementation, &block.where_bounds, &environment, depth)?
            {
                return Ok(Some(environment_substitutions(receiver_environment.clone())));
            }
            continue;
        }
        let mut receiver_bindings = Vec::new();
        if !bind_implementation_generics(&block.receiver_type.node, &receiver_identity, &names, &mut receiver_bindings)
        {
            continue;
        }
        for path in &block.conformances {
            let Some(terminal) = path.node.segments.last() else {
                continue;
            };
            let mut bindings = receiver_bindings.clone();
            if terminal.node.type_args.len() == arguments.len()
                && !terminal
                    .node
                    .type_args
                    .iter()
                    .zip(arguments)
                    .all(|(syntax, actual)| bind_implementation_generics(&syntax.node, actual, &names, &mut bindings))
            {
                continue;
            }
            if names.iter().any(|name| !bindings.iter().any(|(bound, _)| bound == name)) {
                // Every implementation parameter must be fixed by the receiver or the exact
                // applied contract; an unbound parameter cannot issue a concrete witness.
                continue;
            }
            bindings.sort_by_key(|(name, _)| names.iter().position(|candidate| *candidate == name.as_str()));
            let environment = environment_map(&bindings);
            let receiver_matches = generic_source_type_identity_with_substitutions(
                db,
                implementation,
                &block.receiver_type.node,
                &environment,
            )
            .is_ok_and(|actual| actual == receiver_identity);
            if receiver_matches
                && conformance_path_matches(db, implementation, &path.node, &environment, contract, arguments)
                && implementation_bounds_hold(db, implementation, &block.where_bounds, &environment, depth)?
            {
                return Ok(Some(environment_substitutions(bindings.clone())));
            }
        }
    }
    Ok(None)
}

/// Prove an implementation's own where-bounds in its concrete environment.
fn implementation_bounds_hold(
    db: &dyn Db,
    implementation: AstNodeKey,
    bounds: &[beskid_analysis::syntax::WhereBound],
    environment: &HashMap<&str, &GenericSourceTypeIdentity>,
    depth: usize,
) -> Result<bool, SemanticError> {
    for bound in bounds {
        let Some(source) = environment.get(bound.parameter.node.name.as_str()).copied() else {
            return Ok(false);
        };
        let contract = bound_contract_declaration(db, implementation, &bound.contract.node)?;
        let Ok(arguments) = applied_contract_arguments(db, implementation, &bound.contract.node, environment) else {
            return Ok(false);
        };
        let Some(concrete) = concrete_declaration(db, implementation, source) else {
            return Ok(false);
        };
        let GenericSourceTypeIdentity::Nominal { arguments: source_arguments, .. } = source else {
            return Ok(false);
        };
        let syntax = db.syntax_unit(concrete.unit).ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
        let definition = syntax
            .syntax_index(db)
            .node_at(syntax.expanded_program(db), concrete.node)
            .and_then(|node| node.of::<TypeDefinition>())
            .ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
        if applied_conformance_environment(db, concrete, definition, source_arguments, contract, &arguments, depth + 1)?
            .is_none()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Implementation methods of the exact applied contract, typed in the implementation's own
/// environment and compared with the contract signatures under the applied arguments.
fn applied_contract_methods(
    db: &dyn Db,
    concrete: AstNodeKey,
    contract: AstNodeKey,
    applied_arguments: &[GenericSourceTypeIdentity],
    environment: &[GenericSubstitution],
) -> Result<Arc<[(AstNodeKey, AstNodeKey)]>, SemanticError> {
    let contract_syntax =
        db.syntax_unit(contract.unit).ok_or_else(|| SemanticError::unavailable("contract_method.owner"))?;
    let definition = contract_syntax
        .syntax_index(db)
        .node_at(contract_syntax.expanded_program(db), contract.node)
        .and_then(|node| node.of::<ContractDefinition>())
        .ok_or_else(|| SemanticError::unavailable("contract_method.owner"))?;
    if definition.generics.len() != applied_arguments.len() {
        return Err(SemanticError::new("applied contract argument count differs from its declaration"));
    }
    let expected = definition
        .generics
        .iter()
        .zip(applied_arguments)
        .map(|(parameter, argument)| (parameter.node.name.as_str(), argument))
        .collect::<HashMap<_, _>>();
    let substitutions =
        environment.iter().map(|binding| (binding.parameter.as_ref(), binding.source_identity())).collect();
    validated_contract_methods_with_expected(db, concrete, contract, &substitutions, &expected)
}

/// A proven applied conformance, before it is attached to one callable parameter.
struct ProvenConformance {
    concrete: AstNodeKey,
    environment: Arc<[GenericSubstitution]>,
    methods: Arc<[(AstNodeKey, AstNodeKey)]>,
}

/// Prove that `source_identity` conforms to the exact applied `contract<applied_arguments>`.
/// `unsatisfied` receives the concrete type name, or `None` when no concrete type exists.
fn prove_applied_conformance(
    db: &dyn Db,
    context: AstNodeKey,
    source_identity: &GenericSourceTypeIdentity,
    contract: AstNodeKey,
    applied_arguments: &[GenericSourceTypeIdentity],
    unsatisfied: &dyn Fn(Option<&str>) -> SemanticError,
) -> Result<ProvenConformance, SemanticError> {
    let concrete = concrete_declaration(db, context, source_identity).ok_or_else(|| unsatisfied(None))?;
    let GenericSourceTypeIdentity::Nominal { arguments: source_arguments, .. } = source_identity else {
        return Err(unsatisfied(None));
    };
    let syntax = db.syntax_unit(concrete.unit).ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), concrete.node)
        .and_then(|node| node.of::<TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("contract_conformance"))?;
    let environment =
        applied_conformance_environment(db, concrete, definition, source_arguments, contract, applied_arguments, 0)?
            .ok_or_else(|| unsatisfied(Some(definition.name.node.name.as_str())))?;
    let methods = applied_contract_methods(db, concrete, contract, applied_arguments, &environment)?;
    Ok(ProvenConformance { concrete, environment: environment.into(), methods })
}

/// Issue every contract witness of one call after its generic substitutions are complete.
///
/// Contract-typed parameters are proven from their argument's source identity. Each where-bound
/// is interpreted as its exact applied contract in `substitutions` (`T: Serializable<E>` with
/// `E = Wire` requires `Serializable<Wire>`), and every value parameter typed by the bounded
/// generic receives a witness carrying the applied arguments and implementation environment.
pub(super) fn contract_witnesses_for_call(
    db: &dyn Db,
    declaration: AstNodeKey,
    arguments: &[AstNodeKey],
    is_method: bool,
    enclosing: Option<&GenericSpecializationInstance>,
    substitutions: &[GenericSubstitution],
) -> Result<Arc<[ContractParameterWitness]>, SemanticError> {
    let mut witnesses = Vec::new();
    for (parameter, position, contract) in contract_parameter_declarations(db, declaration) {
        let argument = *arguments
            .get(position as usize + usize::from(is_method))
            .ok_or_else(|| SemanticError::unavailable("contract_argument"))?;
        let source_identity = specialized_source_expression_identity(db, argument, enclosing)?;
        let proven =
            prove_applied_conformance(db, argument, &source_identity, contract, &[], &|concrete| match concrete {
                None => SemanticError::new("contract argument has no concrete conforming type"),
                Some(name) => SemanticError::new(format!("missing contract conformance for {name}")),
            })?;
        witnesses.push(ContractParameterWitness {
            parameter,
            position,
            contract,
            concrete: proven.concrete,
            source_identity,
            applied_arguments: Arc::from([]),
            environment: proven.environment,
            methods: proven.methods,
        });
    }
    let environment =
        substitutions.iter().map(|binding| (binding.parameter.as_ref(), binding.source_identity())).collect();
    for bound in declared_where_bounds(db, declaration)? {
        let Some(source_identity) = substitutions
            .iter()
            .find(|binding| &*binding.parameter == bound.generic.as_str())
            .map(|binding| binding.source_identity().clone())
        else {
            if bound.parameters.is_empty() {
                continue;
            }
            return Err(SemanticError::unavailable("where_bound.substitution"));
        };
        let applied_arguments = applied_contract_arguments(db, declaration, &bound.path, &environment)?;
        let proven = prove_applied_conformance(
            db,
            declaration,
            &source_identity,
            bound.contract,
            &applied_arguments,
            &|concrete| generic_bound_not_satisfied(concrete.unwrap_or(bound.generic.as_str()).to_owned(), &bound.path),
        )?;
        for (parameter, position) in &bound.parameters {
            witnesses.push(ContractParameterWitness {
                parameter: *parameter,
                position: *position,
                contract: bound.contract,
                concrete: proven.concrete,
                source_identity: source_identity.clone(),
                applied_arguments: applied_arguments.clone(),
                environment: proven.environment.clone(),
                methods: proven.methods.clone(),
            });
        }
    }
    Ok(witnesses.into())
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
    // A parameter may carry several bound witnesses (`T: A, T: B`); the member's contract
    // method selects exactly one of them.
    let (witness, declaration) = enclosing
        .contract_witnesses
        .iter()
        .filter(|witness| witness.parameter == parameter)
        .find_map(|witness| {
            witness
                .methods
                .iter()
                .find_map(|(signature, implementation)| (*signature == method).then_some((witness, *implementation)))
        })
        .ok_or_else(|| SemanticError::unavailable("contract_witness"))?;
    let target = db.syntax_unit(declaration.unit).ok_or_else(|| SemanticError::unavailable("contract_method"))?;
    let substitutions = witness.environment.clone();
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
        substitutions,
        contract_witnesses: Arc::from([]),
    }))
}

/// Recompute executable conformance before a persisted specialization is used as effect
/// authority. Serialized method pairs and pointer ABI identities cannot issue this witness:
/// the parameter must still be declared contract-typed or bounded, a bound's applied arguments
/// are re-derived from `substitutions`, and the implementation environment and methods are
/// proven again against that exact applied contract.
pub(super) fn validate_current_parameter_witness(
    db: &dyn Db,
    declaration: AstNodeKey,
    substitutions: &[GenericSubstitution],
    witness: &ContractParameterWitness,
) -> Result<bool, SemanticError> {
    let annotated = witness.applied_arguments.is_empty()
        && contract_parameter_declarations(db, declaration).iter().any(|(parameter, position, contract)| {
            *parameter == witness.parameter && *position == witness.position && *contract == witness.contract
        });
    if !annotated {
        let environment =
            substitutions.iter().map(|binding| (binding.parameter.as_ref(), binding.source_identity())).collect();
        let bounded = declared_where_bounds(db, declaration)?.iter().any(|bound| {
            bound.contract == witness.contract
                && bound.parameters.contains(&(witness.parameter, witness.position))
                && substitutions.iter().any(|binding| {
                    &*binding.parameter == bound.generic.as_str()
                        && *binding.source_identity() == witness.source_identity
                })
                && matches!(
                    applied_contract_arguments(db, declaration, &bound.path, &environment),
                    Ok(arguments) if arguments.as_ref() == witness.applied_arguments.as_ref()
                )
        });
        if !bounded {
            return Ok(false);
        }
    }
    let syntax = db
        .syntax_unit(witness.concrete.unit)
        .filter(|unit| unit.accepts_key(db, witness.concrete))
        .ok_or_else(|| SemanticError::unavailable("publication.concrete"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), witness.concrete.node)
        .and_then(|node| node.of::<TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("publication.concrete"))?;
    let GenericSourceTypeIdentity::Nominal { qualified_name, arguments } = &witness.source_identity else {
        return Ok(false);
    };
    if stable_declaration_identity(db, witness.concrete).as_ref() != Some(qualified_name)
        || definition.generics.len() != arguments.len()
    {
        return Ok(false);
    }
    let Some(environment) = applied_conformance_environment(
        db,
        witness.concrete,
        definition,
        arguments,
        witness.contract,
        &witness.applied_arguments,
        0,
    )?
    else {
        return Ok(false);
    };
    if environment.as_slice() != witness.environment.as_ref() {
        return Ok(false);
    }
    Ok(applied_contract_methods(db, witness.concrete, witness.contract, &witness.applied_arguments, &environment)?
        .as_ref()
        == witness.methods.as_ref())
}

/// Applied receiver admission used only with retained, current semantic target arguments.
/// The legacy declaration-only entrypoint deliberately remains unapplied/fail-closed.
pub(crate) fn serialization_target_contract_methods(
    db: &dyn Db,
    owner: AstNodeKey,
    arguments: &[GenericSourceTypeIdentity],
    implementation: AstNodeKey,
) -> Result<Vec<(AppliedContractIdentity, Arc<[(AstNodeKey, AstNodeKey)]>)>, SemanticError> {
    serialization_target_contract_methods_in_environment(db, owner, arguments, implementation, &[])
}

/// Extra implementation parameters (for example E) retain their concrete source
/// identities. Neither the receiver arguments nor pointer ABI can stand in for E.
pub(crate) fn serialization_target_contract_methods_in_environment(
    db: &dyn Db,
    owner: AstNodeKey,
    arguments: &[GenericSourceTypeIdentity],
    implementation: AstNodeKey,
    environment: &[GenericSubstitution],
) -> Result<Vec<(AppliedContractIdentity, Arc<[(AstNodeKey, AstNodeKey)]>)>, SemanticError> {
    let syntax = db
        .syntax_unit(owner.unit)
        .filter(|s| s.accepts_key(db, owner))
        .ok_or_else(|| SemanticError::unavailable("serialization_target.current_owner"))?;
    let implementation_syntax = db
        .syntax_unit(implementation.unit)
        .filter(|s| {
            s.accepts_key(db, implementation)
                && s.project(db) == syntax.project(db)
                && implementation.generation == owner.generation
        })
        .ok_or_else(|| SemanticError::unavailable("serialization_target.current_implementation"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), owner.node)
        .ok_or_else(|| SemanticError::unavailable("serialization_target.owner"))?;
    let generics = node
        .of::<TypeDefinition>()
        .map(|d| &d.generics)
        .or_else(|| node.of::<EnumDefinition>().map(|d| &d.generics))
        .ok_or_else(|| SemanticError::unavailable("serialization_target.nominal"))?;
    if generics.len() != arguments.len() {
        return Err(SemanticError::new("serialization target generic arity differs"));
    }
    let substitutions =
        generics.iter().zip(arguments).map(|(g, a)| (g.node.name.as_str(), a)).collect::<HashMap<_, _>>();
    let block = implementation_syntax
        .syntax_index(db)
        .node_at(implementation_syntax.expanded_program(db), implementation.node)
        .and_then(|n| n.of::<ImplBlock>())
        .ok_or_else(|| SemanticError::unavailable("serialization_target.impl"))?;
    if super::portable_nominal::serialization_impl_receiver(db, implementation) != Some(owner) {
        return Err(SemanticError::new("serialization implementation has another receiver"));
    }
    let mut substitutions = substitutions;
    let names = block.generics.iter().map(|generic| generic.node.name.as_str()).collect::<Vec<_>>();
    if names.iter().enumerate().any(|(i, name)| names[..i].contains(name)) {
        return Err(SemanticError::new("serialization implementation generic scope is ambiguous"));
    }
    for binding in environment {
        if !names.contains(&binding.parameter.as_ref()) {
            return Err(SemanticError::new("serialization implementation environment has a foreign parameter"));
        }
        if let Some(previous) = substitutions.insert(binding.parameter.as_ref(), binding.source_identity()) {
            if previous != binding.source_identity() {
                return Err(SemanticError::new("serialization implementation environment changes receiver arguments"));
            }
        }
    }
    if names.iter().any(|name| !substitutions.contains_key(name)) {
        return Err(SemanticError::new("serialization implementation requires concrete generic arguments"));
    }
    if environment
        .iter()
        .enumerate()
        .any(|(i, binding)| environment[..i].iter().any(|other| other.parameter == binding.parameter))
    {
        return Err(SemanticError::new("serialization implementation environment repeats a parameter"));
    }
    let receiver_identity =
        generic_source_type_identity_with_substitutions(db, implementation, &block.receiver_type.node, &substitutions)?;
    let expected_receiver = GenericSourceTypeIdentity::Nominal {
        qualified_name: stable_declaration_identity(db, owner)
            .ok_or_else(|| SemanticError::unavailable("serialization_target.receiver_identity"))?,
        arguments: arguments.to_vec().into(),
    };
    if receiver_identity != expected_receiver {
        return Err(SemanticError::new("serialization implementation receiver arguments differ"));
    }
    let mut result = Vec::new();
    for path in &block.conformances {
        let application = application_from_path_with_arguments(db, owner, implementation, &path.node, &substitutions)?;
        let contract_syntax = db
            .syntax_unit(application.declaration.unit)
            .filter(|s| s.accepts_key(db, application.declaration) && s.project(db) == syntax.project(db))
            .ok_or_else(|| SemanticError::unavailable("serialization_target.contract_owner"))?;
        let contract = contract_syntax
            .syntax_index(db)
            .node_at(contract_syntax.expanded_program(db), application.declaration.node)
            .and_then(|n| n.of::<ContractDefinition>())
            .ok_or_else(|| SemanticError::unavailable("serialization_target.contract"))?;
        let expected = contract
            .generics
            .iter()
            .zip(application.arguments.iter())
            .map(|(g, a)| (g.node.name.as_str(), a))
            .collect::<HashMap<_, _>>();
        let mut methods = Vec::new();
        for signature_key in contract_methods(db, application.declaration, &mut HashSet::new())? {
            let signature_syntax = db
                .syntax_unit(signature_key.unit)
                .ok_or_else(|| SemanticError::unavailable("serialization_target.signature"))?;
            let signature = signature_syntax
                .syntax_index(db)
                .node_at(signature_syntax.expanded_program(db), signature_key.node)
                .and_then(|n| n.of::<ContractMethodSignature>())
                .ok_or_else(|| SemanticError::unavailable("serialization_target.signature"))?;
            let candidates =
                block.methods.iter().filter(|m| m.node.name.node.name == signature.name.node.name).collect::<Vec<_>>();
            let [actual] = candidates.as_slice() else {
                return Err(SemanticError::new("serialization contribution method missing/ambiguous"));
            };
            if actual.node.visibility.node != Visibility::Public {
                return Err(SemanticError::new("serialization contribution method is private"));
            }
            let actual_node = implementation_syntax
                .syntax_index(db)
                .direct_child_id(
                    implementation_syntax.expanded_program(db),
                    implementation.node,
                    DynNodeRef::from(*actual),
                )
                .ok_or_else(|| SemanticError::unavailable("serialization_target.method_correspondence"))?;
            let actual_key = AstNodeKey { node: actual_node, ..implementation };
            let direct = contract.items.iter().any(|item| {
                matches!(&item.node,
                ContractNode::MethodSignature(method) if std::ptr::eq(&method.node, signature))
            });
            let expected_types = signature
                .parameters
                .iter()
                .map(|p| &p.node.ty.node)
                .chain(signature.return_type.iter().map(|t| &t.node))
                .map(|ty| {
                    if direct {
                        generic_source_type_identity_with_substitutions(db, signature_key, ty, &expected)
                    } else {
                        generic_source_type_identity(db, signature_key, ty)
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;
            let actual_types = actual
                .node
                .parameters
                .iter()
                .map(|p| &p.node.ty.node)
                .chain(actual.node.return_type.iter().map(|t| &t.node))
                .map(|ty| generic_source_type_identity_with_substitutions(db, actual_key, ty, &substitutions))
                .collect::<Result<Vec<_>, _>>()?;
            if signature.parameters.len() != actual.node.parameters.len() || expected_types != actual_types {
                return Err(SemanticError::new("serialization contribution applied signature mismatch"));
            }
            methods.push((signature_key, actual_key));
        }
        result.push((application, methods.into()));
    }
    Ok(result)
}

#[cfg(test)]
mod serialization_target_tests {
    use super::*;
    use crate::{BeskidDatabase, build_typed_program, project_session_for_planned_syntax_assembly};
    use beskid_analysis::projects::{
        AssemblyOptions, CompilePlan, Target, TargetKind, assemble_program_with_materializer,
    };
    fn fixture_with_source(
        source: &str,
    ) -> (tempfile::TempDir, BeskidDatabase, Arc<beskid_analysis::projects::ProgramAssembly>) {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("Src");
        std::fs::create_dir(&src).unwrap();
        let path = src.join("Main.bd");
        std::fs::write(&path, source).unwrap();
        let plan = CompilePlan {
            project_root: root.path().into(),
            manifest_path: root.path().join("Host.bproj"),
            project_name: "Host".into(),
            source_root: src,
            target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
            dependency_projects: vec![],
            unresolved_dependencies: vec![],
            has_std_dependency: false,
        };
        let assembly = Arc::new(
            assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None)
                .unwrap(),
        );
        let mut db = BeskidDatabase::default();
        let project =
            project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "target-contract".into())
                .unwrap();
        build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
        (root, db, assembly)
    }
    fn fixture() -> (tempfile::TempDir, BeskidDatabase, Arc<beskid_analysis::projects::ProgramAssembly>) {
        fixture_with_source(
            "pub contract Codec<T> { T Read(); }\npub type Box<T> { pub T Value, }\nimpl Box<T> : Codec<T> { pub T Read() { return this.Value; } }\npub enum Choice<T> { Some(T Value), }\nimpl Choice<T> : Codec<T> { pub T Read() { return 0; } }\npub unit Main() { return; }",
        )
    }
    fn key(
        db: &dyn Db,
        assembly: &beskid_analysis::projects::ProgramAssembly,
        kind: NodeKind,
        ordinal: usize,
    ) -> AstNodeKey {
        AstNodeKey {
            unit: SourceUnitId::new(db, assembly.entry_unit().path.clone()),
            generation: assembly.generation,
            node: assembly.entry_syntax_index().ids_of_kind(kind).nth(ordinal).unwrap(),
        }
    }
    #[test]
    fn implementation_encoder_parameter_retains_exact_source_application() {
        let (_root, db, assembly) = fixture_with_source(
            "pub contract Codec<E> { unit Write(E encoder); }\npub type Box<T> { pub T Value, }\npub type Wire {}\nimpl<T,E> Box<T> : Codec<E> { pub unit Write(E encoder) { return; } }\npub unit Main() { return; }",
        );
        let owner = key(&db, &assembly, NodeKind::TypeDefinition, 0);
        let wire = key(&db, &assembly, NodeKind::TypeDefinition, 1);
        let implementation = key(&db, &assembly, NodeKind::ImplBlock, 0);
        let args = [GenericSourceTypeIdentity::Abi(SemanticTypeId::I32)];
        assert!(serialization_target_contract_methods(&db, owner, &args, implementation).is_err());
        let source = GenericSourceTypeIdentity::Nominal {
            qualified_name: stable_declaration_identity(&db, wire).unwrap(),
            arguments: Arc::from([]),
        };
        let binding = GenericSubstitution::from_source("E", SemanticTypeId::POINTER, source.clone());
        let admitted =
            serialization_target_contract_methods_in_environment(&db, owner, &args, implementation, &[binding.clone()])
                .unwrap();
        assert_eq!(admitted.len(), 1);
        assert_eq!(admitted[0].0.arguments.as_ref(), &[source]);
        assert_eq!(admitted[0].1.len(), 1);
        assert!(
            serialization_target_contract_methods_in_environment(
                &db,
                owner,
                &args,
                implementation,
                &[binding.clone(), binding.clone()]
            )
            .is_err()
        );
        assert!(
            serialization_target_contract_methods_in_environment(
                &db,
                owner,
                &args,
                implementation,
                &[binding.rebind("Foreign")]
            )
            .is_err()
        );
        let changed_receiver = GenericSubstitution::inferred("T", SemanticTypeId::U32);
        assert!(
            serialization_target_contract_methods_in_environment(
                &db,
                owner,
                &args,
                implementation,
                &[binding, changed_receiver]
            )
            .is_err()
        );
    }

    #[test]
    fn decoder_receiver_retains_reader_and_field_binding_environment() {
        let (_root, db, assembly) = fixture_with_source(
            "pub contract Decode<T> { T Read(); }\npub type Target<T> { pub T Value, }\npub type Reader {}\npub type Field {}\npub type Adapter<T,R,B> { pub R Reader, pub B Binding, }\nimpl<T,R,B> Adapter<T,R,B> : Decode<T> { pub T Read() { return 0; } }\npub unit Main() { return; }",
        );
        let target = key(&db, &assembly, NodeKind::TypeDefinition, 0);
        let reader = key(&db, &assembly, NodeKind::TypeDefinition, 1);
        let field = key(&db, &assembly, NodeKind::TypeDefinition, 2);
        let adapter = key(&db, &assembly, NodeKind::TypeDefinition, 3);
        let implementation = key(&db, &assembly, NodeKind::ImplBlock, 0);
        let arguments = [GenericSourceTypeIdentity::Abi(SemanticTypeId::I32)];
        let identity = |owner| GenericSourceTypeIdentity::Nominal {
            qualified_name: stable_declaration_identity(&db, owner).unwrap(),
            arguments: Arc::from([]),
        };
        let environment = [
            GenericSubstitution::from_source("R", SemanticTypeId::POINTER, identity(reader)),
            GenericSubstitution::from_source("B", SemanticTypeId::POINTER, identity(field)),
        ];
        assert!(serialization_contribution_receiver_arguments(&db, target, &arguments, implementation).is_err());
        let (actual_receiver, actual_arguments) = serialization_contribution_receiver_arguments_in_environment(
            &db,
            target,
            &arguments,
            implementation,
            &environment,
        )
        .unwrap();
        assert_eq!(actual_receiver, adapter);
        assert_eq!(actual_arguments.as_ref(), &[arguments[0].clone(), identity(reader), identity(field)]);
        let methods = serialization_target_contract_methods_in_environment(
            &db,
            actual_receiver,
            &actual_arguments,
            implementation,
            &environment,
        )
        .unwrap();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].0.arguments.as_ref(), &arguments);
        assert_eq!(methods[0].1.len(), 1);
        assert!(
            serialization_contribution_receiver_arguments_in_environment(
                &db,
                target,
                &arguments,
                implementation,
                &[environment[0].clone(), environment[0].clone()]
            )
            .is_err()
        );
        assert!(
            serialization_contribution_receiver_arguments_in_environment(
                &db,
                target,
                &arguments,
                implementation,
                &[environment[0].rebind("Foreign")]
            )
            .is_err()
        );
        let stale = AstNodeKey { generation: crate::SyntaxGenerationId(target.generation.0 + 1), ..target };
        assert!(
            serialization_contribution_receiver_arguments_in_environment(
                &db,
                stale,
                &arguments,
                implementation,
                &environment
            )
            .is_err()
        );
    }

    #[test]
    fn serialization_target_applies_record_arguments_to_contract_and_methods() {
        let (_root, db, assembly) = fixture();
        let owner = key(&db, &assembly, NodeKind::TypeDefinition, 0);
        let implementation = key(&db, &assembly, NodeKind::ImplBlock, 0);
        let args = [GenericSourceTypeIdentity::Abi(SemanticTypeId::I32)];
        let admitted = serialization_target_contract_methods(&db, owner, &args, implementation).unwrap();
        assert_eq!(admitted.len(), 1);
        assert_eq!(admitted[0].0.arguments.as_ref(), &args);
        assert_eq!(admitted[0].1.len(), 1);
        assert!(
            type_applied_contract_implementation_registered(
                &db,
                db.syntax_unit(owner.unit).unwrap(),
                owner,
                &admitted[0].0
            )
            .is_err()
        );
    }
    #[test]
    fn serialization_target_applies_enum_arguments_to_exact_external_impl() {
        let (_root, db, assembly) = fixture();
        let owner = key(&db, &assembly, NodeKind::EnumDefinition, 0);
        let implementation = key(&db, &assembly, NodeKind::ImplBlock, 1);
        let args = [GenericSourceTypeIdentity::Abi(SemanticTypeId::I32)];
        let admitted = serialization_target_contract_methods(&db, owner, &args, implementation).unwrap();
        assert_eq!(admitted[0].0.arguments.as_ref(), &args);
        assert_eq!(admitted[0].1.len(), 1);
    }

    #[test]
    fn serialization_target_rejects_arity_foreign_receiver_and_stale_keys() {
        let (_root, db, assembly) = fixture();
        let owner = key(&db, &assembly, NodeKind::TypeDefinition, 0);
        let implementation = key(&db, &assembly, NodeKind::ImplBlock, 0);
        assert!(serialization_target_contract_methods(&db, owner, &[], implementation).is_err());
        let args = [GenericSourceTypeIdentity::Abi(SemanticTypeId::I32)];
        let other = key(&db, &assembly, NodeKind::ImplBlock, 1);
        assert!(serialization_target_contract_methods(&db, owner, &args, other).is_err());
        let stale = AstNodeKey { generation: SyntaxGenerationId(owner.generation.0 + 1), ..owner };
        assert!(serialization_target_contract_methods(&db, stale, &args, implementation).is_err());
    }
}

pub(crate) fn serialization_encoder_methods(
    db: &dyn Db,
    owner: AstNodeKey,
    arguments: &[GenericSourceTypeIdentity],
) -> Result<Arc<[(AstNodeKey, AstNodeKey)]>, SemanticError> {
    let syntax = db
        .syntax_unit(owner.unit)
        .filter(|s| s.accepts_key(db, owner))
        .ok_or_else(|| SemanticError::unavailable("serialization_encoder.current_owner"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), owner.node)
        .ok_or_else(|| SemanticError::unavailable("serialization_encoder.owner"))?;
    let definition =
        node.of::<TypeDefinition>().ok_or_else(|| SemanticError::unavailable("serialization_encoder.record"))?;
    if definition.generics.len() != arguments.len() {
        return Err(SemanticError::new("serialization encoder argument arity differs"));
    }
    let substitutions =
        definition.generics.iter().zip(arguments).map(|(g, a)| (g.node.name.as_str(), a)).collect::<HashMap<_, _>>();
    let applications = type_contract_applications_registered(db, syntax, owner)?
        .ok_or_else(|| SemanticError::unavailable("serialization_encoder.applications"))?;
    let mut found = None;
    for application in applications.iter() {
        let Some(contract) = db.syntax_unit(application.declaration.unit).filter(|s| {
            s.accepts_key(db, application.declaration)
                && s.project(db) == syntax.project(db)
                && crate::canonical_corelib_source_path(db, application.declaration()).as_deref()
                    == Some(beskid_abi::runtime_source::CANONICAL_SERIALIZATION_CONTRACTS_SOURCE_PATH)
        }) else {
            continue;
        };
        if crate::corelib_source_authority::registered_declaration_name(db, application.declaration).as_deref()
            != Some("Encoder")
        {
            continue;
        }
        if !application.arguments.is_empty() || found.is_some() {
            return Err(SemanticError::new("serialization encoder application ambiguous"));
        }
        let _ = contract;
        found = Some(validated_contract_methods_with_expected(
            db,
            owner,
            application.declaration,
            &substitutions,
            &HashMap::new(),
        )?);
    }
    found.ok_or_else(|| SemanticError::new("serialization requires the exact canonical Encoder conformance"))
}

/// Derive receiver specialization from the exact generated implementation using
/// the retained target's generic environment, never caller-selected type names.
pub(crate) fn serialization_contribution_receiver_arguments(
    db: &dyn Db,
    target: AstNodeKey,
    arguments: &[GenericSourceTypeIdentity],
    implementation: AstNodeKey,
) -> Result<(AstNodeKey, Arc<[GenericSourceTypeIdentity]>), SemanticError> {
    serialization_contribution_receiver_arguments_in_environment(db, target, arguments, implementation, &[])
}

pub(crate) fn serialization_contribution_receiver_arguments_in_environment(
    db: &dyn Db,
    target: AstNodeKey,
    arguments: &[GenericSourceTypeIdentity],
    implementation: AstNodeKey,
    environment: &[GenericSubstitution],
) -> Result<(AstNodeKey, Arc<[GenericSourceTypeIdentity]>), SemanticError> {
    let syntax = db
        .syntax_unit(target.unit)
        .filter(|s| s.accepts_key(db, target))
        .ok_or_else(|| SemanticError::unavailable("serialization_target.current_target"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), target.node)
        .ok_or_else(|| SemanticError::unavailable("serialization_target.target"))?;
    let generics = node
        .of::<TypeDefinition>()
        .map(|d| &d.generics)
        .or_else(|| node.of::<EnumDefinition>().map(|d| &d.generics))
        .ok_or_else(|| SemanticError::unavailable("serialization_target.target_nominal"))?;
    if generics.len() != arguments.len() {
        return Err(SemanticError::new("serialization target argument arity differs"));
    }
    let mut substitutions =
        generics.iter().zip(arguments).map(|(g, a)| (g.node.name.as_str(), a)).collect::<HashMap<_, _>>();
    let implementation_syntax = db
        .syntax_unit(implementation.unit)
        .filter(|s| {
            s.accepts_key(db, implementation)
                && s.project(db) == syntax.project(db)
                && implementation.generation == target.generation
        })
        .ok_or_else(|| SemanticError::unavailable("serialization_target.current_contribution"))?;
    let block = implementation_syntax
        .syntax_index(db)
        .node_at(implementation_syntax.expanded_program(db), implementation.node)
        .and_then(|n| n.of::<ImplBlock>())
        .ok_or_else(|| SemanticError::unavailable("serialization_target.contribution"))?;
    let names = block.generics.iter().map(|parameter| parameter.node.name.as_str()).collect::<Vec<_>>();
    if names.iter().enumerate().any(|(index, name)| names[..index].contains(name)) {
        return Err(SemanticError::new("serialization receiver generic scope is ambiguous"));
    }
    for (index, binding) in environment.iter().enumerate() {
        if !names.contains(&binding.parameter.as_ref())
            || environment[..index].iter().any(|previous| previous.parameter == binding.parameter)
        {
            return Err(SemanticError::new("serialization receiver has foreign or repeated environment parameter"));
        }
        if let Some(previous) = substitutions.insert(binding.parameter.as_ref(), binding.source_identity()) {
            if previous != binding.source_identity() {
                return Err(SemanticError::new("serialization receiver environment changes issued target arguments"));
            }
        }
    }
    let receiver = super::portable_nominal::serialization_impl_receiver(db, implementation)
        .ok_or_else(|| SemanticError::unavailable("serialization_target.receiver"))?;
    let GenericSourceTypeIdentity::Nominal { qualified_name, arguments } =
        generic_source_type_identity_with_substitutions(db, implementation, &block.receiver_type.node, &substitutions)?
    else {
        return Err(SemanticError::new("serialization contribution receiver is not nominal"));
    };
    if stable_declaration_identity(db, receiver).as_ref() != Some(&qualified_name) {
        return Err(SemanticError::new("serialization contribution receiver identity differs"));
    }
    Ok((receiver, arguments))
}
