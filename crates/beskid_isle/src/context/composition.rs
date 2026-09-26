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
}

macro_rules! generated_composition_methods {
    () => {
        fn emit_composition_launch(&mut self, site: AstNodeKey) -> Option<()> {
            let plan = self.facts.composition_launch(site)?;
            (plan.site == site).then_some(())?;
            let pointer = dispatch::pointer_type(self.frontend_config);
            let slots = self.builder.ins().iconst(pointer, i64::from(plan.slot_count));
            let container =
                self.composition_call(site, "composition_container_create", &[slots], &[pointer], Some(pointer))??;
            self.builder.ins().trapz(container, TrapCode::unwrap_user(5));
            let launched =
                self.composition_call(site, "composition_launch", &[container], &[pointer], Some(types::I8))??;
            self.builder.ins().trapz(launched, TrapCode::unwrap_user(9));
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
            self.composition_call(site, "composition_scope_enter", &[container], &[pointer], None)?;
            self.begin_local_root_scope();
            self.local_root_scopes.last_mut()?.composition_cleanups.push(CompositionCleanup::ScopeLeave { site });
            self.lower_nested_statement(plan.body)?;
            self.end_local_root_scope_for_current_block()
        }
    };
}

pub(super) use generated_composition_methods;
