//! Move checks at isolate crossings against the real Corelib `List<T>` and `Port<T>`
//! (`corelib/packages/foundation`, `corelib/packages/concurrency`), so the checker judges the
//! types the build lowers, not a private copy of them.
//!
//! - RB4: `move b.items` of a `List<T>` field at a port send is E1243 with the `exchange` help:
//!   the list's empty value carries copy-on-write sharing state, so it has no canonical empty
//!   value (before, `check` accepted it and `build` failed with an internal error).
//! - RB5: a plain parameter that the callee keeps is E1241; the review's repro
//!   `Keep(h, b); p.Send(move b)` is rejected; a `move` parameter may be kept; a fresh value may
//!   be returned.
//! - L4: `u8[] bytes = Read()?;` is owned, so it may cross.
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::canonical_corelib_service_capability;
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ResolvedDependencyProject, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SourceUnitId, build_typed_program_with_corelib_services, check_items,
    project_session_for_planned_syntax_assembly,
};
use std::path::Path;
use std::sync::Arc;

const PRELUDE: &str =
    "use Core.Collections.List;\nuse Core.Collections.Array;\nuse Core.Optional.Option;\nuse Core.Parallel.Port;\n";

fn package(name: &str) -> ResolvedDependencyProject {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages").join(name).canonicalize().unwrap();
    let project = format!("corelib_{name}");
    ResolvedDependencyProject {
        dependency_name: project.clone(),
        manifest_path: root.join(format!("{project}.bproj")),
        project_root: root.clone(),
        project_name: project,
        source_root: root.join("src"),
    }
}

/// `(code, message, help)` of every legality finding of the functions and methods in `main`.
fn findings(main: &str) -> Vec<(String, String, String)> {
    let root = tempfile::tempdir().unwrap();
    let source_root = root.path().canonicalize().unwrap().join("Src");
    std::fs::create_dir_all(&source_root).unwrap();
    let path = source_root.join("Main.bd");
    std::fs::write(&path, format!("{PRELUDE}{main}")).unwrap();
    let plan = CompilePlan {
        project_root: source_root.parent().unwrap().to_owned(),
        manifest_path: source_root.parent().unwrap().join("Host.bproj"),
        project_name: "Host".into(),
        source_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![package("foundation"), package("concurrency")],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None).unwrap(),
    );
    let mut db = BeskidDatabase::default();
    let project =
        project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "move-test".into()).unwrap();
    let manifest = AbiManifestV5::canonical_runtime(
        TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
            .unwrap(),
    );
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        assembly.generation,
        assembly.clone(),
        canonical_corelib_service_capability(&manifest).unwrap(),
    )
    .unwrap();
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let index = assembly.entry_syntax_index();
    let items = index
        .ids_of_kind(NodeKind::FunctionDefinition)
        .chain(index.ids_of_kind(NodeKind::MethodDefinition))
        .map(|node| AstNodeKey { unit, generation: assembly.generation, node })
        .collect::<Vec<_>>();
    check_items(&db, &items)
        .err()
        .unwrap_or_default()
        .into_iter()
        .map(|finding| {
            (finding.kind.code().to_string(), finding.kind.message(), finding.kind.help().unwrap_or_default())
        })
        .collect()
}

fn move_codes(main: &str) -> Vec<String> {
    findings(main)
        .into_iter()
        .map(|(code, ..)| code)
        .filter(|code| ("E1234"..="E1243").contains(&code.as_str()))
        .collect()
}

#[test]
fn a_list_field_has_no_canonical_empty_value_at_a_crossing() {
    let main = "pub type Bag { List<i64> items; }\n\
                pub unit Flush(Port<List<i64>> p, Bag b) { p.Send(move b.items); return; }\n";
    let report = findings(main);
    let (_, _, help) = report.iter().find(|(code, ..)| code == "E1243").unwrap_or_else(|| panic!("E1243: {report:?}"));
    assert!(help.contains("exchange("), "{help}");
    // An array field has one: the destructive read is accepted at a crossing.
    let array = "pub type Buffer { u8[] bytes; }\n\
                 pub unit Flush(Port<u8[]> p, Buffer b) { p.Send(move b.bytes); return; }\n";
    assert!(!move_codes(array).contains(&"E1243".to_string()), "{:?}", findings(array));
    // Taking the list out with `exchange` needs no canonical empty value.
    let exchanged = "pub type Bag { List<i64> items; }\n\
                     pub unit Flush(Port<List<i64>> p, Bag b) { p.Send(exchange(b.items, List.New<i64>())); return; }\n";
    assert!(!move_codes(exchanged).contains(&"E1243".to_string()), "{:?}", findings(exchanged));
}

