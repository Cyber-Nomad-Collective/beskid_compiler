use std::path::PathBuf;

use super::emit::emit_type_bd;
use super::inventory::collect_declarations;
use super::{BTreeSet, inventory_syntax_type_names, reflect_sdk_node_kind_names, reflect_stub_path, syntax_helpers};

fn trim_field_token(tok: &str) -> &str {
    tok.trim_end_matches([',', ')', ';'])
}

/// True for legacy `f0`/`f1` positional placeholders (not `f32`/`f64` types).
fn is_fnumeric_placeholder_field(tok: &str) -> bool {
    let w = trim_field_token(tok);
    if w == "f32" || w == "f64" {
        return false;
    }
    let Some(rest) = w.strip_prefix('f') else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

#[test]
fn inventory_matches_reflect_sdk_node_kinds() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let reflect_rs = manifest.join("../beskid_analysis/src/compiler_sdk_reflect.rs");
    let inv: BTreeSet<String> = inventory_syntax_type_names(&analysis_src).expect("inventory").into_iter().collect();
    let kinds = reflect_sdk_node_kind_names(&reflect_rs).expect("reflect parse");
    let kinds_needing_shapes: BTreeSet<_> = kinds.iter().filter(|k| *k != "Node").cloned().collect();
    assert!(
        kinds_needing_shapes.is_subset(&inv),
        "every ReflectSdkNodeKind (except contract-only Node) must have a generated shape file; missing: {:?}",
        kinds_needing_shapes.difference(&inv).collect::<Vec<_>>()
    );
    // `AssignOp` is nested under `AssignExpression` (no standalone `NodeKind` today).
    let allowed_extra: BTreeSet<String> =
        ["AssignOp", "FieldKind", "InjectQualifier", "RegistrationLifetime", "ScopeHookKind"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
    let extras: BTreeSet<_> = inv.difference(&kinds).cloned().collect();
    assert!(
        extras.is_subset(&allowed_extra),
        "unexpected syntax types not listed in ReflectSdkNodeKind: {:?}",
        extras.difference(&allowed_extra).collect::<Vec<_>>()
    );
}

#[test]
fn golden_syntax_nodes_inventory_matches_scan() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let names = inventory_syntax_type_names(&analysis_src).expect("inventory");
    let got = names.join("\n") + "\n";
    let expected = include_str!("../../tests/expected/syntax_nodes_inventory.txt");
    assert_eq!(
        got, expected,
        "update tests/expected/syntax_nodes_inventory.txt after adding/removing syntax surface types"
    );
}

/// Generated `Syntax/Nodes/*.bd` must not use legacy `f0`/`f1` positional field names.
#[test]
fn syntax_node_emission_avoids_fnumeric_field_names() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let files = syntax_helpers::load_syntax_files(&analysis_src).expect("load");
    let helpers = syntax_helpers::build_helper_paths(&files);
    let (decls, _) = collect_declarations(&analysis_src, Some(&helpers)).expect("collect");
    for (name, parsed) in &decls {
        let text = emit_type_bd(name, parsed);
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("///") {
                continue;
            }
            for tok in line.split_whitespace() {
                assert!(!is_fnumeric_placeholder_field(tok), "legacy fN field placeholder in emitted `{name}`: {line}");
            }
        }
    }
}

/// With list/optional helpers enabled, syntax node bodies must not reference the removed
/// `Syntax.ReflectStub` placeholder (opaque shapes still map to the same string elsewhere).
#[test]
fn syntax_node_emission_avoids_syntax_reflect_stub_path() {
    let stub = reflect_stub_path();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let files = syntax_helpers::load_syntax_files(&analysis_src).expect("load");
    let helpers = syntax_helpers::build_helper_paths(&files);
    let (decls, _) = collect_declarations(&analysis_src, Some(&helpers)).expect("collect");
    for (name, parsed) in &decls {
        let text = emit_type_bd(name, parsed);
        assert!(!text.contains(stub), "emitted `{name}` must not use {stub}; use concrete Nodes helpers instead");
    }
}

#[test]
fn enum_variant_directives_stay_in_the_enclosing_doc_block() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let analysis_src = manifest.join("../beskid_analysis/src");
    let files = syntax_helpers::load_syntax_files(&analysis_src).expect("load");
    let helpers = syntax_helpers::build_helper_paths(&files);
    let (decls, _) = collect_declarations(&analysis_src, Some(&helpers)).expect("collect");
    let host_body_item = decls.get("HostBodyItem").expect("HostBodyItem mirror");
    let text = emit_type_bd("HostBodyItem", host_body_item);
    let enum_start = text.find("pub enum HostBodyItem").expect("enum declaration");

    assert!(text.matches("@variant(").count() >= 4, "all variant summaries must be retained");
    for (offset, _) in text.match_indices("@variant(") {
        assert!(
            offset < enum_start,
            "@variant directives are valid only in the enclosing enum documentation, not variant documentation:\n{text}"
        );
    }
}
