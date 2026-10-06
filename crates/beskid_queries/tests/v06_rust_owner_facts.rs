//! `RustOwner` source facts: the only authority for Rust owner type and callable mappings.
use beskid_analysis::{
    projects::{AssemblyOptions, CompilePlan, ProgramAssembly, Target, TargetKind, assemble_program_with_materializer},
    syntax_query::NodeKind,
};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, GlueLogicalType, GlueOwnerBindingDigest, RustOwnerCallableRow, RustOwnerPlacement,
    RustOwnerTypeRow, SourceUnitId, build_typed_program, glue_binding, item_name,
    project_session_for_planned_syntax_assembly, rust_owner_declarations, rust_owner_tables, validate_rust_owner_path,
};
use std::sync::Arc;

const HANDLE: &str = "[GlueHandle(Library:\"glue_manual\", Nullable:false)]\n";
const OWNED: &str = "[RustOwner(Path:\"implementation::ManualOwned\")]\n";
const TYPE: &str = "pub type ManualOpaque<T> { u64 token, }\n";
const CONTRACT: &str = r#"[Extern(Abi:"C", Library:"glue_manual")]
pub contract Foreign {
    i32 glue_i32(i32 value);
    [RustOwner(Fallible:true)]
    i32 glue_failure(u32 mode);
    [RustOwner(Path:"implementation::nested::renamed", Fallible:false)]
    i32 glue_renamed(i32 value);
    [RustOwner(Fallible:true)]
    ManualOpaque<u8> make_manual_owned(i32 length);
    i32 check_manual_owned(ManualOpaque<u8> value);
}
"#;
const BRAND: &str = "b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0";

struct Fixture {
    _root: tempfile::TempDir,
    db: BeskidDatabase,
    unit: SourceUnitId,
    assembly: Arc<ProgramAssembly>,
}

impl Fixture {
    fn new(source: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("Src");
        std::fs::create_dir(&src).unwrap();
        let path = src.join("Main.bd");
        std::fs::write(&path, source).unwrap();
        let plan = CompilePlan {
            project_root: root.path().to_owned(),
            manifest_path: root.path().join("Fixture.bproj"),
            project_name: "Fixture".into(),
            source_root: src,
            target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
            dependency_projects: vec![],
            unresolved_dependencies: vec![],
            has_std_dependency: false,
        };
        let assembly = Arc::new(
            assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None)
                .unwrap(),
        );
        let mut db = BeskidDatabase::default();
        let project =
            project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "rust-owner-facts".into())
                .unwrap();
        build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
        let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
        Self { _root: root, db, unit, assembly }
    }

    fn key(&self, node: beskid_analysis::syntax::AstNodeId) -> AstNodeKey {
        AstNodeKey { unit: self.unit, generation: self.assembly.generation, node }
    }

    fn named(&self, kind: NodeKind, name: &str) -> AstNodeKey {
        self.assembly
            .entry_syntax_index()
            .ids_of_kind(kind)
            .map(|node| self.key(node))
            .find(|key| item_name(&self.db, *key).ok().flatten().is_some_and(|value| value.as_ref() == name))
            .unwrap_or_else(|| panic!("declaration `{name}` absent"))
    }

    fn method(&self, name: &str) -> AstNodeKey {
        self.named(NodeKind::ContractMethodSignature, name)
    }

    fn function(&self, name: &str) -> AstNodeKey {
        self.named(NodeKind::FunctionDefinition, name)
    }

    fn handle_type(&self) -> AstNodeKey {
        self.key(self.assembly.entry_syntax_index().ids_of_kind(NodeKind::TypeDefinition).next().unwrap())
    }

    fn declarations_error(&self) -> String {
        rust_owner_declarations(&self.db, self.unit).expect_err("RustOwner placement must be rejected").to_string()
    }

    fn binding_error(&self, key: AstNodeKey) -> String {
        glue_binding(&self.db, key).expect_err("Glue binding must be rejected").to_string()
    }

    /// Canonical digests as the Glue artifact issues them: one identity per binding, one brand per
    /// handle position. Mapping data never comes from here.
    fn digest(&self, name: &str, identity: char, opaque: Vec<(Option<usize>, String)>) -> GlueOwnerBindingDigest {
        GlueOwnerBindingDigest {
            binding: self.method(name),
            symbol: name.into(),
            identity_sha256: identity.to_string().repeat(64),
            opaque,
        }
    }

    fn digests(&self) -> Vec<GlueOwnerBindingDigest> {
        vec![
            self.digest("glue_i32", '1', vec![]),
            self.digest("glue_failure", '2', vec![]),
            self.digest("glue_renamed", '3', vec![]),
            self.digest("make_manual_owned", '4', vec![(None, BRAND.into())]),
            self.digest("check_manual_owned", '5', vec![(Some(0), BRAND.into())]),
        ]
    }
}

