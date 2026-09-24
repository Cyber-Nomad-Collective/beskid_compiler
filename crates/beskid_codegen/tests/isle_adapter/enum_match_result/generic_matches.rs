//! Enum matches over source arms and generic call scrutinees.

use super::super::support::{
    Arc, AssemblyDiscovery, BeskidDatabase, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry,
    SourceUnit, SyntaxGenerationId, SyntaxModuleItem, TargetMetadata, emit_isle_expression, emit_isle_item, enum_match,
    find_function_definitions, find_node, isa, item_fixture, item_fixture_with_root, item_name, lower_syntax_program,
    node_type, parse_program_with_source_name, settings, types,
};
use super::assert_imported_result_lowering;

#[test]
fn parsed_enum_match_uses_source_arms_without_hir() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Choice { None(), Some() } i32 Main() { return match Choice::Some() { Choice::None() => 1, Choice::Some() => 2, }; }",
    );
    let expression =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::MatchExpression).expect("enum match");
    assert!(enum_match(input.database(), expression).expect("enum match query").is_some(), "source match facts");
    assert_eq!(node_type(input.database(), expression).expect("match type"), Some(beskid_queries::SemanticTypeId::I32));
    let function = emit_isle_expression(&input, isa.as_ref(), expression, types::I32)
        .expect("enum match lowers through syntax facts");

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"));
    assert_eq!(clif.matches("brif").count(), 2, "each source arm must retain its ordered tag test: {clif}");
}

#[test]
fn parsed_generic_enum_match_uses_explicit_scrutinee_layout_without_hir() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } i64 Main() { Result<i64, string> value = Result<i64, string>::Ok(7_i64); return match value { Result::Ok(_) => 1_i64, Result::Error(_) => 0_i64, }; }",
    );
    let expression = find_node(input.database(), item, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("generic enum match");
    assert!(
        enum_match(input.database(), expression).expect("generic enum match query").is_some(),
        "generic match semantic facts"
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("generic enum match lowers through its explicit source layout");

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "{clif}");
    assert!(clif.contains("iconst.i64 1"), "{clif}");
}

#[test]
fn direct_generic_call_match_preserves_nominal_payload_identity() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Error { Value(i64 value) } enum Outcome<T> { Ok(T value), None } Outcome<T> Wrap<T>(T value) { return Outcome::Ok(value); } i64 Main() { Error error = Error::Value(7_i64); return match Wrap(error) { Outcome::Ok(value) => 21_i64, _ => -1_i64, }; }",
    );
    let expression = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("direct generic call match");
    let error =
        find_node(input.database(), root, beskid_queries::IndexedNodeKind::EnumDefinition).expect("Error declaration");
    let fact = enum_match(input.database(), expression).expect("match query").expect("nominal match fact");
    assert_eq!(fact.layout.variants[0].fields[0].1, beskid_queries::AggregateFieldShape::Nominal(error));
    let beskid_queries::EnumMatchPatternFact::Enum(pattern) = &fact.arms[0].pattern else {
        panic!("Ok variant pattern");
    };
    let beskid_queries::EnumMatchPatternFact::Binding(binding) = &pattern.items[0] else {
        panic!("nominal payload binding");
    };
    assert_eq!(binding.payload, beskid_queries::AggregateFieldShape::Nominal(error));
    assert_eq!(binding.managed_reference, beskid_queries::ManagedReferenceKind::GcManaged);
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem { key, symbol: item_name(input.database(), key).unwrap().unwrap().to_string() })
        .collect::<Vec<_>>();
    lower_syntax_program(&input, isa.as_ref(), &items).expect("direct nominal call-result match lowers");
}