#[test]
fn a_kept_borrowed_parameter_is_rejected_and_the_review_repro_does_not_compile() {
    // The review's repro: `Keep` stores its plain parameter, the caller then sends the same array.
    let repro = "pub type Holder { u8[] buf; }\n\
                 pub unit Keep(Holder h, u8[] b) { h.buf = b; return; }\n\
                 pub unit Main(Port<u8[]> p, Holder h) { u8[] b = Array.Empty<u8>(); Keep(h, b); p.Send(move b); return; }\n";
    let report = findings(repro);
    let kept = report
        .iter()
        .find(|(code, message, _)| code == "E1241" && message.contains("parameter `b`"))
        .unwrap_or_else(|| panic!("E1241 at the store: {report:?}"));
    assert_eq!(kept.2, "copy it, or declare the parameter `move`");
    // With `move` the callee keeps its own value; the caller's `b` now has another name (the
    // callee's), so it can no longer cross: E1241 "copy it first" at the send.
    let declared = "pub type Holder { u8[] buf; }\n\
                    pub unit Keep(Holder h, move u8[] b) { h.buf = b; return; }\n\
                    pub unit Main(Port<u8[]> p, Holder h) { u8[] b = Array.Empty<u8>(); Keep(h, b); p.Send(move b); return; }\n";
    let report = findings(declared);
    assert!(!report.iter().any(|(_, message, _)| message.contains("parameter `b` is borrowed")), "{report:?}");
    assert!(
        report.iter().any(|(code, message, _)| code == "E1241" && message.contains("not the only name")),
        "{report:?}"
    );
    // Storing a `move` parameter and returning a fresh value are accepted.
    let accepted = "pub type Holder { u8[] buf; }\n\
                    pub unit Keep(Holder h, move u8[] b) { h.buf = b; return; }\n\
                    pub u8[] Fresh(u8[] seed) { u8[] made = Array.Copy<u8>(seed); return made; }\n";
    assert_eq!(move_codes(accepted), Vec::<String>::new(), "{:?}", findings(accepted));
}

#[test]
fn a_copied_list_is_not_another_name_at_a_crossing() {
    // `kept = items` copies the list (`[CopyOnAssign]`), so `items` is still the only name for
    // its value; the send detaches it from the storage `kept` shares.
    let main = "pub unit Main(Port<List<i64>> p) { List<i64> items = List.New<i64>(); List<i64> kept = items; \
                p.Send(move items); i64 n = kept.Count(); return; }\n";
    assert_eq!(move_codes(main), Vec::<String>::new(), "{:?}", findings(main));
}

#[test]
fn a_try_payload_of_a_call_result_is_owned_at_a_crossing() {
    let main = "pub Option<u8[]> Read() { return Option::Some(Array.Empty<u8>()); }\n\
                pub Option<i64> Main(Port<u8[]> p) { u8[] bytes = Read()?; p.Send(move bytes); return Option::Some(0); }\n";
    assert_eq!(move_codes(main), Vec::<String>::new(), "{:?}", findings(main));
}

#[test]
fn a_nested_list_cannot_cross_but_a_top_level_list_can() {
    // A list in a field (or a list of lists) may share its array with a list this isolate keeps;
    // the send detaches only a top-level list, so a nested one is E1241 "copy it first".
    let nested = "[Isolated]\npub type Bag { List<i64> items; }\n\
                  pub unit Main(Port<Bag> p) { List<i64> kept = List.New<i64>(); Bag bag = Bag { items: kept }; \
                  p.Send(move bag); return; }\n";
    let report = findings(nested);
    let (_, message, help) = report
        .iter()
        .find(|(code, message, _)| code == "E1241" && message.contains("nested `List`"))
        .unwrap_or_else(|| panic!("nested list E1241: {report:?}"));
    assert!(message.contains("`bag`"), "{message}");
    assert!(help.starts_with("copy it first"), "{help}");
    let lists = "pub unit Main(Port<List<List<i64>>> p) { List<List<i64>> outer = List.New<List<i64>>(); \
                 p.Send(move outer); return; }\n";
    assert!(
        findings(lists).iter().any(|(code, message, _)| code == "E1241" && message.contains("nested")),
        "{:?}",
        findings(lists)
    );
    let flat = "pub unit Main(Port<List<i64>> p) { List<i64> items = List.New<i64>(); p.Send(move items); return; }\n";
    assert_eq!(move_codes(flat), Vec::<String>::new(), "{:?}", findings(flat));
}
