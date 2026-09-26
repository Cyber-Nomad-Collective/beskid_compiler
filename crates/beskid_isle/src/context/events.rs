use super::*;

macro_rules! generated_event_methods {
    () => {
        fn emit_event_subscribe(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_event_mutation(key, EventOperation::Subscribe)
        }

        fn emit_event_unsubscribe_first(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_event_mutation(key, EventOperation::UnsubscribeFirst)
        }

        fn emit_event_raise_statement(&mut self, key: AstNodeKey) -> Option<()> {
            self.emit_event_raise(key)
        }
    };
}

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn emit_event_raise(&mut self, key: AstNodeKey) -> Option<()> {
        let plan = self.facts.event_operation(key)?;
        (plan.operation == EventOperation::Raise
            && plan.capacity > 0
            && plan.delegate_result == beskid_queries::SemanticTypeId::UNIT)
            .then_some(())?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let receiver_slot = plan.receiver_slot.or_else(|| self.facts.local_slot(plan.receiver));
        let receiver = if let Some(slot) = receiver_slot {
            let binding = self.locals.get(&slot)?;
            self.builder.use_var(binding.variable)
        } else {
            generated::constructor_lower_expression(self, plan.receiver)?
        };
        (self.builder.func.dfg.value_type(receiver) == pointer).then_some(())?;
        let receiver_root = if self.facts.node_kind(plan.receiver) == Some(NodeKind::MethodDefinition) {
            // The implicit receiver is a rooted incoming parameter for this whole method;
            // its reserved slot already keeps it live across any runtime call below.
            None
        } else {
            self.root_expression_value_if_needed(plan.receiver, receiver)?
        };
        let event_slot = self.builder.ins().iadd_imm_s(receiver, i64::from(plan.slot_offset));
        let event = self.builder.ins().load(pointer, MemFlagsData::new(), event_slot, 0);
        let length = self.emit_corelib_service_call(key, "event_len", &[event], &[pointer], Some(pointer))?;

        let mut arguments = Vec::with_capacity(plan.arguments.len());
        let mut argument_roots = Vec::with_capacity(plan.arguments.len());
        for (argument, semantic_type) in plan.arguments.iter().copied().zip(plan.delegate_parameters.iter().copied()) {
            let value_type = event_delegate_type(self.frontend_config, semantic_type)?;
            let value = generated::constructor_lower_expression(self, argument)?;
            let value = if self.builder.func.dfg.value_type(value) == value_type {
                value
            } else {
                self.adapt_scalar_boundary(argument, value, value_type)?
            };
            argument_roots.push(self.root_expression_value_if_needed(argument, value)?);
            arguments.push(value);
        }

        let index = self.builder.declare_var(pointer);
        let zero_index = self.builder.ins().iconst(pointer, 0);
        self.builder.def_var(index, zero_index);
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let capture_free = self.builder.create_block();
        let captured = self.builder.create_block();
        let latch = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.ins().jump(header, &[]);

        self.builder.switch_to_block(header);
        let current = self.builder.use_var(index);
        let done = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, current, length);
        self.builder.ins().brif(done, exit, &[], body, &[]);

        self.builder.switch_to_block(body);
        let index32 = if pointer == types::I32 { current } else { self.builder.ins().ireduce(types::I32, current) };
        let handler = self.emit_corelib_service_call(
            key,
            "event_get_handler",
            &[event, index32],
            &[pointer, types::I32],
            Some(pointer),
        )?;
        self.builder.ins().trapz(handler, TrapCode::unwrap_user(5));
        let code = self.builder.ins().load(pointer, MemFlagsData::new(), handler, 16);
        let environment = self.builder.ins().load(pointer, MemFlagsData::new(), handler, 24);
        let has_environment = self.builder.ins().icmp_imm_s(IntCC::NotEqual, environment, 0);
        self.builder.ins().brif(has_environment, captured, &[], capture_free, &[]);

        self.builder.switch_to_block(capture_free);
        let mut plain_signature = Signature::new(self.builder.func.signature.call_conv);
        plain_signature
            .params
            .extend(arguments.iter().map(|value| AbiParam::new(self.builder.func.dfg.value_type(*value))));
        let plain_signature = self.builder.func.import_signature(plain_signature);
        self.builder.ins().call_indirect(plain_signature, code, &arguments);
        self.builder.ins().jump(latch, &[]);

        self.builder.switch_to_block(captured);
        let mut closure_signature = Signature::new(self.builder.func.signature.call_conv);
        closure_signature.params.push(AbiParam::new(pointer));
        closure_signature
            .params
            .extend(arguments.iter().map(|value| AbiParam::new(self.builder.func.dfg.value_type(*value))));
        let closure_signature = self.builder.func.import_signature(closure_signature);
        let mut closure_arguments = Vec::with_capacity(arguments.len() + 1);
        closure_arguments.push(environment);
        closure_arguments.extend(arguments.iter().copied());
        self.builder.ins().call_indirect(closure_signature, code, &closure_arguments);
        self.builder.ins().jump(latch, &[]);

        self.builder.switch_to_block(latch);
        let current = self.builder.use_var(index);
        let next = self.builder.ins().iadd_imm_s(current, 1);
        self.builder.def_var(index, next);
        self.builder.ins().jump(header, &[]);

        self.builder.seal_block(header);
        self.builder.seal_block(body);
        self.builder.seal_block(capture_free);
        self.builder.seal_block(captured);
        self.builder.seal_block(latch);
        self.builder.switch_to_block(exit);
        self.builder.seal_block(exit);

        for root in argument_roots.into_iter().rev() {
            self.release_expression_root(root)?;
        }
        self.release_expression_root(receiver_root)?;
        Some(())
    }

    pub(super) fn emit_event_handler_local(&mut self, key: AstNodeKey, plan: EventHandlerLocalPlan) -> Option<()> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        let mut signature = Signature::new(self.builder.func.signature.call_conv);
        if plan.closure_environment.is_some() {
            signature.params.push(AbiParam::new(pointer));
        }
        signature.params.extend(plan.parameters.iter().copied().map(AbiParam::new));
        signature.returns.extend(plan.result.map(AbiParam::new));
        let callee = plan.trampoline.clone();
        let trampoline = match self.call_importer.as_deref_mut()?.import(self.builder, callee.clone(), &signature) {
            Ok(function) => function,
            Err(CallImportError::UnknownCallee) => {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(callee) });
                return None;
            }
        };
        let code = self.builder.ins().func_addr(pointer, trampoline);

        let (environment, environment_root) = if let Some(environment_plan) = &plan.closure_environment {
            let (environment, root) = self.emit_inline_closure_environment(environment_plan)?;
            (environment, Some(root))
        } else {
            (self.builder.ins().iconst(pointer, 0), None)
        };
        let request = self.symbol_global("__beskid_event_handler_allocation_request_v5", pointer)?;
        let allocate = self.import_runtime_helper("beskid_rt_v5_managed_object_allocate", &[pointer], Some(pointer))?;
        let allocation = self.builder.ins().call(allocate, &[request]);
        let wrapper = self.builder.inst_results(allocation).first().copied()?;
        self.builder.ins().trapz(wrapper, TrapCode::unwrap_user(5));
        let wrapper_root = self.root_temporary(wrapper)?;
        self.builder.ins().store(MemFlagsData::new(), code, wrapper, 16);
        self.builder.ins().store(MemFlagsData::new(), environment, wrapper, 24);
        self.release_temporary_root(environment_root)?;

        let slot = self.facts.local_slot(key)?;
        self.bind_local(slot, wrapper, pointer, ManagedReferenceFact::GcManaged)?;
        self.unregister_root_slot(wrapper_root)
    }

    pub(super) fn emit_event_mutation(&mut self, key: AstNodeKey, expected: EventOperation) -> Option<Value> {
        let Some(plan) = self.facts.event_operation(key) else {
            return None;
        };
        (plan.operation == expected && plan.capacity > 0 && plan.handler.is_some()).then_some(())?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let receiver = if let Some(slot) = self.facts.local_slot(plan.receiver) {
            let binding = self.locals.get(&slot)?;
            self.builder.use_var(binding.variable)
        } else {
            generated::constructor_lower_expression(self, plan.receiver)?
        };
        (self.builder.func.dfg.value_type(receiver) == pointer).then_some(())?;
        let receiver_root = self.root_expression_value_if_needed(plan.receiver, receiver)?;
        let handler_key = plan.handler?;
        let handler = if let Some(slot) = self.facts.local_slot(handler_key) {
            let binding = self.locals.get(&slot)?;
            self.builder.use_var(binding.variable)
        } else {
            generated::constructor_lower_expression(self, handler_key)?
        };
        (self.builder.func.dfg.value_type(handler) == pointer).then_some(())?;
        let handler_root = self.root_expression_value_if_needed(handler_key, handler)?;
        let slot = self.builder.ins().iadd_imm_s(receiver, i64::from(plan.slot_offset));
        let result = match expected {
            EventOperation::Subscribe => {
                let capacity = self.builder.ins().iconst(pointer, i64::from(plan.capacity));
                self.emit_corelib_service_call(
                    key,
                    "event_subscribe",
                    &[slot, handler, capacity],
                    &[pointer, pointer, pointer],
                    Some(pointer),
                )
            }
            EventOperation::UnsubscribeFirst => self.emit_corelib_service_call(
                key,
                "event_unsubscribe_first",
                &[slot, handler],
                &[pointer, pointer],
                Some(pointer),
            ),
            EventOperation::Raise => None,
        }?;
        self.release_expression_root(handler_root)?;
        self.release_expression_root(receiver_root)?;
        // Compound event assignment has the handler's source value, not the runtime's count.
        let _runtime_count = result;
        Some(handler)
    }
}

fn event_delegate_type(
    frontend_config: TargetFrontendConfig,
    semantic: beskid_queries::SemanticTypeId,
) -> Option<Type> {
    Some(match semantic {
        beskid_queries::SemanticTypeId::BOOL | beskid_queries::SemanticTypeId::U8 => types::I8,
        beskid_queries::SemanticTypeId::I32
        | beskid_queries::SemanticTypeId::U32
        | beskid_queries::SemanticTypeId::CHAR => types::I32,
        beskid_queries::SemanticTypeId::I64 => types::I64,
        beskid_queries::SemanticTypeId::F64 => types::F64,
        beskid_queries::SemanticTypeId::WORD
        | beskid_queries::SemanticTypeId::POINTER
        | beskid_queries::SemanticTypeId::STRING => dispatch::pointer_type(frontend_config),
        _ => return None,
    })
}

pub(super) use generated_event_methods;