#[test]
fn direct_generic_call_match_supports_nested_nominal_payload_pattern() {
    for (wrap, scrutinee) in [
        ("Outcome<T> Wrap<T>(T value) { return Outcome::Ok(value); }", "Wrap(error)"),
        ("Outcome<Error> Wrap(Error value) { return Outcome::Ok(value); }", "Wrap(error)"),
        ("Outcome<T> Wrap<T>(T value) { return Outcome::Ok(value); }", "joined"),
    ] {
        let local = if scrutinee == "joined" { "Outcome<Error> joined = Wrap(error);" } else { "" };
        let (input, isa, root) = item_fixture_with_root(&format!(
            "enum Error {{ Value(i64 value) }} enum Outcome<T> {{ Ok(T value), None }} {wrap} i64 Main() {{ Error error = Error::Value(7_i64); {local} return match {scrutinee} {{ Outcome::Ok(Error::Value(value)) => value, _ => -1_i64, }}; }}"
        ));
        let expression = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MatchExpression).unwrap();
        let fact = enum_match(input.database(), expression).unwrap().expect("nested nominal match");
        let beskid_queries::EnumMatchPatternFact::Enum(outer) = &fact.arms[0].pattern else {
            panic!("outer enum pattern");
        };
        let beskid_queries::EnumMatchPatternFact::Enum(inner) = &outer.items[0] else {
            panic!("nested nominal enum pattern");
        };
        assert!(matches!(&inner.items[0], beskid_queries::EnumMatchPatternFact::Binding(_)));
        let items = find_function_definitions(input.database(), root)
            .into_iter()
            .map(|key| SyntaxModuleItem { key, symbol: item_name(input.database(), key).unwrap().unwrap().to_string() })
            .collect::<Vec<_>>();
        lower_syntax_program(&input, isa.as_ref(), &items).expect("nested match and controls lower");
    }
}

#[test]
fn direct_generic_call_match_distinguishes_scalar_array_and_native_pointer_ownership() {
    for (ty, expected_shape, ownership) in [
        ("i64", beskid_queries::SemanticTypeId::I64, beskid_queries::ManagedReferenceKind::NativeOrScalar),
        ("u8[]", beskid_queries::SemanticTypeId::POINTER, beskid_queries::ManagedReferenceKind::GcManaged),
        ("pointer", beskid_queries::SemanticTypeId::POINTER, beskid_queries::ManagedReferenceKind::NativeOrScalar),
    ] {
        let (input, isa, root) = item_fixture_with_root(&format!(
            "enum Outcome<T> {{ Ok(T value), None }} Outcome<T> Wrap<T>(T value) {{ return Outcome::Ok(value); }} i64 Main({ty} value) {{ return match Wrap(value) {{ Outcome::Ok(payload) => 21_i64, _ => -1_i64, }}; }}"
        ));
        let expression = find_node(input.database(), root, beskid_queries::IndexedNodeKind::MatchExpression).unwrap();
        let fact = enum_match(input.database(), expression).unwrap().expect("direct match fact");
        let beskid_queries::EnumMatchPatternFact::Enum(pattern) = &fact.arms[0].pattern else {
            panic!("Ok pattern");
        };
        let beskid_queries::EnumMatchPatternFact::Binding(binding) = &pattern.items[0] else {
            panic!("payload binding");
        };
        assert_eq!(binding.payload, beskid_queries::AggregateFieldShape::Scalar(expected_shape), "{ty}");
        assert_eq!(binding.managed_reference, ownership, "{ty}");
        let items = find_function_definitions(input.database(), root)
            .into_iter()
            .map(|key| SyntaxModuleItem { key, symbol: item_name(input.database(), key).unwrap().unwrap().to_string() })
            .collect::<Vec<_>>();
        lower_syntax_program(&input, isa.as_ref(), &items).expect("source-proven scalar and pointer shapes lower");
    }
}

