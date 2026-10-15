//! Call kinds, callees, signatures, arguments, intrinsics, spawn, fiber, and lambda facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn call_kind_impl(&self, key: AstNodeKey) -> Option<CallKind> {
        // The `exchange` builtin is a place read and store, never a call: it outranks every call
        // probe, which would otherwise look for a callee named `exchange`.
        if self.query(beskid_queries::exchange_call(self.db, key)).is_some() {
            return Some(CallKind::Exchange);
        }
        // `__transfer_class<T>()` is a compile-time constant of the instance, never a runtime call.
        if self.transfer_class_constant_impl(key).is_some() {
            return Some(CallKind::Direct);
        }
        if self.event_operation_impl(key).is_some_and(|plan| plan.operation == beskid_isle::EventOperation::Raise) {
            return Some(CallKind::EventRaise);
        }
        if self.query(beskid_queries::primitive_numeric_conversion(self.db, key)).is_some() {
            return Some(CallKind::PrimitiveNumericConversion);
        }
        if self.runtime_intrinsic(key).is_some() || self.scheduler_compiler_operation(key).is_some() {
            return Some(CallKind::RuntimeIntrinsic);
        }
        // Canonical `Array.Empty<T>` is a compiler-owned typed allocation form. Recognize its
        // source- and specialization-backed descriptor plan before asking collection dispatch:
        // the legacy `__array_new(size, length)` signature intentionally rejects this one-argument
        // form, and that diagnostic must not misclassify the authorized typed constructor.
        if self.typed_array_plan(key).is_some() {
            return Some(CallKind::TypedArrayAllocation);
        }
        // An exact generation-bound closure target outranks domain-specific call probes. Stored
        // lambdas use a local path as their callee; asking collection dispatch about that path can
        // fail unavailable even though the closure call is fully proven.
        if self.inline_lambda_call(key).is_some() {
            return Some(CallKind::InlineLambda);
        }
        // A function value callee (a function-typed local, field or call result) never reaches
        // collection dispatch, which would misread a local path as a receiver.
        if matches!(self.query(call_lowering(self.db, key)), Some(CallLowering::FunctionValue(_))) {
            return self.function_value_call_impl(key).map(|_| CallKind::FunctionValue);
        }
        match beskid_queries::collection_operation(self.db, key) {
            Ok(Some(_)) | Err(_) => return Some(CallKind::CollectionOperation),
            Ok(None) => {}
        }
        // `Of` is a constructor, not a collection operation, so it never matches above. A bulk
        // callee declares a `bulk T[]` parameter; its call site packs N scalars into a fresh
        // rooted array before the direct call. This must precede the `Direct` fallback, which
        // would otherwise reject the N-scalar-vs-one-array arity mismatch.
        if self.is_bulk_call(key) {
            return Some(CallKind::Bulk);
        }
        matches!(
            self.query(call_lowering(self.db, key)),
            Some(
                CallLowering::Direct(_)
                    | CallLowering::ManifestBuiltin(_)
                    | CallLowering::CorelibService(_)
                    | CallLowering::NativeModCallback(_)
            )
        )
        .then_some(CallKind::Direct)
    }

    pub(super) fn runtime_intrinsic_kind_impl(&self, key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        if let Some(operation) = self.scheduler_compiler_operation(key) {
            return Some(match operation {
                crate::SchedulerCompilerOperation::FiberEntryAddress => {
                    RuntimeIntrinsicKind::SchedulerFiberEntryAddress
                }
                crate::SchedulerCompilerOperation::ReturnTrampolineAddress => {
                    RuntimeIntrinsicKind::SchedulerReturnTrampolineAddress
                }
                crate::SchedulerCompilerOperation::PollEntryInvoke => RuntimeIntrinsicKind::SchedulerPollEntryInvoke,
            });
        }
        let (_, intrinsic) = self.runtime_intrinsic(key)?;
        match intrinsic.name.as_str() {
            "arch_context_size" => {
                return Some(RuntimeIntrinsicKind::ArchContextSize(self.input.target_context_layout()?.size));
            }
            "arch_context_alignment" => {
                return Some(RuntimeIntrinsicKind::ArchContextAlignment(self.input.target_context_layout()?.alignment));
            }
            _ => {}
        }
        runtime_intrinsic_kind_for_name(intrinsic.name.as_str())
    }

    pub(super) fn direct_callee_impl(&self, key: AstNodeKey) -> Option<DirectCallee> {
        if let Some((index, _)) = self.runtime_intrinsic(key) {
            return Some(DirectCallee::runtime_intrinsic(index));
        }
        let lowering = self.query(call_lowering(self.db, key))?;
        if let CallLowering::NativeModCallback(callback) = lowering {
            return Some(DirectCallee::NativeModCallback(callback.wrapper()));
        }
        if let CallLowering::ManifestBuiltin(builtin) = lowering {
            return Some(DirectCallee::corelib_service(builtin.symbol));
        }
        if let CallLowering::CorelibService(service) = lowering {
            self.input.corelib_service_capability()?;
            let symbol = if beskid_abi::runtime_source::canonical_corelib_service_value_dispatch(service).is_some() {
                self.typed_corelib_value_service(key, service)?.0
            } else {
                service.symbol
            };
            return Some(DirectCallee::corelib_service(symbol));
        }
        let CallLowering::Direct(declaration) = lowering else {
            return None;
        };
        if let Some(specialization) = self.generic_call_specialization_in_context(key) {
            if specialization.substitutions.is_empty() && specialization.contract_witnesses.is_empty() {
                return Some(DirectCallee::item(specialization.declaration));
            }
            return Some(DirectCallee::specialized_item(
                specialization.declaration,
                specialization_identity(&specialization),
            ));
        }
        Some(DirectCallee::item(declaration))
    }

    pub(super) fn call_signature_impl(&self, key: AstNodeKey) -> Option<Signature> {
        if let Some((_, intrinsic)) = self.runtime_intrinsic(key) {
            return signature_for_runtime_intrinsic(self.isa?, intrinsic);
        }
        if let Some(operation) = self.scheduler_compiler_operation(key) {
            let emitter = FunctionEmitter::new(self.isa?);
            let pointer = self.isa?.pointer_type();
            return Some(match operation {
                crate::SchedulerCompilerOperation::FiberEntryAddress
                | crate::SchedulerCompilerOperation::ReturnTrampolineAddress => emitter.signature([], [pointer]),
                crate::SchedulerCompilerOperation::PollEntryInvoke => {
                    emitter.signature([pointer, pointer, pointer, pointer], [types::I32])
                }
            });
        }
        if let Some(CallLowering::CorelibService(service)) = self.query(call_lowering(self.db, key))
            && beskid_abi::runtime_source::canonical_corelib_service_value_dispatch(service).is_some()
        {
            let (_, result) = self.typed_corelib_value_service(key, service)?;
            return signature_for_item(
                self.isa?,
                ItemSignature { parameters: std::sync::Arc::from([SemanticTypeId::I64]), result },
            );
        }
        if let Some(specialization) = self.generic_call_specialization_in_context(key) {
            return signature_for_item(self.isa?, specialization.signature);
        }
        // A call through a function value inside a specialized body: its result type is the
        // callee's function type in the instance (`T` of `(T) => T f` bound), which the
        // target-neutral `call_abi_signature` cannot prove.
        if self.current_item_specialization().is_some()
            && let Some(call) = self.function_value_call_in_context(key)
        {
            return signature_for_item(self.isa?, call.signature);
        }
        signature_for_item(self.isa?, self.query(call_abi_signature(self.db, key))?)
    }

    pub(super) fn call_arguments_impl(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.query(call_arguments(self.db, key))
            .and_then(|arguments| arguments.iter().copied().map(|argument| self.unwrap_transparent(argument)).collect())
    }

    pub(super) fn inline_lambda_call_impl(&self, key: AstNodeKey) -> Option<InlineLambdaCall> {
        let target = self.query(closure_call_target(self.db, key))?;
        let environment = self.query(closure_environment(self.db, target.lambda))?;
        if environment.parameters.len() != target.callable.parameters.len() {
            return None;
        }
        let closure_environment = if environment.captures.is_empty() {
            None
        } else {
            Some(self.inline_closure_environment(key, target.lambda)?)
        };
        let parameters = environment
            .parameters
            .iter()
            .copied()
            .zip(target.callable.parameters.iter().copied())
            .map(|(parameter, semantic)| {
                let slot = self.query(local_slot(self.db, parameter))?;
                Some(ParameterSlot {
                    slot: LocalSlotId { owner_node: slot.owner.node.0, index: slot.index },
                    value_type: map_signature_type(self.isa?, semantic)?,
                    managed_reference: self.managed_reference(parameter)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(InlineLambdaCall {
            body: target.body,
            parameters,
            result_type: map_signature_type(self.isa?, target.callable.result)?,
            closure_environment,
        })
    }

    pub(super) fn spawn_entry_impl(&self, key: AstNodeKey) -> Option<beskid_isle::SpawnEntry> {
        let validation = self.query(spawn_entry_validation(self.db, key))?;
        if !validation.is_legal_entry {
            return None;
        }
        let (closure_environment, argument_environment) = match self.query(node_kind(self.db, validation.target))? {
            beskid_queries::IndexedNodeKind::PathExpression => {
                let _target = self.query(resolved_item(self.db, validation.target))?;
                let arguments = if validation.arguments.is_empty() {
                    None
                } else {
                    Some(self.spawn_argument_environment(key, &validation.arguments)?)
                };
                (None, arguments)
            }
            beskid_queries::IndexedNodeKind::LambdaExpression => {
                if !validation.arguments.is_empty() {
                    return None;
                }
                let environment = self.query(closure_environment(self.db, validation.target))?;
                if environment.captures.is_empty() {
                    (None, None)
                } else {
                    (Some(self.inline_closure_environment(key, validation.target)?), None)
                }
            }
            _ => return None,
        };
        let handle = self.input.spawn_handle_static_plan(key)?;
        Some(beskid_isle::SpawnEntry {
            trampoline: DirectCallee::spawn_trampoline(key),
            closure_environment,
            argument_environment,
            handle_request_symbol: handle.allocation_request_symbol.into(),
            handle_field_offset: i32::try_from(handle.fields[0].field_offset).ok()?,
        })
    }

    pub(super) fn traced_fiber_join_layout_impl(&self, key: AstNodeKey) -> Option<beskid_isle::TracedFiberJoinLayout> {
        let DirectCallee::CorelibService(
            symbol @ ("fiber_join_value"
            | "channel_receive_value"
            | "hub_wait_receive_value"
            | "isolate_join_value"
            | "port_receive_value"),
        ) = self.direct_callee(key)?
        else {
            return None;
        };
        let slot = self.input.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidAbiValue")?;
        let header = self.input.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidObjectHeader")?;
        Some(beskid_isle::TracedFiberJoinLayout {
            symbol,
            slot_size: u32::try_from(slot.size).ok()?,
            alignment_shift: u8::try_from(slot.alignment.ilog2()).ok()?,
            payload_offset: i32::try_from(slot.fields.iter().find(|field| field.name == "payload")?.offset).ok()?,
            value_offset: i32::try_from(header.size).ok()?,
        })
    }

    /// `__isolate_spawn<T>` / `__isolate_spawn_local<T>`: the entry trampoline is chosen by the ABI
    /// shape of `T` in the item being lowered (see `module_emission::trampolines::isolate`).
    pub(super) fn isolate_spawn_call_impl(&self, key: AstNodeKey) -> Option<beskid_isle::IsolateSpawnCall> {
        let DirectCallee::CorelibService(symbol @ ("isolate_spawn" | "isolate_spawn_local")) =
            self.direct_callee(key)?
        else {
            return None;
        };
        let (result, managed) = crate::module_emission::isolate_entry_shape(self.db, key, self.current_item_specialization())?;
        Some(beskid_isle::IsolateSpawnCall {
            symbol,
            trampoline: DirectCallee::isolate_entry_trampoline(result, managed),
        })
    }

    /// `__transfer_class<T>()` in the item being lowered: 0 Shared, 1 Moved, 2 Copied.
    pub(super) fn transfer_class_constant_impl(&self, key: AstNodeKey) -> Option<i64> {
        match self.query(beskid_queries::transfer_class_intrinsic(self.db, key))? {
            beskid_queries::TransferClassIntrinsic::Constant(code) => Some(code),
            beskid_queries::TransferClassIntrinsic::PerInstantiation { name } => {
                let substitution = self
                    .current_item_specialization()?
                    .substitutions
                    .iter()
                    .find(|binding| binding.parameter.as_ref() == name.as_str())?;
                self.query(
                    beskid_queries::transfer_class_code_for_substitution(self.db, key, substitution).map(Some),
                )
            }
        }
    }

    pub(super) fn traced_channel_send_layout_impl(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_isle::TracedFiberJoinLayout> {
        let DirectCallee::CorelibService(symbol @ ("channel_send" | "channel_try_send" | "port_send")) =
            self.direct_callee(key)?
        else {
            return None;
        };
        let arguments = self.call_arguments(key)?;
        // `port_send` carries the value's transfer class as a third, scalar argument.
        let value = match (symbol, arguments.as_slice()) {
            ("port_send", [_, value, _]) => *value,
            ("channel_send" | "channel_try_send", [_, value]) => *value,
            _ => return None,
        };
        (self.managed_reference_in_context(value)? == beskid_isle::ManagedReferenceFact::GcManaged).then_some(())?;
        let slot = self.input.abi_manifest().layouts.iter().find(|layout| layout.name == "BeskidAbiValue")?;
        Some(beskid_isle::TracedFiberJoinLayout {
            symbol,
            slot_size: u32::try_from(slot.size).ok()?,
            alignment_shift: u8::try_from(slot.alignment.ilog2()).ok()?,
            payload_offset: i32::try_from(slot.fields.iter().find(|field| field.name == "payload")?.offset).ok()?,
            value_offset: 0,
        })
    }

    pub(super) fn lambda_entry_impl(&self, key: AstNodeKey) -> Option<beskid_isle::LambdaEntry> {
        let environment = self.query(closure_environment(self.db, key))?;
        let lambda = self.query(closure_signature(self.db, key))?;
        self.lambda_entry_from_environment(key, environment, &lambda.callable, false)
    }

    pub(super) fn event_lambda_entry_impl(&self, key: AstNodeKey) -> Option<beskid_isle::LambdaEntry> {
        let environment = self.query(closure_environment(self.db, key))?;
        let handler = self.query(beskid_queries::event_handler_lambda_for_local(self.db, key))?;
        self.lambda_entry_from_environment(key, environment, &handler.signature, true)
    }

    pub(super) fn function_value_call_impl(&self, key: AstNodeKey) -> Option<beskid_isle::FunctionValueCall> {
        // Inside a generic specialization the callee's function type may name the enclosing
        // declaration's parameters; its exact instance binds them.
        let call = self.function_value_call_in_context(key)?;
        let (parameters, result) = self.callable_abi(&call.signature)?;
        Some(beskid_isle::FunctionValueCall {
            callee: call.callee,
            parameters,
            result,
            code_offset: crate::EVENT_HANDLER_CODE_OFFSET,
            environment_offset: crate::EVENT_HANDLER_ENVIRONMENT_OFFSET,
        })
    }

    /// A named function read as a value: its closure record carries the item's own entry, or the
    /// entry of the specialization a generic item is instantiated with where it is read.
    pub(super) fn function_item_value_impl(&self, key: AstNodeKey) -> Option<beskid_isle::FunctionItemValue> {
        let value = self.query(beskid_queries::function_item_value(self.db, key))?;
        let (parameters, result) = self.callable_abi(&value.signature)?;
        let callee = match value.specialization.as_ref() {
            Some(specialization)
                if !specialization.substitutions.is_empty() || !specialization.contract_witnesses.is_empty() =>
            {
                DirectCallee::specialized_item(value.declaration, specialization_identity(specialization))
            }
            _ => DirectCallee::item(value.declaration),
        };
        Some(beskid_isle::FunctionItemValue { callee, parameters, result })
    }

    /// Map a callable's semantic signature to entry parameter and result ABI types.
    fn callable_abi(&self, signature: &ItemSignature) -> Option<(Vec<Type>, Option<Type>)> {
        let parameters = signature
            .parameters
            .iter()
            .copied()
            .map(|ty| map_signature_type(self.isa?, ty))
            .collect::<Option<Vec<_>>>()?;
        let result = if matches!(signature.result, SemanticTypeId::UNIT | SemanticTypeId::NEVER) {
            None
        } else {
            Some(map_signature_type(self.isa?, signature.result)?)
        };
        Some((parameters, result))
    }

    fn lambda_entry_from_environment(
        &self,
        key: AstNodeKey,
        environment: beskid_queries::ClosureEnvironment,
        callable: &ItemSignature,
        event_handler: bool,
    ) -> Option<beskid_isle::LambdaEntry> {
        let (parameters, result) = self.callable_abi(callable)?;
        // Only support capture-free or fully-resolved capture environments.
        let closure_environment = if environment.captures.is_empty() {
            None
        } else {
            let authority = if event_handler {
                self.input.event_handler_closure_lowering_authority(key, key)
            } else {
                self.input.closure_lowering_authority(key, key)
            };
            let Some(authority) = authority else {
                return None;
            };
            let captures = authority
                .plan
                .captures
                .iter()
                .map(|field| {
                    Some(beskid_isle::InlineCaptureField {
                        local_slot: beskid_isle::LocalSlotId {
                            owner_node: field.capture.slot.owner.node.0,
                            index: field.capture.slot.index,
                        },
                        field_offset: u32::try_from(field.field_offset).ok()?,
                        pointer_map_index: field.pointer_map_index,
                        value_type: map_signature_type(self.isa?, field.abi_type)?,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(beskid_isle::InlineClosureEnvironment {
                allocation_request_symbol: authority.plan.allocation_request_symbol.clone().into(),
                descriptor_symbol: authority.plan.descriptor_symbol.clone().into(),
                captures,
            })
        };
        Some(beskid_isle::LambdaEntry {
            trampoline: DirectCallee::lambda_trampoline(key),
            closure_environment,
            parameters,
            result,
        })
    }
}
