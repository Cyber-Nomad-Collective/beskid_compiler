use super::*;

/// Explicit module importer keyed by syntax-resolved item identity.
///
/// Call lowering never guesses symbols: the host declares each item and supplies its exact
/// [`FuncId`] keyed by [`DirectCallee`].
pub struct ItemModuleImporter<'module, M: Module> {
    module: &'module mut M,
    functions: HashMap<DirectCallee, FuncId>,
    /// Bodyless callees linked from another object (Rust Glue import adapters), declared on first
    /// use with the exact call-site signature that ordinary artifact lowering also imports.
    imports: HashMap<DirectCallee, String>,
}

impl<'module, M: Module> ItemModuleImporter<'module, M> {
    pub fn new(module: &'module mut M, functions: HashMap<DirectCallee, FuncId>) -> Self {
        Self { module, functions, imports: HashMap::new() }
    }

    pub fn with_imports(mut self, imports: HashMap<DirectCallee, String>) -> Self {
        self.imports = imports;
        self
    }
}

impl<M: Module> CallImporter for ItemModuleImporter<'_, M> {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        signature: &Signature,
    ) -> Result<FuncRef, beskid_isle::CallImportError> {
        let function = match self.functions.get(&callee).copied() {
            Some(function) => function,
            None => {
                let symbol = self.imports.get(&callee).ok_or(beskid_isle::CallImportError::UnknownCallee)?;
                // A conflicting earlier declaration of the same adapter symbol is an ABI mismatch.
                let function = self
                    .module
                    .declare_function(symbol, cranelift_module::Linkage::Import, signature)
                    .map_err(|_| beskid_isle::CallImportError::UnknownCallee)?;
                self.functions.insert(callee, function);
                function
            }
        };
        let reference = self.module.declare_func_in_func(function, builder.func);
        // `Module` marks final-linkage functions colocated by default. JIT code allocation does
        // not provide that placement guarantee, and this reference can later be materialized by
        // `func_addr`, so require the architecture-independent absolute-address lowering.
        builder.func.dfg.ext_funcs[reference].colocated = false;
        Ok(reference)
    }
}
