//! Compiler-issued executable native Mod object and source-bound CABI adapter evidence.
use super::adapter_plan::NativeAdapterPlan;
use crate::CodegenArtifact;
use beskid_abi::abi_v5::TargetMetadata;
use beskid_analysis::mod_host::{ContractRegistration, NativeModCallableIdentity};

pub struct PreparedNativeMod {
    artifact: CodegenArtifact,
    target: TargetMetadata,
    adapter_source: Vec<u8>,
    adapter_metadata: Vec<u8>,
    discriminator_symbol: String,
    entries: Vec<PreparedNativeModEntry>,
}
pub struct PreparedNativeModEntry {
    registration: ContractRegistration,
    identity: NativeModCallableIdentity,
}
impl PreparedNativeModEntry {
    pub fn registration(&self) -> &ContractRegistration {
        &self.registration
    }
    pub fn identity(&self) -> &NativeModCallableIdentity {
        &self.identity
    }
}
impl PreparedNativeMod {
    pub fn artifact(&self) -> &CodegenArtifact {
        &self.artifact
    }
    pub fn target(&self) -> &TargetMetadata {
        &self.target
    }
    pub fn host_abi_version(&self) -> u32 {
        super::adapter_plan::NATIVE_ADAPTER_ABI_VERSION
    }
    pub fn adapter_metadata(&self) -> &[u8] {
        &self.adapter_metadata
    }
    pub fn adapter_source(&self) -> &[u8] {
        &self.adapter_source
    }
    pub fn discriminator_symbol(&self) -> &str {
        &self.discriminator_symbol
    }
    pub fn entries(&self) -> &[PreparedNativeModEntry] {
        &self.entries
    }
    pub fn sdk_schema(&self) -> &[u8] {
        super::sdk_schema::CANONICAL_SDK_SCHEMA
    }
    pub fn sdk_sources(&self) -> impl Iterator<Item = (&'static str, &'static [u8])> {
        beskid_abi::sdk_source::canonical_sdk_sources().iter().map(|source| (source.path(), source.bytes()))
    }
}
/// Called only after current-generation witnesses, layouts, source constructor symbols and
/// native callback capabilities have all been issued by this module's producer.
fn seal(
    artifact: CodegenArtifact,
    target: TargetMetadata,
    plan: NativeAdapterPlan,
    entries: Vec<PreparedNativeModEntry>,
) -> anyhow::Result<PreparedNativeMod> {
    let adapter_source = super::c_codec::emit_native_adapters(&plan)?;
    let adapter_metadata = serde_json::to_vec(&plan)?;
    Ok(PreparedNativeMod {
        artifact,
        target,
        adapter_source,
        adapter_metadata,
        discriminator_symbol: plan.discriminator_symbol,
        entries,
    })
}

struct SelectedContract {
    witness: super::contracts::ContractWitness,
    method: beskid_queries::AstNodeKey,
    factory: super::contracts::ContractWitness,
    factory_method: beskid_queries::AstNodeKey,
}
fn discover(input: &crate::CodegenInput<'_>) -> anyhow::Result<Vec<SelectedContract>> {
    use anyhow::{Context, bail};
    use beskid_queries::{IndexedNodeKind, NativeModContractFamily, child_nodes, node_kind};
    let mut pending = input.roots().to_vec();
    let mut visited = std::collections::HashSet::new();
    let mut owners = Vec::new();
    while let Some(key) = pending.pop() {
        if !visited.insert(key) {
            continue;
        }
        if visited.len() > 1_000_000 {
            bail!("native Mod declaration discovery budget exceeded");
        }
        if node_kind(input.database(), key)
            .with_context(|| format!("native Mod discovery: node kind query failed at {}", beskid_queries::format_ast_node_site(input.database(), key)))?
            == Some(IndexedNodeKind::TypeDefinition)
        {
            owners.push(key);
        }
        if let Some(children) =
            child_nodes(input.database(), key).with_context(|| format!("native Mod discovery: child nodes query failed at {}", beskid_queries::format_ast_node_site(input.database(), key)))?
        {
            pending.extend(children.iter().copied());
        }
    }
    owners.sort_by_key(|key| (key.unit.path(input.database()).clone(), key.node.0));
    let mut contracts = Vec::new();
    for owner in &owners {
        // User dependency closures may contain ordinary non-contract records. Selection only
        // admits exact current canonical SDK conformances, never terminal spelling matches.
        for witness in super::contracts::select_contract_witnesses(input, *owner)
            .with_context(|| format!("native Mod contract witness selection failed at {}", beskid_queries::format_ast_node_site(input.database(), *owner)))?
        {
            if witness.family == NativeModContractFamily::Factory {
                continue;
            }
            if witness.methods.len() != 1 {
                bail!("native Mod contract requires one exact callable method");
            }
            let method = witness.methods[0].1;
            let factory = super::contracts::select_instance_factory(input, *owner, &owners)
                .with_context(|| format!("native Mod instance factory selection failed at {}", beskid_queries::format_ast_node_site(input.database(), *owner)))?;
            if factory.methods.len() != 1 {
                bail!("native Mod instance factory has ambiguous callable methods");
            }
            let factory_method = factory.methods.first().context("native instance factory lacks Create witness")?.1;
            contracts.push(SelectedContract { witness, method, factory, factory_method });
        }
    }
    if contracts.is_empty() {
        bail!("prepared source contains no exact canonical native Mod contracts");
    }
    Ok(contracts)
}

