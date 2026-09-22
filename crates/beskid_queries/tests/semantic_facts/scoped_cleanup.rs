use super::support::{key, setup};
use beskid_analysis::syntax_query::NodeKind;

const CONTRACT: &str = "enum DisposeError { Failed(i64 code) } enum Result<T, E> { Ok(T value), Error(E error) } contract Disposable { Result<unit, DisposeError> Dispose(); } type Resource: Disposable { pub Result<unit, DisposeError> Dispose() { return Result::Ok(unit); } } Resource Open() { return Resource {}; }";

#[test]
fn scoped_cleanup_requires_disposable_result_and_unique_explicit_conversion() {
    for (prefix, result, binding, expected) in [
        ("", "unit", "Resource", "NonResultCallable"),
        ("", "Result<unit, DisposeError>", "i64", "NotDisposable"),
        ("enum DomainError { Failed() }", "Result<unit, DomainError>", "Resource", "MissingConversion"),
        (
            "enum DomainError { Failed() } mod Hidden { [CleanupConversion] DomainError Convert(DisposeError error) { return DomainError::Failed(); } }",
            "Result<unit, DomainError>",
            "Resource",
            "MissingConversion",
        ),
        (
            "enum DomainError { Failed() } [CleanupConversion] DomainError First(DisposeError error) { return DomainError::Failed(); } [CleanupConversion] DomainError Second(DisposeError error) { return DomainError::Failed(); }",
            "Result<unit, DomainError>",
            "Resource",
            "AmbiguousConversion",
        ),
        (
            "enum DomainError { Failed() } [CleanupConversion] DomainError Convert<T>(DisposeError error) { return DomainError::Failed(); }",
            "Result<unit, DomainError>",
            "Resource",
            "InvalidConversion",
        ),
    ] {
        let source = format!("{CONTRACT} {prefix} {result} Main() {{ use {binding} resource = Open(); return; }}");
        let (db, _, unit, generation, index) = setup(&source);
        let scoped = key(unit, generation, &index, NodeKind::ScopedUseStatement, 0);
        let fact = beskid_queries::scoped_cleanup(&db, scoped).unwrap().unwrap();
        assert_eq!(format!("{:?}", fact.diagnostic), format!("Some({expected})"), "{source}");
    }
}

#[test]
fn nested_private_contract_does_not_shadow_the_visible_cleanup_contract() {
    let source = format!(
        "{CONTRACT} mod Hidden {{ contract Disposable {{ unit Dispose(); }} }} Result<unit, DisposeError> Main() {{ use Resource resource = Open(); return Result::Ok(()); }}"
    );
    let (db, _, unit, generation, index) = setup(&source);
    let fact = beskid_queries::scoped_cleanup(&db, key(unit, generation, &index, NodeKind::ScopedUseStatement, 0))
        .unwrap()
        .unwrap();
    assert!(fact.diagnostic.is_none(), "{fact:?}");
}

