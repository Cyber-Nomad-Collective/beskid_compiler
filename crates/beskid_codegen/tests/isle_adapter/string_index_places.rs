//! A string is indexed directly wherever it is reached: a local, a parameter, a field of a
//! parameter or local, a field of `this`, and a field of a field. The byte has type `u8` in every
//! position, so it converts and compares like any other `u8`.

use super::support::{SyntaxModuleItem, find_nodes_of_kind, item_fixture_with_root, item_name, lower_syntax_program};

const CURSOR: &str = "type Cursor { string text, i64 at, pub i64 Current() { return i64(this.text[this.at]); } } \
                      type Outer { Cursor cursor } ";

fn lowers(source: &str) {
    let source = format!("{CURSOR}{source}");
    let (input, isa, root) = item_fixture_with_root(&source);
    let db = input.database();
    let mut items = Vec::new();
    for kind in [beskid_queries::IndexedNodeKind::MethodDefinition, beskid_queries::IndexedNodeKind::FunctionDefinition] {
        for key in find_nodes_of_kind(db, root, kind) {
            let name = item_name(db, key).ok().flatten().expect("item name");
            items.push(SyntaxModuleItem { key, symbol: name.to_string() });
        }
    }
    if let Err(error) = lower_syntax_program(&input, isa.as_ref(), &items) {
        panic!("{source}: string index must lower: {error:?}");
    }
}

#[test]
fn string_index_of_a_local_or_parameter_converts_and_compares() {
    lowers("i64 Main(string text) { string local = text; if i64(local[0]) != 98 { return 1; } return i64(text[1]); }");
}

#[test]
fn string_index_of_a_field_converts_and_compares() {
    lowers("i64 Main(Cursor cursor) { if i64(cursor.text[5]) != 100 { return 1; } return i64(cursor.text[cursor.at]); }");
}

#[test]
fn string_index_of_a_field_of_a_field_converts_and_compares() {
    lowers("i64 Main(Outer outer) { u8 first = outer.cursor.text[0]; if first != 98_u8 { return 1; } return i64(outer.cursor.text[2]) + 1; }");
}

#[test]
fn string_index_of_a_field_of_this_lowers_in_a_method() {
    lowers("i64 Main(Cursor cursor) { return cursor.Current(); }");
}
