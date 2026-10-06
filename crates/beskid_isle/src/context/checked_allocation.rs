//! Checked clone exits release only already-admitted compiler roots, without allocating.
use super::*;

impl IsleContext<'_, '_, '_, '_> {
    /// Every checked helper shares the outer invocation's domain-owned scope. Missing entry
    /// context is a protocol violation, never a fabricated recoverable allocation failure.
    pub(crate) fn require_checked_scope(&mut self) -> Option<()> {
        if !self.facts.checked_allocation_body() {
            return Some(());
        }
        let pointer = dispatch::pointer_type(self.frontend_config);
        let current = self.import_runtime_helper("beskid_rt_v5_checked_scope_current", &[], Some(pointer))?;
        let call = self.builder.ins().call(current, &[]);
        let scope = self.builder.inst_results(call).first().copied()?;
        self.builder.ins().trapz(scope, TrapCode::unwrap_user(10));
        self.guard_checked_allocation()
    }

    /// Branch before the first managed dereference/store or publication after fallible work.
    /// Each edge captures its precise live-root set; compile-time scopes remain available on
    /// the successful edge. Child return bits are private and cannot become a source result.
    pub(super) fn guard_checked_allocation(&mut self) -> Option<()> {
        if !self.facts.checked_allocation_body() {
            return Some(());
        }
        let pointer = dispatch::pointer_type(self.frontend_config);
        let status = self.import_runtime_helper("beskid_rt_v5_checked_scope_failure_reason", &[], Some(pointer))?;
        let call = self.builder.ins().call(status, &[]);
        let reason = self.builder.inst_results(call).first().copied()?;
        let failed = self.builder.ins().icmp_imm_u(cranelift_codegen::ir::condcodes::IntCC::NotEqual, reason, 0);
        let failure = self.builder.create_block();
        let continuation = self.builder.create_block();
        self.builder.ins().brif(failed, failure, &[], continuation, &[]);
        self.builder.switch_to_block(failure);
        self.builder.seal_block(failure);
        self.release_managed_local_roots()?;
        let returns = self.builder.func.signature.returns.iter().map(|value| value.value_type).collect::<Vec<_>>();
        let mut values = Vec::with_capacity(returns.len());
        for ty in returns {
            values.push(if ty == types::F32 {
                self.builder.ins().f32const(cranelift_codegen::ir::immediates::Ieee32::with_bits(0))
            } else if ty == types::F64 {
                self.builder.ins().f64const(cranelift_codegen::ir::immediates::Ieee64::with_bits(0))
            } else if ty.is_int() {
                self.builder.ins().iconst(ty, 0)
            } else {
                return None;
            });
        }
        self.builder.ins().return_(&values);
        self.builder.switch_to_block(continuation);
        self.builder.seal_block(continuation);
        Some(())
    }
}
