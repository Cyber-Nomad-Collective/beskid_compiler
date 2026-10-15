//! Glue declaration selection follows the manifest Glue libraries, as the semantic extern profile
//! does. An assembly that also holds a non-Glue `Extern` contract over unbranded nominal types (the
//! shape of the Corelib `beskid_runtime` Dynamic bridges) must not fail the Glue build with
//! "nominal Glue types require exactly one validated GlueHandle brand".
use beskid_abi::abi_v5::{AbiManifestV5, TargetMetadata};
use beskid_aot::api::glue::glue_declarations;
use beskid_analysis::projects::{AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit};
use beskid_queries::{
    AstNodeId, AstNodeKey, BeskidDatabase, GlueDirection, ProjectSession, SourceUnitId, SyntaxGenerationId,
    build_typed_program,
};
use std::sync::Arc;

const SOURCE: &str = r#"
[GlueHandle(Library:"glue_manual", Nullable:false)]
[RustOwner(Path:"implementation::ManualOwned")]
pub type ManualOpaque<T> { u64 token, }

pub type RuntimePlain { i64 value, }

[Extern(Abi:"C", Library:"beskid_runtime")]
contract RuntimeBridge {
    RuntimePlain beskid_runtime_plain(RuntimePlain value);
}

[Extern(Abi:"C", Library:"glue_manual")]
pub contract Foreign {
    [RustOwner(Fallible:true)]
    ManualOpaque<u8> make_manual_owned(word length);
    [RustOwner(Fallible:true)]
    word check_manual_owned(ManualOpaque<u8> value);
}

[Export(Abi:"C", Symbol:"beskid_handle")]
pub ManualOpaque<u8> ExportHandle(ManualOpaque<u8> value) { return value; }
"#;

#[test]
fn glue_declarations_select_only_manifest_glue_libraries() {
    let directory = tempfile::tempdir().unwrap();
    // Units are interned canonically (macOS /var is /private/var), so the fixture must use the canonical path.
    let directory_path = directory.path().canonicalize().unwrap();
    let path = directory_path.join("Main.bd");
    std::fs::write(&path, SOURCE).unwrap();
    let program = beskid_analysis::services::parse_program_with_source_name(path.to_str().unwrap(), SOURCE).unwrap();
    let mut db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, path.clone());
    let project = ProjectSession::new(&db, directory_path.clone(), path.clone(), "App".into(), "lock".into());
    let generation = SyntaxGenerationId(1);
    let assembly = Arc::new(ProgramAssembly::new(
        EffectiveCompilationRoots {
            host: RootEntry { dependency_name: None, source_root: directory_path.clone() },
            dependencies: vec![],
        },
        Arc::new(vec![SourceUnit {
            logical_name: "Main".into(),
            origin_path: path.clone(),
            path,
            source: SOURCE.into(),
            program,
        }]),
        0,
        AssemblyDiscovery::ImportClosure,
        Arc::new(ModuleIndex::empty()),
        false,
        generation,
    ));
    let typed = build_typed_program(&mut db, project, generation, assembly).unwrap();
    let root = AstNodeKey { unit, generation, node: AstNodeId(0) };
    let target =
        TargetMetadata::supported().into_iter().find(|target| target.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap();
    let input =
        beskid_codegen::CodegenInput::new(&db, typed, Arc::from([root]), target.clone(), AbiManifestV5::canonical_runtime(target))
            .unwrap();

    let declarations = glue_declarations(&input, &[], &["glue_manual".to_owned()])
        .expect("a non-Glue Extern contract must not be judged as a Glue import");
    let mut selected = declarations
        .iter()
        .map(|declaration| (declaration.direction.clone(), declaration.library.clone(), declaration.symbol.clone()))
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| left.2.cmp(&right.2));
    assert_eq!(
        selected,
        vec![
            (GlueDirection::Export, None, "beskid_handle".to_owned()),
            (GlueDirection::Import, Some("glue_manual".to_owned()), "check_manual_owned".to_owned()),
            (GlueDirection::Import, Some("glue_manual".to_owned()), "make_manual_owned".to_owned()),
        ]
    );

    // Fail closed: a library that the manifest declares as Glue is still held to the brand rule.
    let error = glue_declarations(&input, &[], &["glue_manual".to_owned(), "beskid_runtime".to_owned()])
        .expect_err("an unbranded nominal in a declared Glue import must be rejected");
    assert!(error.to_string().contains("exactly one validated GlueHandle brand"), "{error}");
}
