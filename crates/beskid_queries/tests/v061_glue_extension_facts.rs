//! Glue extension facts (v0.6.1): records by value, fieldless enums and call-scoped callbacks,
//! their rejections, and the one boundary vocabulary (Glue lowers to Interop.Contracts).
use beskid_abi::interop::{
    CallShapeClass, OwnershipClass, ScalarDomain, TypeShape,
    c_profile::{CAbiProfile, CProfileView},
};
use beskid_analysis::{
    projects::{AssemblyOptions, CompilePlan, ProgramAssembly, Target, TargetKind, assemble_program_with_materializer},
    syntax::PrimitiveType,
    syntax_query::NodeKind,
};
use beskid_queries::{
    AstNodeKey, BeskidDatabase, GlueLogicalForm, GlueLogicalType, GlueOwnerBindingDigest, RustOwnerPlacement,
    SourceUnitId, build_typed_program, glue_binding, glue_interop_signature, glue_logical_form, glue_type_shape,
    item_name, project_session_for_planned_syntax_assembly, rust_owner_declarations, rust_owner_tables,
};
use std::sync::Arc;

const TYPES: &str = r#"[GlueRecord(Library:"glue_manual")]
[RustOwner(Path:"implementation::Point")]
pub type Point { u8 tag, i64 x, f64 weight, bool visible, char mark, word size, }

[GlueEnum(Library:"glue_manual")]
[RustOwner(Path:"implementation::Color")]
pub enum Color { Red, Green, Blue, }

[GlueHandle(Library:"glue_manual", Nullable:true)]
[RustOwner(Path:"implementation::ManualOwned")]
pub type ManualOpaque<T> { u64 token, }
"#;
const CONTRACT: &str = r#"[Extern(Abi:"C", Library:"glue_manual")]
pub contract Foreign {
    Point glue_point(Point value);
    Color glue_color(Color value);
    i64 glue_each(i64 count, (i64, Color) => bool visit);
    i64 glue_tick((u32) => unit tick);
    i64 glue_handle(ManualOpaque<u8> value);
    string glue_text(string value, u8[] bytes);
}
[Export(Abi:"C", Symbol:"beskid_point")]
pub Point ExportPoint(Point value) { return value; }
"#;

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
            has_core_dependency: false,
        };
        let assembly = Arc::new(
            assemble_program_with_materializer(&plan, None, &path, None, &AssemblyOptions::default(), None, None)
                .unwrap(),
        );
        let mut db = BeskidDatabase::default();
        let project =
            project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "glue-extension-facts".into())
                .unwrap();
        build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
        let unit = SourceUnitId::new(&db, assembly.entry_unit().path.clone());
        Self { _root: root, db, unit, assembly }
    }

    fn named(&self, kind: NodeKind, name: &str) -> AstNodeKey {
        self.assembly
            .entry_syntax_index()
            .ids_of_kind(kind)
            .map(|node| AstNodeKey { unit: self.unit, generation: self.assembly.generation, node })
            .find(|key| item_name(&self.db, *key).ok().flatten().is_some_and(|value| value.as_ref() == name))
            .unwrap_or_else(|| panic!("declaration `{name}` absent"))
    }

    fn method(&self, name: &str) -> AstNodeKey {
        self.named(NodeKind::ContractMethodSignature, name)
    }

    fn binding_error(&self, key: AstNodeKey) -> String {
        glue_binding(&self.db, key).expect_err("Glue binding must be rejected").to_string()
    }
}

fn fixture() -> Fixture {
    Fixture::new(&format!("{TYPES}{CONTRACT}"))
}

