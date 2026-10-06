//! `BSP-REQ-35580A7D7B75` (`DeadCollectionGrowth`, E1231): a discarded canonical growth of a
//! `mut T[]` parameter that the body never publishes is rejected by the reachability-scoped
//! legality gate for the item that contains it; every legal corelib shape is not. Admission of the
//! typed program never judges bodies, so an unreachable item cannot poison an assembly.
//!
//! Collection growth authority belongs only to the exact compiler-owned Foundation
//! `Core/Collections/Array.bd` unit admitted by `build_typed_program_with_corelib_services`. The
//! fixture therefore assembles the real Foundation package; a look-alike `Array.bd` in the host
//! source root receives no collection or dead-growth authority.

use super::support::{key, key_at_start};
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_abi::runtime_source::canonical_corelib_service_capability;
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ResolvedDependencyProject, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, SourceUnitId, SyntaxGenerationId, build_typed_program_with_corelib_services,
    collection_operation, dead_collection_growth, project_session_for_planned_syntax_assembly,
};
use std::path::Path;
use std::sync::Arc;

/// A host-local look-alike of the canonical growth declaration shape. It is not the
/// compiler-owned Foundation unit and must never acquire collection authority.
const LOOKALIKE_ARRAY_SOURCE: &str =
    "pub T[] Append<T>(mut T[] values, T value) { return values; } pub i64 Len<T>(T[] values) { return 0_i64; }";

/// Where `Core.Collections.Array` comes from in a fixture.
#[derive(Clone, Copy)]
enum ArraySource {
    /// The real Foundation package dependency (canonical, admitted).
    CanonicalFoundation,
    /// A host-local `Core/Collections/Array.bd` with the given bytes and no Foundation dependency.
    HostLocal(&'static str),
}

struct Fixture {
    _root: tempfile::TempDir,
    db: BeskidDatabase,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: SyntaxIndex,
    typed: Result<(), String>,
    /// E-codes the legality gate reports for every function of the main unit.
    gate: Vec<String>,
}

fn fixture(main_body: &str) -> Fixture {
    fixture_with(main_body, ArraySource::CanonicalFoundation)
}

fn fixture_with(main_body: &str, array: ArraySource) -> Fixture {
    let root = tempfile::tempdir().expect("fixture root");
    let source_root = root.path().canonicalize().expect("canonical fixture root").join("src");
    std::fs::create_dir_all(&source_root).expect("source root");
    if let ArraySource::HostLocal(source) = array {
        std::fs::create_dir_all(source_root.join("Core/Collections")).expect("look-alike directory");
        std::fs::write(source_root.join("Core/Collections/Array.bd"), source).expect("look-alike Array.bd");
        // A byte-exact copy declares `pub mod Core.Collections.Array.ArrayIter;`.
        std::fs::create_dir_all(source_root.join("Core/Collections/Array")).expect("look-alike module directory");
        std::fs::write(
            source_root.join("Core/Collections/Array/ArrayIter.bd"),
            include_str!("../../../../corelib/packages/foundation/src/Core/Collections/Array/ArrayIter.bd"),
        )
        .expect("look-alike ArrayIter.bd");
    }
    let foundation = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/foundation")
        .canonicalize()
        .expect("canonical Foundation package");
    let main_path = source_root.join("Main.bd");
    std::fs::write(&main_path, format!("use Core.Collections.Array;\n{main_body}")).expect("Main.bd");
    let plan = CompilePlan {
        project_root: source_root.parent().expect("project root").to_owned(),
        manifest_path: source_root.parent().expect("project root").join("App.bproj"),
        project_name: "App".into(),
        source_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: match array {
            ArraySource::CanonicalFoundation => vec![ResolvedDependencyProject {
                dependency_name: "corelib_foundation".into(),
                manifest_path: foundation.join("corelib_foundation.bproj"),
                project_root: foundation.clone(),
                project_name: "corelib_foundation".into(),
                source_root: foundation.join("src"),
            }],
            ArraySource::HostLocal(_) => Vec::new(),
        },
        unresolved_dependencies: Vec::new(),
        has_std_dependency: false,
    };
    let assembly = Arc::new(
        assemble_program_with_materializer(&plan, None, &main_path, None, &AssemblyOptions::default(), None, None)
            .expect("assemble dead-growth fixture"),
    );
    let mut db = BeskidDatabase::default();
    let project = project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &main_path, "lock".into())
        .expect("project session");
    let manifest = AbiManifestV5::canonical_runtime(
        TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
            .expect("linux target"),
    );
    let generation = assembly.generation;
    let typed = build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        Arc::clone(&assembly),
        canonical_corelib_service_capability(&manifest).expect("corelib capability"),
    )
    .map(|_| ())
    .map_err(|error| error.to_string());
    let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
    let index = assembly.entry_syntax_index().clone();
    let items = index
        .ids_of_kind(NodeKind::FunctionDefinition)
        .map(|node| AstNodeKey { unit, generation, node })
        .collect::<Vec<_>>();
    let gate = match beskid_queries::check_items(&db, &items) {
        Ok(()) => Vec::new(),
        Err(findings) => findings.iter().map(|finding| finding.kind.code().to_string()).collect(),
    };
    Fixture { _root: root, db, unit, generation, index, typed, gate }
}

