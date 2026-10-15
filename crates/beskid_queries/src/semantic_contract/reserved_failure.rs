//! Constructor authority for preadmission; this grants no callgraph allocation effect.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservedFailureConstructor {
    factory: AstNodeKey,
    result: AstNodeKey,
    failure: AstNodeKey,
}
impl ReservedFailureConstructor {
    pub fn factory(&self) -> AstNodeKey {
        self.factory
    }
    pub fn result(&self) -> AstNodeKey {
        self.result
    }
    pub fn failure(&self) -> AstNodeKey {
        self.failure
    }
}

#[derive(Clone, Copy)]
struct FailureFamily {
    path: &'static str,
    factory: &'static str,
    error: &'static str,
    variant: &'static str,
}
fn failure_families() -> [FailureFamily; 2] {
    use beskid_abi::runtime_source::*;
    [
        FailureFamily {
            path: CANONICAL_SERIALIZATION_ERRORS_SOURCE_PATH,
            factory: "AllocationFailureResult",
            error: "SerializationError",
            variant: "AllocationFailure",
        },
        FailureFamily {
            path: CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH,
            factory: "ReserveAllocationFailureV1",
            error: "DynamicErrorV1",
            variant: "Allocation",
        },
    ]
}

fn canonical_source_without_registry(db: &dyn Db, key: AstNodeKey) -> bool {
    db.syntax_unit(key.unit).is_some_and(|unit| {
        unit.generation(db) == key.generation
            && unit.revision(db).runtime_source_authority.as_deref()
                == Some(beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH)
    })
}
fn canonical_source(db: &dyn Db, key: AstNodeKey, path: &str) -> bool {
    if path == beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH {
        return db.syntax_unit(key.unit).is_some_and(|unit| {
            unit.accepts_key(db, key) && unit.revision(db).runtime_source_authority.as_deref() == Some(path)
        });
    }
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .corelib_source_paths
        .get(&(key.unit, key.generation))
        .is_some_and(|actual| actual.as_str() == path)
}

/// Issue only the immutable canonical factory's exact Error(AllocationFailure) constructor.
/// Copied source, foreign Results, stale keys and same-layout enums cannot issue this fact.
pub fn reserved_failure_constructor(
    db: &dyn Db,
    key: AstNodeKey,
    substitutions: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<ReservedFailureConstructor> {
    use beskid_abi::runtime_source::CANONICAL_FOUNDATION_RESULTS_SOURCE_PATH;
    let Some(syntax) = db.syntax_unit(key.unit) else {
        return Ok(None);
    };
    let Some(family) = failure_families().into_iter().find(|family| canonical_source(db, key, family.path)) else {
        return Ok(None);
    };
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    let factory = with_node(db, syntax, key, |program, index, _| {
        let mut parent = parent_node(index, key.node);
        while let Some(node) = parent {
            if let Some(function) = index.node_at(program, node)?.of::<beskid_analysis::syntax::FunctionDefinition>() {
                return (function.name.node.name == family.factory).then_some(AstNodeKey { node, ..key });
            }
            parent = parent_node(index, node);
        }
        None
    })?;
    let Some(factory) = factory else {
        return Ok(None);
    };
    let specialized = enum_constructor_specialization(db, key, substitutions)?;
    let result = if let Some(fact) = &specialized {
        fact.constructor.clone()
    } else if let Some(fact) = enum_constructor(db, key)? {
        fact
    } else {
        return Ok(None);
    };
    if !canonical_source(db, result.declaration, CANONICAL_FOUNDATION_RESULTS_SOURCE_PATH) {
        return Ok(None);
    }
    let [failure] = result.payloads.as_ref() else {
        return Ok(None);
    };
    let Some(error) = enum_constructor(db, *failure)? else {
        return Ok(None);
    };
    if !canonical_source(db, error.declaration, family.path) || !error.payloads.is_empty() {
        return Ok(None);
    }
    let result_layout = if let Some(fact) = specialized {
        fact.layout
    } else if let Some(layout) = enum_layout(db, key)? {
        layout
    } else {
        return Ok(None);
    };
    let Some(error_layout) = enum_layout(db, *failure)? else {
        return Ok(None);
    };
    if result_layout.variants.get(result.variant_index as usize).is_none_or(|variant| variant.name.as_ref() != "Error")
        || error_layout
            .variants
            .get(error.variant_index as usize)
            .is_none_or(|variant| variant.name.as_ref() != family.variant)
    {
        return Ok(None);
    }
    Ok(Some(ReservedFailureConstructor { factory, result: key, failure: *failure }))
}

/// Exact return-type and factory specialization for a checked entry. This is constructor
/// eligibility only: callers still require an independently issued finite effect closure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedFailureDestination {
    entry: AstNodeKey,
    constructor: ReservedFailureConstructor,
    factory: GenericSpecializationInstance,
}
impl CheckedFailureDestination {
    pub fn entry(&self) -> AstNodeKey {
        self.entry
    }
    pub fn constructor(&self) -> &ReservedFailureConstructor {
        &self.constructor
    }
    pub fn factory(&self) -> &GenericSpecializationInstance {
        &self.factory
    }
}

