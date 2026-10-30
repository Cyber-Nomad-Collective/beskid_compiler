//! Copy on assignment (v0.6.7): a new name of a `[CopyOnAssign]` value (a place read in a copy
//! position, an array `for` element, a closure capture) calls the type's `Copy` hook. The hook is
//! copy-on-write: the new value shares storage until its first mutation.

use super::*;
use crate::ValueCopyPlan;

impl IsleContext<'_, '_, '_, '_> {
    /// `value` was just read from the place `key`. When the place is in a copy position, return
    /// the hook's copy of it instead; otherwise return `value` unchanged.
    pub(super) fn copy_on_assign(&mut self, key: AstNodeKey, value: Value) -> Option<Value> {
        let Some(plan) = self.facts.value_copy(key) else {
            return self.mark_array_shared(key, value);
        };
        self.call_copy_hook(key, &plan, value)
    }

    /// Call the `Copy` hook of `plan` on `value` and return the copy.
    pub(super) fn call_copy_hook(&mut self, key: AstNodeKey, plan: &ValueCopyPlan, value: Value) -> Option<Value> {
        if self.facts.nonallocating_publication_body() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::ValueCopyInAllocationFreeBody });
            return None;
        }
        let function =
            match self.call_importer.as_deref_mut()?.import(self.builder, plan.callee.clone(), &plan.signature) {
                Ok(function) => function,
                Err(CallImportError::UnknownCallee) => {
                    self.pending_error =
                        Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(plan.callee.clone()) });
                    return None;
                }
            };
        // The source stays reachable through its place for the whole call, and the hook roots its
        // receiver on entry; the copy is a fresh result rooted by whichever name stores it.
        let call = self.builder.ins().call(function, &[value]);
        self.guard_checked_allocation()?;
        self.builder.inst_results(call).first().copied()
    }

    /// `value` is the array the place read or call at `key` produced. When that makes a second
    /// handle to the array, set its sharing mark (bit 0 of the hidden element-descriptor word just
    /// before the header): a later growth through any handle then copies the array instead of
    /// growing it in place, so growth never changes what another handle sees. A null array has
    /// no header, so its mark goes to a scratch slot instead (branch-free).
    pub(in crate::context) fn mark_array_shared(&mut self, key: AstNodeKey, value: Value) -> Option<Value> {
        if !self.facts.array_share(key) {
            return Some(value);
        }
        let pointer = dispatch::pointer_type(self.frontend_config);
        if self.builder.func.dfg.value_type(value) != pointer {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
            return None;
        }
        let scratch = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            pointer.bytes(),
            pointer.bytes().ilog2() as u8,
        ));
        let scratch = self.builder.ins().stack_addr(pointer, scratch, 0);
        let descriptor_word = self.builder.ins().iadd_imm_s(value, -(i64::from(pointer.bytes())));
        let address = self.builder.ins().select(value, descriptor_word, scratch);
        let word = self.builder.ins().load(pointer, MemFlagsData::new(), address, 0);
        let mark = self.builder.ins().iconst(pointer, 1);
        let marked = self.builder.ins().bor(word, mark);
        self.builder.ins().store(MemFlagsData::new(), marked, address, 0);
        Some(value)
    }

    /// Whether the place read at `key` calls a copy hook (and so allocates).
    pub(super) fn reads_with_copy(&self, key: AstNodeKey) -> bool {
        self.facts.value_copy(key).is_some()
    }
}
