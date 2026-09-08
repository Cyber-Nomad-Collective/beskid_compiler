use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn local_managed_reference(
        &self,
        key: AstNodeKey,
        value_type: Type,
    ) -> Option<ManagedReferenceFact> {
        if value_type == dispatch::pointer_type() {
            self.facts.managed_reference(key)
        } else {
            Some(ManagedReferenceFact::NativeOrScalar)
        }
    }

    fn new_root_slot(&mut self, value: Value) -> StackSlot {
        let pointer = dispatch::pointer_type();
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            pointer.bytes(),
            pointer.bytes().ilog2() as u8,
        ));
        self.builder.ins().stack_store(value, slot, 0);
        slot
    }

    fn register_root_slot(&mut self, slot: StackSlot) -> Option<()> {
        let pointer = dispatch::pointer_type();
        let address = self.builder.ins().stack_addr(pointer, slot, 0);
        let register = self.import_runtime_helper("gc_register_root", &[pointer], Some(types::I8))?;
        let call = self.builder.ins().call(register, &[address]);
        let registered = self.builder.inst_results(call).first().copied()?;
        self.builder.ins().trapz(registered, TrapCode::unwrap_user(8));
        Some(())
    }

    fn unregister_root_slot(&mut self, slot: StackSlot) -> Option<()> {
        let pointer = dispatch::pointer_type();
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
            (value_type == dispatch::pointer_type()).then_some(())?;
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

    pub(super) fn assign_local(&mut self, slot: LocalSlotId, value: Value) -> Option<()> {
        let binding = self.locals.get(&slot).copied()?;
        (self.builder.func.dfg.value_type(value) == binding.value_type).then_some(())?;
        self.builder.def_var(binding.variable, value);
        if let Some(root_slot) = binding.root_slot {
            self.builder.ins().stack_store(value, root_slot, 0);
        }
        Some(())
    }

    pub(super) fn publish_managed_local(&mut self, slot: LocalSlotId, value: Value) -> Option<()> {
        let binding = self.locals.get(&slot).copied()?;
        (binding.managed_reference == ManagedReferenceFact::GcManaged && binding.root_slot.is_some()).then_some(())?;
        self.assign_local(slot, value)
    }

    pub(super) fn root_temporary(&mut self, value: Value) -> Option<StackSlot> {
        (self.builder.func.dfg.value_type(value) == dispatch::pointer_type()).then_some(())?;
        let slot = self.new_root_slot(value);
        self.register_root_slot(slot)?;
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
            self.unregister_root_slot(slot)?;
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

    pub(super) fn end_local_root_scope(&mut self, unregister: bool) -> Option<()> {
        (self.local_root_scopes.len() > 1).then_some(())?;
        let scope = self.local_root_scopes.pop()?;
        if unregister {
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
        self.end_local_root_scope(!block_is_terminated(self.builder, current))
    }
}
