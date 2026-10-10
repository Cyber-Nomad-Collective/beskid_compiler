//! `beskid check` on an entry-less Corelib library judges every own unit as a root. These
//! regressions pin the checker false positives that the first full checks of
//! `corelib_bsol`, `corelib_serialization` and `corelib_glue` exposed: each source is valid
//! Beskid and must produce no error through the same query-backed prepare the CLI runs.
use std::sync::Arc;

use beskid_analysis::Severity;
use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::{
    PrepareOptions, ResolvedInput, parse_program_with_source_name, synthetic_compile_plan_for_source,
};
use beskid_analysis::syntax::SyntaxGenerationId;
use beskid_analysis::syntax_query::SyntaxIndex;
use beskid_queries::{BeskidDatabase, prepare_compilation_diagnostics_with_db};

const RESULTS: (&str, &str) =
    ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }");

/// Error diagnostics of `units[0]` prepared as the entry of an import-closure assembly.
fn errors(units: &[(&str, &str)]) -> Vec<String> {
    let directory = tempfile::tempdir().unwrap();
    let generation = SyntaxGenerationId(611);
    let units = units
        .iter()
        .map(|(relative, source)| {
            let path = directory.path().join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, source).unwrap();
            SourceUnit {
                logical_name: (*relative).into(),
                origin_path: path.clone(),
                program: parse_program_with_source_name(path.to_str().unwrap(), source).unwrap(),
                path,
                source: (*source).into(),
            }
        })
        .collect::<Vec<_>>();
    let plan = synthetic_compile_plan_for_source(&units[0].path);
    let roots = EffectiveCompilationRoots {
        host: RootEntry { dependency_name: None, source_root: directory.path().into() },
        dependencies: vec![],
    };
    let indexes = units.iter().map(|unit| SyntaxIndex::from_program(&unit.program, generation)).collect::<Vec<_>>();
    let index = Arc::new(ModuleIndex::build(&units, &indexes, &roots, &plan));
    let resolved = ResolvedInput {
        source_path: units[0].path.clone(),
        source: units[0].source.clone(),
        compile_plan: Some(plan),
        prepared_workspace: None,
        workspace_summary: None,
        assembly: Some(ProgramAssembly::new(
            roots,
            Arc::new(units),
            0,
            AssemblyDiscovery::ImportClosure,
            index,
            false,
            generation,
        )),
    };
    let mut db = BeskidDatabase::default();
    let (_, diagnostics, _) = prepare_compilation_diagnostics_with_db(&mut db, &resolved, PrepareOptions::without_mods(), None)
        .expect("prepare diagnostics");
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| format!("{:?} {} @{:?}", diagnostic.code, diagnostic.message, diagnostic.span))
        .collect()
}

