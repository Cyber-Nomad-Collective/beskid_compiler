//! Runtime contract dispatch (v0.6.7): concrete values written into contract-typed destinations
//! are boxed with a witness table; calls through a contract value load the table entry and call
//! it indirectly; conf2's static selections stay direct calls.

use super::support::{find_nodes_of_kind, item_fixture_with_root, item_name, lower_syntax_program, SyntaxModuleItem};

fn lower(source: &str) -> beskid_codegen::CodegenArtifact {
    let (input, isa, root) = item_fixture_with_root(source);
    let mut items = Vec::new();
    for kind in [beskid_queries::IndexedNodeKind::FunctionDefinition, beskid_queries::IndexedNodeKind::MethodDefinition] {
        for key in find_nodes_of_kind(input.database(), root, kind) {
            let symbol = item_name(input.database(), key).expect("item query").expect("item name").to_string();
            items.push(SyntaxModuleItem { symbol, key });
        }
    }
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items).expect("contract values lower");
    beskid_codegen::validate_artifact(&artifact).expect("contract value artifact");
    artifact
}

fn clif(artifact: &beskid_codegen::CodegenArtifact, name: &str) -> String {
    artifact.functions.iter().find(|function| function.name == name).unwrap_or_else(|| panic!("{name} lowered")).function.display().to_string()
}

const GREETERS: &str = r#"
contract Greeter { string Greet(); i64 Rank(); }
type Hello : Greeter { pub string Greet() { return "hello"; } pub i64 Rank() { return 1_i64; } }
type Bye : Greeter { pub string Greet() { return "bye"; } pub i64 Rank() { return 2_i64; } }
"#;

#[test]
fn a_mutable_contract_local_boxes_its_values_and_dispatches_through_the_witness_table() {
    let artifact = lower(&format!(
        "{GREETERS}
        i64 Main() {{ mut Greeter g = Hello {{}}; g = Bye {{}}; return g.Rank(); }}"
    ));
    let main = clif(&artifact, "Main");
    assert!(main.contains("call_indirect"), "`g.Rank()` dispatches indirectly:\n{main}");
    assert!(main.contains("__beskid_contract_box_reference_request_v5"), "both values are boxed:\n{main}");
    assert_eq!(artifact.witness_tables.len(), 2, "one table per implementing type: {:?}", artifact.witness_tables);
    assert!(artifact.contract_boxes_required);
    for table in &artifact.witness_tables {
        assert_eq!(table.entries.len(), 2, "Greet and Rank in slot order: {table:?}");
    }
    assert!(artifact.witness_tables.iter().any(|table| table.entries == ["Greet", "Rank"] || table.entries.iter().all(|entry| artifact.functions.iter().any(|function| &function.name == entry))));
}

#[test]
fn an_immutable_local_with_a_concrete_initializer_keeps_the_direct_call_and_unboxes_the_receiver() {
    let artifact = lower(&format!(
        "{GREETERS}
        i64 Main() {{ Greeter g = Hello {{}}; return g.Rank(); }}"
    ));
    let main = clif(&artifact, "Main");
    assert!(!main.contains("call_indirect"), "conf2's static selection stays a direct call:\n{main}");
    assert!(main.contains("__beskid_contract_box_reference_request_v5"), "the local holds a box:\n{main}");
    assert!(main.contains("load.i64") && main.contains("+16"), "the receiver word is read out of the box:\n{main}");
}

#[test]
fn contract_values_in_arrays_fields_parameters_and_returns_dispatch() {
    let artifact = lower(&format!(
        "{GREETERS}
        type Holder {{ Greeter greeter; }}
        Greeter Pick(bool first) {{ if first {{ return Hello {{}}; }} return Bye {{}}; }}
        i64 Through(Greeter g) {{ return g.Rank(); }}
        i64 Main() {{
            Greeter[] all = [Hello {{}}, Bye {{}}];
            mut i64 total = 0_i64;
            mut i64 i = 0_i64;
            while i < 2_i64 {{ total = total + all[i].Rank(); i = i + 1_i64; }}
            Holder holder = Holder {{ greeter: Bye {{}} }};
            total = total + holder.greeter.Rank();
            total = total + Pick(true).Rank();
            total = total + Through(all[0]);
            total = total + Through(Hello {{}});
            return total;
        }}"
    ));
    let main = clif(&artifact, "Main");
    assert!(main.contains("call_indirect"), "element, field and returned values dispatch:\n{main}");
    let through = clif(&artifact, "Through");
    assert!(through.contains("call_indirect"), "the unspecialized body takes a box:\n{through}");
    assert!(
        artifact.functions.iter().any(|function| function.name.starts_with("Through#generic_")),
        "`Through(Hello {{}})` keeps its witness specialization: {:?}",
        artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>()
    );
    let pick = clif(&artifact, "Pick");
    assert_eq!(pick.matches("__beskid_contract_box_reference_request_v5").count(), 2, "both returns box:\n{pick}");
}

#[test]
fn a_primitive_with_an_implementation_is_boxed_into_a_contract_value_through_a_thunk() {
    let artifact = lower(
        r#"
contract Shown { i64 Twice(); }
impl i32 : Shown { pub i64 Twice() { return i64(this) * 2_i64; } }
impl f64 : Shown { pub i64 Twice() { return 4_i64; } }
i64 Use(Shown s) { return s.Twice(); }
i64 Main() { Shown a = 21_i32; Shown b = 1.5_f64; return a.Twice() + Use(b) + Use(7_i32); }
"#,
    );
    let main = clif(&artifact, "Main");
    assert!(main.contains("__beskid_contract_box_scalar_request_v5"), "primitives use the scalar box:\n{main}");
    assert_eq!(artifact.witness_tables.len(), 2, "{:?}", artifact.witness_tables);
    for table in &artifact.witness_tables {
        assert!(table.entries[0].contains("__thunk0"), "a scalar receiver dispatches through a thunk: {table:?}");
        let thunk = clif(&artifact, &table.entries[0]);
        assert!(thunk.contains("ireduce") || thunk.contains("bitcast"), "the thunk narrows the word:\n{thunk}");
    }
}

#[test]
fn generic_implementations_dispatch_through_their_specialized_methods() {
    let artifact = lower(
        r#"
contract Shown { i64 Size(); }
type Box<T> { T value; }
impl<T> Box<T> : Shown { pub i64 Size() { return 8_i64; } }
i64 Main() { mut Shown s = Box<i64> { value: 1_i64 }; s = Box<bool> { value: true }; return s.Size(); }
"#,
    );
    assert_eq!(artifact.witness_tables.len(), 2, "{:?}", artifact.witness_tables);
    for table in &artifact.witness_tables {
        assert!(table.entries[0].contains("#generic_"), "the entry is the specialized method: {table:?}");
    }
}
