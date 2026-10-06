//! Parity of the query-backed `beskid check` gate (`semantic_diagnostics_for_roots`) with the
//! legacy `beskid_analysis` `TypeError` and `ResolveError` classes.
//!
//! Each positive case is one legacy diagnostic class rendered by a minimal single-unit library
//! root; the query gate alone (no legacy checker runs here) must report the same diagnostic
//! code. The coverage table in `docs/reports/2026-10-05-v06-query-check-authority.md` names the
//! query obligation behind each code. The negative cases are the Corelib shapes the legacy
//! checker rejected wrongly when every own unit became a root; the query gate must stay silent.

use std::sync::Arc;

use beskid_analysis::projects::{
    AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
};
use beskid_analysis::services::{SemanticFactFinding, parse_program_with_source_name, synthetic_compile_plan_for_source};
use beskid_analysis::syntax::SyntaxGenerationId;
use beskid_analysis::syntax_query::SyntaxIndex;

use super::semantic_diagnostics_for_roots;
use crate::BeskidDatabase;

/// Judge `source` as the single own root of a library through the query gate only.
fn root_findings(source: &str, generation: u64) -> Vec<SemanticFactFinding> {
    root_findings_with_glue(source, generation, &[])
}

/// [`root_findings`] for an assembly whose host manifest declares `glue_libraries` as Rust Glue
/// owner blocks.
fn root_findings_with_glue(source: &str, generation: u64, glue_libraries: &[&str]) -> Vec<SemanticFactFinding> {
    let root = std::env::temp_dir().join(format!("beskid_query_check_parity_{}_{generation}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let source_root = root.join("src");
    std::fs::create_dir_all(&source_root).expect("library source root");
    let path = source_root.join("Main.bd");
    std::fs::write(&path, source).expect("library source");
    let generation = SyntaxGenerationId(generation);
    let unit = SourceUnit {
        logical_name: "Main".to_string(),
        origin_path: path.clone(),
        program: parse_program_with_source_name(path.to_str().expect("utf-8 path"), source)
            .unwrap_or_else(|error| panic!("fixture must parse: {error}\n{source}")),
        path: path.clone(),
        source: source.to_string(),
    };
    let plan = synthetic_compile_plan_for_source(&path);
    let roots = EffectiveCompilationRoots {
        host: RootEntry { dependency_name: None, source_root: source_root.clone() },
        dependencies: Vec::new(),
    };
    let indexes = vec![SyntaxIndex::from_program(&unit.program, generation)];
    let index = Arc::new(ModuleIndex::build(std::slice::from_ref(&unit), &indexes, &roots, &plan));
    let program = unit.program.clone();
    let assembly = ProgramAssembly::new(
        roots,
        Arc::new(vec![unit]),
        0,
        AssemblyDiscovery::ImportClosure,
        index,
        false,
        generation,
    )
    .with_glue_libraries(glue_libraries.iter().map(|library| (*library).to_string()).collect::<Vec<_>>().into());
    let mut db = BeskidDatabase::default();
    let findings = semantic_diagnostics_for_roots(&mut db, None, &assembly, &program)
        .unwrap_or_else(|error| panic!("query gate must judge the fixture: {error}\n{source}"));
    let _ = std::fs::remove_dir_all(&root);
    findings
}

fn codes(findings: &[SemanticFactFinding]) -> Vec<&'static str> {
    findings.iter().map(|finding| finding.kind.code()).collect()
}

const PAIR: &str = "pub type Pair { i64 a } ";
const SHAPE: &str = "enum Shape { Dot, Circle(i64 radius) } ";

/// One fixture per legacy diagnostic class the query gate covers: (legacy class, code, source).
const COVERED: &[(&str, &str, &str)] = &[
    ("ResolveError::UnknownValue (call target)", "E1101", "unit Main() { Missing(); return; }"),
    (
        "ResolveError::UnknownModulePath / UnknownImportPath",
        "E1105",
        "use Missing.Module; unit Main() { return; }",
    ),
    ("TypeError::UnknownType", "E1201", "unit Main() { Missing m = 1; return; }"),
    (
        "TypeError::CallArityMismatch",
        "E1204",
        "unit Helper(i32 a, i32 b) { return; } unit Main() { Helper(1); return; }",
    ),
    (
        "TypeError::TypeMismatch (typed let)",
        "E1206",
        "unit Main() { bool flag = 1_i64; return; }",
    ),
    ("TypeError::TypeMismatch (return value)", "E1206", "i64 Main() { return true; }"),
    (
        "TypeError::TypeMismatch (local assignment)",
        "E1206",
        "unit Main() { mut i64 n = 0_i64; n = true; return; }",
    ),
    (
        "TypeError::TypeMismatch (call argument)",
        "E1206",
        "unit Take(bool flag) { return; } unit Main() { Take(1_i64); return; }",
    ),
    (
        "TypeError::TypeMismatch (match arm)",
        "E1206",
        "enum Shape { Dot, Circle(i64 radius) } \
         i64 Main(Shape s) { return match s { Shape::Dot => 0_i64, Shape::Circle(r) => true, }; }",
    ),
    ("TypeError::ReturnTypeMismatch (bare return)", "E1207", "i64 Main() { return; }"),
    ("TypeError::NonBoolCondition (if)", "E1208", "unit Main() { if 1_i64 { return; } return; }"),
    ("TypeError::NonBoolCondition (while)", "E1208", "unit Main() { while \"x\" { return; } return; }"),
    (
        "TypeError::UnknownStructField",
        "E1211",
        "pub type Pair { i64 a } i64 Main() { Pair p = Pair { b: 1 }; return 0; }",
    ),
    ("ResolveError / immutable assignment", "E1214", "unit Main() { i64 n = 0; n = 1; return; }"),
    (
        "TypeError::GenericParameterConflict",
        "E1229",
        "i64 AsI64() { return 1_i64; } word AsWord() { return 1_word; } unit Equal<T>(T actual, T expected) { return; } \
         unit Main() { Equal(AsI64(), AsWord()); return; }",
    ),
    (
        "TypeError::UnknownEnumVariant",
        "E1301",
        "enum Shape { Dot, Circle(i64 radius) } i64 Main() { Shape s = Shape::Square; return 0; }",
    ),
    (
        "TypeError::EnumConstructorMismatch",
        "E1302",
        "enum Shape { Dot, Circle(i64 radius) } i64 Main() { Shape s = Shape::Circle(); return 0; }",
    ),
    (
        "match exhaustiveness",
        "E1304",
        "enum Shape { Dot, Circle(i64 radius) } i64 Main(Shape s) { return match s { Shape::Dot => 0, }; }",
    ),
    (
        "pattern arity",
        "E1307",
        "enum Shape { Dot, Circle(i64 radius) } \
         i64 Main(Shape s) { return match s { Shape::Dot => 0, Shape::Circle(r, extra) => r, }; }",
    ),
];

#[test]
fn every_covered_legacy_class_is_reported_by_the_query_gate_alone() {
    for (offset, (class, code, source)) in COVERED.iter().enumerate() {
        let findings = root_findings(source, 500 + offset as u64);
        assert!(
            codes(&findings).contains(code),
            "{class}: query gate must report {code}, got {:?}\n{source}",
            codes(&findings)
        );
    }
}

/// One fixture per legacy diagnostic class whose code is shared with another class, so the
/// query gate must report the exact `SemanticIssueKind` variant: (legacy class, variant, code,
/// source). Phase 2 of `docs/reports/2026-10-05-v06-query-check-authority.md`.
const COVERED_KINDS: &[(&str, &str, &str, &str)] = &[
    ("TypeError::InvalidBinaryOp", "TypeInvalidBinaryOp", "E1209", "unit Main() { bool b = true - false; return; }"),
    ("TypeError::InvalidUnaryOp", "TypeInvalidUnaryOp", "E1210", "unit Main() { bool b = -true; return; }"),
    (
        "TypeError::NumericLiteralOutOfRange",
        "NumericLiteralOutOfRange",
        "T0905",
        "unit Main() { i32 n = 3000000000_i32; return; }",
    ),
    ("TypeError::InvalidMemberTarget (field path)", "TypeInvalidMemberTarget", "E1213", "i64 Main(i64 n) { return n.value; }"),
    (
        "TypeError::InvalidMemberTarget (method path)",
        "TypeInvalidMemberTarget",
        "E1213",
        "i64 Main(i64 n) { return n.Double(); }",
    ),
    (
        "TypeError::UnknownCallTarget (non-callable field)",
        "TypeUnknownCallTarget",
        "E1606",
        "pub type Box { i64 value } i64 Main(Box b) { return b.value(); }",
    ),
    (
        "TypeError::UnknownStructField (unknown method)",
        "TypeUnknownStructField",
        "E1211",
        "pub type Box { i64 value } i64 Main(Box b) { return b.Missing(); }",
    ),
    (
        "TypeError::UnknownStructType",
        "TypeUnknownStructType",
        "E1201",
        "enum Shape { Dot } unit Main() { Shape s = Shape { }; return; }",
    ),
    (
        "TypeError::UnknownEnumType",
        "TypeUnknownEnumType",
        "E1201",
        "pub type Pair { i64 a } unit Main() { Pair p = Pair::A; return; }",
    ),
    (
        "TypeError::UnknownValueType (method as value)",
        "TypeUnknownValueType",
        "E1201",
        "pub type Box { i64 value, pub i64 Get() { return 1_i64; } } i64 Main(Box b) { i64 f = (b).Get; return f; }",
    ),
    ("TypeError::UnsupportedExpression (index)", "TypeUnsupportedExpression", "E1202", "i64 Main(i64 n) { return n[0]; }"),
    (
        "TypeError::UnsupportedExpression (compound assignment)",
        "TypeUnsupportedExpression",
        "E1202",
        "unit Main() { mut bool b = true; b += false; return; }",
    ),
    (
        "TypeError::MissingTypeAnnotation",
        "TypeMissingTypeAnnotation",
        "E1202",
        "unit Main() { let f = (x) => x; return; }",
    ),
    (
        "TypeError::InvalidPrimitiveConversionArgument",
        "TypeInvalidPrimitiveConversionArgument",
        "E1228",
        "unit Main() { i64 n = i64(true); return; }",
    ),
    (
        "TypeError::GenericArgumentMismatch",
        "TypeGenericArgumentMismatch",
        "E1204",
        "unit Take<T>(T value) { return; } unit Main() { Take<i64, bool>(1); return; }",
    ),
    (
        "TypeError::GenericBoundNotSatisfied",
        "GenericBoundNotSatisfied",
        "E1610",
        "pub contract Named { string Name(); } pub type Plain { i64 v } \
         string Describe<T>(T value) where T: Named { return value.Name(); } \
         unit Main() { Describe(Plain { v: 1 }); return; }",
    ),
    ("ResolveError::DuplicateLocal", "ResolveDuplicateLocal", "E1102", "unit Main() { i64 n = 1; i64 n = 2; return; }"),
    (
        "ResolveError::DuplicateItem (module scope)",
        "ResolveDuplicateItem",
        "E1102",
        "unit Helper() { return; } unit Helper() { return; } unit Main() { return; }",
    ),
    ("ResolveError::DuplicateItem (constant)", "ResolveDuplicateItem", "E1102", "const N = 1; const N = 2;"),
    (
        "ResolveError::DuplicateItem (method named like a field)",
        "ResolveDuplicateItem",
        "E1102",
        "pub type Box { i64 value, pub i64 value() { return 1_i64; } }",
    ),
    (
        "TypeError::NonIterableForTarget (primitive)",
        "TypeNonIterableForTarget",
        "E1215",
        "unit Main(i64 n) { for i in n { return; } return; }",
    ),
    (
        "TypeError::NonIterableForTarget (nominal without Next)",
        "TypeNonIterableForTarget",
        "E1215",
        "pub type Bag { i64 n } unit Main(Bag bag) { for i in bag { return; } return; }",
    ),
    (
        "TypeError::IterableNextArityMismatch",
        "TypeIterableNextArityMismatch",
        "E1216",
        "pub enum Option<T> { Some(T value), None } \
         pub type Iter { i64 n, pub Option<i64> Next(i64 step) { return Option::None; } } \
         unit Main(Iter it) { for x in it { return; } return; }",
    ),
    (
        "TypeError::IterableNextReturnNotOption",
        "TypeIterableNextReturnNotOption",
        "E1217",
        "pub type Iter { i64 n, pub i64 Next() { return 1_i64; } } \
         unit Main(Iter it) { for x in it { return; } return; }",
    ),
    (
        "TypeError::IterableOptionSomeArityMismatch",
        "TypeIterableOptionSomeArityMismatch",
        "E1218",
        "pub enum Option<T> { Some(T value), None } \
         pub type Iter { i64 n, pub Option<i64, bool> Next() { return Option::None; } } \
         unit Main(Iter it) { for x in it { return; } return; }",
    ),
    (
        "TypeError::InvalidEventInvocationScope",
        "TypeInvalidEventInvocationScope",
        "E1219",
        "pub type Hub { event OnPing(i64 value) } unit Main(Hub hub) { hub.OnPing(1_i64); return; }",
    ),
    (
        "TypeError::InvalidEventCapacity",
        "TypeInvalidEventCapacity",
        "E1220",
        "pub type Hub { event{0} OnPing(i64 value) } unit Main() { return; }",
    ),
    (
        "TypeError::InvalidEventSubscriptionTarget",
        "TypeInvalidEventSubscriptionTarget",
        "E1221",
        "pub type Box { i64 value } unit Main(Box b) { b.value += ((i64 x) => x); return; }",
    ),
    (
        "TypeError::SpawnTargetNotFiberCompatible",
        "SpawnTargetNotFiberCompatible",
        "E1223",
        "i64 Work(i64 n) { return n; } unit Main() { let f = spawn Work; return; }",
    ),
    (
        "TypeError::ContractMethodMissingImplementation",
        "ContractMethodMissingImplementation",
        "E1601",
        "pub contract Named { string Name(); } pub type Plain : Named { i64 v }",
    ),
    (
        "TypeError::ContractImplementationSignatureMismatch",
        "ContractImplementationSignatureMismatch",
        "E1602",
        "pub contract Named { string Name(); } pub type Plain : Named { i64 v, pub i64 Name() { return 1_i64; } }",
    ),
    (
        "TypeError::ContractAssociatedTypeMissingBinding",
        "ContractAssociatedTypeMissingBinding",
        "E1607",
        "pub contract Seq { type Item; } pub type Numbers : Seq { i64 v }",
    ),
    (
        "ResolveError::InvalidConformanceTarget",
        "ResolveInvalidConformanceTarget",
        "E1607",
        "pub type Base { i64 v } pub type Derived : Base { i64 w }",
    ),
    (
        "TypeError::ThisUsedOutsideContractOrImpl",
        "ThisUsedOutsideContractOrImpl",
        "E1608",
        "unit Take(This value) { return; }",
    ),
    (
        "TypeError::UnresolvedAssociatedType",
        "UnresolvedAssociatedType",
        "E1609",
        "unit Take<T>(T::Item value) { return; }",
    ),
    (
        "TypeError::ExternInvalidAbi",
        "ExternInvalidAbi",
        "T0901",
        "[Extern(Abi:\"Rust\", Library:\"m\")] contract Native { i64 Abs(i64 v); }",
    ),
    ("TypeError::ExternMissingLibrary", "ExternMissingLibrary", "T0902", "[Extern(Abi:\"C\")] contract Native { i64 Abs(i64 v); }"),
    (
        "TypeError::ExternDisallowedParamType",
        "ExternDisallowedParamType",
        "T0903",
        "[Extern(Abi:\"C\", Library:\"m\")] contract Native { i64 Length(string text); }",
    ),
    (
        "TypeError::ExternDisallowedReturnType",
        "ExternDisallowedReturnType",
        "T0904",
        "[Extern(Abi:\"C\", Library:\"m\")] contract Native { string Name(); }",
    ),
    (
        "TypeError::TypeMismatch (nominal let)",
        "TypeMismatch",
        "E1206",
        "pub type Pair { i64 a } pub type Other { i64 b } unit Main(Other o) { Pair p = o; return; }",
    ),
    (
        "TypeError::TypeMismatch (nominal return)",
        "TypeMismatch",
        "E1206",
        "pub type Pair { i64 a } string Main(Pair p) { return p; }",
    ),
    (
        "TypeError::TypeMismatch (nominal call argument)",
        "TypeMismatch",
        "E1206",
        "pub type Pair { i64 a } unit Take(Pair p) { return; } unit Main() { Take(1_i64); return; }",
    ),
    (
        "TypeError::TypeMismatch (struct literal field)",
        "TypeMismatch",
        "E1206",
        "pub type Pair { i64 a } unit Main() { Pair p = Pair { a: true }; return; }",
    ),
    (
        "TypeError::TypeMismatch (enum constructor argument)",
        "TypeMismatch",
        "E1206",
        "enum Shape { Dot, Circle(i64 radius) } unit Main() { Shape s = Shape::Circle(true); return; }",
    ),
    (
        "TypeError::TypeMismatch (method call argument)",
        "TypeMismatch",
        "E1206",
        "pub type Counter { i64 n, pub unit Add(i64 step) { return; } } unit Main(Counter c) { c.Add(true); return; }",
    ),
    (
        "TypeError::TypeMismatch (literal pattern against scrutinee)",
        "TypeMismatch",
        "E1206",
        "i64 Main(bool flag) { return match flag { 1 => 0_i64, _ => 1_i64, }; }",
    ),
    (
        "TypeError::TypeMismatch (enum pattern against scrutinee)",
        "TypeMismatch",
        "E1206",
        "enum A { X } enum B { Y } i64 Main(A a) { return match a { B::Y => 0_i64, _ => 1_i64, }; }",
    ),
    (
        "TypeError::TypeMismatch (match arm against contextual destination)",
        "TypeMismatch",
        "E1206",
        "pub type Pair { i64 a } enum Shape { Dot, Circle(i64 radius) } \
         i64 Pick(Shape s, Pair p) { i64 v = match s { Shape::Dot => 0_i64, Shape::Circle(r) => p, }; return v; }",
    ),
];

/// Every phase-2 class, judged by the query gate alone, carries its exact variant and code. All
/// classes are evaluated before the assertion so one run names every failing class.
#[test]
fn every_covered_legacy_kind_is_reported_by_the_query_gate_alone() {
    let mut failures = Vec::new();
    for (offset, (class, variant, code, source)) in COVERED_KINDS.iter().enumerate() {
        let findings = root_findings(source, 700 + offset as u64);
        let reported = findings
            .iter()
            .any(|finding| finding.kind.code() == *code && format!("{:?}", finding.kind).starts_with(variant));
        if !reported {
            failures.push(format!("{class}: expected {variant} ({code}), got {:?}\n  {source}", codes(&findings)));
        }
    }
    assert!(failures.is_empty(), "query gate misses {} class(es):\n{}", failures.len(), failures.join("\n"));
}

/// A manifest Glue library holds its `Extern` methods to the Glue representation authority: a raw
/// `pointer` parameter is rejected (T0903) where the C user profile would accept it.
#[test]
fn glue_library_extern_methods_are_held_to_the_glue_binding_authority() {
    let source = "[Extern(Abi:\"C\", Library:\"native_glue\")] contract Native { i64 Peek(pointer at); }";
    let findings = root_findings_with_glue(source, 800, &["native_glue"]);
    assert!(
        findings.iter().any(|finding| matches!(
            &finding.kind,
            beskid_analysis::analysis::SemanticIssueKind::GlueBindingRejected { method, .. } if method == "Peek"
        )),
        "{findings:?}"
    );
    let c_profile = root_findings(source, 801);
    assert!(c_profile.is_empty(), "a raw pointer is a permitted C profile scalar: {c_profile:?}");
}

/// Well-typed nominal, contract, enum, method, and pattern positions produce no phase-2 finding.
#[test]
fn well_typed_nominal_positions_produce_no_finding() {
    let source = "pub contract Named { string Name(); } \
                  pub type Pair : Named { i64 a, pub string Name() { return \"pair\"; } \
                  pub i64 Sum(i64 extra) { return this.a + extra; } } \
                  enum Shape { Dot, Circle(i64 radius), Boxed(Pair pair) } \
                  Pair Make(i64 a) { return Pair { a: a }; } \
                  string Describe(Named named) { return named.Name(); } \
                  i64 Measure(Shape s) { return match s { Shape::Dot => 0_i64, Shape::Circle(r) => r, Shape::Boxed(p) => p.a, }; } \
                  i64 Main() { Pair p = Make(1_i64); Pair q = p; Shape s = Shape::Boxed(q); string n = Describe(p); \
                  i64 total = p.Sum(2_i64); i64 code = match total { 0 => 1_i64, _ => 2_i64, }; return Measure(s) + code; }";
    let findings = root_findings(source, 802);
    assert!(findings.is_empty(), "{findings:?}");
}

#[test]
fn clean_root_items_produce_no_finding() {
    let source = format!(
        "{PAIR}{SHAPE}\
         i64 Area(Shape s) {{ return match s {{ Shape::Dot => 0_i64, Shape::Circle(r) => r, }}; }} \
         Pair Make(i64 a) {{ return Pair {{ a: a }}; }} \
         unit Loop(bool go) {{ mut i64 n = 0; while go {{ n = n + 1; if n > 3 {{ return; }} }} return; }} \
         test Smoke {{ i64 v = Area(Shape::Dot); }}"
    );
    let findings = root_findings(&source, 600);
    assert!(findings.is_empty(), "{findings:?}");
}

/// A method call on a generic receiver bounded by a contract (`r.Resolve(x)` with
/// `where R: Resolver`, the `Core.Bsol.Imports.ResolveVerified` shape).
#[test]
fn generic_bound_member_call_is_not_a_finding() {
    let source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } \
                  pub contract Resolver { Result<i64, string> Resolve(i64 x); } \
                  pub Result<i64, string> Run<R>(R r, i64 x) where R: Resolver { return r.Resolve(x); }";
    let findings = root_findings(source, 601);
    assert!(findings.is_empty(), "{findings:?}");
}

