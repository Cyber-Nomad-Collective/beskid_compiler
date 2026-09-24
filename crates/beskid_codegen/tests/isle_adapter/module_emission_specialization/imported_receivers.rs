//! Imported nominal receivers, projected array fields, and string comparison helpers.

use super::super::support::{
    AbiManifestV5, Arc, AssemblyDiscovery, AstNodeId, AstNodeKey, BeskidDatabase, CodegenInput,
    EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, ProjectSession, RootEntry, SourceUnit, SourceUnitId,
    SyntaxGenerationId, SyntaxIndex, SyntaxModuleItem, TargetMetadata, build_typed_program, call_lowering, isa,
    item_name, lower_syntax_program, parse_program_with_source_name, settings,
};

#[derive(Clone, Copy)]
pub(super) enum StringComparison {
    Equal,
    NotEqual,
}

#[test]
fn imported_chained_nominal_receiver_specializes_generic_argument_and_lowers() {
    assert_imported_chained_receiver(
        "pub type Inner { pub i64 value, pub i64 Value() { return value; } } pub type Outer { pub Inner inner }",
        "outer.inner.Value()",
        true,
    );
}

#[test]
fn imported_chained_nominal_receiver_retains_generic_identity_and_nested_projections() {
    assert_imported_chained_receiver(
        "pub type Inner<T> { pub T value, pub T Value() { return value; } } pub type Middle { pub Inner<i64> inner } pub type Outer { pub Middle middle }",
        "outer.middle.inner.Value()",
        true,
    );
}

#[test]
fn imported_chained_nominal_receiver_rejects_unproven_or_inaccessible_members() {
    for (model, expression) in [
        ("pub type Inner { pub i64 Value() { return 1_i64; } } pub type Outer { Inner inner }", "outer.inner.Value()"),
        ("pub type Inner { i64 Value() { return 1_i64; } } pub type Outer { pub Inner inner }", "outer.inner.Value()"),
        (
            "pub type Inner { pub i64 Value() { return 1_i64; } } pub type Outer { pub Inner inner }",
            "outer.missing.Value()",
        ),
        ("pub type Outer { pub pointer inner }", "outer.inner.Value()"),
        (
            "pub type Inner { pub i64 Value() { return 1_i64; } } pub type Outer { pub Inner inner, pub Inner inner }",
            "outer.inner.Value()",
        ),
    ] {
        assert_imported_chained_receiver(model, expression, false);
    }
}

fn assert_imported_chained_receiver(model: &str, expression: &str, supported: bool) {
    let main_source = format!(
        "use Model; use Testing.Assert; bool Main(Model.Outer outer) {{ return Assert.Equal({expression}, 443_i64); }}"
    );
    assert_imported_receiver_program(model, &main_source, supported);
}

#[test]
fn indexed_match_bound_nominal_receiver_preserves_source_identity_and_lowers() {
    assert_imported_receiver_program(
        "pub type Inner<T> { pub T value, pub T Value() { return value; } } pub type Outer<T> { pub Inner<T> inner }",
        "use Model; use Testing.Assert; enum Result<T,E> { Ok(T value), Error(E error) } bool Main(Result<Model.Outer<i64>[], i64> result) { return match result { Result::Ok(addresses) => Assert.Equal(addresses[0].inner.Value(), 443_i64), Result::Error(_) => false, }; }",
        true,
    );
}

#[test]
fn indexed_match_bound_nominal_receiver_rejects_unproven_bindings_indices_and_members() {
    let model =
        "pub type Inner { pub i64 value, pub i64 Value() { return value; } } pub type Outer { pub Inner inner }";
    for (payload, expression) in [
        ("Model.Outer[]", "addresses[true].inner.Value()"),
        ("Model.Outer[]", "addresses[Unknown()].inner.Value()"),
        ("pointer[]", "addresses[0].inner.Value()"),
        ("pointer", "addresses[0].inner.Value()"),
        ("Model.Outer[]", "addresses[0].missing.Value()"),
    ] {
        let source = format!(
            "use Model; use Testing.Assert; enum Result<T,E> {{ Ok(T value), Error(E error) }} bool Main(Result<{payload}, i64> result) {{ return match result {{ Result::Ok(addresses) => Assert.Equal({expression}, 443_i64), Result::Error(_) => false, }}; }}"
        );
        assert_imported_receiver_program(model, &source, false);
    }
    for model in [
        "pub type Inner { pub i64 Value() { return 1_i64; } } pub type Outer { Inner inner }",
        "pub type Inner { i64 Value() { return 1_i64; } } pub type Outer { pub Inner inner }",
        "pub type Inner { pub i64 Value() { return 1_i64; } } pub type Outer { pub Inner inner, pub Inner inner }",
    ] {
        assert_imported_receiver_program(
            model,
            "use Model; use Testing.Assert; enum Result<T,E> { Ok(T value), Error(E error) } bool Main(Result<Model.Outer[], i64> result) { return match result { Result::Ok(addresses) => Assert.Equal(addresses[0].inner.Value(), 443_i64), Result::Error(_) => false, }; }",
            false,
        );
    }
}

