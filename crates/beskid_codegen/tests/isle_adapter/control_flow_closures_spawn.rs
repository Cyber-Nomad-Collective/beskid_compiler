use super::support::{
    AbiManifestV5, Arc, AstNodeId, AstNodeKey, BeskidDatabase, CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH,
    CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH, CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH, CodegenInput, DirectCallee,
    JITBuilder, JITModule, Linkage, Ordering, ProjectSession, SourceUnitId, SyntaxGenerationId, SyntaxModuleItem,
    TEST_CURRENT_TLS, TargetMetadata, build_canonical_runtime_typed_program, canonical_runtime_intrinsic_capability,
    canonical_runtime_test_assembly, default_libcall_names, emit_closure_static_data, emit_isle_item,
    emit_syntax_program, find_definition_of_kind, find_function_definition, find_function_definitions, find_node,
    find_nodes_of_kind, isa, item_fixture, item_fixture_with_root, item_name, lower_syntax_program, settings,
    test_system_allocate, test_tls_get,
};

#[test]
fn parsed_zero_capture_immediate_lambda_call_lowers_without_a_runtime_closure() {
    let (input, isa, root) = item_fixture_with_root("i32 Main() { return ((i32 value) => value + 1)(41); }");
    let db = input.database();
    let item = find_function_definition(db, root).expect("Main item");
    let call = find_node(db, root, beskid_queries::IndexedNodeKind::CallExpression).expect("immediate lambda call");
    let target =
        beskid_queries::closure_call_target(db, call).expect("closure call target").expect("immediate lambda target");
    let environment = beskid_queries::closure_environment(db, target.lambda)
        .expect("closure environment")
        .expect("lambda environment");
    assert!(environment.captures.is_empty());
    assert_eq!(environment.parameters.len(), 1);
    assert!(
        beskid_queries::local_slot(db, environment.parameters[0]).expect("lambda parameter slot query").is_some(),
        "lambda parameter must have a generation-safe local slot"
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("zero-capture immediate lambda call lowers through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 41"), "{clif}");
    assert!(clif.contains("iadd"), "{clif}");
}

#[test]
fn parsed_zero_capture_stored_lambda_call_lowers_through_generation_bound_local_callable_fact() {
    let (input, isa, root) =
        item_fixture_with_root("i32 Main() { let add = (i32 value) => value + 1; return add(41); }");
    let db = input.database();
    let item = find_function_definition(db, root).expect("Main item");
    let call = find_node(db, root, beskid_queries::IndexedNodeKind::CallExpression).expect("stored lambda call");
    let target = beskid_queries::closure_call_target(db, call)
        .expect("closure call target query")
        .expect("stored lambda target");
    let environment = beskid_queries::closure_environment(db, target.lambda)
        .expect("closure environment")
        .expect("lambda environment");
    assert!(environment.captures.is_empty());
    assert_eq!(environment.parameters.len(), 1);

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("stored zero-capture lambda call lowers through syntax facts");
    let clif = function.display().to_string();
    assert!(clif.contains("iconst.i32 41"), "{clif}");
    assert!(clif.contains("iadd"), "{clif}");
}

#[test]
fn closure_static_plan_is_generation_bound_and_never_claims_tls_or_root_frame_authority() {
    let (input, _isa, root) = item_fixture_with_root(
        "i32 Main(i32 count, string label) { let scalar = () => count; let pointer = () => label; return scalar(); }",
    );
    let lambdas = find_nodes_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::LambdaExpression);
    assert_eq!(lambdas.len(), 2, "fixture must retain both capture shapes");

    let scalar =
        input.closure_static_plan(lambdas[0]).expect("current scalar capture receives a static descriptor plan");
    assert_eq!(scalar.descriptor_symbol, "__beskid_closure_descriptor_module_u0_g21_n20");
    assert_eq!(scalar.pointer_map_symbol, "__beskid_closure_pointer_map_module_u0_g21_n20");
    assert_eq!(scalar.allocation_request_symbol, "__beskid_closure_allocation_request_module_u0_g21_n20");
    assert_eq!(scalar.object_size, 24, "16-byte header plus aligned i32 field");
    assert_eq!(scalar.object_alignment, 8);
    assert!(scalar.pointer_map_offsets.is_empty());
    assert_eq!(scalar.captures.len(), 1);
    assert_eq!(scalar.captures[0].pointer_map_index, None);
    assert!(scalar.runtime_root_context().is_none());

    let pointer =
        input.closure_static_plan(lambdas[1]).expect("current pointer capture receives a static descriptor plan");
    assert_eq!(pointer.object_size, 24, "16-byte header plus pointer field");
    assert_eq!(pointer.object_alignment, 8);
    assert_eq!(pointer.pointer_map_offsets.as_ref(), &[16]);
    assert_eq!(pointer.captures.len(), 1);
    assert_eq!(pointer.captures[0].field_offset, 16);
    assert_eq!(pointer.captures[0].pointer_map_index, Some(0));
    assert!(pointer.runtime_root_context().is_none());

    let mut static_module = JITModule::new(JITBuilder::new(default_libcall_names()).expect("JIT builder"));
    let data = emit_closure_static_data(&mut static_module, &pointer)
        .expect("static descriptor/request data materializes without runtime imports");
    assert_ne!(data.descriptor, data.pointer_map);
    assert_ne!(data.descriptor, data.allocation_request);
    static_module.finalize_definitions().expect("static closure data needs no root-helper or TLS relocation");

    let stale = AstNodeKey { generation: SyntaxGenerationId(lambdas[1].generation.0 + 1), ..lambdas[1] };
    assert!(
        input.closure_static_plan(stale).is_none(),
        "a stale syntax identity cannot receive static allocation authority"
    );
}

