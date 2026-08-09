use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn import_direct_call(&mut self, key: AstNodeKey) -> Option<(cranelift_codegen::ir::Inst, Signature)> {
        let callee = self.facts.direct_callee(key)?;
        let signature = self.facts.call_signature(key)?;
        let function = match self.call_importer.as_deref_mut()?.import(self.builder, callee.clone(), &signature) {
            Ok(function) => function,
            Err(CallImportError::UnknownCallee) => {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(callee) });
                return None;
            }
        };
        let argument_keys = self.facts.call_arguments(key)?;
        if argument_keys.len() != signature.params.len() {
            return None;
        }
        let mut arguments = Vec::with_capacity(argument_keys.len());
        for (argument, parameter) in argument_keys.into_iter().zip(&signature.params) {
            let value = generated::constructor_lower_expression(self, argument)?;
            let value = if self.builder.func.dfg.value_type(value) == parameter.value_type {
                value
            } else {
                self.materialize_canonical_runtime_direct_constant(argument, parameter.value_type)?
            };
            arguments.push(value);
        }
        let call = self.builder.ins().call(function, &arguments);
        Some((call, signature))
    }

    /// Re-materialize a compiler-owned runtime layout constant at the exact
    /// ABI type of its direct-call parameter.  The source grammar intentionally
    /// keeps module constants untyped; this is therefore narrowly contextual,
    /// requires compiler-minted authority, and never coerces arbitrary values.
    pub(super) fn materialize_canonical_runtime_direct_constant(
        &mut self,
        key: AstNodeKey,
        expected: Type,
    ) -> Option<Value> {
        (self.facts.node_kind(key) == Some(NodeKind::PathExpression)).then_some(())?;
        let value = self.facts.canonical_runtime_constant_integer(key)?;
        if value < 0 || !expected.is_int() {
            return None;
        }
        let width = expected.bits();
        if width < 64 && u64::try_from(value).ok()? > ((1_u64 << width) - 1) {
            return None;
        }
        Some(self.builder.ins().iconst(expected, value))
    }
    pub(super) fn direct_call(&mut self, key: AstNodeKey) -> Option<Value> {
        let signature = self.facts.call_signature(key)?;
        let result_type = self.facts.scalar_type(key)?;
        if signature.returns.len() != 1 || signature.returns[0].value_type != result_type {
            return None;
        }
        let (call, _) = self.import_direct_call(key)?;
        self.builder.inst_results(call).first().copied()
    }

    pub(super) fn inline_lambda_call(&mut self, key: AstNodeKey) -> Option<Value> {
        let lambda = self.facts.inline_lambda_call(key)?;
        let arguments = self.facts.call_arguments(key)?;
        if arguments.len() != lambda.parameters.len() {
            return None;
        }
        let mut values = Vec::with_capacity(arguments.len());
        for (argument, parameter) in arguments.into_iter().zip(&lambda.parameters) {
            let value = generated::constructor_lower_expression(self, argument)?;
            (self.builder.func.dfg.value_type(value) == parameter.value_type).then_some(())?;
            values.push((value, parameter));
        }
        for (value, parameter) in values {
            (!self.locals.contains_key(&parameter.slot)).then_some(())?;
            let variable = self.builder.declare_var(parameter.value_type);
            self.builder.def_var(variable, value);
            self.locals.insert(parameter.slot, (variable, parameter.value_type));
        }
        if let Some(environment) = &lambda.closure_environment {
            let _env = self.emit_inline_closure_environment(environment)?;
        }
        let value = generated::constructor_lower_expression(self, lambda.body)?;
        (self.builder.func.dfg.value_type(value) == lambda.result_type).then_some(value)
    }

    pub(super) fn emit_inline_closure_environment(&mut self, environment: &InlineClosureEnvironment) -> Option<Value> {
        let pointer = dispatch::pointer_type();
        let request = self.symbol_global(environment.allocation_request_symbol.as_ref(), pointer)?;
        let allocate =
            self.import_runtime_helper("beskid_rt_v5_closure_environment_allocate", &[pointer], Some(pointer))?;
        let allocate_call = self.builder.ins().call(allocate, &[request]);
        let env_ptr = self.builder.inst_results(allocate_call).first().copied()?;
        self.builder.ins().trapz(env_ptr, TrapCode::unwrap_user(5));
        let descriptor = self.symbol_global(environment.descriptor_symbol.as_ref(), pointer)?;
        for capture in &environment.captures {
            let (variable, value_type) = self.locals.get(&capture.local_slot).copied()?;
            (value_type == capture.value_type).then_some(())?;
            let value = self.builder.use_var(variable);
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
                let address = self.builder.ins().iadd_imm(env_ptr, i64::from(capture.field_offset));
                self.builder.ins().store(MemFlags::new(), value, address, 0);
            }
        }
        let slot = self.builder.ins().iconst(pointer, environment.root_slot_index as i64);
        let root = self.import_runtime_helper(
            "beskid_rt_v5_closure_environment_root_current",
            &[pointer, pointer],
            Some(types::I8),
        )?;
        let root_call = self.builder.ins().call(root, &[slot, env_ptr]);
        let rooted = self.builder.inst_results(root_call).first().copied()?;
        self.builder.ins().trapz(rooted, TrapCode::unwrap_user(8));
        Some(env_ptr)
    }

    pub(super) fn symbol_global(&mut self, symbol: &str, pointer: Type) -> Option<Value> {
        let global = self.builder.func.create_global_value(GlobalValueData::Symbol {
            name: ExternalName::testcase(symbol),
            offset: 0.into(),
            colocated: true,
            tls: false,
        });
        Some(self.builder.ins().global_value(pointer, global))
    }

    pub(super) fn import_runtime_helper(
        &mut self,
        symbol: &str,
        params: &[Type],
        result: Option<Type>,
    ) -> Option<FuncRef> {
        let mut signature = Signature::new(self.builder.func.signature.call_conv);
        signature.params.extend(params.iter().copied().map(AbiParam::new));
        if let Some(result) = result {
            signature.returns.push(AbiParam::new(result));
        }
        let signature = self.builder.func.import_signature(signature);
        Some(self.builder.func.import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase(symbol),
            signature,
            colocated: false,
            patchable: false,
        }))
    }

    pub(super) fn direct_call_statement(&mut self, key: AstNodeKey) -> Option<()> {
        self.facts.call_signature(key)?.returns.is_empty().then_some(())?;
        let (call, _) = self.import_direct_call(key)?;
        self.builder.inst_results(call).is_empty().then_some(())
    }
}

