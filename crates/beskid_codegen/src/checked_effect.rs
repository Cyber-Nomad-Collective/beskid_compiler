//! Finite, generation-bound no-yield closure proof. This grants no allocator or layout authority.
use crate::{CodegenInput, isle_adapter::SyntaxNodeFacts};
use beskid_isle::{CallKind, CollectionOperation, DirectCallee, NodeFacts, RuntimeIntrinsicKind};
use beskid_queries::{AstNodeKey, GenericSpecializationInstance, IndexedNodeKind, child_nodes, item_body, node_kind};
use cranelift_codegen::isa::TargetIsa;
use std::collections::{HashMap, HashSet};

/// No public constructor or deserializer: only exact registered executable bodies issue a plan.
#[derive(Debug, Clone)]
pub struct CheckedEffectClosure {
    entry: DirectCallee,
    members: HashMap<DirectCallee, CheckedEffectItem>,
    visited_nodes: usize,
    providers: HashSet<&'static str>,
}
#[derive(Debug, Clone)]
pub struct CheckedEffectItem {
    key: AstNodeKey,
    specialization: Option<GenericSpecializationInstance>,
    bridge: Option<CheckedDynamicBridge>,
    owned_bridge: Option<&'static str>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
enum CheckedDynamicBridge {
    Pack(GenericSpecializationInstance, beskid_queries::DynamicPackingBridge),
    Unpack(GenericSpecializationInstance, beskid_queries::DynamicUnpackingBridge),
}
impl CheckedDynamicBridge {
    fn factories(&self) -> [(AstNodeKey, Option<GenericSpecializationInstance>); 2] {
        match self {
            Self::Pack(_, b) => [(b.result_factory(), None), (b.invalid_factory(), None)],
            Self::Unpack(_, b) => [
                (b.result_factory().declaration, Some(b.result_factory().clone())),
                (b.invalid_factory().declaration, Some(b.invalid_factory().clone())),
            ],
        }
    }
}
fn identity(key: AstNodeKey, instance: &Option<GenericSpecializationInstance>) -> DirectCallee {
    match instance {
        Some(value) if !value.substitutions.is_empty() || !value.contract_witnesses.is_empty() => {
            DirectCallee::specialized_item(key, beskid_queries::generic_specialization_identity(value))
        }
        _ => DirectCallee::item(key),
    }
}
impl CheckedEffectItem {
    pub fn key(&self) -> AstNodeKey {
        self.key
    }
    pub fn specialization(&self) -> Option<&GenericSpecializationInstance> {
        self.specialization.as_ref()
    }
}
impl CheckedEffectClosure {
    pub fn entry(&self) -> &DirectCallee {
        &self.entry
    }
    pub fn item(&self, callee: &DirectCallee) -> Option<&CheckedEffectItem> {
        self.members.get(callee)
    }
    pub fn members(&self) -> impl Iterator<Item = (&DirectCallee, &CheckedEffectItem)> {
        self.members.iter()
    }
    /// Source-issued leaf services needed by this exact current finite closure.
    pub fn required_providers(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.providers.iter().copied()
    }
    pub fn visited_nodes(&self) -> usize {
        self.visited_nodes
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedEffectRejection {
    pub site: AstNodeKey,
    pub reason: &'static str,
}

/// Checked helpers remain private native entries: calling without an admitted outer scope
/// traps. This artifact is not a public recoverable callable or a reservation handle.
pub struct EmittedCheckedClosure {
    entry: cranelift_module::FuncId,
    members: HashMap<DirectCallee, cranelift_module::FuncId>,
    rollback: Option<cranelift_module::FuncId>,
}
impl EmittedCheckedClosure {
    pub(crate) fn rollback(&self) -> Option<cranelift_module::FuncId> {
        self.rollback
    }
    pub fn entry(&self) -> cranelift_module::FuncId {
        self.entry
    }
    pub fn member(&self, callee: &DirectCallee) -> Option<cranelift_module::FuncId> {
        self.members.get(callee).copied()
    }
}

/// Emit after the ordinary single-pass descriptor data publication. All source calls import
/// the exact checked clone table; no ordinary body, externally supplied FuncId or callback can
/// silently escape the effect closure. Reservation and caller publication are separate gates.
pub fn emit_checked_effect_closure<'db, M: cranelift_module::Module>(
    module: &mut M,
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    proof: &CheckedEffectClosure,
    string_interner: &mut dyn beskid_isle::StringInterner,
    runtime_functions: &HashMap<String, cranelift_module::FuncId>,
    namespace: &str,
) -> cranelift_module::ModuleResult<EmittedCheckedClosure> {
    emit_effect_closure(
        module,
        input,
        isa,
        proof,
        string_interner,
        runtime_functions,
        namespace,
        false,
        &HashMap::new(),
    )
}

pub fn emit_nonallocating_publication<'db, M: cranelift_module::Module>(
    module: &mut M,
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    proof: &NonAllocatingPublication,
    string_interner: &mut dyn beskid_isle::StringInterner,
    runtime_functions: &HashMap<String, cranelift_module::FuncId>,
    namespace: &str,
) -> cranelift_module::ModuleResult<EmittedCheckedClosure> {
    let member = proof.closure.item(proof.closure.entry()).expect("private proof entry");
    input.nonallocating_publication(isa, member.key(), member.specialization.clone()).map_err(|error| {
        cranelift_module::ModuleError::Backend(anyhow::anyhow!("publication proof expired: {:?}", error))
    })?;
    emit_effect_closure(
        module,
        input,
        isa,
        &proof.closure,
        string_interner,
        runtime_functions,
        namespace,
        true,
        &HashMap::new(),
    )
}
fn emit_effect_closure<'db, M: cranelift_module::Module>(
    module: &mut M,
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    proof: &CheckedEffectClosure,
    string_interner: &mut dyn beskid_isle::StringInterner,
    runtime_functions: &HashMap<String, cranelift_module::FuncId>,
    namespace: &str,
    nonallocating: bool,
    overrides: &HashMap<DirectCallee, cranelift_module::FuncId>,
) -> cranelift_module::ModuleResult<EmittedCheckedClosure> {
    use cranelift_module::{Linkage, ModuleError};
    let error = |message: String| ModuleError::Backend(anyhow::anyhow!(message));
    let root = proof.item(proof.entry()).ok_or_else(|| error("checked closure entry absent".into()))?;
    let current = input
        .checked_effect_closure(isa, root.key, root.specialization.clone())
        .map_err(|failure| error(format!("checked closure unavailable at {:?}: {}", failure.site, failure.reason)))?;
    if current.providers != proof.providers
        || current.members.len() != proof.members.len()
        || current.members.iter().any(|(key, value)| {
            proof.members.get(key).is_none_or(|prior| {
                prior.key != value.key
                    || prior.specialization != value.specialization
                    || prior.bridge != value.bridge
                    || prior.owned_bridge != value.owned_bridge
            })
        })
    {
        return Err(error("checked closure generation or specialization changed".into()));
    }
    let mut runtime_functions = runtime_functions.clone();
    for symbol in &current.providers {
        ensure_checked_runtime_import(module, input, &mut runtime_functions, symbol)?;
    }
    if current.members.values().any(|member| member.bridge.is_some()) {
        for symbol in
            ["gc_root_handle", "gc_resolve_handle", "gc_unroot_handle", "beskid_rt_v5_checked_scope_failure_reason"]
        {
            ensure_checked_runtime_import(module, input, &mut runtime_functions, symbol)?;
        }
    }
    let mut ordered = current.members().collect::<Vec<_>>();
    ordered.sort_by_key(|(callee, _)| format!("{callee:?}"));
    let mut functions = HashMap::new();
    for (index, (callee, member)) in ordered.iter().enumerate() {
        let signature = match member.specialization() {
            Some(instance) => crate::isle_adapter::mappings::signature_for_item(isa, instance.signature.clone())
                .ok_or_else(|| error("checked specialization ABI unavailable".into()))?,
            None => crate::isle_adapter::syntax_item_signature(input, isa, member.key())
                .map_err(|failure| error(format!("{failure:?}")))?,
        };
        let name = beskid_queries::item_name(input.database(), member.key())
            .ok()
            .flatten()
            .ok_or_else(|| error("checked source callable name unavailable".into()))?;
        let symbol = format!("{namespace}__checked_{index}_{name}");
        let id = module.declare_function(&symbol, Linkage::Local, &signature)?;
        functions.insert((*callee).clone(), id);
    }
    for (callee, id) in overrides {
        if !current.members.contains_key(callee) {
            return Err(error("publication override outside current closure".into()));
        }
        functions.insert(callee.clone(), *id);
    }
    for symbol in &current.providers {
        let id = runtime_functions
            .get(*symbol)
            .copied()
            .ok_or_else(|| error(format!("checked canonical provider unavailable: {symbol}")))?;
        functions.insert(DirectCallee::corelib_service(symbol), id);
    }
    for (callee, member) in ordered {
        if overrides.contains_key(callee) {
            continue;
        }
        let function = if let Some(symbol) = member.owned_bridge {
            emit_checked_owned_bridge(module, input, isa, member, symbol, &runtime_functions)?
        } else if let Some(bridge) = &member.bridge {
            let factories = bridge.factories();
            let valid = functions
                .get(&identity(factories[0].0, &factories[0].1))
                .copied()
                .ok_or_else(|| error("checked Dynamic result factory absent".into()))?;
            let invalid = functions
                .get(&identity(factories[1].0, &factories[1].1))
                .copied()
                .ok_or_else(|| error("checked Dynamic invalid factory absent".into()))?;
            match bridge {
                CheckedDynamicBridge::Pack(instance, proof) => {
                    crate::module_emission::dynamic::emit_checked_pack_bridge(
                        module,
                        input,
                        isa,
                        instance,
                        proof,
                        valid,
                        invalid,
                        &runtime_functions,
                    )?
                }
                CheckedDynamicBridge::Unpack(instance, proof) => {
                    crate::module_emission::dynamic::emit_checked_unpack_bridge(
                        module,
                        input,
                        isa,
                        instance,
                        proof,
                        valid,
                        invalid,
                        &runtime_functions,
                    )?
                }
            }
        } else {
            let mut importer = crate::isle_adapter::ItemModuleImporter::new(module, functions.clone());
            crate::isle_adapter::emit_isle_checked_effect_item(
                input,
                isa,
                member,
                string_interner,
                &mut importer,
                nonallocating,
            )
            .map_err(|failure| error(format!("checked body emission failed: {failure:?}")))?
        };
        let mut context = module.make_context();
        context.func = function;
        for (_, external) in context.func.dfg.ext_funcs.iter() {
            if let cranelift_codegen::ir::ExternalName::TestCase(name) = &external.name {
                let name = String::from_utf8_lossy(name.raw());
                ensure_checked_runtime_import(module, input, &mut runtime_functions, name.as_ref())?;
                let id = runtime_functions
                    .get(name.as_ref())
                    .ok_or_else(|| error(format!("checked provider import unavailable: {name}")))?;
                let signature = &context.func.dfg.signatures[external.signature];
                if &module.declarations().get_function_decl(*id).signature != signature {
                    return Err(error(format!("checked provider ABI mismatch: {name}")));
                }
            }
        }
        crate::cranelift_host::remap_testcase_externals(module, &mut context, &runtime_functions)
            .map_err(|failure| error(format!("checked provider remapping failed: {failure}")))?;
        module.define_function(functions[callee], &mut context)?;
    }
    Ok(EmittedCheckedClosure { entry: functions[proof.entry()], members: functions, rollback: None })
}

