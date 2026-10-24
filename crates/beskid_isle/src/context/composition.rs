//! Generated composition statements consume only the frozen, source-keyed plan.

use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn composition_call(
        &mut self,
        site: AstNodeKey,
        symbol: &'static str,
        arguments: &[Value],
        parameters: &[Type],
        result: Option<Type>,
    ) -> Option<Option<Value>> {
        let mut signature = Signature::new(self.builder.func.signature.call_conv);
        signature.params.extend(parameters.iter().copied().map(AbiParam::new));
        signature.returns.extend(result.map(AbiParam::new));
        let callee = DirectCallee::corelib_service(symbol);
        let function = match self.call_importer.as_deref_mut()?.import(self.builder, callee.clone(), &signature) {
            Ok(function) => function,
            Err(CallImportError::UnknownCallee) => {
                self.pending_error = Some(LoweringError { key: site, kind: LoweringErrorKind::UnknownCallee(callee) });
                return None;
            }
        };
        let call = self.builder.ins().call(function, arguments);
        if result.is_some() {
            Some(Some(*self.builder.inst_results(call).first()?))
        } else {
            self.builder.inst_results(call).is_empty().then_some(None)
        }
    }

    pub(crate) fn emit_composition_cleanup_from(&mut self, depth: usize) -> Option<()> {
        let actions = self
            .local_root_scopes
            .get(depth..)?
            .iter()
            .rev()
            .flat_map(|scope| scope.composition_cleanups.iter().rev().cloned())
            .collect::<Vec<_>>();
        let pointer = dispatch::pointer_type(self.frontend_config);
        for action in actions {
            match action {
                CompositionCleanup::ScopeLeave { site } => {
                    // `dispose` runs on every exit of the `with` body, before the frame is left.
                    let plan = self.facts.composition_scope(site)?;
                    let (_, slots) = self.active_composition_container()?;
                    self.emit_composition_hook_calls(site, &plan.dispose, &slots)?;
                    self.composition_call(site, "composition_scope_leave", &[], &[], None)?;
                }
                CompositionCleanup::Container { site, value, slots } => {
                    let plan = self.facts.composition_launch(site)?;
                    self.emit_composition_hook_calls(site, &plan.shutdown, &slots)?;
                    self.composition_call(site, "composition_shutdown", &[value], &[pointer], None)?;
                    self.composition_call(site, "composition_container_drop", &[value], &[pointer], None)?;
                }
            }
        }
        Some(())
    }

    /// The innermost launched container of this function and its installed slot objects.
    pub(super) fn active_composition_container(&self) -> Option<(Value, std::rc::Rc<[Value]>)> {
        self.local_root_scopes.iter().rev().find_map(|scope| {
            scope.composition_cleanups.iter().rev().find_map(|cleanup| match cleanup {
                CompositionCleanup::Container { value, slots, .. } => Some((*value, std::rc::Rc::clone(slots))),
                CompositionCleanup::ScopeLeave { .. } => None,
            })
        })
    }

    /// The arguments of one hook call: the site's record, when the hook takes one, then its
    /// argument object. The argument objects are container slots, rooted for the whole launch.
    fn composition_hook_arguments(&mut self, call: &crate::CompositionHookCall, slots: &[Value]) -> Option<Vec<Value>> {
        (call.signature.params.len() == usize::from(call.record.is_some()) + usize::from(call.arguments_slot.is_some()))
            .then_some(())?;
        let mut arguments = Vec::with_capacity(call.signature.params.len());
        if let Some(record) = call.record {
            // The record local is declared just before the `launch`/`with` statement, in the
            // same block, so it is live at every hook call of that site.
            let slot = self.facts.local_slot(record)?;
            arguments.push(self.builder.use_var(self.locals.get(&slot)?.variable));
        }
        if let Some(slot) = call.arguments_slot {
            arguments.push(*slots.get(usize::try_from(slot).ok()?)?);
        }
        Some(arguments)
    }

    fn import_composition_hook(&mut self, site: AstNodeKey, call: &crate::CompositionHookCall) -> Option<FuncRef> {
        match self.call_importer.as_deref_mut()?.import(self.builder, call.function.clone(), &call.signature) {
            Ok(function) => Some(function),
            Err(CallImportError::UnknownCallee) => {
                self.pending_error =
                    Some(LoweringError { key: site, kind: LoweringErrorKind::UnknownCallee(call.function.clone()) });
                None
            }
        }
    }

    /// Call each generated hook function with its arguments.
    pub(super) fn emit_composition_hook_calls(
        &mut self,
        site: AstNodeKey,
        calls: &[crate::CompositionHookCall],
        slots: &[Value],
    ) -> Option<()> {
        for call in calls {
            let arguments = self.composition_hook_arguments(call, slots)?;
            let function = self.import_composition_hook(site, call)?;
            self.builder.ins().call(function, &arguments);
        }
        Some(())
    }

    /// Register the `dispose` hook calls of the level just entered (the launched container, or
    /// the scope just entered) with the runtime, so `composition_exit` (`CommandLine.Exit`) can
    /// run them. They are registered in reverse order: the runtime runs its list from the head.
    /// Ordinary exits run the hooks directly; leaving the level releases the registrations.
    pub(super) fn emit_composition_exit_registrations(
        &mut self,
        site: AstNodeKey,
        container: Value,
        calls: &[crate::CompositionHookCall],
        slots: &[Value],
    ) -> Option<()> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        for call in calls.iter().rev() {
            let arguments = self.composition_hook_arguments(call, slots)?;
            (arguments.len() <= crate::COMPOSITION_HOOK_ARGUMENTS_MAX).then_some(())?;
            let function = self.import_composition_hook(site, call)?;
            let address = self.builder.ins().func_addr(pointer, function);
            let array = self.builder.create_sized_stack_slot(StackSlotData::new(
                StackSlotKind::ExplicitSlot,
                pointer.bytes() * u32::try_from(arguments.len().max(1)).ok()?,
                pointer.bytes().ilog2() as u8,
            ));
            for (index, argument) in arguments.iter().enumerate() {
                let offset = i32::try_from(index * pointer.bytes() as usize).ok()?;
                self.builder.ins().stack_store(pointer, *argument, array, offset);
            }
            let array_address = self.builder.ins().stack_addr(pointer, array, 0);
            let count = self.builder.ins().iconst(pointer, i64::try_from(arguments.len()).ok()?);
            self.composition_call(
                site,
                "composition_exit_register",
                &[container, address, array_address, count],
                &[pointer, pointer, pointer, pointer],
                None,
            )?;
        }
        Some(())
    }

    pub(super) fn composition_failure_guard(
        &mut self,
        site: AstNodeKey,
        container: Value,
        status: Value,
        code: u8,
    ) -> Option<()> {
        let active = self.builder.create_block();
        let failed = self.builder.create_block();
        self.builder.ins().brif(status, active, &[], failed, &[]);
        self.builder.switch_to_block(failed);
        self.builder.seal_block(failed);
        self.release_local_roots_from(0)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        self.composition_call(site, "composition_shutdown", &[container], &[pointer], None)?;
        self.composition_call(site, "composition_container_drop", &[container], &[pointer], None)?;
        self.emit_composition_cleanup_from(0)?;
        self.builder.ins().trap(TrapCode::unwrap_user(code));
        self.builder.switch_to_block(active);
        self.builder.seal_block(active);
        Some(())
    }
}

