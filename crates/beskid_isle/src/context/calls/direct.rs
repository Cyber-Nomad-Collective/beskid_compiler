//! Direct, Corelib-service, and runtime-constant call lowering.

use super::super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(in crate::context) fn emit_corelib_service_call(
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

    pub(in crate::context) fn import_direct_call(
        &mut self,
        key: AstNodeKey,
    ) -> Option<(cranelift_codegen::ir::Inst, Signature)> {
        let callee = self.facts.direct_callee(key)?;
        let source_signature = self.facts.call_signature(key)?;
        let argument_keys = self.facts.call_arguments(key)?;
        let mut arguments = Vec::with_capacity(argument_keys.len());
        let mut roots = Vec::with_capacity(argument_keys.len());
        let mut parameters = source_signature.params.iter();
        for argument in argument_keys {
            if self.facts.semantic_type(argument) == Some(beskid_queries::SemanticTypeId::UNIT) {
                self.lower_expression_for_effect(argument)?;
                continue;
            }
            let parameter = parameters.next()?;
            let value = generated::constructor_lower_expression(self, argument)?;
            let value = if self.builder.func.dfg.value_type(value) == parameter.value_type {
                value
            } else {
                self.adapt_scalar_boundary(argument, value, parameter.value_type)
                    .or_else(|| self.materialize_canonical_runtime_direct_constant(argument, parameter.value_type))?
            };
            roots.push(self.root_expression_value_if_needed(argument, value)?);
            arguments.push(value);
        }
        if parameters.next().is_some() {
            return None;
        }
        let (native_signature, arguments) = self.adapt_corelib_service_call(&callee, &source_signature, arguments)?;
        let function = match self.call_importer.as_deref_mut()?.import(self.builder, callee.clone(), &native_signature)
        {
            Ok(function) => function,
            Err(CallImportError::UnknownCallee) => {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(callee) });
                return None;
            }
        };
        let call = self.builder.ins().call(function, &arguments);
        for root in roots.into_iter().rev() {
            self.release_expression_root(root)?;
        }
        Some((call, source_signature))
    }

    fn adapt_corelib_service_call(
        &mut self,
        callee: &DirectCallee,
        source_signature: &Signature,
        arguments: Vec<Value>,
    ) -> Option<(Signature, Vec<Value>)> {
        let DirectCallee::CorelibService(symbol) = callee else {
            return Some((source_signature.clone(), arguments));
        };
        let native_signature =
            corelib_service_native_signature(self.frontend_config, self.builder.func.signature.call_conv, symbol)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let word = pointer;
        let header_parts = |builder: &mut FunctionBuilder<'_>, value: Value| {
            let data = builder.ins().load(pointer, MemFlagsData::new(), value, 0);
            let len = builder.ins().load(word, MemFlagsData::new(), value, i32::try_from(pointer.bytes()).ok()?);
            Some((data, len))
        };
        let adapted = match (*symbol, arguments.as_slice()) {
            ("str_from_bytes_utf8", [header]) => {
                let (data, len) = header_parts(self.builder, *header)?;
                vec![data, len]
            }
            ("syscall_write_bytes", [fd, header]) => {
                let fd = self.builder.ins().ireduce(types::I32, *fd);
                let (data, len) = header_parts(self.builder, *header)?;
                vec![fd, data, len]
            }
            ("syscall_read" | "syscall_read_bytes", [fd, header, len]) => {
                let (data, _) = header_parts(self.builder, *header)?;
                vec![*fd, data, *len]
            }
            _ => arguments,
        };
        (adapted.len() == native_signature.params.len()).then_some((native_signature, adapted))
    }

    /// Re-materialize a compiler-owned runtime layout constant at the exact
    /// ABI type of its direct-call parameter.  The source grammar intentionally
    /// keeps module constants untyped; this is therefore narrowly contextual,
    /// requires compiler-minted authority, and never coerces arbitrary values.
    pub(in crate::context) fn materialize_canonical_runtime_direct_constant(
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

    pub(in crate::context) fn direct_call(&mut self, key: AstNodeKey) -> Option<Value> {
        let signature = self.facts.call_signature(key)?;
        let result_type = self.facts.scalar_type(key)?;
        if signature.returns.len() != 1 || signature.returns[0].value_type != result_type {
            return None;
        }
        if self.facts.traced_fiber_join_layout(key).is_some() {
            return self.traced_value_move_call(key, Some(result_type))?;
        }
        if self.facts.traced_channel_send_layout(key).is_some() {
            return self.traced_channel_send_call(key);
        }
        let (call, _) = self.import_direct_call(key)?;
        self.builder.inst_results(call).first().copied()
    }

    pub(in crate::context) fn direct_call_statement(&mut self, key: AstNodeKey) -> Option<()> {
        self.facts.call_signature(key)?.returns.is_empty().then_some(())?;
        if self.facts.traced_fiber_join_layout(key).is_some() {
            self.traced_value_move_call(key, None)?;
            return Some(());
        }
        let (call, _) = self.import_direct_call(key)?;
        self.builder.inst_results(call).is_empty().then_some(())?;
        if self.facts.semantic_type(key) == Some(beskid_queries::SemanticTypeId::NEVER) {
            self.builder.ins().trap(TrapCode::unwrap_user(9));
        }
        Some(())
    }
}

fn corelib_service_native_signature(
    frontend_config: TargetFrontendConfig,
    call_conv: CallConv,
    symbol: &str,
) -> Option<Signature> {
    use beskid_abi::runtime_source::CorelibServiceAbiType;

    let abi = beskid_abi::runtime_source::canonical_corelib_service_abi_for_adapter(symbol)?;
    let pointer = dispatch::pointer_type(frontend_config);
    let abi_type = |ty| match ty {
        CorelibServiceAbiType::Pointer | CorelibServiceAbiType::String | CorelibServiceAbiType::Usize => Some(pointer),
        CorelibServiceAbiType::I64 => Some(types::I64),
        CorelibServiceAbiType::I32 | CorelibServiceAbiType::U32 => Some(types::I32),
        CorelibServiceAbiType::U8 => Some(types::I8),
        CorelibServiceAbiType::F64 => Some(types::F64),
        CorelibServiceAbiType::Void | CorelibServiceAbiType::Never => None,
    };
    let mut signature = Signature::new(call_conv);
    signature
        .params
        .extend(abi.parameters.into_iter().map(|ty| abi_type(ty).map(AbiParam::new)).collect::<Option<Vec<_>>>()?);
    if !matches!(abi.result, CorelibServiceAbiType::Void | CorelibServiceAbiType::Never) {
        signature.returns.push(AbiParam::new(abi_type(abi.result)?));
    }
    Some(signature)
}