impl CodegenInput<'_> {
    /// A closed recursive graph is finite; recursion itself does not invalidate no-yield.
    /// Open callbacks, unproved provider calls, arbitrary CLIF and scoped cleanup are denied.
    pub fn checked_effect_closure(
        &self,
        isa: &dyn TargetIsa,
        entry: AstNodeKey,
        specialization: Option<GenericSpecializationInstance>,
    ) -> Result<CheckedEffectClosure, CheckedEffectRejection> {
        const MAX_ITEMS: usize = 65_536;
        const MAX_NODES: usize = 1_048_576;
        let reject = |site, reason| CheckedEffectRejection { site, reason };
        if specialization.as_ref().is_some_and(|value| value.declaration != entry) {
            return Err(reject(entry, "foreign entry specialization"));
        }
        let identity = |key, instance: &Option<GenericSpecializationInstance>| match instance {
            Some(value) if !value.substitutions.is_empty() || !value.contract_witnesses.is_empty() => {
                DirectCallee::specialized_item(key, beskid_queries::generic_specialization_identity(value))
            }
            _ => DirectCallee::item(key),
        };
        let root = identity(entry, &specialization);
        let mut pending =
            vec![(root.clone(), CheckedEffectItem { key: entry, specialization, bridge: None, owned_bridge: None })];
        let mut members = HashMap::new();
        let mut visited_nodes = 0usize;
        let mut providers = HashSet::new();
        let mut bridges: HashMap<DirectCallee, CheckedDynamicBridge> = HashMap::new();
        while let Some((callee, mut item)) = pending.pop() {
            if members.contains_key(&callee) {
                continue;
            }
            if members.len() >= MAX_ITEMS {
                return Err(reject(item.key, "effect closure item limit"));
            }
            if let Some(bridge) = bridges.get(&callee).cloned() {
                for (factory, specialization) in bridge.factories() {
                    pending.push((
                        identity(factory, &specialization),
                        CheckedEffectItem { key: factory, specialization, bridge: None, owned_bridge: None },
                    ));
                }
                item.bridge = Some(bridge);
                members.insert(callee, item);
                continue;
            }
            if let Some(pack) = &item.specialization {
                if let Some(bridge) = beskid_queries::dynamic_packing_bridge(self.database(), pack)
                    .map_err(|_| reject(item.key, "canonical Pack bridge proof unavailable"))?
                {
                    let target = identity(bridge.instance().declaration, &Some(bridge.instance().clone()));
                    if bridges.insert(target, CheckedDynamicBridge::Pack(pack.clone(), bridge)).is_some() {
                        return Err(reject(item.key, "duplicate checked Pack bridge instance"));
                    }
                }
            }
            if let Some(unpack) = &item.specialization {
                if let Some(bridge) = beskid_queries::dynamic_unpacking_bridge(self.database(), unpack)
                    .map_err(|_| reject(item.key, "canonical Unpack bridge proof unavailable"))?
                {
                    let target = identity(bridge.instance().declaration, &Some(bridge.instance().clone()));
                    if bridges.insert(target, CheckedDynamicBridge::Unpack(unpack.clone(), bridge)).is_some() {
                        return Err(reject(item.key, "duplicate checked Unpack bridge instance"));
                    }
                }
            }
            let facts = match &item.specialization {
                Some(instance) => SyntaxNodeFacts::new_with_item_specialization(self, isa, item.key, instance.clone()),
                None => SyntaxNodeFacts::new_with_isa(self, isa),
            };
            if let Some(symbol) = self.canonical_dynamic_owned_bridge(item.key) {
                item.owned_bridge = Some(symbol);
                providers.insert(symbol);
                members.insert(callee, item);
                continue;
            }
            let body = item_body(self.database(), item.key)
                .ok()
                .flatten()
                .ok_or_else(|| reject(item.key, "missing executable body"))?;
            let mut nodes = vec![body];
            let mut seen = HashSet::new();
            while let Some(site) = nodes.pop() {
                if !seen.insert(site) {
                    continue;
                }
                visited_nodes = visited_nodes
                    .checked_add(1)
                    .filter(|value| *value <= MAX_NODES)
                    .ok_or_else(|| reject(site, "effect closure node limit"))?;
                let kind = node_kind(self.database(), site)
                    .ok()
                    .flatten()
                    .ok_or_else(|| reject(site, "missing generation-bound node"))?;
                if facts.event_operation(site).is_some()
                    || matches!(
                        facts.assignment_kind(site),
                        Some(
                            beskid_isle::AssignmentKind::EventSubscribe
                                | beskid_isle::AssignmentKind::EventUnsubscribeFirst
                        )
                    )
                {
                    return Err(reject(site, "event publication lacks closed callback effect"));
                }
                match kind {
                    IndexedNodeKind::SpawnExpression
                    | IndexedNodeKind::ClifBlockExpression
                    | IndexedNodeKind::ScopedUseStatement
                    | IndexedNodeKind::LaunchStatement
                    | IndexedNodeKind::WithStatement
                    | IndexedNodeKind::LambdaExpression => {
                        return Err(reject(site, "unproved yield, callback or cleanup effect"));
                    }
                    IndexedNodeKind::CallExpression => match facts.call_kind(site) {
                        Some(CallKind::PrimitiveNumericConversion | CallKind::TypedArrayAllocation) => {}
                        Some(CallKind::CollectionOperation) => {
                            if matches!(
                                facts.collection_operation(site),
                                None | Some(CollectionOperation::UnprovenMutationOwner)
                            ) {
                                return Err(reject(site, "unproved collection owner"));
                            }
                        }
                        Some(CallKind::RuntimeIntrinsic) => match facts.runtime_intrinsic_kind(site) {
                            Some(
                                RuntimeIntrinsicKind::SchedulerFiberEntryAddress
                                | RuntimeIntrinsicKind::SchedulerReturnTrampolineAddress
                                | RuntimeIntrinsicKind::SchedulerPollEntryInvoke,
                            )
                            | None => return Err(reject(site, "scheduler or unknown intrinsic effect")),
                            Some(_) => {}
                        },
                        Some(CallKind::Direct | CallKind::Bulk) => {
                            let target =
                                facts.direct_callee(site).ok_or_else(|| reject(site, "missing exact callee"))?;
                            let declaration = match &target {
                                DirectCallee::Item(key) => *key,
                                DirectCallee::SpecializedItem { declaration, .. } => *declaration,
                                DirectCallee::CorelibService(symbol) => {
                                    let provider = beskid_queries::checked_provider_call(self.database(), site)
                                        .ok()
                                        .flatten()
                                        .filter(|proof| proof.call() == site && proof.symbol() == *symbol)
                                        .ok_or_else(|| reject(site, "provider lacks exact no-yield witness"))?;
                                    providers.insert(provider.symbol());
                                    let children = child_nodes(self.database(), site)
                                        .ok()
                                        .flatten()
                                        .ok_or_else(|| reject(site, "missing current argument graph"))?;
                                    nodes.extend(children.iter().copied());
                                    continue;
                                }
                                _ => return Err(reject(site, "provider or callback lacks no-yield witness")),
                            };
                            let instance = facts.generic_call_specialization_in_context(site);
                            if identity(declaration, &instance) != target {
                                return Err(reject(site, "callee specialization identity mismatch"));
                            }
                            pending.push((
                                target,
                                CheckedEffectItem {
                                    key: declaration,
                                    specialization: instance,
                                    bridge: None,
                                    owned_bridge: None,
                                },
                            ));
                        }
                        _ => return Err(reject(site, "open or unresolved call effect")),
                    },
                    _ => {}
                }
                let children = child_nodes(self.database(), site)
                    .ok()
                    .flatten()
                    .ok_or_else(|| reject(site, "missing current child graph"))?;
                nodes.extend(children.iter().copied());
            }
            members.insert(callee, item);
        }
        Ok(CheckedEffectClosure { entry: root, members, visited_nodes, providers })
    }
}

