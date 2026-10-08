use std::collections::{HashMap, HashSet};

use beskid_isle::{AstNodeKey, DirectCallee, StringInterner};
use beskid_queries::{
    CallLowering, EventOperationKind, call_lowering, child_nodes, event_operation,
    extern_contract_import_for_declaration,
};
use cranelift_codegen::ir::{ExtFuncData, ExternalName, FuncRef, GlobalValueData, InstBuilder, Signature, Type, Value};
use cranelift_frontend::FunctionBuilder;

use super::items::ResolvedSyntaxModuleItem;
use crate::{CodegenContext, CodegenInput, ExternImport};

/// Syntax-ISLE adapter over the existing artifact-owned literal pool.
pub(crate) struct ArtifactStringInterner<'a> {
    pub(crate) context: &'a mut CodegenContext,
    pub(crate) pointer_type: Type,
}

impl StringInterner for ArtifactStringInterner<'_> {
    fn intern(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        _key: AstNodeKey,
        text: &str,
    ) -> Result<Value, beskid_isle::StringMaterializationError> {
        let symbol = self.context.intern_string_literal(text.as_bytes());
        let global = builder.func.create_global_value(GlobalValueData::Symbol {
            name: ExternalName::testcase(symbol),
            offset: 0.into(),
            // JIT code and readonly data use independent allocation arenas. Keep the literal
            // address range-independent instead of promising an AArch64 ADRP-reachable target.
            colocated: false,
            tls: false,
        });
        let bytes = builder.ins().symbol_value(self.pointer_type, global);
        let byte_len = builder.ins().iconst(self.pointer_type, text.len() as i64);
        let mut signature = Signature::new(builder.func.signature.call_conv);
        signature.params.push(cranelift_codegen::ir::AbiParam::new(self.pointer_type));
        signature.params.push(cranelift_codegen::ir::AbiParam::new(self.pointer_type));
        signature.returns.push(cranelift_codegen::ir::AbiParam::new(self.pointer_type));
        let signature = builder.import_signature(signature);
        let function = builder.func.import_function(ExtFuncData {
            name: ExternalName::testcase("str_new"),
            signature,
            colocated: false,
            patchable: false,
        });
        let call = builder.ins().call(function, &[bytes, byte_len]);
        builder
            .inst_results(call)
            .first()
            .copied()
            .ok_or(beskid_isle::StringMaterializationError::Artifact("str_new returned no value"))
    }
}