#[test]
fn records_enums_and_callbacks_are_admitted_logical_types() {
    let fixture = fixture();
    let db = &fixture.db;
    let point = glue_binding(db, fixture.method("glue_point")).unwrap().unwrap();
    let GlueLogicalType::Record(record) = &point.parameters[0].ty else { panic!("record parameter: {point:?}") };
    assert_eq!(point.result, point.parameters[0].ty);
    assert_eq!(record.library.as_deref(), Some("glue_manual"));
    assert_eq!(record.rust_owner.as_deref(), Some("implementation::Point"));
    assert_eq!(
        record.fields.iter().map(|field| (field.name.as_str(), field.ty)).collect::<Vec<_>>(),
        [
            ("tag", PrimitiveType::U8),
            ("x", PrimitiveType::I64),
            ("weight", PrimitiveType::F64),
            ("visible", PrimitiveType::Bool),
            ("mark", PrimitiveType::Char),
            ("size", PrimitiveType::Word),
        ]
    );
    let color = glue_binding(db, fixture.method("glue_color")).unwrap().unwrap();
    let GlueLogicalType::Enum(enumeration) = &color.result else { panic!("enum result: {color:?}") };
    assert_eq!(&*enumeration.variants, ["Red", "Green", "Blue"]);
    let each = glue_binding(db, fixture.method("glue_each")).unwrap().unwrap();
    let GlueLogicalType::Callback(callback) = &each.parameters[1].ty else { panic!("callback parameter: {each:?}") };
    assert_eq!(callback.parameters[0], GlueLogicalType::Primitive(PrimitiveType::I64));
    assert!(matches!(callback.parameters[1], GlueLogicalType::Enum(_)));
    assert_eq!(*callback.result, GlueLogicalType::Primitive(PrimitiveType::Bool));
    let export = glue_binding(db, fixture.named(NodeKind::FunctionDefinition, "ExportPoint")).unwrap().unwrap();
    assert!(matches!(export.result, GlueLogicalType::Record(_)));
}

/// Every admitted form lowers to exactly one Interop.Contracts shape that the C ABI profile
/// permits, and lifts back to the same logical form (task 2.5).
#[test]
fn every_admitted_form_round_trips_through_the_boundary_vocabulary() {
    let fixture = fixture();
    let db = &fixture.db;
    let mut forms = Vec::new();
    for primitive in [
        PrimitiveType::I8,
        PrimitiveType::I16,
        PrimitiveType::I32,
        PrimitiveType::I64,
        PrimitiveType::U8,
        PrimitiveType::U16,
        PrimitiveType::U32,
        PrimitiveType::U64,
        PrimitiveType::F32,
        PrimitiveType::F64,
        PrimitiveType::Bool,
        PrimitiveType::Char,
        PrimitiveType::Word,
        PrimitiveType::String,
        PrimitiveType::Unit,
    ] {
        forms.push(GlueLogicalType::Primitive(primitive));
    }
    forms.push(GlueLogicalType::Array(Box::new(GlueLogicalType::Primitive(PrimitiveType::U8))));
    for method in ["glue_point", "glue_color", "glue_each", "glue_tick", "glue_handle", "glue_text"] {
        let fact = glue_binding(db, fixture.method(method)).unwrap().unwrap();
        forms.extend(fact.parameters.iter().map(|parameter| parameter.ty.clone()));
        forms.push(fact.result.clone());
        let signature = glue_interop_signature(&fact).unwrap();
        assert_eq!(signature.parameters.len(), fact.parameters.len());
        assert_eq!(signature.symbol, fact.symbol);
    }
    let mut nullable = forms.iter().find_map(|form| match form {
        GlueLogicalType::Handle(brand) => Some(brand.clone()),
        _ => None,
    });
    let handle = nullable.as_mut().expect("handle form");
    handle.nullable = false;
    forms.push(GlueLogicalType::Handle(handle.clone()));
    for form in &forms {
        let shape = glue_type_shape(form).unwrap_or_else(|error| panic!("{form:?} does not lower: {error}"));
        assert!(CAbiProfile.bind(&shape).permitted, "{form:?} lowers to an unpermitted shape {shape:?}");
        assert_eq!(glue_logical_form(&shape), Some(GlueLogicalForm::of(form).unwrap()), "{form:?} does not round trip");
    }
}

#[test]
fn records_take_their_layout_from_the_c_profile() {
    let fixture = fixture();
    let point = glue_binding(&fixture.db, fixture.method("glue_point")).unwrap().unwrap();
    let TypeShape::Record(shape) = glue_type_shape(&point.result).unwrap() else { panic!("record shape") };
    assert_eq!(shape.fields.iter().map(|field| field.offset).collect::<Vec<_>>(), [0, 8, 16, 24, 28, 32]);
    assert_eq!((shape.size, shape.alignment), (40, 8));
    assert_eq!(shape.fields[3].scalar.domain, ScalarDomain::Bool);
    assert_eq!(CAbiProfile.bind(&TypeShape::Record(shape)).view, CProfileView::Record { size: 40, alignment: 8 });
    let text = glue_interop_signature(&glue_binding(&fixture.db, fixture.method("glue_text")).unwrap().unwrap()).unwrap();
    assert_eq!(text.parameters[0].call, CallShapeClass::View);
    assert_eq!(text.returns.ownership, OwnershipClass::Transfer);
}