/// Stronger source proof for publication/rollback. This is not issued from an Encoder
/// contract name: the concrete specialized method and every reachable body are examined.
/// Its emitter may omit root registration only because no allocation, collection, callback
/// or yield can run while the caller retains receiver/argument roots.
#[derive(Debug, Clone)]
pub struct NonAllocatingPublication {
    closure: CheckedEffectClosure,
}
impl NonAllocatingPublication {
    pub fn closure(&self) -> &CheckedEffectClosure {
        &self.closure
    }
}
impl CodegenInput<'_> {
    pub fn nonallocating_publication(
        &self,
        isa: &dyn TargetIsa,
        entry: AstNodeKey,
        specialization: Option<GenericSpecializationInstance>,
    ) -> Result<NonAllocatingPublication, CheckedEffectRejection> {
        use beskid_isle::{LiteralKind, OperatorFact};
        let closure = self.checked_effect_closure(isa, entry, specialization)?;
        for (_, item) in closure.members() {
            if item.bridge.is_some() {
                return Err(CheckedEffectRejection {
                    site: item.key,
                    reason: "Pack erasure constructs a managed result",
                });
            }
            let facts = match item.specialization() {
                Some(instance) => {
                    SyntaxNodeFacts::new_with_item_specialization(self, isa, item.key(), instance.clone())
                }
                None => SyntaxNodeFacts::new_with_isa(self, isa),
            };
            let reject = |site, reason| CheckedEffectRejection { site, reason };
            let mut pending = vec![
                item_body(self.database(), item.key())
                    .ok()
                    .flatten()
                    .ok_or_else(|| reject(item.key(), "publication body unavailable"))?,
            ];
            let mut seen = HashSet::new();
            while let Some(site) = pending.pop() {
                if !seen.insert(site) {
                    continue;
                }
                if matches!(
                    node_kind(self.database(), site).ok().flatten(),
                    Some(
                        IndexedNodeKind::StructLiteralExpression
                            | IndexedNodeKind::EnumConstructorExpression
                            | IndexedNodeKind::ArrayLiteralExpression
                    )
                ) || facts.literal_kind(site) == Some(LiteralKind::String)
                    || facts.operator_fact(site) == Some(OperatorFact::StringAdd)
                {
                    return Err(reject(site, "publication constructs a managed value"));
                }
                if matches!(facts.call_kind(site), Some(CallKind::TypedArrayAllocation | CallKind::Bulk)) {
                    return Err(reject(site, "publication allocates an array"));
                }
                if facts.call_kind(site) == Some(CallKind::CollectionOperation)
                    && !matches!(
                        facts.collection_operation(site),
                        Some(
                            CollectionOperation::Capacity
                                | CollectionOperation::Clear
                                | CollectionOperation::RemoveLast
                        )
                    )
                {
                    return Err(reject(site, "publication can grow a collection"));
                }
                if facts.call_kind(site) == Some(CallKind::RuntimeIntrinsic) {
                    // Raw memory writes and allocation/GC hooks are not publication capabilities.
                    return Err(reject(site, "publication invokes an intrinsic without nonallocating authority"));
                }
                if let Some(DirectCallee::CorelibService(symbol)) = facts.direct_callee(site) {
                    if !matches!(
                        symbol,
                        "__gc_same_identity"
                            | "__array_len"
                            | "__str_len"
                            | "__float_to_bits32"
                            | "__float_from_bits32"
                            | "__float_to_bits64"
                            | "__float_from_bits64"
                    ) {
                        return Err(reject(site, "publication provider may allocate"));
                    }
                }
                pending.extend(
                    child_nodes(self.database(), site)
                        .ok()
                        .flatten()
                        .ok_or_else(|| reject(site, "publication child facts unavailable"))?
                        .iter()
                        .copied(),
                );
            }
        }
        Ok(NonAllocatingPublication { closure })
    }
}