pub fn lower_prepared_native_mod(
    db: &mut beskid_queries::BeskidDatabase,
    front: &beskid_analysis::services::FrontEndTypedResult,
    target: TargetMetadata,
    isa: &dyn cranelift_codegen::isa::TargetIsa,
) -> anyhow::Result<PreparedNativeMod> {
    use super::{adapter_plan::*, transport_layout::TransportLayouts};
    use anyhow::{Context, bail};
    use beskid_queries::{NativeModTransportType, SourceUnitId, native_mod_transport_signature};
    use sha2::{Digest, Sha256};
    crate::prepared_syntax::with_prepared_module_input(db, front, target, |input, items| {
        let selected = discover(input).context("native Mod phase: declaration discovery and contract witness selection")?;
        let selected_keys = selected.iter().flat_map(|entry| [entry.method, entry.factory_method]).collect::<Vec<_>>();
        let capability = super::callbacks::NativeModCallbackCapability::for_selected_closure(input, &selected_keys, items)
                .with_context(|| format!(
                    "native Mod phase: callback capability selection for {}",
                    describe_sites(input.database(), &selected_keys)
                ))?;
        let callbacks_issued = capability.callbacks().collect::<Vec<_>>();
        let input =
            input.with_artifact_namespace(input.artifact_namespace().into()).with_native_mod_callbacks(capability);
        let schema = super::sdk_schema::CanonicalSdkSchema::parse(super::sdk_schema::CANONICAL_SDK_SCHEMA)?;
        let _syntax = super::marshaling::SyntaxConstructors::select(&input, items, &schema)?;
        let _requests = super::requests::RequestConstructors::select(&input, items)?;
        let mut layouts = TransportLayouts::new();
        let mut constructors = Vec::new();
        let mut constructor_keys = Vec::new();
        for item in items {
            let site = || beskid_queries::format_ast_node_site(input.database(), item.key);
            if beskid_queries::native_mod_syntax_constructor(input.database(), item.key)
                .with_context(|| format!("native Mod phase: syntax constructor query failed at {}", site()))?
                .is_some()
                || beskid_queries::native_mod_request_constructor(input.database(), item.key)
                    .with_context(|| format!("native Mod phase: request constructor query failed at {}", site()))?
                    .is_some()
                || super::constructors::is_callback_value_constructor(&input, item)
                    .with_context(|| format!("native Mod phase: callback value constructor check failed at {}", site()))?
            {
                constructor_keys.push(item.key);
                constructors.push(
                    super::constructors::issue_constructor(&input, item, &mut layouts)
                        .with_context(|| format!("native Mod phase: constructor issuance failed at {}", site()))?,
                );
            }
        }
        // Emit one canonical reachable union for actual methods/factories and all
        // typed value constructors required by the C adapter. Unrelated SDK
        // query helpers must not enter lowering without callback admission.
        let mut emitted_keys = std::collections::HashSet::new();
        for key in selected_keys.iter().chain(constructor_keys.iter()).copied() {
            let root = input
                .roots()
                .iter()
                .copied()
                .find(|root| root.unit == key.unit)
                .context("native callable has no registered source root")?;
            let reachable = beskid_queries::reachable_items(input.database(), root, key)
                .with_context(|| format!(
                    "native Mod phase: reachable items query failed at {}",
                    beskid_queries::format_ast_node_site(input.database(), key)
                ))?
                .context("native callable reachable facts are incomplete")?;
            emitted_keys.extend(reachable.iter().copied());
            emitted_keys.insert(key);
        }
        emitted_keys.extend(callbacks_issued.iter().map(|callback| callback.wrapper()));
        let emitted_items = items.iter().filter(|item| emitted_keys.contains(&item.key)).cloned().collect::<Vec<_>>();
        let mut artifact = crate::lower_syntax_program(&input, isa, &emitted_items)
            .map_err(|error| error.into_report(&input, "native Mod source lowering failed"))?;
        // Source callables cross only the internal object boundary into the generated C adapter.
        // They are not issued as CABI request entries.
        for item in items {
            if artifact.functions.iter().any(|function| function.name == item.symbol) {
                artifact.exports.push(crate::ExportEntry {
                    beskid_name: item.symbol.clone(),
                    exported_symbol: crate::internal_link_symbol(&item.symbol),
                    abi: "C".to_owned(),
                });
            }
        }
        let mut entries = Vec::new();
        let mut planned_entries = Vec::new();
        for (ordinal, entry) in selected.iter().enumerate() {
            let method = items
                .iter()
                .find(|item| item.key == entry.method)
                .context("native method is not in current lowered module")?;
            let factory = items
                .iter()
                .find(|item| item.key == entry.factory_method)
                .context("native factory is not in current lowered module")?;
            let method_signature = native_mod_transport_signature(input.database(), entry.method)
                .with_context(|| format!(
                    "native Mod phase: transport signature query for method at {}",
                    beskid_queries::format_ast_node_site(input.database(), entry.method)
                ))?
                .context("native method source signature unavailable")?;
            let factory_signature = native_mod_transport_signature(input.database(), entry.factory_method)
                .with_context(|| format!(
                    "native Mod phase: transport signature query for factory at {}",
                    beskid_queries::format_ast_node_site(input.database(), entry.factory_method)
                ))?
                .context("native factory source signature unavailable")?;
            if method_signature.parameters().len() != 2 || factory_signature.parameters().len() != 2 {
                bail!("native contract methods require exact receiver and one source request");
            }
            let NativeModTransportType::Nominal { declaration: owner, .. } = &method_signature.parameters()[0] else {
                bail!("native receiver lacks nominal identity");
            };
            if *owner != entry.witness.owner || factory_signature.result() != &method_signature.parameters()[0] {
                bail!("native factory result differs from exact contract receiver");
            }
            let receiver_type = layouts.issue(&input, entry.method, &method_signature.parameters()[0])?;
            let request_type = layouts.issue(&input, entry.method, &method_signature.parameters()[1])?;
            let result_type = layouts.issue(&input, entry.method, method_signature.result())?;
            let factory_request_type =
                layouts.issue(&input, entry.factory_method, &factory_signature.parameters()[1])?;
            let bootstrap = bootstrap_factory(
                &input,
                items,
                &mut artifact,
                isa,
                entry.factory.owner,
                &mut layouts,
                &mut Vec::new(),
            )?;
            let symbol = format!("beskid_mod_v2_entry_{ordinal}");
            planned_entries.push(NativeEntryPlan {
                symbol: symbol.clone(),
                method_symbol: crate::internal_link_symbol(&method.symbol),
                family: entry.witness.family.canonical_name().to_owned(),
                receiver_type,
                factory_symbol: crate::internal_link_symbol(&factory.symbol),
                factory_receiver: bootstrap,
                factory_request_type,
                request_type,
                result_type,
            });
            let unit = input
                .typed_program()
                .assembly
                .units
                .iter()
                .find(|unit| SourceUnitId::new(input.database(), unit.path.clone()) == entry.method.unit)
                .context("native source unit is absent")?;
            let proof = input
                .typed_program()
                .assembly
                .verified_package_identities
                .for_source(&unit.path)
                .context("native source lacks prepared package proof")?;
            input.typed_program().assembly.verified_package_identities.validate_source(&unit.path, &unit.source)?;
            let source_file = proof
                .project_relative_source_path(&unit.path)
                .context("native source path is ambiguous or outside owning project")?;
            let identity = NativeModCallableIdentity {
                source_file: format!("sources/{source_file}"),
                source_sha256: format!("{:x}", Sha256::digest(unit.source.as_bytes())),
                generation: entry.method.generation.0,
                node: entry.method.node.0,
                internal_symbol: method.symbol.clone(),
                link_symbol: symbol.clone(),
                signature_sha256: format!("{:x}", Sha256::digest(format!("{method_signature:?}").as_bytes())),
                contract_family: entry.witness.family.canonical_name().to_owned(),
                sdk_request_layout_sha256: format!(
                    "{:x}",
                    Sha256::digest(format!("{:?}", method_signature.parameters()[1]).as_bytes())
                ),
                sdk_result_layout_sha256: format!(
                    "{:x}",
                    Sha256::digest(format!("{:?}", method_signature.result()).as_bytes())
                ),
            };
            entries.push(PreparedNativeModEntry {
                registration: ContractRegistration {
                    contract_id: format!("Beskid.Compiler.Collect.{}", entry.witness.family.canonical_name()),
                    type_id: beskid_queries::native_mod_transport_nominal(
                        input.database(),
                        entry.method,
                        &method_signature.parameters()[0],
                    )
                    .with_context(|| format!(
                        "native Mod phase: transport nominal query for owner at {}",
                        beskid_queries::format_ast_node_site(input.database(), entry.method)
                    ))?
                    .context("native owner projection absent")?
                    .name()
                    .to_owned(),
                    entry_symbol: symbol,
                },
                identity,
            });
        }
        let mut callbacks = Vec::new();
        for issued in callbacks_issued {
            let key = issued.wrapper();
            let item = items.iter().find(|item| item.key == key).context("native callback wrapper is not lowered")?;
            let signature =
                native_mod_transport_signature(input.database(), key)
                    .with_context(|| format!(
                        "native Mod phase: transport signature query for callback wrapper at {}",
                        beskid_queries::format_ast_node_site(input.database(), key)
                    ))?
                    .context("native callback signature absent")?;
            callbacks.push(NativeCallbackPlan {
                symbol: beskid_queries::native_mod_callback_symbol(input.database(), issued).with_context(|| {
                    format!(
                        "native Mod phase: transport callback symbol query at {}",
                        beskid_queries::format_ast_node_site(input.database(), key)
                    )
                })?,
                operation: match issued.operation() {
                    beskid_queries::NativeModCallbackOperation::PlanCanonicalPaths => {
                        "__mod_semantic_plan_canonical_paths".into()
                    }
                    beskid_queries::NativeModCallbackOperation::ResolveFunction => {
                        "__mod_semantic_resolve_function".into()
                    }
                    beskid_queries::NativeModCallbackOperation::CheckSerializable => {
                        "__mod_semantic_check_serializable".into()
                    }
                    beskid_queries::NativeModCallbackOperation::ResolveSyntaxTemplate => {
                        "__mod_semantic_resolve_syntax_template".into()
                    }
                    beskid_queries::NativeModCallbackOperation::ResolveSyntaxType => {
                        "__mod_semantic_resolve_syntax_type".into()
                    }
                    beskid_queries::NativeModCallbackOperation::ResolveType => "__mod_semantic_resolve_type".into(),
                    beskid_queries::NativeModCallbackOperation::TypeShape => "__mod_semantic_type_shape".into(),
                    beskid_queries::NativeModCallbackOperation::CaptureCatchall => {
                        "__mod_semantic_capture_catchall".into()
                    }
                    beskid_queries::NativeModCallbackOperation::ValidateCatchall => {
                        "__mod_semantic_validate_catchall".into()
                    }
                    beskid_queries::NativeModCallbackOperation::Query => format!(
                        "__mod_query_{}",
                        beskid_queries::item_name(input.database(), key)
                            .with_context(|| format!(
                                "native Mod phase: callback item name query failed at {}",
                                beskid_queries::format_ast_node_site(input.database(), key)
                            ))?
                            .context("native callback operation absent")?
                    ),
                },
                parameters: signature
                    .parameters()
                    .iter()
                    .map(|ty| layouts.issue(&input, key, ty))
                    .collect::<anyhow::Result<Vec<_>>>()?,
                result_type: layouts.issue(&input, key, signature.result())?,
            });
        }
        for ty in &layouts.types {
            if let NativeAdapterKind::Array { request_getter, .. } = &ty.kind {
                let plan = layouts
                    .arrays
                    .iter()
                    .find(|plan| plan.allocation_request_symbol.ends_with(&format!("_{}", ty.id)))
                    .context("native array metadata pairing absent")?;
                super::data_getters::append_request_getter(
                    &mut artifact,
                    isa,
                    request_getter,
                    &plan.allocation_request_symbol,
                )?;
            }
        }
        artifact.array_static_plans.extend(layouts.arrays);
        let runtime = NativeRuntimeHooks {
            string_construct: "str_new".into(),
            string_length: "str_len".into(),
            string_data_offset: 0,
            root: "gc_root_handle".into(),
            resolve_root: "gc_resolve_handle".into(),
            unroot: "gc_unroot_handle".into(),
            array_allocate_rooted: "beskid_rt_v5_array_allocate_rooted".into(),
            construction_finish: "beskid_rt_v5_array_construction_finish".into(),
            record_allocate: "beskid_rt_v5_managed_object_allocate".into(),
            array_write_barrier: "beskid_rt_v5_array_write_barrier".into(),
            write_barrier: "gc_write_barrier".into(),
        };
        let plan = NativeAdapterPlan {
            syntax_types: layouts.syntax_types,
            pointer_bytes: input.target().pointer_width / 8,
            discriminator_symbol: "beskid_mod_v2_abi".into(),
            types: layouts.types,
            constructors,
            entries: planned_entries,
            callbacks,
            runtime,
        };
        seal(artifact, input.target().clone(), plan, entries)
    })
}
fn bootstrap_factory(
    input: &crate::CodegenInput<'_>,
    items: &[crate::SyntaxModuleItem],
    artifact: &mut CodegenArtifact,
    isa: &dyn cranelift_codegen::isa::TargetIsa,
    owner: beskid_queries::AstNodeKey,
    layouts: &mut super::transport_layout::TransportLayouts,
    active: &mut Vec<beskid_queries::AstNodeKey>,
) -> anyhow::Result<super::adapter_plan::NativeReceiverBootstrap> {
    use super::adapter_plan::NativeReceiverBootstrap;
    use anyhow::{Context, bail};
    if active.len() >= 128 || active.contains(&owner) {
        bail!("native instance factory bootstrap cycle or depth limit");
    }
    if let Some(plan) = input.native_empty_factory_plan(owner) {
        let getter = format!(
            "__beskid_mod_factory_request_u{}_g{}_n{}",
            owner
                .unit
                .path(input.database())
                .to_string_lossy()
                .as_bytes()
                .iter()
                .fold(0u64, |hash, byte| hash.wrapping_mul(131).wrapping_add(u64::from(*byte))),
            owner.generation.0,
            owner.node.0
        );
        if !artifact.functions.iter().any(|function| function.name == getter) {
            super::data_getters::append_request_getter(artifact, isa, &getter, &plan.allocation_request_symbol)?;
            artifact.aggregate_static_plans.push(plan);
        }
        return Ok(NativeReceiverBootstrap::EmptyRecord { request_getter: getter });
    }
    active.push(owner);
    let mut pending = input.roots().to_vec();
    let mut candidates = Vec::new();
    let mut visited = std::collections::HashSet::new();
    while let Some(key) = pending.pop() {
        if !visited.insert(key) {
            continue;
        }
        if visited.len() > 1_000_000 {
            bail!("native factory bootstrap discovery limit");
        }
        if beskid_queries::node_kind(input.database(), key)
            .with_context(|| format!(
                "native factory bootstrap: node kind query failed at {}",
                beskid_queries::format_ast_node_site(input.database(), key)
            ))?
            == Some(beskid_queries::IndexedNodeKind::TypeDefinition)
        {
            candidates.push(key);
        }
        if let Some(children) = beskid_queries::child_nodes(input.database(), key).with_context(|| {
            format!(
                "native factory bootstrap: child nodes query failed at {}",
                beskid_queries::format_ast_node_site(input.database(), key)
            )
        })? {
            pending.extend(children.iter().copied());
        }
    }
    let witness = super::contracts::select_instance_factory(input, owner, &candidates).with_context(|| {
        format!(
            "native factory bootstrap: instance factory selection failed at {}",
            beskid_queries::format_ast_node_site(input.database(), owner)
        )
    })?;
    if witness.methods.len() != 1 {
        bail!("native nested factory lacks one exact callable");
    }
    let method = witness.methods[0].1;
    let item = items.iter().find(|item| item.key == method).context("native nested factory method not lowered")?;
    let signature = beskid_queries::native_mod_transport_signature(input.database(), method)
        .with_context(|| format!(
            "native factory bootstrap: transport signature query failed at {}",
            beskid_queries::format_ast_node_site(input.database(), method)
        ))?
        .context("native nested factory signature unavailable")?;
    if signature.parameters().len() != 2 {
        bail!("native nested factory requires initialized receiver and CollectRequest");
    }
    let receiver = bootstrap_factory(input, items, artifact, isa, witness.owner, layouts, active)?;
    let request = layouts.issue(input, method, &signature.parameters()[1])?;
    active.pop();
    Ok(NativeReceiverBootstrap::SourceCall {
        symbol: crate::internal_link_symbol(&item.symbol),
        arguments: vec![receiver, NativeReceiverBootstrap::FactoryRequest { type_id: request }],
    })
}

fn describe_sites(db: &dyn beskid_queries::Db, keys: &[beskid_queries::AstNodeKey]) -> String {
    keys.iter().map(|key| beskid_queries::format_ast_node_site(db, *key)).collect::<Vec<_>>().join(", ")
}
