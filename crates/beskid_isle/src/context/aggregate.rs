use super::*;

impl IsleContext<'_, '_, '_, '_> {
    /// Prefer a proven local receiver slot (nominal `local.field` paths). Fall back to
    /// lowering the expression child so temporary struct literals still work.
    pub(super) fn field_base_pointer(&mut self, field_key: AstNodeKey) -> Option<Value> {
        let base = if let Some(receiver_slot) = self.facts.field_receiver_slot(field_key) {
            let receiver = self.locals.get(&receiver_slot).copied()?;
            let base = self.builder.use_var(receiver.variable);
            if self.builder.func.dfg.value_type(base) != receiver.value_type {
                return None;
            }
            base
        } else {
            let base_key = self.facts.child(field_key, 0)?;
            generated::constructor_lower_expression(self, base_key)?
        };
        self.builder.func.dfg.value_type(base).is_int().then_some(base)
    }

    /// The validated layout slot of the field that the field expression `key` names, as its CLIF
    /// type and byte offset.
    pub(crate) fn field_slot(&mut self, key: AstNodeKey) -> Option<(Type, i32)> {
        let layout = self.facts.struct_layout(key)?;
        if !layout.is_valid() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
            return None;
        }
        let field_index = self.facts.field_index(key)?;
        let Some(field) = usize::try_from(field_index).ok().and_then(|index| layout.fields.get(index)).copied() else {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructField(field_index) });
            return None;
        };
        let field = field?;
        if self.facts.scalar_type(key)? != field.value_type {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
            return None;
        }
        Some((field.value_type, i32::try_from(field.offset).ok()?))
    }

    /// Evaluates the operands of the indexed base `key` (the `values` of `values[index]`): the
    /// receiver of a field, the operands and index of an element. The base itself is read later
    /// by [`Self::read_indexed_base`], after the index (and an assigned value) are evaluated, so
    /// a side effect of either on the place, such as a growth that replaces the array a field
    /// holds, is observed. A base that is not a place (a call, a literal) is its value.
    pub(crate) fn indexed_base_operands(&mut self, key: AstNodeKey) -> Option<IndexedBase> {
        match self.facts.node_kind(key)? {
            NodeKind::PathExpression
                if self.facts.constant_integer(key).is_none()
                    && self.facts.function_item_value(key).is_none()
                    && self.facts.field_index(key).is_none()
                    && self.facts.local_slot(key).is_some_and(|slot| self.locals.contains_key(&slot)) =>
            {
                Some(IndexedBase::Local(self.facts.local_slot(key)?))
            }
            NodeKind::FieldExpression => {
                let (value_type, offset) = self.field_slot(key)?;
                let receiver = self.field_base_pointer(key)?;
                Some(IndexedBase::Field { receiver, value_type, offset })
            }
            NodeKind::IndexExpression => {
                let layout = self.array_index_layout(key)?;
                let array = self.indexed_base_operands(self.facts.child(key, 0)?)?;
                let index = generated::constructor_lower_expression(self, self.facts.child(key, 1)?)?;
                Some(IndexedBase::Element { array: Box::new(array), index, layout })
            }
            _ => Some(IndexedBase::Value(generated::constructor_lower_expression(self, key)?)),
        }
    }

    /// Reads the indexed base whose operands [`Self::indexed_base_operands`] evaluated.
    pub(crate) fn read_indexed_base(&mut self, base: IndexedBase) -> Option<Value> {
        match base {
            IndexedBase::Local(slot) => {
                let binding = self.locals.get(&slot).copied()?;
                Some(self.builder.use_var(binding.variable))
            }
            IndexedBase::Field { receiver, value_type, offset } => {
                Some(self.builder.ins().load(value_type, MemFlagsData::new(), receiver, offset))
            }
            IndexedBase::Element { array, index, layout } => {
                let array = self.read_indexed_base(*array)?;
                let address = self.checked_element_address(array, index, layout)?;
                Some(self.builder.ins().load(layout.element_type, MemFlagsData::new(), address, 0))
            }
            IndexedBase::Value(value) => Some(value),
        }
    }

    /// The validated array layout of the index expression `key`, whose value is one element.
    pub(crate) fn array_index_layout(&mut self, key: AstNodeKey) -> Option<crate::layout::ArrayLayout> {
        let layout = self.facts.array_layout(key)?;
        if !layout.is_valid() || self.facts.scalar_type(key)? != layout.element_type {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
            return None;
        }
        Some(layout)
    }

    /// The address of element `index` of `array`, after the null and bounds checks.
    pub(crate) fn checked_element_address(&mut self, array: Value, index: Value, layout: crate::layout::ArrayLayout) -> Option<Value> {
        let index_type = self.builder.func.dfg.value_type(index);
        let pointer_type = self.builder.func.dfg.value_type(array);
        if !index_type.is_int() || !pointer_type.is_int() {
            return None;
        }
        self.builder.ins().trapz(array, TrapCode::unwrap_user(1));
        let pointer_index = if index_type.bits() < pointer_type.bits() {
            self.builder.ins().uextend(pointer_type, index)
        } else if index_type.bits() > pointer_type.bits() {
            self.builder.ins().ireduce(pointer_type, index)
        } else {
            index
        };
        let length = self.builder.ins().load(
            pointer_type,
            MemFlagsData::new(),
            array,
            i32::try_from(pointer_type.bytes()).ok()?,
        );
        let out_of_bounds = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, pointer_index, length);
        self.builder.ins().trapnz(out_of_bounds, TrapCode::HEAP_OUT_OF_BOUNDS);
        let offset = if layout.stride == 1 {
            pointer_index
        } else {
            self.builder.ins().imul_imm_s(pointer_index, i64::from(layout.stride))
        };
        let data = self.builder.ins().load(pointer_type, MemFlagsData::new(), array, 0);
        Some(self.builder.ins().iadd(data, offset))
    }
}

