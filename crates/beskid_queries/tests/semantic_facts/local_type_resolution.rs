//! A nominal type named in any type position -- a `let` declaration's declared type (and its
//! generic arguments), a parameter, a return type, a field, or an explicit call type argument --
//! for example `FiberError` in `Result<u8[], FiberError> joined = ...;` when the unit never
//! imports `Concurrency.FiberError` -- does not fail name resolution or type checking with a
//! clear diagnostic at the declaration; it silently resolves to nothing and only reappears later
//! as an opaque `SemanticError::unavailable` or ISLE `MissingRuleOrFact` wherever some downstream
//! layout/ABI query happens to need that position's layout. See
//! `crates/beskid_engine/tests/fixtures/heap_growth.bd`'s `RunFiberHeapInteraction`, which hit
//! exactly this shape (missing `use Concurrency.FiberError;`).
//!
//! `unresolved_type_reference` is the authoritative fact for that gap (E1201,
//! `beskid_queries::semantic_contract::legality`): it walks every type-bearing position of one
//! item exactly as `resolve_type_declaration` and `type_syntax_is_enclosing_generic_parameter_reference`
//! already require elsewhere, and reports the first name that does not resolve. It is wired into
//! production lowering through `beskid_queries::check_items`
//! (`beskid_codegen::module_emission::orchestration::lower_syntax_program`), which judges only the
//! items the caller actually asks to lower -- never every root of the assembly -- so the partial
//! synthetic assemblies several existing tests build (`beskid_codegen`'s
//! `isle_adapter::calls_conversions::canonical_foundation_*` and
//! `corelib_services::user_copy_of_foundation_output_cannot_import_the_panic_service`,
//! `beskid_queries`'s `incremental::typed_entry_state_uses_fast_resolution_when_stale` and
//! `runtime_fixture::real_runtime_fixtures_resolve_actual_module_signatures_and_constants`) stay
//! valid: they never reach `lower_syntax_program`, or they never name the items the gate would
//! reject.

use super::support;
use super::support::key;
use beskid_analysis::macros::{DEFAULT_MAX_MACRO_EXPANSION_DEPTH, expand_program};
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::parse_program;
use beskid_analysis::syntax_query::{NodeKind, SyntaxIndex};
use beskid_queries::{
    BeskidDatabase, ProjectSession, SourceUnitId, SyntaxGenerationId, build_typed_program, unresolved_type_reference,
};
use std::path::PathBuf;
use std::sync::Arc;

/// A generic result-shaped enum, self-contained so the fixture does not depend on corelib.
const RESULT_SOURCE: &str = "pub enum MyResult<T, E> { Ok(T value), Err(E error) }";

/// The error type deliberately kept in its own, separate module so referencing it by name alone
/// -- without a `use` -- is the only way the fixture's `let` declaration can name it.
const OTHER_ERROR_SOURCE: &str = "pub enum OtherError { Bad(i64 code) }";

struct Fixture {
    db: BeskidDatabase,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    index: SyntaxIndex,
}

fn fixture(import_other_error: bool) -> Fixture {
    let import_line = if import_other_error { "use Other.OtherError;\n" } else { "" };
    let main_source = format!(
        "use Result;\n{import_line}unit Main() {{ MyResult<i64, OtherError> outcome = MyResult::Ok(1_i64); return; }}"
    );
    fixture_with(main_source, "Other/OtherError.bd", OTHER_ERROR_SOURCE, 31)
}

/// A contract kept in its own one-declaration-per-file module, the shape of Corelib's
/// `Core.IO.Reader` (`pub contract Reader { ... }` in `Core/IO/Reader.bd`, imported by
/// `Core/IO/IO.bd` through `use Core.IO.Reader;` and named as a parameter type).
const OTHER_READER_SOURCE: &str = "pub contract Reader { i64 Read(i64 count); }";

fn contract_fixture(import_reader: bool, generation: u64) -> Fixture {
    let import_line = if import_reader { "use Other.Reader;\n" } else { "" };
    let main_source = format!("use Result;\n{import_line}i64 Drain(Reader reader) {{ return 0_i64; }}");
    fixture_with(main_source, "Other/Reader.bd", OTHER_READER_SOURCE, generation)
}

