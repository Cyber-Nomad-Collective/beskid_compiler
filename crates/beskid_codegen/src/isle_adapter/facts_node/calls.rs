//! Call kinds, callees, signatures, arguments, intrinsics, spawn, fiber, and lambda facts.

use super::super::*;

impl SyntaxNodeFacts<'_> {
    pub(super) fn call_kind_impl(&self, key: AstNodeKey) -> Option<CallKind> {
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
        match beskid_queries::collection_operation(self.db, key) {
            Ok(Some(_)) | Err(_) => return Some(CallKind::CollectionOperation),
            Ok(None) => {}
        }
        // `Of` is a constructor, not a collection operation, so it never matches above. A bulk
        // callee declares a `bulk T[]` parameter; its call site packs N scalars into a fresh
        // rooted array before the direct call. This must precede the `Direct` fallback, which
        // would otherwise reject the N-scalar-vs-one-array arity mismatch.
        if self.callee_bulk_parameter(key).is_some() {
            return Some(CallKind::Bulk);
        }
        matches!(
            self.query(call_lowering(self.db, key)),
            Some(CallLowering::Direct(_) | CallLowering::ManifestBuiltin(_) | CallLowering::CorelibService(_))
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
            symbol @ ("fiber_join_value" | "channel_receive_value" | "hub_wait_receive_value"),
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

    pub(super) fn traced_channel_send_layout_impl(
        &self,
        key: AstNodeKey,
    ) -> Option<beskid_isle::TracedFiberJoinLayout> {
        let DirectCallee::CorelibService(symbol @ ("channel_send" | "channel_try_send")) = self.direct_callee(key)?
        else {
            return None;
        };
        let arguments = self.call_arguments(key)?;
        let [_, value] = arguments.as_slice() else {
            return None;
        };
        (self.managed_reference_in_context(*value)? == beskid_isle::ManagedReferenceFact::GcManaged).then_some(())?;
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
        let _lambda = self.query(closure_signature(self.db, key))?;
        // Only support capture-free or fully-resolved capture environments.
        let closure_environment = if environment.captures.is_empty() {
            None
        } else {
            let Some(authority) = self.input.closure_lowering_authority(key, key) else {
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
        Some(beskid_isle::LambdaEntry { trampoline: DirectCallee::lambda_trampoline(key), closure_environment })
    }
}