#[test]
fn projected_array_field_specializes_generic_argument_and_lowers() {
    for (model, receiver) in [
        ("pub type Outer { pub u8[] payload }", "Model.Outer"),
        ("pub type Outer<T> { pub T[] payload }", "Model.Outer<u8>"),
    ] {
        let main = format!(
            "use Model; use Testing.Assert; bool Main({receiver} received) {{ return Assert.Equal(received.payload[0], 99_u8); }}"
        );
        assert_imported_argument_program(model, &main, true, ImportedArgument::ArrayElement);
    }
}

#[test]
fn projected_array_field_rejects_unproven_indices_and_fields() {
    for (model, expression) in [
        ("pub type Outer { u8[] payload }", "received.payload[0]"),
        ("pub type Outer { pub u8[] payload, pub u8[] payload }", "received.payload[0]"),
        ("pub type Outer { pub pointer payload }", "received.payload[0]"),
        ("pub type Outer { pub u8[] payload }", "received.missing[0]"),
        ("pub type Outer { pub u8[] payload }", "received.payload[true]"),
        ("pub type Outer { pub u8[] payload }", "received.payload[Unknown()]"),
    ] {
        let main = format!(
            "use Model; use Testing.Assert; bool Main(Model.Outer received) {{ return Assert.Equal({expression}, 99_u8); }}"
        );
        assert_imported_argument_program(model, &main, false, ImportedArgument::ArrayElement);
    }
}

#[derive(Clone, Copy)]
enum ImportedArgument {
    NominalReceiver,
    ArrayElement,
}

fn assert_imported_receiver_program(model: &str, main_source: &str, supported: bool) {
    assert_imported_argument_program(model, main_source, supported, ImportedArgument::NominalReceiver);
}