/// Exact concrete publication bodies selected by canonical Encode's applied E witness.
/// This plan must be consumed by an outer transaction boundary, not inferred from names.
pub struct SerializationPublicationPlan {
    encode: GenericSpecializationInstance,
    commit: NonAllocatingPublication,
    abort: NonAllocatingPublication,
}
impl SerializationPublicationPlan {
    pub fn encode(&self) -> &GenericSpecializationInstance {
        &self.encode
    }
    pub fn commit(&self) -> &NonAllocatingPublication {
        &self.commit
    }
    pub fn abort(&self) -> &NonAllocatingPublication {
        &self.abort
    }
}
impl CodegenInput<'_> {
    pub fn serialization_publication_plan(
        &self,
        isa: &dyn TargetIsa,
        instance: GenericSpecializationInstance,
    ) -> Result<SerializationPublicationPlan, CheckedEffectRejection> {
        let reject = |site, reason| CheckedEffectRejection { site, reason };
        let (commit, abort) = beskid_queries::serialization_publication_methods(self.database(), &instance)
            .ok()
            .flatten()
            .ok_or_else(|| reject(instance.declaration, "canonical applied Encoder publication witness unavailable"))?;
        let facts = SyntaxNodeFacts::new_with_item_specialization(self, isa, instance.declaration, instance.clone());
        let body = item_body(self.database(), instance.declaration)
            .ok()
            .flatten()
            .ok_or_else(|| reject(instance.declaration, "canonical Encode body unavailable"))?;
        let mut pending = vec![body];
        let mut seen = HashSet::new();
        let mut commit_proofs = Vec::new();
        let mut abort_proofs = Vec::new();
        while let Some(site) = pending.pop() {
            if !seen.insert(site) {
                continue;
            }
            if let Some(target) = facts.direct_callee(site) {
                let declaration = match target {
                    DirectCallee::Item(key) | DirectCallee::SpecializedItem { declaration: key, .. } => Some(key),
                    _ => None,
                };
                if declaration == Some(commit) || declaration == Some(abort) {
                    let specialization = facts.generic_call_specialization_in_context(site);
                    let proof = self.nonallocating_publication(isa, declaration.unwrap(), specialization)?;
                    if declaration == Some(commit) {
                        commit_proofs.push(proof);
                    } else {
                        abort_proofs.push(proof);
                    }
                }
            }
            pending.extend(
                child_nodes(self.database(), site)
                    .ok()
                    .flatten()
                    .ok_or_else(|| reject(site, "canonical Encode child facts unavailable"))?
                    .iter()
                    .copied(),
            );
        }
        if commit_proofs.len() != 1 || abort_proofs.len() != 3 {
            return Err(reject(
                instance.declaration,
                "canonical Encode publication calls do not match applied witnesses",
            ));
        }
        let commit = commit_proofs.remove(0);
        let abort = abort_proofs.remove(0);
        if abort_proofs.iter().any(|other| other.closure.entry() != abort.closure.entry()) {
            return Err(reject(instance.declaration, "rollback specialization differs across failure edges"));
        }
        Ok(SerializationPublicationPlan { encode: instance, commit, abort })
    }
}