macro_rules! generated_composition_methods {
    () => {
        fn emit_composition_launch(&mut self, site: AstNodeKey) -> Option<()> {
            let plan = self.facts.composition_launch(site)?;
            (plan.site == site && usize::try_from(plan.slot_count).ok()? == plan.registrations.len()).then_some(())?;
            let pointer = dispatch::pointer_type(self.frontend_config);
            let slots = self.builder.ins().iconst(pointer, i64::from(plan.slot_count));
            let container =
                self.composition_call(site, "composition_container_create", &[slots], &[pointer], Some(pointer))??;
            self.builder.ins().trapz(container, TrapCode::unwrap_user(5));
            let mut installed = Vec::with_capacity(plan.registrations.len());
            for registration in &plan.registrations {
                (usize::try_from(registration.slot).ok()? == installed.len()).then_some(())?;
                let mut field_values = Vec::with_capacity(registration.injections.len());
                let mut array_roots = Vec::new();
                for injection in &registration.injections {
                    let value = if let Some(symbol) = &injection.plural_allocation_request_symbol {
                        let request = self.symbol_global(symbol.as_ref(), pointer)?;
                        let root_slot = self.builder.create_sized_stack_slot(StackSlotData::new(
                            StackSlotKind::ExplicitSlot,
                            pointer.bytes(),
                            pointer.bytes().ilog2() as u8,
                        ));
                        let root_address = self.builder.ins().stack_addr(pointer, root_slot, 0);
                        let allocate = self.import_runtime_helper(
                            "beskid_rt_v5_array_allocate_rooted",
                            &[pointer, pointer],
                            Some(pointer),
                        )?;
                        let call = self.builder.ins().call(allocate, &[request, root_address]);
                        let array = self.builder.inst_results(call).first().copied()?;
                        self.composition_failure_guard(site, container, array, 5)?;
                        let root = ScopedTemporaryRoot::ArrayConstruction(root_slot);
                        self.track_expression_root(root)?;
                        array_roots.push(root);
                        let data = self.builder.ins().load(pointer, MemFlagsData::new(), array, 0);
                        for (index, slot) in injection.target_slots.iter().enumerate() {
                            let target = *installed.get(usize::try_from(*slot).ok()?)?;
                            let byte_offset = i64::try_from(index.checked_mul(pointer.bytes() as usize)?).ok()?;
                            let address = self.builder.ins().iadd_imm_s(data, byte_offset);
                            // Initialization store into the array allocated above: no barrier.
                            self.builder.ins().store(MemFlagsData::new(), target, address, 0);
                        }
                        array
                    } else {
                        let [slot] = injection.target_slots.as_slice() else {
                            return None;
                        };
                        *installed.get(usize::try_from(*slot).ok()?)?
                    };
                    field_values.push((injection.field_offset, value));
                }
                let request = self.symbol_global(registration.allocation_request_symbol.as_ref(), pointer)?;
                let allocate =
                    self.import_runtime_helper("beskid_rt_v5_managed_object_allocate", &[pointer], Some(pointer))?;
                let call = self.builder.ins().call(allocate, &[request]);
                let object = self.builder.inst_results(call).first().copied()?;
                self.composition_failure_guard(site, container, object, 5)?;
                for (offset, value) in field_values {
                    let address = self.builder.ins().iadd_imm_s(object, i64::from(offset));
                    self.builder.ins().store(MemFlagsData::new(), value, address, 0);
                }
                let slot = self.builder.ins().iconst(pointer, i64::from(registration.slot));
                let stored = self.composition_call(
                    site,
                    "composition_slot_store",
                    &[container, slot, object],
                    &[pointer, pointer, pointer],
                    Some(types::I8),
                )??;
                self.composition_failure_guard(site, container, stored, 9)?;
                installed.push(object);
                for root in array_roots.into_iter().rev() {
                    self.release_expression_root(Some(root))?;
                }
            }
            let launched =
                self.composition_call(site, "composition_launch", &[container], &[pointer], Some(types::I8))??;
            self.composition_failure_guard(site, container, launched, 9)?;
            let slots = std::rc::Rc::<[Value]>::from(installed);
            // The container is registered for teardown first, so an early exit from a
            // `startup` caller path still runs the global `dispose` hooks and shuts down.
            self.local_root_scopes.last_mut()?.composition_cleanups.push(CompositionCleanup::Container {
                site,
                value: container,
                slots: std::rc::Rc::clone(&slots),
            });
            // `CommandLine.Exit` from here on runs the global `dispose` hooks.
            self.emit_composition_exit_registrations(site, container, &plan.shutdown, &slots)?;
            self.emit_composition_hook_calls(site, &plan.startup, &slots)?;
            Some(())
        }

        fn emit_composition_scope(&mut self, site: AstNodeKey) -> Option<()> {
            let plan = self.facts.composition_scope(site)?;
            (plan.site == site && plan.scope_id != 0).then_some(())?;
            let (container, slots) = self.active_composition_container()?;
            let pointer = dispatch::pointer_type(self.frontend_config);
            let scope_id = self.builder.ins().iconst(pointer, i64::from(plan.scope_id));
            let parent_scope_id = self.builder.ins().iconst(pointer, i64::from(plan.parent_scope_id));
            self.composition_call(
                site,
                "composition_scope_enter",
                &[container, scope_id, parent_scope_id],
                &[pointer, pointer, pointer],
                None,
            )?;
            self.begin_local_root_scope();
            self.local_root_scopes.last_mut()?.composition_cleanups.push(CompositionCleanup::ScopeLeave { site });
            // `init` runs after the frame is entered and before the body. The leave cleanup is
            // already registered, so every exit of the body runs `dispose`.
            self.emit_composition_hook_calls(site, &plan.init, &slots)?;
            // Registered once `init` completed: an exit runs `dispose` only for such an activation.
            self.emit_composition_exit_registrations(site, container, &plan.dispose, &slots)?;
            self.lower_nested_statement(plan.body)?;
            self.end_local_root_scope_for_current_block()
        }
    };
}

pub(super) use generated_composition_methods;
