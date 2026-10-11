//! Inline lambda calls and inline closure environments.

use super::super::*;
use cranelift_codegen::ir::BlockArg;

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

impl IsleContext<'_, '_, '_, '_> {
    /// Call through the closure record held by a function-typed local value.
    ///
    /// Arguments are evaluated first; the callee is a plain local read with no effects, so the
    /// record is loaded only after every argument has been computed and rooted, and nothing can
    /// collect between that load and the indirect call. The record itself stays reachable through
    /// the local's own root for the whole call. A null environment selects the capture-free entry
    /// convention, otherwise the environment pointer is passed first. A value position requires a
    /// result before anything is emitted.
    pub(in crate::context) fn function_value_call(&mut self, key: AstNodeKey, value: bool) -> Option<Option<Value>> {
        let plan = self.facts.function_value_call(key)?;
        (!value || plan.result.is_some()).then_some(())?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let argument_keys = self.facts.call_arguments(key)?;
        (argument_keys.len() == plan.parameters.len()).then_some(())?;
        let mut arguments = Vec::with_capacity(argument_keys.len());
        let mut roots = Vec::with_capacity(argument_keys.len());
        for (argument, parameter) in argument_keys.into_iter().zip(plan.parameters.iter().copied()) {
            let value = generated::constructor_lower_expression(self, argument)?;
            let value = if self.builder.func.dfg.value_type(value) == parameter {
                value
            } else {
                self.adapt_scalar_boundary(argument, value, parameter)?
            };
            roots.push(self.root_expression_value_if_needed(argument, value)?);
            arguments.push(value);
        }
        (self.facts.node_kind(plan.callee) == Some(NodeKind::PathExpression)).then_some(())?;
        let record = generated::constructor_lower_expression(self, plan.callee)?;
        (self.builder.func.dfg.value_type(record) == pointer).then_some(())?;
        self.builder.ins().trapz(record, TrapCode::unwrap_user(5));
        let code = self.builder.ins().load(pointer, MemFlagsData::new(), record, plan.code_offset);
        let environment = self.builder.ins().load(pointer, MemFlagsData::new(), record, plan.environment_offset);

        let capture_free = self.builder.create_block();
        let captured = self.builder.create_block();
        let merge = self.builder.create_block();
        if let Some(result) = plan.result {
            self.builder.append_block_param(merge, result);
        }
        let has_environment = self.builder.ins().icmp_imm_s(IntCC::NotEqual, environment, 0);
        self.builder.ins().brif(has_environment, captured, &[], capture_free, &[]);

        self.builder.switch_to_block(capture_free);
        self.builder.seal_block(capture_free);
        let mut plain_signature = Signature::new(self.builder.func.signature.call_conv);
        plain_signature.params.extend(plan.parameters.iter().copied().map(AbiParam::new));
        plain_signature.returns.extend(plan.result.map(AbiParam::new));
        let plain_signature = self.builder.func.import_signature(plain_signature);
        let plain_call = self.builder.ins().call_indirect(plain_signature, code, &arguments);
        let plain_results: Vec<BlockArg> =
            self.builder.inst_results(plain_call).iter().copied().map(BlockArg::Value).collect();
        self.builder.ins().jump(merge, &plain_results);

        self.builder.switch_to_block(captured);
        self.builder.seal_block(captured);
        let mut closure_signature = Signature::new(self.builder.func.signature.call_conv);
        closure_signature.params.push(AbiParam::new(pointer));
        closure_signature.params.extend(plan.parameters.iter().copied().map(AbiParam::new));
        closure_signature.returns.extend(plan.result.map(AbiParam::new));
        let closure_signature = self.builder.func.import_signature(closure_signature);
        let mut closure_arguments = Vec::with_capacity(arguments.len() + 1);
        closure_arguments.push(environment);
        closure_arguments.extend(arguments.iter().copied());
        let closure_call = self.builder.ins().call_indirect(closure_signature, code, &closure_arguments);
        let closure_results: Vec<BlockArg> =
            self.builder.inst_results(closure_call).iter().copied().map(BlockArg::Value).collect();
        self.builder.ins().jump(merge, &closure_results);

        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        let result = self.builder.block_params(merge).first().copied();
        for root in roots.into_iter().rev() {
            self.release_expression_root(root)?;
        }
        Some(result)
    }
}
