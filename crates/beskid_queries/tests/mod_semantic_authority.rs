//! Mod shape access must consume the registered production assembly and canonical facts.
use beskid_analysis::mod_host::{
    ModSemanticAuthority, ModSemanticField, ModSemanticFieldType, ModSemanticOwnership, ModSemanticShape,
    ModSemanticShapeBody,
};
use beskid_analysis::projects::{
    AssemblyOptions, CompilePlan, ProgramAssembly, Target, TargetKind, assemble_program_with_materializer,
};
use beskid_analysis::syntax::PrimitiveType;
use beskid_analysis::syntax_query::NodeKind;
use beskid_queries::{
    AstNodeKey, BeskidDatabase, ModSemanticQueryAuthority, ProjectSession, SourceUnitId, SyntaxGenerationId,
    build_typed_program, project_session_for_planned_syntax_assembly,
};
use std::sync::Arc;

struct Fixture {
    _root: tempfile::TempDir,
    db: BeskidDatabase,
    project: ProjectSession,
    assembly: Arc<ProgramAssembly>,
}
impl Fixture {
    fn new() -> Self {
        Self::with_proof(false)
    }
    fn with_proof(verified: bool) -> Self {
        Self::with_source(
            verified,
            r#"
pub type Inner { pub string Label, }
pub type Configuration { pub i32 Count, pub Inner Child, pub bool Enabled, pub f64 Ratio, }
pub type Box<T> { pub T Value, }
pub type Applied { pub Box<i32> Item, }
pub enum Node { Empty, Branch(string Label, Node[] Children), }
pub enum Choice<T> { None, Some(T Value), }
pub type EnumApplied { pub Choice<i32> Item, }
pub unit Main() { return; }
"#,
        )
    }
    fn with_source(verified: bool, source: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let source_root = root.path().join("Src");
        std::fs::create_dir(&source_root).unwrap();
        let path = source_root.join("Main.bd");
        std::fs::write(&path, source).unwrap();
        let plan = CompilePlan {
            project_root: root.path().to_owned(),
            manifest_path: root.path().join("Host.bproj"),
            project_name: "Host".into(),
            source_root,
            target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
            dependency_projects: vec![],
            unresolved_dependencies: vec![],
            has_std_dependency: false,
        };
        let workspace = if verified {
            std::fs::write(&plan.manifest_path, "Host { name = \"Host\" version = \"1.0.0\" root = \"Src\" }\ntarget \"Main\" { kind = \"Lib\" entry = \"Main.bd\" }\n").unwrap();
            Some(beskid_analysis::projects::prepare_project_workspace(&plan).unwrap())
        } else {
            None
        };
        let assembly = Arc::new(
            assemble_program_with_materializer(
                &plan,
                workspace.as_ref(),
                &path,
                None,
                &AssemblyOptions::default(),
                None,
                None,
            )
            .unwrap(),
        );
        let mut db = BeskidDatabase::default();
        let project =
            project_session_for_planned_syntax_assembly(&mut db, &assembly, &plan, &path, "mod-shape-test".into())
                .unwrap();
        build_typed_program(&mut db, project, assembly.generation, assembly.clone()).unwrap();
        Self { _root: root, db, project, assembly }
    }
    fn key(&self, kind: NodeKind, occurrence: usize) -> AstNodeKey {
        AstNodeKey {
            unit: SourceUnitId::new(&self.db, self.assembly.entry_unit().path.clone()),
            generation: self.assembly.generation,
            node: self.assembly.entry_syntax_index().ids_of_kind(kind).nth(occurrence).unwrap(),
        }
    }
}

#[test]
fn mod_shape_authority_preserves_primitive_nominal_and_gc_field_facts() {
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    assert_eq!(authority.generation(), fixture.assembly.generation);
    let handle = authority.resolve_type(fixture.key(NodeKind::TypeDefinition, 1)).unwrap();
    let shape = authority.type_shape(handle).unwrap();
    assert_eq!(shape.name, "Configuration");
    assert_eq!(shape.declaration.source_unit, fixture.assembly.entry_unit().path);
    assert_eq!(shape.declaration.generation, fixture.assembly.generation);
    assert_eq!(
        record_fields(&shape).iter().map(|field| field.name.as_str()).collect::<Vec<_>>(),
        ["Count", "Child", "Enabled", "Ratio"]
    );
    assert_eq!(record_fields(&shape)[0].ty, ModSemanticFieldType::Scalar(PrimitiveType::I32));
    assert_eq!(record_fields(&shape)[0].ownership, ModSemanticOwnership::NativeOrScalar);
    assert_eq!(record_fields(&shape)[2].ty, ModSemanticFieldType::Scalar(PrimitiveType::Bool));
    assert_eq!(record_fields(&shape)[3].ty, ModSemanticFieldType::Scalar(PrimitiveType::F64));
    let ModSemanticFieldType::Nominal(child) = record_fields(&shape)[1].ty else {
        panic!("nested nominal identity required")
    };
    assert_eq!(record_fields(&shape)[1].ownership, ModSemanticOwnership::GcManaged);
    let child = authority.type_shape(child).unwrap();
    assert_eq!(child.name, "Inner");
    assert_eq!(record_fields(&child)[0].ty, ModSemanticFieldType::Scalar(PrimitiveType::String));
    assert_eq!(record_fields(&child)[0].ownership, ModSemanticOwnership::GcManaged);
}

