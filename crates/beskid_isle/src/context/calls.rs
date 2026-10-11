//! Call lowering for `IsleContext`, grouped by call shape.

mod closures;
mod collections;
mod direct;
mod helpers;
mod traced;

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
            let pointer = dispatch::pointer_type(self.frontend_config);
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
            let (environment, environment_root) = match (&entry.closure_environment, &entry.argument_environment) {
                (Some(closure), None) => {
                    let (value, root) = self.emit_inline_closure_environment(closure)?;
                    (value, Some(root))
                }
                (None, Some(arguments)) => {
                    let (value, root) = self.emit_spawn_argument_environment(arguments)?;
                    (value, Some(root))
                }
                (None, None) => (self.builder.ins().iconst(pointer, 0), None),
                (Some(_), Some(_)) => return None,
            };
            let mut signature = Signature::new(self.builder.func.signature.call_conv);
            signature.params.push(AbiParam::new(pointer));
            signature.params.push(AbiParam::new(pointer));
            signature.returns.push(AbiParam::new(types::I64));
            let signature = self.builder.func.import_signature(signature);
            let runtime_entry = self.builder.func.import_function(cranelift_codegen::ir::ExtFuncData {
                name: ExternalName::testcase("fiber_spawn"),
                signature,
                colocated: false,
                patchable: false,
            });
            let spawn_call = self.builder.ins().call(runtime_entry, &[entry_ptr, environment]);
            let handle = self.builder.inst_results(spawn_call).first().copied()?;
            self.release_temporary_root(environment_root)?;
            let failed =
                self.builder.ins().icmp_imm_s(cranelift_codegen::ir::condcodes::IntCC::SignedLessThan, handle, 0);
            self.builder.ins().trapnz(failed, TrapCode::unwrap_user(5));
            let request = self.symbol_global(&entry.handle_request_symbol, pointer)?;
            let allocate =
                self.import_runtime_helper("beskid_rt_v5_managed_object_allocate", &[pointer], Some(pointer))?;
            let allocation = self.builder.ins().call(allocate, &[request]);
            let value = self.builder.inst_results(allocation).first().copied()?;
            self.builder.ins().trapz(value, TrapCode::unwrap_user(5));
            self.builder.ins().store(MemFlagsData::new(), handle, value, entry.handle_field_offset);
            Some(value)
        }

        /// Lower a freestanding [`LambdaExpression`] to its closure record.
        ///
        /// The record holds the lifted entry's code pointer and, for a capturing lambda, the
        /// environment allocated here with a by-value snapshot of every capture.
        fn emit_lambda(&mut self, key: AstNodeKey) -> Option<Value> {
            self.facts.lambda_value_required(key)?.then_some(())?;
            let entry = self.facts.lambda_entry(key)?;
            self.emit_closure_record(
                key,
                &entry.trampoline,
                entry.closure_environment.as_ref(),
                &entry.parameters,
                entry.result,
            )
        }

        fn emit_function_value_call(&mut self, key: AstNodeKey) -> Option<Value> {
            self.function_value_call(key, true)?
        }

        /// A call in statement position discards any result.
        fn emit_function_value_call_statement(&mut self, key: AstNodeKey) -> Option<()> {
            self.function_value_call(key, false).map(|_| ())
        }

        fn emit_inline_lambda_call(&mut self, key: AstNodeKey) -> Option<Value> {
            self.inline_lambda_call(key)
        }
    };
}

pub(super) use generated_call_methods;
