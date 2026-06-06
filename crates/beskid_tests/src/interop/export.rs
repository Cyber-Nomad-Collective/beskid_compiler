use beskid_aot::ExportTable;
use beskid_codegen::services::lower_source;
use std::path::Path;

#[test]
fn export_metadata_populates_codegen_artifact() {
    let src = r#"
[Export(Abi:"C", Symbol:"beskid_plugin_init")]
pub unit plugin_init() { return; }
"#;
    let lowered =
        lower_source(Path::new("<memory>"), src, false).expect("lower export fixture");
    assert_eq!(lowered.artifact.exports.len(), 1);
    let entry = &lowered.artifact.exports[0];
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
    let lowered =
        lower_source(Path::new("<memory>"), src, false).expect("lower export fixture");
    let table = ExportTable::from_artifact(&lowered.artifact);
    assert_eq!(table.entries().len(), 1);
    assert_eq!(table.entries()[0].symbol_id, 1);
    assert_eq!(table.linker_symbols(), vec!["beskid_plugin_init".to_string()]);
}

#[test]
fn export_on_non_pub_function_fails_codegen() {
    let src = r#"
[Export(Abi:"C", Symbol:"secret")]
unit secret() { return; }
"#;
    let result = lower_source(Path::new("<memory>"), src, false);
    assert!(result.is_err(), "non-pub export should fail lowering");
}
