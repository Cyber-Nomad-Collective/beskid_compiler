//! Checked canonical array owner transactions. No null result reaches a typed store.
use super::super::*;

impl IsleContext<'_, '_, '_, '_> {
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
        let owner = generated::constructor_lower_expression(self, *array_key)?;
        (self.builder.func.dfg.value_type(owner) == pointer).then_some(())?;
        self.builder.ins().trapz(owner, TrapCode::unwrap_user(1));
        let owner_root = self.root_temporary(owner)?;
        let (aggregate_base, aggregate_root) = self.collection_owner_base(key, *array_key, mutation_owner)?;
        let value = generated::constructor_lower_expression(self, *value_key)?;
        (self.builder.func.dfg.value_type(value) == element_type).then_some(())?;
        let value_root = self.root_temporary_if_needed(*value_key, value)?;
        let length_offset = i32::try_from(pointer.bytes()).ok()?;
        let length = self.builder.ins().load(pointer, MemFlagsData::new(), owner, length_offset);
        let next_length = self.builder.ins().iadd_imm_s(length, 1);
        let overflow = self.builder.ins().icmp(IntCC::UnsignedLessThanOrEqual, next_length, length);
        let grow_block = self.builder.create_block();
        self.builder.ins().brif(overflow, failure, &[], grow_block, &[]);
        self.builder.switch_to_block(grow_block);
        self.builder.seal_block(grow_block);
        let grow = self.import_runtime_helper(
            "beskid_rt_v5_array_try_grow",
            &[pointer, pointer],
            Some(pointer),
        )?;
        let minimum = self.growth_minimum_capacity(mutation_owner, owner, next_length)?;
        let grow_call = self.builder.ins().call(grow, &[owner, minimum]);
        let array = self.builder.inst_results(grow_call).first().copied()?;
        let succeeded = self.builder.ins().icmp_imm_u(IntCC::NotEqual, array, 0);
        let publish = self.builder.create_block();
        self.builder.ins().brif(succeeded, publish, &[], failure, &[]);
        self.builder.switch_to_block(publish);
        self.builder.seal_block(publish);
        let array_root = self.root_temporary(array)?;
        let data = self.builder.ins().load(pointer, MemFlagsData::new(), array, 0);
        let stride = element_type.bytes();
        let offset = if stride == 1 { length } else { self.builder.ins().imul_imm_s(length, i64::from(stride)) };
        let address = self.builder.ins().iadd(data, offset);
        // The slot at the old length holds null: the store deletes no reference, no barrier.
        self.builder.ins().store(MemFlagsData::new(), value, address, 0);
        self.builder.ins().store(MemFlagsData::new(), next_length, array, length_offset);
        self.publish_collection_owner(key, *array_key, mutation_owner, aggregate_base, array)?;
        self.release_temporary_root(Some(array_root))?;
        self.release_temporary_root(value_root)?;
        self.release_temporary_root(aggregate_root)?;
        self.release_temporary_root(Some(owner_root))?;
        let yes = self.builder.ins().iconst(types::I8, 1);
        self.builder.ins().jump(merge, &[yes.into()]);
        self.builder.switch_to_block(failure);
        self.builder.seal_block(failure);
        let no = self.builder.ins().iconst(types::I8, 0);
        self.builder.ins().jump(merge, &[no.into()]);
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        self.guard_checked_allocation()?;
        self.builder.block_params(merge).first().copied()
    }
}
