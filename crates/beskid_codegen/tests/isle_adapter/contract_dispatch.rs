//! Runtime contract dispatch (v0.6.7): concrete values written into contract-typed destinations
//! are boxed with a witness table; calls through a contract value load the table entry and call
//! it indirectly; conf2's static selections stay direct calls.

use super::support::{
    SyntaxGenerationId, SyntaxModuleItem, canonical_corelib_host_project_fixture, find_nodes_of_kind,
    item_fixture_with_root, item_name, lower_syntax_program,
};

fn lower(source: &str) -> beskid_codegen::CodegenArtifact {
    let (input, isa, root) = item_fixture_with_root(source);
    let mut items = Vec::new();
    for kind in [beskid_queries::IndexedNodeKind::FunctionDefinition, beskid_queries::IndexedNodeKind::MethodDefinition]
    {
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
    artifact
        .functions
        .iter()
        .find(|function| function.name == name)
        .unwrap_or_else(|| panic!("{name} lowered"))
        .function
        .display()
        .to_string()
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
    assert!(artifact.witness_tables.iter().any(|table| table.entries == ["Greet", "Rank"]
        || table.entries.iter().all(|entry| artifact.functions.iter().any(|function| &function.name == entry))));
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
            total = total + Through(holder.greeter);
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
fn an_immutable_local_whose_initializer_forwards_the_expected_type_dispatches_at_run_time() {
    // The identity of `c ? Hello {} : Bye {}` is its then-branch, but either branch may be boxed
    // into the local: a static `Hello.Rank` would run on a `Bye`. Every forwarding initializer
    // (conditional, grouped conditional, `match`, value block, lambda result) dispatches.
    let artifact = lower(&format!(
        "{GREETERS}
        enum Pick {{ First, Second }}
        i64 Conditional(bool c) {{ Greeter g = c ? Hello {{}} : Bye {{}}; return g.Rank(); }}
        i64 Grouped(bool c) {{ Greeter g = (c ? Hello {{}} : Bye {{}}); return g.Rank(); }}
        i64 Matched(Pick p) {{
            Greeter g = match p {{ Pick::First => Hello {{}}, Pick::Second => Bye {{}}, }};
            return g.Rank();
        }}
        i64 Blocked(bool c) {{ Greeter g = {{ c ? Hello {{}} : Bye {{}}; }}; return g.Rank(); }}
        i64 Lambda(bool c) {{ Greeter() make = () => c ? Hello {{}} : Bye {{}}; Greeter g = make(); return g.Rank(); }}
        i64 Static() {{ Greeter g = Hello {{}}; return g.Rank(); }}"
    ));
    for function in ["Conditional", "Grouped", "Matched", "Blocked", "Lambda"] {
        let clif = clif(&artifact, function);
        assert!(clif.contains("call_indirect"), "`g` in `{function}` dispatches through its witness table:\n{clif}");
    }
    let main = clif(&artifact, "Static");
    assert!(!main.contains("call_indirect"), "a plain nominal initializer keeps the static selection:\n{main}");
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
    assert!(
        specializations.iter().any(|clif| clif.contains("call_indirect")),
        "T = Greeter dispatches: {specializations:#?}"
    );
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
    assert!(
        artifact.witness_tables.iter().any(|table| table.symbol.contains("Repo_i64")),
        "{:?}",
        artifact.witness_tables
    );
}

/// Every function and method of the entry unit, with its item symbol.
fn entry_items(input: &beskid_codegen::CodegenInput<'_>, root: beskid_queries::AstNodeKey) -> Vec<SyntaxModuleItem> {
    let mut items = Vec::new();
    for kind in [beskid_queries::IndexedNodeKind::FunctionDefinition, beskid_queries::IndexedNodeKind::MethodDefinition]
    {
        for key in find_nodes_of_kind(input.database(), root, kind) {
            let symbol = item_name(input.database(), key).expect("item query").expect("item name").to_string();
            items.push(SyntaxModuleItem { symbol, key });
        }
    }
    items
}

/// The error codes `beskid check` reports for `source` (one `App/Main.bd` against the canonical
/// Corelib, host hooks expanded as the front end does).
fn check_errors(source: &str, generation: u64) -> Vec<&'static str> {
    let (input, _, root) =
        canonical_corelib_host_project_fixture(&[("App/Main.bd", source)], SyntaxGenerationId(generation));
    let keys = entry_items(&input, root).iter().map(|item| item.key).collect::<Vec<_>>();
    check_findings(input.database(), &keys)
}

/// The error codes of every pass `beskid check` runs over `items`: the legality gate, the gate
/// obligations and the typing obligations (E1206).
fn check_findings(db: &dyn beskid_queries::Db, items: &[beskid_queries::AstNodeKey]) -> Vec<&'static str> {
    let mut findings = beskid_queries::check_items(db, items).err().unwrap_or_default();
    findings.extend(beskid_queries::check_gate_obligations(db, items));
    findings.extend(beskid_queries::check_typing_obligations(db, items));
    findings.iter().map(|finding| finding.kind.code()).filter(|code| code.starts_with('E')).collect()
}

