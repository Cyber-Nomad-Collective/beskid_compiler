use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn local_managed_reference(&self, key: AstNodeKey, value_type: Type) -> Option<ManagedReferenceFact> {
        if value_type == dispatch::pointer_type(self.frontend_config) {
            self.facts.managed_reference(key)
        } else {
            Some(ManagedReferenceFact::NativeOrScalar)
        }
    }

    pub(super) fn lowered_value_reference(&self, key: AstNodeKey, value: Value) -> Option<ManagedReferenceFact> {
        self.expression_references
            .get(&value)
            .copied()
            .or_else(|| self.local_managed_reference(key, self.builder.func.dfg.value_type(value)))
    }

    pub(crate) fn bind_local(
        &mut self,
        slot: LocalSlotId,
        value: Value,
        value_type: Type,
        managed_reference: ManagedReferenceFact,
    ) -> Option<()> {
        (!self.locals.contains_key(&slot) && self.builder.func.dfg.value_type(value) == value_type).then_some(())?;
        let variable = self.builder.declare_var(value_type);
        if managed_reference == ManagedReferenceFact::GcManaged {
            (value_type == dispatch::pointer_type(self.frontend_config)).then_some(())?;
            self.builder.declare_var_needs_stack_map(variable);
        }
        self.builder.def_var(variable, value);
        self.local_root_scopes.last_mut()?.bindings.push(slot);
        self.locals.insert(slot, ManagedLocalBinding { variable, value_type, managed_reference });
        Some(())
    }

    pub(super) fn assign_local(&mut self, slot: LocalSlotId, value: Value) -> Option<()> {
        let binding = self.locals.get(&slot).copied()?;
        (self.builder.func.dfg.value_type(value) == binding.value_type).then_some(())?;
        self.builder.def_var(binding.variable, value);
        Some(())
    }

    pub(super) fn publish_managed_local(&mut self, slot: LocalSlotId, value: Value) -> Option<()> {
        let binding = self.locals.get(&slot).copied()?;
        (binding.managed_reference == ManagedReferenceFact::GcManaged).then_some(())?;
        self.assign_local(slot, value)
    }

    pub(super) fn root_temporary(&mut self, value: Value) -> Option<TemporaryRoot> {
        (self.builder.func.dfg.value_type(value) == dispatch::pointer_type(self.frontend_config)).then_some(())?;
        self.builder.declare_value_needs_stack_map(value);
        let variable = self.builder.declare_var(self.frontend_config.pointer_type());
        self.builder.declare_var_needs_stack_map(variable);
        self.builder.def_var(variable, value);
        Some(TemporaryRoot(variable))
    }

    pub(super) fn root_temporary_if_needed(&mut self, key: AstNodeKey, value: Value) -> Option<Option<TemporaryRoot>> {
        if let Some(slot) = self.facts.local_slot(key)
            && self.locals.contains_key(&slot)
        {
            return Some(None);
        }
        if self.lowered_value_reference(key, value)? == ManagedReferenceFact::NativeOrScalar {
            return Some(None);
        }
        self.root_temporary(value).map(Some)
    }

    pub(super) fn release_temporary_root(&mut self, root: Option<TemporaryRoot>) -> Option<()> {
        if let Some(TemporaryRoot(variable)) = root {
            let value = self.builder.use_var(variable);
            // Some owners are borrowed through derived addresses until this point.
            self.builder.ins().bor(value, value);
        }
        Some(())
    }

    pub(super) fn track_expression_root(&mut self, root: TemporaryRoot) -> Option<()> {
        self.local_root_scopes.last_mut()?.temporaries.push(root);
        Some(())
    }

    pub(super) fn root_expression_value(&mut self, value: Value) -> Option<TemporaryRoot> {
        let root = self.root_temporary(value)?;
        self.track_expression_root(root)?;
        Some(root)
    }

    pub(super) fn root_expression_value_if_needed(
        &mut self,
        key: AstNodeKey,
        value: Value,
    ) -> Option<Option<TemporaryRoot>> {
        // Value blocks preserve the managed category of their source-proven result;
        // their block node need not have an independently inferred semantic type.
        if let Some(result) = self.facts.block_result(key) {
            return self.root_expression_value_if_needed(result, value);
        }
        // A field path can reuse its receiver's slot for address lowering; that
        // receiver's category is not the category of the projected field value.
        if self.facts.field_index(key).is_none()
            && let Some(binding) = self.facts.local_slot(key).and_then(|slot| self.locals.get(&slot))
        {
            return match binding.managed_reference {
                ManagedReferenceFact::NativeOrScalar => Some(None),
                ManagedReferenceFact::GcManaged => self.root_expression_value(value).map(Some),
            };
        }
        match self.lowered_value_reference(key, value)? {
            ManagedReferenceFact::NativeOrScalar => Some(None),
            // Even a rooted local needs a snapshot root: a later argument may reassign it.
            ManagedReferenceFact::GcManaged => self.root_expression_value(value).map(Some),
        }
    }

    pub(super) fn release_expression_root(&mut self, root: Option<TemporaryRoot>) -> Option<()> {
        if let Some(root) = root {
            let scope = self.local_root_scopes.iter_mut().rev().find(|scope| scope.temporaries.contains(&root))?;
            let index = scope.temporaries.iter().position(|candidate| *candidate == root)?;
            scope.temporaries.remove(index);
            self.release_temporary_root(Some(root))?;
        }
        Some(())
    }

    pub(crate) fn release_managed_local_roots(&mut self) -> Option<()> {
        self.release_local_roots_from(0)
    }

    pub(super) fn local_root_scope_depth(&self) -> usize {
        self.local_root_scopes.len()
    }

    pub(super) fn begin_local_root_scope(&mut self) {
        self.local_root_scopes.push(LocalRootScope::default());
    }

    pub(super) fn release_local_roots_from(&mut self, depth: usize) -> Option<()> {
        let temporaries = self
            .local_root_scopes
            .get(depth..)?
            .iter()
            .rev()
            .flat_map(|scope| scope.temporaries.iter().rev().copied())
            .collect::<Vec<_>>();
        // This emits an exit edge, without consuming the compile-time roots needed by
        // other branches (including the normal expression-completion edge).
        for root in temporaries {
            self.release_temporary_root(Some(root))?;
        }
        Some(())
    }

    pub(super) fn end_local_root_scope(
        &mut self,
        emit_exit: bool,
        abandoned_root: Option<TemporaryRoot>,
    ) -> Option<()> {
        (self.local_root_scopes.len() > 1).then_some(())?;
        if emit_exit {
            self.cleanup_scope_exit(self.local_root_scopes.len() - 1, abandoned_root)?;
        }
        let scope = self.local_root_scopes.pop()?;
        if emit_exit {
            for root in scope.temporaries.iter().rev() {
                self.release_temporary_root(Some(*root))?;
            }
        }
        for local in scope.bindings {
            self.locals.remove(&local);
        }
        Some(())
    }

    pub(super) fn end_local_root_scope_for_current_block(&mut self) -> Option<()> {
        let current = self.builder.current_block()?;
        self.end_local_root_scope(!block_is_terminated(self.builder, current), None)
    }
}

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn allocate_array(&mut self, request: Value) -> Option<(Value, TemporaryRoot)> {
        self.array_constructor("beskid_rt_v5_array_allocate", &[request])
    }

    pub(super) fn grow_array(&mut self, owner: Value, length: Value) -> Option<(Value, TemporaryRoot)> {
        self.array_constructor("beskid_rt_v5_array_grow", &[owner, length])
    }

    fn array_constructor(
        &mut self,
        symbol: &str,
        arguments: &[Value],
    ) -> Option<(Value, TemporaryRoot)> {
        let pointer = self.frontend_config.pointer_type();
        let function = self.import_runtime_helper(symbol, &vec![pointer; arguments.len()], Some(pointer))?;
        let call = self.builder.ins().call(function, arguments);
        let array = self.builder.inst_results(call).first().copied()?;
        let root = self.root_temporary(array)?;
        Some((array, root))
    }
}
