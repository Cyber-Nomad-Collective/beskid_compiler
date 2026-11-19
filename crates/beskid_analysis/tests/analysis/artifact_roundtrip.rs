use beskid_analysis::artifacts::{decode_syntax_program, encode_syntax_program, source_unit_snapshot};
use beskid_analysis::macros::expand_program_with_diagnostics;
use beskid_analysis::services::parse_program_with_source_name;
use beskid_artifacts::encode_ast;

#[test]
fn syntax_program_snapshot_roundtrips() {
    let source = "use std.io;\ni32 Main() { return 0; }";
    let program = parse_program_with_source_name("Main.bd", source)
        .map(|p| {
            expand_program_with_diagnostics(p, beskid_analysis::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH, "", "")
                .program
        })
        .expect("parse");
    let wire = encode_syntax_program(&program).expect("encode");
    let decoded = decode_syntax_program(&wire).expect("decode");
    assert_eq!(decoded.node.items.len(), program.node.items.len());
}

#[test]
fn mixed_type_members_round_trip_through_syntax_cache() {
    let source = "type Counter { prop i64 Count = 0; i64 Read() => Count; i64 raw; i64 Double => Count + Count; }";
    let program = parse_program_with_source_name("Counter.bd", source).expect("parse mixed members");
    let wire = encode_syntax_program(&program).expect("encode mixed members");
    let decoded = decode_syntax_program(&wire).expect("decode mixed members");
    assert_eq!(decoded, program);
}

#[test]
fn property_accessor_blocks_round_trip_through_syntax_cache() {
    let source = "type Counter { i64 Count { get; set { field = value < 0 ? 0 : value; } } = 0; }";
    let program = parse_program_with_source_name("Counter.bd", source).expect("parse accessor block");
    let wire = encode_syntax_program(&program).expect("encode accessor block");
    let decoded = decode_syntax_program(&wire).expect("decode accessor block");
    assert_eq!(decoded, program);
}

#[test]
fn expanded_syntax_unit_snapshot_roundtrips() {
    let source = "i32 Main() { return 0; }";
    let program = parse_program_with_source_name("Main.bd", source)
        .map(|p| {
            expand_program_with_diagnostics(p, beskid_analysis::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH, "", "")
                .program
        })
        .expect("parse");
    let unit = beskid_analysis::projects::assembly::SourceUnit {
        logical_name: "Main.bd".to_string(),
        origin_path: std::path::PathBuf::from("/tmp/Main.bd"),
        path: std::path::PathBuf::from("/tmp/Main.bd"),
        source: source.to_string(),
        program,
    };
    let ast = source_unit_snapshot(&unit, &[]).expect("ast snap");
    let ast_wire = encode_ast(&ast).expect("encode ast");
    assert!(!ast_wire.is_empty());
    let decoded = beskid_artifacts::decode_ast(&ast_wire).expect("decode ast");
    assert_eq!(decoded.meta.content_fingerprint, beskid_artifacts::content_fingerprint(source));
}
