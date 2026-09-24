//! Bulk-parameter calls and collection operation lowering.

use super::super::*;

impl IsleContext<'_, '_, '_, '_> {
    /// Lower a `bulk`-parameter call.
    ///
    /// The callee declares one `bulk T[]` parameter, so its signature has a single array parameter
    /// while the call site passes N scalar arguments. This packs the N scalars into a fresh rooted
    /// array — reusing the exact `emit_array_literal` allocation/store/barrier/finish sequence —
    /// then direct-calls the callee with that array as its sole argument. It bypasses
    /// [`import_direct_call`], whose scalar-arity check cannot hold for a bulk call.
    pub(in crate::context) fn emit_bulk_call(&mut self, key: AstNodeKey) -> Option<Value> {
        let elements = self.facts.call_arguments(key)?;
        let layout = self.facts.array_layout(key)?;
        let allocation = self.facts.managed_array_allocation(key)?;
        if !layout.is_valid() || usize::try_from(layout.length).ok()? != elements.len() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
            return None;
        }
        let pointer = dispatch::pointer_type(self.frontend_config);
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
        let root = ScopedTemporaryRoot::ArrayConstruction(root_slot);
        self.track_expression_root(root)?;
        // `BeskidArray.ptr` remains at offset zero.  The backing bytes are owned by the same
        // descriptor-backed GC allocation; they are never a stack temporary.
        let data = self.builder.ins().load(pointer, MemFlagsData::new(), array, 0);
        for (index, element) in elements.into_iter().enumerate() {
            let value = generated::constructor_lower_expression(self, element)?;
            if self.builder.func.dfg.value_type(value) != layout.element_type {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
                return None;
            }
            let offset =
                u32::try_from(index).ok()?.checked_mul(layout.stride).and_then(|offset| i32::try_from(offset).ok())?;
            let address = self.builder.ins().iadd_imm_s(data, i64::from(offset));
            self.builder.ins().store(MemFlagsData::new(), value, address, 0);
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
        // Direct-call the callee with the packed array as its sole argument. The callee signature
        // has one array parameter, so this import bypasses `import_direct_call`'s scalar-arity
        // check (N scalars vs. one array parameter would otherwise fail it).
        let callee = self.facts.direct_callee(key)?;
        let signature = self.facts.call_signature(key)?;
        let result_type = self.facts.scalar_type(key)?;
        if signature.params.len() != 1
            || signature.params[0].value_type != pointer
            || signature.returns.len() != 1
            || signature.returns[0].value_type != result_type
        {
            return None;
        }
        let function = match self.call_importer.as_deref_mut()?.import(self.builder, callee.clone(), &signature) {
            Ok(function) => function,
            Err(CallImportError::UnknownCallee) => {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(callee) });
                return None;
            }
        };
        let call = self.builder.ins().call(function, &[array]);
        self.release_expression_root(Some(root))?;
        self.builder.inst_results(call).first().copied()
    }

    pub(in crate::context) fn emit_collection_operation_value(&mut self, key: AstNodeKey) -> Option<Value> {
        let operation = self.facts.collection_operation(key)?;
        let arguments = self.facts.call_arguments(key)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let word = pointer;
        let element_type = self.facts.collection_element_type(key)?;
        let stride = element_type.bytes();
        match operation {
            CollectionOperation::UnprovenMutationOwner => {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                None
            }
            CollectionOperation::Capacity => {
                let [array] = arguments.as_slice() else { return None };
                let array = generated::constructor_lower_expression(self, *array)?;
                (self.builder.func.dfg.value_type(array) == pointer).then_some(())?;
                self.builder.ins().trapz(array, TrapCode::unwrap_user(1));
                Some(self.builder.ins().load(
                    word,
                    MemFlagsData::new(),
                    array,
                    i32::try_from(pointer.bytes() * 2).ok()?,
                ))
            }
            CollectionOperation::Append { owner: mutation_owner } => {
                let [array_key, value_key] = arguments.as_slice() else { return None };
                let owner = generated::constructor_lower_expression(self, *array_key)?;
                (self.builder.func.dfg.value_type(owner) == pointer).then_some(())?;
                self.builder.ins().trapz(owner, TrapCode::unwrap_user(1));
                let aggregate_base = match mutation_owner {
                    CollectionMutationOwner::Local(slot) => {
                        let binding = self.locals.get(&slot).copied()?;
                        if binding.value_type != pointer
                            || binding.managed_reference != ManagedReferenceFact::GcManaged
                            || binding.root_slot.is_none()
                            || self.facts.local_slot(*array_key) != Some(slot)
                        {
                            self.pending_error =
                                Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                            return None;
                        }
                        None
                    }
                    CollectionMutationOwner::AggregateField { receiver, .. } => {
                        let binding = self.locals.get(&receiver).copied()?;
                        if binding.value_type != pointer
                            || binding.managed_reference != ManagedReferenceFact::GcManaged
                            || binding.root_slot.is_none()
                        {
                            self.pending_error =
                                Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                            return None;
                        }
                        Some(self.builder.use_var(binding.variable))
                    }
                };
                let value = generated::constructor_lower_expression(self, *value_key)?;
                (self.builder.func.dfg.value_type(value) == element_type).then_some(())?;
                let value_root = self.root_temporary_if_needed(*value_key, value)?;
                let length_offset = i32::try_from(pointer.bytes()).ok()?;
                let length = self.builder.ins().load(word, MemFlagsData::new(), owner, length_offset);
                let next_length = self.builder.ins().iadd_imm_s(length, 1);
                let overflow = self.builder.ins().icmp(IntCC::UnsignedLessThanOrEqual, next_length, length);
                self.builder.ins().trapnz(overflow, TrapCode::unwrap_user(3));
                let root_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    pointer.bytes(),
                    pointer.bytes().ilog2() as u8,
                ));
                let root_out = self.builder.ins().stack_addr(pointer, root_slot, 0);
                let grow = self.import_runtime_helper(
                    "beskid_rt_v5_array_grow_rooted",
                    &[pointer, word, pointer],
                    Some(pointer),
                )?;
                let grow_call = self.builder.ins().call(grow, &[owner, next_length, root_out]);
                let array = self.builder.inst_results(grow_call).first().copied()?;
                self.builder.ins().trapz(array, TrapCode::unwrap_user(5));
                let data = self.builder.ins().load(pointer, MemFlagsData::new(), array, 0);
                let offset =
                    if stride == 1 { length } else { self.builder.ins().imul_imm_s(length, i64::from(stride)) };
                let address = self.builder.ins().iadd(data, offset);
                self.builder.ins().store(MemFlagsData::new(), value, address, 0);
                if element_type == pointer {
                    let barrier = self.import_runtime_helper(
                        "beskid_rt_v5_array_write_barrier",
                        &[pointer, pointer],
                        Some(types::I8),
                    )?;
                    let call = self.builder.ins().call(barrier, &[array, value]);
                    let published = self.builder.inst_results(call).first().copied()?;
                    self.builder.ins().trapz(published, TrapCode::unwrap_user(8));
                }
                self.builder.ins().store(MemFlagsData::new(), next_length, array, length_offset);
                match mutation_owner {
                    CollectionMutationOwner::Local(slot) => {
                        self.publish_managed_local(slot, array)?;
                    }
                    CollectionMutationOwner::AggregateField { receiver, field_index } => {
                        let layout = self.facts.struct_layout(*array_key)?;
                        let Some(field) =
                            usize::try_from(field_index).ok().and_then(|index| layout.fields.get(index)).copied()
                        else {
                            self.pending_error =
                                Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                            return None;
                        };
                        let base = aggregate_base?;
                        if self.locals.get(&receiver)?.value_type != pointer || field.value_type != pointer {
                            self.pending_error =
                                Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                            return None;
                        }
                        self.builder.ins().store(MemFlagsData::new(), array, base, i32::try_from(field.offset).ok()?);
                        // No write-barrier call: collection is stop-the-world, so a managed store can
                        // never race a concurrent marker. `gc_write_barrier` stays exported for a future
                        // incremental collector (see docs/superpowers/specs/2026-09-22-gc-span-heap-design.md
                        // section 6.3), but generated code no longer calls it.
                    }
                }
                let root_handle = self.builder.ins().stack_load(pointer, pointer, root_slot, 0);
                let finish =
                    self.import_runtime_helper("beskid_rt_v5_array_construction_finish", &[pointer], Some(types::I8))?;
                let finish_call = self.builder.ins().call(finish, &[root_handle]);
                let released = self.builder.inst_results(finish_call).first().copied()?;
                self.builder.ins().trapz(released, TrapCode::unwrap_user(10));
                self.release_temporary_root(value_root)?;
                Some(array)
            }
            CollectionOperation::Clear => {
                let [array_key, index_key] = arguments.as_slice() else { return None };
                let array = generated::constructor_lower_expression(self, *array_key)?;
                let index = generated::constructor_lower_expression(self, *index_key)?;
                (self.builder.func.dfg.value_type(array) == pointer
                    && self.builder.func.dfg.value_type(index).is_int())
                .then_some(())?;
                self.builder.ins().trapz(array, TrapCode::unwrap_user(1));
                let length =
                    self.builder.ins().load(word, MemFlagsData::new(), array, i32::try_from(pointer.bytes()).ok()?);
                let out_of_bounds = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
                self.builder.ins().trapnz(out_of_bounds, TrapCode::HEAP_OUT_OF_BOUNDS);
                let data = self.builder.ins().load(pointer, MemFlagsData::new(), array, 0);
                let offset = if stride == 1 { index } else { self.builder.ins().imul_imm_s(index, i64::from(stride)) };
                let address = self.builder.ins().iadd(data, offset);
                let zero = if element_type == types::F64 {
                    self.builder.ins().f64const(Ieee64::with_float(0.0))
                } else {
                    self.builder.ins().iconst(element_type, 0)
                };
                self.builder.ins().store(MemFlagsData::new(), zero, address, 0);
                Some(array)
            }
            CollectionOperation::RemoveLast => {
                let [array_key] = arguments.as_slice() else { return None };
                let array = generated::constructor_lower_expression(self, *array_key)?;
                (self.builder.func.dfg.value_type(array) == pointer).then_some(())?;
                self.builder.ins().trapz(array, TrapCode::unwrap_user(1));
                let length_offset = i32::try_from(pointer.bytes()).ok()?;
                let length = self.builder.ins().load(word, MemFlagsData::new(), array, length_offset);
                let empty = self.builder.ins().icmp_imm_s(IntCC::Equal, length, 0);
                self.builder.ins().trapnz(empty, TrapCode::HEAP_OUT_OF_BOUNDS);
                let next_length = self.builder.ins().iadd_imm_s(length, -1);
                let data = self.builder.ins().load(pointer, MemFlagsData::new(), array, 0);
                let offset = if stride == 1 {
                    next_length
                } else {
                    self.builder.ins().imul_imm_s(next_length, i64::from(stride))
                };
                let address = self.builder.ins().iadd(data, offset);
                let zero = if element_type == types::F64 {
                    self.builder.ins().f64const(Ieee64::with_float(0.0))
                } else {
                    self.builder.ins().iconst(element_type, 0)
                };
                self.builder.ins().store(MemFlagsData::new(), zero, address, 0);
                self.builder.ins().store(MemFlagsData::new(), next_length, array, length_offset);
                Some(array)
            }
        }
    }
}
