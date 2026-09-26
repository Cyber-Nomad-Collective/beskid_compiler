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
            .flat_map(|scope| scope.composition_cleanups.iter().rev().copied())
            .collect::<Vec<_>>();
        let pointer = dispatch::pointer_type(self.frontend_config);
        for action in actions {
            match action {
                CompositionCleanup::ScopeLeave { site } => {
                    self.composition_call(site, "composition_scope_leave", &[], &[], None)?;
                }
                CompositionCleanup::Container { site, value } => {
                    self.composition_call(site, "composition_shutdown", &[value], &[pointer], None)?;
                    self.composition_call(site, "composition_container_drop", &[value], &[pointer], None)?;
                }
            }
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
                            self.builder.ins().store(MemFlagsData::new(), target, address, 0);
                            let barrier = self.import_runtime_helper(
                                "beskid_rt_v5_array_write_barrier",
                                &[pointer, pointer],
                                Some(types::I8),
                            )?;
                            let call = self.builder.ins().call(barrier, &[array, target]);
                            let published = self.builder.inst_results(call).first().copied()?;
                            self.composition_failure_guard(site, container, published, 8)?;
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
            self.local_root_scopes
                .last_mut()?
                .composition_cleanups
                .push(CompositionCleanup::Container { site, value: container });
            Some(())
        }

        fn emit_composition_scope(&mut self, site: AstNodeKey) -> Option<()> {
            let plan = self.facts.composition_scope(site)?;
            (plan.site == site && plan.scope_id != 0).then_some(())?;
            let container = self.local_root_scopes.iter().rev().find_map(|scope| {
                scope.composition_cleanups.iter().rev().find_map(|cleanup| match cleanup {
                    CompositionCleanup::Container { value, .. } => Some(*value),
                    CompositionCleanup::ScopeLeave { .. } => None,
                })
            })?;
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
            self.lower_nested_statement(plan.body)?;
            self.end_local_root_scope_for_current_block()
        }
    };
}

pub(super) use generated_composition_methods;
