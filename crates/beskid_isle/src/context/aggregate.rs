use super::*;

impl IsleContext<'_, '_, '_, '_> {
    /// Prefer a proven local receiver slot (nominal `local.field` paths). Fall back to
    /// lowering the expression child so temporary struct literals still work.
    pub(super) fn field_base_pointer(&mut self, field_key: AstNodeKey) -> Option<Value> {
        let base = if let Some(receiver_slot) = self.facts.field_receiver_slot(field_key) {
            let (receiver, receiver_type) = self.locals.get(&receiver_slot).copied()?;
            let base = self.builder.use_var(receiver);
            if self.builder.func.dfg.value_type(base) != receiver_type {
                return None;
            }
            base
        } else {
            let base_key = self.facts.child(field_key, 0)?;
            generated::constructor_lower_expression(self, base_key)?
        };
        self.builder.func.dfg.value_type(base).is_int().then_some(base)
    }
}

macro_rules! generated_aggregate_methods {
    () => {
        fn index_target(&mut self, key: AstNodeKey) -> Option<IndexTarget> {
            if self.facts.index_target_is_string(key) {
                return Some(IndexTarget::String);
            }
            if self.facts.array_layout(key).is_some() {
                return Some(IndexTarget::Array);
            }
            None
        }
        fn emit_array_literal(&mut self, key: AstNodeKey) -> Option<Value> {
            let elements = self.facts.array_elements(key)?;
            let layout = self.facts.array_layout(key)?;
            let allocation = self.facts.managed_array_allocation(key)?;
            if !layout.is_valid() || usize::try_from(layout.length).ok()? != elements.len() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
                return None;
            }
            let pointer = dispatch::pointer_type();
            let request = self.symbol_global(allocation.allocation_request_symbol.as_ref(), pointer)?;
            let root_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                pointer.bytes(),
                pointer.bytes().ilog2() as u8,
            ));
            let root_slot_address = self.builder.ins().stack_addr(pointer, root_slot, 0);
            let allocate =
                self.import_runtime_helper("beskid_rt_v5_array_allocate_rooted", &[pointer, pointer], Some(pointer))?;
            let allocation_call = self.builder.ins().call(allocate, &[request, root_slot_address]);
            let array = self.builder.inst_results(allocation_call).first().copied()?;
            self.builder.ins().trapz(array, TrapCode::unwrap_user(5));
            // `BeskidArray.ptr` remains at offset zero.  The backing bytes are owned by the same
            // descriptor-backed GC allocation; they are never a stack temporary.
            let data = self.builder.ins().load(pointer, MemFlags::new(), array, 0);
            for (index, element) in elements.into_iter().enumerate() {
                let value = generated::constructor_lower_expression(self, element)?;
                if self.builder.func.dfg.value_type(value) != layout.element_type {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
                    return None;
                }
                let offset = u32::try_from(index)
                    .ok()?
                    .checked_mul(layout.stride)
                    .and_then(|offset| i32::try_from(offset).ok())?;
                let address = self.builder.ins().iadd_imm(data, i64::from(offset));
                self.builder.ins().store(MemFlags::new(), value, address, 0);
                if layout.element_type == pointer {
                    let barrier = self.import_runtime_helper(
                        "beskid_rt_v5_array_write_barrier",
                        &[pointer, pointer],
                        Some(types::I8),
                    )?;
                    let barrier_call = self.builder.ins().call(barrier, &[array, value]);
                    let published = self.builder.inst_results(barrier_call).first().copied()?;
                    self.builder.ins().trapz(published, TrapCode::unwrap_user(8));
                }
            }
            // The allocation was rooted before the first nested element was lowered. Release only
            // after every store and pointer-publication barrier has completed.
            let root_handle = self.builder.ins().stack_load(pointer, root_slot, 0);
            let finish =
                self.import_runtime_helper("beskid_rt_v5_array_construction_finish", &[pointer], Some(types::I8))?;
            let finish_call = self.builder.ins().call(finish, &[root_handle]);
            let released = self.builder.inst_results(finish_call).first().copied()?;
            self.builder.ins().trapz(released, TrapCode::unwrap_user(10));
            Some(array)
        }

        fn emit_index_read(&mut self, key: AstNodeKey) -> Option<Value> {
            let layout = self.facts.array_layout(key)?;
            if !layout.is_valid() || self.facts.scalar_type(key)? != layout.element_type {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
                return None;
            }
            let base_key = self.facts.child(key, 0)?;
            let index_key = self.facts.child(key, 1)?;
            let base = generated::constructor_lower_expression(self, base_key)?;
            let index = generated::constructor_lower_expression(self, index_key)?;
            let index_type = self.builder.func.dfg.value_type(index);
            let pointer_type = self.builder.func.dfg.value_type(base);
            if !index_type.is_int() || !pointer_type.is_int() {
                return None;
            }
            self.builder.ins().trapz(base, TrapCode::unwrap_user(1));
            let pointer_index = if index_type.bits() < pointer_type.bits() {
                self.builder.ins().uextend(pointer_type, index)
            } else if index_type.bits() > pointer_type.bits() {
                self.builder.ins().ireduce(pointer_type, index)
            } else {
                index
            };
            let length =
                self.builder.ins().load(pointer_type, MemFlags::new(), base, i32::try_from(pointer_type.bytes()).ok()?);
            let out_of_bounds = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, pointer_index, length);
            self.builder.ins().trapnz(out_of_bounds, TrapCode::HEAP_OUT_OF_BOUNDS);
            let offset = if layout.stride == 1 {
                pointer_index
            } else {
                self.builder.ins().imul_imm(pointer_index, i64::from(layout.stride))
            };
            let data = self.builder.ins().load(pointer_type, MemFlags::new(), base, 0);
            let address = self.builder.ins().iadd(data, offset);
            Some(self.builder.ins().load(layout.element_type, MemFlags::new(), address, 0))
        }

        fn emit_index_assign(&mut self, key: AstNodeKey) -> Option<Value> {
            let target = self.facts.child(key, 0)?;
            let value_key = self.facts.child(key, 1)?;
            let layout = self.facts.array_layout(target)?;
            if !layout.is_valid() || self.facts.scalar_type(key)? != layout.element_type {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
                return None;
            }
            let base_key = self.facts.child(target, 0)?;
            let index_key = self.facts.child(target, 1)?;
            let base = generated::constructor_lower_expression(self, base_key)?;
            let index = generated::constructor_lower_expression(self, index_key)?;
            let value = generated::constructor_lower_expression(self, value_key)?;
            if self.builder.func.dfg.value_type(value) != layout.element_type {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
                return None;
            }
            let index_type = self.builder.func.dfg.value_type(index);
            let pointer_type = self.builder.func.dfg.value_type(base);
            if !index_type.is_int() || !pointer_type.is_int() {
                return None;
            }
            self.builder.ins().trapz(base, TrapCode::unwrap_user(1));
            let pointer_index = if index_type.bits() < pointer_type.bits() {
                self.builder.ins().uextend(pointer_type, index)
            } else if index_type.bits() > pointer_type.bits() {
                self.builder.ins().ireduce(pointer_type, index)
            } else {
                index
            };
            let length =
                self.builder.ins().load(pointer_type, MemFlags::new(), base, i32::try_from(pointer_type.bytes()).ok()?);
            let out_of_bounds = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, pointer_index, length);
            self.builder.ins().trapnz(out_of_bounds, TrapCode::HEAP_OUT_OF_BOUNDS);
            let offset = if layout.stride == 1 {
                pointer_index
            } else {
                self.builder.ins().imul_imm(pointer_index, i64::from(layout.stride))
            };
            let data = self.builder.ins().load(pointer_type, MemFlags::new(), base, 0);
            let address = self.builder.ins().iadd(data, offset);
            self.builder.ins().store(MemFlags::new(), value, address, 0);
            if layout.element_type == pointer_type {
                let barrier = self.import_runtime_helper(
                    "beskid_rt_v5_array_write_barrier",
                    &[pointer_type, pointer_type],
                    Some(types::I8),
                )?;
                let barrier_call = self.builder.ins().call(barrier, &[base, value]);
                let published = self.builder.inst_results(barrier_call).first().copied()?;
                self.builder.ins().trapz(published, TrapCode::unwrap_user(8));
            }
            Some(value)
        }

        fn emit_struct_literal(&mut self, key: AstNodeKey) -> Option<Value> {
            let fields = self.facts.struct_fields(key)?;
            let layout = self.facts.struct_layout(key)?;
            if !layout.is_valid() || fields.len() != layout.fields.len() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                return None;
            }
            let allocation = self.facts.managed_struct_allocation(key)?;
            let mut values = Vec::with_capacity(fields.len());
            for (field_key, field_layout) in fields.into_iter().zip(&layout.fields) {
                let value = generated::constructor_lower_expression(self, field_key)?;
                if self.builder.func.dfg.value_type(value) != field_layout.value_type {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                    return None;
                }
                values.push((value, *field_layout));
            }
            let pointer_type = self.facts.scalar_type(key)?;
            if !pointer_type.is_int() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                return None;
            }
            let request = self.symbol_global(allocation.allocation_request_symbol.as_ref(), pointer_type)?;
            let allocate = self.import_runtime_helper(
                "beskid_rt_v5_managed_object_allocate",
                &[pointer_type],
                Some(pointer_type),
            )?;
            let allocation_call = self.builder.ins().call(allocate, &[request]);
            let object = self.builder.inst_results(allocation_call).first().copied()?;
            self.builder.ins().trapz(object, TrapCode::unwrap_user(5));
            for (value, field_layout) in values {
                let address = self.builder.ins().iadd_imm(object, i64::from(field_layout.offset));
                self.builder.ins().store(MemFlags::new(), value, address, 0);
            }
            Some(object)
        }

        fn emit_field_read(&mut self, key: AstNodeKey) -> Option<Value> {
            let layout = self.facts.struct_layout(key)?;
            if !layout.is_valid() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                return None;
            }
            let field_index = self.facts.field_index(key)?;
            let Some(field) = usize::try_from(field_index).ok().and_then(|index| layout.fields.get(index)).copied()
            else {
                self.pending_error =
                    Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructField(field_index) });
                return None;
            };
            if self.facts.scalar_type(key)? != field.value_type {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                return None;
            }
            let base = self.field_base_pointer(key)?;
            Some(self.builder.ins().load(field.value_type, MemFlags::new(), base, i32::try_from(field.offset).ok()?))
        }

        fn emit_field_assign(&mut self, key: AstNodeKey) -> Option<Value> {
            let target = self.facts.child(key, 0)?;
            let value_key = self.facts.child(key, 1)?;
            let layout = self.facts.struct_layout(target)?;
            if !layout.is_valid() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                return None;
            }
            let field_index = self.facts.field_index(target)?;
            let Some(field) = usize::try_from(field_index).ok().and_then(|index| layout.fields.get(index)).copied()
            else {
                self.pending_error =
                    Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructField(field_index) });
                return None;
            };
            let base = self.field_base_pointer(target)?;
            let value = generated::constructor_lower_expression(self, value_key)?;
            if self.builder.func.dfg.value_type(value) != field.value_type
                || self.facts.scalar_type(key)? != field.value_type
            {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                return None;
            }
            self.builder.ins().store(MemFlags::new(), value, base, i32::try_from(field.offset).ok()?);
            Some(value)
        }
    };
}

pub(super) use generated_aggregate_methods;
