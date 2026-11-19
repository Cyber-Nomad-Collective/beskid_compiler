#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::services::{
    FrontEndOptions, ResolvedInput, resolved_input_from_plan, synthetic_compile_plan_for_source,
};
use beskid_aot::{BuildProfile, lower_prepared_syntax_entrypoint, object_module::BeskidObjectModule};
use beskid_queries::compile_mod_free_front_end;
use object::{Object, ObjectSection};

fn managed_fixture() -> beskid_codegen::CodegenArtifact {
    let directory = tempfile::tempdir().expect("fixture directory");
    let source_path = directory.path().join("main.bd");
    let source = r#"
type Item { i64 value; }
i64 Probe() { return 0_i64; }
i64 Keep(Item item) {
    i64 result = Probe();
    return item.value + result;
}
i64 Main() { return Keep(Item { value: 42_i64 }); }
"#;
    std::fs::write(&source_path, source).expect("write fixture");
    let plan = synthetic_compile_plan_for_source(&source_path);
    let resolved: ResolvedInput = resolved_input_from_plan(source_path, source.to_owned(), plan, None, None);
    let front = compile_mod_free_front_end(
        &resolved,
        FrontEndOptions { with_semantic_diagnostics: true, ..Default::default() },
        None,
    )
    .expect("prepare fixture");
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .expect("Linux ABI target");
    lower_prepared_syntax_entrypoint(&front, "Main", target).expect("lower fixture")
}

#[test]
fn aot_object_contains_stack_maps_for_managed_values_live_across_calls() {
    let directory = tempfile::tempdir().expect("output directory");
    let artifact = managed_fixture();
    let mut module = BeskidObjectModule::new(None, BuildProfile::Debug).expect("create object module");
    module.compile_artifact(&artifact, None).expect("compile fixture");
    let object_path = directory.path().join("fixture.o");
    module.finalize_to_path(&object_path).expect("write object");
    let bytes = std::fs::read(object_path).expect("read object");
    let object = object::File::parse(bytes.as_slice()).expect("parse object");
    let maps = object.section_by_name(".data.rel.ro.beskid_gc").expect("AOT object must retain GC stack-map metadata");
    let data = maps.data().expect("read stack maps");
    let relocations = maps.relocations().map(|(offset, _)| offset as usize).collect::<Vec<_>>();
    let read_u32 = |offset: usize| {
        u32::from_le_bytes(data.get(offset..offset + 4).expect("stack-map word").try_into().unwrap()) as usize
    };
    let mut record_offset = 0;
    let mut found_live_root = false;
    while record_offset < data.len() {
        assert_eq!(&data[record_offset..record_offset + 8], b"BSKGC\0\x01\0");
        let record_length = read_u32(record_offset + 8);
        assert!(record_length >= 28 && record_offset + record_length <= data.len());
        assert!(relocations.contains(&(record_offset + 16)), "record must reference its compiled function");
        let count = read_u32(record_offset + 24);
        let mut cursor = record_offset + 28;
        for _ in 0..count {
            let root_count = read_u32(cursor + 8);
            found_live_root |= root_count > 0;
            cursor += 12 + 4 * root_count;
            assert!(cursor <= record_offset + record_length, "safepoint must fit its record");
        }
        record_offset += record_length;
    }
    assert!(found_live_root, "a managed value live across a call must appear in a stack map");
}

#[test]
fn program_exports_cannot_replace_runtime_checkpoints() {
    for symbol in ["beskid_rt_v5_stack_map_push", "beskid_rt_v5_stack_map_pop"] {
        let mut artifact = managed_fixture();
        artifact.exports.push(beskid_codegen::ExportEntry {
            function: artifact
                .functions
                .iter()
                .find(|f| f.name.split('#').next() == Some("Probe"))
                .expect("lowered Probe")
                .name
                .clone(),
            exported_symbol: symbol.to_owned(),
            abi: "C".to_owned(),
        });
        let mut module = BeskidObjectModule::new(None, BuildProfile::Debug).expect("object module");
        let error = module.compile_artifact(&artifact, None).expect_err("reserved runtime checkpoint");
        assert!(error.to_string().contains(symbol), "{error}");
    }
}