#[test]
fn qualified_enum_paths_resolve_by_module_not_by_simple_name() {
    let found = errors(&[
        (
            "Main.bd",
            "use Repro.A; use Repro.B; \
             pub bool IsGamma(B.Kind k) { return match k { B.Kind::Gamma => true, }; } \
             pub bool IsAlpha(A.Kind k) { return match k { A.Kind::Alpha => true, A.Kind::Beta => false, }; } \
             pub B.Kind Make() { return B.Kind::Gamma; }",
        ),
        ("Repro/A.bd", "pub enum Kind { Alpha, Beta, }"),
        ("Repro/B.bd", "pub enum Kind { Gamma, }"),
    ]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn colliding_enum_names_still_reject_unknown_variants() {
    let found = errors(&[
        ("Main.bd", "use Repro.A; use Repro.B; pub B.Kind Make() { return B.Kind::Alpha; }"),
        ("Repro/A.bd", "pub enum Kind { Alpha, Beta, }"),
        ("Repro/B.bd", "pub enum Kind { Gamma, }"),
    ]);
    assert!(found.iter().any(|error| error.contains("E1301")), "{found:#?}");
}

#[test]
fn match_arm_block_that_returns_does_not_produce_a_unit_value() {
    let found = errors(&[
        (
            "Main.bd",
            "use Core.Results; \
             pub Result<i64, string> Forward(Result<i64, string> r) { \
                 return match r { \
                     Result::Error(e) => Result::Error(e), \
                     Result::Ok(v) => { if (v > 0_i64) { return Result::Ok(v); } return Result::Ok(0_i64); }, \
                 }; \
             } \
             pub i64 Bound(Result<i64, string> r) { \
                 i64 x = match r { Result::Ok(v) => v, Result::Error(e) => { return 0_i64; }, }; \
                 return x; \
             }",
        ),
        RESULTS,
    ]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn match_value_in_conditions_nested_in_statements() {
    let found = errors(&[
        (
            "Main.bd",
            "use Repro.A; \
             pub bool Loop(A.Kind[] kinds) { \
                 mut i64 i = 0_i64; \
                 while (i < 2_i64) { \
                     if (match kinds[i] { A.Kind::Alpha => true, _ => false, }) { return true; } \
                     i = i + 1_i64; \
                 } \
                 return false; \
             } \
             pub bool Arm(A.Kind outer, A.Kind inner) { \
                 match outer { \
                     A.Kind::Alpha => { if (!(match inner { A.Kind::Beta => true, _ => false, })) { return false; } return true; }, \
                     _ => { return false; }, \
                 }; \
                 return false; \
             }",
        ),
        ("Repro/A.bd", "pub enum Kind { Alpha, Beta, }"),
    ]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn impl_where_bounds_govern_method_bodies() {
    let found = errors(&[
        (
            "Main.bd",
            "use Core.Results; \
             pub contract Source { Result<i64, string> Next(); } \
             pub contract Sink<T, R> { Result<T, string> Read(R reader); } \
             Result<unit, string> Check<R>(R reader) where R: Source { i64 n = reader.Next()?; return Result::Ok(()); } \
             pub type Wrap<R> { i64 dummy, } \
             impl<R> Wrap<R> : Sink<i64, R> where R: Source { \
                 pub Result<i64, string> Read(R reader) { \
                     Check<R>(reader)?; \
                     i64 n = reader.Next()?; \
                     return Result::Ok(n); \
                 } \
             }",
        ),
        RESULTS,
    ]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn generic_call_infers_parameter_from_earlier_argument() {
    let found = errors(&[
        (
            "Main.bd",
            "use Core.Results; \
             pub type Box<T> { T value, } \
             pub unit Put<T>(Box<T> b, T v) { } \
             pub unit UsePut(Box<Result<i64, string>> b) { Put(b, Result::Error(\"x\")); Put(b, Result::Ok(1_i64)); }",
        ),
        RESULTS,
    ]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn impl_bounds_relating_several_parameters_govern_method_bodies() {
    let found = errors(&[("Main.bd", CONTAINER_DECODER), RESULTS]);
    assert!(found.is_empty(), "{found:#?}");
}

const CONTAINER_DECODER: &str = "use Core.Results; \
     pub contract Source { Result<i64, string> Next(); } \
     pub contract FieldDecoder<T, R> { Result<T, string> Read(R reader); } \
     Result<unit, string> Count<R>(R reader, i64 count) where R: Source { return Result::Ok(()); } \
     pub type ArrayBinding<T, R, B> { B element, } \
     impl<T, R, B> ArrayBinding<T, R, B> : FieldDecoder<T[], R> where R: Source, B: FieldDecoder<T, R> { \
         pub Result<T[], string> Read(R reader) { \
             i64 count = reader.Next()?; \
             Count<R>(reader, count)?; \
             T value = this.element.Read(reader)?; \
             return Result::Ok([value]); \
         } \
     }";

#[test]
fn qualified_generic_call_with_enclosing_type_argument_and_bound_decoder_method() {
    let found = errors(&[
        (
            "Main.bd",
            "use Core.Results; use Repro.Registry; use Repro.Decoding; \
             pub Result<T, string> ReadTyped<T, R, B>(R reader, B binding) where R: Decoding.Source, B: Decoding.FieldDecoder<T, R> { \
                 Registry.RegisterCompiled<T>(1_i64)?; \
                 let decoder = Decoding.Bind<T, R, B>(reader, binding); \
                 return decoder.ReadTyped(); \
             }",
        ),
        ("Repro/Registry.bd", "use Core.Results; pub Result<unit, string> RegisterCompiled<T>(i64 limit) { return Result::Ok(()); }"),
        (
            "Repro/Decoding.bd",
            "use Core.Results; \
             pub contract Source { Result<i64, string> Next(); } \
             pub contract FieldDecoder<T, R> { Result<T, string> Read(R reader); } \
             pub contract Decoder<T> { Result<T, string> ReadTyped(); } \
             type BoundDecoder<T, R, B> { R reader, B binding, } \
             impl<T, R, B> BoundDecoder<T, R, B> : Decoder<T> where R: Source, B: FieldDecoder<T, R> { \
                 pub Result<T, string> ReadTyped() { T value = this.binding.Read(this.reader)?; return Result::Ok(value); } \
             } \
             pub BoundDecoder<T, R, B> Bind<T, R, B>(R reader, B binding) where R: Source, B: FieldDecoder<T, R> { \
                 return BoundDecoder<T, R, B> { reader: reader, binding: binding }; \
             }",
        ),
        RESULTS,
    ]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn empty_array_return_in_diverging_statement_match_arm_takes_the_return_type() {
    let found = errors(&[
        (
            "Main.bd",
            "use Repro.Nodes; use Repro.Check; \
             pub Item[] Body(Kind kind, Item[] items) { \
                 match kind { Kind::Alpha => { return items; }, _ => { Check.Fail(\"expected alpha\"); return []; }, }; \
             } \
             pub i64[] Numbers(Kind kind) { \
                 match kind { Kind::Alpha => { return [1_i64]; }, _ => { return []; }, }; \
             } \
             pub i64[] Empty() { return []; }",
        ),
        ("Repro/Nodes.bd", "pub enum Kind { Alpha, Beta, } pub type Item { pub i64 value, }"),
        ("Repro/Check.bd", "pub unit Fail(string message) { }"),
    ]);
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn index_subscript_is_not_typed_by_the_expected_element_type() {
    let found = errors(&[(
        "Main.bd",
        "pub bool Same<T>(T left, T right) { return true; } \
         pub bool Last(u8[] bytes) { return Same(bytes[9999], 7_u8); }",
    )]);
    assert!(found.is_empty(), "{found:#?}");
}
