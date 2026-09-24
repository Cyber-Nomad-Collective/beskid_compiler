//! Traced fiber-join value moves and channel sends.

use super::super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn traced_value_move_call(
        &mut self,
        key: AstNodeKey,
        result_type: Option<Type>,
    ) -> Option<Option<Value>> {
        let layout = self.facts.traced_fiber_join_layout(key)?;
        let arguments = self.facts.call_arguments(key)?;
        let [argument] = arguments.as_slice() else {
            return None;
        };
        let handle = generated::constructor_lower_expression(self, *argument)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            layout.slot_size,
            layout.alignment_shift,
        ));
        let zero = self.builder.ins().iconst(pointer, 0);
        for offset in (0..layout.slot_size).step_by(pointer.bytes() as usize) {
            self.builder.ins().stack_store(self.frontend_config.pointer_type(), zero, slot, offset as i32);
        }
        let destination = self.builder.ins().stack_addr(pointer, slot, 0);
        let transfer = self.import_runtime_helper(layout.symbol, &[types::I64, pointer], Some(types::I8))?;
        let call = self.builder.ins().call(transfer, &[handle, destination]);
        let moved = self.builder.inst_results(call)[0];
        self.builder.ins().trapz(moved, TrapCode::unwrap_user(10));
        let payload = self.builder.ins().stack_load(pointer, pointer, slot, layout.payload_offset);
        let value =
            result_type.map(|ty| self.builder.ins().load(ty, MemFlagsData::new(), payload, layout.value_offset));
        // Clear is non-allocating: no safepoint exists between the rooted payload
        // read and the caller installing the ordinary result/local root.
        let clear = self.import_runtime_helper("beskid_rt_v5_abi_value_clear", &[pointer], Some(types::I8))?;
        let call = self.builder.ins().call(clear, &[destination]);
        let cleared = self.builder.inst_results(call)[0];
        self.builder.ins().trapz(cleared, TrapCode::unwrap_user(10));
        Some(value)
    }

    pub(super) fn traced_channel_send_call(&mut self, key: AstNodeKey) -> Option<Value> {
        let layout = self.facts.traced_channel_send_layout(key)?;
        let arguments = self.facts.call_arguments(key)?;
        let [handle, boxed] = arguments.as_slice() else {
            return None;
        };
        let handle = generated::constructor_lower_expression(self, *handle)?;
        let boxed = generated::constructor_lower_expression(self, *boxed)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            layout.slot_size,
            layout.alignment_shift,
        ));
        let zero = self.builder.ins().iconst(pointer, 0);
        for offset in (0..layout.slot_size).step_by(pointer.bytes() as usize) {
            self.builder.ins().stack_store(self.frontend_config.pointer_type(), zero, slot, offset as i32);
        }
        let owner = self.builder.ins().stack_addr(pointer, slot, 0);
        let descriptor = self.builder.ins().load(pointer, MemFlagsData::new(), boxed, 0);
        let tag = self.builder.ins().iconst(pointer, 1);
        let init = self.import_runtime_helper(
            "beskid_rt_v5_abi_value_initialize",
            &[pointer, pointer, pointer, pointer],
            Some(types::I8),
        )?;
        let call = self.builder.ins().call(init, &[owner, tag, boxed, descriptor]);
        let initialized = self.builder.inst_results(call)[0];
        self.builder.ins().trapz(initialized, TrapCode::unwrap_user(10));
        let send = self.import_runtime_helper(layout.symbol, &[types::I64, pointer], Some(types::I64))?;
        let call = self.builder.ins().call(send, &[handle, owner]);
        let status = self.builder.inst_results(call)[0];
        // A precommit failure releases this adapter's tracing root only. The
        // source sender still has its original value; no disposal is implicit.
        let clear = self.import_runtime_helper("beskid_rt_v5_abi_value_clear", &[pointer], Some(types::I8))?;
        let call = self.builder.ins().call(clear, &[owner]);
        let cleared = self.builder.inst_results(call)[0];
        self.builder.ins().trapz(cleared, TrapCode::unwrap_user(10));
        Some(status)
    }
}