/// Canonical Encode receives allocation-free Commit/Abort clones, and an outer rollback
/// target for sticky failure before its own match arms could execute.
pub fn emit_serialization_checked_closure<'db, M: cranelift_module::Module>(
    module: &mut M,
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    instance: GenericSpecializationInstance,
    string_interner: &mut dyn beskid_isle::StringInterner,
    runtime_functions: &HashMap<String, cranelift_module::FuncId>,
    namespace: &str,
) -> cranelift_module::ModuleResult<EmittedCheckedClosure> {
    let publication = input
        .serialization_publication_plan(isa, instance.clone())
        .map_err(|error| cranelift_module::ModuleError::Backend(anyhow::anyhow!("{:?}", error)))?;
    let effect = input
        .checked_effect_closure(isa, instance.declaration, Some(instance))
        .map_err(|error| cranelift_module::ModuleError::Backend(anyhow::anyhow!("{:?}", error)))?;
    let commit = emit_nonallocating_publication(
        module,
        input,
        isa,
        publication.commit(),
        string_interner,
        runtime_functions,
        &format!("{namespace}__commit"),
    )?;
    let abort = emit_nonallocating_publication(
        module,
        input,
        isa,
        publication.abort(),
        string_interner,
        runtime_functions,
        &format!("{namespace}__abort"),
    )?;
    let overrides = HashMap::from([
        (publication.commit.closure.entry().clone(), commit.entry()),
        (publication.abort.closure.entry().clone(), abort.entry()),
    ]);
    let mut emitted = emit_effect_closure(
        module,
        input,
        isa,
        &effect,
        string_interner,
        runtime_functions,
        namespace,
        false,
        &overrides,
    )?;
    emitted.rollback = Some(abort.entry());
    Ok(emitted)
}