#[test]
fn mod_shape_authority_preserves_applied_generic_identity_and_substitution() {
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let applied =
        authority.type_shape(authority.resolve_type(fixture.key(NodeKind::TypeDefinition, 3)).unwrap()).unwrap();
    let ModSemanticFieldType::Nominal(boxed) = record_fields(&applied)[0].ty else {
        panic!("applied nominal identity required")
    };
    let boxed = authority.type_shape(boxed).unwrap();
    assert_eq!(boxed.name, "Box");
    assert_eq!(boxed.type_arguments, [ModSemanticFieldType::Scalar(PrimitiveType::I32)]);
    assert_eq!(record_fields(&boxed)[0].name, "Value");
    assert_eq!(record_fields(&boxed)[0].ty, ModSemanticFieldType::Scalar(PrimitiveType::I32));
    assert_eq!(record_fields(&boxed)[0].ownership, ModSemanticOwnership::NativeOrScalar);
    assert!(
        authority.type_shape(authority.resolve_type(fixture.key(NodeKind::TypeDefinition, 2)).unwrap()).is_err(),
        "unapplied generic cannot invent a field type"
    );
}

#[test]
fn mod_shape_authority_rejects_stale_foreign_and_non_type_keys_and_tokens() {
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let current = fixture.key(NodeKind::TypeDefinition, 1);
    let stale = AstNodeKey { generation: SyntaxGenerationId(fixture.assembly.generation.0 + 1), ..current };
    assert!(authority.resolve_type(stale).is_err());
    assert!(authority.resolve_type(fixture.key(NodeKind::FunctionDefinition, 0)).is_err());
    let unknown =
        AstNodeKey { unit: SourceUnitId::new(&fixture.db, fixture._root.path().join("Unregistered.bd")), ..current };
    assert!(authority.resolve_type(unknown).is_err());
    let foreign = ProjectSession::new(
        &fixture.db,
        fixture._root.path().join("Foreign"),
        fixture.assembly.entry_unit().path.clone(),
        "Foreign".into(),
        "foreign".into(),
    );
    assert!(ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, foreign, &fixture.assembly).is_err());
    let other_issuer =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let token = authority.resolve_type(current).unwrap();
    assert!(
        other_issuer.type_shape(token).is_err(),
        "semantic handle issuer must be checked independently of matching generation"
    );
}

fn record_fields(shape: &ModSemanticShape) -> &[ModSemanticField] {
    match &shape.body {
        ModSemanticShapeBody::Record { fields } => fields,
        _ => panic!("record body required"),
    }
}