/// The function definition that contains `node`.
fn parent_item(fixture: &Fixture, node: AstNodeKey) -> AstNodeKey {
    let mut current = node.node;
    loop {
        if fixture.index.kind(current) == Some(NodeKind::FunctionDefinition) {
            return AstNodeKey { node: current, ..node };
        }
        current = fixture.index.metadata()[current.0 as usize].parent.expect("call is inside a function");
    }
}

fn growth_call(fixture: &Fixture, occurrence: usize) -> AstNodeKey {
    key(fixture.unit, fixture.generation, &fixture.index, NodeKind::CallExpression, occurrence)
}

#[test]
fn discarded_growth_of_an_unpublished_mut_array_parameter_is_dead() {
    let body = "unit Push(mut u8[] buffer, u8 value) { Array.Append<u8>(buffer, value); }";
    let fixture = fixture(body);
    let source = format!("use Core.Collections.Array;\n{body}");
    let parameter = key_at_start(
        fixture.unit,
        fixture.generation,
        &fixture.index,
        NodeKind::Identifier,
        source.find("buffer, u8 value").expect("parameter declaration"),
    );
    let call = growth_call(&fixture, 0);
    let fact = dead_collection_growth(&fixture.db, call).expect("dead growth query").expect("dead growth fact");
    assert_eq!(fact, beskid_queries::DeadCollectionGrowth { call, parameter });
    assert!(fixture.typed.is_ok(), "typed program admission does not judge bodies: {:?}", fixture.typed);
    assert_eq!(fixture.gate, vec!["E1231".to_string()], "the legality gate rejects the dead growth");
    let findings = beskid_queries::check_items(&fixture.db, &[parent_item(&fixture, call)]).expect_err("gate rejects");
    assert_eq!(findings[0].site, call, "E1231 is reported at the growth call");
}

#[test]
fn every_discarded_growth_of_the_same_dead_parameter_is_reported() {
    // The pre-fix corelib `AppendCrlf` shape: two appends, nothing returned.
    let fixture = fixture("unit Crlf(mut u8[] destination) { Array.Append<u8>(destination, u8(13)); Array.Append<u8>(destination, u8(10)); }");
    let calls = fixture
        .index
        .ids_of_kind(NodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: fixture.unit, generation: fixture.generation, node })
        .filter(|call| dead_collection_growth(&fixture.db, *call).expect("dead growth query").is_some())
        .count();
    assert_eq!(calls, 2);
    assert_eq!(fixture.gate, vec!["E1231".to_string(), "E1231".to_string()]);
}

#[test]
fn index_reads_and_self_assignment_do_not_publish_the_parameter() {
    for body in [
        "unit Push(mut u8[] buffer) { Array.Append<u8>(buffer, buffer[0_i64]); }",
        "unit Push(mut u8[] buffer, u8 value) { buffer[0_i64] = value; Array.Append<u8>(buffer, value); }",
        "unit Push(mut u8[] buffer, u8[] other, u8 value) { buffer = other; Array.Append<u8>(buffer, value); }",
    ] {
        let fixture = fixture(body);
        let dead = fixture
            .index
            .ids_of_kind(NodeKind::CallExpression)
            .map(|node| AstNodeKey { unit: fixture.unit, generation: fixture.generation, node })
            .any(|call| dead_collection_growth(&fixture.db, call).expect("dead growth query").is_some());
        assert!(dead, "{body}");
        assert_eq!(fixture.gate, vec!["E1231".to_string()], "{body}");
    }
}

