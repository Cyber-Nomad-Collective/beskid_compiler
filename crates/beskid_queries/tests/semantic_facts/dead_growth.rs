//! `BSP-REQ-35580A7D7B75` (`DeadCollectionGrowth`): a discarded canonical growth of a `mut T[]`
//! parameter that the body never publishes is rejected; every legal corelib shape is not.

use super::support::{key, key_at_start};
use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId, build_typed_program,
    dead_collection_growth,
};
use std::path::PathBuf;
use std::sync::Arc;

/// The canonical growth declaration shape from `corelib/packages/foundation/src/Core/Collections/Array.bd`.
const ARRAY_SOURCE: &str = "pub T[] Append<T>(mut T[] values, T value) { return values; } pub i64 Len<T>(T[] values) { return 0_i64; }";

struct Fixture {
    db: BeskidDatabase,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: SyntaxIndex,
    typed: Result<(), String>,
}

fn fixture(main_body: &str) -> Fixture {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/dead-growth/project/src");
    let main_path = root.join("Main.bd");
    let array_path = root.join("Core/Collections/Array.bd");
    let main_source = format!("use Core.Collections.Array;\n{main_body}");
    let main_program = expand_program(parse_program(&main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let array_program =
        expand_program(parse_program(ARRAY_SOURCE).expect("array parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(23);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: Vec::new(),
        },
        Arc::new(vec![
            SourceUnit {
                logical_name: main_path.display().to_string(),
                origin_path: main_path.clone(),
                path: main_path.clone(),
                source: main_source.clone(),
                program: main_program.clone(),
            },
            SourceUnit {
                logical_name: array_path.display().to_string(),
                origin_path: array_path.clone(),
                path: array_path.clone(),
                source: ARRAY_SOURCE.to_string(),
                program: array_program,
            },
        ]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let unit = SourceUnitId::new(&db, main_path);
    let project = ProjectSession::new(
        &db,
        root.parent().expect("project root").to_path_buf(),
        unit.path(&db).clone(),
        "App".to_string(),
        "lock".to_string(),
    );
    let typed = build_typed_program(&mut db, project, generation, assembly).map(|_| ()).map_err(|error| error.to_string());
    let index = SyntaxIndex::from_program(&main_program, generation);
    Fixture { db, unit, generation, index, typed }
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
    let error = fixture.typed.clone().expect_err("typed program must fail closed on dead growth");
    assert!(error.contains("DeadCollectionGrowth"), "{error}");
    assert!(error.contains("CallExpression@"), "diagnostic must name the growth call site: {error}");
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
    assert!(fixture.typed.is_err());
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
        assert!(fixture.typed.is_err(), "{body}");
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
    }
}
