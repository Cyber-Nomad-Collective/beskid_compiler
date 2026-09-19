use super::*;

impl IsleContext<'_, '_, '_, '_> {
    /// Emit a slice of the one lexical scope stack without consuming compiler state: sibling
    /// control-flow paths must independently emit the same active regions. The rooted pending
    /// slot is initially null; the first cleanup failure wins, but every destructor still runs.
    fn emit_cleanup_sequence(&mut self, from: usize, to: usize, pending: StackSlot) -> Option<()> {
        let plans = self
            .local_root_scopes
            .get(from..to)?
            .iter()
            .rev()
            .flat_map(|scope| scope.cleanups.iter().rev().cloned())
            .collect::<Vec<_>>();
        let pointer = dispatch::pointer_type();
        for plan in plans {
            let slot = self.facts.local_slot(plan.binding)?;
            let receiver = self.builder.use_var(self.locals.get(&slot)?.variable);
            let function =
                self.call_importer.as_deref_mut()?.import(self.builder, plan.dispose, &plan.dispose_signature).ok()?;
            let call = self.builder.ins().call(function, &[receiver]);
            let result = self.builder.inst_results(call).first().copied()?;
            let result_root = self.root_temporary(result)?;
            let tag = self.builder.ins().load(
                plan.dispose_layout.tag.value_type,
                MemFlags::new(),
                result,
                i32::try_from(plan.dispose_layout.tag.offset).ok()?,
            );
            let error_variant = plan.dispose_layout.variants.get(1)?;
            let failed = self.builder.ins().icmp_imm(IntCC::Equal, tag, error_variant.discriminant as i64);
            let prior = self.builder.ins().stack_load(pointer, pending, 0);
            let empty = self.builder.ins().icmp_imm(IntCC::Equal, prior, 0);
            let select = self.builder.ins().band(failed, empty);
            let error_block = self.builder.create_block();
            let next = self.builder.create_block();
            self.builder.ins().brif(select, error_block, &[], next, &[]);
            self.builder.switch_to_block(error_block);
            self.builder.seal_block(error_block);
            let [Some(field)] = error_variant.payload_fields.as_slice() else {
                return None;
            };
            let error =
                self.builder.ins().load(field.value_type, MemFlags::new(), result, i32::try_from(field.offset).ok()?);
            let converted = if let Some((callee, signature)) = plan.conversion {
                let function = self.call_importer.as_deref_mut()?.import(self.builder, callee, &signature).ok()?;
                let call = self.builder.ins().call(function, &[error]);
                self.builder.inst_results(call).first().copied()
            } else {
                Some(error)
            };
            let converted_root =
                if plan.converted_error_managed { Some(self.root_temporary(converted?)?) } else { None };
            let object = self.allocate_enum_variant(&plan.allocation, &plan.enclosing_layout, 1)?;
            match (plan.enclosing_layout.variants.get(1)?.payload_fields.as_slice(), converted) {
                ([Some(field)], Some(converted)) => {
                    (self.builder.func.dfg.value_type(converted) == field.value_type).then_some(())?;
                    self.builder.ins().store(MemFlags::new(), converted, object, i32::try_from(field.offset).ok()?);
                }
                ([None], None) => {}
                _ => return None,
            }
            self.builder.ins().stack_store(object, pending, 0);
            self.release_temporary_root(converted_root)?;
            self.builder.ins().jump(next, &[]);
            self.builder.switch_to_block(next);
            self.builder.seal_block(next);
            self.release_temporary_root(Some(result_root))?;
        }
        Some(())
    }

    pub(super) fn return_with_cleanup(&mut self, value: Value) -> Option<()> {
        if self.local_root_scopes.iter().all(|scope| scope.cleanups.is_empty()) {
            self.release_managed_local_roots()?;
            self.builder.ins().return_(&[value]);
            return Some(());
        }
        let pointer = dispatch::pointer_type();
        let value_root = self.root_temporary(value)?;
        let null = self.builder.ins().iconst(pointer, 0);
        let pending = self.root_temporary(null)?;
        self.emit_cleanup_sequence(0, self.local_root_scopes.len(), pending)?;
        let failure = self.builder.ins().stack_load(pointer, pending, 0);
        let failed = self.builder.ins().icmp_imm(IntCC::NotEqual, failure, 0);
        let selected = self.builder.ins().select(failed, failure, value);
        self.release_temporary_root(Some(pending))?;
        self.release_temporary_root(Some(value_root))?;
        self.release_managed_local_roots()?;
        self.builder.ins().return_(&[selected]);
        Some(())
    }

    /// Successful structured exits only clean their leaving scopes. A cleanup failure changes
    /// the exit into an error return, draining the remaining outer regions with the same slot.
    pub(super) fn cleanup_scope_exit(&mut self, depth: usize, abandoned_root: Option<StackSlot>) -> Option<()> {
        if self.local_root_scopes.get(depth..)?.iter().all(|scope| scope.cleanups.is_empty()) {
            return Some(());
        }
        let pointer = dispatch::pointer_type();
        let null = self.builder.ins().iconst(pointer, 0);
        let pending = self.root_temporary(null)?;
        self.emit_cleanup_sequence(depth, self.local_root_scopes.len(), pending)?;
        let failure = self.builder.ins().stack_load(pointer, pending, 0);
        let failed = self.builder.ins().icmp_imm(IntCC::NotEqual, failure, 0);
        let error_block = self.builder.create_block();
        let resume = self.builder.create_block();
        self.builder.ins().brif(failed, error_block, &[], resume, &[]);
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        self.emit_cleanup_sequence(0, depth, pending)?;
        let failure = self.builder.ins().stack_load(pointer, pending, 0);
        self.release_temporary_root(abandoned_root)?;
        self.release_temporary_root(Some(pending))?;
        self.release_managed_local_roots()?;
        self.builder.ins().return_(&[failure]);
        self.builder.switch_to_block(resume);
        self.builder.seal_block(resume);
        self.release_temporary_root(Some(pending))?;
        Some(())
    }
}
