use beskid_aot::ExportTable;
use beskid_engine::services::prepare_jit_module;
use std::path::Path;

#[test]
fn export_metadata_populates_codegen_artifact() {
    let src = r#"
[Export(Abi:"C", Symbol:"beskid_plugin_init")]
pub unit plugin_init() { return; }
"#;
    let artifact = prepare_jit_module(Path::new("<memory>"), src, &beskid_engine::ModHost::NoMods)
        .expect("lower export fixture through syntax codegen");
    assert_eq!(artifact.exports.len(), 1);
    let entry = &artifact.exports[0];
    assert_eq!(entry.exported_symbol, "beskid_plugin_init");
    assert!(entry.function.starts_with("plugin_init#syntax_"), "the entry names its declaration: {}", entry.function);
    assert_eq!(entry.abi, "C");
}

#[test]
fn export_table_from_artifact_matches_codegen_exports() {
    let src = r#"
[Export(Abi:"C", Symbol:"beskid_plugin_init")]
pub unit plugin_init() { return; }
"#;
    let artifact = prepare_jit_module(Path::new("<memory>"), src, &beskid_engine::ModHost::NoMods)
        .expect("lower export fixture through syntax codegen");
    let table = ExportTable::from_artifact(&artifact);
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
    let result = prepare_jit_module(Path::new("<memory>"), src, &beskid_engine::ModHost::NoMods);
    assert!(result.is_err(), "non-pub export should fail lowering");
}

#[test]
fn raw_c_exports_reject_property_objects_in_parameters_and_results() {
    let cases = [
        ("pub i64 Read(Counter counter) { return counter.Count; }", "parameter `counter`", "Counter"),
        ("pub Counter Create() { return Counter {}; }", "result", "Counter"),
        ("pub Envelope Wrap() { return Envelope { counter: Counter {} }; }", "result", "Counter"),
        ("pub i64 ReadView(View view) { return view.Count; }", "parameter `view`", "View"),
        ("pub Derived ExposeDerived(Derived value) { return value; }", "parameter `value`", "Derived"),
    ];
    for (function, position, owner) in cases {
        let source = format!(
            r#"
type Counter {{ prop i64 Count = 0; }}
type Envelope {{ Counter counter; }}
contract View {{ i64 Count {{ get; }} }}
type Derived {{ i64 base; i64 Total => base + base; }}
[Export(Abi:"C", Symbol:"foreign_call")]
{function}
"#
        );
        let error = match prepare_jit_module(Path::new("<memory>"), &source, &beskid_engine::ModHost::NoMods) {
            Ok(_) => panic!("raw C export was admitted for {function}"),
            Err(error) => format!("{error:#}"),
        };
        assert!(error.contains(&format!("{position} reaches property-bearing type `{owner}`")), "{error}");
    }
}

#[test]
fn raw_c_exports_allow_scalar_facades_and_ordinary_values() {
    let source = r#"
type Counter { prop i64 Count = 0; }
type Plain { i64 value; }

[Export(Abi:"C", Symbol:"read_scalar")]
pub i64 ReadScalar(i64 count) {
    Counter counter = Counter {};
    counter.Count = count;
    return counter.Count;
}

[Export(Abi:"C", Symbol:"echo_plain")]
pub Plain EchoPlain(Plain value) { return value; }
"#;
    let artifact = prepare_jit_module(Path::new("<memory>"), source, &beskid_engine::ModHost::NoMods)
        .expect("scalar facade and ordinary nominal export");
    let symbols = ExportTable::from_artifact(&artifact).linker_symbols();
    assert!(symbols.contains(&"read_scalar".to_owned()));
    assert!(symbols.contains(&"echo_plain".to_owned()));
}