/// Declare only imports selected by a reissued checked closure or its actual emitted CLIF.
/// Signatures come from the current ABI catalog, never from the proposed CLIF signature.
pub(crate) fn ensure_checked_runtime_import<M: cranelift_module::Module>(
    module: &mut M,
    input: &CodegenInput<'_>,
    functions: &mut HashMap<String, cranelift_module::FuncId>,
    symbol: &str,
) -> cranelift_module::ModuleResult<cranelift_module::FuncId> {
    use beskid_abi::abi_v5::AbiType;
    use cranelift_codegen::ir::{AbiParam, Signature, types};
    use cranelift_module::{Linkage, ModuleError};
    let error = |reason: &str| ModuleError::Backend(anyhow::anyhow!("{reason}: {symbol}"));
    let pointer = module.target_config().pointer_type();
    let conv = module.isa().default_call_conv();
    let signature = if let Some(export) = input.abi_manifest().exports.iter().find(|export| export.symbol == symbol) {
        let scalar = |ty: AbiType| match ty {
            AbiType::Pointer | AbiType::USize | AbiType::ISize => Some(pointer),
            AbiType::I8 | AbiType::U8 => Some(types::I8),
            AbiType::I16 | AbiType::U16 => Some(types::I16),
            AbiType::I32 | AbiType::U32 => Some(types::I32),
            AbiType::I64 | AbiType::U64 => Some(types::I64),
            AbiType::F32 => Some(types::F32),
            AbiType::F64 => Some(types::F64),
            AbiType::V128 | AbiType::Void => None,
        };
        let mut signature = Signature::new(conv);
        for ty in &export.params {
            signature
                .params
                .push(AbiParam::new(scalar(*ty).ok_or_else(|| error("unsupported canonical runtime parameter ABI"))?));
        }
        if export.result != AbiType::Void {
            signature.returns.push(AbiParam::new(
                scalar(export.result).ok_or_else(|| error("unsupported canonical runtime result ABI"))?,
            ));
        }
        signature
    } else if let Some(spec) = beskid_abi::all_builtin_specs().into_iter().find(|spec| spec.symbol == symbol) {
        crate::cranelift_host::builtin_signature(pointer, conv, spec.params, spec.returns)
    } else {
        return Err(error("checked runtime import absent from canonical ABI catalog"));
    };
    if let Some(id) = functions.get(symbol).copied() {
        if module.get_name(symbol) != Some(cranelift_module::FuncOrDataId::Func(id))
            || module.declarations().get_function_decl(id).signature != signature
        {
            return Err(error("checked runtime import differs from canonical ABI catalog"));
        }
        return Ok(id);
    }
    let id = module.declare_function(symbol, Linkage::Import, &signature)?;
    functions.insert(symbol.to_owned(), id);
    Ok(id)
}

