//! Eager `spawn Entry(args)` argument transfer into the fiber's managed start environment.

use super::*;
use crate::facts::SpawnArgumentEnvironment;

impl IsleContext<'_, '_, '_, '_> {
    /// Evaluate every spawn argument in source order, then allocate and fill the managed start
    /// environment that the generated spawn trampoline reads.
    ///
    /// Managed argument values stay rooted until the environment holds them; the environment
    /// itself is returned with its temporary root, which the caller releases only after the
    /// runtime has taken ownership of it. A value whose ABI type differs from the entry
    /// parameter fails closed.
    pub(super) fn emit_spawn_argument_environment(
        &mut self,
        environment: &SpawnArgumentEnvironment,
    ) -> Option<(Value, StackSlot)> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        let mut values = Vec::with_capacity(environment.fields.len());
        for field in &environment.fields {
            let value = generated::constructor_lower_expression(self, field.argument)?;
            (self.builder.func.dfg.value_type(value) == field.value_type).then_some(())?;
            let root = self.root_expression_value_if_needed(field.argument, value)?;
            values.push((value, field.field_offset, root));
        }
        let request = self.symbol_global(environment.allocation_request_symbol.as_ref(), pointer)?;
        let allocate =
            self.import_runtime_helper("beskid_rt_v5_managed_object_allocate", &[pointer], Some(pointer))?;
        let allocation = self.builder.ins().call(allocate, &[request]);
        let object = self.builder.inst_results(allocation).first().copied()?;
        self.builder.ins().trapz(object, TrapCode::unwrap_user(5));
        let root = self.root_temporary(object)?;
        // Collection is stop-the-world and the environment is rooted, so plain field stores
        // publish managed arguments exactly like aggregate field initialization.
        for (value, offset, _) in &values {
            self.builder.ins().store(MemFlagsData::new(), *value, object, *offset);
        }
        for (_, _, argument_root) in values.into_iter().rev() {
            self.release_expression_root(argument_root)?;
        }
        Some((object, root))
    }
}