fn assert_imported_argument_program(model: &str, main_source: &str, supported: bool, argument: ImportedArgument) {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().expect("project");
    let root = directory.path().to_path_buf();
    let sources = [
        ("Main.bd", main_source),
        ("Model.bd", model),
        ("Testing/Assert.bd", "pub bool Equal<T>(T actual, T expected) { return actual == expected; }"),
    ];
    let generation = SyntaxGenerationId(151);
    let units = sources
        .iter()
        .map(|(path, source)| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).expect("source directory");
            std::fs::write(&path, source).expect("source file");
            SourceUnit {
                logical_name: path.display().to_string(),
                origin_path: path.clone(),
                path: path.clone(),
                source: (*source).into(),
                program: parse_program_with_source_name(path.to_str().unwrap(), source).expect("parse"),
            }
        })
        .collect::<Vec<_>>();
    let main_path = units[0].path.clone();
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: root.clone() },
            dependencies: vec![],
        },
        Arc::new(units.clone()),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let project = ProjectSession::new(&db, root, main_path.clone(), "chained-receiver".into(), "test".into());
    let typed = build_typed_program(&mut db, project, generation, assembly.clone()).expect("typed assembly");
    let main_unit = SourceUnitId::new(&db, main_path);
    let index = SyntaxIndex::from_program(&units[0].program, generation);
    let calls = index
        .ids_of_kind(beskid_queries::IndexedNodeKind::CallExpression)
        .map(|node| AstNodeKey { unit: main_unit, generation, node })
        .collect::<Vec<_>>();
    let specialization = beskid_queries::generic_call_specialization(&db, calls[0]);
    if !supported {
        if let ImportedArgument::NominalReceiver = argument {
            assert!(
                specialization.is_err(),
                "unsupported receiver must not fabricate specialization: {model} {main_source}: {specialization:?}"
            );
            assert!(!matches!(call_lowering(&db, calls[1]), Ok(Some(beskid_queries::CallLowering::Direct(_)))));
            return;
        } else {
            for node in index.ids_of_kind(beskid_queries::IndexedNodeKind::IndexExpression) {
                let key = AstNodeKey { unit: main_unit, generation, node };
                assert!(
                    !matches!(beskid_queries::array_index_element_abi_type(&db, key), Ok(Some(_))),
                    "unproven array field cannot authorize an element layout: {model} {main_source}"
                );
            }
        }
    }
    if supported {
        let specialization = specialization.expect("generic argument facts").expect("Equal specialization");
        let (argument_type, indexed_type) = match argument {
            ImportedArgument::NominalReceiver => {
                (beskid_queries::SemanticTypeId::I64, beskid_queries::SemanticTypeId::POINTER)
            }
            ImportedArgument::ArrayElement => (beskid_queries::SemanticTypeId::U8, beskid_queries::SemanticTypeId::U8),
        };
        assert_eq!(specialization.signature.parameters.as_ref(), &[argument_type, argument_type]);
        if let ImportedArgument::NominalReceiver = argument {
            assert!(matches!(call_lowering(&db, calls[1]), Ok(Some(beskid_queries::CallLowering::Direct(_)))));
        }
        for node in index.ids_of_kind(beskid_queries::IndexedNodeKind::IndexExpression) {
            let key = AstNodeKey { unit: main_unit, generation, node };
            assert_eq!(beskid_queries::array_index_element_abi_type(&db, key).unwrap(), Some(indexed_type));
            assert_eq!(
                beskid_queries::managed_reference_kind(&db, key).unwrap(),
                Some(match argument {
                    ImportedArgument::NominalReceiver => beskid_queries::ManagedReferenceKind::GcManaged,
                    ImportedArgument::ArrayElement => beskid_queries::ManagedReferenceKind::NativeOrScalar,
                })
            );
        }
    }
    let target = TargetMetadata::supported()
        .into_iter()
        .find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu")
        .unwrap();
    let isa = isa::lookup_by_name("x86_64").unwrap().finish(settings::Flags::new(settings::builder())).unwrap();
    let roots = units
        .iter()
        .map(|unit| AstNodeKey { unit: SourceUnitId::new(&db, unit.path.clone()), generation, node: AstNodeId(0) })
        .collect::<Vec<_>>();
    let items = units
        .iter()
        .flat_map(|unit| {
            let unit_id = SourceUnitId::new(&db, unit.path.clone());
            let index = SyntaxIndex::from_program(&unit.program, generation);
            [beskid_queries::IndexedNodeKind::FunctionDefinition, beskid_queries::IndexedNodeKind::MethodDefinition]
                .into_iter()
                .flat_map(move |kind| index.ids_of_kind(kind).collect::<Vec<_>>())
                .map(move |node| AstNodeKey { unit: unit_id, generation, node })
        })
        .map(|key| SyntaxModuleItem { key, symbol: item_name(&db, key).unwrap().unwrap().to_string() })
        .collect::<Vec<_>>();
    let manifest = AbiManifestV5::canonical_runtime(target.clone());
    let input = CodegenInput::new(&db, typed, roots.into(), target, manifest).expect("codegen input");
    let emitted = lower_syntax_program(&input, isa.as_ref(), &items);
    if !supported {
        assert!(emitted.is_err(), "unproven array-field indexing must not lower: {model} {main_source}");
        return;
    }
    let artifact = emitted.expect("imported argument lowers through production ISLE");
    assert!(artifact.functions.iter().any(|function| function.name.starts_with("Equal#generic_")));
    let main = artifact.functions.iter().find(|function| function.name == "Main").expect("Main CLIF");
    assert!(
        main.function.display().to_string().contains("load"),
        "receiver projects the inner value rather than passing outer"
    );
}

pub(super) fn assert_string_content_comparison(clif: &str, comparison: StringComparison, scenario: &str) {
    let service_import = clif
        .lines()
        .find(|line| line.contains(" = %str_eq "))
        .unwrap_or_else(|| panic!("{scenario} must import the manifest-authorized str_eq service:\n{clif}"));
    let service_ref =
        service_import.split_whitespace().next().expect("str_eq import has a Cranelift function reference");
    let service_call = clif
        .lines()
        .find(|line| line.contains(&format!("= call {service_ref}(")))
        .unwrap_or_else(|| panic!("{scenario} must call its str_eq import:\n{clif}"));
    let service_result = service_call.split_whitespace().next().expect("str_eq call has a Cranelift result value");
    let zero = clif
        .lines()
        .find(|line| line.contains(" = iconst.i64 0"))
        .unwrap_or_else(|| panic!("{scenario} must compare the str_eq result with zero:\n{clif}"))
        .split_whitespace()
        .next()
        .expect("zero constant has a Cranelift value");
    let predicate = match comparison {
        StringComparison::Equal => "ne",
        StringComparison::NotEqual => "eq",
    };
    let expected = format!("icmp {predicate} {service_result}, {zero}");
    assert!(clif.contains(&expected), "{scenario} must derive its result from str_eq via `{expected}`:\n{clif}");
}
