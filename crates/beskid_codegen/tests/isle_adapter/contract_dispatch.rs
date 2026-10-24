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

#[test]
fn equality_of_contract_values_compares_type_tags_and_receiver_words() {
    let artifact = lower(&format!(
        "{GREETERS}
        contract Shown {{ i64 Twice(); }}
        impl f64 : Shown {{ pub i64 Twice() {{ return 2_i64; }} }}
        bool Same(Greeter a, Greeter b) {{ return a == b; }}
        bool Main() {{
            mut Greeter a = Hello {{}};
            mut Greeter b = Bye {{}};
            mut Shown x = 1.5_f64;
            mut Shown y = 1.5_f64;
            return a != b && x == y && Same(a, b);
        }}"
    ));
    let main = clif(&artifact, "Main");
    // Both box words are read: the witness table (+24), its type tag and kind, the receiver (+16).
    assert!(main.contains("+24") && main.contains("+16"), "both boxes are read:\n{main}");
    assert!(main.contains("fcmp eq"), "a float receiver compares as a float value:\n{main}");
    assert!(main.contains("band"), "the type tags and the receiver values must both match:\n{main}");
    assert!(main.contains("bxor"), "`!=` inverts the comparison:\n{main}");
    let same = clif(&artifact, "Same");
    assert!(same.contains("fcmp eq") && same.contains("band"), "the unspecialized body compares boxes:\n{same}");
    for table in &artifact.witness_tables {
        assert_ne!(table.type_tag, 0, "every table carries its implementing type's tag: {table:?}");
    }
    let float_kind = artifact.witness_tables.iter().find(|table| table.symbol.contains("f64")).expect("f64 table");
    assert_eq!(float_kind.receiver_kind, beskid_queries::WITNESS_RECEIVER_F64);
    let hello = artifact.witness_tables.iter().find(|table| table.symbol.contains("Hello")).expect("Hello table");
    assert_eq!(hello.receiver_kind, beskid_queries::WITNESS_RECEIVER_REFERENCE);
}

#[test]
fn dispatch_reads_the_method_entry_after_the_witness_header() {
    let artifact = lower(&format!(
        "{GREETERS}
        i64 Main() {{ mut Greeter g = Hello {{}}; g = Bye {{}}; return g.Rank(); }}"
    ));
    let main = clif(&artifact, "Main");
    // `Rank` is slot 1: header (2 words) + 1 = word 3, byte offset 24 inside the table.
    assert!(main.contains("call_indirect"), "{main}");
    let header = beskid_queries::WITNESS_HEADER_WORDS as usize;
    assert!(main.contains(&format!("+{}", (header + 1) * 8)), "slot 1 is read past the header:\n{main}");
}

const CHANNELS: &str = r#"
contract Reader { i64 Read(); }
contract Closer { i64 Close(); }
contract Channel { Reader; Closer; i64 Flush(); }
type File : Channel {
    pub i64 Read() { return 1_i64; }
    pub i64 Close() { return 2_i64; }
    pub i64 Flush() { return 3_i64; }
}
type Pipe : Channel {
    pub i64 Read() { return 10_i64; }
    pub i64 Close() { return 20_i64; }
    pub i64 Flush() { return 30_i64; }
}
"#;

#[test]
fn a_contract_value_converts_to_every_contract_its_contract_embeds() {
    let artifact = lower(&format!(
        "{CHANNELS}
        i64 ReadAll(Reader r) {{ return r.Read(); }}
        i64 Main() {{
            mut Channel c = File {{}};
            c = Pipe {{}};
            Reader r = c;
            mut Closer k = c;
            return r.Read() + k.Close() + ReadAll(c);
        }}"
    ));
    let main = clif(&artifact, "Main");
    // Channel has three methods (Read, Close, Flush); its embedded tables follow: Reader at word
    // header + 3, Closer at word header + 4.
    let header = beskid_queries::WITNESS_HEADER_WORDS as usize;
    for word in [header + 3, header + 4] {
        assert!(main.contains(&format!("+{}", word * 8)), "the embedded table at word {word} is read:\n{main}");
    }
    assert!(main.contains("select"), "the receiver kind selects the box descriptor:\n{main}");
    assert!(main.contains("__beskid_contract_box_scalar_request_v5"), "{main}");
    // File and Pipe each have a Channel table that names their Reader and Closer tables.
    let channel_tables =
        artifact.witness_tables.iter().filter(|table| table.symbol.contains("Channel")).collect::<Vec<_>>();
    assert_eq!(channel_tables.len(), 2, "{:?}", artifact.witness_tables);
    for table in channel_tables {
        assert_eq!(table.entries.len(), 3, "{table:?}");
        assert_eq!(table.embedded.len(), 2, "Reader and Closer: {table:?}");
        for embedded in &table.embedded {
            assert!(
                artifact.witness_tables.iter().any(|candidate| &candidate.symbol == embedded),
                "the embedded table `{embedded}` is defined in the module"
            );
        }
    }
}