#[test]
fn closure_static_plan_rejects_stack_reference_captures() {
    let (input, _isa, root) =
        item_fixture_with_root("i32 Main(i32 count) { let mut mutable = count; return (() => mutable)(); }");
    let lambda = find_definition_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::LambdaExpression)
        .expect("capturing lambda");

    assert!(
        input.closure_static_plan(lambda).is_none(),
        "a stack-reference capture cannot receive static allocation authority"
    );
}

#[test]
fn parsed_capturing_immediate_lambda_call_lowers_through_abi_v5_closure_environment() {
    let (input, isa, item) = item_fixture("i32 Main(i32 outer) { return ((i32 value) => outer + value)(41); }");
    let db = input.database();
    let call = find_node(db, item, beskid_queries::IndexedNodeKind::CallExpression).expect("immediate lambda call");
    let target = beskid_queries::closure_call_target(db, call)
        .expect("closure call target query")
        .expect("immediate lambda target");
    let environment =
        beskid_queries::closure_environment(db, target.lambda).expect("environment query").expect("lambda environment");
    assert_eq!(environment.captures.len(), 1);
    let authority =
        input.closure_lowering_authority(call, target.lambda).expect("capturing call must receive closure authority");
    assert_eq!(authority.plan.captures.len(), 1);
    assert_eq!(authority.plan.captures[0].capture.slot.index, environment.captures[0].slot.index);
    let outer_decl = environment.captures[0].declaration;
    let outer_slot =
        beskid_queries::local_slot(db, outer_decl).expect("outer slot query").expect("outer parameter slot");
    assert_eq!(
        outer_slot.index, environment.captures[0].slot.index,
        "capture slot must match the outer parameter local slot"
    );
    let params = beskid_queries::item_abi_signature(db, item).expect("item abi").expect("main signature");
    assert_eq!(params.parameters.len(), 1);

    let function = match emit_isle_item(&input, isa.as_ref(), item) {
        Ok(function) => function,
        Err(error) => panic!(
            "capturing immediate lambda lowers through ABI-v5 allocate/store/root: {}",
            error.display_with_db(db)
        ),
    };
    let clif = function.display().to_string();
    assert!(clif.contains("beskid_rt_v5_closure_environment_allocate"), "{clif}");
    assert!(clif.contains("beskid_rt_v5_closure_environment_root_current"), "{clif}");
    assert!(clif.contains("__beskid_closure_allocation_request_"), "{clif}");
    assert!(!clif.contains("interop_dispatch_"), "{clif}");
    assert!(clif.contains("iadd"), "{clif}");
}

#[test]
fn closure_lowering_authority_reserves_root_slot_without_tls_pointer() {
    let (input, _isa, root) = item_fixture_with_root("i32 Main(i32 outer) { return (() => outer)(); }");
    let call =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::CallExpression).expect("immediate call");
    let lambda = find_definition_of_kind(input.database(), root, beskid_queries::IndexedNodeKind::LambdaExpression)
        .expect("capturing lambda");
    let authority =
        input.closure_lowering_authority(call, lambda).expect("current transferable capture receives root authority");
    assert_eq!(authority.root.root_helper, "beskid_rt_v5_closure_environment_root_current");
    assert!(authority.plan.runtime_root_context().is_none());
    assert!(authority.root.slot_index < 64);
}