#[test]
fn mod_shape_enum_retains_ordered_unit_and_recursive_payload_declarations() {
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let key = fixture.key(NodeKind::EnumDefinition, 0);
    let handle = authority.resolve_type(key).unwrap();
    let shape = authority.type_shape(handle).unwrap();
    let ModSemanticShapeBody::Enum { variants } = shape.body else { panic!("enum body required") };
    assert_eq!(variants.iter().map(|v| v.name.as_str()).collect::<Vec<_>>(), ["Empty", "Branch"]);
    assert_eq!(variants.iter().map(|v| v.ordinal).collect::<Vec<_>>(), [0, 1]);
    assert!(variants[0].fields.is_empty());
    assert_eq!(variants[1].fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Label", "Children"]);
    assert_eq!(variants[1].fields[0].ty, ModSemanticFieldType::Scalar(PrimitiveType::String));
    assert_eq!(variants[1].fields[0].ownership, ModSemanticOwnership::GcManaged);
    let ModSemanticFieldType::Array(element) = &variants[1].fields[1].ty else { panic!("array identity required") };
    assert_eq!(
        element.as_ref(),
        &ModSemanticFieldType::Nominal(handle),
        "recursive handle is interned, never expanded indefinitely"
    );
    let source = &fixture.assembly.entry_unit().source;
    for variant in &variants {
        assert_eq!(variant.declaration.generation, fixture.assembly.generation);
        assert!(source[variant.declaration.span.start..variant.declaration.span.end].contains(&variant.name));
        for field in &variant.fields {
            assert_eq!(field.declaration.source_unit, fixture.assembly.entry_unit().path);
            assert!(source[field.declaration.span.start..field.declaration.span.end].contains(&field.name));
            assert_eq!(fixture.assembly.entry_syntax_index().kind(field.declaration.node), Some(NodeKind::Field));
        }
    }
}

#[test]
fn mod_shape_applied_enum_substitutes_payload_and_rejects_unapplied_generic() {
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let applied =
        authority.type_shape(authority.resolve_type(fixture.key(NodeKind::TypeDefinition, 4)).unwrap()).unwrap();
    let ModSemanticFieldType::Nominal(choice) = record_fields(&applied)[0].ty else {
        panic!("applied enum nominal handle required")
    };
    let shape = authority.type_shape(choice).unwrap();
    assert_eq!(shape.type_arguments, [ModSemanticFieldType::Scalar(PrimitiveType::I32)]);
    let ModSemanticShapeBody::Enum { variants } = shape.body else { panic!("applied enum body required") };
    assert!(variants[0].fields.is_empty());
    assert_eq!(variants[1].fields[0].ty, ModSemanticFieldType::Scalar(PrimitiveType::I32));
    assert_eq!(variants[1].fields[0].ownership, ModSemanticOwnership::NativeOrScalar);
    assert!(
        authority.type_shape(authority.resolve_type(fixture.key(NodeKind::EnumDefinition, 1)).unwrap()).is_err(),
        "unapplied enum cannot invent generic payload"
    );
    let stale = AstNodeKey {
        generation: SyntaxGenerationId(fixture.assembly.generation.0 + 1),
        ..fixture.key(NodeKind::EnumDefinition, 0)
    };
    assert!(authority.resolve_type(stale).is_err());
}

#[test]
fn mod_shape_stable_package_declaration_ignores_checkout_and_synthetic_is_ineligible() {
    let left = Fixture::with_proof(true);
    let right = Fixture::with_proof(true);
    let a = ModSemanticQueryAuthority::for_registered_assembly(&left.db, left.project, &left.assembly).unwrap();
    let b = ModSemanticQueryAuthority::for_registered_assembly(&right.db, right.project, &right.assembly).unwrap();
    let x = a.type_shape(a.resolve_type(left.key(NodeKind::EnumDefinition, 0)).unwrap()).unwrap();
    let y = b.type_shape(b.resolve_type(right.key(NodeKind::EnumDefinition, 0)).unwrap()).unwrap();
    assert_eq!(x.package, y.package);
    assert_eq!(x.package_declaration, y.package_declaration);
    let declaration = x.package_declaration.unwrap();
    assert_eq!(declaration.source_path, "Main.bd");
    assert_eq!(declaration.lexical_path, ["Node"]);
    let synthetic = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&synthetic.db, synthetic.project, &synthetic.assembly)
            .unwrap();
    let shape =
        authority.type_shape(authority.resolve_type(synthetic.key(NodeKind::EnumDefinition, 0)).unwrap()).unwrap();
    assert!(shape.package.is_none());
    assert!(shape.package_declaration.is_none());
}

#[test]
fn native_callback_declarations_require_current_source_node_and_exact_span() {
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let handle = authority.resolve_type(fixture.key(NodeKind::TypeDefinition, 1)).unwrap();
    let shape = authority.type_shape(handle).unwrap();
    assert_eq!(authority.resolve_declaration(&shape.declaration).unwrap(), handle);
    let mut foreign = shape.declaration.clone();
    foreign.source_unit = std::path::PathBuf::from("foreign.bd");
    assert!(authority.resolve_declaration(&foreign).is_err());
    let mut stale = shape.declaration.clone();
    stale.generation = SyntaxGenerationId(stale.generation.0 + 1);
    assert!(authority.resolve_declaration(&stale).is_err());
    let mut wrong_span = shape.declaration.clone();
    wrong_span.span.start += 1;
    assert!(authority.resolve_declaration(&wrong_span).is_err());
}