/// The base of an index expression with its operands evaluated and its storage not yet read.
pub(crate) enum IndexedBase {
    /// A local binding, read when the base is read.
    Local(LocalSlotId),
    /// A field of an evaluated receiver.
    Field { receiver: Value, value_type: Type, offset: i32 },
    /// An element of an array base, at an evaluated index.
    Element { array: Box<IndexedBase>, index: Value, layout: crate::layout::ArrayLayout },
    /// A base that is not a place, already evaluated.
    Value(Value),
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
            if let Some(length) = self.facts.sized_array_length(key) {
                return self.emit_sized_array_allocation(key, length);
            }
            let elements = self.facts.array_elements(key)?;
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
            self.guard_checked_allocation()?;
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
                let offset = u32::try_from(index)
                    .ok()?
                    .checked_mul(layout.stride)
                    .and_then(|offset| i32::try_from(offset).ok())?;
                let address = self.builder.ins().iadd_imm_s(data, i64::from(offset));
                // An initialization store into the array this literal allocated: the old value is
                // null and an array allocated while marking is black, so no barrier is needed.
                self.builder.ins().store(MemFlagsData::new(), value, address, 0);
            }
            // The allocation was rooted before the first nested element was lowered. Release only
            // after every element store has completed.
            self.release_expression_root(Some(root))?;
            Some(array)
        }

        fn emit_index_read(&mut self, key: AstNodeKey) -> Option<Value> {
            let layout = self.array_index_layout(key)?;
            let base = self.indexed_base_operands(self.facts.child(key, 0)?)?;
            let index = generated::constructor_lower_expression(self, self.facts.child(key, 1)?)?;
            let array = self.read_indexed_base(base)?;
            let address = self.checked_element_address(array, index, layout)?;
            Some(self.builder.ins().load(layout.element_type, MemFlagsData::new(), address, 0))
        }

        fn emit_index_assign(&mut self, key: AstNodeKey) -> Option<Value> {
            let target = self.facts.child(key, 0)?;
            let value_key = self.facts.child(key, 1)?;
            let layout = self.facts.array_layout(target)?;
            if !layout.is_valid() || self.facts.scalar_type(key)? != layout.element_type {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
                return None;
            }
            // The base's operands, the index, and the value are evaluated in source order; the
            // array is read from the base after them (language-meta/evaluation/control-flow,
            // "Index expression evaluation order").
            let base = self.indexed_base_operands(self.facts.child(target, 0)?)?;
            let index = generated::constructor_lower_expression(self, self.facts.child(target, 1)?)?;
            let value = generated::constructor_lower_expression(self, value_key)?;
            if self.builder.func.dfg.value_type(value) != layout.element_type {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
                return None;
            }
            let managed = self.store_overwrites_managed_reference(key, target)?;
            let array = self.read_indexed_base(base)?;
            let address = self.checked_element_address(array, index, layout)?;
            if managed {
                self.emit_barriered_store(address, value)?;
            } else {
                self.builder.ins().store(MemFlagsData::new(), value, address, 0);
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
                let Some(field_layout) = field_layout else {
                    if !self.is_unit_value(field_key) {
                        self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                        return None;
                    }
                    self.lower_expression_for_effect(field_key)?;
                    continue;
                };
                let value = generated::constructor_lower_expression(self, field_key)?;
                if self.builder.func.dfg.value_type(value) != field_layout.value_type {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                    return None;
                }
                let root = self.root_expression_value_if_needed(field_key, value)?;
                values.push((value, *field_layout, root));
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
            self.guard_checked_allocation()?;
            self.builder.ins().trapz(object, TrapCode::unwrap_user(5));
            for (value, field_layout, _) in &values {
                let address = self.builder.ins().iadd_imm_s(object, i64::from(field_layout.offset));
                self.builder.ins().store(MemFlagsData::new(), *value, address, 0);
            }
            for (_, _, root) in values.into_iter().rev() {
                self.release_expression_root(root)?;
            }
            Some(object)
        }

        fn emit_field_read(&mut self, key: AstNodeKey) -> Option<Value> {
            let (value_type, offset) = self.field_slot(key)?;
            let base = self.field_base_pointer(key)?;
            Some(self.builder.ins().load(value_type, MemFlagsData::new(), base, offset))
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
            let field = field?;
            let base = self.field_base_pointer(target)?;
            let value = generated::constructor_lower_expression(self, value_key)?;
            if self.builder.func.dfg.value_type(value) != field.value_type
                || self.facts.scalar_type(key)? != field.value_type
            {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidStructLayout });
                return None;
            }
            let managed = self.store_overwrites_managed_reference(key, target)?;
            self.emit_slot_store(base, i32::try_from(field.offset).ok()?, value, managed)?;
            Some(value)
        }
    };
}

pub(super) use generated_aggregate_methods;