pub fn checked_failure_destination(
    db: &dyn Db,
    entry: AstNodeKey,
    substitutions: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<CheckedFailureDestination> {
    use beskid_abi::runtime_source::CANONICAL_FOUNDATION_RESULTS_SOURCE_PATH;
    use beskid_analysis::syntax::{EnumDefinition, FunctionDefinition, MethodDefinition};
    use beskid_analysis::syntax_query::NodeKind;
    let Some(syntax) = db.syntax_unit(entry.unit) else { return Ok(None) };
    if !syntax.accepts_key(db, entry) {
        return Ok(None);
    }
    let Some(node) = syntax.syntax_index(db).node_at(syntax.expanded_program(db), entry.node) else { return Ok(None) };
    let result_type = if let Some(function) = node.of::<FunctionDefinition>() {
        function.return_type.as_ref()
    } else if let Some(method) = node.of::<MethodDefinition>() {
        method.return_type.as_ref()
    } else {
        return Ok(None);
    };
    let Some(result_type) = result_type else { return Ok(None) };
    let environment = substitutions.iter().map(|value| (value.parameter.as_ref(), value.source_identity())).collect();
    let identity = generic_source_type_identity_with_substitutions(db, entry, &result_type.node, &environment)?;
    let GenericSourceTypeIdentity::Nominal { qualified_name, arguments } = identity else { return Ok(None) };
    if arguments.len() != 2 {
        return Ok(None);
    }
    // Only the compiler's registered current canonical units may supply either declaration.
    // Caller strings, copied source and a pointer-sized nominal cannot reconstruct these keys.
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    let mut units = registry
        .corelib_source_paths
        .iter()
        .filter_map(|((unit, generation), path)| {
            (path.as_str() == CANONICAL_FOUNDATION_RESULTS_SOURCE_PATH
                || failure_families().iter().any(|family| family.path == path.as_str()))
            .then_some((*unit, *generation, path.clone()))
        })
        .collect::<Vec<_>>();
    for (unit, generation) in registry.imports.keys() {
        let key = AstNodeKey { unit: *unit, generation: *generation, node: entry.node };
        if canonical_source_without_registry(db, key)
            && !units.iter().any(|(owner, epoch, _)| owner == unit && epoch == generation)
        {
            units.push((
                *unit,
                *generation,
                beskid_abi::runtime_source::CANONICAL_PUBLIC_DYNAMIC_SOURCE_PATH.to_owned(),
            ));
        }
    }
    drop(registry);
    let mut results = Vec::new();
    let mut errors = Vec::new();
    let mut factories = Vec::new();
    for (unit, generation, path) in units {
        let Some(owner) = db.syntax_unit(unit) else { continue };
        if owner.generation(db) != generation {
            continue;
        }
        let index = owner.syntax_index(db);
        for id in index.ids_of_kind(NodeKind::EnumDefinition) {
            let key = AstNodeKey { unit, generation, node: id };
            let Some(definition) =
                index.node_at(owner.expanded_program(db), id).and_then(|node| node.of::<EnumDefinition>())
            else {
                continue;
            };
            if path.as_str() == CANONICAL_FOUNDATION_RESULTS_SOURCE_PATH && definition.name.node.name == "Result" {
                results.push(key);
            }
            if failure_families()
                .iter()
                .any(|family| path.as_str() == family.path && definition.name.node.name == family.error)
            {
                errors.push((key, path.clone()));
            }
        }
        if let Some(family) = failure_families().into_iter().find(|family| family.path == path.as_str()) {
            for id in index.ids_of_kind(NodeKind::FunctionDefinition) {
                let Some(function) =
                    index.node_at(owner.expanded_program(db), id).and_then(|node| node.of::<FunctionDefinition>())
                else {
                    continue;
                };
                if function.name.node.name == family.factory
                    && function.parameters.is_empty()
                    && function.generics.len() == 1
                {
                    factories.push((
                        AstNodeKey { unit, generation, node: id },
                        function.generics[0].node.name.clone(),
                        path.clone(),
                    ));
                }
            }
        }
    }
    let [result] = results.as_slice() else { return Ok(None) };
    if stable_declaration_identity(db, *result).as_ref() != Some(&qualified_name) {
        return Ok(None);
    }
    let GenericSourceTypeIdentity::Nominal { qualified_name: error_name, arguments: error_arguments } = &arguments[1]
    else {
        return Ok(None);
    };
    if !error_arguments.is_empty() {
        return Ok(None);
    }
    let matching = errors
        .iter()
        .filter(|(error, _)| stable_declaration_identity(db, *error).as_ref() == Some(error_name))
        .cloned()
        .collect::<Vec<_>>();
    let [(_, path)] = matching.as_slice() else { return Ok(None) };
    let matching_factories = factories.iter().filter(|(_, _, owner)| owner == path).cloned().collect::<Vec<_>>();
    let [(factory, parameter, _)] = matching_factories.as_slice() else { return Ok(None) };
    let binding = GenericSubstitution::from_source(parameter.as_str(), arguments[0].abi_type(), arguments[0].clone());
    let factory_instance = GenericSpecializationInstance {
        declaration: *factory,
        declaration_identity: stable_declaration_identity(db, *factory)
            .ok_or_else(|| SemanticError::unavailable("checked_failure_destination"))?,
        signature: ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::POINTER },
        substitutions: Arc::from([binding]),
        contract_witnesses: Arc::from([]),
    };
    let owner =
        db.syntax_unit(factory.unit).ok_or_else(|| SemanticError::unavailable("checked_failure_destination"))?;
    let mut constructors = Vec::new();
    for id in owner.syntax_index(db).ids_of_kind(NodeKind::EnumConstructorExpression) {
        let key = AstNodeKey { node: id, ..*factory };
        if let Some(constructor) = reserved_failure_constructor(db, key, factory_instance.substitutions.clone())? {
            if constructor.factory() == *factory {
                constructors.push(constructor)
            }
        }
    }
    let [constructor] = constructors.as_slice() else { return Ok(None) };
    Ok(Some(CheckedFailureDestination { entry, constructor: constructor.clone(), factory: factory_instance }))
}