fn owned_source() -> String {
    format!("{HANDLE}{OWNED}{TYPE}{CONTRACT}")
}

fn assert_spanned(error: &str, expected: &str) {
    assert!(error.contains(expected), "`{error}` does not contain `{expected}`");
    assert!(error.contains(" at ") && error.contains("(bytes "), "`{error}` carries no source span");
}

#[test]
fn declared_and_default_mappings_are_binding_and_brand_facts() {
    let fixture = Fixture::new(&owned_source());
    let db = &fixture.db;
    let default = glue_binding(db, fixture.method("glue_i32")).unwrap().unwrap().rust_owner.unwrap();
    assert_eq!((&*default.path, default.fallible, default.declared), ("implementation::glue_i32", false, false));
    let fallible = glue_binding(db, fixture.method("glue_failure")).unwrap().unwrap().rust_owner.unwrap();
    assert_eq!((&*fallible.path, fallible.fallible, fallible.declared), ("implementation::glue_failure", true, true));
    let renamed = glue_binding(db, fixture.method("glue_renamed")).unwrap().unwrap().rust_owner.unwrap();
    assert_eq!((&*renamed.path, renamed.fallible), ("implementation::nested::renamed", false));
    let factory = glue_binding(db, fixture.method("make_manual_owned")).unwrap().unwrap();
    let GlueLogicalType::Handle(brand) = &factory.result else { panic!("factory result is not a Glue handle") };
    assert_eq!(brand.declaration, fixture.handle_type());
    assert_eq!(brand.rust_owner.as_deref(), Some("implementation::ManualOwned"));

    let declarations = rust_owner_declarations(db, fixture.unit).unwrap();
    let rows = declarations
        .iter()
        .map(|row| (row.placement, row.library.as_str(), &*row.path, row.fallible))
        .collect::<Vec<_>>();
    assert_eq!(
        rows,
        [
            (RustOwnerPlacement::HandleType, "glue_manual", "implementation::ManualOwned", false),
            (RustOwnerPlacement::ExternMethod, "glue_manual", "implementation::glue_failure", true),
            (RustOwnerPlacement::ExternMethod, "glue_manual", "implementation::nested::renamed", false),
            (RustOwnerPlacement::ExternMethod, "glue_manual", "implementation::make_manual_owned", true),
        ]
    );
    assert_eq!(declarations[0].declaration, fixture.handle_type());
}

#[test]
fn owner_tables_contain_one_brand_row_and_one_callable_per_binding() {
    let fixture = Fixture::new(&owned_source());
    let tables = rust_owner_tables(&fixture.db, "glue_manual", &fixture.digests()).unwrap();
    assert_eq!(tables.library, "glue_manual");
    assert_eq!(
        tables.types,
        [RustOwnerTypeRow { brand_sha256: BRAND.into(), rust_type_path: "implementation::ManualOwned".into() }]
    );
    let row = |identity: char, path: &str, fallible: bool| RustOwnerCallableRow {
        binding_identity_sha256: identity.to_string().repeat(64),
        rust_callable_path: path.into(),
        fallible,
    };
    assert_eq!(
        tables.callables,
        [
            row('1', "implementation::glue_i32", false),
            row('2', "implementation::glue_failure", true),
            row('3', "implementation::nested::renamed", false),
            row('4', "implementation::make_manual_owned", true),
            row('5', "implementation::check_manual_owned", false),
        ]
    );
}

