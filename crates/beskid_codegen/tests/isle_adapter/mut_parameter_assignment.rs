//! `mut` parameters pass by value (E1259): a whole assignment to one that no later read observes
//! is rejected by the legality gate before lowering, and the shapes that do reach the caller (a
//! field store through the parameter, a value read after the assignment) lower.

use super::support::{emit_isle_item, find_function_definitions, item_fixture_with_root};

const SIZE: &str = "type Size { i32 columns; i32 rows; } ";

fn gate_codes(source: &str) -> Vec<&'static str> {
    let (input, _isa, root) = item_fixture_with_root(source);
    let db = input.database();
    let items = find_function_definitions(db, root);
    beskid_queries::check_items(db, &items)
        .err()
        .unwrap_or_default()
        .iter()
        .map(|finding| finding.kind.code())
        .filter(|code| *code == "E1259")
        .collect()
}

#[test]
fn a_dead_whole_assignment_is_rejected_before_lowering() {
    for source in [
        format!("{SIZE}unit AssignWhole(mut Size s) {{ s = Size {{ columns: 80, rows: 24, }}; }}"),
        "enum Color { Red(), Green() } unit Paint(mut Color c) { c = Color::Green(); }".to_string(),
        "unit Bump(mut i64 value) { value = value + 1; }".to_string(),
        format!(
            "{SIZE}unit Inner(mut Size s) {{ s = Size {{ columns: 80, rows: 24, }}; }} \
             unit Outer(mut Size s) {{ Inner(s); }}"
        ),
    ] {
        assert_eq!(gate_codes(&source), ["E1259"], "{source}");
    }
}

#[test]
fn assignments_that_reach_a_read_lower() {
    for (source, function) in [
        (format!("{SIZE}unit AssignFields(mut Size s) {{ s.columns = 80; s.rows = 24; }}"), "AssignFields"),
        (
            format!(
                "{SIZE}Size Normalized(mut Size s) {{ if (s.columns < 80) {{ s = Size {{ columns: 80, rows: 24, }}; }} return s; }}"
            ),
            "Normalized",
        ),
        ("i64 Clamp(mut i64 v) { if (v < 0) { v = 0; } return v; }".to_string(), "Clamp"),
        ("enum Color { Red(), Green() } Color Next(mut Color c) { c = Color::Green(); return c; }".to_string(), "Next"),
    ] {
        assert!(gate_codes(&source).is_empty(), "the gate accepts {source}");
        let (input, isa, root) = item_fixture_with_root(&source);
        let db = input.database();
        let item = find_function_definitions(db, root)
            .into_iter()
            .find(|item| {
                beskid_queries::node_span(db, *item)
                    .ok()
                    .flatten()
                    .is_some_and(|span| source[span.start..span.end].contains(&format!(" {function}(")))
            })
            .unwrap_or_else(|| panic!("function {function}"));
        let lowered =
            emit_isle_item(&input, isa.as_ref(), item).unwrap_or_else(|error| panic!("{function} must lower: {error}"));
        if function == "AssignFields" {
            let clif = lowered.display().to_string();
            assert!(clif.contains("store"), "the field assignments store through the parameter: {clif}");
        }
    }
}
