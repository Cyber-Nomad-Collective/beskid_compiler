use beskid_analysis::artifacts::{
    decode_hir_program, decode_syntax_program, encode_syntax_program, hir_unit_snapshot,
    source_unit_snapshot,
};
use beskid_analysis::macros::expand_program_with_diagnostics;
use beskid_analysis::projects::assembly::build_hir_units;
use beskid_analysis::services::parse_program_with_source_name;
use beskid_artifacts::encode_ast;

#[test]
fn syntax_program_snapshot_roundtrips() {
    let source = "use std.io;\ni32 main() { return 0; }";
    let program = parse_program_with_source_name("Main.bd", source)
        .map(|p| {
            expand_program_with_diagnostics(
                p,
                beskid_analysis::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
                "",
                "",
            )
            .program
        })
        .expect("parse");
    let wire = encode_syntax_program(&program).expect("encode");
    let decoded = decode_syntax_program(&wire).expect("decode");
    assert_eq!(decoded.node.items.len(), program.node.items.len());
}

#[test]
fn unit_artifact_snapshots_roundtrip() {
    let source = "i32 main() { return 0; }";
    let program = parse_program_with_source_name("Main.bd", source)
        .map(|p| {
            expand_program_with_diagnostics(
                p,
                beskid_analysis::macros::DEFAULT_MAX_MACRO_EXPANSION_DEPTH,
                "",
                "",
            )
            .program
        })
        .expect("parse");
    let unit = beskid_analysis::projects::assembly::SourceUnit {
        logical_name: "Main.bd".to_string(),
        path: std::path::PathBuf::from("/tmp/Main.bd"),
        source: source.to_string(),
        program,
    };
    let hir = build_hir_units(std::slice::from_ref(&unit))
        .into_iter()
        .next()
        .expect("hir");
    let ast = source_unit_snapshot(&unit, &[]).expect("ast snap");
    let hir_snap =
        hir_unit_snapshot(&beskid_artifacts::content_fingerprint(source), &hir).expect("hir snap");
    let ast_wire = encode_ast(&ast).expect("encode ast");
    assert!(!ast_wire.is_empty());
    assert!(!hir_snap.hir_wire.is_empty());
    let _ = decode_hir_program(&hir_snap.hir_wire, &unit, &hir_snap.content_fingerprint)
        .expect("hir marker decode");
}
