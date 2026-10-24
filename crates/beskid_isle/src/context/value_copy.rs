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
            return Some(value);
        };
        self.call_copy_hook(key, &plan, value)
    }

    /// Call the `Copy` hook of `plan` on `value` and return the copy.
    pub(super) fn call_copy_hook(&mut self, key: AstNodeKey, plan: &ValueCopyPlan, value: Value) -> Option<Value> {
        if self.facts.nonallocating_publication_body() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::ValueCopyInAllocationFreeBody });
            return None;
        }
        let function = match self.call_importer.as_deref_mut()?.import(self.builder, plan.callee.clone(), &plan.signature)
        {
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

    /// Whether the place read at `key` calls a copy hook (and so allocates).
    pub(super) fn reads_with_copy(&self, key: AstNodeKey) -> bool {
        self.facts.value_copy(key).is_some()
    }
}