/// A value `match` whose arm ends in `return` yields no value; the arm is `never`-typed, not a
/// type mismatch against the other arms.
#[test]
fn return_ending_arm_in_a_value_match_is_not_a_finding() {
    let source = format!(
        "{SHAPE}i64 Pick(Shape s) {{ \
         i64 v = match s {{ Shape::Dot => 1_i64, Shape::Circle(r) => {{ return r; }}, }}; return v; }}"
    );
    let findings = root_findings(&source, 602);
    assert!(findings.is_empty(), "{findings:?}");
}

/// A generic enum constructor assigned under a declared destination (`Result::Error(e)` into a
/// `Result<i64, string>` local) takes its arguments from context.
#[test]
fn contextual_generic_enum_constructor_assignment_is_not_a_finding() {
    let source = "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } \
                  pub Result<i64, string> Fail(string e) { Result<i64, string> r = Result::Error(e); return r; }";
    let findings = root_findings(source, 603);
    assert!(findings.is_empty(), "{findings:?}");
}

/// A hub unit that only re-exports child modules (`pub mod Core.Glue.StdioBridge;` in
/// `Core.Glue.Glue`) declares no import the query gate must resolve.
#[test]
fn hub_module_declarations_are_not_a_finding() {
    let source = "pub mod Hub.Child; pub mod Hub.Other; pub i64 Version() { return 6_i64; }";
    let findings = root_findings(source, 604);
    assert!(findings.is_empty(), "{findings:?}");
}

