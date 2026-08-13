use std::collections::HashMap;

use beskid_isle::{AstNodeKey, DirectCallee};
use beskid_queries::{
    CallLowering, GenericSpecializationInstance, GenericSubstitution, call_lowering, child_nodes,
    generic_call_specialization, generic_call_template, generic_specialization_identity,
    generic_specialization_instance, item_abi_signature, node_kind,
};

use super::contracts::{SyntaxModuleEmissionError, emission_verification};
use super::items::{ResolvedSyntaxModuleItem, SyntaxModuleItem};
use super::trace::{format_declaration_for_trace, trace_key};
use crate::CodegenInput;

pub(super) fn resolve_module_items(
    input: &CodegenInput<'_>,
    source_items: &[SyntaxModuleItem],
) -> Result<Vec<ResolvedSyntaxModuleItem>, SyntaxModuleEmissionError> {
    let db = input.database();
    let mut specializations = HashMap::<AstNodeKey, Vec<GenericSpecializationInstance>>::new();
    for item in source_items {
        // A generic declaration body has no concrete substitution environment of its own.
        // Walking it once here falsely treats `T`-dependent call sites as executable source and
        // can reject a program before a real direct-call instantiation reaches it. Only concrete
        // entry items seed the collection; each emitted generic item is represented solely by a
        // call-derived `DirectCallee::SpecializedItem` identity below.
        if is_concrete_executable_item(db, item.key)? {
            collect_generic_call_specializations(db, item.key, &mut specializations).map_err(|error| {
                emission_verification(format!(
                    "generic specialization collection failed for {}: {error}",
                    format_declaration_for_trace(db, item.key)
                ))
            })?;
        }
    }
    // Also collect specializations from entry-point roots (test files) that may
    // call generic functions defined in this module with concrete type arguments.
    for root in input.roots() {
        collect_generic_call_specializations(db, *root, &mut specializations).map_err(|error| {
            emission_verification(format!(
                "generic specialization collection failed for root {}: {error}",
                format_declaration_for_trace(db, *root)
            ))
        })?;
    }

    let mut resolved = Vec::with_capacity(source_items.len());
    for item in source_items {
        if item_abi_signature(db, item.key).ok().flatten().is_some() {
            resolved.push(ResolvedSyntaxModuleItem {
                key: item.key,
                symbol: item.symbol.clone(),
                callee: DirectCallee::item(item.key),
                specialization: None,
            });
            continue;
        }
        let kind = node_kind(db, item.key).map_err(|error| emission_verification(error.to_string()))?;
        if !matches!(
            kind,
            Some(
                beskid_queries::IndexedNodeKind::FunctionDefinition | beskid_queries::IndexedNodeKind::MethodDefinition
            )
        ) {
            // Type and enum declarations carry source layout facts but have no executable
            // syntax body. They deliberately do not require a call-derived function ABI.
            continue;
        }
        let Some(signatures) = specializations.get(&item.key) else {
            // Generic declarations do not have an executable ABI on their own. They enter a
            // module only when the same source traversal has proven a concrete direct-call ABI.
            // A direct generic call without that proof is rejected while collecting below, so
            // this is only an uncalled declaration (for example a Corelib helper outside the
            // selected entrypoint's call graph).
            continue;
        };
        for specialization in signatures {
            let identity = generic_specialization_identity(specialization);
            resolved.push(ResolvedSyntaxModuleItem {
                key: item.key,
                symbol: format!("{}#generic_{}", item.symbol, specialization_mangle(specialization)),
                callee: DirectCallee::specialized_item(item.key, identity),
                specialization: Some(specialization.clone()),
            });
        }
    }
    Ok(resolved)
}

/// Return true only for a declaration body with a declaration-level, non-generic ABI.
///
/// Absence of an ABI is *not* treated as generic generally: non-function syntax declarations
/// are structural and have no executable body. Keeping this predicate explicit is the boundary
/// that prevents the specialization collector from scanning generic source as concrete code.
fn is_concrete_executable_item(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
) -> Result<bool, SyntaxModuleEmissionError> {
    match item_abi_signature(db, key) {
        Ok(Some(_)) => Ok(true),
        // Generic functions and generic-owner methods are both intentionally ABI-less until a
        // direct call has supplied their immutable specialization environment.
        Ok(None) => Ok(false),
        Err(error) => Err(emission_verification(format!(
            "item ABI signature is unavailable for {}: {error}",
            format_declaration_for_trace(db, key)
        ))),
    }
}

fn collect_generic_call_specializations(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    specializations: &mut HashMap<AstNodeKey, Vec<GenericSpecializationInstance>>,
) -> Result<(), SyntaxModuleEmissionError> {
    collect_generic_call_specializations_in_environment(db, key, None, specializations)
}

