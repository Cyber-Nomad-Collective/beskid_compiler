//! Checked canonical array owner transactions. No null result reaches a typed store.
use super::super::*;

impl IsleContext<'_, '_, '_, '_> {
    fn checked_snapshot_root(&mut self, value: Value, slot: StackSlot, failure: Block) -> Option<()> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        self.builder.ins().stack_store(pointer, value, slot, 0);
        let address = self.builder.ins().stack_addr(pointer, slot, 0);
        let register = self.import_runtime_helper("beskid_rt_v5_gc_try_register_root", &[pointer], Some(types::I8))?;
        let call = self.builder.ins().call(register, &[address]);
        let admitted = self.builder.inst_results(call).first().copied()?;
        let next = self.builder.create_block();
        self.builder.ins().brif(admitted, next, &[], failure, &[]);
        self.builder.switch_to_block(next);
        self.builder.seal_block(next);
        if self.facts.checked_allocation_body() {
            self.track_expression_root(ScopedTemporaryRoot::Managed(slot))?;
        }
        Some(())
    }

    pub(in crate::context) fn emit_checked_array_append(
        &mut self,
        key: AstNodeKey,
        mutation_owner: CollectionMutationOwner,
        element_type: Type,
    ) -> Option<Value> {
        let arguments = self.facts.call_arguments(key)?;
        let [array_key, value_key] = arguments.as_slice() else { return None };
        let pointer = dispatch::pointer_type(self.frontend_config);
        let failure = self.builder.create_block();
        let merge = self.builder.create_block();
        self.builder.append_block_param(merge, types::I8);
        let zero = self.builder.ins().iconst(pointer, 0);
        let mut snapshots = Vec::with_capacity(3);
        for _ in 0..3 {
            let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                pointer.bytes(),
                pointer.bytes().ilog2() as u8,
            ));
            self.builder.ins().stack_store(pointer, zero, slot, 0);
            snapshots.push(slot);
        }
        let owner = generated::constructor_lower_expression(self, *array_key)?;
        (self.builder.func.dfg.value_type(owner) == pointer).then_some(())?;
        self.builder.ins().trapz(owner, TrapCode::unwrap_user(1));
        self.checked_snapshot_root(owner, snapshots[0], failure)?;
        let aggregate_base = match mutation_owner {
            CollectionMutationOwner::Local(slot) => {
                let binding = self.locals.get(&slot).copied()?;
                if binding.value_type != pointer
                    || binding.managed_reference != ManagedReferenceFact::GcManaged
                    || binding.root_slot.is_none()
                    || self.facts.local_slot(*array_key) != Some(slot)
                {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                    return None;
                }
                None
            }
            CollectionMutationOwner::AggregateField { root, receiver, .. } => {
                let binding = self.locals.get(&root).copied()?;
                if binding.value_type != pointer
                    || binding.managed_reference != ManagedReferenceFact::GcManaged
                    || binding.root_slot.is_none()
                {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                    return None;
                }
                let base = if self.facts.local_slot(receiver) == Some(root) {
                    self.builder.use_var(binding.variable)
                } else {
                    generated::constructor_lower_expression(self, receiver)?
                };
                (self.builder.func.dfg.value_type(base) == pointer).then_some(())?;
                self.builder.ins().trapz(base, TrapCode::unwrap_user(1));
                self.checked_snapshot_root(base, snapshots[1], failure)?;
                Some(base)
            }
        };
        let value = generated::constructor_lower_expression(self, *value_key)?;
        (self.builder.func.dfg.value_type(value) == element_type).then_some(())?;
        if self.facts.managed_reference(*value_key)? == ManagedReferenceFact::GcManaged {
            self.checked_snapshot_root(value, snapshots[2], failure)?;
        }
        let length_offset = i32::try_from(pointer.bytes()).ok()?;
        let length = self.builder.ins().load(pointer, MemFlagsData::new(), owner, length_offset);
        let next_length = self.builder.ins().iadd_imm_s(length, 1);
        let overflow = self.builder.ins().icmp(IntCC::UnsignedLessThanOrEqual, next_length, length);
        let grow_block = self.builder.create_block();
        self.builder.ins().brif(overflow, failure, &[], grow_block, &[]);
        self.builder.switch_to_block(grow_block);
        self.builder.seal_block(grow_block);
        let root_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            pointer.bytes(),
            pointer.bytes().ilog2() as u8,
        ));
        let root_out = self.builder.ins().stack_addr(pointer, root_slot, 0);
        let grow = self.import_runtime_helper(
            "beskid_rt_v5_array_try_grow_rooted",
            &[pointer, pointer, pointer],
            Some(pointer),
        )?;
        let grow_call = self.builder.ins().call(grow, &[owner, next_length, root_out]);
        let array = self.builder.inst_results(grow_call).first().copied()?;
        let succeeded = self.builder.ins().icmp_imm_u(IntCC::NotEqual, array, 0);
        let publish = self.builder.create_block();
        self.builder.ins().brif(succeeded, publish, &[], failure, &[]);
        self.builder.switch_to_block(publish);
        self.builder.seal_block(publish);
        let data = self.builder.ins().load(pointer, MemFlagsData::new(), array, 0);
        let stride = element_type.bytes();
        let offset = if stride == 1 { length } else { self.builder.ins().imul_imm_s(length, i64::from(stride)) };
        let address = self.builder.ins().iadd(data, offset);
        self.builder.ins().store(MemFlagsData::new(), value, address, 0);
        if element_type == pointer {
            let barrier =
                self.import_runtime_helper("beskid_rt_v5_array_write_barrier", &[pointer, pointer], Some(types::I8))?;
            let call = self.builder.ins().call(barrier, &[array, value]);
            let accepted = self.builder.inst_results(call).first().copied()?;
            self.builder.ins().trapz(accepted, TrapCode::unwrap_user(8));
        }
        self.builder.ins().store(MemFlagsData::new(), next_length, array, length_offset);
        match mutation_owner {
            CollectionMutationOwner::Local(slot) => {
                self.publish_managed_local(slot, array)?;
            }
            CollectionMutationOwner::AggregateField { root, field_index, .. } => {
                let layout = self.facts.struct_layout(*array_key)?;
                let field = usize::try_from(field_index).ok().and_then(|i| layout.fields.get(i)).copied().flatten()?;
                if self.locals.get(&root)?.value_type != pointer || field.value_type != pointer {
                    return None;
                }
                self.builder.ins().store(
                    MemFlagsData::new(),
                    array,
                    aggregate_base?,
                    i32::try_from(field.offset).ok()?,
                );
            }
        }
        let handle = self.builder.ins().stack_load(pointer, pointer, root_slot, 0);
        let finish =
            self.import_runtime_helper("beskid_rt_v5_array_construction_finish", &[pointer], Some(types::I8))?;
        let call = self.builder.ins().call(finish, &[handle]);
        let released = self.builder.inst_results(call).first().copied()?;
        self.builder.ins().trapz(released, TrapCode::unwrap_user(10));
        for slot in snapshots.iter().rev() {
            self.unregister_root_slot(*slot)?;
        }
        let yes = self.builder.ins().iconst(types::I8, 1);
        self.builder.ins().jump(merge, &[yes.into()]);
        self.builder.switch_to_block(failure);
        self.builder.seal_block(failure);
        for slot in snapshots.iter().rev() {
            self.unregister_root_slot(*slot)?;
        }
        let no = self.builder.ins().iconst(types::I8, 0);
        self.builder.ins().jump(merge, &[no.into()]);
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        for slot in snapshots {
            self.forget_checked_snapshot(slot);
        }
        self.guard_checked_allocation()?;
        self.builder.block_params(merge).first().copied()
    }
}