#[test]
fn canonical_runtime_allocation_and_root_frame_helpers_emit_verified_clif_with_manifest_imports() {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("runtime project").keep();
    let (assembly, source_path) = canonical_runtime_test_assembly(&mut db, directory.as_ref());
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "beskid-runtime-native".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(31);
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("linux target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_canonical_runtime_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_runtime_intrinsic_capability(&manifest).expect("compiler authority"),
    )
    .expect("canonical runtime syntax facts");
    let native_root = AstNodeKey {
        unit: SourceUnitId::new(&*db, directory.join(CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH)),
        generation,
        node: AstNodeId(0),
    };
    let roots_root = AstNodeKey {
        unit: SourceUnitId::new(&*db, directory.join(CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH)),
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([native_root, roots_root]), target, manifest)
        .expect("canonical runtime codegen input");
    let isa = isa::lookup_by_name("x86_64")
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let items = [native_root, roots_root]
        .into_iter()
        .flat_map(|root| find_function_definitions(input.database(), root))
        .collect::<Vec<_>>();
    let selected = ["NativePointer", "SystemAllocate", "RootFramePrevious", "RootFrame"];
    let module_items = selected
        .into_iter()
        .map(|name| {
            let key = items
                .iter()
                .copied()
                .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some(name))
                .unwrap_or_else(|| panic!("canonical helper {name}"));
            SyntaxModuleItem { key, symbol: name.into() }
        })
        .collect::<Vec<_>>();

    let artifact = lower_syntax_program(&input, isa.as_ref(), &module_items)
        .expect("canonical helpers lower through the syntax-only module emitter");

    beskid_codegen::validate_artifact(&artifact)
        .expect("canonical helper imports are declared by the manifest authority");
    let imports = beskid_codegen::referenced_extern_imports(&artifact);
    assert!(imports.iter().any(|entry| entry.symbol == "beskid_rt_v5_intrinsic_system_allocate"));
    let root_frame =
        artifact.functions.iter().find(|function| function.name == "RootFrame").expect("RootFrame helper is lowered");
    assert!(
        root_frame.function.display().to_string().contains("load.i64"),
        "manifest-authorized raw_word_load is lowered inline through ISLE"
    );
    assert!(
        !imports.iter().any(|entry| entry.symbol == "beskid_rt_v5_intrinsic_raw_word_load"),
        "the inline load must not retain an unnecessary ABI import"
    );
    assert!(
        root_frame.function.display().to_string().contains("iadd"),
        "manifest-authorized pointer_add is lowered inline through ISLE"
    );
    assert!(
        !imports.iter().any(|entry| {
            matches!(
                entry.symbol.as_str(),
                "beskid_rt_v5_intrinsic_pointer_from_native_word" | "beskid_rt_v5_intrinsic_pointer_add"
            )
        }),
        "inline pointer conversions and arithmetic must not retain ABI imports"
    );
    assert_eq!(
        imports.iter().map(|entry| entry.symbol.as_str()).collect::<Vec<_>>(),
        ["beskid_rt_v5_intrinsic_system_allocate"],
        "only the still-external allocation primitive remains imported"
    );

    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let declared = emit_syntax_program(&mut module, &input, isa.as_ref(), &module_items, Linkage::Export)
        .expect("canonical runtime helpers define through the production module emitter");
    assert_eq!(declared.len(), module_items.len());
}