/// Traverse an executable source body with an optional immutable enclosing specialization.
/// Nested calls of the explicit `inner<T>(...)` form are materialized using the enclosing
/// bindings, then their body is queued in the same pass. This replaces the old diagnostic-only
/// guard with a finite `(declaration, substitutions)` worklist.
fn collect_generic_call_specializations_in_environment(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    environment: Option<&GenericSpecializationInstance>,
    specializations: &mut HashMap<AstNodeKey, Vec<GenericSpecializationInstance>>,
) -> Result<(), SyntaxModuleEmissionError> {
    if environment.is_none()
        && matches!(
            node_kind(db, key).map_err(|error| emission_verification(error.to_string()))?,
            Some(
                beskid_queries::IndexedNodeKind::FunctionDefinition | beskid_queries::IndexedNodeKind::MethodDefinition
            )
        )
    {
        match item_abi_signature(db, key) {
            Ok(Some(_)) => {}
            Ok(None) => {
                // Program roots contain every declaration in an assembled unit. A generic
                // declaration body is not executable until a concrete call supplies its immutable
                // environment; that recursive path re-enters this collector with an environment.
                return Ok(());
            }
            Err(error) if error.is_unavailable() => {
                // Root discovery is supplemental: selected executable items already fail closed
                // in `is_concrete_executable_item`. An unrelated concrete declaration whose ABI
                // has not been ported cannot prove a generic specialization and is skipped here.
                return Ok(());
            }
            Err(error) => {
                return Err(emission_verification(format!(
                    "root item ABI classification failed for {}: {error}",
                    format_declaration_for_trace(db, key),
                )));
            }
        }
    }
    if let Some(declaration) = direct_generic_call_declaration(db, key).map_err(|error| {
        emission_verification(format!(
            "generic call analysis failed at {}: {error}",
            beskid_queries::format_ast_node_site(db, key)
        ))
    })? {
        let specialization = if let Some(template) =
            generic_call_template(db, key).map_err(|error| emission_verification(error.to_string()))?
        {
            let enclosing = environment.ok_or_else(|| {
                emission_verification(format!(
                    "generic call template has no enclosing specialization: call={} declaration={}",
                    trace_key(db, key),
                    format_declaration_for_trace(db, declaration)
                ))
            })?;
            let bindings = template
                .parameters
                .iter()
                .zip(template.parameter_arguments.iter())
                .map(|(target, argument)| {
                    enclosing
                        .substitutions
                        .iter()
                        .find(|binding| binding.parameter.as_ref() == argument.as_ref())
                        .cloned()
                        .map(|binding| GenericSubstitution { parameter: target.clone(), argument: binding.argument })
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    emission_verification(format!(
                        "nested generic call references an unbound parameter: call={} declaration={}",
                        trace_key(db, key),
                        format_declaration_for_trace(db, declaration)
                    ))
                })?;
            generic_specialization_instance(db, template.declaration, bindings.into())
                .map_err(|error| emission_verification(error.to_string()))?
                .ok_or_else(|| emission_verification("nested generic specialization is unavailable"))?
        } else {
            let specialization = generic_call_specialization(db, key)
                .map_err(|error| {
                    emission_verification(format!(
                        "generic specialization facts are unavailable at {} for declaration {}: {error}",
                        beskid_queries::format_ast_node_site(db, key),
                        format_declaration_for_trace(db, declaration),
                    ))
                })?
                .ok_or_else(|| {
                    emission_verification(format!(
                        "generic direct call has no provable ABI specialization: call={} declaration={}",
                        trace_key(db, key),
                        format_declaration_for_trace(db, declaration)
                    ))
                })?;
            GenericSpecializationInstance {
                declaration: specialization.declaration,
                signature: specialization.signature,
                substitutions: specialization.substitutions,
            }
        };
        if specialization.declaration != declaration {
            return Err(emission_verification(format!(
                "generic direct call specialization resolved a different declaration: call={} expected={} actual={}",
                trace_key(db, key),
                format_declaration_for_trace(db, declaration),
                format_declaration_for_trace(db, specialization.declaration),
            )));
        }
        let instances = specializations.entry(declaration).or_default();
        if !instances.contains(&specialization) {
            instances.push(specialization.clone());
            // Only a newly discovered instance can add new nested work. This makes recursive
            // generic helpers finite without relying on a source-body traversal order.
            collect_generic_call_specializations_in_environment(
                db,
                declaration,
                Some(&specialization),
                specializations,
            )?;
        }
    }
    if let Some(children) = child_nodes(db, key).map_err(|error| emission_verification(error.to_string()))? {
        for child in children.iter().copied() {
            collect_generic_call_specializations_in_environment(db, child, environment, specializations)?;
        }
    }
    Ok(())
}

