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
            colocated: true,
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
                    u32::try_from(index)
                        .ok()
                        .map(|index| (DirectCallee::runtime_intrinsic(index), intrinsic.symbol.clone()))
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

/// ABI symbols admitted by the distinct Corelib syscall capability. Unlike runtime intrinsics,
/// these imports can only be selected by a `CallLowering::CorelibService` syntax fact from the
/// exact embedded facade; ordinary dynamic calls never reach this table.
pub(super) fn corelib_service_symbols(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
) -> HashMap<DirectCallee, String> {
    let Some(capability) = input.corelib_service_capability() else {
        return HashMap::new();
    };
    let mut callees = HashSet::new();
    for item in items {
        collect_corelib_service_callees(input.database(), item.key, &mut callees);
    }
    capability
        .services()
        .iter()
        .filter(|service| {
            callees.contains(&service.symbol)
                || matches!(service.symbol, "str_new" | "str_from_i64" | "str_eq" | "str_concat")
        })
        .map(|service| (DirectCallee::corelib_service(service.symbol), service.symbol.to_owned()))
        .collect()
}

fn collect_corelib_service_callees(db: &dyn beskid_queries::Db, key: AstNodeKey, callees: &mut HashSet<&'static str>) {
    if let Ok(Some(CallLowering::CorelibService(service))) = call_lowering(db, key) {
        callees.insert(service.symbol);
    }
    if let Ok(Some(children)) = child_nodes(db, key) {
        for child in children.iter().copied() {
            collect_corelib_service_callees(db, child, callees);
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