#[test]
fn cross_unit_generic_receiver_direct_match_preserves_nominal_value() {
    let compiler = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
    let concurrency = compiler.join("corelib/packages/concurrency/src");
    let foundation = compiler.join("corelib/packages/foundation/src");
    for (payload, make, local, scrutinee, pattern) in [
        ("FiberError", "FiberError::Cancelled(73_i64, 91_i64)", "", "child.Join()", "Result::Ok(error) => 21_i64"),
        (
            "FiberError",
            "FiberError::Cancelled(73_i64, 91_i64)",
            "",
            "child.Join()",
            "Result::Ok(FiberError::Cancelled(reason, canceler)) => reason + canceler",
        ),
        (
            "FiberError",
            "FiberError::Cancelled(73_i64, 91_i64)",
            "Result<FiberError, FiberError> joined = child.Join();",
            "joined",
            "Result::Ok(error) => 21_i64",
        ),
        ("FiberError", "FiberError::Cancelled(73_i64, 91_i64)", "", "child.Join()", "Result::Error(error) => 21_i64"),
        ("i64", "7_i64", "", "child.Join()", "Result::Ok(value) => value"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let source_path = directory.path().join("Main.bd");
        let source = format!(
            "use Concurrency.Fiber; use Concurrency.FiberError; use Core.Results; {payload} Make() {{ return {make}; }} i64 Main() {{ Fiber<{payload}> child = spawn Make(); {local} return match {scrutinee} {{ {pattern}, _ => -1_i64, }}; }}"
        );
        std::fs::write(&source_path, &source).unwrap();
        let mut units = vec![SourceUnit {
            program: parse_program_with_source_name(source_path.to_str().unwrap(), &source).unwrap(),
            origin_path: source_path.clone(),
            path: source_path,
            logical_name: "Main.bd".into(),
            source,
        }];
        for (base, relative) in [
            (&concurrency, "Concurrency/Fiber.bd"),
            (&concurrency, "Concurrency/FiberError.bd"),
            (&foundation, "Core/Disposable.bd"),
            (&foundation, "Core/Results/Results.bd"),
        ] {
            let path = base.join(relative);
            let source = std::fs::read_to_string(&path).unwrap();
            units.push(SourceUnit {
                program: parse_program_with_source_name(path.to_str().unwrap(), &source).unwrap(),
                origin_path: path.clone(),
                path,
                logical_name: relative.into(),
                source,
            });
        }
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory.path().to_path_buf() },
                dependencies: vec![
                    RootEntry { dependency_name: Some("concurrency".into()), source_root: concurrency.clone() },
                    RootEntry { dependency_name: Some("foundation".into()), source_root: foundation.clone() },
                ],
            },
            Arc::new(units),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            SyntaxGenerationId(151),
        ));
        let target = TargetMetadata::supported()
            .into_iter()
            .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
            .unwrap();
        let isa = isa::lookup_by_name("x86_64").unwrap().finish(settings::Flags::new(settings::builder())).unwrap();
        beskid_codegen::lower_syntax_assembly_entrypoint(
            &mut BeskidDatabase::default(),
            assembly,
            "Main",
            target,
            isa.as_ref(),
        )
        .expect("real Fiber.Join direct match and controls lower");
    }
}

#[test]
fn generic_result_predicate_match_uses_each_call_specialization_for_its_scrutinee() {
    let (input, isa, root) = item_fixture_with_root(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } bool IsOk<TValue, TError>(Result<TValue, TError> value) { return match value { Result::Ok(_) => true, Result::Error(_) => false, }; } bool Main(Result<i64, string> left, Result<string, i64> right) { return IsOk<i64, string>(left) && IsOk<string, i64>(right); }",
    );
    let items = find_function_definitions(input.database(), root);
    let is_ok = items
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("IsOk"))
        .expect("IsOk definition");
    let main = items
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("Main"))
        .expect("Main definition");

    lower_syntax_program(
        &input,
        isa.as_ref(),
        &[
            SyntaxModuleItem { key: is_ok, symbol: "IsOk".into() },
            SyntaxModuleItem { key: main, symbol: "Main".into() },
        ],
    )
    .expect("a generic predicate match must consume the same call specialization as its parameter local");
}

#[test]
fn imported_generic_result_match_specialization_preserves_payload_provenance() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; T RequireHttp<T>(Result<T, HttpError> value, T fallback) { return match value { Result::Ok(result) => result, Result::Error(_) => fallback, }; } bool IsError<T>(Result<T, HttpError> result, HttpError expected) { return match result { Result::Ok(_) => false, Result::Error(value) => match value { HttpError::InvalidFraming => match expected { HttpError::InvalidFraming => true, HttpError::Closed => false, }, HttpError::Closed => match expected { HttpError::InvalidFraming => false, HttpError::Closed => true, }, }, }; } bool Main(Result<Request, HttpError> value, Request fallback, HttpError expected) { Request request = RequireHttp<Request>(value, fallback); if request.id == 0_i64 { return false; } return IsError<Request>(value, expected); }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        ("Http/Requests.bd", "pub type Request { i64 id, }"),
    ]);
}