#[test]
fn syntax_queries_use_issued_registered_nodes_and_owned_projections() {
    use beskid_analysis::mod_host::{ModSyntaxAuthority, ModSyntaxBounds, ModSyntaxRequest, ModSyntaxResponse};
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let issuer = 711;
    let ModSyntaxResponse::Node(Some(root)) = authority.syntax_query(issuer, &ModSyntaxRequest::Root).unwrap() else {
        panic!("root missing")
    };
    let mut invented = root.clone();
    invented.node = beskid_queries::AstNodeId(1);
    assert!(authority.syntax_query(issuer, &ModSyntaxRequest::Span { node: invented }).is_err());
    let bounds = ModSyntaxBounds { max_nodes: 65536, max_depth: 128 };
    let ModSyntaxResponse::Nodes(nodes) =
        authority.syntax_query(issuer, &ModSyntaxRequest::Descendants { node: root.clone(), bounds }).unwrap()
    else {
        panic!("descendants missing")
    };
    assert!(!nodes.is_empty());
    assert!(nodes.iter().all(|node| node.source_unit == root.source_unit
        && node.generation == root.generation
        && node.invocation_issuer == issuer));
    assert!(
        authority
            .syntax_query(
                issuer,
                &ModSyntaxRequest::Descendants {
                    node: root.clone(),
                    bounds: ModSyntaxBounds { max_nodes: 1, max_depth: 128 }
                }
            )
            .is_err()
    );
    let ModSyntaxResponse::Node(Some(declaration)) = authority
        .syntax_query(
            issuer,
            &ModSyntaxRequest::FindFirst { node: root.clone(), bounds, kind: NodeKind::TypeDefinition },
        )
        .unwrap()
    else {
        panic!("type declaration missing")
    };
    let ModSyntaxResponse::Projection { kind, value: Some(value) } = authority
        .syntax_query(issuer, &ModSyntaxRequest::Project { node: declaration.clone(), kind: NodeKind::TypeDefinition })
        .unwrap()
    else {
        panic!("owned type projection missing")
    };
    assert_eq!(kind, NodeKind::TypeDefinition);
    assert!(value.is_object());
    let ModSyntaxResponse::Projection { value: None, .. } = authority
        .syntax_query(
            issuer,
            &ModSyntaxRequest::Project { node: declaration.clone(), kind: NodeKind::FunctionDefinition },
        )
        .unwrap()
    else {
        panic!("foreign projection kind accepted")
    };
    let mut foreign = declaration.clone();
    foreign.invocation_issuer += 1;
    assert!(authority.syntax_query(issuer, &ModSyntaxRequest::Span { node: foreign }).is_err());
    let mut stale = declaration;
    stale.generation = SyntaxGenerationId(stale.generation.0 + 1);
    assert!(authority.syntax_query(issuer, &ModSyntaxRequest::Span { node: stale }).is_err());
    assert!(authority.take_applied_syntax(issuer).unwrap().is_empty());
    assert!(authority.syntax_query(issuer, &ModSyntaxRequest::Root).is_err(), "closed issuer must not reopen");
    assert!(authority.syntax_query(issuer, &ModSyntaxRequest::Span { node: root }).is_err());
}

#[test]
fn typed_pipeline_retains_owned_program_and_rejects_conflicts() {
    use beskid_analysis::mod_host::{ModSyntaxAuthority, ModSyntaxBounds, ModSyntaxRequest, ModSyntaxResponse};
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let issuer = 712;
    let ModSyntaxResponse::Node(Some(root)) = authority.syntax_query(issuer, &ModSyntaxRequest::Root).unwrap() else {
        panic!("root missing")
    };
    let ModSyntaxResponse::Nodes(children) =
        authority.syntax_query(issuer, &ModSyntaxRequest::Children { node: root.clone() }).unwrap()
    else {
        panic!("children missing")
    };
    let target = children.first().unwrap().clone();
    let bounds = ModSyntaxBounds { max_nodes: 65536, max_depth: 128 };
    let remove = ModSyntaxRequest::Remove { root: root.clone(), target, bounds };
    authority.syntax_query(issuer, &remove).unwrap();
    assert!(authority.syntax_query(issuer, &remove).is_err());
    authority.syntax_query(issuer, &ModSyntaxRequest::Apply { root: root.clone(), bounds }).unwrap();
    assert!(authority.syntax_query(issuer, &ModSyntaxRequest::Apply { root, bounds }).is_err());
    let applied = authority.take_applied_syntax(issuer).unwrap();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].previous_generation, fixture.assembly.generation);
    assert_eq!(applied[0].source_unit, fixture.assembly.entry_unit().path);
    assert_eq!(applied[0].program.node.items.len() + 1, fixture.assembly.entry_unit().program.node.items.len());
    assert_eq!(applied[0].program.node.leading_docs.len(), applied[0].program.node.items.len());
    assert!(authority.take_applied_syntax(issuer).is_err());
}

