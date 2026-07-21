use beskid_aot::ExportTable;
use beskid_engine::services::prepare_jit_module;
use std::path::Path;

#[test]
fn export_metadata_populates_codegen_artifact() {
    let src = r#"
[Export(Abi:"C", Symbol:"beskid_plugin_init")]
pub unit plugin_init() { return; }
"#;
    let artifact = prepare_jit_module(Path::new("<memory>"), src)
        .expect("lower export fixture through syntax codegen");
    assert_eq!(artifact.exports.len(), 1);
    let entry = &artifact.exports[0];
    assert_eq!(entry.exported_symbol, "beskid_plugin_init");
    assert_eq!(entry.beskid_name, "plugin_init");
    assert_eq!(entry.abi, "C");
}

#[test]
fn export_table_from_artifact_matches_codegen_exports() {
    let src = r#"
[Export(Abi:"C", Symbol:"beskid_plugin_init")]
pub unit plugin_init() { return; }
"#;
    let artifact = prepare_jit_module(Path::new("<memory>"), src)
        .expect("lower export fixture through syntax codegen");
    let table = ExportTable::from_artifact(&artifact);
    assert_eq!(table.entries().len(), 1);
    assert_eq!(table.entries()[0].symbol_id, 1);
    assert_eq!(
        table.linker_symbols(),
        vec!["beskid_plugin_init".to_string()]
    );
}

#[test]
fn export_on_non_pub_function_fails_codegen() {
    let src = r#"
[Export(Abi:"C", Symbol:"secret")]
unit secret() { return; }
"#;
    let result = prepare_jit_module(Path::new("<memory>"), src);
    assert!(result.is_err(), "non-pub export should fail lowering");
}