#[test]
fn mapping_text_changes_the_owner_tables() {
    let base = Fixture::new(&owned_source());
    let base = rust_owner_tables(&base.db, "glue_manual", &base.digests()).unwrap();
    let path = Fixture::new(&owned_source().replace("implementation::ManualOwned", "implementation::OtherOwned"));
    assert_ne!(rust_owner_tables(&path.db, "glue_manual", &path.digests()).unwrap().types, base.types);
    let fallible = Fixture::new(&owned_source().replace(
        "[RustOwner(Path:\"implementation::nested::renamed\", Fallible:false)]",
        "[RustOwner(Path:\"implementation::nested::renamed\", Fallible:true)]",
    ));
    assert_ne!(rust_owner_tables(&fallible.db, "glue_manual", &fallible.digests()).unwrap().callables, base.callables);
}

#[test]
fn glue_handle_without_rust_owner_fails_the_owner_tables() {
    let fixture = Fixture::new(&format!("{HANDLE}{TYPE}{CONTRACT}"));
    let factory = glue_binding(&fixture.db, fixture.method("make_manual_owned")).unwrap().unwrap();
    let GlueLogicalType::Handle(brand) = &factory.result else { panic!("factory result is not a Glue handle") };
    assert_eq!(brand.rust_owner, None);
    let error = rust_owner_tables(&fixture.db, "glue_manual", &fixture.digests()).unwrap_err().to_string();
    assert_spanned(&error, "requires RustOwner");
}

#[test]
fn malformed_type_attributes_are_rejected_with_spans() {
    for (attribute, expected) in [
        (format!("{OWNED}{OWNED}"), "duplicate RustOwner"),
        ("[RustOwner(Path:\"implementation::A\", Fallible:true)]\n".into(), "Fallible is not allowed"),
        ("[RustOwner]\n".into(), "requires Path"),
        ("[RustOwner(Path:\"implementation::A\", Path:\"implementation::B\")]\n".into(), "unknown or repeated"),
        ("[RustOwner(Path:\"implementation::A\", Mode:1)]\n".into(), "unknown or repeated"),
        ("[RustOwner(Path:\"std::process::abort\")]\n".into(), "must start with `implementation::`"),
    ] {
        let fixture = Fixture::new(&format!("{HANDLE}{attribute}{TYPE}{CONTRACT}"));
        assert_spanned(&fixture.declarations_error(), expected);
        assert_spanned(&fixture.binding_error(fixture.method("make_manual_owned")), expected);
    }
}

#[test]
fn malformed_method_attributes_are_rejected_with_spans() {
    for (attribute, expected) in [
        ("[RustOwner(Fallible:true)] [RustOwner(Fallible:false)]", "duplicate RustOwner"),
        ("[RustOwner(Fallible:1)]", "must be a bool literal"),
        ("[RustOwner(Fallible:true, Fallible:false)]", "unknown or repeated"),
        ("[RustOwner(Symbol:\"x\")]", "unknown or repeated"),
        ("[RustOwner(Path:\"crate::glue_i32\")]", "must start with `implementation::`"),
        ("[RustOwner(Path:\"implementation::super::glue_i32\")]", "invalid segment `super`"),
    ] {
        let source = owned_source().replace("    i32 glue_i32(i32 value);", &format!("    {attribute}\n    i32 glue_i32(i32 value);"));
        let fixture = Fixture::new(&source);
        assert_spanned(&fixture.declarations_error(), expected);
        assert_spanned(&fixture.binding_error(fixture.method("glue_i32")), expected);
    }
}