/// Current embedded Contracts unit only. This identifies the orchestration source;
/// concrete Encoder method eligibility is independently checked from its applied witness.
pub fn canonical_serialization_encode(db: &dyn Db, entry: AstNodeKey) -> bool {
    if !canonical_source(db, entry, beskid_abi::runtime_source::CANONICAL_SERIALIZATION_CONTRACTS_SOURCE_PATH) {
        return false;
    }
    let Some(unit) = db.syntax_unit(entry.unit).filter(|unit| unit.accepts_key(db, entry)) else {
        return false;
    };
    unit.syntax_index(db)
        .node_at(unit.expanded_program(db), entry.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .is_some_and(|function| {
            function.name.node.name == "Encode" && function.generics.len() == 2 && function.parameters.len() == 3
        })
}

/// Extract only the current applied E: Encoder witness of canonical Encode.
/// Method spellings are interpreted inside that exact embedded contract declaration.
pub fn serialization_publication_methods(
    db: &dyn Db,
    instance: &GenericSpecializationInstance,
) -> SemanticQueryResult<(AstNodeKey, AstNodeKey)> {
    if !canonical_serialization_encode(db, instance.declaration) {
        return Ok(None);
    }
    let mut pairs = Vec::new();
    for witness in instance.contract_witnesses.iter() {
        if !canonical_source(
            db,
            witness.contract,
            beskid_abi::runtime_source::CANONICAL_SERIALIZATION_CONTRACTS_SOURCE_PATH,
        ) || item_name(db, witness.contract)?.as_deref() != Some("Encoder")
        {
            continue;
        }
        if witness.position != 1
            || !contracts::validate_current_parameter_witness(
                db,
                instance.declaration,
                &instance.substitutions,
                witness,
            )?
        {
            return Ok(None);
        }
        let mut commit = Vec::new();
        let mut abort = Vec::new();
        for (signature, implementation) in witness.methods.iter() {
            match item_name(db, *signature)?.as_deref() {
                Some("Commit") => commit.push(*implementation),
                Some("Abort") => abort.push(*implementation),
                _ => {}
            }
        }
        let ([commit], [abort]) = (commit.as_slice(), abort.as_slice()) else {
            return Ok(None);
        };
        pairs.push((*commit, *abort));
    }
    let [pair] = pairs.as_slice() else {
        return Ok(None);
    };
    Ok(Some(*pair))
}