#[test]
fn typed_rewriter_accepts_new_owned_declaration_and_rejects_invalid_projection() {
    use beskid_analysis::mod_host::{ModSyntaxAuthority, ModSyntaxBounds, ModSyntaxRequest, ModSyntaxResponse};
    use beskid_analysis::syntax::Node;
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let issuer = 713;
    let bounds = ModSyntaxBounds { max_nodes: 65536, max_depth: 128 };
    let ModSyntaxResponse::Node(Some(root)) = authority.syntax_query(issuer, &ModSyntaxRequest::Root).unwrap() else {
        panic!("root")
    };
    let ModSyntaxResponse::Node(Some(target)) = authority
        .syntax_query(
            issuer,
            &ModSyntaxRequest::FindFirst { node: root.clone(), bounds, kind: NodeKind::TypeDefinition },
        )
        .unwrap()
    else {
        panic!("declaration")
    };
    let replacement = beskid_analysis::services::parse_program("pub type Rewritten { pub bool Ready, }").unwrap();
    let Node::TypeDefinition(declaration) = &replacement.node.items[0].node else { panic!("typed declaration") };
    let value = serde_json::to_value(&declaration.node).unwrap();
    let request = |target, kind, value, bounds| ModSyntaxRequest::ReplaceProjection {
        root: root.clone(),
        target,
        kind,
        value,
        bounds,
    };
    let mut foreign = target.clone();
    foreign.invocation_issuer += 1;
    assert!(
        authority.syntax_query(issuer, &request(foreign, NodeKind::TypeDefinition, value.clone(), bounds)).is_err()
    );
    assert!(
        authority
            .syntax_query(issuer, &request(target.clone(), NodeKind::FunctionDefinition, value.clone(), bounds))
            .is_err()
    );
    assert!(
        authority
            .syntax_query(issuer, &request(target.clone(), NodeKind::TypeDefinition, serde_json::json!({}), bounds))
            .is_err()
    );
    assert!(
        authority
            .syntax_query(
                issuer,
                &request(
                    target.clone(),
                    NodeKind::TypeDefinition,
                    value.clone(),
                    ModSyntaxBounds { max_nodes: 1, max_depth: 128 }
                )
            )
            .is_err()
    );
    let mut unknown = value.clone();
    unknown.as_object_mut().unwrap().insert("forgedAuthority".into(), serde_json::json!(true));
    assert!(
        authority.syntax_query(issuer, &request(target.clone(), NodeKind::TypeDefinition, unknown, bounds)).is_err()
    );
    authority.syntax_query(issuer, &request(target, NodeKind::TypeDefinition, value, bounds)).unwrap();
    authority.syntax_query(issuer, &ModSyntaxRequest::Apply { root, bounds }).unwrap();
    let applied = authority.take_applied_syntax(issuer).unwrap();
    let Node::TypeDefinition(actual) = &applied[0].program.node.items[0].node else { panic!("replacement") };
    assert_eq!(actual.node.name.node.name, "Rewritten");
    assert_eq!(
        actual.span,
        match &fixture.assembly.entry_unit().program.node.items[0].node {
            Node::TypeDefinition(original) => original.span,
            _ => panic!("original"),
        }
    );
    assert_eq!(applied[0].program.node.items[0].span, fixture.assembly.entry_unit().program.node.items[0].span);
    assert_eq!(applied[0].program.node.leading_docs, fixture.assembly.entry_unit().program.node.leading_docs);
    // The ProgramItem slot is a tagged union: a generic Rewriter<TSource,
    // TTarget> may change declaration kind when that slot admits the target.
    let issuer = 714;
    let ModSyntaxResponse::Node(Some(root)) = authority.syntax_query(issuer, &ModSyntaxRequest::Root).unwrap() else {
        panic!("root")
    };
    let ModSyntaxResponse::Node(Some(target)) = authority
        .syntax_query(
            issuer,
            &ModSyntaxRequest::FindFirst { node: root.clone(), bounds, kind: NodeKind::TypeDefinition },
        )
        .unwrap()
    else {
        panic!("target")
    };
    let function = beskid_analysis::services::parse_program("pub unit RewrittenFunction() { return; }").unwrap();
    let Node::Function(function) = &function.node.items[0].node else { panic!("function") };
    authority
        .syntax_query(
            issuer,
            &ModSyntaxRequest::ReplaceProjection {
                root: root.clone(),
                target,
                kind: NodeKind::FunctionDefinition,
                value: serde_json::to_value(&function.node).unwrap(),
                bounds,
            },
        )
        .unwrap();
    authority.syntax_query(issuer, &ModSyntaxRequest::Apply { root, bounds }).unwrap();
    let applied = authority.take_applied_syntax(issuer).unwrap();
    let Node::Function(function) = &applied[0].program.node.items[0].node else { panic!("cross-kind replacement") };
    assert_eq!(function.node.name.node.name, "RewrittenFunction");
}

#[test]
fn catchall_requires_exact_owned_map_field_and_live_invocation() {
    use beskid_analysis::mod_host::ModSyntaxAuthority;
    let fixture = Fixture::with_proof(true);
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let owner = authority.resolve_type(fixture.key(NodeKind::TypeDefinition, 1)).unwrap();
    let shape = authority.type_shape(owner).unwrap();
    let ModSemanticShapeBody::Record { fields } = shape.body else {
        panic!("record fixture");
    };
    // A prepared package identity does not turn a numeric field into Map<string,V>.
    assert!(authority.issue_catchall(9001, owner, &fields[0].declaration).is_err());
    let mut foreign = fields[0].declaration.clone();
    foreign.source_unit = std::path::PathBuf::from("outside.bd");
    assert!(authority.issue_catchall(9001, owner, &foreign).is_err());
    assert!(authority.lookup_catchall(9001, 123).is_err());
    let forged = beskid_analysis::mod_host::ModSemanticCatchallClaim {
        token: 123,
        owner,
        field: fields[0].declaration.clone(),
        map: owner,
        value: fields[0].ty.clone(),
    };
    assert!(authority.compile_catchall(9001, &forged).is_err());
    let carrier = beskid_analysis::mod_host::ModCompiledMetadata::with_issuer_payload(forged.clone(), ());
    assert!(carrier.issuer_payload::<beskid_queries::CompiledCatchallBinding>().is_none());

    authority.take_applied_syntax(9001).unwrap();
    assert!(authority.issue_catchall(9001, owner, &fields[0].declaration).is_err());
    assert!(authority.lookup_catchall(9001, 123).is_err());
    assert!(authority.compile_catchall(9001, &forged).is_err());
}

