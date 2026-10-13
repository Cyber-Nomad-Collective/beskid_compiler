//! Actual canonical Decoder contract and prepare-owned source proof.
#[path = "isle_adapter/support.rs"]
mod support;
use beskid_analysis::mod_host::ModSemanticAuthority;
use beskid_analysis::projects::{CompilePlan, Target, TargetKind, prepare_project_workspace};
use beskid_analysis::mod_host::ModContributionMember;
use beskid_queries::{
    CompiledSerializationTarget, CompiledSerializationTemplate, MemberReferenceKind, ModSemanticQueryAuthority,
    SerializationContributionKind, contribution_attributed_unit, member_reference_legality,
};
use support::*;

fn fixture(output: &str) -> Result<(), String> {
    fixture_with_service_origins(output, true)
}
fn fixture_with_service_origins(output: &str, owned_origins: bool) -> Result<(), String> {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().unwrap();
    let user_root = directory.path().join("User");
    std::fs::create_dir(&user_root).unwrap();
    let path = user_root.join("Main.bd");
    let source = "use Core.Serialization.Contracts; use Core.Serialization.Errors; use Core.Serialization.Limits; use Core.Results; pub type Counter { pub i32 value, } pub type Reader { } pub unit Main() { return; }";
    std::fs::write(&path, source).unwrap();
    let manifest_path = directory.path().join("Host.bproj");
    std::fs::write(&manifest_path, "Host { name = \"Host\" version = \"1.0.0\" root = \"User\" } target \"Main\" { kind = Lib entry = \"Main.bd\" }").unwrap();
    let plan = CompilePlan {
        project_root: directory.path().into(),
        manifest_path,
        project_name: "Host".into(),
        source_root: user_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let prepared = prepare_project_workspace(&plan).unwrap();
    let generation = SyntaxGenerationId(120);
    let (base, _) = canonical_runtime_test_assembly(&mut db, directory.path(), generation);
    let mut units = base.units.as_ref().clone();
    // Runtime corpus registration does not inject the distinct Corelib service
    // corpus. Use its actual compiler-owned origins, never temporary byte copies
    // or a caller-populated trusted-path list to manufacture source authority.
    for canonical in canonical_corelib_service_sources() {
        let identity = beskid_abi::runtime_source::corelib_service_source_identity(&canonical.logical_path)
            .expect("canonical service has a compiler-owned source identity");
        units.retain(|unit| unit.logical_name != canonical.logical_path);
        let (origin_path, canonical_path) = if owned_origins {
            (identity.declared_path, identity.canonical_path)
        } else {
            let copied = directory.path().join(&canonical.logical_path);
            std::fs::create_dir_all(copied.parent().unwrap()).unwrap();
            std::fs::write(&copied, &canonical.source).unwrap();
            (copied.clone(), copied)
        };
        units.push(SourceUnit {
            logical_name: canonical.logical_path,
            origin_path,
            path: canonical_path,
            program: parse_program_with_source_name("canonical-corelib", &canonical.source).unwrap(),
            source: canonical.source,
        });
    }
    units.push(SourceUnit {
        logical_name: "User.Main".into(),
        origin_path: path.clone(),
        path: path.clone(),
        source: source.into(),
        program: parse_program_with_source_name("User.Main", source).unwrap(),
    });
    let entry_index = units.len() - 1;
    let mut assembly = ProgramAssembly::new(
        base.roots.clone(),
        Arc::new(units),
        entry_index,
        base.discovery,
        base.module_index.clone(),
        false,
        generation,
    );
    assembly.verified_package_identities = prepared.verified_package_identities;
    let project = ProjectSession::new(
        &db,
        directory.path().into(),
        path.clone(),
        "serialization-target".into(),
        "canonical".into(),
    );
    let target =
        TargetMetadata::supported().into_iter().find(|t| t.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap();
    let manifest = AbiManifestV5::canonical_runtime(target);
    let capability = canonical_corelib_service_capability(&manifest).unwrap();
    build_typed_program_with_corelib_services(
        &mut db,
        project,
        generation,
        Arc::new(assembly.clone()),
        canonical_corelib_service_capability(&manifest).unwrap(),
    )
    .unwrap();
    let contracts = assembly
        .units
        .iter()
        .find(|unit| unit.logical_name == beskid_abi::runtime_source::CANONICAL_SERIALIZATION_CONTRACTS_SOURCE_PATH)
        .unwrap();
    let contract_key =
        AstNodeKey { unit: SourceUnitId::new(&db, contracts.path.clone()), generation, node: AstNodeId(0) };
    assert_eq!(
        beskid_queries::canonical_corelib_source_path(&db, contract_key).as_deref(),
        owned_origins.then_some(beskid_abi::runtime_source::CANONICAL_SERIALIZATION_CONTRACTS_SOURCE_PATH)
    );
    let stale = AstNodeKey { generation: SyntaxGenerationId(119), ..contract_key };
    assert!(beskid_queries::canonical_corelib_source_path(&db, stale).is_none());
    let owner = AstNodeKey {
        unit: SourceUnitId::new(&db, path.clone()),
        generation,
        node: assembly.entry_syntax_index().ids_of_kind(NodeKind::TypeDefinition).next().unwrap(),
    };
    let contribution_source = format!(
        "impl Reader : Decoder<{output}> {{ pub Result<{output},SerializationError> ReadTyped(SerializationLimits limits) {{ return Result::Error(Errors.NumericError(\"fixture\")); }} }}"
    );
    let contribution = parse_program_with_source_name("generated", &contribution_source).unwrap().node.items[0].clone();
    let carrier = {
        let authority = ModSemanticQueryAuthority::for_registered_assembly(&db, project, &assembly).unwrap();
        use beskid_analysis::mod_host::{ModSyntaxAuthority, ModSyntaxRequest, ModSyntaxResponse};
        let ModSyntaxResponse::Node(Some(reference)) = authority.syntax_query(991, &ModSyntaxRequest::Root).unwrap()
        else {
            panic!("caller root missing")
        };
        let routes = authority.plan_canonical_paths(
            991,
            &reference,
            &[
                "Core.Results.Result".into(),
                "Core.Serialization.Contracts.Decoder".into(),
                "Core.Serialization.Reader.Reader".into(),
                "Core.Serialization.Limits.SerializationLimits".into(),
                "Core.Serialization.Errors.NumericError".into(),
                "Core.String.Core.SortOrdinal".into(),
            ],
        );
        if owned_origins {
            let routes = routes.expect("actual owned canonical sources must provide caller-visible routes");
            assert_eq!(routes.len(), 6);
            assert_eq!(routes[0].segments, ["Core", "Results", "Result"]);
            assert_eq!(routes[1].segments, ["Core", "Serialization", "Contracts", "Decoder"]);
            assert_eq!(routes[2].segments, ["Core", "Serialization", "Reader", "Reader"]);
            // The Mod's numeric failure constructor is a function in the exact
            // canonical Errors source, routed like the Diagnostic constructor.
            assert_eq!(routes[4].logical_name, "Core.Serialization.Errors.NumericError");
            assert_eq!(routes[4].segments, ["Core", "Serialization", "Errors", "NumericError"]);
            // Ordinal key ordering is the exact Foundation String Core function.
            assert_eq!(routes[5].logical_name, "Core.String.Core.SortOrdinal");
            assert_eq!(routes[5].segments, ["Core", "String", "Core", "SortOrdinal"]);
            assert!(
                authority
                    .plan_canonical_paths(
                        991,
                        &reference,
                        &["Core.Results.Result".into(), "Core.Results.Result".into()]
                    )
                    .is_err()
            );
        } else {
            assert!(routes.is_err(), "copied canonical-looking source cannot issue a route");
        }
        let handle = authority.resolve_type(owner).unwrap();
        authority.compile_serialization_target(991, 0, handle, &contribution, &[]).unwrap()
    };
    let proof = carrier.issuer_payload::<CompiledSerializationTarget>().unwrap();
    let mut units = assembly.units.as_ref().clone();
    units[assembly.entry_index].program.node.items.push(contribution);
    let mut next = ProgramAssembly::new(
        assembly.roots.clone(),
        Arc::new(units),
        assembly.entry_index,
        assembly.discovery,
        assembly.module_index.clone(),
        false,
        SyntaxGenerationId(121),
    );
    next.verified_package_identities = assembly.verified_package_identities.clone();
    next.compiled_mod_metadata.push(carrier.clone());
    let typed =
        build_typed_program_with_corelib_services(&mut db, project, next.generation, Arc::new(next), capability)
            .unwrap();
    let rebound = proof.rebind(&db, &typed).map_err(|e| e.to_string())?;
    assert_eq!(rebound.kind(), SerializationContributionKind::Decoder);
    assert_ne!(rebound.owner(), rebound.receiver(), "typed Decoder receiver is independently owned");
    assert!(rebound.shape(&db, &typed).is_ok());
    Ok(())
}
#[test]
fn canonical_decoder_target_preserves_typed_output_and_correspondence() {
    fixture("Counter").unwrap();
}
#[test]
fn canonical_decoder_cannot_bind_foreign_output() {
    assert!(fixture("Reader").is_err());
}

#[test]
fn copied_corelib_contract_bytes_cannot_bind_decoder_authority() {
    assert!(fixture_with_service_origins("Counter", false).is_err());
}

/// Two source units of one package: `Main.bd` is the entry unit (where the merger appends
/// contributions) and declares `Local` with a private field and the generic `Box<T>`;
/// `Model.bd` declares `Hidden` (a private field) and `Open` (only `pub` fields).
struct CrossUnit {
    db: BeskidDatabase,
    _directory: tempfile::TempDir,
    project: ProjectSession,
    assembly: ProgramAssembly,
    entry_path: std::path::PathBuf,
    model_path: std::path::PathBuf,
}
const CROSS_UNIT_MAIN: &str = "use Core.Serialization.Contracts; use Core.Serialization.Errors; use Core.Serialization.Limits; use Core.Results; use User.Model; pub type Local { i32 hidden, } pub type Reader { } pub type Box<T> { pub T value, } pub unit Use(Adapter<i32, Reader> adapter, SerializationLimits limits) { adapter.ReadTyped(limits); return; } pub unit Main() { return; }";
const CROSS_UNIT_MODEL: &str = "pub type Hidden { pub i32 shown, i32 secret, } pub type Open { pub i32 value, }";
fn cross_unit(generation: SyntaxGenerationId) -> CrossUnit {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().unwrap();
    let user_root = directory.path().join("User");
    std::fs::create_dir(&user_root).unwrap();
    let path = user_root.join("Main.bd");
    std::fs::write(&path, CROSS_UNIT_MAIN).unwrap();
    let model_path = user_root.join("Model.bd");
    std::fs::write(&model_path, CROSS_UNIT_MODEL).unwrap();
    let manifest_path = directory.path().join("Host.bproj");
    std::fs::write(&manifest_path, "Host { name = \"Host\" version = \"1.0.0\" root = \"User\" } target \"Main\" { kind = Lib entry = \"Main.bd\" }").unwrap();
    let plan = CompilePlan {
        project_root: directory.path().into(),
        manifest_path,
        project_name: "Host".into(),
        source_root: user_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let prepared = prepare_project_workspace(&plan).unwrap();
    let (base, _) = canonical_runtime_test_assembly(&mut db, directory.path(), generation);
    let mut units = base.units.as_ref().clone();
    for canonical in canonical_corelib_service_sources() {
        let identity = beskid_abi::runtime_source::corelib_service_source_identity(&canonical.logical_path)
            .expect("canonical service has a compiler-owned source identity");
        units.retain(|unit| unit.logical_name != canonical.logical_path);
        units.push(SourceUnit {
            logical_name: canonical.logical_path,
            origin_path: identity.declared_path,
            path: identity.canonical_path,
            program: parse_program_with_source_name("canonical-corelib", &canonical.source).unwrap(),
            source: canonical.source,
        });
    }
    units.push(SourceUnit {
        logical_name: "User.Model".into(),
        origin_path: model_path.clone(),
        path: model_path.clone(),
        source: CROSS_UNIT_MODEL.into(),
        program: parse_program_with_source_name("User.Model", CROSS_UNIT_MODEL).unwrap(),
    });
    units.push(SourceUnit {
        logical_name: "User.Main".into(),
        origin_path: path.clone(),
        path: path.clone(),
        source: CROSS_UNIT_MAIN.into(),
        program: parse_program_with_source_name("User.Main", CROSS_UNIT_MAIN).unwrap(),
    });
    let entry_index = units.len() - 1;
    let mut assembly = ProgramAssembly::new(
        base.roots.clone(),
        Arc::new(units),
        entry_index,
        base.discovery,
        base.module_index.clone(),
        false,
        generation,
    );
    assembly.verified_package_identities = prepared.verified_package_identities;
    let project =
        ProjectSession::new(&db, directory.path().into(), path.clone(), "serialization-cross-unit".into(), "canonical".into());
    build_typed_program_with_corelib_services(&mut db, project, generation, Arc::new(assembly.clone()), linux_capability())
        .unwrap();
    CrossUnit { db, _directory: directory, project, assembly, entry_path: path, model_path }
}
fn linux_capability() -> beskid_abi::runtime_source::CorelibServiceCapability {
    let target_metadata =
        TargetMetadata::supported().into_iter().find(|t| t.triple.as_str() == "x86_64-unknown-linux-gnu").unwrap();
    canonical_corelib_service_capability(&AbiManifestV5::canonical_runtime(target_metadata)).unwrap()
}
impl CrossUnit {
    fn unit_of(&self, name: &str) -> (std::path::PathBuf, usize) {
        let path = if name == "Hidden" || name == "Open" { self.model_path.clone() } else { self.entry_path.clone() };
        let index = self.assembly.units.iter().position(|unit| unit.path == path).unwrap();
        (path, index)
    }
    fn type_key(&self, name: &str) -> AstNodeKey {
        let (path, unit_index) = self.unit_of(name);
        let index = &self.assembly.syntax_indexes[unit_index];
        let program = &self.assembly.units[unit_index].program;
        let node = index
            .ids_of_kind(NodeKind::TypeDefinition)
            .find(|node| {
                index
                    .node_at(program, *node)
                    .and_then(|item| item.of::<beskid_analysis::syntax::TypeDefinition>())
                    .is_some_and(|definition| definition.name.node.name == name)
            })
            .unwrap();
        AstNodeKey { unit: SourceUnitId::new(&self.db, path), generation: self.assembly.generation, node }
    }
    /// Append `items` to the entry as the merger does and register the next generation with
    /// the retained `metadata`.
    fn appended(
        &mut self,
        items: &[beskid_analysis::syntax::Spanned<beskid_analysis::mod_host::ProgramItem>],
        metadata: &[beskid_analysis::mod_host::ModCompiledMetadata],
    ) -> (beskid_queries::TypedProgram, ProgramAssembly) {
        let mut units = self.assembly.units.as_ref().clone();
        units[self.assembly.entry_index].program.node.items.extend(items.iter().cloned());
        let mut next = ProgramAssembly::new(
            self.assembly.roots.clone(),
            Arc::new(units),
            self.assembly.entry_index,
            self.assembly.discovery,
            self.assembly.module_index.clone(),
            false,
            SyntaxGenerationId(self.assembly.generation.0 + 1),
        );
        next.verified_package_identities = self.assembly.verified_package_identities.clone();
        next.compiled_mod_metadata.extend(metadata.iter().cloned());
        let typed = build_typed_program_with_corelib_services(
            &mut self.db,
            self.project,
            next.generation,
            Arc::new(next.clone()),
            linux_capability(),
        )
        .unwrap();
        (typed, next)
    }
}
fn item(source: &str) -> beskid_analysis::syntax::Spanned<beskid_analysis::mod_host::ProgramItem> {
    parse_program_with_source_name("generated", source).unwrap().node.items[0].clone()
}
fn decoder_item(target: &str) -> beskid_analysis::syntax::Spanned<beskid_analysis::mod_host::ProgramItem> {
    item(&format!(
        "impl Reader : Decoder<{target}> {{ pub Result<{target},SerializationError> ReadTyped(SerializationLimits limits) {{ return Result::Error(Errors.NumericError(\"fixture\")); }} }}"
    ))
}
/// The free `read` function of a generated decoder group: it constructs the whole target
/// record literal, including its private field.
fn hidden_read_item(name: &str) -> beskid_analysis::syntax::Spanned<beskid_analysis::mod_host::ProgramItem> {
    item(&format!("pub Hidden {name}(i32 shown, i32 secret) {{ return Hidden {{ shown: shown, secret: secret, }}; }}"))
}
/// The top-level entry item of `assembly` equal to `item`, as a key of `generation`.
fn item_key(db: &BeskidDatabase, assembly: &ProgramAssembly, item: &beskid_analysis::syntax::Spanned<beskid_analysis::mod_host::ProgramItem>) -> AstNodeKey {
    let entry = assembly.entry_unit();
    let index = assembly.entry_syntax_index();
    let root = index.ids_of_kind(NodeKind::Program).next().unwrap();
    let appended = entry.program.node.items.iter().find(|candidate| *candidate == item).unwrap();
    let node = index.direct_child_id(&entry.program, root, beskid_analysis::syntax_query::DynNodeRef::from(appended)).unwrap();
    AstNodeKey { unit: SourceUnitId::new(db, entry.path.clone()), generation: assembly.generation, node }
}

/// Bind a Decoder contribution to the type `target` and return the binding error, if any.
fn bind_cross_unit_target(target: &str) -> Result<(), String> {
    let fixture = cross_unit(SyntaxGenerationId(130));
    let owner = fixture.type_key(target);
    let authority = ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let handle = authority.resolve_type(owner).map_err(|error| error.to_string())?;
    authority
        .compile_serialization_target(992, 0, handle, &decoder_item(target), &[])
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Rust precedent: a target declared in the entry unit, where the generated adapter is merged,
/// keeps its private fields readable and constructible by that adapter.
#[test]
fn entry_unit_target_with_a_private_field_binds() {
    bind_cross_unit_target("Local").unwrap();
}

/// A target in another source unit whose fields are all `pub` binds: the adapter needs no
/// private access.
#[test]
fn other_unit_target_with_only_pub_fields_binds() {
    bind_cross_unit_target("Open").unwrap();
}

/// Per-contribution unit attribution: a group generated for a target declared in another unit
/// binds, and the whole group (impl and its free `read` member) is judged for field visibility
/// in the declaring unit, as derive output lives in the module of its input item. An equal
/// function appended without a binding stays in the entry unit and keeps the E1211 denial.
#[test]
fn other_unit_target_with_a_private_field_binds_and_its_group_is_attributed_to_the_declaring_unit() {
    let mut fixture = cross_unit(SyntaxGenerationId(140));
    let owner = fixture.type_key("Hidden");
    let contribution = decoder_item("Hidden");
    let read = hidden_read_item("ReadHidden");
    let twin = hidden_read_item("ReadHiddenUnbound");
    let carrier = {
        let authority =
            ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
        let handle = authority.resolve_type(owner).unwrap();
        let members = [ModContributionMember { contribution_index: 1, item: read.clone() }];
        authority.compile_serialization_target(993, 0, handle, &contribution, &members).unwrap()
    };
    let attribution = carrier.description().attribution().expect("serialization claim carries its attribution");
    assert_eq!(attribution.attributed_unit, fixture.model_path);
    assert_eq!(attribution.items, vec![contribution.clone(), read.clone()]);
    let proof = carrier.issuer_payload::<CompiledSerializationTarget>().unwrap();
    assert_eq!(proof.members().len(), 1);
    let (typed, next) = fixture.appended(&[contribution.clone(), read.clone(), twin.clone()], &[carrier.clone()]);
    let db = &fixture.db;
    let model = SourceUnitId::new(db, fixture.model_path.clone());
    let entry = SourceUnitId::new(db, fixture.entry_path.clone());
    // The query gate judges the bound group in the declaring unit and nothing else.
    let read_key = item_key(db, &next, &read);
    let impl_key = item_key(db, &next, &contribution);
    let twin_key = item_key(db, &next, &twin);
    let original_key = AstNodeKey { node: next.entry_syntax_index().ids_of_kind(NodeKind::TypeDefinition).next().unwrap(), ..read_key };
    assert_eq!(contribution_attributed_unit(db, read_key), model);
    assert_eq!(contribution_attributed_unit(db, impl_key), model);
    assert_eq!(contribution_attributed_unit(db, twin_key), entry);
    assert_eq!(contribution_attributed_unit(db, original_key), entry);
    // A node inside the attributed item is attributed through its top-level item.
    let inner = next.entry_syntax_index().children(read_key.node).unwrap()[0];
    assert_eq!(contribution_attributed_unit(db, AstNodeKey { node: inner, ..read_key }), model);
    // Field visibility: the attributed `read` builds the private field; the unbound twin is E1211.
    let bound = member_reference_legality(db, read_key).expect("legality of the attributed read function");
    assert!(bound.is_none(), "an attributed group constructs its target's private field: {bound:?}");
    let unbound = member_reference_legality(db, twin_key)
        .expect("legality of the unbound twin")
        .expect("an unattributed function in the entry unit cannot build a private field of another unit");
    assert_eq!(unbound.kind, MemberReferenceKind::InaccessibleStructField { name: "secret".into() });
    // The legacy checker's descriptive mirror names the same items and no other.
    let mirror = next.entry_item_attributions();
    let items = &next.entry_unit().program.node.items;
    for (index, item) in items.iter().enumerate() {
        let expected = (*item == contribution || *item == read).then(|| fixture.model_path.clone());
        assert_eq!(mirror[index], expected, "item {index}");
    }
    // The binding proof rebinds with its group present.
    let rebound = proof.rebind(db, &typed).unwrap();
    assert_eq!(rebound.kind(), SerializationContributionKind::Decoder);
}

/// A group member that is itself an impl block, or an index bound twice, is not a group.
#[test]
fn group_members_exclude_impl_blocks_and_repeated_indexes() {
    let fixture = cross_unit(SyntaxGenerationId(150));
    let owner = fixture.type_key("Open");
    let authority = ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
    let handle = authority.resolve_type(owner).unwrap();
    let contribution = decoder_item("Open");
    let read = item("pub Open ReadOpen(i32 value) { return Open { value: value, }; }");
    let as_impl = [ModContributionMember { contribution_index: 1, item: decoder_item("Open") }];
    let error = authority.compile_serialization_target(994, 0, handle, &contribution, &as_impl).unwrap_err().to_string();
    assert!(error.contains("impl block"), "{error}");
    let repeated = [
        ModContributionMember { contribution_index: 1, item: read.clone() },
        ModContributionMember { contribution_index: 1, item: read.clone() },
    ];
    let error = authority.compile_serialization_target(994, 0, handle, &contribution, &repeated).unwrap_err().to_string();
    assert!(error.contains("bound more than once"), "{error}");
    let self_bound = [ModContributionMember { contribution_index: 0, item: read }];
    assert!(authority.compile_serialization_target(994, 0, handle, &contribution, &self_bound).is_err());
}

/// The exact append correspondence covers the whole group: an appended impl whose `read`
/// member is absent cannot rebind, and the group is unattributed.
#[test]
fn missing_group_member_breaks_correspondence_and_attribution() {
    let mut fixture = cross_unit(SyntaxGenerationId(160));
    let owner = fixture.type_key("Hidden");
    let contribution = decoder_item("Hidden");
    let read = hidden_read_item("ReadHidden");
    let carrier = {
        let authority =
            ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
        let handle = authority.resolve_type(owner).unwrap();
        let members = [ModContributionMember { contribution_index: 1, item: read.clone() }];
        authority.compile_serialization_target(995, 0, handle, &contribution, &members).unwrap()
    };
    let (typed, next) = fixture.appended(&[contribution.clone()], &[carrier.clone()]);
    let proof = carrier.issuer_payload::<CompiledSerializationTarget>().unwrap();
    let error = proof.rebind(&fixture.db, &typed).err().expect("a group without its member cannot rebind").to_string();
    assert!(error.contains("group member"), "{error}");
    let impl_key = item_key(&fixture.db, &next, &contribution);
    assert_eq!(contribution_attributed_unit(&fixture.db, impl_key), SourceUnitId::new(&fixture.db, fixture.entry_path.clone()));
    assert!(next.entry_item_attributions().iter().all(Option::is_none));
}

/// Every typed program of one generation registers the same attribution table. The compile
/// session, the query gate (`semantic_diagnostics_for_roots`, run after the Mod rewrite on the
/// rewritten entry) and test enumeration (a fresh session over the front end's syntax assembly)
/// each rebuild the assembly through `ProgramAssembly::with_entry_program`, which retains the
/// compiled Mod metadata; a rebuild that drops it would derive an empty table and is rejected.
#[test]
fn every_build_of_a_generation_registers_the_same_attribution() {
    let mut fixture = cross_unit(SyntaxGenerationId(165));
    let owner = fixture.type_key("Hidden");
    let contribution = decoder_item("Hidden");
    let read = hidden_read_item("ReadHidden");
    let carrier = {
        let authority =
            ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
        let handle = authority.resolve_type(owner).unwrap();
        let members = [ModContributionMember { contribution_index: 1, item: read.clone() }];
        authority.compile_serialization_target(996, 0, handle, &contribution, &members).unwrap()
    };
    let (_, next) = fixture.appended(&[contribution.clone(), read.clone()], &[carrier]);
    let model = SourceUnitId::new(&fixture.db, fixture.model_path.clone());
    // The query gate over the same generation.
    let program = next.entry_unit().program.clone();
    beskid_queries::semantic_diagnostics_for_roots(&mut fixture.db, None, &next, &program, None)
        .expect("the query gate registers the attribution the compile session registered");
    // The rebuilt syntax assembly (the front end's `syntax_assembly`) registers the same table.
    let rebuilt = next.with_entry_program(program, next.generation);
    assert_eq!(rebuilt.compiled_mod_metadata, next.compiled_mod_metadata);
    build_typed_program_with_corelib_services(
        &mut fixture.db,
        fixture.project,
        next.generation,
        Arc::new(rebuilt.clone()),
        linux_capability(),
    )
    .expect("a rebuilt assembly registers the same attribution");
    assert_eq!(contribution_attributed_unit(&fixture.db, item_key(&fixture.db, &next, &read)), model);
    // Executable lowering registers the prepared assembly in a database of its own, under a
    // session of its own. The retained payload names no interned id of the issuing database, so
    // the group is attributed and the target rebinds there too.
    let mut lowering = BeskidDatabase::default();
    let session = ProjectSession::new(
        &lowering,
        next.roots.host.source_root.clone(),
        fixture.entry_path.clone(),
        "syntax-codegen".into(),
        "prepared-frontend".into(),
    );
    let typed = build_typed_program_with_corelib_services(
        &mut lowering,
        session,
        next.generation,
        Arc::new(rebuilt.clone()),
        linux_capability(),
    )
    .expect("the lowering database registers the prepared generation");
    let lowering_model = SourceUnitId::new(&lowering, fixture.model_path.clone());
    assert_eq!(contribution_attributed_unit(&lowering, item_key(&lowering, &rebuilt, &read)), lowering_model);
    let proof = rebuilt.compiled_mod_metadata[0].issuer_payload::<CompiledSerializationTarget>().unwrap();
    let rebound = proof.rebind(&lowering, &typed).expect("the retained target rebinds in the lowering database");
    assert_eq!(rebound.kind(), SerializationContributionKind::Decoder);
    // Dropping the retained metadata derives a different table for the registered generation.
    let mut stripped = next.clone();
    stripped.compiled_mod_metadata.clear();
    let error = build_typed_program_with_corelib_services(
        &mut fixture.db,
        fixture.project,
        next.generation,
        Arc::new(stripped),
        linux_capability(),
    )
    .err()
    .expect("a build without the retained metadata cannot re-register the generation")
    .to_string();
    assert!(error.contains("attribution differs"), "{error}");
}

/// Generic serialization: a template bound to `Box<T>` is rebound per concrete instantiation.
/// The emitted specialization of the adapter method `ReadTyped` for `Adapter<i32, Reader>`
/// rebinds the canonical `Decoder<Box<i32>>` application in its complete environment; an
/// instance of an ordinary method is not a template contribution.
#[test]
fn generic_target_template_rebinds_per_concrete_instance() {
    use beskid_analysis::mod_host::{ModSemanticDeclaration, ModSyntaxAuthority, ModSyntaxBounds, ModSyntaxRequest, ModSyntaxResponse};
    let mut fixture = cross_unit(SyntaxGenerationId(170));
    let owner = fixture.type_key("Box");
    let adapter = item("pub type Adapter<T, R> { pub R reader, }");
    let contribution = item(
        "impl<T, R> Adapter<T, R> : Decoder<Box<T>> { pub Result<Box<T>,SerializationError> ReadTyped(SerializationLimits limits) { return Result::Error(Errors.NumericError(\"fixture\")); } }",
    );
    let carrier = {
        let authority =
            ModSemanticQueryAuthority::for_registered_assembly(&fixture.db, fixture.project, &fixture.assembly).unwrap();
        let ModSyntaxResponse::Node(Some(root)) = authority.syntax_query(996, &ModSyntaxRequest::Root).unwrap() else {
            panic!("entry root")
        };
        let ModSyntaxResponse::Nodes(nodes) = authority
            .syntax_query(996, &ModSyntaxRequest::OfKind { node: root, bounds: ModSyntaxBounds { max_nodes: 65536, max_depth: 256 }, kind: NodeKind::TypeDefinition })
            .unwrap()
        else {
            panic!("entry type definitions")
        };
        let reference = nodes.into_iter().find(|node| node.node == owner.node).expect("Box<T> reference");
        let span = fixture.assembly.entry_syntax_index().metadata()[owner.node.0 as usize].span.unwrap();
        let declaration = ModSemanticDeclaration { source_unit: fixture.entry_path.clone(), generation: owner.generation, node: owner.node, span };
        let members = [ModContributionMember { contribution_index: 1, item: adapter.clone() }];
        authority.compile_serialization_template(996, 0, &reference, &declaration, &contribution, &members).unwrap()
    };
    assert!(carrier.issuer_payload::<CompiledSerializationTemplate>().is_some());
    let (typed, next) = fixture.appended(&[contribution.clone(), adapter.clone()], &[carrier.clone()]);
    let db = &fixture.db;
    // The call `adapter.ReadTyped(limits)` in the entry instantiates the adapter method.
    let entry = next.entry_unit();
    let index = next.entry_syntax_index();
    // The call node's span is the call operator inside the postfix chain, so locate it by
    // containment in the source text of the whole call.
    let text = "adapter.ReadTyped(limits)";
    let offset = CROSS_UNIT_MAIN.find(text).unwrap();
    let call = index
        .metadata()
        .iter()
        .find(|metadata| {
            metadata.kind == NodeKind::CallExpression
                && metadata.span.is_some_and(|span| span.start >= offset && span.end <= offset + text.len())
        })
        .expect("ReadTyped call")
        .id;
    let call_key = AstNodeKey { unit: SourceUnitId::new(db, entry.path.clone()), generation: next.generation, node: call };
    let specialization = beskid_queries::generic_call_specialization(db, call_key)
        .expect("call specialization query")
        .expect("ReadTyped call is a concrete instantiation of the generic adapter method");
    let instance = beskid_queries::generic_call_specialization_instance(db, specialization).unwrap().unwrap();
    let rebound = beskid_queries::serialization_template_instance_rebind(db, &typed, &instance)
        .expect("template instance rebind")
        .expect("the adapter method is a retained template contribution");
    assert_eq!(rebound.kind(), SerializationContributionKind::Decoder);
    assert_eq!(rebound.owner(), AstNodeKey { generation: next.generation, ..owner });
    assert!(rebound.shape(db, &typed).is_ok());
    // An ordinary entry method instance is not a template contribution.
    let main = index
        .ids_of_kind(NodeKind::FunctionDefinition)
        .map(|node| AstNodeKey { node, ..call_key })
        .next()
        .unwrap();
    let foreign = beskid_queries::GenericSpecializationInstance { declaration: main, ..instance.clone() };
    assert!(beskid_queries::serialization_template_instance_rebind(db, &typed, &foreign).unwrap().is_none());
}

/// Typed field decoder factory correspondence: a retained Counter decoder group with its field
/// decoder factory `ReadCounter<R>`, one decoy member with the wrong arity and one with the wrong
/// output. `ReadField<Counter, Wire>` must issue exactly `ReadCounter<Wire>`, selected by its
/// resolved signature and admitted through the group's target correspondence.
fn field_decoder_fixture(factory_output: &str) -> Result<String, String> {
    let mut db = BeskidDatabase::default();
    let directory = tempfile::tempdir().unwrap();
    let user_root = directory.path().join("User");
    std::fs::create_dir(&user_root).unwrap();
    let path = user_root.join("Main.bd");
    let source = "use Core.Serialization.Contracts; use Core.Serialization.Errors; use Core.Serialization.Limits; use Core.Serialization.Metadata; use Core.Serialization.Reader; use Core.Results; pub type Counter { pub i32 value, } pub type Wire : Reader { pub SourceLocation Location() { return SourceLocation { path: \"wire\", start: 0_i64, end: 0_i64 }; } pub Result<unit, SerializationError> Begin(SerializationLimits limits) { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<unit, SerializationError> Finish() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<unit, SerializationError> ReadUnit() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<bool, SerializationError> ReadBoolean() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<i64, SerializationError> ReadSigned(u8 width) { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<u64, SerializationError> ReadUnsigned(u8 width) { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<u64, SerializationError> ReadFloat(u8 width) { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<char, SerializationError> ReadScalar() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<string, SerializationError> ReadString() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<u8[], SerializationError> ReadBytes() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<i64, SerializationError> BeginRecord(Metadata.ShapeId shape) { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<string, SerializationError> BeginField() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<unit, SerializationError> EndField() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<unit, SerializationError> EndRecord() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<i64, SerializationError> BeginSequence() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<unit, SerializationError> EndSequence() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<i64, SerializationError> BeginMap() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<string, SerializationError> BeginMapEntry() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<unit, SerializationError> EndMapEntry() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<unit, SerializationError> EndMap() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<string, SerializationError> BeginVariant(Metadata.ShapeId shape) { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<i64, SerializationError> VariantCount() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<unit, SerializationError> EndVariant() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<bool, SerializationError> BeginOptional() { return Result::Error(Errors.NumericError(\"wire\")); } pub Result<unit, SerializationError> EndOptional() { return Result::Error(Errors.NumericError(\"wire\")); } } pub Result<Counter, SerializationError> Use(Wire wire, SerializationLimits limits) { return Core.Serialization.Reader.ReadField<Counter, Wire>(wire, limits); } pub unit Main() { return; }";
    std::fs::write(&path, source).unwrap();
    let manifest_path = directory.path().join("Host.bproj");
    std::fs::write(&manifest_path, "Host { name = \"Host\" version = \"1.0.0\" root = \"User\" } target \"Main\" { kind = Lib entry = \"Main.bd\" }").unwrap();
    let plan = CompilePlan {
        project_root: directory.path().into(),
        manifest_path,
        project_name: "Host".into(),
        source_root: user_root,
        target: Target { name: "Main".into(), kind: TargetKind::Lib, entry: Some("Main.bd".into()) },
        dependency_projects: vec![],
        unresolved_dependencies: vec![],
        has_core_dependency: false,
    };
    let prepared = prepare_project_workspace(&plan).unwrap();
    let generation = SyntaxGenerationId(130);
    let (base, _) = canonical_runtime_test_assembly(&mut db, directory.path(), generation);
    let mut units = base.units.as_ref().clone();
    for canonical in canonical_corelib_service_sources() {
        let identity = beskid_abi::runtime_source::corelib_service_source_identity(&canonical.logical_path)
            .expect("canonical service has a compiler-owned source identity");
        units.retain(|unit| unit.logical_name != canonical.logical_path);
        units.push(SourceUnit {
            logical_name: canonical.logical_path,
            origin_path: identity.declared_path,
            path: identity.canonical_path,
            program: parse_program_with_source_name("canonical-corelib", &canonical.source).unwrap(),
            source: canonical.source,
        });
    }
    // Reader.bd names `Metadata.ShapeId`; the bound `Wire: Reader` needs that real declaration.
    let metadata = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../corelib/packages/serialization/src/Core/Serialization/Metadata.bd")
        .canonicalize()
        .unwrap();
    let metadata_text = std::fs::read_to_string(&metadata).unwrap();
    units.push(SourceUnit {
        logical_name: "Core.Serialization.Metadata".into(),
        origin_path: metadata.clone(),
        path: metadata,
        program: parse_program_with_source_name("Metadata", &metadata_text).unwrap(),
        source: metadata_text,
    });
    units.push(SourceUnit {
        logical_name: "User.Main".into(),
        origin_path: path.clone(),
        path: path.clone(),
        source: source.into(),
        program: parse_program_with_source_name("User.Main", source).unwrap(),
    });
    let entry_index = units.len() - 1;
    let mut assembly = ProgramAssembly::new(
        base.roots.clone(),
        Arc::new(units),
        entry_index,
        base.discovery,
        base.module_index.clone(),
        false,
        generation,
    );
    assembly.verified_package_identities = prepared.verified_package_identities;
    let project =
        ProjectSession::new(&db, directory.path().into(), path.clone(), "serialization-field-decoder".into(), "canonical".into());
    build_typed_program_with_corelib_services(&mut db, project, generation, Arc::new(assembly.clone()), linux_capability())
        .unwrap();
    let owner = AstNodeKey {
        unit: SourceUnitId::new(&db, path.clone()),
        generation,
        node: assembly.entry_syntax_index().ids_of_kind(NodeKind::TypeDefinition).next().unwrap(),
    };
    let group = format!(
        "impl<R> Adapter<R> : Decoder<Counter> where R: Reader {{ pub Result<Counter, SerializationError> ReadTyped(SerializationLimits limits) {{ return Result::Error(Errors.NumericError(\"fixture\")); }} }} \
         pub type Adapter<R> {{ R reader, }} \
         pub Result<{factory_output}, SerializationError> ReadCounter<R>(R reader, SerializationLimits limits) where R: Reader {{ return Result::Error(Errors.NumericError(\"fixture\")); }} \
         pub Result<Counter, SerializationError> ReadWithoutLimits<R>(R reader) where R: Reader {{ return Result::Error(Errors.NumericError(\"fixture\")); }}"
    );
    let items = parse_program_with_source_name("generated", &group).unwrap().node.items;
    let contribution = items[0].clone();
    let members = items[1..]
        .iter()
        .enumerate()
        .map(|(index, item)| ModContributionMember { contribution_index: index as u32 + 1, item: item.clone() })
        .collect::<Vec<_>>();
    let carrier = {
        let authority = ModSemanticQueryAuthority::for_registered_assembly(&db, project, &assembly).unwrap();
        let handle = authority.resolve_type(owner).unwrap();
        authority.compile_serialization_target(992, 0, handle, &contribution, &members).unwrap()
    };
    let mut units = assembly.units.as_ref().clone();
    units[assembly.entry_index].program.node.items.extend(items.iter().cloned());
    let mut next = ProgramAssembly::new(
        assembly.roots.clone(),
        Arc::new(units),
        assembly.entry_index,
        assembly.discovery,
        assembly.module_index.clone(),
        false,
        SyntaxGenerationId(131),
    );
    next.verified_package_identities = assembly.verified_package_identities.clone();
    next.compiled_mod_metadata.push(carrier);
    let typed = build_typed_program_with_corelib_services(&mut db, project, next.generation, Arc::new(next), linux_capability())
        .unwrap();
    let entry = SourceUnitId::new(&db, path.clone());
    let mut instances = Vec::new();
    let mut seen = Vec::new();
    for node in typed.assembly.entry_syntax_index().ids_of_kind(NodeKind::CallExpression) {
        let key = AstNodeKey { unit: entry, generation: typed.generation, node };
        match beskid_queries::generic_call_specialization(&db, key) {
            Ok(Some(call)) => match beskid_queries::generic_call_specialization_instance(&db, call) {
                Ok(Some(instance)) => {
                    let name = item_name(&db, instance.declaration).ok().flatten();
                    seen.push(format!("{node:?}: {name:?}"));
                    if name.as_deref() == Some("ReadField") {
                        instances.push(instance);
                    }
                }
                other => seen.push(format!("{node:?}: instance {other:?}")),
            },
            other => seen.push(format!("{node:?}: call {other:?}")),
        }
    }
    let [read_field] = instances.as_slice() else { panic!("one ReadField<Counter, Wire> call: {seen:#?}") };
    let bridge = beskid_queries::serialization_field_codec_bridge(&db, read_field)
        .map_err(|error| error.to_string())?
        .expect("the canonical ReadField has one bridge call");
    let binding = beskid_queries::serialization_field_codec_binding(&db, &typed, read_field)
        .map_err(|error| error.to_string())?
        .expect("the canonical ReadField issues a field read");
    assert_eq!(binding.instance(), &bridge, "the issued body belongs to the ReadField bridge call");
    assert_eq!(binding.target().signature, bridge.signature, "the issued read has the bridge ABI");
    item_name(&db, binding.target().declaration)
        .map_err(|error| error.to_string())?
        .map(|name| name.to_string())
        .ok_or_else(|| "issued read has no name".into())
}

#[test]
fn field_decoder_correspondence_issues_the_generated_factory_by_signature() {
    assert_eq!(field_decoder_fixture("Counter").unwrap(), "ReadCounter");
}

#[test]
fn field_decoder_correspondence_rejects_a_factory_with_another_output() {
    let error = field_decoder_fixture("i32").expect_err("a factory for another output cannot decode Counter");
    assert!(error.contains("no admitted field decoder factory"), "{error}");
}
