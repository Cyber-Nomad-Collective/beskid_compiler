use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn emit_corelib_service_call(
        &mut self,
        key: AstNodeKey,
        symbol: &'static str,
        arguments: &[Value],
        parameter_types: &[Type],
        return_type: Option<Type>,
    ) -> Option<Value> {
        let mut signature = Signature::new(self.builder.func.signature.call_conv);
        signature.params.extend(parameter_types.iter().copied().map(AbiParam::new));
        signature.returns.extend(return_type.map(AbiParam::new));
        let callee = DirectCallee::corelib_service(symbol);
        let function = match self.call_importer.as_deref_mut()?.import(self.builder, callee.clone(), &signature) {
            Ok(function) => function,
            Err(CallImportError::UnknownCallee) => {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(callee) });
                return None;
            }
        };
        let call = self.builder.ins().call(function, arguments);
        return_type.and_then(|_| self.builder.inst_results(call).first().copied())
    }

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

    /// Lower a `bulk`-parameter call.
    ///
    /// The callee declares one `bulk T[]` parameter, so its signature has a single array parameter
    /// while the call site passes N scalar arguments. This packs the N scalars into a fresh rooted
    /// array — reusing the exact `emit_array_literal` allocation/store/barrier/finish sequence —
    /// then direct-calls the callee with that array as its sole argument. It bypasses
    /// [`import_direct_call`], whose scalar-arity check cannot hold for a bulk call.
    pub(super) fn emit_bulk_call(&mut self, key: AstNodeKey) -> Option<Value> {
        let elements = self.facts.call_arguments(key)?;
        let layout = self.facts.array_layout(key)?;
        let allocation = self.facts.managed_array_allocation(key)?;
        if !layout.is_valid() || usize::try_from(layout.length).ok()? != elements.len() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
            return None;
        }
        let pointer = dispatch::pointer_type();
        let request = self.symbol_global(allocation.allocation_request_symbol.as_ref(), pointer)?;
        let root_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            pointer.bytes(),
            pointer.bytes().ilog2() as u8,
        ));
        let root_slot_address = self.builder.ins().stack_addr(pointer, root_slot, 0);
        let allocate =
            self.import_runtime_helper("beskid_rt_v5_array_allocate_rooted", &[pointer, pointer], Some(pointer))?;
        let allocation_call = self.builder.ins().call(allocate, &[request, root_slot_address]);
        let array = self.builder.inst_results(allocation_call).first().copied()?;
        self.builder.ins().trapz(array, TrapCode::unwrap_user(5));
        // `BeskidArray.ptr` remains at offset zero.  The backing bytes are owned by the same
        // descriptor-backed GC allocation; they are never a stack temporary.
        let data = self.builder.ins().load(pointer, MemFlags::new(), array, 0);
        for (index, element) in elements.into_iter().enumerate() {
            let value = generated::constructor_lower_expression(self, element)?;
            if self.builder.func.dfg.value_type(value) != layout.element_type {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayLayout });
                return None;
            }
            let offset =
                u32::try_from(index).ok()?.checked_mul(layout.stride).and_then(|offset| i32::try_from(offset).ok())?;
            let address = self.builder.ins().iadd_imm(data, i64::from(offset));
            self.builder.ins().store(MemFlags::new(), value, address, 0);
            if layout.element_type == pointer {
                let barrier = self.import_runtime_helper(
                    "beskid_rt_v5_array_write_barrier",
                    &[pointer, pointer],
                    Some(types::I8),
                )?;
                let barrier_call = self.builder.ins().call(barrier, &[array, value]);
                let published = self.builder.inst_results(barrier_call).first().copied()?;
                self.builder.ins().trapz(published, TrapCode::unwrap_user(8));
            }
        }
        // The allocation was rooted before the first nested element was lowered. Release only
        // after every store and pointer-publication barrier has completed.
        let root_handle = self.builder.ins().stack_load(pointer, root_slot, 0);
        let finish =
            self.import_runtime_helper("beskid_rt_v5_array_construction_finish", &[pointer], Some(types::I8))?;
        let finish_call = self.builder.ins().call(finish, &[root_handle]);
        let released = self.builder.inst_results(finish_call).first().copied()?;
        self.builder.ins().trapz(released, TrapCode::unwrap_user(10));

        // Direct-call the callee with the packed array as its sole argument. The callee signature
        // has one array parameter, so this import bypasses `import_direct_call`'s scalar-arity
        // check (N scalars vs. one array parameter would otherwise fail it).
        let callee = self.facts.direct_callee(key)?;
        let signature = self.facts.call_signature(key)?;
        let result_type = self.facts.scalar_type(key)?;
        if signature.params.len() != 1
            || signature.params[0].value_type != pointer
            || signature.returns.len() != 1
            || signature.returns[0].value_type != result_type
        {
            return None;
        }
        let function = match self.call_importer.as_deref_mut()?.import(self.builder, callee.clone(), &signature) {
            Ok(function) => function,
            Err(CallImportError::UnknownCallee) => {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(callee) });
                return None;
            }
        };
        let call = self.builder.ins().call(function, &[array]);
        self.builder.inst_results(call).first().copied()
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
    pub(super) fn emit_collection_operation_value(&mut self, key: AstNodeKey) -> Option<Value> {
        let operation = self.facts.collection_operation(key)?;
        let arguments = self.facts.call_arguments(key)?;
        let pointer = dispatch::pointer_type();
        let word = pointer;
        let element_type = self.facts.collection_element_type(key)?;
        let stride = element_type.bytes();
        match operation {
            CollectionOperation::UnprovenMutationOwner => {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                None
            }
            CollectionOperation::Capacity => {
                let [array] = arguments.as_slice() else { return None };
                let array = generated::constructor_lower_expression(self, *array)?;
                (self.builder.func.dfg.value_type(array) == pointer).then_some(())?;
                self.builder.ins().trapz(array, TrapCode::unwrap_user(1));
                Some(self.builder.ins().load(word, MemFlags::new(), array, i32::try_from(pointer.bytes() * 2).ok()?))
            }
            CollectionOperation::Append { owner: mutation_owner } => {
                let [array_key, value_key] = arguments.as_slice() else { return None };
                let owner = generated::constructor_lower_expression(self, *array_key)?;
                let value = generated::constructor_lower_expression(self, *value_key)?;
                (self.builder.func.dfg.value_type(owner) == pointer
                    && self.builder.func.dfg.value_type(value) == element_type)
                    .then_some(())?;
                self.builder.ins().trapz(owner, TrapCode::unwrap_user(1));
                let length_offset = i32::try_from(pointer.bytes()).ok()?;
                let length = self.builder.ins().load(word, MemFlags::new(), owner, length_offset);
                let next_length = self.builder.ins().iadd_imm(length, 1);
                let overflow = self.builder.ins().icmp(IntCC::UnsignedLessThanOrEqual, next_length, length);
                self.builder.ins().trapnz(overflow, TrapCode::unwrap_user(3));
                let root_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                    StackSlotKind::ExplicitSlot,
                    pointer.bytes(),
                    pointer.bytes().ilog2() as u8,
                ));
                let root_out = self.builder.ins().stack_addr(pointer, root_slot, 0);
                let grow = self.import_runtime_helper(
                    "beskid_rt_v5_array_grow_rooted",
                    &[pointer, word, pointer],
                    Some(pointer),
                )?;
                let grow_call = self.builder.ins().call(grow, &[owner, next_length, root_out]);
                let array = self.builder.inst_results(grow_call).first().copied()?;
                self.builder.ins().trapz(array, TrapCode::unwrap_user(5));
                let data = self.builder.ins().load(pointer, MemFlags::new(), array, 0);
                let offset = if stride == 1 { length } else { self.builder.ins().imul_imm(length, i64::from(stride)) };
                let address = self.builder.ins().iadd(data, offset);
                self.builder.ins().store(MemFlags::new(), value, address, 0);
                if element_type == pointer {
                    let barrier = self.import_runtime_helper(
                        "beskid_rt_v5_array_write_barrier",
                        &[pointer, pointer],
                        Some(types::I8),
                    )?;
                    let call = self.builder.ins().call(barrier, &[array, value]);
                    let published = self.builder.inst_results(call).first().copied()?;
                    self.builder.ins().trapz(published, TrapCode::unwrap_user(8));
                }
                self.builder.ins().store(MemFlags::new(), next_length, array, length_offset);
                let publication_owner = match mutation_owner {
                    CollectionMutationOwner::Local(slot) => {
                        let (variable, owner_type) = self.locals.get(&slot).copied()?;
                        if owner_type != pointer || self.facts.local_slot(*array_key) != Some(slot) {
                            self.pending_error =
                                Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                            return None;
                        }
                        self.builder.def_var(variable, array);
                        array
                    }
                    CollectionMutationOwner::AggregateField { receiver, field_index } => {
                        let layout = self.facts.struct_layout(*array_key)?;
                        let Some(field) =
                            usize::try_from(field_index).ok().and_then(|index| layout.fields.get(index)).copied()
                        else {
                            self.pending_error =
                                Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                            return None;
                        };
                        let (variable, receiver_type) = self.locals.get(&receiver).copied()?;
                        let base = self.builder.use_var(variable);
                        if receiver_type != pointer || field.value_type != pointer {
                            self.pending_error =
                                Some(LoweringError { key, kind: LoweringErrorKind::UnprovenCollectionOwner });
                            return None;
                        }
                        self.builder.ins().store(MemFlags::new(), array, base, i32::try_from(field.offset).ok()?);
                        base
                    }
                };
                let owner_barrier = self.import_runtime_helper(
                    "beskid_rt_v5_array_write_barrier",
                    &[pointer, pointer],
                    Some(types::I8),
                )?;
                let owner_call = self.builder.ins().call(owner_barrier, &[publication_owner, array]);
                let owner_published = self.builder.inst_results(owner_call).first().copied()?;
                self.builder.ins().trapz(owner_published, TrapCode::unwrap_user(8));
                let root_handle = self.builder.ins().stack_load(pointer, root_slot, 0);
                let finish =
                    self.import_runtime_helper("beskid_rt_v5_array_construction_finish", &[pointer], Some(types::I8))?;
                let finish_call = self.builder.ins().call(finish, &[root_handle]);
                let released = self.builder.inst_results(finish_call).first().copied()?;
                self.builder.ins().trapz(released, TrapCode::unwrap_user(10));
                Some(array)
            }
            CollectionOperation::Clear => {
                let [array_key, index_key] = arguments.as_slice() else { return None };
                let array = generated::constructor_lower_expression(self, *array_key)?;
                let index = generated::constructor_lower_expression(self, *index_key)?;
                (self.builder.func.dfg.value_type(array) == pointer
                    && self.builder.func.dfg.value_type(index).is_int())
                .then_some(())?;
                self.builder.ins().trapz(array, TrapCode::unwrap_user(1));
                let length =
                    self.builder.ins().load(word, MemFlags::new(), array, i32::try_from(pointer.bytes()).ok()?);
                let out_of_bounds = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, index, length);
                self.builder.ins().trapnz(out_of_bounds, TrapCode::HEAP_OUT_OF_BOUNDS);
                let data = self.builder.ins().load(pointer, MemFlags::new(), array, 0);
                let offset = if stride == 1 { index } else { self.builder.ins().imul_imm(index, i64::from(stride)) };
                let address = self.builder.ins().iadd(data, offset);
                let zero = if element_type == types::F64 {
                    self.builder.ins().f64const(Ieee64::with_float(0.0))
                } else {
                    self.builder.ins().iconst(element_type, 0)
                };
                self.builder.ins().store(MemFlags::new(), zero, address, 0);
                Some(array)
            }
            CollectionOperation::RemoveLast => {
                let [array_key] = arguments.as_slice() else { return None };
                let array = generated::constructor_lower_expression(self, *array_key)?;
                (self.builder.func.dfg.value_type(array) == pointer).then_some(())?;
                self.builder.ins().trapz(array, TrapCode::unwrap_user(1));
                let length_offset = i32::try_from(pointer.bytes()).ok()?;
                let length = self.builder.ins().load(word, MemFlags::new(), array, length_offset);
                let empty = self.builder.ins().icmp_imm(IntCC::Equal, length, 0);
                self.builder.ins().trapnz(empty, TrapCode::HEAP_OUT_OF_BOUNDS);
                let next_length = self.builder.ins().iadd_imm(length, -1);
                let data = self.builder.ins().load(pointer, MemFlags::new(), array, 0);
                let offset =
                    if stride == 1 { next_length } else { self.builder.ins().imul_imm(next_length, i64::from(stride)) };
                let address = self.builder.ins().iadd(data, offset);
                let zero = if element_type == types::F64 {
                    self.builder.ins().f64const(Ieee64::with_float(0.0))
                } else {
                    self.builder.ins().iconst(element_type, 0)
                };
                self.builder.ins().store(MemFlags::new(), zero, address, 0);
                self.builder.ins().store(MemFlags::new(), next_length, array, length_offset);
                Some(array)
            }
        }
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
            colocated: false,
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

        fn emit_bulk_call(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_bulk_call(key)
        }

        fn emit_collection_operation(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_collection_operation_value(key)
        }

        fn emit_direct_call_statement(&mut self, key: AstNodeKey) -> Option<()> {
            self.direct_call_statement(key)
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