#[test]
fn synthetic_shape_without_package_proof_cannot_issue_catchall() {
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let owner = authority.resolve_type(fixture.key(NodeKind::TypeDefinition, 1)).unwrap();
    let shape = authority.type_shape(owner).unwrap();
    let ModSemanticShapeBody::Record { fields } = shape.body else {
        panic!("record fixture");
    };
    let error = authority.catchall_field(owner, &fields[0].declaration).unwrap_err();
    assert!(error.to_string().contains("prepare-owned package identity"));
}

#[test]
fn portable_nominal_identity_uses_verified_package_relative_source_across_copies() {
    let mut first = Fixture::with_proof(true);
    let mut second = Fixture::with_proof(true);
    let first_key = first.key(NodeKind::TypeDefinition, 1);
    let second_key = second.key(NodeKind::TypeDefinition, 1);
    let first_program =
        build_typed_program(&mut first.db, first.project, first.assembly.generation, first.assembly.clone()).unwrap();
    let second_program =
        build_typed_program(&mut second.db, second.project, second.assembly.generation, second.assembly.clone())
            .unwrap();
    let a = beskid_queries::portable_nominal_identity(&first.db, &first_program, first_key).unwrap();
    let b = beskid_queries::portable_nominal_identity(&second.db, &second_program, second_key).unwrap();
    assert_ne!(first.assembly.entry_unit().path, second.assembly.entry_unit().path);
    assert_eq!(a, b);
    assert_eq!(a.package().package_name(), "Host");
    assert_eq!(a.package().version(), "1.0.0");
    assert_eq!(a.declaration().source_path, "Main.bd");
    assert_eq!(a.declaration().lexical_path, ["Configuration"]);
}

#[test]
fn portable_nominal_identity_rejects_synthetic_foreign_and_stale_keys() {
    let mut synthetic = Fixture::new();
    let key = synthetic.key(NodeKind::TypeDefinition, 1);
    let program = build_typed_program(
        &mut synthetic.db,
        synthetic.project,
        synthetic.assembly.generation,
        synthetic.assembly.clone(),
    )
    .unwrap();
    assert!(beskid_queries::portable_nominal_identity(&synthetic.db, &program, key).is_err());
    let mut verified = Fixture::with_proof(true);
    let verified_key = verified.key(NodeKind::TypeDefinition, 1);
    let typed = build_typed_program(
        &mut verified.db,
        verified.project,
        verified.assembly.generation,
        verified.assembly.clone(),
    )
    .unwrap();
    assert!(beskid_queries::portable_nominal_identity(&verified.db, &typed, key).is_err());
    let stale = AstNodeKey { generation: synthetic.assembly.generation, ..verified_key };
    assert!(beskid_queries::portable_nominal_identity(&verified.db, &typed, stale).is_err());
}

#[test]
fn canonical_path_planning_rejects_unregistered_sources_unknown_and_foreign_requests() {
    use beskid_analysis::mod_host::{ModSyntaxAuthority, ModSyntaxRequest, ModSyntaxResponse};
    let fixture = Fixture::new();
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let issuer = 872;
    let ModSyntaxResponse::Node(Some(root)) = authority.syntax_query(issuer, &ModSyntaxRequest::Root).unwrap() else {
        panic!("root missing")
    };
    assert!(
        authority.plan_canonical_paths(issuer, &root, &["Core.Results.Result".into()]).is_err(),
        "synthetic source cannot grant canonical routes"
    );
    assert!(authority.plan_canonical_paths(issuer, &root, &["User.Results.Result".into()]).is_err());
    assert!(authority.plan_canonical_paths(issuer + 1, &root, &[]).is_err());
    assert!(authority.plan_canonical_paths(issuer, &root, &[]).unwrap().is_empty());
    let mut stale = root.clone();
    stale.invocation_issuer = issuer + 1;
    assert!(authority.plan_canonical_paths(issuer, &stale, &[]).is_err());
}

