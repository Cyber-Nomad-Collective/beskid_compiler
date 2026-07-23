use super::*;

/// Explicit module importer keyed by syntax-resolved item identity.
///
/// Call lowering never guesses symbols: the host declares each item and supplies its exact
/// [`FuncId`] keyed by [`DirectCallee`].
pub struct ItemModuleImporter<'module, M: Module> {
    module: &'module mut M,
    functions: HashMap<DirectCallee, FuncId>,
}

impl<'module, M: Module> ItemModuleImporter<'module, M> {
    pub fn new(module: &'module mut M, functions: HashMap<DirectCallee, FuncId>) -> Self {
        Self { module, functions }
    }
}

impl<M: Module> CallImporter for ItemModuleImporter<'_, M> {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        _signature: &Signature,
    ) -> Result<FuncRef, beskid_isle::CallImportError> {
        let function = self
            .functions
            .get(&callee)
            .copied()
            .ok_or(beskid_isle::CallImportError::UnknownCallee)?;
        Ok(self.module.declare_func_in_func(function, builder.func))
    }
}
