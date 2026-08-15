use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn runtime_intrinsic_arguments(&mut self, key: AstNodeKey) -> Option<Vec<Value>> {
        let signature = self.facts.call_signature(key)?;
        let argument_keys = self.facts.call_arguments(key)?;
        if argument_keys.len() != signature.params.len() {
            return None;
        }
        let mut arguments = Vec::with_capacity(argument_keys.len());
        for (argument, parameter) in argument_keys.into_iter().zip(&signature.params) {
            let value = generated::constructor_lower_expression(self, argument)?;
            let value = if self.builder.func.dfg.value_type(value) != parameter.value_type
                && self.facts.node_kind(argument) == Some(NodeKind::PathExpression)
                && self.facts.canonical_runtime_constant_integer(argument).is_some()
            {
                self.materialize_canonical_runtime_intrinsic_constant(argument, parameter.value_type)?
            } else {
                value
            };
            arguments.push(value);
        }
        Some(arguments)
    }

    /// Re-materialize a compiler-owned canonical runtime module constant at
    /// the exact ABI type required by a manifest-authorized intrinsic
    /// argument. This deliberately mirrors the direct-call rule without
    /// sharing its control path: the authority is restricted to an already
    /// selected runtime intrinsic, an exact canonical PathExpression, and a
    /// compiler-minted capability.
    pub(super) fn materialize_canonical_runtime_intrinsic_constant(
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

    pub(super) fn emit_runtime_intrinsic_statement(&mut self, key: AstNodeKey) -> Option<()> {
        let Some(kind) = self.facts.runtime_intrinsic_kind(key) else {
            return self.direct_call_statement(key);
        };
        let arguments = self.runtime_intrinsic_arguments(key)?;
        match kind {
            RuntimeIntrinsicKind::RawWordStore | RuntimeIntrinsicKind::RawByteStore => {
                let [address, value] = arguments.as_slice() else {
                    return None;
                };
                let pointer = self.builder.func.dfg.value_type(*address);
                if !pointer.is_int() {
                    return None;
                }
                self.builder.ins().store(MemFlags::new(), *value, *address, 0);
                Some(())
            }
            RuntimeIntrinsicKind::MemorySet => {
                let [destination, byte, length] = arguments.as_slice() else {
                    return None;
                };
                let pointer = self.builder.func.dfg.value_type(*destination);
                let length_key = self.facts.call_arguments(key)?.get(2).copied()?;
                let length = if self.builder.func.dfg.value_type(*length) == pointer {
                    *length
                } else {
                    self.materialize_canonical_runtime_intrinsic_constant(length_key, pointer)?
                };
                self.emit_memory_set(*destination, *byte, length)
            }
            RuntimeIntrinsicKind::MemoryCopy => {
                let [destination, source, length] = arguments.as_slice() else {
                    return None;
                };
                self.emit_memory_copy(*destination, *source, *length)
            }
            _ => self.direct_call_statement(key),
        }
    }

    pub(super) fn emit_memory_set(&mut self, destination: Value, byte: Value, length: Value) -> Option<()> {
        let pointer = self.builder.func.dfg.value_type(destination);
        if !pointer.is_int() || self.builder.func.dfg.value_type(length) != pointer {
            return None;
        }
        let byte_type = self.builder.func.dfg.value_type(byte);
        if !byte_type.is_int() {
            return None;
        }
        let byte = if byte_type == types::I8 { byte } else { self.builder.ins().ireduce(types::I8, byte) };
        let address = self.builder.declare_var(pointer);
        let remaining = self.builder.declare_var(pointer);
        self.builder.def_var(address, destination);
        self.builder.def_var(remaining, length);
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);
        self.builder.switch_to_block(header);
        let count = self.builder.use_var(remaining);
        let done = self.builder.ins().icmp_imm(IntCC::Equal, count, 0);
        self.builder.ins().brif(done, exit, &[], body, &[]);
        self.builder.switch_to_block(body);
        let current = self.builder.use_var(address);
        self.builder.ins().store(MemFlags::new(), byte, current, 0);
        let next_address = self.builder.ins().iadd_imm(current, 1);
        self.builder.def_var(address, next_address);
        let count = self.builder.use_var(remaining);
        let next_count = self.builder.ins().iadd_imm(count, -1);
        self.builder.def_var(remaining, next_count);
        self.builder.ins().jump(header, &[]);
        self.builder.seal_block(header);
        self.builder.seal_block(body);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);
        Some(())
    }

    pub(super) fn emit_memory_copy(&mut self, destination: Value, source: Value, length: Value) -> Option<()> {
        let pointer = self.builder.func.dfg.value_type(destination);
        if !pointer.is_int()
            || self.builder.func.dfg.value_type(source) != pointer
            || self.builder.func.dfg.value_type(length) != pointer
        {
            return None;
        }
        let destination_var = self.builder.declare_var(pointer);
        let source_var = self.builder.declare_var(pointer);
        let remaining = self.builder.declare_var(pointer);
        self.builder.def_var(destination_var, destination);
        self.builder.def_var(source_var, source);
        self.builder.def_var(remaining, length);
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);
        self.builder.switch_to_block(header);
        let count = self.builder.use_var(remaining);
        let done = self.builder.ins().icmp_imm(IntCC::Equal, count, 0);
        self.builder.ins().brif(done, exit, &[], body, &[]);
        self.builder.switch_to_block(body);
        let source_address = self.builder.use_var(source_var);
        let byte = self.builder.ins().load(types::I8, MemFlags::new(), source_address, 0);
        let destination_address = self.builder.use_var(destination_var);
        self.builder.ins().store(MemFlags::new(), byte, destination_address, 0);
        let next_source = self.builder.ins().iadd_imm(source_address, 1);
        self.builder.def_var(source_var, next_source);
        let next_destination = self.builder.ins().iadd_imm(destination_address, 1);
        self.builder.def_var(destination_var, next_destination);
        let count = self.builder.use_var(remaining);
        let next_count = self.builder.ins().iadd_imm(count, -1);
        self.builder.def_var(remaining, next_count);
        self.builder.ins().jump(header, &[]);
        self.builder.seal_block(header);
        self.builder.seal_block(body);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);
        Some(())
    }
}

