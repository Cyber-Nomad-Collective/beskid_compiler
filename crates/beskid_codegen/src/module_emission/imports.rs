use std::collections::{HashMap, HashSet};

use beskid_isle::{AstNodeKey, DirectCallee, StringInterner};
use beskid_queries::{CallLowering, call_lowering, child_nodes, extern_contract_import_for_declaration};
use cranelift_codegen::ir::{ExtFuncData, ExternalName, FuncRef, GlobalValueData, InstBuilder, Signature, Type, Value};
use cranelift_frontend::FunctionBuilder;

use super::items::ResolvedSyntaxModuleItem;
use crate::{CodegenContext, CodegenInput, ExternImport};

/// Syntax-ISLE adapter over the existing artifact-owned literal pool.
pub(super) struct ArtifactStringInterner<'a> {
    pub(super) context: &'a mut CodegenContext,
    pub(super) pointer_type: Type,
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
            colocated: false,
            tls: false,
        });
        let bytes = builder.ins().global_value(self.pointer_type, global);
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
    callees: &mut HashMap<DirectCallee, ExternImport>,
) {
    if let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, key)
        && let Some((symbol, abi, library)) = extern_contract_import_for_declaration(db, declaration)
    {
        callees.entry(DirectCallee::item(declaration)).or_insert(ExternImport { symbol, abi, library });
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_extern_contract_callees(db, child, callees);
        }
    }
}

pub(super) fn extern_contract_symbols(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> HashMap<DirectCallee, String> {
    let mut callees = HashMap::new();
    for item in items {
        collect_extern_contract_callees(input.database(), item.key, &mut callees);
    }
    callees.into_iter().map(|(callee, import)| (callee, import.symbol)).collect()
}

pub(super) fn extern_contract_imports(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> Vec<ExternImport> {
    let mut callees = HashMap::new();
    for item in items {
        collect_extern_contract_callees(input.database(), item.key, &mut callees);
    }
    callees.into_values().collect()
}

/// String runtime helpers that ISLE lowering emits directly (string literals, coercion,
/// comparison, concatenation). These are always required — they are not gated by the
/// Corelib syscall capability because they are fundamental operations, not facade services.
const ALWAYS_AVAILABLE_STRING_SERVICES: &[&str] = &["str_new", "str_from_i64", "str_eq", "str_concat"];

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
) -> HashMap<DirectCallee, String> {
    let mut callees = HashSet::new();
    for item in items {
        collect_manifest_service_callees(input.database(), item.key, &mut callees);
    }
    let mut symbols = HashMap::new();
    for symbol in ALWAYS_AVAILABLE_STRING_SERVICES {
        symbols.insert(DirectCallee::corelib_service(symbol), (*symbol).to_owned());
    }
    for symbol in callees {
        if !ALWAYS_AVAILABLE_STRING_SERVICES.contains(&symbol) {
            symbols.insert(DirectCallee::corelib_service(symbol), symbol.to_owned());
        }
    }
    symbols
}

fn collect_manifest_service_callees(db: &dyn beskid_queries::Db, key: AstNodeKey, callees: &mut HashSet<&'static str>) {
    if let Ok(Some(lowering)) = call_lowering(db, key) {
        match lowering {
            CallLowering::ManifestBuiltin(builtin) => {
                callees.insert(builtin.symbol);
            }
            CallLowering::CorelibService(service) => {
                callees.insert(service.symbol);
            }
            CallLowering::Direct(_) | CallLowering::Dynamic | CallLowering::Runtime(_) => {}
        }
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_manifest_service_callees(db, child, callees);
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