fn fixture_with(main_source: String, other_relative: &str, other_source: &str, generation: u64) -> Fixture {
    let mut db = BeskidDatabase::default();
    let root = PathBuf::from("/tmp/local-type-resolution/project/src");
    let main_path = root.join("Main.bd");
    let result_path = root.join("Result.bd");
    let other_error_path = root.join(other_relative);
    let main_program = expand_program(parse_program(&main_source).expect("main parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let result_program =
        expand_program(parse_program(RESULT_SOURCE).expect("result parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let other_error_program =
        expand_program(parse_program(other_source).expect("other unit parse"), DEFAULT_MAX_MACRO_EXPANSION_DEPTH);
    let generation = SyntaxGenerationId(generation);
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
                logical_name: result_path.display().to_string(),
                origin_path: result_path.clone(),
                path: result_path.clone(),
                source: RESULT_SOURCE.to_string(),
                program: result_program,
            },
            SourceUnit {
                logical_name: other_error_path.display().to_string(),
                origin_path: other_error_path.clone(),
                path: other_error_path.clone(),
                source: other_source.to_string(),
                program: other_error_program,
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
    // `build_typed_program` still succeeds either way: this fact is not wired into it (see the
    // module doc comment). Constructing the typed program here only proves the fixture assembly
    // itself is otherwise well-formed.
    build_typed_program(&mut db, project, generation, assembly).expect("fixture assembly parses and type-checks");
    let index = SyntaxIndex::from_program(&main_program, generation);
    Fixture { db, unit, generation, index }
}

#[test]
fn unimported_type_named_only_as_a_generic_argument_is_reported_at_the_declaration() {
    let fixture = fixture(false);
    let outcome = key(fixture.unit, fixture.generation, &fixture.index, NodeKind::LetStatement, 0);

    let fact = unresolved_type_reference(&fixture.db, outcome)
        .expect("unresolved_type_reference query")
        .expect("OtherError must not resolve without its import");
    assert_eq!(fact.site, outcome);
    assert_eq!(&*fact.name, "OtherError");
}

#[test]
fn imported_type_named_as_a_generic_argument_resolves() {
    let fixture = fixture(true);
    let outcome = key(fixture.unit, fixture.generation, &fixture.index, NodeKind::LetStatement, 0);

    assert_eq!(
        unresolved_type_reference(&fixture.db, outcome).expect("unresolved_type_reference query"),
        None,
        "OtherError resolves once imported"
    );
}

#[test]
fn unimported_type_named_bare_as_a_let_declaration_is_reported_at_the_let() {
    let (db, _project, unit, generation, index) = support::setup("unit Main() { Missing value = 0; return; }");
    let item = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let outcome = key(unit, generation, &index, NodeKind::LetStatement, 0);

    let fact = unresolved_type_reference(&db, item)
        .expect("unresolved_type_reference query")
        .expect("Missing must not resolve");
    assert_eq!(fact.site, outcome, "the let statement itself is the site, not only its generic arguments");
    assert_eq!(&*fact.name, "Missing");
}

#[test]
fn unimported_type_named_as_a_parameter_is_reported_at_the_parameter() {
    let (db, _project, unit, generation, index) = support::setup("unit Main(Missing value) { return; }");
    let item = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);
    let outcome = key(unit, generation, &index, NodeKind::Parameter, 0);

    let fact = unresolved_type_reference(&db, item)
        .expect("unresolved_type_reference query")
        .expect("Missing must not resolve as a parameter type");
    assert_eq!(fact.site, outcome);
    assert_eq!(&*fact.name, "Missing");
}

#[test]
fn unimported_type_named_as_a_return_type_is_reported_at_the_item() {
    let (db, _project, unit, generation, index) = support::setup("Missing Main() { return; }");
    let item = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);

    let fact = unresolved_type_reference(&db, item)
        .expect("unresolved_type_reference query")
        .expect("Missing must not resolve as a return type");
    assert_eq!(fact.site, item, "the return type has no separate node; the item itself is the site");
    assert_eq!(&*fact.name, "Missing");
}

#[test]
fn unimported_type_named_as_a_field_is_reported_at_the_field() {
    let (db, _project, unit, generation, index) = support::setup("type Holder { Missing value }");
    let item = key(unit, generation, &index, NodeKind::TypeDefinition, 0);
    let outcome = key(unit, generation, &index, NodeKind::Field, 0);

    let fact = unresolved_type_reference(&db, item)
        .expect("unresolved_type_reference query")
        .expect("Missing must not resolve as a field type");
    assert_eq!(fact.site, outcome);
    assert_eq!(&*fact.name, "Missing");
}

#[test]
fn unimported_type_named_as_an_explicit_call_type_argument_is_reported_at_the_call() {
    let (db, _project, unit, generation, index) =
        support::setup("unit Identity<T>(T value) { return; } unit Main() { Identity<Missing>(0); return; }");
    let item = key(unit, generation, &index, NodeKind::FunctionDefinition, 1);
    let outcome = key(unit, generation, &index, NodeKind::CallExpression, 0);

    let fact = unresolved_type_reference(&db, item)
        .expect("unresolved_type_reference query")
        .expect("Missing must not resolve as an explicit call type argument");
    assert_eq!(fact.site, outcome);
    assert_eq!(&*fact.name, "Missing");
}

#[test]
fn imported_contract_named_as_a_parameter_type_resolves() {
    let fixture = contract_fixture(true, 32);
    let item = key(fixture.unit, fixture.generation, &fixture.index, NodeKind::FunctionDefinition, 0);

    assert_eq!(
        unresolved_type_reference(&fixture.db, item).expect("unresolved_type_reference query"),
        None,
        "an imported contract is a legal parameter type (Core.IO.IO.bd `Read(Reader reader, ...)`)"
    );
}

#[test]
fn unimported_contract_named_as_a_parameter_type_is_still_reported() {
    let fixture = contract_fixture(false, 33);
    let item = key(fixture.unit, fixture.generation, &fixture.index, NodeKind::FunctionDefinition, 0);
    let outcome = key(fixture.unit, fixture.generation, &fixture.index, NodeKind::Parameter, 0);

    let fact = unresolved_type_reference(&fixture.db, item)
        .expect("unresolved_type_reference query")
        .expect("Reader must not resolve without its import");
    assert_eq!(fact.site, outcome);
    assert_eq!(&*fact.name, "Reader");
}

#[test]
fn same_unit_contract_named_as_a_parameter_type_resolves() {
    let (db, _project, unit, generation, index) = support::setup(
        "contract Sink { i64 Put(i64 value); } i64 Feed(Sink sink) { return 0_i64; }",
    );
    let item = key(unit, generation, &index, NodeKind::FunctionDefinition, 0);

    assert_eq!(unresolved_type_reference(&db, item).expect("unresolved_type_reference query"), None);
}
