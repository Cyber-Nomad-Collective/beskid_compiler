//! Validate that a [`CodegenArtifact`] defines every `TestCase` callee referenced in CLIF.

use std::collections::HashSet;

use cranelift_codegen::ir::ExternalName;

use beskid_abi::{all_builtin_specs, is_dispatch_symbol};

use crate::{CodegenArtifact, ExternImport};

/// A `TestCase` import in lowered CLIF with no matching [`CodegenArtifact::functions`] entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingSymbol {
    pub name: String,
}

/// Extern imports that appear as `TestCase` callees in lowered CLIF.
///
/// The full [`CodegenArtifact::extern_imports`] list may include contract symbols from every
/// assembly unit (for link-plan completeness); JIT/AOT runtime resolution only needs this subset.
pub fn referenced_extern_imports(artifact: &CodegenArtifact) -> Vec<ExternImport> {
    let defined: HashSet<String> = artifact.functions.iter().map(|f| f.name.clone()).collect();
    let extern_by_symbol: std::collections::HashMap<&str, &ExternImport> =
        artifact.extern_imports.iter().map(|entry| (entry.symbol.as_str(), entry)).collect();

    let mut out = Vec::new();
    for symbol in collect_referenced_testcase_symbols(artifact) {
        if defined.contains(&symbol) || is_runtime_builtin(&symbol) {
            continue;
        }
        if let Some(entry) = extern_by_symbol.get(symbol.as_str()) {
            out.push((*entry).clone());
        }
    }
    out.sort_by(|left, right| left.symbol.cmp(&right.symbol));
    out
}

/// Scan all lowered functions for `ExternalName::TestCase` references and ensure each name is defined
/// in `artifact.functions` or is a known builtin/extern import.
pub fn validate_artifact(artifact: &CodegenArtifact) -> Result<(), Vec<MissingSymbol>> {
    let defined: HashSet<String> = artifact.functions.iter().map(|f| f.name.clone()).collect();
    let extern_syms: HashSet<String> = artifact.extern_imports.iter().map(|e| e.symbol.clone()).collect();

    let mut missing = Vec::new();
    for symbol in collect_referenced_testcase_symbols(artifact) {
        if defined.contains(&symbol) || extern_syms.contains(&symbol) {
            continue;
        }
        if is_runtime_builtin(&symbol) {
            continue;
        }
        missing.push(MissingSymbol { name: symbol });
    }

    if missing.is_empty() {
        Ok(())
    } else {
        missing.sort_by(|a, b| a.name.cmp(&b.name));
        Err(missing)
    }
}

fn collect_referenced_testcase_symbols(artifact: &CodegenArtifact) -> HashSet<String> {
    let mut referenced = HashSet::new();
    let mut ctx_probe = cranelift_codegen::Context::new();
    for function in &artifact.functions {
        ctx_probe.func = function.function.clone();
        for (_func_ref, ext_func) in ctx_probe.func.dfg.ext_funcs.iter() {
            if let ExternalName::TestCase(name) = &ext_func.name {
                let symbol = String::from_utf8_lossy(name.raw()).to_string();
                referenced.insert(symbol);
            }
        }
    }
    referenced
}

fn is_runtime_builtin(symbol: &str) -> bool {
    all_builtin_specs().any(|spec| spec.symbol == symbol) || is_dispatch_symbol(symbol)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LoweredFunction;
    use cranelift_codegen::ir::{AbiParam, ExternalName, Function, Signature, types};
    use cranelift_codegen::isa::CallConv;

    #[test]
    fn referenced_extern_imports_only_include_clif_callees() {
        let mut callee = Function::new();
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        let sig_id = callee.import_signature(sig);
        callee.import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase("isatty".as_bytes()),
            signature: sig_id,
            colocated: false,
            patchable: false,
        });

        let artifact = CodegenArtifact {
            functions: vec![LoweredFunction { name: "main".into(), function: callee }],
            extern_imports: vec![
                ExternImport { symbol: "isatty".into(), abi: Some("C".into()), library: Some("libc".into()) },
                ExternImport {
                    symbol: "GetConsoleScreenBufferInfo".into(),
                    abi: Some("C".into()),
                    library: Some("kernel32".into()),
                },
            ],
            ..Default::default()
        };

        let referenced = referenced_extern_imports(&artifact);
        assert_eq!(referenced.len(), 1);
        assert_eq!(referenced[0].symbol, "isatty");
    }
}
