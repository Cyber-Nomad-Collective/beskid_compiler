use crate::errors::CodegenError;
use crate::lowering::descriptor::{TypeDescriptorData, TypeLayout};
use beskid_analysis::resolve::ItemId;
use beskid_analysis::types::TypeId;
use cranelift_codegen::ir::Function;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

/// Result of lowering a single node or expression.
pub type CodegenResult<T> = Result<T, CodegenError>;

/// One user function lowered to a standalone Cranelift [`Function`] (still uses `TestCase` extern names until linking).
#[derive(Debug, Clone)]
pub struct LoweredFunction {
    /// Beskid symbol name used when declaring and defining this function on the module.
    pub name: String,
    /// CLIF body and signature for `name`.
    pub function: Function,
}

/// External (FFI) import discovered from Extern-annotated contracts/modules.
///
/// Semantics:
/// - `symbol`: function symbol to link (e.g., "getpid")
/// - `abi`: optional ABI name (v0.1 supports only "C")
/// - `library`: required shared object name on Linux (e.g., "libc.so.6")
#[derive(Debug, Clone)]
pub struct ExternImport {
    pub symbol: String,
    pub abi: Option<String>,
    pub library: Option<String>,
}

/// Output of codegen: lowered functions, descriptors, literals, extern imports, and exports.
#[derive(Debug, Clone, Default)]
pub struct CodegenArtifact {
    pub functions: Vec<LoweredFunction>,
    pub type_descriptors: HashMap<TypeId, TypeDescriptorData>,
    pub string_literals: HashMap<String, Vec<u8>>,
    pub closure_static_plans: Vec<crate::closure_static::ClosureStaticPlan>,
    pub aggregate_static_plans: Vec<crate::aggregate_static::AggregateStaticPlan>,
    pub extern_imports: Vec<ExternImport>,
    pub exports: Vec<crate::lowering::expressions::export::ExportEntry>,
}

/// Key for a monomorphized function instance (`item` plus concrete type `args`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MonomorphKey {
    pub item: ItemId,
    pub args: Vec<TypeId>,
}

/// Mutable accumulator while lowering: emitted functions, layouts, string pool, and monomorph cache.
#[derive(Default)]
pub struct CodegenContext {
    pub functions_emitted: usize,
    pub lowered_functions: Vec<LoweredFunction>,
    pub type_layouts: HashMap<TypeId, TypeLayout>,
    pub type_descriptors: HashMap<TypeId, TypeDescriptorData>,
    pub string_literals: HashMap<String, Vec<u8>>,
    pub monomorphized_functions: HashMap<MonomorphKey, String>,
    /// Items currently being lowered (detects mutual recursion during emission).
    pub emitting_items: HashSet<ItemId>,
    /// Source file for the function body currently being lowered (cross-unit local lookup).
    pub current_source_path: Option<PathBuf>,
    /// Active generic parameter substitution while lowering a monomorphized function body.
    pub active_generic_substitution: HashMap<String, TypeId>,
    next_string_literal_id: usize,
}

impl CodegenContext {
    /// Empty context (no functions or literals yet).
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a symbol name was already lowered into [`Self::lowered_functions`].
    pub fn symbol_emitted(&self, name: &str) -> bool {
        self.lowered_functions.iter().any(|f| f.name == name)
    }

    /// Compute or return cached [`crate::lowering::descriptor::TypeLayout`] for `type_id`.
    pub fn type_layout(
        &mut self,
        type_result: &beskid_analysis::types::TypeResult,
        type_id: TypeId,
    ) -> Option<TypeLayout> {
        crate::lowering::descriptor::get_or_compute_layout(
            &mut self.type_layouts,
            type_result,
            type_id,
        )
    }

    /// Compute or return cached [`crate::lowering::descriptor::TypeDescriptorData`] for runtime metadata emission.
    pub fn type_descriptor(
        &mut self,
        type_result: &beskid_analysis::types::TypeResult,
        type_id: TypeId,
    ) -> Option<TypeDescriptorData> {
        if let Some(existing) = self.type_descriptors.get(&type_id) {
            return Some(existing.clone());
        }
        let layout = self.type_layout(type_result, type_id)?;
        let descriptor = crate::lowering::descriptor::build_descriptor(&layout);
        self.type_descriptors.insert(type_id, descriptor.clone());
        Some(descriptor)
    }

    /// Deduplicating pool for string literal globals; returns a stable symbol name for `bytes`.
    pub fn intern_string_literal(&mut self, bytes: &[u8]) -> String {
        for (symbol, data) in &self.string_literals {
            if data.as_slice() == bytes {
                return symbol.clone();
            }
        }
        let symbol = format!("__beskid_str_lit_{}", self.next_string_literal_id);
        self.next_string_literal_id += 1;
        self.string_literals.insert(symbol.clone(), bytes.to_vec());
        symbol
    }
}