#[test]
fn fixed_corelib_shapes_and_published_handles_are_not_dead() {
    for body in [
        // corelib Http/Codec.bd AppendCrlf: the parameter is returned after growth.
        "u8[] AppendCrlf(mut u8[] destination) { Array.Append<u8>(destination, u8(13)); Array.Append<u8>(destination, u8(10)); return destination; }",
        // corelib Http/Codec.bd AppendText: growth inside a loop, then the parameter is returned.
        "u8[] AppendText(mut u8[] destination, u8[] source) { mut i64 index = 0_i64; while index < Array.Len<u8>(source) { Array.Append<u8>(destination, source[index]); index = index + 1_i64; } return destination; }",
        // corelib Set.bd/List.bd/Args.bd/Slice.bd: the owner is a mutable local, not a parameter.
        "u8[] Build(u8[] seed, u8 value) { mut u8[] output = seed; Array.Append<u8>(output, value); return output; }",
        // The grown handle is bound.
        "u8[] Push(mut u8[] buffer, u8 value) { u8[] grown = Array.Append<u8>(buffer, value); return grown; }",
        // The grown handle is returned directly.
        "u8[] Push(mut u8[] buffer, u8 value) { return Array.Append<u8>(buffer, value); }",
        // The grown handle is rebound onto the parameter and the parameter is returned.
        "u8[] Push(mut u8[] buffer, u8 value) { buffer = Array.Append<u8>(buffer, value); return buffer; }",
        // The parameter is handed to another callee after growth.
        "unit Consume(u8[] bytes) { return; } unit Push(mut u8[] buffer, u8 value) { Array.Append<u8>(buffer, value); Consume(buffer); }",
        // The parameter is published through a struct field.
        "type Box { u8[] items, } Box Push(mut u8[] buffer, u8 value) { Array.Append<u8>(buffer, value); return Box { items: buffer }; }",
        // The parameter is published before growth: a binding copy can outlive the call.
        "u8[] Push(mut u8[] buffer, u8 value) { u8[] before = buffer; Array.Append<u8>(buffer, value); return before; }",
        // The parameter is captured by a nested lambda.
        "unit Push(mut u8[] buffer, u8 value) { Array.Append<u8>(buffer, value); let capture = () => buffer; return; }",
        // Length is read through a call argument.
        "unit Push(mut u8[] buffer, u8 value) { if Array.Len<u8>(buffer) > 4_i64 { return; } Array.Append<u8>(buffer, value); }",
        // A user-declared Append lookalike is not a canonical growth operation.
        "unit Append<T>(mut T[] values, T value) { return; } unit Push(mut u8[] buffer, u8 value) { Append<u8>(buffer, value); }",
        // A non-`mut` parameter is the existing unproven-owner lowering error, not dead growth.
        "unit Push(u8[] buffer, u8 value) { Array.Append<u8>(buffer, value); }",
    ] {
        let fixture = fixture(body);
        let dead = fixture
            .index
            .ids_of_kind(NodeKind::CallExpression)
            .map(|node| AstNodeKey { unit: fixture.unit, generation: fixture.generation, node })
            .filter(|call| dead_collection_growth(&fixture.db, *call).expect("dead growth query").is_some())
            .count();
        assert_eq!(dead, 0, "{body}");
        assert!(fixture.typed.is_ok(), "{body}: {:?}", fixture.typed);
        assert!(fixture.gate.is_empty(), "{body}: {:?}", fixture.gate);
    }
}

#[test]
fn lookalike_host_array_source_has_no_collection_or_dead_growth_authority() {
    let body = "unit Push(mut u8[] buffer, u8 value) { Array.Append<u8>(buffer, value); }";
    let canonical_bytes = include_str!("../../../../corelib/packages/foundation/src/Core/Collections/Array.bd");
    // Both a hand-written look-alike and a byte-exact copy outside the compiler-owned Foundation
    // root are ordinary host units: the call stays a direct call without collection authority.
    for array in [ArraySource::HostLocal(LOOKALIKE_ARRAY_SOURCE), ArraySource::HostLocal(canonical_bytes)] {
        let fixture = fixture_with(body, array);
        let calls = fixture
            .index
            .ids_of_kind(NodeKind::CallExpression)
            .map(|node| AstNodeKey { unit: fixture.unit, generation: fixture.generation, node })
            .collect::<Vec<_>>();
        assert!(!calls.is_empty());
        for call in calls {
            assert!(collection_operation(&fixture.db, call).expect("collection query").is_none());
            assert!(dead_collection_growth(&fixture.db, call).expect("dead growth query").is_none());
        }
        assert!(!fixture.gate.iter().any(|code| code == "E1231"), "{:?}", fixture.gate);
    }
}