#[test]
fn unadmitted_record_enum_and_callback_forms_are_rejected() {
    let cases = [
        ("[GlueRecord] pub type Bad { string text, }", "Bad glue_bad(Bad value);", "GlueRecord field `text`"),
        ("[GlueRecord] pub type Bad { }", "Bad glue_bad(Bad value);", "1..=64 fields"),
        ("[GlueRecord] pub type Bad<T> { i64 value, }", "Bad<i64> glue_bad(Bad<i64> value);", "not generic"),
        ("[GlueRecord] pub type Bad { i64 value, }\n[GlueRecord] pub type Outer { i64 v, }", "i64 glue_bad((Bad) => unit visit);", "records are not"),
        ("[GlueEnum] pub enum Bad { None, Some(i64 value), }", "Bad glue_bad(Bad value);", "tagged variants are not admitted"),
        ("pub enum Bad { A, B, }", "Bad glue_bad(Bad value);", "exactly one GlueEnum marker"),
        ("pub type Bad { i64 value, }", "Bad glue_bad(Bad value);", "GlueHandle brand or GlueRecord marker"),
        ("[GlueRecord(Unknown:\"x\")] pub type Bad { i64 value, }", "Bad glue_bad(Bad value);", "unknown Glue metadata"),
        ("", "(i64) => i64 glue_bad(i64 value);", "admitted only as import parameters"),
        ("", "i64 glue_bad((string) => unit visit);", "string is a view"),
        ("", "i64 glue_bad(((i64) => unit) => unit visit);", "admitted only as import parameters"),
        ("", "i64 glue_bad((unit) => unit visit);", "unit is return-only"),
    ];
    for (types, method, expected) in cases {
        let fixture = Fixture::new(&format!(
            "{types}\n[Extern(Abi:\"C\", Library:\"glue_manual\")]\npub contract Foreign {{\n    {method}\n}}\n"
        ));
        let error = fixture.binding_error(fixture.method("glue_bad"));
        assert!(error.contains(expected), "`{method}`: `{error}` does not contain `{expected}`");
    }
    let export = Fixture::new("[Export(Abi:\"C\", Symbol:\"beskid_visit\")]\npub i64 Visit((i64) => i64 visit) { return visit(1); }\n");
    let error = export.binding_error(export.named(NodeKind::FunctionDefinition, "Visit"));
    assert!(error.contains("admitted only as import parameters"), "{error}");
}

#[test]
fn record_and_enum_owner_types_are_closed_placements() {
    let fixture = fixture();
    let declarations = rust_owner_declarations(&fixture.db, fixture.unit).unwrap();
    let placements = declarations.iter().map(|row| (row.placement, &*row.path)).collect::<Vec<_>>();
    assert!(placements.contains(&(RustOwnerPlacement::RecordType, "implementation::Point")), "{placements:?}");
    assert!(placements.contains(&(RustOwnerPlacement::EnumType, "implementation::Color")), "{placements:?}");
    for (source, expected) in [
        (
            "[GlueRecord(Library:\"glue_manual\")]\n[RustOwner(Path:\"implementation::P\", Fallible:true)]\npub type P { i64 x, }\n",
            "Fallible is not allowed on a GlueRecord type",
        ),
        ("[GlueRecord]\n[RustOwner(Path:\"implementation::P\")]\npub type P { i64 x, }\n", "requires GlueRecord Library"),
        ("[GlueEnum(Library:\"glue_manual\")]\n[RustOwner]\npub enum E { A, }\n", "requires Path"),
    ] {
        let fixture = Fixture::new(source);
        let error = rust_owner_declarations(&fixture.db, fixture.unit).expect_err("placement must be rejected").to_string();
        assert!(error.contains(expected), "`{error}` does not contain `{expected}`");
    }
}

#[test]
fn owner_tables_require_owner_types_for_records_and_enums() {
    let fixture = Fixture::new(&format!(
        "[GlueRecord(Library:\"glue_manual\")]\npub type Point {{ i64 x, }}\n{}",
        "[Extern(Abi:\"C\", Library:\"glue_manual\")]\npub contract Foreign {\n    Point glue_point(Point value);\n}\n"
    ));
    let digest = GlueOwnerBindingDigest {
        binding: fixture.method("glue_point"),
        symbol: "glue_point".into(),
        identity_sha256: "1".repeat(64),
        opaque: vec![],
    };
    let error = rust_owner_tables(&fixture.db, "glue_manual", &[digest]).expect_err("record owner type required").to_string();
    assert!(error.contains("GlueRecord type of a Rust glue owner requires RustOwner"), "{error}");
}
