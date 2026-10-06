//! Scalar values round-trip exactly; invalid scalar ranges never produce a character.
use beskid_analysis::services::{FrontEndOptions, resolved_input_from_plan, synthetic_compile_plan_for_source};
use beskid_codegen::lower_prepared_syntax_module;
use beskid_queries::{compile_front_end_from_resolved_input, with_db};
use cranelift_codegen::{isa, settings};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
#[test]
fn checked_unicode_projection_preserves_scalars_and_rejects_invalid_ranges() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Main.bd");
    let source = "pub u32 Project(u32 value) { if (value > 1114111_u32 || (value >= 55296_u32 && value <= 57343_u32)) { return 4294967295_u32; } return u32(char(value)); }";
    std::fs::write(&path, source).unwrap();
    let plan = synthetic_compile_plan_for_source(&path);
    let resolved = resolved_input_from_plan(path, source.into(), plan, None, None);
    let front = compile_front_end_from_resolved_input(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: false, ..Default::default() },
        None,
    )
    .unwrap();
    let target = beskid_abi::runtime_kit::host_runtime_target().unwrap();
    let isa =
        isa::lookup(target_lexicon::Triple::host()).unwrap().finish(settings::Flags::new(settings::builder())).unwrap();
    let artifact = with_db(|db| lower_prepared_syntax_module(db, &front, target, isa.as_ref())).unwrap();
    assert_eq!(artifact.functions.len(), 1);
    let function = &artifact.functions[0];
    assert!(
        function.function.display().to_string().contains("trapnz"),
        "raw char conversion must validate Unicode scalar range"
    );
    let mut module = JITModule::new(JITBuilder::new(default_libcall_names()).unwrap());
    let id = module.declare_function("Project", Linkage::Export, &function.function.signature).unwrap();
    let mut context = module.make_context();
    context.func = function.function.clone();
    module.define_function(id, &mut context).unwrap();
    module.finalize_definitions().unwrap();
    let project: unsafe extern "C" fn(u32) -> u32 = unsafe { std::mem::transmute(module.get_finalized_function(id)) };
    for value in [0, 65, 0xd7ff, 0xe000, 0x1f600, 0x10ffff] {
        assert_eq!(unsafe { project(value) }, value);
    }
    for value in [0xd800, 0xdfff, 0x110000, u32::MAX] {
        assert_eq!(unsafe { project(value) }, u32::MAX);
    }
}