macro_rules! generated_call_methods {
    () => {
        fn emit_direct_call(&mut self, key: AstNodeKey) -> Option<Value> {
            self.direct_call(key)
        }

        fn emit_direct_call_statement(&mut self, key: AstNodeKey) -> Option<()> {
            self.direct_call_statement(key)
        }

        fn emit_dispatch_call(&mut self, key: AstNodeKey) -> Option<Value> {
            let symbol = self.facts.dispatch_builtin_symbol(key)?;
            let arguments = self
                .facts
                .call_arguments(key)?
                .into_iter()
                .map(|argument| generated::constructor_lower_expression(self, argument))
                .collect::<Option<Vec<_>>>()?;
            // Dispatch route (interop envelope) — string operations, syscalls, etc.
            if let Some(route) = beskid_abi::dispatch_route_for_symbol(symbol) {
                let returns_value = !matches!(route.group, beskid_abi::DispatchReturnGroup::Unit);
                return match dispatch::emit_dispatch_call(self.builder, route, &arguments, returns_value) {
                    Ok(Some(value)) => Some(value),
                    Ok(None) | Err(_) => None,
                };
            }
            // No dispatch route — treat as a direct extern call (math builtins, etc.).
            let signature = self.facts.call_signature(key)?;
            let mut ext_sig = cranelift_codegen::ir::Signature::new(CallConv::SystemV);
            for arg in &signature.params {
                ext_sig.params.push(*arg);
            }
            for ret in &signature.returns {
                ext_sig.returns.push(*ret);
            }
            let sig_ref = self.builder.func.import_signature(ext_sig);
            let func_ref = self.builder.func.import_function(cranelift_codegen::ir::ExtFuncData {
                name: ExternalName::testcase(symbol),
                signature: sig_ref,
                colocated: false,
                patchable: false,
            });
            let call = self.builder.ins().call(func_ref, &arguments);
            self.builder.inst_results(call).first().copied()
        }

        fn emit_spawn(&mut self, key: AstNodeKey) -> Option<Value> {
            let entry = self.facts.spawn_entry(key)?;
            let pointer = dispatch::pointer_type();
            let mut signature = Signature::new(self.builder.func.signature.call_conv);
            signature.params.push(AbiParam::new(pointer));
            signature.returns.push(AbiParam::new(types::I64));
            let trampoline =
                match self.call_importer.as_deref_mut()?.import(self.builder, entry.trampoline.clone(), &signature) {
                    Ok(function) => function,
                    Err(CallImportError::UnknownCallee) => {
                        self.pending_error =
                            Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(entry.trampoline) });
                        return None;
                    }
                };
            let entry_ptr = self.builder.ins().func_addr(pointer, trampoline);
            let environment = if let Some(closure) = &entry.closure_environment {
                self.emit_inline_closure_environment(closure)?
            } else {
                self.builder.ins().iconst(pointer, 0)
            };
            let cancel_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                pointer.bytes(),
                pointer.bytes().ilog2() as u8,
            ));
            self.builder.ins().stack_store(environment, cancel_slot, 0);
            let cancel_slot_address = self.builder.ins().stack_addr(pointer, cancel_slot, 0);
            let mut signature = Signature::new(self.builder.func.signature.call_conv);
            signature.params.push(AbiParam::new(pointer));
            signature.params.push(AbiParam::new(pointer));
            signature.params.push(AbiParam::new(pointer));
            signature.returns.push(AbiParam::new(types::I64));
            let signature = self.builder.func.import_signature(signature);
            let runtime_entry = self.builder.func.import_function(cranelift_codegen::ir::ExtFuncData {
                name: ExternalName::testcase("beskid_rt_v5_fiber_spawn_with_cancel_slot"),
                signature,
                colocated: false,
                patchable: false,
            });
            self.builder.ins().call(runtime_entry, &[entry_ptr, environment, cancel_slot_address]);
            let entry_call = self.builder.ins().call(trampoline, &[environment]);
            self.builder.inst_results(entry_call).first().copied()
        }

        /// Lower a freestanding [`LambdaExpression`] to a closure value.
        ///
        /// Capture-free lambdas return the trampoline function pointer directly. Capturing
        /// lambdas allocate and populate an ABI-v5 closure environment at the expression site
        /// before returning the trampoline function pointer; the trampoline loads captures
        /// from the environment at its first-parameter pointer.
        fn emit_lambda(&mut self, key: AstNodeKey) -> Option<Value> {
            let entry = self.facts.lambda_entry(key)?;
            let pointer = dispatch::pointer_type();
            let mut signature = Signature::new(self.builder.func.signature.call_conv);
            // The trampoline always receives the environment pointer as its first argument.
            signature.params.push(AbiParam::new(pointer));
            // Return type is a pointer (the function pointer itself for the closure struct).
            signature.returns.push(AbiParam::new(pointer));
            let trampoline =
                match self.call_importer.as_deref_mut()?.import(self.builder, entry.trampoline.clone(), &signature) {
                    Ok(function) => function,
                    Err(CallImportError::UnknownCallee) => {
                        self.pending_error =
                            Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(entry.trampoline) });
                        return None;
                    }
                };
            let entry_ptr = self.builder.ins().func_addr(pointer, trampoline);
            if let Some(closure) = &entry.closure_environment {
                self.emit_inline_closure_environment(closure)?;
            }
            Some(entry_ptr)
        }
        fn emit_inline_lambda_call(&mut self, key: AstNodeKey) -> Option<Value> {
            self.inline_lambda_call(key)
        }
    };
}

pub(super) use generated_call_methods;
