//! The one lowering seam for barriered stores into existing managed objects.

use super::*;
use crate::write_barrier::{GC_BARRIER_ACTIVE_SYMBOL, GC_WRITE_BARRIER_SYMBOL};

impl IsleContext<'_, '_, '_, '_> {
    /// Whether a store to the place `target` (a field or element expression) of the statement
    /// `key` overwrites a GC-managed reference. The managed-reference fact of the target is the
    /// only authority, the same fact that decides managed-local rooting; a target without one is
    /// a lowering error, never a store without a barrier.
    pub(crate) fn store_overwrites_managed_reference(&mut self, key: AstNodeKey, target: AstNodeKey) -> Option<bool> {
        let Some(fact) = self.facts.managed_reference(target) else {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::MissingManagedReferenceFact });
            return None;
        };
        Some(fact == ManagedReferenceFact::GcManaged)
    }

    /// Store `value` into the GC-managed pointer slot at `slot` inside an existing managed object,
    /// preceded by the guarded hybrid barrier: load the process word
    /// `beskid_rt_v5_gc_barrier_active`, branch on it, call `beskid_rt_v5_gc_write_barrier(slot,
    /// value)` in a cold block when it is non-zero, and store in the common successor. The idle
    /// path makes no call; the barrier returns no value, so no `trapz` follows it.
    pub(crate) fn emit_barriered_store(&mut self, slot: Value, value: Value) -> Option<()> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        (self.builder.func.dfg.value_type(slot) == pointer && self.builder.func.dfg.value_type(value) == pointer)
            .then_some(())?;
        let flag = self.symbol_global(GC_BARRIER_ACTIVE_SYMBOL, pointer)?;
        let active = self.builder.ins().load(pointer, MemFlagsData::new(), flag, 0);
        let barrier_block = self.builder.create_block();
        let store_block = self.builder.create_block();
        self.builder.set_cold_block(barrier_block);
        self.builder.ins().brif(active, barrier_block, &[], store_block, &[]);
        self.builder.switch_to_block(barrier_block);
        self.builder.seal_block(barrier_block);
        let barrier = self.import_runtime_helper(GC_WRITE_BARRIER_SYMBOL, &[pointer, pointer], None)?;
        self.builder.ins().call(barrier, &[slot, value]);
        self.builder.ins().jump(store_block, &[]);
        self.builder.switch_to_block(store_block);
        self.builder.seal_block(store_block);
        self.builder.ins().store(MemFlagsData::new(), value, slot, 0);
        Some(())
    }

    /// Store `value` into the slot `offset` bytes into `object`, barriered when `managed`.
    pub(crate) fn emit_slot_store(&mut self, object: Value, offset: i32, value: Value, managed: bool) -> Option<()> {
        if managed {
            let slot = self.builder.ins().iadd_imm_s(object, i64::from(offset));
            self.emit_barriered_store(slot, value)
        } else {
            self.builder.ins().store(MemFlagsData::new(), value, object, offset);
            Some(())
        }
    }
}