#[test]
fn dispose_signature_must_match_the_exact_contract() {
    for method in [
        "pub unit Dispose() { return; }",
        "pub Result<i64, DisposeError> Dispose() { return Result::Ok(0_i64); }",
        "pub Result<unit, DisposeError> Dispose(i64 extra) { return Result::Ok(()); }",
        "pub Result<unit, i64> Dispose() { return Result::Error(1_i64); }",
    ] {
        let source = format!(
            "enum DisposeError {{ Failed() }} enum Result<T, E> {{ Ok(T value), Error(E error) }} contract Disposable {{ Result<unit, DisposeError> Dispose(); }} type Resource: Disposable {{ {method} }} Result<unit, DisposeError> Main() {{ use Resource resource = Resource {{}}; return Result::Ok(()); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let fact = beskid_queries::scoped_cleanup(&db, key(unit, generation, &index, NodeKind::ScopedUseStatement, 0))
            .unwrap()
            .unwrap();
        assert_eq!(fact.diagnostic, Some(beskid_queries::ScopedCleanupDiagnostic::InvalidDisposeSignature));
    }
}

#[test]
fn cleanup_string_errors_use_the_existing_managed_reference_authority() {
    let source = format!(
        "{CONTRACT} [CleanupConversion] string Convert(DisposeError error) {{ return \"cleanup\"; }} Result<unit, string> Main() {{ use Resource resource = Open(); return Result::Ok(()); }}"
    );
    let (db, _, unit, generation, index) = setup(&source);
    let fact = beskid_queries::scoped_cleanup(&db, key(unit, generation, &index, NodeKind::ScopedUseStatement, 0))
        .unwrap()
        .unwrap();
    assert!(fact.diagnostic.is_none(), "{fact:?}");
    assert!(fact.converted_error_managed, "string cleanup error must be rooted across result allocation");
}

#[test]
fn scoped_cleanup_resolves_identity_and_marked_conversion_to_source_declarations() {
    for prefix in [
        "",
        "enum DomainError { Failed() } [CleanupConversion] DomainError Convert(DisposeError error) { return DomainError::Failed(); }",
    ] {
        let error = if prefix.is_empty() { "DisposeError" } else { "DomainError" };
        let source = format!(
            "{CONTRACT} {prefix} Result<unit, {error}> Main() {{ use Resource resource = Open(); return Result::Ok(unit); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let scoped = key(unit, generation, &index, NodeKind::ScopedUseStatement, 0);
        let fact = beskid_queries::scoped_cleanup(&db, scoped).unwrap().unwrap();
        assert!(fact.diagnostic.is_none(), "{fact:?}");
        assert!(fact.dispose.is_some(), "the concrete Dispose method must be resolved");
        assert_eq!(fact.conversion.is_some(), !prefix.is_empty());
        let callees = beskid_queries::direct_callees(&db, fact.callable.unwrap()).unwrap().unwrap();
        assert!(
            callees.contains(&fact.dispose.unwrap()),
            "implicit cleanup must retain its actual Dispose implementation"
        );
        if let Some(conversion) = fact.conversion {
            assert!(callees.contains(&conversion));
        }
    }
}

#[test]
fn scoped_resources_cannot_be_copied_captured_returned_or_manually_disposed() {
    for (body, expected) in [
        ("let alias = resource;", "ResourceEscapesScope"),
        ("Store(resource);", "ResourceEscapesScope"),
        ("let capture = () => resource;", "ResourceEscapesScope"),
        ("return resource;", "ResourceEscapesScope"),
        ("resource.Dispose();", "ExplicitDispose"),
        ("let method = resource.Dispose;", "ResourceEscapesScope"),
    ] {
        let source = format!(
            "{CONTRACT} Result<unit, DisposeError> Main() {{ use Resource resource = Open(); {body} return Result::Ok(unit); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let scoped = key(unit, generation, &index, NodeKind::ScopedUseStatement, 0);
        let fact = beskid_queries::scoped_cleanup(&db, scoped).unwrap().unwrap();
        assert_eq!(format!("{:?}", fact.diagnostic), format!("Some({expected})"), "{body}");
    }
}

#[test]
fn scoped_receiver_methods_must_not_leak_self_directly_or_transitively() {
    for (methods, expected) in [
        ("pub i64 Read() { return 7_i64; }", None),
        ("pub i64 Value() { return 7_i64; } pub i64 Read() { return Value(); }", None),
        ("pub Resource Read() { return self; }", Some("ResourceEscapesScope")),
        ("pub Resource Leak() { return self; } pub Resource Read() { return Leak(); }", Some("ResourceEscapesScope")),
        ("pub i64 Read() { Store(self); return 0_i64; }", Some("ResourceEscapesScope")),
        ("pub i64 Read() { let capture = () => self; return 0_i64; }", Some("ResourceEscapesScope")),
        (
            "pub i64 Value() { return 7_i64; } pub i64 Read() { let callback = Value; Store(callback); return 0_i64; }",
            Some("ResourceEscapesScope"),
        ),
    ] {
        let source = format!(
            "enum DisposeError {{ Failed(i64 code) }} enum Result<T, E> {{ Ok(T value), Error(E error) }} contract Disposable {{ Result<unit, DisposeError> Dispose(); }} type Resource: Disposable {{ pub Result<unit, DisposeError> Dispose() {{ return Result::Ok(unit); }} {methods} }} Resource Open() {{ return Resource {{}}; }} Result<unit, DisposeError> Main() {{ use Resource resource = Open(); resource.Read(); return Result::Ok(unit); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let scoped = key(unit, generation, &index, NodeKind::ScopedUseStatement, 0);
        let fact = beskid_queries::scoped_cleanup(&db, scoped).unwrap().unwrap();
        assert_eq!(fact.diagnostic.map(|diagnostic| format!("{diagnostic:?}")).as_deref(), expected, "{methods}");
    }
}

#[test]
fn scoped_acquisition_requires_source_proven_fresh_ownership() {
    for (extra, initializer, accepted) in [
        ("", "Resource {}", true),
        ("", "Open()", true),
        ("Resource Fresh() { return Open(); }", "Fresh()", true),
        ("Resource Fresh(bool again) { if again { return Fresh(false); } return Resource {}; }", "Fresh(true)", true),
        ("", "existing", false),
        ("Resource Borrow(Resource value) { return value; }", "Borrow(existing)", false),
        ("Resource Loop() { return Loop(); }", "Loop()", false),
        ("", "Unknown()", false),
        ("", "factory()", false),
        ("type Holder { Resource item, pub Resource Borrow() { return item; } }", "holder.Borrow()", false),
    ] {
        let source = format!(
            "{CONTRACT} {extra} Result<unit, DisposeError> Main(Resource existing) {{ use Resource resource = {initializer}; return Result::Ok(unit); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let scoped = key(unit, generation, &index, NodeKind::ScopedUseStatement, 0);
        let fact = beskid_queries::scoped_cleanup(&db, scoped).unwrap().unwrap();
        assert_eq!(fact.diagnostic.is_none(), accepted, "{source}: {fact:?}");
    }
}

#[test]
fn scoped_fallible_acquisition_proves_only_fresh_success_payloads() {
    for factory in [
        "Result<Resource, DisposeError> Acquire() { return Result::Ok(Resource {}); }",
        "Result<Resource, DisposeError> Acquire() { return Result::Ok(Open()); }",
        "Result<Resource, DisposeError> Inner() { if false { return Result::Error(DisposeError::Failed(1_i64)); } return Result::Ok(Resource {}); } Result<Resource, DisposeError> Acquire() { return Inner(); }",
    ] {
        let source = format!(
            "{CONTRACT} {factory} Result<unit, DisposeError> Main() {{ use Resource resource = Acquire()?; return Result::Ok(()); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let expression = key(unit, generation, &index, NodeKind::TryExpression, 0);
        assert!(beskid_queries::try_expression_fact(&db, expression).unwrap().is_some(), "{source}");
        let fact = beskid_queries::scoped_cleanup(&db, key(unit, generation, &index, NodeKind::ScopedUseStatement, 0))
            .unwrap()
            .unwrap();
        assert!(fact.diagnostic.is_none(), "{source}: {fact:?}");
        assert!(fact.acquisition.is_some());
    }
}

#[test]
fn scoped_fallible_acquisition_rejects_aliases_cycles_and_unproven_successes() {
    for (factory, initializer) in [
        ("", "existing?"),
        (
            "Result<Resource, DisposeError> Acquire(Resource borrowed) { return Result::Ok(borrowed); }",
            "Acquire(borrowed)?",
        ),
        (
            "Result<Resource, DisposeError> Acquire() { Resource alias = Resource {}; return Result::Ok(alias); }",
            "Acquire()?",
        ),
        ("Result<Resource, DisposeError> Acquire() { return Acquire(); }", "Acquire()?"),
        (
            "Result<Resource, DisposeError> Acquire() { if true { return Result::Ok(Resource {}); } return Acquire(); }",
            "Acquire()?",
        ),
        (
            "Result<Resource, DisposeError> Acquire() { return Result::Error(DisposeError::Failed(1_i64)); }",
            "Acquire()?",
        ),
        ("Result<Resource, DisposeError> Acquire() { return Result::Ok(Unknown()); }", "Acquire()?"),
        (
            "Result<Resource, DisposeError> Acquire(Resource borrowed) { if true { return Result::Ok(Resource {}); } return Result::Ok(borrowed); }",
            "Acquire(borrowed)?",
        ),
        (
            "enum OtherError { Failed() } Result<Resource, OtherError> Acquire() { return Result::Ok(Resource {}); }",
            "Acquire()?",
        ),
    ] {
        let source = format!(
            "{CONTRACT} {factory} Result<unit, DisposeError> Main(Resource borrowed, Result<Resource, DisposeError> existing) {{ use Resource resource = {initializer}; return Result::Ok(()); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let fact = beskid_queries::scoped_cleanup(&db, key(unit, generation, &index, NodeKind::ScopedUseStatement, 0))
            .unwrap()
            .unwrap();
        assert_eq!(fact.diagnostic, Some(beskid_queries::ScopedCleanupDiagnostic::ResourceEscapesScope), "{source}");
        assert!(fact.acquisition.is_none());
    }
}

#[test]
fn scoped_binding_is_not_resolvable_after_its_lexical_block() {
    for region in [
        "if true { use Resource resource = Open(); resource.Dispose(); }",
        "use (Resource resource = Open()) { resource.Dispose(); }",
    ] {
        let source = format!(
            "{CONTRACT} Result<unit, DisposeError> Main() {{ {region} resource.Dispose(); return Result::Ok(unit); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let paths = index
            .ids_of_kind(NodeKind::PathExpression)
            .filter_map(|node| {
                let key = beskid_queries::AstNodeKey { unit, generation, node };
                let span = index.metadata_for(generation, node)?.span?;
                source.get(span.start..span.end).filter(|text| *text == "resource.Dispose").map(|_| key)
            })
            .collect::<Vec<_>>();
        assert_eq!(paths.len(), 2);
        assert!(beskid_queries::nominal_member_receiver(&db, paths[0]).unwrap().is_some());
        assert!(beskid_queries::nominal_member_receiver(&db, paths[1]).unwrap().is_none());
    }
}

#[test]
fn explicit_use_body_shares_the_cleanup_fact_and_rejects_capture() {
    for (body, valid) in [("resource.Read();", true), ("let escape = () => resource;", false)] {
        let source = format!(
            "enum DisposeError {{ Failed() }} enum Result<T, E> {{ Ok(T value), Error(E error) }} contract Disposable {{ Result<unit, DisposeError> Dispose(); }} type Resource: Disposable {{ pub Result<unit, DisposeError> Dispose() {{ return Result::Ok(()); }} pub i64 Read() {{ return 7_i64; }} }} Result<unit, DisposeError> Main() {{ use (Resource resource = Resource {{}}) {{ {body} }} return Result::Ok(()); }}"
        );
        let (db, _, unit, generation, index) = setup(&source);
        let fact = beskid_queries::scoped_cleanup(&db, key(unit, generation, &index, NodeKind::ScopedUseStatement, 0))
            .unwrap()
            .unwrap();
        assert!(fact.body.is_some());
        assert_eq!(fact.diagnostic.is_none(), valid, "{fact:?}");
    }
}

#[test]
fn invalid_scoped_cleanup_is_rejected_before_a_typed_program_is_admitted() {
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use std::{path::PathBuf, sync::Arc};
    let source = format!("{CONTRACT} unit Main() {{ use Resource resource = Open(); return; }}");
    let program = beskid_analysis::services::parse_program(&source).unwrap();
    let (mut db, project, _, generation, _) = setup(&source);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: PathBuf::from("/tmp/project/src") },
            dependencies: vec![],
        },
        Arc::new(vec![SourceUnit {
            origin_path: PathBuf::from("/tmp/project/src/Main.bd"),
            path: PathBuf::from("/tmp/project/src/Main.bd"),
            logical_name: "Main".into(),
            source,
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let result = beskid_queries::build_typed_program(&mut db, project, generation, assembly);
    assert!(result.is_err(), "invalid cleanup must fail semantic admission, before lowering");
    assert!(result.err().unwrap().to_string().contains("NonResultCallable"));
}

#[test]
fn canonical_foundation_disposal_resolves_across_registered_modules() {
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use std::{path::PathBuf, sync::Arc};
    let foundation =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corelib/packages/foundation/src").canonicalize().unwrap();
    let source = "use Core.Disposable; use Core.Results; type Resource: Disposable { pub Core.Results.Result<unit, DisposeError> Dispose() { return Result::Ok(()); } } Result<unit, DisposeError> Main() { use Resource resource = Resource {}; return Result::Ok(()); }";
    let (mut db, project, unit, generation, index) = setup(source);
    let mut units = vec![SourceUnit {
        origin_path: PathBuf::from("/tmp/project/src/Main.bd"),
        path: PathBuf::from("/tmp/project/src/Main.bd"),
        logical_name: "Main".into(),
        source: source.into(),
        program: beskid_analysis::services::parse_program(source).unwrap(),
    }];
    for relative in ["Core/Disposable.bd", "Core/Results/Results.bd"] {
        let path = foundation.join(relative);
        let source = std::fs::read_to_string(&path).unwrap();
        let program = beskid_analysis::services::parse_program(&source).unwrap();
        units.push(SourceUnit { origin_path: path.clone(), path, logical_name: relative.into(), source, program });
    }
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: PathBuf::from("/tmp/project/src") },
            dependencies: vec![RootEntry { dependency_name: Some("foundation".into()), source_root: foundation }],
        },
        Arc::new(units),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    beskid_queries::build_typed_program(&mut db, project, generation, assembly)
        .expect("canonical Disposable import must satisfy scoped cleanup");
    let fact = beskid_queries::scoped_cleanup(&db, key(unit, generation, &index, NodeKind::ScopedUseStatement, 0))
        .unwrap()
        .unwrap();
    assert!(fact.diagnostic.is_none());
}