#[test]
fn target_typed_conditionals_match_arms_block_results_and_lambdas_convert_each_branch() {
    let artifact = lower(&format!(
        "{GREETERS}
        enum Pick {{ First, Second }}
        Greeter Choose(bool c) {{ return c ? Hello {{}} : Bye {{}}; }}
        Greeter Matched(Pick p) {{ return match p {{ Pick::First => Hello {{}}, Pick::Second => {{ Bye {{}}; }}, }}; }}
        i64 Main(bool c) {{
            Greeter g = c ? Hello {{}} : Bye {{}};
            Greeter() make = () => Hello {{}};
            return g.Rank() + make().Rank() + Choose(c).Rank() + Matched(Pick::First).Rank();
        }}"
    ));
    for function in ["Choose", "Matched"] {
        let clif = clif(&artifact, function);
        assert_eq!(
            clif.matches("__beskid_contract_box_reference_request_v5").count(),
            2,
            "every branch of `{function}` is boxed on its own:\n{clif}"
        );
    }
    let main = clif(&artifact, "Main");
    assert!(main.matches("__beskid_contract_box_reference_request_v5").count() >= 2, "{main}");
    assert!(
        artifact.functions.iter().any(|function| {
            function.name.contains("lambda")
                && function.function.display().to_string().contains("__beskid_contract_box_reference_request_v5")
        }),
        "the lambda body is boxed into its contract result: {:?}",
        artifact.functions.iter().map(|function| function.name.as_str()).collect::<Vec<_>>()
    );
}

#[test]
fn a_contract_as_the_argument_of_a_bounded_generic_dispatches_at_run_time() {
    let artifact = lower(&format!(
        "{GREETERS}
        i64 RankOf<T>(T value) where T: Greeter {{ return value.Rank(); }}
        i64 Main() {{ mut Greeter g = Hello {{}}; g = Bye {{}}; return RankOf<Greeter>(g) + RankOf(Hello {{}}); }}"
    ));
    let specializations = artifact
        .functions
        .iter()
        .filter(|function| function.name.starts_with("RankOf#generic_"))
        .map(|function| function.function.display().to_string())
        .collect::<Vec<_>>();
    assert_eq!(specializations.len(), 2, "T = Greeter and T = Hello");
    assert!(specializations.iter().any(|clif| clif.contains("call_indirect")), "T = Greeter dispatches: {specializations:#?}");
    assert!(
        specializations.iter().any(|clif| !clif.contains("call_indirect")),
        "T = Hello calls the implementation directly: {specializations:#?}"
    );
}

#[test]
fn a_generic_contract_value_dispatches_with_its_applied_arguments() {
    let artifact = lower(
        r#"
contract Repo<T> { T Get(); i64 Count(); }
type Numbers : Repo<i64> { pub i64 Get() { return 5_i64; } pub i64 Count() { return 1_i64; } }
type Words : Repo<string> { pub string Get() { return "w"; } pub i64 Count() { return 2_i64; } }
i64 Main() {
    mut Repo<i64> numbers = Numbers {};
    Repo<string> words = Words {};
    return numbers.Get() + numbers.Count() + words.Count();
}
"#,
    );
    let main = clif(&artifact, "Main");
    assert!(main.contains("call_indirect"), "`numbers.Get()` dispatches:\n{main}");
    assert_eq!(artifact.witness_tables.len(), 2, "{:?}", artifact.witness_tables);
    assert!(artifact.witness_tables.iter().any(|table| table.symbol.contains("Repo_i64")), "{:?}", artifact.witness_tables);
}