#[test]
#[cfg(any(all(target_os = "linux", target_arch = "x86_64"), all(target_os = "macos", target_arch = "aarch64"),))]
fn canonical_runtime_closure_descriptor_validation_and_rooting_execute_fail_closed() {
    let mut db = Box::new(BeskidDatabase::default());
    let directory = tempfile::tempdir().expect("runtime project").keep();
    let (assembly, source_path) = canonical_runtime_test_assembly(&mut db, directory.as_ref());
    let project = ProjectSession::new(
        &*db,
        directory.clone(),
        source_path.clone(),
        "beskid-runtime-native".into(),
        "lock".into(),
    );
    let generation = SyntaxGenerationId(32);
    let host_triple = if cfg!(target_os = "macos") { "aarch64-apple-darwin" } else { "x86_64-unknown-linux-gnu" };
    let host_isa_name = if cfg!(target_os = "macos") { "aarch64" } else { "x86_64" };
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == host_triple)
        .expect("host ABI-v5 target");
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let typed = build_canonical_runtime_typed_program(
        &mut db,
        project,
        generation,
        assembly,
        canonical_runtime_intrinsic_capability(&manifest).expect("compiler authority"),
    )
    .expect("canonical runtime syntax facts");
    let native_root = AstNodeKey {
        unit: SourceUnitId::new(&*db, directory.join(CANONICAL_BOOTSTRAP_NATIVE_SOURCE_PATH)),
        generation,
        node: AstNodeId(0),
    };
    let objects_root = AstNodeKey {
        unit: SourceUnitId::new(&*db, directory.join(CANONICAL_BOOTSTRAP_OBJECTS_SOURCE_PATH)),
        generation,
        node: AstNodeId(0),
    };
    let roots_root = AstNodeKey {
        unit: SourceUnitId::new(&*db, directory.join(CANONICAL_BOOTSTRAP_ROOTS_SOURCE_PATH)),
        generation,
        node: AstNodeId(0),
    };
    let leaked: &'static BeskidDatabase = Box::leak(db);
    let input = CodegenInput::new(leaked, typed, Arc::from([native_root, objects_root, roots_root]), target, manifest)
        .expect("canonical runtime codegen input");
    let isa = isa::lookup_by_name(host_isa_name)
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let items = [native_root, objects_root, roots_root]
        .into_iter()
        .flat_map(|root| find_function_definitions(input.database(), root))
        .collect::<Vec<_>>();
    let selected = [
        "NativePointer",
        "NativeWord",
        "NativeWordMax",
        "SystemAllocate",
        "AllocationSize",
        "AllocationAlignment",
        "AllocationDescriptor",
        "InitializeObjectHeader",
        "TypeDescriptorSize",
        "TypeDescriptorAlignment",
        "TypeDescriptorPointerMap",
        "TypeDescriptorPointerCount",
        "IsValidObjectAlignment",
        "ValidatePointerMap",
        "ValidateTypeDescriptor",
        "AllocateObject",
        "AllocateClosureEnvironment",
        "CurrentThreadState",
        "RootFrame",
        "RootFrameSlots",
        "RootFrameSlotCount",
        "SetRootSlotValue",
        "RootClosureEnvironment",
        "RootClosureEnvironmentCurrent",
    ];
    let module_items = selected
        .into_iter()
        .map(|name| {
            let key = items
                .iter()
                .copied()
                .find(|key| item_name(input.database(), *key).ok().flatten().as_deref() == Some(name))
                .unwrap_or_else(|| panic!("canonical helper {name}"));
            SyntaxModuleItem { key, symbol: name.into() }
        })
        .collect::<Vec<_>>();
    let mut builder = JITBuilder::with_isa(isa.clone(), default_libcall_names());
    builder.symbol("beskid_rt_v5_intrinsic_system_allocate", test_system_allocate as *const u8);
    builder.symbol("beskid_rt_v5_intrinsic_tls_get", test_tls_get as *const u8);
    let mut module = JITModule::new(builder);
    let declared = emit_syntax_program(&mut module, &input, isa.as_ref(), &module_items, Linkage::Export)
        .expect("closure descriptor helpers lower through the production module emitter");
    module.finalize_definitions().expect("finalize closure helpers");

    let validate = module.get_finalized_function(
        *declared
            .get(&DirectCallee::item(
                *items
                    .iter()
                    .find(|key| {
                        item_name(input.database(), **key).ok().flatten().as_deref() == Some("ValidateTypeDescriptor")
                    })
                    .expect("ValidateTypeDescriptor item"),
            ))
            .expect("ValidateTypeDescriptor declaration"),
    );
    let root_environment = module.get_finalized_function(
        *declared
            .get(&DirectCallee::item(
                *items
                    .iter()
                    .find(|key| {
                        item_name(input.database(), **key).ok().flatten().as_deref() == Some("RootClosureEnvironment")
                    })
                    .expect("RootClosureEnvironment item"),
            ))
            .expect("RootClosureEnvironment declaration"),
    );
    let root_environment_current = module.get_finalized_function(
        *declared
            .get(&DirectCallee::item(
                *items
                    .iter()
                    .find(|key| {
                        item_name(input.database(), **key).ok().flatten().as_deref()
                            == Some("RootClosureEnvironmentCurrent")
                    })
                    .expect("RootClosureEnvironmentCurrent item"),
            ))
            .expect("RootClosureEnvironmentCurrent declaration"),
    );
    let allocate_environment = module.get_finalized_function(
        *declared
            .get(&DirectCallee::item(
                *items
                    .iter()
                    .find(|key| {
                        item_name(input.database(), **key).ok().flatten().as_deref()
                            == Some("AllocateClosureEnvironment")
                    })
                    .expect("AllocateClosureEnvironment item"),
            ))
            .expect("AllocateClosureEnvironment declaration"),
    );
    let allocate_object = module.get_finalized_function(
        *declared
            .get(&DirectCallee::item(
                *items
                    .iter()
                    .find(|key| item_name(input.database(), **key).ok().flatten().as_deref() == Some("AllocateObject"))
                    .expect("AllocateObject item"),
            ))
            .expect("AllocateObject declaration"),
    );
    let validate: extern "C" fn(*const usize) -> u8 = unsafe { std::mem::transmute(validate) };
    let root_environment: extern "C" fn(*mut usize, usize, *mut u8) -> u8 =
        unsafe { std::mem::transmute(root_environment) };
    let root_environment_current: extern "C" fn(usize, *mut u8) -> u8 =
        unsafe { std::mem::transmute(root_environment_current) };
    let allocate_environment: extern "C" fn(*const usize) -> *mut u8 =
        unsafe { std::mem::transmute(allocate_environment) };
    let allocate_object: extern "C" fn(*const usize) -> *mut u8 = unsafe { std::mem::transmute(allocate_object) };

    let mut pointer_map = [16usize];
    let mut descriptor = [32usize, 8, pointer_map.as_mut_ptr() as usize, 1, 0];
    assert_eq!(validate(descriptor.as_ptr()), 1, "valid descriptor is accepted");

    pointer_map[0] = 17;
    assert_eq!(validate(descriptor.as_ptr()), 0, "unaligned pointer offset is rejected");
    pointer_map[0] = usize::MAX;
    assert_eq!(validate(descriptor.as_ptr()), 0, "overflowing pointer end is rejected");
    // Restored through the descriptor pointer map; keep the write observable to rustc.
    pointer_map[0] = std::hint::black_box(16);
    descriptor[1] = 24;
    assert_eq!(validate(descriptor.as_ptr()), 0, "non-power-of-two alignment is rejected");
    assert_eq!(validate(std::ptr::null()), 0, "null descriptor is rejected before dereference");

    pointer_map[0] = 16;
    assert_eq!(pointer_map[0], 16, "restore valid pointer offset before allocate");
    descriptor[1] = 8;
    assert_eq!(validate(descriptor.as_ptr()), 1, "restored descriptor is accepted before allocate");
    let request = [32usize, 8, descriptor.as_mut_ptr() as usize];
    let object = allocate_object(request.as_ptr());
    assert!(!object.is_null(), "valid managed request allocates an object");
    let object_header = object as *const usize;
    assert_eq!(unsafe { *object_header }, descriptor.as_mut_ptr() as usize);
    assert_eq!(unsafe { *object_header.add(1) }, 0, "managed allocation clears the GC word");
    let mismatched_size = [40usize, 8, descriptor.as_mut_ptr() as usize];
    assert!(
        allocate_object(mismatched_size.as_ptr()).is_null(),
        "managed allocation rejects a size that differs from its descriptor"
    );
    let mismatched_alignment = [32usize, 16, descriptor.as_mut_ptr() as usize];
    assert!(
        allocate_object(mismatched_alignment.as_ptr()).is_null(),
        "managed allocation rejects an alignment that differs from its descriptor"
    );
    let missing_descriptor = [32usize, 8, 0];
    assert!(allocate_object(missing_descriptor.as_ptr()).is_null(), "managed allocation rejects a null descriptor");
    assert!(
        allocate_environment(std::ptr::null()).is_null(),
        "null allocation request fails closed before dereference"
    );
    let environment = allocate_environment(request.as_ptr());
    assert!(!environment.is_null(), "valid request allocates a closure environment");
    let header = environment as *const usize;
    assert_eq!(unsafe { *header }, descriptor.as_mut_ptr() as usize);
    assert_eq!(unsafe { *header.add(1) }, 0, "allocation clears the GC word");

    let mut slots = [0usize];
    let mut frame = [0usize, slots.as_mut_ptr() as usize, 1];
    let mut tls = [0usize, frame.as_mut_ptr() as usize, 0, 1];
    assert_eq!(root_environment(tls.as_mut_ptr(), 0, environment), 1);
    assert_eq!(slots[0], environment as usize, "valid environment is rooted in its slot");
    slots[0] = 0;
    TEST_CURRENT_TLS.store(0, Ordering::SeqCst);
    assert_eq!(root_environment_current(0, environment), 0, "missing current TLS fails closed without a root write");
    assert_eq!(slots[0], 0);
    TEST_CURRENT_TLS.store(tls.as_mut_ptr() as usize, Ordering::SeqCst);
    assert_eq!(
        root_environment_current(0, environment),
        1,
        "the generated-only entry reads the actual current TLS internally"
    );
    assert_eq!(slots[0], environment as usize);
    TEST_CURRENT_TLS.store(0, Ordering::SeqCst);
}
