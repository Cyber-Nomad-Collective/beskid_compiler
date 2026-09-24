//! Inline lambda calls and inline closure environments.

use super::super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(in crate::context) fn inline_lambda_call(&mut self, key: AstNodeKey) -> Option<Value> {
        let lambda = self.facts.inline_lambda_call(key)?;
        let arguments = self.facts.call_arguments(key)?;
        if arguments.len() != lambda.parameters.len() {
            return None;
        }
        let mut values = Vec::with_capacity(arguments.len());
        for (argument, parameter) in arguments.into_iter().zip(&lambda.parameters) {
            let value = generated::constructor_lower_expression(self, argument)?;
            (self.builder.func.dfg.value_type(value) == parameter.value_type).then_some(())?;
            let root = self.root_expression_value_if_needed(argument, value)?;
            values.push((value, parameter, root));
        }
        for (value, parameter, root) in values {
            (!self.locals.contains_key(&parameter.slot)).then_some(())?;
            self.bind_local(parameter.slot, value, parameter.value_type, parameter.managed_reference)?;
            self.release_expression_root(root)?;
        }
        if let Some(environment) = &lambda.closure_environment {
            let (_, root) = self.emit_inline_closure_environment(environment)?;
            self.release_temporary_root(Some(root))?;
        }
        let value = generated::constructor_lower_expression(self, lambda.body)?;
        (self.builder.func.dfg.value_type(value) == lambda.result_type).then_some(value)
    }

    pub(in crate::context) fn emit_inline_closure_environment(
        &mut self,
        environment: &InlineClosureEnvironment,
    ) -> Option<(Value, StackSlot)> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        let request = self.symbol_global(environment.allocation_request_symbol.as_ref(), pointer)?;
        let allocate =
            self.import_runtime_helper("beskid_rt_v5_closure_environment_allocate", &[pointer], Some(pointer))?;
        let allocate_call = self.builder.ins().call(allocate, &[request]);
        let env_ptr = self.builder.inst_results(allocate_call).first().copied()?;
        self.builder.ins().trapz(env_ptr, TrapCode::unwrap_user(5));
        let root = self.root_temporary(env_ptr)?;
        let descriptor = self.symbol_global(environment.descriptor_symbol.as_ref(), pointer)?;
        for capture in &environment.captures {
            let binding = self.locals.get(&capture.local_slot).copied()?;
            (binding.value_type == capture.value_type).then_some(())?;
            let value = self.builder.use_var(binding.variable);
            if let Some(map_index) = capture.pointer_map_index {
                let index = self.builder.ins().iconst(pointer, map_index as i64);
                let store = self.import_runtime_helper(
                    "beskid_rt_v5_closure_capture_store",
                    &[pointer, pointer, pointer, pointer],
                    Some(types::I8),
                )?;
                let store_call = self.builder.ins().call(store, &[env_ptr, descriptor, index, value]);
                let ok = self.builder.inst_results(store_call).first().copied()?;
                self.builder.ins().trapz(ok, TrapCode::unwrap_user(8));
            } else {
                let address = self.builder.ins().iadd_imm_s(env_ptr, i64::from(capture.field_offset));
                self.builder.ins().store(MemFlagsData::new(), value, address, 0);
            }
        }
        Some((env_ptr, root))
    }
}