#[test]
fn misplaced_rust_owner_is_rejected() {
    let export = "[Export(Abi:\"C\", Symbol:\"exported\")]\n[RustOwner(Path:\"implementation::exported\")]\npub i32 Exported(i32 value) { return value; }\n";
    let fixture = Fixture::new(&format!("{}{export}", owned_source()));
    assert_spanned(&fixture.declarations_error(), "allowed only on a GlueHandle type or an Extern contract method");
    assert_spanned(&fixture.binding_error(fixture.function("Exported")), "not allowed on an Export function");

    for misplaced in [
        "[RustOwner(Path:\"implementation::plain\")]\npub i32 Plain(i32 value) { return value; }\n",
        "[RustOwner(Path:\"implementation::Plain\")]\npub type Plain { u64 token, }\n",
        "pub contract Local {\n    [RustOwner(Path:\"implementation::local\")]\n    i32 local(i32 value);\n}\n",
        "[RustOwner(Path:\"implementation::Foreign\")]\n[Extern(Abi:\"C\", Library:\"other\")]\npub contract Other {\n    i32 other(i32 value);\n}\n",
        "pub type Fielded {\n    [RustOwner(Path:\"implementation::field\")]\n    u64 token,\n}\n",
    ] {
        let fixture = Fixture::new(&format!("{}{misplaced}", owned_source()));
        assert_spanned(&fixture.declarations_error(), "allowed only on a GlueHandle type or an Extern contract method");
    }
}

#[test]
fn owner_tables_reject_rows_outside_the_owner_signature() {
    let fixture = Fixture::new(&owned_source());
    let db = &fixture.db;
    let mut foreign = fixture.digests();
    foreign[0].symbol = "glue_failure".into();
    assert!(rust_owner_tables(db, "glue_manual", &foreign).is_err(), "symbol must match the binding fact");
    assert!(rust_owner_tables(db, "other", &fixture.digests()).is_err(), "rows must be imports of the owner");
    let mut duplicate = fixture.digests();
    duplicate[1].identity_sha256 = duplicate[0].identity_sha256.clone();
    assert!(rust_owner_tables(db, "glue_manual", &duplicate).is_err(), "identities must be unique");
    let mut missing = fixture.digests();
    missing[4].opaque.clear();
    assert!(rust_owner_tables(db, "glue_manual", &missing).is_err(), "every handle position needs a brand");
    let mut moved = fixture.digests();
    moved[4].opaque = vec![(None, BRAND.into())];
    assert!(rust_owner_tables(db, "glue_manual", &moved).is_err(), "brand positions are canonical");
    let mut malformed = fixture.digests();
    malformed[3].opaque = vec![(None, "zz".into())];
    assert!(rust_owner_tables(db, "glue_manual", &malformed).is_err(), "brands are sha256 digests");
    assert!(rust_owner_tables(db, "glue_manual", &[]).is_err(), "an owner without bindings is rejected");
}

#[test]
fn rust_owner_paths_are_closed_to_the_implementation_module() {
    for valid in ["implementation::ManualOwned", "implementation::nested::glue_i32", "implementation::_private"] {
        validate_rust_owner_path(valid).unwrap();
    }
    for invalid in [
        "implementation",
        "ManualOwned",
        "std::process::abort",
        "core::ptr::null",
        "crate::implementation::X",
        "implementation::self::X",
        "implementation::super::X",
        "implementation::std::X",
        "implementation::r#type",
        "implementation::Vec<u8>",
        "implementation::fn",
        "implementation::",
        "implementation::::X",
        "::implementation::X",
        "implementation::_",
        "implementation::1x",
        "implementation::caf\u{e9}",
    ] {
        assert!(validate_rust_owner_path(invalid).is_err(), "`{invalid}` must be rejected");
    }
}
