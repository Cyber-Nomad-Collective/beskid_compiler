//! Validate that a [`CodegenArtifact`] defines every `TestCase` callee referenced in CLIF.

use std::collections::HashSet;

use cranelift_codegen::ir::ExternalName;

use beskid_abi::BUILTIN_SPECS;

use crate::CodegenArtifact;

/// A `TestCase` import in lowered CLIF with no matching [`CodegenArtifact::functions`] entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingSymbol {
    pub name: String,
}

/// Scan all lowered functions for `ExternalName::TestCase` references and ensure each name is defined
/// in `artifact.functions` or is a known builtin/extern import.
pub fn validate_artifact(artifact: &CodegenArtifact) -> Result<(), Vec<MissingSymbol>> {
    let defined: HashSet<String> = artifact
        .functions
        .iter()
        .map(|f| f.name.clone())
        .collect();
    let extern_syms: HashSet<String> = artifact
        .extern_imports
        .iter()
        .map(|e| e.symbol.clone())
        .collect();

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

    let mut missing = Vec::new();
    for symbol in referenced {
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

fn is_runtime_builtin(symbol: &str) -> bool {
    BUILTIN_SPECS
        .iter()
        .any(|spec| spec.symbol == symbol)
        || matches!(
            symbol,
            "event_len"
                | "event_subscribe"
                | "event_unsubscribe_first"
                | "event_get_handler"
        )
}
