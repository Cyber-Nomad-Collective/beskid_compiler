//! Copy on assignment (v0.6.7): a place read of a `[CopyOnAssign]` value in a copy position
//! (a binding, an assignment, a non-`mut` argument, a construction or a return of a shared place)
//! calls the type's `Copy` hook, so the position stores a value with its own storage.

use super::*;

impl IsleContext<'_, '_, '_, '_> {
    /// `value` was just read from the place `key`. When the place is in a copy position, return
    /// the hook's copy of it instead; otherwise return `value` unchanged.
    pub(super) fn copy_on_assign(&mut self, key: AstNodeKey, value: Value) -> Option<Value> {
        let Some(plan) = self.facts.value_copy(key) else {
            return Some(value);
        };
        if self.facts.nonallocating_publication_body() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::ValueCopyInAllocationFreeBody });
            return None;
        }
        let function = match self.call_importer.as_deref_mut()?.import(self.builder, plan.callee.clone(), &plan.signature)
        {
            Ok(function) => function,
            Err(CallImportError::UnknownCallee) => {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(plan.callee) });
                return None;
            }
        };
        // The source stays reachable through its place for the whole call, and the hook roots its
        // receiver on entry; the copy is a fresh result rooted by whichever position stores it.
        let call = self.builder.ins().call(function, &[value]);
        self.guard_checked_allocation()?;
        self.builder.inst_results(call).first().copied()
    }

    /// Whether the place read at `key` calls a copy hook (and so allocates).
    pub(super) fn reads_with_copy(&self, key: AstNodeKey) -> bool {
        self.facts.value_copy(key).is_some()
    }
}
