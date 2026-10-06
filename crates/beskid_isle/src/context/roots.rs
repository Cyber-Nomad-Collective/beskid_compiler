use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn local_managed_reference(&self, key: AstNodeKey, value_type: Type) -> Option<ManagedReferenceFact> {
        if value_type == dispatch::pointer_type(self.frontend_config) {
            self.facts.managed_reference(key)
        } else {
            Some(ManagedReferenceFact::NativeOrScalar)
        }
    }

    fn new_root_slot(&mut self, value: Value) -> StackSlot {
        let pointer = dispatch::pointer_type(self.frontend_config);
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            pointer.bytes(),
            pointer.bytes().ilog2() as u8,
        ));
        self.builder.ins().stack_store(self.frontend_config.pointer_type(), value, slot, 0);
        slot
    }

    fn register_root_slot(&mut self, slot: StackSlot) -> Option<()> {
        if self.facts.nonallocating_publication_body() {
            return Some(());
        }
        let pointer = dispatch::pointer_type(self.frontend_config);
        let address = self.builder.ins().stack_addr(pointer, slot, 0);
        let register = self.import_runtime_helper("gc_register_root", &[pointer], Some(types::I8))?;
        let call = self.builder.ins().call(register, &[address]);
        let registered = self.builder.inst_results(call).first().copied()?;
        self.guard_checked_allocation()?;
        self.builder.ins().trapz(registered, TrapCode::unwrap_user(8));
        Some(())
    }

    pub(super) fn unregister_root_slot(&mut self, slot: StackSlot) -> Option<()> {
        if self.facts.nonallocating_publication_body() {
            return Some(());
        }
        let pointer = dispatch::pointer_type(self.frontend_config);
        let address = self.builder.ins().stack_addr(pointer, slot, 0);
        let unregister = self.import_runtime_helper("gc_unregister_root", &[pointer], None)?;
        self.builder.ins().call(unregister, &[address]);
        Some(())
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
        self.builder.def_var(variable, value);
        let root_slot = if managed_reference == ManagedReferenceFact::GcManaged {
            (value_type == dispatch::pointer_type(self.frontend_config)).then_some(())?;
            let root_slot = self.new_root_slot(value);
            self.register_root_slot(root_slot)?;
            Some(root_slot)
        } else {
            None
        };
        self.local_root_scopes.last_mut()?.bindings.push((slot, root_slot));
        self.locals.insert(slot, ManagedLocalBinding { variable, value_type, managed_reference, root_slot });
        Some(())
    }

    /// A checked destination is admitted before evaluating its initializer or call arguments.
    /// Failure exits the clone through its outer reserved boundary, before any value escapes.
    pub(super) fn prepare_checked_destination(
        &mut self,
        value_type: Type,
        managed_reference: ManagedReferenceFact,
    ) -> Option<Option<StackSlot>> {
        if !self.facts.checked_allocation_body() || managed_reference != ManagedReferenceFact::GcManaged {
            return Some(None);
        }
        (value_type == dispatch::pointer_type(self.frontend_config)).then_some(())?;
        let empty = self.builder.ins().iconst(value_type, 0);
        let root = self.new_root_slot(empty);
        self.register_root_slot(root)?;
        self.track_expression_root(ScopedTemporaryRoot::Managed(root))?;
        Some(Some(root))
    }

    /// Transfer the admitted destination from the construction cleanup set to its local owner.
    /// No second registration, allocation or managed callback occurs after result evaluation.
    pub(super) fn bind_local_in_destination(
        &mut self,
        slot: LocalSlotId,
        value: Value,
        value_type: Type,
        managed_reference: ManagedReferenceFact,
        admitted: Option<StackSlot>,
    ) -> Option<()> {
        let Some(root) = admitted else {
            return self.bind_local(slot, value, value_type, managed_reference);
        };
        (!self.locals.contains_key(&slot)
            && self.builder.func.dfg.value_type(value) == value_type
            && value_type == dispatch::pointer_type(self.frontend_config)
            && managed_reference == ManagedReferenceFact::GcManaged)
            .then_some(())?;
        let construction = ScopedTemporaryRoot::Managed(root);
        let scope = self.local_root_scopes.last_mut()?;
        let position = scope.temporaries.iter().position(|candidate| *candidate == construction)?;
        scope.temporaries.remove(position);
        scope.bindings.push((slot, Some(root)));
        let variable = self.builder.declare_var(value_type);
        self.builder.ins().stack_store(self.frontend_config.pointer_type(), value, root, 0);
        self.builder.def_var(variable, value);
        self.locals
            .insert(slot, ManagedLocalBinding { variable, value_type, managed_reference, root_slot: Some(root) });
        Some(())
    }

    pub(super) fn assign_local(&mut self, slot: LocalSlotId, value: Value) -> Option<()> {
        let binding = self.locals.get(&slot).copied()?;
        (self.builder.func.dfg.value_type(value) == binding.value_type).then_some(())?;
        self.builder.def_var(binding.variable, value);
        if let Some(root_slot) = binding.root_slot {
            self.builder.ins().stack_store(self.frontend_config.pointer_type(), value, root_slot, 0);
        }
        Some(())
    }

    pub(super) fn publish_managed_local(&mut self, slot: LocalSlotId, value: Value) -> Option<()> {
        let binding = self.locals.get(&slot).copied()?;
        (binding.managed_reference == ManagedReferenceFact::GcManaged && binding.root_slot.is_some()).then_some(())?;
        self.assign_local(slot, value)
    }

    pub(super) fn root_temporary(&mut self, value: Value) -> Option<StackSlot> {
        (self.builder.func.dfg.value_type(value) == dispatch::pointer_type(self.frontend_config)).then_some(())?;
        let slot = self.new_root_slot(value);
        self.register_root_slot(slot)?;
        if self.facts.checked_allocation_body() {
            self.track_expression_root(ScopedTemporaryRoot::Managed(slot))?;
        }
        Some(slot)
    }

    pub(super) fn root_temporary_if_needed(&mut self, key: AstNodeKey, value: Value) -> Option<Option<StackSlot>> {
        if let Some(slot) = self.facts.local_slot(key)
            && let Some(binding) = self.locals.get(&slot)
        {
            if binding.managed_reference == ManagedReferenceFact::GcManaged {
                binding.root_slot?;
            }
            return Some(None);
        }
        if self.facts.managed_reference(key)? == ManagedReferenceFact::NativeOrScalar {
            return Some(None);
        }
        self.root_temporary(value).map(Some)
    }

    pub(super) fn release_temporary_root(&mut self, slot: Option<StackSlot>) -> Option<()> {
        if let Some(slot) = slot {
            self.forget_checked_snapshot(slot);
            self.unregister_root_slot(slot)?;
        }
        Some(())
    }

    pub(super) fn forget_checked_snapshot(&mut self, slot: StackSlot) {
        if self.facts.checked_allocation_body() {
            let root = ScopedTemporaryRoot::Managed(slot);
            if let Some(scope) = self.local_root_scopes.iter_mut().rev().find(|scope| scope.temporaries.contains(&root))
            {
                if let Some(index) = scope.temporaries.iter().position(|candidate| *candidate == root) {
                    scope.temporaries.remove(index);
                }
            }
        }
    }

    pub(super) fn track_expression_root(&mut self, root: ScopedTemporaryRoot) -> Option<()> {
        self.local_root_scopes.last_mut()?.temporaries.push(root);
        Some(())
    }

    pub(super) fn root_expression_value(&mut self, value: Value) -> Option<ScopedTemporaryRoot> {
        let root = ScopedTemporaryRoot::Managed(self.root_temporary(value)?);
        if !self.facts.checked_allocation_body() {
            self.track_expression_root(root)?;
        }
        Some(root)
    }

    pub(super) fn root_expression_value_if_needed(
        &mut self,
        key: AstNodeKey,
        value: Value,
    ) -> Option<Option<ScopedTemporaryRoot>> {
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
        match self.local_managed_reference(key, self.builder.func.dfg.value_type(value))? {
            ManagedReferenceFact::NativeOrScalar => Some(None),
            // Even a rooted local needs a snapshot root: a later argument may reassign it.
            ManagedReferenceFact::GcManaged => self.root_expression_value(value).map(Some),
        }
    }

    fn emit_expression_root_release(&mut self, root: ScopedTemporaryRoot) -> Option<()> {
        match root {
            ScopedTemporaryRoot::Managed(slot) => self.unregister_root_slot(slot),
            ScopedTemporaryRoot::ArrayConstruction(slot) => {
                let pointer = dispatch::pointer_type(self.frontend_config);
                let handle = self.builder.ins().stack_load(pointer, pointer, slot, 0);
                let finish =
                    self.import_runtime_helper("beskid_rt_v5_array_construction_finish", &[pointer], Some(types::I8))?;
                let call = self.builder.ins().call(finish, &[handle]);
                let released = self.builder.inst_results(call).first().copied()?;
                self.builder.ins().trapz(released, TrapCode::unwrap_user(10));
                Some(())
            }
        }
    }

    pub(super) fn release_expression_root(&mut self, root: Option<ScopedTemporaryRoot>) -> Option<()> {
        if let Some(root) = root {
            let scope = self.local_root_scopes.iter_mut().rev().find(|scope| scope.temporaries.contains(&root))?;
            let index = scope.temporaries.iter().position(|candidate| *candidate == root)?;
            scope.temporaries.remove(index);
            self.emit_expression_root_release(root)?;
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
            self.emit_expression_root_release(root)?;
        }
        let slots = self
            .local_root_scopes
            .get(depth..)?
            .iter()
            .rev()
            .flat_map(|scope| scope.bindings.iter().rev().filter_map(|(_, slot)| *slot))
            .collect::<Vec<_>>();
        for slot in slots {
            self.unregister_root_slot(slot)?;
        }
        Some(())
    }

    pub(super) fn end_local_root_scope(&mut self, unregister: bool, abandoned_root: Option<StackSlot>) -> Option<()> {
        (self.local_root_scopes.len() > 1).then_some(())?;
        if unregister {
            self.cleanup_scope_exit(self.local_root_scopes.len() - 1, abandoned_root)?;
        }
        let scope = self.local_root_scopes.pop()?;
        if unregister {
            for root in scope.temporaries.iter().rev() {
                if self.facts.checked_allocation_body()
                    && matches!(root, ScopedTemporaryRoot::Managed(slot) if Some(*slot) == abandoned_root)
                {
                    self.local_root_scopes.last_mut()?.temporaries.push(*root);
                } else {
                    self.emit_expression_root_release(*root)?;
                }
            }
            for (_, slot) in scope.bindings.iter().rev() {
                if let Some(slot) = slot {
                    self.unregister_root_slot(*slot)?;
                }
            }
        }
        for (local, _) in scope.bindings {
            self.locals.remove(&local);
        }
        Some(())
    }

    pub(super) fn end_local_root_scope_for_current_block(&mut self) -> Option<()> {
        let current = self.builder.current_block()?;
        self.end_local_root_scope(!block_is_terminated(self.builder, current), None)
    }
}
