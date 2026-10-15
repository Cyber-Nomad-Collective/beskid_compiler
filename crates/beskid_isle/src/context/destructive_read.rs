//! `move` expressions and the `exchange(place, move value)` builtin
//! (`language-meta--memory-model--memory-and-references`, "Destructive read of a field").
//!
//! `move x` is a transfer: ownership is a static fact, so it lowers as its operand and stores
//! nothing. `move x.f` reads the field and stores the canonical empty value of the field's type
//! into it in the same step; `exchange(place, move v)` reads the place and stores `v` into it.
//! Both stores overwrite a pointer slot of an existing object and go through
//! [`IsleContext::emit_barriered_store`]; the empty value itself is a new object, so its
//! initialization stores carry no barrier.

use super::*;
use crate::facts::{CanonicalEmptyPlan, ExchangePlan};
use crate::layout::ManagedArrayAllocation;

impl IsleContext<'_, '_, '_, '_> {
    /// A descriptor-backed array of length zero, allocated through its static request (whose
    /// length word is zero) and finished at once: it has no element to initialize.
    fn allocate_empty_array(&mut self, allocation: &ManagedArrayAllocation) -> Option<Value> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        let request = self.symbol_global(allocation.allocation_request_symbol.as_ref(), pointer)?;
        let root_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            pointer.bytes(),
            pointer.bytes().ilog2() as u8,
        ));
        let root_address = self.builder.ins().stack_addr(pointer, root_slot, 0);
        let allocate =
            self.import_runtime_helper("beskid_rt_v5_array_allocate_rooted", &[pointer, pointer], Some(pointer))?;
        let call = self.builder.ins().call(allocate, &[request, root_address]);
        let array = self.builder.inst_results(call).first().copied()?;
        self.guard_checked_allocation()?;
        self.builder.ins().trapz(array, TrapCode::unwrap_user(5));
        let root_handle = self.builder.ins().stack_load(pointer, pointer, root_slot, 0);
        let finish =
            self.import_runtime_helper("beskid_rt_v5_array_construction_finish", &[pointer], Some(types::I8))?;
        let finish_call = self.builder.ins().call(finish, &[root_handle]);
        let released = self.builder.inst_results(finish_call).first().copied()?;
        self.builder.ins().trapz(released, TrapCode::unwrap_user(10));
        Some(array)
    }

    /// The canonical empty value `empty`: `Option::None`, the empty array or the empty list.
    fn emit_canonical_empty(&mut self, empty: &CanonicalEmptyPlan) -> Option<Value> {
        match empty {
            CanonicalEmptyPlan::OptionNone { layout, allocation, variant } => {
                self.allocate_enum_variant(allocation, layout, usize::try_from(*variant).ok()?)
            }
            CanonicalEmptyPlan::EmptyArray { allocation } => self.allocate_empty_array(allocation),
            CanonicalEmptyPlan::EmptyList { storage, list, storage_offset, count_offset, count_type } => {
                let pointer = dispatch::pointer_type(self.frontend_config);
                let array = self.allocate_empty_array(storage)?;
                let root = self.root_expression_value(array)?;
                let request = self.symbol_global(list.allocation_request_symbol.as_ref(), pointer)?;
                let allocate =
                    self.import_runtime_helper("beskid_rt_v5_managed_object_allocate", &[pointer], Some(pointer))?;
                let call = self.builder.ins().call(allocate, &[request]);
                let object = self.builder.inst_results(call).first().copied()?;
                self.guard_checked_allocation()?;
                self.builder.ins().trapz(object, TrapCode::unwrap_user(5));
                // Initialization stores into the list this step allocated: no barrier.
                self.builder.ins().store(MemFlagsData::new(), array, object, *storage_offset);
                let zero = self.builder.ins().iconst(*count_type, 0);
                self.builder.ins().store(MemFlagsData::new(), zero, object, *count_offset);
                self.release_expression_root(Some(root))?;
                Some(object)
            }
        }
    }

    /// `move x.f`: the empty value is built first and rooted, then the receiver is evaluated,
    /// the field is read and the empty value is stored into it behind the guarded barrier. The
    /// read value is the result.
    pub(super) fn emit_destructive_read(&mut self, key: AstNodeKey, place: AstNodeKey, empty: &CanonicalEmptyPlan) -> Option<Value> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        if self.facts.node_kind(place) != Some(NodeKind::FieldExpression) {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidDestructiveRead });
            return None;
        }
        // Every canonical empty value is a managed object, so the field must be a traced
        // pointer slot.
        if !self.store_overwrites_managed_reference(key, place)? {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidDestructiveRead });
            return None;
        }
        let (value_type, offset) = self.field_slot(place)?;
        if value_type != pointer {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidDestructiveRead });
            return None;
        }
        let empty = self.emit_canonical_empty(empty)?;
        let root = self.root_expression_value(empty)?;
        let receiver = self.field_base_pointer(place)?;
        let old = self.builder.ins().load(value_type, MemFlagsData::new(), receiver, offset);
        self.emit_slot_store(receiver, offset, empty, true)?;
        self.release_expression_root(Some(root))?;
        Some(old)
    }

    /// `exchange(place, move value)`: the place's operands, then the value, are evaluated in
    /// source order; the place is read and the value stored into it, barriered when the place
    /// is a managed slot. The read value is the result.
    pub(super) fn emit_exchange_call(&mut self, key: AstNodeKey, plan: ExchangePlan) -> Option<Value> {
        let invalid = |context: &mut Self| -> Option<Value> {
            context.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidExchange });
            None
        };
        match self.facts.node_kind(plan.place) {
            Some(NodeKind::FieldExpression) => {
                let (value_type, offset) = self.field_slot(plan.place)?;
                let receiver = self.field_base_pointer(plan.place)?;
                let value = generated::constructor_lower_expression(self, plan.value)?;
                if self.builder.func.dfg.value_type(value) != value_type {
                    return invalid(self);
                }
                let managed = self.store_overwrites_managed_reference(key, plan.place)?;
                let old = self.builder.ins().load(value_type, MemFlagsData::new(), receiver, offset);
                self.emit_slot_store(receiver, offset, value, managed)?;
                Some(old)
            }
            Some(NodeKind::IndexExpression) => {
                let layout = self.array_index_layout(plan.place)?;
                let base = self.indexed_base_operands(self.facts.child(plan.place, 0)?)?;
                let index = generated::constructor_lower_expression(self, self.facts.child(plan.place, 1)?)?;
                let value = generated::constructor_lower_expression(self, plan.value)?;
                if self.builder.func.dfg.value_type(value) != layout.element_type {
                    return invalid(self);
                }
                let managed = self.store_overwrites_managed_reference(key, plan.place)?;
                let array = self.read_indexed_base(base)?;
                let address = self.checked_element_address(array, index, layout)?;
                let old = self.builder.ins().load(layout.element_type, MemFlagsData::new(), address, 0);
                if managed {
                    self.emit_barriered_store(address, value)?;
                } else {
                    self.builder.ins().store(MemFlagsData::new(), value, address, 0);
                }
                Some(old)
            }
            _ => invalid(self),
        }
    }
}

macro_rules! generated_destructive_read_methods {
    () => {
        fn emit_move_expression(&mut self, key: AstNodeKey) -> Option<Value> {
            match self.facts.move_expression(key)? {
                crate::facts::MoveExpressionPlan::Transfer { operand } => generated::constructor_lower_expression(self, operand),
                crate::facts::MoveExpressionPlan::DestructiveRead { place, empty } => self.emit_destructive_read(key, place, &empty),
            }
        }

        fn emit_exchange(&mut self, key: AstNodeKey) -> Option<Value> {
            let plan = self.facts.exchange(key)?;
            self.emit_exchange_call(key, plan)
        }
    };
}

pub(super) use generated_destructive_read_methods;