macro_rules! generated_intrinsic_methods {
    () => {
        fn emit_runtime_intrinsic_statement(&mut self, key: AstNodeKey) -> Option<()> {
            IsleContext::emit_runtime_intrinsic_statement(self, key)
        }
        fn emit_runtime_intrinsic(&mut self, key: AstNodeKey) -> Option<Value> {
            let Some(kind) = self.facts.runtime_intrinsic_kind(key) else {
                return self.direct_call(key);
            };
            let arguments = self.runtime_intrinsic_arguments(key)?;
            let result = self.facts.scalar_type(key)?;
            match kind {
                RuntimeIntrinsicKind::NativeWordFromPointer | RuntimeIntrinsicKind::PointerFromNativeWord => {
                    let [value] = arguments.as_slice() else {
                        return None;
                    };
                    (self.builder.func.dfg.value_type(*value) == result).then_some(*value)
                }
                RuntimeIntrinsicKind::PointerAdd => {
                    let [base, offset] = arguments.as_slice() else {
                        return None;
                    };
                    (self.builder.func.dfg.value_type(*base) == result
                        && self.builder.func.dfg.value_type(*offset) == result)
                        .then(|| self.builder.ins().iadd(*base, *offset))
                }
                RuntimeIntrinsicKind::RawWordLoad => {
                    let [address] = arguments.as_slice() else {
                        return None;
                    };
                    (self.builder.func.dfg.value_type(*address) == result)
                        .then(|| self.builder.ins().load(result, MemFlags::new(), *address, 0))
                }
                RuntimeIntrinsicKind::RawByteLoad => {
                    let [address] = arguments.as_slice() else {
                        return None;
                    };
                    let address_ty = self.builder.func.dfg.value_type(*address);
                    if !address_ty.is_int() {
                        return None;
                    }
                    let loaded = self.builder.ins().load(types::I8, MemFlags::trusted(), *address, 0);
                    if result == types::I8 {
                        Some(loaded)
                    } else if result.is_int() && result.bits() > 8 {
                        Some(self.builder.ins().uextend(result, loaded))
                    } else {
                        None
                    }
                }
                RuntimeIntrinsicKind::ArchContextSize(value) | RuntimeIntrinsicKind::ArchContextAlignment(value) => {
                    arguments.is_empty().then(|| self.builder.ins().iconst(result, value as i64))
                }
                RuntimeIntrinsicKind::SchedulerFiberEntryAddress => arguments.is_empty().then(|| {
                    let signature =
                        let mut signature = Signature::new(self.builder.func.signature.call_conv);
                    signature.params.push(AbiParam::new(result));
                    let signature = self.builder.func.import_signature(signature);
                    let function = self.builder.func.import_function(ExtFuncData {
                        name: ExternalName::testcase("__beskid_scheduler_fiber_entry"),
                        signature,
                        colocated: true,
                        patchable: false,
                    });
                    self.builder.ins().func_addr(result, function)
                }),
                RuntimeIntrinsicKind::SchedulerReturnTrampolineAddress => arguments.is_empty().then(|| {
                    let signature =
                        self.builder.func.import_signature(Signature::new(self.builder.func.signature.call_conv));
                    let function = self.builder.func.import_function(ExtFuncData {
                        name: ExternalName::testcase("__beskid_scheduler_return_trampoline"),
                        signature,
                        colocated: true,
                        patchable: false,
                    });
                    self.builder.ins().func_addr(result, function)
                }),
                RuntimeIntrinsicKind::SchedulerPollEntryInvoke => {
                    let [entry, task, context, result_slot] = arguments.as_slice() else { return None };
                    let pointer = self.builder.func.dfg.value_type(*entry);
                    (pointer == self.builder.func.dfg.value_type(*task)
                        && pointer == self.builder.func.dfg.value_type(*context)
                        && pointer == self.builder.func.dfg.value_type(*result_slot))
                    .then_some(())?;
                    let mut signature = Signature::new(self.builder.func.signature.call_conv);
                    signature.params.extend(
                        [*task, *context, *result_slot]
                            .map(|value| AbiParam::new(self.builder.func.dfg.value_type(value))),
                    );
                    signature.returns.push(AbiParam::new(types::I32));
                    let signature = self.builder.func.import_signature(signature);
                    let call = self.builder.ins().call_indirect(signature, *entry, &[*task, *context, *result_slot]);
                    self.builder.inst_results(call).first().copied()
                }
                RuntimeIntrinsicKind::MemoryCopy
                | RuntimeIntrinsicKind::MemorySet
                | RuntimeIntrinsicKind::RawWordStore
                | RuntimeIntrinsicKind::RawByteStore => None,
            }
        }
    };
}

pub(super) use generated_intrinsic_methods;