/// Returns the declaration only for a source call that has resolved directly to a generic
/// function with no declaration-level ABI. This keeps module emission constrained to the same
/// semantic call facts that ISLE uses for `DirectCallee::SpecializedItem`; unresolved calls stay
/// unavailable rather than being guessed from syntax.
fn direct_generic_call_declaration(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
) -> Result<Option<AstNodeKey>, SyntaxModuleEmissionError> {
    if node_kind(db, key).map_err(|error| emission_verification(error.to_string()))?
        != Some(beskid_queries::IndexedNodeKind::CallExpression)
    {
        return Ok(None);
    }
    let lowering = match call_lowering(db, key) {
        Ok(Some(lowering)) => lowering,
        Ok(None) => return Ok(None),
        // `generic_call_specialization` deliberately leaves unavailable call sites out of the
        // fact set (for example unresolved Core.Output paths). They cannot prove a direct
        // generic declaration and must not broaden module emission.
        Err(error) if error.is_unavailable() => return Ok(None),
        Err(error) => return Err(emission_verification(error.to_string())),
    };
    let CallLowering::Direct(declaration) = lowering else {
        return Ok(None);
    };
    if !matches!(
        node_kind(db, declaration).map_err(|error| emission_verification(error.to_string()))?,
        Some(beskid_queries::IndexedNodeKind::FunctionDefinition | beskid_queries::IndexedNodeKind::MethodDefinition)
    ) {
        return Ok(None);
    }
    match item_abi_signature(db, declaration) {
        Ok(Some(_)) => Ok(None),
        // Function definitions are ABI-less only when generic. The generic call fact below
        // must now prove one exact ABI shape, otherwise the caller is rejected fail-closed.
        Ok(None) => Ok(Some(declaration)),
        // An unavailable ABI on a non-generic declaration is not evidence of specialization.
        // It must remain unavailable rather than entering this collector under a false identity.
        Err(error) if error.is_unavailable() => Ok(None),
        Err(error) => Err(emission_verification(error.to_string())),
    }
}

fn specialization_mangle(instance: &GenericSpecializationInstance) -> String {
    generic_specialization_identity(instance).iter().map(u32::to_string).collect::<Vec<_>>().join("_")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, RootEntry, SourceUnit, SyntaxProgramAssembly,
    };
    use beskid_analysis::services::parse_program_with_source_name;
    use beskid_queries::{
        AstNodeId, BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId, build_typed_program,
    };

    use super::*;

    #[test]
    fn concrete_specialization_scans_nested_generic_template_with_its_environment() {
        let source = "unit Inner<T>(T value) { return; } unit Outer<T>(T value) { Inner<T>(value); return; } unit Main() { Outer(1); return; }";
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let source_path = directory.join("Main.bd");
        std::fs::write(&source_path, source).expect("source");
        let program =
            parse_program_with_source_name(source_path.to_str().expect("source path"), source).expect("parse source");
        let entry = SourceUnitId::new(&db, source_path.clone());
        let project = ProjectSession::new(&db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
        let generation = SyntaxGenerationId(1);
        let assembly = Arc::new(SyntaxProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit {
                logical_name: "Main".into(),
                path: source_path,
                source: source.into(),
                program,
            }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
        ));
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
        let root = AstNodeKey { unit: entry, generation, node: AstNodeId(0) };
        let declarations = function_definitions(&db, root);

        let mut specializations = HashMap::new();
        collect_generic_call_specializations(&db, root, &mut specializations)
            .expect("concrete outer call supplies the environment for its nested generic call");

        assert_eq!(specializations.get(&declarations[0]).map(Vec::len), Some(1), "Inner<i32>");
        assert_eq!(specializations.get(&declarations[1]).map(Vec::len), Some(1), "Outer<i32>");
        assert!(!specializations.contains_key(&declarations[2]), "concrete Main is not a specialization");
    }

    fn function_definitions(db: &dyn beskid_queries::Db, key: AstNodeKey) -> Vec<AstNodeKey> {
        let mut found = Vec::new();
        if node_kind(db, key).expect("node kind") == Some(beskid_queries::IndexedNodeKind::FunctionDefinition) {
            found.push(key);
        }
        if let Some(children) = child_nodes(db, key).expect("child nodes") {
            for child in children.iter().copied() {
                found.extend(function_definitions(db, child));
            }
        }
        found
    }
}