/// Manifest symbols available only to compiler-authorized canonical runtime source. Ordinary
/// syntax programs never receive these entries, so an unresolved name cannot turn into an extern
/// fallback.
pub(super) fn runtime_intrinsic_symbols(input: &CodegenInput<'_>) -> HashMap<DirectCallee, String> {
    input
        .runtime_intrinsic_capability()
        .map(|_| {
            input
                .abi_manifest()
                .trusted_runtime_intrinsics
                .iter()
                .enumerate()
                .filter_map(|(index, intrinsic)| {
                    let symbol = intrinsic
                        .target_bindings
                        .iter()
                        .find(|binding| binding.target == input.target().triple.as_str())
                        .map(|binding| binding.implementation.clone())
                        .unwrap_or_else(|| intrinsic.symbol.clone());
                    u32::try_from(index).ok().map(|index| (DirectCallee::runtime_intrinsic(index), symbol))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn collect_extern_contract_callees(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    callees: &mut HashMap<DirectCallee, AstNodeKey>,
) {
    if let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, key)
        && extern_contract_import_for_declaration(db, declaration).is_some()
    {
        callees.entry(DirectCallee::item(declaration)).or_insert(declaration);
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_extern_contract_callees(db, child, callees);
        }
    }
}

/// The `bool Available()` query of one optional `[Extern]` contract that the program calls. The
/// compiler defines `symbol` itself; it checks every C symbol in `members`.
pub(super) struct AvailabilityQuery {
    pub(super) symbol: String,
    pub(super) members: Vec<(AstNodeKey, ExternImport)>,
}

/// `[Extern]` contract methods called by the lowered items: the symbol each call imports, the
/// C imports they need, and the availability queries the compiler must define.
pub(super) struct ExternContractCallees {
    pub(super) symbols: HashMap<DirectCallee, String>,
    pub(super) imports: Vec<ExternImport>,
    pub(super) availability: Vec<AvailabilityQuery>,
}

pub(super) fn extern_contract_callees(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> ExternContractCallees {
    let db = input.database();
    let mut callees = HashMap::new();
    for item in items {
        collect_extern_contract_callees(db, item.key, &mut callees);
    }
    let mut ordered = callees.into_iter().collect::<Vec<_>>();
    ordered.sort_by_cached_key(|(_, declaration)| beskid_queries::format_ast_node_key(db, *declaration));
    let mut result =
        ExternContractCallees { symbols: HashMap::new(), imports: Vec::new(), availability: Vec::new() };
    for (callee, declaration) in ordered {
        let Some(import) = extern_contract_import_for_declaration(db, declaration) else {
            continue;
        };
        if import.availability_query {
            let symbol = format!("__beskid_extern_available_{}", result.availability.len());
            let members = beskid_queries::optional_extern_contract_members(db, declaration)
                .unwrap_or_default()
                .into_iter()
                .map(|(key, member)| (key, extern_import(member)))
                .collect();
            result.symbols.insert(callee, symbol.clone());
            result.availability.push(AvailabilityQuery { symbol, members });
            continue;
        }
        let mut import = extern_import(import);
        // An admitted Rust Glue import replaces the declared C symbol and is linked from the
        // owner image, not from the contract's `Library`.
        if let Some(symbol) = input.glue_import_callee_symbol(&callee) {
            import.symbol = symbol.to_owned();
            import.library = None;
        }
        result.symbols.insert(callee, import.symbol.clone());
        if !result.imports.iter().any(|existing: &ExternImport| existing.symbol == import.symbol) {
            result.imports.push(import);
        }
    }
    result
}

fn extern_import(import: beskid_queries::ExternContractImport) -> ExternImport {
    ExternImport { symbol: import.symbol, abi: import.abi, library: import.library, optional: import.optional }
}

/// Authorize `clif { call @symbol(...) }` callees by declaration: a symbol that no lowered
/// function defines and no other import supplies is admitted when some unit declares it as a
/// method of a C-ABI `[Extern(Abi: "C", Library: "...")]` contract. A source call through the
/// contract is not required. Runtime- and host-owned names are never admitted this way.
pub(super) fn clif_declared_extern_imports(
    input: &CodegenInput<'_>,
    functions: &[crate::LoweredFunction],
    known: &[ExternImport],
) -> Vec<ExternImport> {
    let defined = functions.iter().map(|function| function.name.as_str()).collect::<HashSet<_>>();
    let mut unresolved = std::collections::BTreeSet::new();
    for function in functions {
        for (_, external) in function.function.dfg.ext_funcs.iter() {
            let ExternalName::TestCase(name) = &external.name else {
                continue;
            };
            let symbol = String::from_utf8_lossy(name.raw()).into_owned();
            if !defined.contains(symbol.as_str()) && !known.iter().any(|import| import.symbol == symbol) {
                unresolved.insert(symbol);
            }
        }
    }
    if unresolved.is_empty() {
        return Vec::new();
    }
    let db = input.database();
    let mut declarations = HashMap::new();
    for unit in input.typed_program().assembly.units.iter() {
        let unit = beskid_queries::SourceUnitId::new(db, unit.path.clone());
        for import in beskid_queries::extern_contract_declarations_in_unit(db, unit) {
            declarations.entry(import.symbol.clone()).or_insert(import);
        }
    }
    unresolved
        .into_iter()
        .filter_map(|symbol| {
            let import = declarations.remove(&symbol)?;
            let authorized = import.abi.as_deref() == Some("C")
                && import.library.as_deref().is_some_and(|library| !library.is_empty())
                && !beskid_abi::is_runtime_owned_ffi_symbol(&symbol);
            authorized.then(|| extern_import(import))
        })
        .collect()
}

/// String runtime helpers that ISLE lowering emits directly (string literals, coercion,
/// comparison, concatenation). These are always required — they are not gated by the
/// Corelib syscall capability because they are fundamental operations, not facade services.
const ALWAYS_AVAILABLE_STRING_SERVICES: &[&str] = &["str_new", "str_from_i64", "str_eq", "str_concat"];

/// Compiler-planned composition calls are not source-callable Corelib services. Their
/// authority comes from the attached, generation-validated composition graph, and the
/// exact target binding must still be present in the canonical ABI-v5 manifest.
const COMPOSITION_SERVICES: &[&str] = &[
    "composition_container_create",
    "composition_container_drop",
    "composition_launch",
    "composition_scope_enter",
    "composition_scope_leave",
    "composition_shutdown",
    "composition_slot_store",
];

/// ABI symbols admitted by either the distinct Corelib syscall capability or a generated
/// manifest-backed builtin fact. Both use the same exact-symbol import table; neither path may
/// guess a native symbol from source spelling.
///
/// String runtime helpers (`str_new`, `str_from_i64`, `str_eq`, `str_concat`) are always
/// available because ISLE lowering emits them directly for string literals, coercion,
/// comparison, and concatenation — they are not facade services requiring capability authority.
pub(super) fn corelib_service_symbols(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> Result<HashMap<DirectCallee, String>, String> {
    let mut manifest_builtins = HashSet::new();
    let mut corelib_services = HashSet::new();
    let mut event_services = HashSet::new();
    for item in items {
        collect_manifest_service_callees(input.database(), item.key, &mut manifest_builtins, &mut corelib_services);
        collect_event_service_callees(input.database(), item.key, &mut event_services);
    }
    let mut symbols = HashMap::new();
    let mut callback_nodes = HashSet::new();
    let mut pending = items.iter().map(|item| (item.key, 0usize)).collect::<Vec<_>>();
    while let Some((key, depth)) = pending.pop() {
        if depth > 1024 {
            return Err("native callback import traversal exceeds depth budget".into());
        }
        if !callback_nodes.insert(key) {
            continue;
        }
        if callback_nodes.len() > 1_000_000 {
            return Err("native callback import traversal exceeds node budget".into());
        }
        if let Ok(Some(CallLowering::NativeModCallback(callback))) = call_lowering(input.database(), key) {
            if !input.permits_native_mod_callback(callback) {
                return Err(format!(
                    "SDK host callback at {} (wrapper {}) requires a prepared native Mod invocation capability",
                    beskid_queries::format_ast_node_site(input.database(), key),
                    beskid_queries::format_ast_node_site(input.database(), callback.wrapper()),
                ));
            }
            let symbol = beskid_queries::native_mod_callback_symbol(input.database(), callback)
                .map_err(|error| error.to_string())?;
            symbols.insert(DirectCallee::NativeModCallback(callback.wrapper()), symbol);
        }
        if let Ok(Some(children)) = child_nodes(input.database(), key) {
            pending.extend(children.iter().copied().map(|child| (child, depth + 1)));
        }
    }
    for symbol in ALWAYS_AVAILABLE_STRING_SERVICES {
        symbols.insert(DirectCallee::corelib_service(symbol), (*symbol).to_owned());
    }
    if input.composition_authority().is_some() {
        if input.abi_manifest() != &beskid_abi::abi_v5::AbiManifestV5::canonical_runtime(input.target().clone()) {
            return Err("composition requires the exact canonical ABI-v5 manifest".to_owned());
        }
        for symbol in COMPOSITION_SERVICES {
            let bindings = beskid_abi::generated::abi_v5_contract::ABI_V5_CORELIB_SERVICE_BINDINGS
                .iter()
                .filter(|binding| binding.adapter == *symbol && binding.target == input.target().triple.as_str())
                .collect::<Vec<_>>();
            if bindings.len() != 1 || bindings[0].implementation != *symbol {
                return Err(format!("composition service `{symbol}` has no unique exact target binding"));
            }
            symbols.insert(DirectCallee::corelib_service(symbol), (*symbol).to_owned());
        }
    }
    // Event operations are compiler-owned source forms. Their only lowering path is the
    // generated event rule, and CodegenInput has already validated this target's canonical
    // ABI-v5 manifest. Admit only the exact canonical event service shapes here; ordinary
    // source calls still require source-scoped Corelib service capability below.
    use beskid_abi::runtime_source::CorelibServiceAbiType::{Pointer, Usize};
    for (symbol, parameters) in [
        ("event_subscribe", &[Pointer, Pointer, Usize][..]),
        ("event_unsubscribe_first", &[Pointer, Pointer][..]),
        ("event_len", &[Pointer][..]),
        ("event_get_handler", &[Pointer, beskid_abi::runtime_source::CorelibServiceAbiType::U32][..]),
    ]
    .into_iter()
    .filter(|(symbol, _)| event_services.contains(symbol))
    {
        let abi = beskid_abi::runtime_source::canonical_corelib_service_abi_for_adapter(symbol)
            .ok_or_else(|| format!("canonical event service `{symbol}` is unavailable in ABI-v5"))?;
        if abi.parameters != parameters || abi.result != if symbol == "event_get_handler" { Pointer } else { Usize } {
            return Err(format!("canonical event service `{symbol}` has an unexpected ABI-v5 signature"));
        }
        symbols.insert(DirectCallee::corelib_service(symbol), symbol.to_owned());
    }
    for symbol in manifest_builtins {
        if !ALWAYS_AVAILABLE_STRING_SERVICES.contains(&symbol) {
            symbols.insert(DirectCallee::corelib_service(symbol), symbol.to_owned());
        }
    }
    let capability = input.corelib_service_capability();
    if !corelib_services.is_empty() && capability.is_none() {
        return Err("Corelib service call has no source-scoped capability".to_owned());
    }
    if let Some(capability) = capability {
        for service in corelib_services {
            if ALWAYS_AVAILABLE_STRING_SERVICES.contains(&service.symbol) {
                continue;
            }
            beskid_abi::runtime_source::preflight_corelib_service_import(capability, input.abi_manifest(), service)
                .map_err(|error| error.to_string())?;
            symbols.insert(DirectCallee::corelib_service(service.symbol), service.symbol.to_owned());
        }
    }
    Ok(symbols)
}

fn collect_event_service_callees(db: &dyn beskid_queries::Db, key: AstNodeKey, services: &mut HashSet<&'static str>) {
    if let Ok(Some(fact)) = event_operation(db, key)
        && fact.capacity > 0
        && fact.delegate_signature.is_some()
    {
        match fact.operation {
            EventOperationKind::Subscribe => {
                services.insert("event_subscribe");
            }
            EventOperationKind::UnsubscribeFirst => {
                services.insert("event_unsubscribe_first");
            }
            EventOperationKind::Raise => {
                services.insert("event_len");
                services.insert("event_get_handler");
            }
        }
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_event_service_callees(db, child, services);
        }
    }
}

fn collect_manifest_service_callees(
    db: &dyn beskid_queries::Db,
    key: AstNodeKey,
    manifest_builtins: &mut HashSet<&'static str>,
    corelib_services: &mut HashSet<beskid_abi::runtime_source::CorelibService>,
) {
    if let Ok(Some(lowering)) = call_lowering(db, key) {
        match lowering {
            CallLowering::ManifestBuiltin(builtin) => {
                manifest_builtins.insert(builtin.symbol);
            }
            CallLowering::CorelibService(service) => {
                corelib_services.insert(service);
            }
            CallLowering::NativeModCallback(_)
            | CallLowering::Direct(_)
            | CallLowering::Dynamic
            | CallLowering::Runtime(_) => {}
        }
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_manifest_service_callees(db, child, manifest_builtins, corelib_services);
        }
    }
}

pub(super) struct ArtifactCallImporter<'a> {
    pub(super) symbols: &'a HashMap<DirectCallee, String>,
}

impl beskid_isle::CallImporter for ArtifactCallImporter<'_> {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        signature: &Signature,
    ) -> Result<FuncRef, beskid_isle::CallImportError> {
        let symbol = self.symbols.get(&callee).ok_or(beskid_isle::CallImportError::UnknownCallee)?;
        let signature = builder.import_signature(signature.clone());
        Ok(builder.func.import_function(ExtFuncData {
            name: ExternalName::testcase(symbol.as_bytes()),
            signature,
            colocated: false,
            patchable: false,
        }))
    }
}