/// Check `source` as `beskid build` does (it must be clean), then compose its hosts with the
/// frozen plan and lower every function and method of the entry unit against the canonical
/// Corelib.
fn build(source: &str, generation: u64) -> beskid_codegen::CodegenArtifact {
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let (input, isa, root) =
        canonical_corelib_host_project_fixture(&[("App/Main.bd", source)], SyntaxGenerationId(generation));
    let keys = entry_items(&input, root).iter().map(|item| item.key).collect::<Vec<_>>();
    let errors = check_findings(input.database(), &keys);
    assert!(errors.is_empty(), "the program checks clean: {errors:?}");
    let input = if source.contains("host ") {
        let result = resolve_composition(CompositionInput {
            program: &input.typed_program().assembly.entry_unit().program,
            is_mod_project: false,
            types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
        });
        assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
        let mut snapshot = result.snapshot;
        snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
        let generation = input.typed_program().generation;
        input
            .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
            .expect("attach frozen composition authority")
    } else {
        input
    };
    let items = entry_items(&input, root);
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .unwrap_or_else(|error| panic!("the program lowers without an internal error: {error}"));
    beskid_codegen::validate_artifact(&artifact).expect("contract value artifact");
    artifact
}

fn function_names(artifact: &beskid_codegen::CodegenArtifact) -> Vec<&str> {
    artifact.functions.iter().map(|function| function.name.as_str()).collect()
}

const CORELIB_GREETERS: &str = r#"
use Core.Collections.List;
use Core.Results;
contract Greeter {
    string Greet(string name);
}
type Hello : Greeter {
    pub string Greet(string name) { return "Hello, " + name; }
}
type Bye : Greeter {
    i64 times;
    pub string Greet(string name) { return "Bye, " + name; }
}
unit Use(string text) { }
"#;

