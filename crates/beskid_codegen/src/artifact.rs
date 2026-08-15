//! Output records and accumulation state for syntax-driven ISLE code generation.

use std::collections::HashMap;

use beskid_analysis::types::TypeId;
use cranelift_codegen::ir::Function;

/// One generated function at the Cranelift artifact boundary.
#[derive(Debug, Clone)]
pub struct LoweredFunction {
    pub name: String,
    pub function: Function,
}

/// External function import authorized by syntax and semantic facts.
#[derive(Debug, Clone)]
pub struct ExternImport {
    pub symbol: String,
    pub abi: Option<String>,
    pub library: Option<String>,
}

/// One exported Beskid function and its linker-visible symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportEntry {
    pub beskid_name: String,
    pub exported_symbol: String,
    pub abi: String,
}

/// Serialized type descriptor payload emitted into object or JIT data.
#[derive(Debug, Clone)]
pub struct TypeDescriptorData {
    pub size: usize,
    pub align: usize,
    pub pointer_offsets: Vec<usize>,
}

/// Complete output of the syntax → ISLE → verified CLIF path.
#[derive(Debug, Clone, Default)]
pub struct CodegenArtifact {
    pub functions: Vec<LoweredFunction>,
    pub type_descriptors: HashMap<TypeId, TypeDescriptorData>,
    pub string_literals: HashMap<String, Vec<u8>>,
    pub closure_static_plans: Vec<crate::closure_static::ClosureStaticPlan>,
    pub aggregate_static_plans: Vec<crate::aggregate_static::AggregateStaticPlan>,
    pub array_static_plans: Vec<crate::array_static::ArrayStaticPlan>,
    pub extern_imports: Vec<ExternImport>,
    pub exports: Vec<ExportEntry>,
}

/// Artifact-owned state shared by generated function emitters.
#[derive(Default)]
pub struct CodegenContext {
    pub string_literals: HashMap<String, Vec<u8>>,
    artifact_namespace: String,
    next_string_literal_id: usize,
}

impl CodegenContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn new_with_artifact_namespace(namespace: impl Into<String>) -> Self {
        Self { artifact_namespace: namespace.into(), ..Self::default() }
    }

    /// Intern bytes in the artifact's addressable literal pool.
    pub fn intern_string_literal(&mut self, bytes: &[u8]) -> String {
        let storage = if bytes.is_empty() { &[0] } else { bytes };
        for (symbol, data) in &self.string_literals {
            if data.as_slice() == storage {
                return symbol.clone();
            }
        }
        let symbol = if self.artifact_namespace.is_empty() {
            format!("__beskid_str_lit_{}", self.next_string_literal_id)
        } else {
            format!("__beskid_{}_str_lit_{}", self.artifact_namespace, self.next_string_literal_id)
        };
        self.next_string_literal_id += 1;
        self.string_literals.insert(symbol.clone(), storage.to_vec());
        symbol
    }
}

/// Native object-file symbol for an emitted function.
pub fn object_link_symbol(beskid_name: &str, exports: &[ExportEntry]) -> String {
    let logical = beskid_name.split('#').next().unwrap_or(beskid_name);
    if let Some(entry) = exports.iter().find(|entry| entry.beskid_name == logical || entry.beskid_name == beskid_name) {
        return entry.exported_symbol.clone();
    }
    if logical == "Main" {
        return "main".to_owned();
    }
    beskid_name.to_owned()
}
