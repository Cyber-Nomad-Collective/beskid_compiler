use super::support::{
    DirectCallee, HashMap, ItemModuleImporter, JITBuilder, JITModule, Linkage, Module, default_libcall_names,
    emit_isle_item_with_call_importer, find_function_definitions, function_signature, item_fixture_with_root, types,
};
use beskid_isle::CallImporter;
use cranelift_codegen::{
    Context,
    binemit::Reloc,
    control::ControlPlane,
    ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName},
    isa, settings,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

#[test]
fn item_importer_never_assumes_direct_callees_are_colocated() {
    for linkage in [Linkage::Import, Linkage::Local, Linkage::Export] {
        let (input, isa, root) =
            item_fixture_with_root("i32 AddOne(i32 value) { return value; } i32 Main() { return AddOne(41); }");
        let items = find_function_definitions(input.database(), root);
        let callee = items[0];
        let caller = items[1];
        let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
        let signature = function_signature(isa.as_ref(), types::I32, [types::I32]);
        let imported = module.declare_function("AddOne", linkage, &signature).expect("declare syntax item");
        let mut importer =
            ItemModuleImporter::new(&mut module, HashMap::from([(beskid_isle::DirectCallee::item(callee), imported)]));

        let function = emit_isle_item_with_call_importer(&input, isa.as_ref(), caller, &mut importer)
            .expect("direct call lowers through the item importer");
        let imports = function.dfg.ext_funcs.values().collect::<Vec<_>>();

        assert_eq!(imports.len(), 1, "fixture must emit exactly one direct function reference");
        assert!(
            imports.iter().all(|import| !import.colocated),
            "{linkage:?} item references may be materialized with func_addr and cannot assume JIT placement: {}",
            function.display()
        );
    }
}

#[test]
fn aarch64_materialized_item_address_uses_an_unbounded_absolute_relocation() {
    let isa = isa::lookup_by_name("aarch64")
        .expect("AArch64 backend")
        .finish(settings::Flags::new(settings::builder()))
        .expect("AArch64 flags");
    let mut module = JITModule::new(JITBuilder::with_isa(isa.clone(), default_libcall_names()));
    let callee = DirectCallee::corelib_service("materialized_address_fixture");
    let callee_signature = Signature::new(isa.default_call_conv());
    let function = module
        .declare_function("materialized_address_fixture", Linkage::Local, &callee_signature)
        .expect("declare local function");
    let mut importer = ItemModuleImporter::new(&mut module, HashMap::from([(callee.clone(), function)]));
    let mut caller_signature = Signature::new(isa.default_call_conv());
    caller_signature.returns.push(AbiParam::new(types::I64));
    let mut caller = Function::with_name_signature(UserFuncName::user(0, 0), caller_signature);
    let mut builder_context = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut caller, &mut builder_context);
    let entry = builder.create_block();
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let reference = importer.import(&mut builder, callee, &callee_signature).expect("import function address");
    let address = builder.ins().func_addr(types::I64, reference);
    builder.ins().return_(&[address]);
    builder.finalize();

    let mut context = Context::for_function(caller);
    let compiled = context.compile(isa.as_ref(), &mut ControlPlane::default()).expect("compile AArch64 fixture");
    let relocations = compiled.buffer.relocs().iter().map(|relocation| relocation.kind).collect::<Vec<_>>();

    assert!(relocations.contains(&Reloc::Abs8), "far function address must use Abs8: {relocations:?}");
    assert!(
        !relocations.contains(&Reloc::Aarch64AdrPrelPgHi21),
        "function address must not retain the range-limited ADRP relocation: {relocations:?}"
    );
}