/// A generic parameter already bound to a contract (by the receiver's type arguments, explicit
/// type arguments or an earlier argument) is the expected type of every later argument of that
/// parameter: a conforming value is boxed there instead of conflicting with the binding (E1229).
#[test]
fn a_generic_parameter_bound_to_a_contract_boxes_conforming_arguments() {
    let artifact = build(
        &format!(
            "{CORELIB_GREETERS}
T Pick<T>(T first, T second) {{ return second; }}
T Same<T>(T value) {{ return value; }}
i64 Main() {{
    mut List<Greeter> list = List.New<Greeter>();
    list.Push(Hello {{}});
    list.Push(Bye {{ times: 2 }});
    mut Greeter g = Bye {{ times: 3 }};
    Greeter picked = Pick(g, Hello {{}});
    Greeter same = Same<Greeter>(Hello {{}});
    Use(picked.Greet(\"a\"));
    Use(same.Greet(\"b\"));
    return list.Count();
}}"
        ),
        6731,
    );
    let main = clif(&artifact, "Main");
    assert!(
        main.matches("__beskid_contract_box_reference_request_v5").count() >= 5,
        "both pushed values, the local, and the later `Pick` and explicit `Same` arguments are boxed:\n{main}"
    );
    let names = function_names(&artifact);
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Pick#generic_")).count(),
        1,
        "`Pick` has one specialization, at `T = Greeter`: {names:?}"
    );
}

/// A conforming argument never rebinds a contract-bound parameter: a non-conforming one still
/// conflicts.
#[test]
fn a_non_conforming_argument_of_a_contract_bound_parameter_is_still_rejected() {
    let errors = check_errors(
        &format!(
            "{CORELIB_GREETERS}
type Stone {{ i64 weight; }}
i64 Main() {{
    mut List<Greeter> list = List.New<Greeter>();
    list.Push(Stone {{ weight: 1 }});
    return list.Count();
}}"
        ),
        6732,
    );
    assert!(!errors.is_empty(), "a `Stone` is no `Greeter`");
}

/// `List.Get` returns `Result<T, string>`: written into a contract-typed local it is a type
/// mismatch, not a value to box.
#[test]
fn a_result_written_into_a_contract_typed_local_is_a_type_mismatch() {
    let errors = check_errors(
        &format!(
            "{CORELIB_GREETERS}
i64 Main() {{
    mut List<Greeter> list = List.New<Greeter>();
    Greeter h = Hello {{}};
    list.Push(h);
    Greeter got = list.Get(0);
    Use(got.Greet(\"Cy\"));
    return 0;
}}"
        ),
        6733,
    );
    assert!(errors.contains(&"E1206"), "`Result<Greeter, string>` is no `Greeter`: {errors:?}");
}

/// Contract values that come back out of a generic container (a `Result<Greeter, string>`
/// payload bound by a `match` pattern, a generic return specialized at `T = Greeter`) are
/// already boxes: they pass through unchanged and calls on them dispatch.
#[test]
fn contract_values_returned_by_generic_code_pass_through_and_dispatch() {
    let artifact = build(
        &format!(
            "{CORELIB_GREETERS}
T Same<T>(T value) {{ return value; }}
string First(List<Greeter> list) {{
    Greeter fallback = Hello {{}};
    Greeter got = match list.Get(0) {{ Result::Ok(v) => v, Result::Error(e) => fallback, }};
    Greeter again = Same<Greeter>(got);
    return again.Greet(\"Cy\");
}}
string Each(List<Greeter> list) {{
    mut string out = \"\";
    mut i64 i = 0;
    while (i < list.Count()) {{
        match list.Get(i) {{
            Result::Ok(v) => {{ out = out + v.Greet(\"Cy\"); }},
            Result::Error(e) => {{ out = out + e; }},
        }};
        i += 1;
    }}
    return out;
}}
i64 Main() {{
    mut List<Greeter> list = List.New<Greeter>();
    Greeter h = Hello {{}};
    list.Push(h);
    Use(First(list));
    Use(Each(list));
    return 0;
}}"
        ),
        6734,
    );
    for function in ["First", "Each"] {
        let body = clif(&artifact, function);
        assert!(body.contains("call_indirect"), "`{function}` dispatches through the box it got back:\n{body}");
        assert!(
            !body.contains("__beskid_contract_box_scalar_request_v5"),
            "no value that is already a box is boxed again:\n{body}"
        );
    }
    let first = clif(&artifact, "First");
    assert_eq!(
        first.matches("__beskid_contract_box_reference_request_v5").count(),
        1,
        "only the concrete fallback is boxed:\n{first}"
    );
}

const CROWD: &str = r#"
use Core.Collections.Array;
contract Greeter {
    string Greet(string name);
}
type Hello : Greeter {
    pub string Greet(string name) { return "Hello, " + name; }
}
type Bye : Greeter {
    pub string Greet(string name) { return "Bye, " + name; }
}
type Crowd {
    inject Greeter[] all;
    pub string First(string name) { return this.all[0].Greet(name); }
    pub string Both(string name) { return this.all[0].Greet(name) + " / " + this.all[1].Greet(name); }
    pub string Local(string name) { Greeter g = this.all[0]; return g.Greet(name); }
    pub i64 Count() { return Array.Len<Greeter>(this.all); }
}
unit Use(string text) { }
host ManyHost() {
    registry {
        single Hello for Greeter;
        single Bye for Greeter;
        single Crowd;
    }
    startup(Crowd crowd) {
        Use(crowd.First("Bo") + crowd.Both("Bo") + crowd.Local("Bo"));
        mut i64 count = crowd.Count();
    }
}
i32 Main() {
    launch ManyHost();
    return 0;
}
"#;

/// Element reads of a contract-typed injected array field through `this` (`this.all[0]`) are
/// contract values: a call on one dispatches through its witness table, and one written into a
/// contract-typed local passes through.
#[test]
fn elements_of_a_contract_typed_injected_array_read_through_this_dispatch() {
    let artifact = build(CROWD, 6735);
    for method in ["First", "Both", "Local"] {
        let body = clif(&artifact, method);
        assert!(body.contains("call_indirect"), "`{method}` dispatches through each element's table:\n{body}");
        assert!(!body.contains("__beskid_contract_box_reference_request_v5"), "an element is already a box:\n{body}");
    }
    assert_eq!(clif(&artifact, "Both").matches("call_indirect").count(), 2);
    assert_eq!(artifact.witness_tables.len(), 2, "Hello and Bye: {:?}", artifact.witness_tables);
}

/// The 0.6.7-preview.3 smoke repros (`c1`, `c2`, `d3`, `d4`, `d5`) as programs that check and
/// lower (`Core.Output.WriteLine` is a local `Use`: the fixture lowers the entry unit only).
/// `List.Get` returns `Result<T, string>`, so `c1` and `d4` unwrap it with `match`
/// (written as in the smoke, `Greeter got = list.Get(0);` is E1206, see
/// `a_result_written_into_a_contract_typed_local_is_a_type_mismatch`). `c1` keeps only its
/// launched host and its `Reception` (the fixture lowers every hook of every host); `c2`, `d3`
/// and `d5` cover `ManyHost` and `Crowd`.
#[test]
fn the_preview_smoke_contract_repros_build() {
    const C1: &str = r#"
use Core.Collections.List;
use Core.Results;
unit Use(string text) { }
contract Greeter {
    string Greet(string name);
}
type Hello : Greeter {
    pub string Greet(string name) { return "Hello, " + name; }
}
type Bye : Greeter {
    i64 times;
    pub string Greet(string name) { return "Bye, " + name; }
}
type Reception {
    inject Greeter greeter;
    pub string Welcome(string name) { return this.greeter.Greet(name); }
}
host AppHost() {
    registry {
        single Hello for Greeter;
        single Reception;
    }
    startup(Reception reception) {
        Use(reception.Welcome("Ada"));
    }
}
i32 Main() {
    mut List<Greeter> list = List.New<Greeter>();
    list.Push(Hello {});
    list.Push(Bye { times: 2 });
    mut i64 i = 0;
    while (i < list.Count()) {
        match list.Get(i) {
            Result::Ok(g) => { Use(g.Greet("Cy")); },
            Result::Error(e) => { Use(e); },
        };
        i += 1;
    }
    Greeter g = Bye { times: 3 };
    Use(g.Greet("Di"));
    launch AppHost();
    return 0;
}
"#;
    const D4: &str = r#"
use Core.Collections.List;
use Core.Results;
unit Use(string text) { }
contract Greeter {
    string Greet(string name);
}
type Hello : Greeter {
    pub string Greet(string name) { return "Hello, " + name; }
}
i32 Main() {
    mut List<Greeter> list = List.New<Greeter>();
    Greeter h = Hello {};
    list.Push(h);
    Greeter got = match list.Get(0) { Result::Ok(v) => v, Result::Error(e) => h, };
    Use(got.Greet("Cy"));
    return 0;
}
"#;
    let crowd = |body: &str| {
        format!(
            r#"
use Core.Collections.Array;
unit Use(string text) {{ }}
contract Greeter {{
    string Greet(string name);
}}
type Hello : Greeter {{
    pub string Greet(string name) {{ return "Hello, " + name; }}
}}
type Bye : Greeter {{
    pub string Greet(string name) {{ return "Bye, " + name; }}
}}
type Crowd {{
    inject Greeter[] all;
    {body}
    pub i64 Count() {{ return Array.Len<Greeter>(this.all); }}
}}
host ManyHost() {{
    registry {{
        single Hello for Greeter;
        single Bye for Greeter;
        single Crowd;
    }}
    startup(Crowd crowd) {{
        Use(crowd.Run("Bo"));
    }}
}}
i32 Main() {{
    launch ManyHost();
    return 0;
}}
"#
        )
    };
    let c2 =
        crowd(r#"pub string Run(string name) { return this.all[0].Greet(name) + " / " + this.all[1].Greet(name); }"#);
    let d3 = crowd("pub string Run(string name) { Greeter g = this.all[0]; return g.Greet(name); }");
    let d5 = crowd("pub string Run(string name) { return this.all[0].Greet(name); }");
    for (generation, (name, source)) in
        (6740_u64..).zip([("c1", C1), ("c2", c2.as_str()), ("d3", d3.as_str()), ("d4", D4), ("d5", d5.as_str())])
    {
        let artifact = build(source, generation);
        assert!(
            artifact.functions.iter().any(|function| function.function.display().to_string().contains("call_indirect")),
            "`{name}` dispatches at least one contract call: {:?}",
            function_names(&artifact)
        );
    }
}

/// `c1` as written in the smoke: a method call on the `Result` that `List.Get` returns is a
/// check error (E1257), not a lowering failure.
#[test]
fn the_smoke_method_call_on_a_list_get_result_is_a_check_error() {
    let errors = check_errors(
        &format!(
            "{CORELIB_GREETERS}
i64 Main() {{
    mut List<Greeter> list = List.New<Greeter>();
    list.Push(Hello {{}});
    Use(list.Get(0).Greet(\"Cy\"));
    return 0;
}}"
        ),
        6737,
    );
    assert!(errors.contains(&"E1257"), "{errors:?}");
}

/// The arguments of one `T` that are a contract value and conforming values bind `T` to the
/// contract whichever comes first: `Pick(Hello {}, greeter)` boxes `Hello {}` like
/// `Pick(greeter, Hello {})`.
#[test]
fn a_contract_argument_after_a_conforming_value_binds_the_contract() {
    let artifact = build(
        &format!(
            "{CORELIB_GREETERS}
T Pick<T>(T first, T second) {{ return second; }}
i64 Main() {{
    mut Greeter g = Bye {{ times: 3 }};
    Greeter first = Pick(Hello {{}}, g);
    Greeter second = Pick(g, Hello {{}});
    Use(first.Greet(\"a\"));
    Use(second.Greet(\"b\"));
    return 0;
}}"
        ),
        6736,
    );
    let names = function_names(&artifact);
    assert_eq!(
        names.iter().filter(|name| name.starts_with("Pick#generic_")).count(),
        1,
        "both calls specialize `Pick` at `T = Greeter`: {names:?}"
    );
    let main = clif(&artifact, "Main");
    assert_eq!(
        main.matches("__beskid_contract_box_reference_request_v5").count(),
        3,
        "the local and both `Hello {{}}` arguments are boxed:\n{main}"
    );
}

/// A host that is never launched (`ManyHost` in the smoke's `c1`): the real build lowers only
/// the items reachable from `Main`, and the hooks of a host that is never launched are not
/// among them (their services are never activated). The launched host's hooks are.
#[test]
fn hooks_of_a_host_that_is_never_launched_are_not_reachable() {
    let source = r#"
use Core.Collections.Array;
unit Use(string text) { }
contract Greeter {
    string Greet(string name);
}
type Hello : Greeter {
    pub string Greet(string name) { return "Hello, " + name; }
}
type Bye : Greeter {
    pub string Greet(string name) { return "Bye, " + name; }
}
type Reception {
    inject Greeter greeter;
    pub string Welcome(string name) { return this.greeter.Greet(name); }
}
type Crowd {
    inject Greeter[] all;
    pub i64 Count() { return Array.Len<Greeter>(this.all); }
    pub string First(string name) { return this.all[0].Greet(name); }
}
host AppHost() {
    registry {
        single Hello for Greeter;
        single Reception;
    }
    startup(Reception reception) {
        Use(reception.Welcome("Ada"));
    }
}
host ManyHost() {
    registry {
        single Hello for Greeter;
        single Bye for Greeter;
        single Crowd;
    }
    startup(Crowd crowd) {
        Use(crowd.First("Bo"));
    }
}
i32 Main() {
    launch AppHost();
    return 0;
}
"#;
    use std::sync::Arc;

    use beskid_analysis::composition::{CompositionInput, resolve_composition};

    let (input, isa, root) =
        canonical_corelib_host_project_fixture(&[("App/Main.bd", source)], SyntaxGenerationId(6738));
    let all = entry_items(&input, root);
    let errors = check_findings(input.database(), &all.iter().map(|item| item.key).collect::<Vec<_>>());
    assert!(errors.is_empty(), "the program checks clean: {errors:?}");
    let main = all.iter().find(|item| item.symbol == "Main").expect("Main").key;
    let reachable =
        beskid_queries::reachable_items(input.database(), root, main).expect("reachability").expect("reachable items");
    let reachable = all.iter().filter(|item| reachable.contains(&item.key)).cloned().collect::<Vec<_>>();
    let names = reachable.iter().map(|item| item.symbol.as_str()).collect::<Vec<_>>();
    assert!(names.iter().any(|name| name.starts_with("__BeskidHook_AppHost_startup_")), "{names:?}");
    assert!(!names.iter().any(|name| name.starts_with("__BeskidHook_ManyHost_")), "{names:?}");
    let result = resolve_composition(CompositionInput {
        program: &input.typed_program().assembly.entry_unit().program,
        is_mod_project: false,
        types: &beskid_queries::CompositionTypes::new(input.database(), input.typed_program()),
    });
    assert!(result.issues.is_empty(), "composition must validate: {:?}", result.issues);
    let mut snapshot = result.snapshot;
    snapshot.source_unit_path = Some(root.unit.path(input.database()).to_path_buf());
    let generation = input.typed_program().generation;
    let input = input
        .with_composition_authority(generation, Arc::new(result.plan), Arc::new(snapshot))
        .expect("attach frozen composition authority");
    let artifact = lower_syntax_program(&input, isa.as_ref(), &reachable)
        .unwrap_or_else(|error| panic!("the reachable items lower without an internal error: {error}"));
    beskid_codegen::validate_artifact(&artifact).expect("reachable artifact");
}

/// `==` between a contract value and a concrete value is no contract-value comparison: the
/// operand is not boxed for an operator, so the comparison is E1209 in either order.
#[test]
fn a_contract_value_compared_with_a_concrete_value_is_a_check_error() {
    for comparison in ["g == h", "h == g", "g != Hello {}"] {
        let errors = check_errors(
            &format!(
                "{CORELIB_GREETERS}
bool Main() {{
    Greeter g = Hello {{}};
    Hello h = Hello {{}};
    return {comparison};
}}"
            ),
            6740,
        );
        assert!(errors.contains(&"E1209"), "`{comparison}`: {errors:?}");
    }
}

const MARKED_SCORES: &str = r#"
use Core.Results;
use Core.Parallel;
use Core.Parallel.Scopes;
use Core.Parallel.Port;
contract Scored { Transferable; i64 Score(); }
contract Loose { i64 Score(); }
[Isolated]
type Low : Scored, Loose { pub i64 Score() { return 1_i64; } }
type Plain : Scored, Loose { pub i64 Score() { return 2_i64; } }
"#;

/// A contract value takes its transfer class from the markers its contract embeds: a contract
/// without one is not Transferable (a `Port<Loose>` is E1610), and a type that does not satisfy
/// the marker cannot be boxed into the contract (E1206), so a box never carries a
/// non-transferable value across a heap boundary.
#[test]
fn contract_values_cross_a_heap_boundary_only_through_a_marked_contract() {
    let port_of = |contract: &str, value: &str, generation: u64| {
        check_errors(
            &format!(
                "{MARKED_SCORES}
i64 Main() {{
    Result<i64, ParallelError> result = Parallel.Scope<i64>((Core.Parallel.Scopes.Scope s) => {{
        Port<{contract}> scores = s.PortWith<{contract}>(1_i64);
        {contract} value = {value};
        scores.Send(move value);
        return Result::Ok(0_i64);
    }});
    return 0_i64;
}}"
            ),
            generation,
        )
    };
    let marked = port_of("Scored", "Low {}", 6741);
    assert!(marked.is_empty(), "a marked contract with a Moved implementation crosses: {marked:?}");
    let unmarked = port_of("Loose", "Low {}", 6742);
    assert!(unmarked.contains(&"E1610"), "an unmarked contract is not Transferable: {unmarked:?}");
    let laundered = port_of("Scored", "Plain {}", 6743);
    assert!(laundered.contains(&"E1206"), "a type without a marker is not boxed into `Scored`: {laundered:?}");
    // A task captures the port and sends contract values of two implementations through it.
    let captured = check_errors(
        &format!(
            "{MARKED_SCORES}
[Isolated]
type High : Scored {{ i64 bonus; pub i64 Score() {{ return 100_i64 + this.bonus; }} }}
i64 Main() {{
    Result<i64, ParallelError> result = Parallel.Scope<i64>((Core.Parallel.Scopes.Scope s) => {{
        Port<Scored> scores = s.PortWith<Scored>(2_i64);
        Task<unit> producer = s.Spawn(() => {{
            Scored low = Low {{}};
            scores.Send(move low);
            Scored high = High {{ bonus: 5_i64 }};
            scores.Send(move high);
            scores.Close();
            return;
        }});
        mut i64 total = 0_i64;
        mut bool open = true;
        while open {{
            match scores.Receive() {{
                Result::Ok(v) => {{ total = total + v.Score(); }},
                Result::Error(_) => {{ open = false; }},
            }};
        }}
        return Result::Ok(total);
    }});
    return 0_i64;
}}"
        ),
        6744,
    );
    assert!(captured.is_empty(), "a captured port of a marked contract crosses: {captured:?}");
}