/// Policy, template and descriptor fixtures. Field policy is the Mod's
/// `SerializeField` attribute; the host reads it with the Mod's literal rules.
const POLICY_SOURCE: &str = r#"
pub type Loop<T> { pub T Value, pub Loop<T> Next, }
pub type SkippedLoop<T> { pub T Value, [SerializeField(SkipSerialize: true, SkipDeserialize: true, Default: "MakeSkipped")] pub SkippedLoop<T> Next, }
pub enum Tree<T> { Leaf(T Value), Branch(Tree<T> Left, Tree<T> Right), }
pub enum Packet { Data(i32 Kept, [SerializeField(SkipSerialize: true, SkipDeserialize: true, Default: "MakeCache")] string Cache), Other(i32 Value), }
pub enum Halved { Broken([SerializeField(SkipSerialize: true)] i32 Half), }
pub type Policies { [SerializeField(Name: "renamed")] pub i32 Plain, [SerializeField(SkipSerialize: true)] pub i32 DecodeOnly, [SerializeField(SkipDeserialize: true, Default: "MakeCount")] pub i32 EncodeOnly, [SerializeField(WordWidth: 32)] pub word[] Widths, [SerializeField(Bytes: true)] pub u8[] Blob, }
pub type RawHolder { pub i32 Kept, [SerializeField(SkipSerialize: true, SkipDeserialize: true, Default: "MakeRaw")] pub pointer Raw, }
pub type RawLeak { pub pointer Raw, }
pub type Cached<T> { pub T Value, [SerializeField(SkipSerialize: true, SkipDeserialize: true, Default: "MakeCache")] pub string Cache, }
pub type Counted<T> { pub T Value, [SerializeField(SkipDeserialize: true, Default: "MakeCount")] pub T Count, }
pub type Box<T> { pub T Value, }
pub type Uses { pub Loop<i32> A, pub SkippedLoop<i32> B, pub Tree<string> C, pub Box<bool> D, pub Cached<i32> E, pub Counted<i32> F, pub Counted<string> G, }
pub string MakeCache() { return ""; }
pub i32 MakeCount() { return 0_i32; }
pub unit Main() { return; }
"#;

fn field_key(fixture: &Fixture, field: &ModSemanticField) -> AstNodeKey {
    AstNodeKey {
        unit: SourceUnitId::new(&fixture.db, field.declaration.source_unit.clone()),
        generation: field.declaration.generation,
        node: field.declaration.node,
    }
}

fn named(fixture: &Fixture, authority: &ModSemanticQueryAuthority<'_>, kind: NodeKind, name: &str) -> ModSemanticShape {
    (0..)
        .map_while(|occurrence| {
            fixture.assembly.entry_syntax_index().ids_of_kind(kind).nth(occurrence).map(|_| fixture.key(kind, occurrence))
        })
        .filter_map(|key| authority.type_shape(authority.resolve_type(key).ok()?).ok())
        .find(|shape| shape.name == name)
        .unwrap_or_else(|| panic!("{name} fixture"))
}

fn handle_named(
    fixture: &Fixture,
    authority: &ModSemanticQueryAuthority<'_>,
    kind: NodeKind,
    name: &str,
) -> beskid_analysis::mod_host::ModSemanticHandle {
    (0..)
        .map_while(|occurrence| {
            fixture.assembly.entry_syntax_index().ids_of_kind(kind).nth(occurrence).map(|_| fixture.key(kind, occurrence))
        })
        .filter_map(|key| authority.resolve_type(key).ok())
        .find(|handle| authority.type_shape(*handle).is_ok_and(|shape| shape.name == name))
        .unwrap_or_else(|| panic!("{name} fixture"))
}

#[test]
fn payload_wire_policy_omits_only_fields_skipped_in_both_directions() {
    let fixture = Fixture::with_source(true, POLICY_SOURCE);
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let packet = named(&fixture, &authority, NodeKind::EnumDefinition, "Packet");
    let ModSemanticShapeBody::Enum { variants } = packet.body else { panic!("enum fixture") };
    let data = &variants[0].fields;
    let kept = beskid_queries::serialization_payload_field_policy(&fixture.db, field_key(&fixture, &data[0])).unwrap();
    assert!(!kept.absent_from_wire());
    let cache = beskid_queries::serialization_payload_field_policy(&fixture.db, field_key(&fixture, &data[1])).unwrap();
    assert!(cache.skip_read() && cache.skip_write() && cache.absent_from_wire());
    assert_eq!(cache.default_factory(), Some(&["MakeCache".to_owned()][..]));
    // An asymmetric payload skip cannot be described positionally and fails closed.
    let halved = named(&fixture, &authority, NodeKind::EnumDefinition, "Halved");
    let ModSemanticShapeBody::Enum { variants: halved } = halved.body else { panic!("enum fixture") };
    let half = field_key(&fixture, &halved[0].fields[0]);
    assert!(beskid_queries::serialization_payload_field_policy(&fixture.db, half).is_err());
    let halved = handle_named(&fixture, &authority, NodeKind::EnumDefinition, "Halved");
    assert!(authority.serialization_shape(halved).is_err(), "an asymmetric payload skip has no wire shape");
    // A non-field key is not a policy owner.
    assert!(beskid_queries::serialization_field_policy(&fixture.db, fixture.key(NodeKind::EnumDefinition, 1)).is_err());
    // The wire graph of the payload enum omits the symmetric skip.
    let handle = handle_named(&fixture, &authority, NodeKind::EnumDefinition, "Packet");
    let graph = authority.serialization_shape(handle).unwrap();
    let beskid_queries::DynamicPackingNode::Enum { variants, .. } = &graph.nodes()[0] else { panic!("enum graph") };
    assert_eq!(variants[0].fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>(), ["Kept"]);
    assert_eq!(variants[1].fields.len(), 1);
}