impl CodegenInput<'_> {
    fn canonical_dynamic_owned_bridge(&self, key: AstNodeKey) -> Option<&'static str> {
        if let Some(proof) = beskid_queries::checked_dynamic_result_bridge(self.database(), key).ok().flatten() {
            if proof.declaration() == key {
                return Some(proof.symbol());
            }
        }
        if self.runtime_intrinsic_capability().is_none()
            || beskid_queries::node_kind(self.database(), key).ok().flatten()
                != Some(IndexedNodeKind::ContractMethodSignature)
        {
            return None;
        }
        let unit = self
            .typed_program()
            .assembly
            .units
            .iter()
            .find(|unit| beskid_queries::SourceUnitId::new(self.database(), unit.path.clone()) == key.unit)?;
        let canonical = beskid_abi::runtime_source::canonical_runtime_sources()
            .into_iter()
            .find(|unit| unit.logical_path == beskid_abi::runtime_source::CANONICAL_DYNAMIC_SOURCE_PATH)?;
        if unit.logical_name != canonical.logical_path || unit.source != canonical.source {
            return None;
        }
        match beskid_queries::item_name(self.database(), key).ok().flatten()?.as_ref() {
            "beskid_dynamic_v1_create_owned" => Some("beskid_dynamic_v1_create_owned"),
            "beskid_dynamic_v1_cast_owned" => Some("beskid_dynamic_v1_cast_owned"),
            "beskid_dynamic_v1_map_owned" => Some("beskid_dynamic_v1_map_owned"),
            _ => None,
        }
    }
}
fn emit_checked_owned_bridge<'db, M: cranelift_module::Module>(
    module: &mut M,
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    member: &CheckedEffectItem,
    symbol: &'static str,
    runtime: &HashMap<String, cranelift_module::FuncId>,
) -> cranelift_module::ModuleResult<cranelift_codegen::ir::Function> {
    use cranelift_codegen::ir::{Function, InstBuilder, UserFuncName};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    let error = |reason: &str| cranelift_module::ModuleError::Backend(anyhow::anyhow!("{reason}"));
    if input.canonical_dynamic_owned_bridge(member.key) != Some(symbol) || member.specialization.is_some() {
        return Err(error("checked Dynamic owned bridge source changed"));
    }
    let signature = crate::isle_adapter::syntax_item_signature(input, isa, member.key)
        .map_err(|_| error("checked Dynamic owned bridge signature unavailable"))?;
    let mut function = Function::with_name_signature(UserFuncName::user(0, 0), signature);
    let provider = *runtime.get(symbol).ok_or_else(|| error("checked Dynamic owned provider import absent"))?;
    let callee = module.declare_func_in_func(provider, &mut function);
    let mut frontend = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut function, &mut frontend);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);
        let arguments = builder.block_params(entry).to_vec();
        let call = builder.ins().call(callee, &arguments);
        let result = builder.inst_results(call).to_vec();
        builder.ins().return_(&result);
    }
    Ok(function)
}