#[test]
fn imported_result_binding_array_field_flows_into_a_call_argument() {
    assert_imported_result_lowering(&[
        (
            "Main.bd",
            "use Core.Results; use Http.Errors; use Http.Requests; Result<i64, HttpError> Framing(Header[] headers) { return Result::Ok(0_i64); } bool Main(Result<Request, HttpError> head) { return match head { Result::Error(_) => false, Result::Ok(value) => { Result<i64, HttpError> framing = Framing(value.headers); return match framing { Result::Ok(_) => true, Result::Error(_) => false, }; }, }; }",
        ),
        ("Core/Results.bd", "pub enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }"),
        ("Http/Errors.bd", "pub enum HttpError { InvalidFraming(), Closed() }"),
        ("Http/Requests.bd", "pub type Header { string name, } pub type Request { string method, Header[] headers, }"),
    ]);
}

#[test]
fn concrete_array_result_match_survives_a_sibling_generic_result_specialization() {
    let (input, isa, root) = item_fixture_with_root(
        "enum EncodingError { Invalid() } enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } bool IsOk<TValue, TError>(Result<TValue, TError> value) { return match value { Result::Ok(_) => true, Result::Error(_) => false, }; } u8[] StringToBytes(Result<u8[], EncodingError> encoded, u8[] empty) { return match encoded { Result::Ok(bytes) => bytes, Result::Error(_) => empty, }; } unit Main(Result<u8[], EncodingError> encoded, u8[] empty) { IsOk<u8[], EncodingError>(encoded); u8[] bytes = StringToBytes(encoded, empty); return; }",
    );
    let definitions = find_function_definitions(input.database(), root);
    let string_to_bytes = definitions
        .iter()
        .copied()
        .find(|item| item_name(input.database(), *item).ok().flatten().as_deref() == Some("StringToBytes"))
        .expect("StringToBytes definition");
    let expression = find_node(input.database(), string_to_bytes, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("StringToBytes match");
    assert!(
        enum_match(input.database(), expression).expect("concrete Result match query").is_some(),
        "the concrete applied Result type must retain its array payload ownership"
    );
    let items = definitions
        .into_iter()
        .map(|item| SyntaxModuleItem {
            symbol: item_name(input.database(), item).expect("item name query").expect("named function").to_string(),
            key: item,
        })
        .collect::<Vec<_>>();

    lower_syntax_program(&input, isa.as_ref(), &items)
        .expect("a generic sibling specialization must not shadow a concrete array Result match");
}

#[test]
fn parsed_generic_unit_payload_pattern_lowers_without_fabricating_storage() {
    let (input, isa, item) = item_fixture(
        "enum Result<TValue, TError> { Ok(TValue value), Error(TError error) } bool Main() { Result<unit, string> value = Result<unit, string>::Ok(()); return match value { Result::Ok(()) => true, Result::Error(_) => false, }; }",
    );
    let expression = find_node(input.database(), item, beskid_queries::IndexedNodeKind::MatchExpression)
        .expect("generic unit-payload enum match");
    assert!(
        enum_match(input.database(), expression).expect("generic unit-payload match query").is_some(),
        "the canonical unit pattern must be represented as a payload-free variant test"
    );

    let function = emit_isle_item(&input, isa.as_ref(), item)
        .expect("a unit payload pattern lowers through the variant tag without a payload load");

    let clif = function.display().to_string();
    assert!(clif.contains("load.i32"), "the match must load the variant tag: {clif}");
    assert!(!clif.contains("load.i8"), "the zero-sized unit payload must not be loaded: {clif}");
}

#[test]
fn generic_unit_arguments_preserve_effects_without_fabricating_abi_storage() {
    let (input, isa, root) = item_fixture_with_root(
        "unit Effect() { return; } i64 Consume<T>(T value, i64 count) { return count; } i64 Main() { return Consume<unit>(Effect(), 42_i64); }",
    );
    let items = find_function_definitions(input.database(), root)
        .into_iter()
        .map(|key| SyntaxModuleItem { symbol: item_name(input.database(), key).unwrap().unwrap().to_string(), key })
        .collect::<Vec<_>>();
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items).expect("zero-sized generic parameter erasure");
    let consume = artifact.functions.iter().find(|function| function.name.starts_with("Consume")).unwrap();
    assert_eq!(consume.function.signature.params.len(), 1);
    let main = artifact.functions.iter().find(|function| function.name == "Main").unwrap();
    assert!(main.function.display().to_string().contains("Effect"));
}