#[test]
fn record_policy_reads_wire_name_direction_requiredness_width_and_bytes() {
    let fixture = Fixture::with_source(true, POLICY_SOURCE);
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let shape = named(&fixture, &authority, NodeKind::TypeDefinition, "Policies");
    let fields = record_fields(&shape);
    let policy = |index: usize| beskid_queries::serialization_field_policy(&fixture.db, field_key(&fixture, &fields[index])).unwrap();
    assert_eq!(policy(0).wire_name(), "renamed");
    assert!(policy(0).required_in_both_directions());
    // Decode-only and encode-only fields are never required in both directions.
    assert!(policy(1).skip_write() && !policy(1).required_in_both_directions() && !policy(1).absent_from_wire());
    assert!(policy(2).skip_read() && !policy(2).required_in_both_directions() && !policy(2).absent_from_wire());
    assert_eq!(policy(2).wire_name(), "EncodeOnly");
    assert_eq!(policy(3).word_width(), Some(32));
    assert!(policy(4).bytes());
    // The word wire width rewrites the field's word scalars to the fixed-width tag.
    let handle = handle_named(&fixture, &authority, NodeKind::TypeDefinition, "Policies");
    let graph = authority.serialization_shape(handle).unwrap();
    let beskid_queries::DynamicPackingNode::Record { fields: wire, .. } = &graph.nodes()[0] else { panic!("record graph") };
    let widths = wire.iter().find(|field| field.name == "Widths").unwrap();
    let rewritten = graph.with_word_width(widths.ty, 32).unwrap();
    let beskid_queries::DynamicPackingNode::Array { element } = &rewritten.nodes()[0] else { panic!("array root") };
    assert!(matches!(rewritten.nodes()[*element as usize], beskid_queries::DynamicPackingNode::Scalar { name: "u32", .. }));
    assert!(graph.with_word_width(widths.ty, 12).is_err());
}

#[test]
fn serialization_shape_exempts_target_fields_skipped_in_both_directions() {
    let fixture = Fixture::with_source(true, POLICY_SOURCE);
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let holder = handle_named(&fixture, &authority, NodeKind::TypeDefinition, "RawHolder");
    let graph = authority.serialization_shape(holder).unwrap();
    let beskid_queries::DynamicPackingNode::Record { fields, .. } = &graph.nodes()[0] else { panic!("record graph") };
    assert_eq!(fields.iter().map(|field| field.name.as_str()).collect::<Vec<_>>(), ["Kept"]);
    let leak = handle_named(&fixture, &authority, NodeKind::TypeDefinition, "RawLeak");
    assert!(authority.serialization_shape(leak).is_err(), "a serialized pointer field stays ineligible");
}

#[test]
fn specialized_template_runs_concrete_recursion_gate_with_skip_exemption() {
    let fixture = Fixture::with_source(true, POLICY_SOURCE);
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let uses = named(&fixture, &authority, NodeKind::TypeDefinition, "Uses");
    let applied = |index: usize| {
        let ModSemanticFieldType::Nominal(handle) = record_fields(&uses)[index].ty else {
            panic!("applied template field")
        };
        handle
    };
    let direct = authority.serialization_template_shape(applied(0)).unwrap_err();
    assert!(direct.to_string().contains("SerializationRecursiveCycle"), "{direct}");
    // The recursive field is skipped in both directions, so it is not serialized data.
    if let Err(error) = authority.serialization_template_shape(applied(1)) {
        assert!(!error.to_string().contains("SerializationRecursiveCycle"), "{error}");
    }
    // Enum payload recursion is indirection, as for concrete targets.
    authority.serialization_template_shape(applied(2)).unwrap();
    authority.serialization_template_shape(applied(3)).unwrap();
    // A concrete declaration is not a template application.
    let concrete = handle_named(&fixture, &authority, NodeKind::TypeDefinition, "Uses");
    assert!(authority.serialization_template_shape(concrete).is_err());
}

#[test]
fn specialized_template_proves_default_factories_against_substituted_field_types() {
    let fixture = Fixture::with_source(true, POLICY_SOURCE);
    let authority =
        ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let uses = named(&fixture, &authority, NodeKind::TypeDefinition, "Uses");
    let applied = |index: usize| {
        let ModSemanticFieldType::Nominal(handle) = record_fields(&uses)[index].ty else {
            panic!("applied template field")
        };
        handle
    };
    // A skip-read field with a factory returning its exact type specializes.
    authority.serialization_template_shape(applied(4)).unwrap();
    // `Counted<T>.Count` is `T`; `MakeCount` returns i32, so only `Counted<i32>` holds.
    authority.serialization_template_shape(applied(5)).unwrap();
    let wrong = authority.serialization_template_shape(applied(6)).unwrap_err().to_string();
    assert!(wrong.contains("SerializationDefaultType"), "{wrong}");
    assert!(wrong.contains("Main.bd:"), "the rejection names the field source span: {wrong}");
}