pub(crate) fn emit_artifact_checked_effect_item<'db>(
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    member: &CheckedEffectItem,
    strings: &mut dyn beskid_isle::StringInterner,
    importer: &mut dyn beskid_isle::CallImporter,
) -> Result<cranelift_codegen::ir::Function, String> {
    if let Some(symbol) = member.owned_bridge {
        use cranelift_codegen::ir::{ExtFuncData, ExternalName, Function, InstBuilder, UserFuncName};
        use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
        if input.canonical_dynamic_owned_bridge(member.key) != Some(symbol) {
            return Err("checked owned bridge source authority changed".into());
        }
        let signature =
            crate::isle_adapter::syntax_item_signature(input, isa, member.key).map_err(|e| format!("{e:?}"))?;
        let mut function = Function::with_name_signature(UserFuncName::user(0, 0), signature.clone());
        let signature_ref = function.import_signature(signature);
        let callee = function.import_function(ExtFuncData {
            name: ExternalName::testcase(symbol.as_bytes()),
            signature: signature_ref,
            colocated: false,
            patchable: false,
        });
        let mut frontend = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut frontend);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let args = builder.block_params(entry).to_vec();
            let call = builder.ins().call(callee, &args);
            let result = builder.inst_results(call).to_vec();
            builder.ins().return_(&result);
        }
        Ok(function)
    } else {
        crate::isle_adapter::emit_isle_checked_effect_item(input, isa, member, strings, importer, false)
            .map_err(|e| format!("{e:?}"))
    }
}
