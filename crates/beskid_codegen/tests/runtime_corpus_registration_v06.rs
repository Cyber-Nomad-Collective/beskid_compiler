use beskid_abi::{
    abi_v5::{AbiManifestV5, TargetMetadata},
    runtime_source::{
        CANONICAL_BOOTSTRAP_SOURCE_PATH, canonical_runtime_intrinsic_capability, canonical_runtime_sources,
    },
};
use beskid_analysis::{
    projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit},
    services::parse_program_with_source_name_and_diagnostics,
};
use beskid_codegen::CodegenInput;
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, SourceUnitId, SyntaxGenerationId, build_canonical_runtime_typed_program,
    node_kind, project_session_for_syntax_assembly,
};
use std::{path::PathBuf, sync::Arc};

fn descendants(db: &BeskidDatabase, key: AstNodeKey, nodes: &mut Vec<AstNodeKey>) {
    nodes.push(key);
    if let Some(children) = beskid_queries::child_nodes(db, key).unwrap() {
        for child in children.iter().copied() {
            descendants(db, child, nodes);
        }
    }
}

#[test]
fn v06_canonical_runtime_registration_preserves_every_source_root() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime/beskid");
    let sources = canonical_runtime_sources();
    let units = sources
        .into_iter()
        .map(|source| {
            let path = root.join(&source.logical_path);
            let parsed =
                parse_program_with_source_name_and_diagnostics(path.to_str().unwrap(), &source.source).unwrap();
            assert!(!parsed.recovered && parsed.diagnostics.is_empty(), "{}: {:?}", path.display(), parsed.diagnostics);
            SourceUnit {
                logical_name: source.logical_path,
                origin_path: path.clone(),
                path,
                source: source.source,
                program: parsed.program,
            }
        })
        .collect::<Vec<_>>();
    let entry = units.iter().position(|unit| unit.logical_name == CANONICAL_BOOTSTRAP_SOURCE_PATH).unwrap();
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.join("src") },
            dependencies: vec![RootEntry { dependency_name: Some("canonical-corelib".into()), source_root: root }],
        },
        Arc::new(units),
        entry,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_syntax_assembly(&db, &assembly, "beskid-runtime-native", "canonical-runtime").unwrap();
    let target =
        TargetMetadata::supported().into_iter().find(|target| target.triple == "aarch64-apple-darwin".into()).unwrap();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let mut altered_assembly = assembly.as_ref().clone();
    let mut altered_units = assembly.units.as_ref().clone();
    altered_units
        .iter_mut()
        .find(|unit| unit.logical_name == "Core/Hash/Sha256.bd")
        .unwrap()
        .program
        .node
        .items
        .clear();
    altered_assembly.units = Arc::new(altered_units);
    let altered = build_canonical_runtime_typed_program(
        &mut db,
        project,
        generation,
        Arc::new(altered_assembly),
        canonical_runtime_intrinsic_capability(&manifest).unwrap(),
    );
    assert!(altered.is_err(), "unchanged embedded bytes cannot authorize an altered support AST");
    let mut duplicated_assembly = assembly.as_ref().clone();
    let mut duplicated_units = assembly.units.as_ref().clone();
    let sha = duplicated_units.iter().position(|unit| unit.logical_name == "Core/Hash/Sha256.bd").unwrap();
    duplicated_units[sha] = duplicated_units[0].clone();
    duplicated_assembly.units = Arc::new(duplicated_units);
    assert!(
        build_canonical_runtime_typed_program(
            &mut db,
            project,
            generation,
            Arc::new(duplicated_assembly),
            canonical_runtime_intrinsic_capability(&manifest).unwrap(),
        )
        .is_err(),
        "duplicating a verified unit cannot replace a required dependency"
    );
    let capability = canonical_runtime_intrinsic_capability(&manifest).unwrap();
    let typed =
        build_canonical_runtime_typed_program(&mut db, project, generation, assembly.clone(), capability).unwrap();
    let roots = assembly
        .units
        .iter()
        .map(|unit| {
            let root = AstNodeKey { unit: SourceUnitId::new(&db, unit.path.clone()), generation, node: AstNodeId(0) };
            assert!(
                node_kind(&db, root).unwrap().is_some(),
                "unregistered root for {} ({:?})",
                unit.path.display(),
                root
            );
            root
        })
        .collect::<Vec<_>>();
    let mut unresolved = Vec::new();
    for root in &roots {
        for import in beskid_queries::unresolved_imports(&db, *root).unwrap().unwrap().iter() {
            unresolved.push(format!("{}: {}", root.unit.path(&db).display(), import.path));
        }
    }
    assert!(unresolved.is_empty(), "canonical runtime dependencies must resolve:\n{}", unresolved.join("\n"));
    let records = roots.iter().find(|root| root.unit.path(&db).ends_with("Runtime/Dynamic/Records.bd")).unwrap();
    let mut nodes = Vec::new();
    descendants(&db, *records, &mut nodes);
    let input = CodegenInput::new(&db, typed, roots.clone().into(), target, manifest)
        .expect("all exact embedded runtime source roots remain admitted");
    let process = roots.iter().find(|root| root.unit.path(&db).ends_with("Runtime/Host/Process.bd")).unwrap();
    let mut process_nodes = Vec::new();
    descendants(&db, *process, &mut process_nodes);
    let environment = process_nodes
        .iter()
        .copied()
        .find(|key| beskid_queries::item_name(&db, *key).ok().flatten().as_deref() == Some("ChildEnvironment"))
        .unwrap();
    let mut calls = Vec::new();
    descendants(&db, environment, &mut calls);
    for key in calls
        .into_iter()
        .filter(|key| node_kind(&db, *key).unwrap() == Some(beskid_queries::IndexedNodeKind::CallExpression))
    {
        let name = beskid_queries::runtime_intrinsic_name(&db, key).unwrap().unwrap();
        if name.0.as_ref() == "StringViewData" || name.0.as_ref() == "StringViewLength" {
            let lowering = beskid_queries::call_lowering(&db, key);
            assert!(
                matches!(lowering, Ok(Some(beskid_queries::CallLowering::Direct(_)))),
                "exact private canonical helper must resolve: {name:?}: {lowering:?}"
            );
        }
    }
    let array = roots.iter().find(|root| root.unit.path(&db).ends_with("Core/Collections/Array.bd")).unwrap();
    let mut array_nodes = Vec::new();
    descendants(&db, *array, &mut array_nodes);
    let empty = array_nodes
        .iter()
        .copied()
        .find(|key| beskid_queries::item_name(&db, *key).ok().flatten().as_deref() == Some("Empty"))
        .unwrap();
    let mut empty_nodes = Vec::new();
    descendants(&db, empty, &mut empty_nodes);
    let allocation = empty_nodes
        .into_iter()
        .find(|key| node_kind(&db, *key).unwrap() == Some(beskid_queries::IndexedNodeKind::CallExpression))
        .unwrap();
    assert!(
        beskid_queries::typed_array_allocation(&db, allocation).unwrap().is_some(),
        "exact embedded ordinary Array support must retain typed allocation authority"
    );
    let mut concrete_arrays = 0;
    for root in &roots {
        let mut calls = Vec::new();
        descendants(&db, *root, &mut calls);
        for call in calls
            .into_iter()
            .filter(|key| node_kind(&db, *key).unwrap() == Some(beskid_queries::IndexedNodeKind::CallExpression))
        {
            let Some(template) = beskid_queries::generic_call_specialization(&db, call).ok().flatten() else {
                continue;
            };
            let Some(instance) = beskid_queries::generic_call_specialization_instance(&db, template).ok().flatten()
            else {
                continue;
            };
            if instance.declaration == empty {
                concrete_arrays += 1;
                let plan = input.typed_array_static_plan(allocation, Some(&instance));
                assert!(plan.is_some(), "actual canonical Array.Empty specialization must allocate: {instance:?}");
            }
        }
    }
    assert!(concrete_arrays > 0, "runtime uses actual typed Array.Empty calls");
    for entry_name in [
        "DynamicI8BoxV1",
        "DynamicI16BoxV1",
        "DynamicI32BoxV1",
        "DynamicI64BoxV1",
        "DynamicU8BoxV1",
        "DynamicU16BoxV1",
        "DynamicU32BoxV1",
        "DynamicU64BoxV1",
        "DynamicF32BoxV1",
        "DynamicF64BoxV1",
        "DynamicBoolBoxV1",
        "DynamicCharBoxV1",
        "DynamicWordBoxV1",
        "DynamicStringBoxV1",
        "DynamicBytesBoxV1",
        "DynamicUnitBoxV1",
    ] {
        let entry = nodes
            .iter()
            .copied()
            .find(|key| {
                node_kind(&db, *key).unwrap() == Some(beskid_queries::IndexedNodeKind::FunctionDefinition)
                    && beskid_queries::item_name(&db, *key).unwrap().as_deref() == Some(entry_name)
            })
            .unwrap();
        let mut body = Vec::new();
        descendants(&db, entry, &mut body);
        let instance = body
            .into_iter()
            .find_map(|key| {
                let call = beskid_queries::generic_call_specialization(&db, key).ok().flatten()?;
                let instance = beskid_queries::generic_call_specialization_instance(&db, call).unwrap()?;
                (beskid_queries::item_name(&db, instance.declaration).unwrap().as_deref() == Some("DynamicBoxTypedV1"))
                    .then_some(instance)
            })
            .expect("canonical i8 entry issues its actual generic box specialization");
        let mut helper = Vec::new();
        descendants(&db, instance.declaration, &mut helper);
        let literal = helper
            .into_iter()
            .find(|key| node_kind(&db, *key).unwrap() == Some(beskid_queries::IndexedNodeKind::StructLiteralExpression))
            .unwrap();
        let layout = beskid_queries::aggregate_literal_specialization(&db, literal, instance.substitutions.clone());
        assert!(
            matches!(layout, Ok(Some(_))),
            "actual Dynamic i8 box specialization must supply an aggregate layout: {layout:?}"
        );
        let plan = input.aggregate_static_plan_for_specialization(literal, Some(&instance));
        assert!(
            plan.as_ref().and_then(|plan| plan.descriptor_getter.as_ref()).is_some(),
            "actual {entry_name} must publish its exact canonical descriptor getter"
        );
        if entry_name == "DynamicUnitBoxV1" {
            let unit = plan.as_ref().expect("unit box allocation plan");
            assert_eq!(unit.object_size, 16);
            assert!(unit.pointer_map_offsets.is_empty());
            assert_eq!(unit.fields.len(), 1);
            assert_eq!(unit.fields[0].abi_type, beskid_queries::SemanticTypeId::UNIT);
        }
        assert!(plan.is_some(), "actual {entry_name} specialization must issue descriptor-backed allocation metadata");
    }
}