/// Inline type methods witness a local contract by name, `pub` or not (the `sample_mod` fixture
/// shape), and a contract and nominal types of the same names declared in another module scope
/// (an SDK-shaped `Collector`) replace neither the local contract nor the local signature types.
#[test]
fn inline_method_conformance_to_a_same_named_local_contract_is_not_a_finding() {
    let source = "mod Sdk { pub type CollectRequest { i64 marker } pub type CollectTargetSet { i64 marker } \
                  pub contract Collector { CollectTargetSet Collect(CollectRequest request); } } \
                  pub type CollectRequest {} pub type CollectTargetSet {} \
                  pub contract Collector { CollectTargetSet Collect(CollectRequest request); } \
                  pub type PrivateCollect : Collector { \
                  CollectTargetSet Collect(CollectRequest request) { return CollectTargetSet {}; } } \
                  pub type PublicCollect : Collector { \
                  pub CollectTargetSet Collect(CollectRequest request) { return CollectTargetSet {}; } }";
    let findings = root_findings(source, 606);
    assert!(findings.is_empty(), "{findings:?}");
}

/// The same-named local contract stays authoritative for E1601: an implementor that declares no
/// method of the contract's name is still reported, once, against the local `Collector`.
#[test]
fn missing_method_against_a_same_named_local_contract_is_reported_once() {
    let source = "mod Sdk { pub type CollectRequest {} pub type CollectTargetSet {} \
                  pub contract Collector { CollectTargetSet Collect(CollectRequest request); } } \
                  pub type CollectRequest {} pub type CollectTargetSet {} \
                  pub contract Collector { CollectTargetSet Gather(CollectRequest request); } \
                  pub type SampleCollect : Collector { \
                  CollectTargetSet Collect(CollectRequest request) { return CollectTargetSet {}; } }";
    let findings = root_findings(source, 607);
    let missing = findings
        .iter()
        .filter(|finding| matches!(
            &finding.kind,
            beskid_analysis::analysis::SemanticIssueKind::ContractMethodMissingImplementation { contract_name, method_name, .. }
                if contract_name == "Collector" && method_name == "Gather"
        ))
        .count();
    assert_eq!(missing, 1, "{findings:?}");
}

/// A local function whose name matches an enum variant resolves as the function at a call.
#[test]
fn local_function_named_like_an_enum_variant_is_not_a_finding() {
    let source = "enum Mode { Fast, Slow } i64 Fast() { return 1_i64; } i64 Main() { return Fast(); }";
    let findings = root_findings(source, 605);
    assert!(findings.is_empty(), "{findings:?}");
}
