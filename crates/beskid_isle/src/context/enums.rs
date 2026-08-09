use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn bind_match_arm_payload(
        &mut self,
        key: AstNodeKey,
        layout: &EnumLayout,
        scrutinee: Value,
        arm: &MatchArmFact,
    ) -> Option<Option<LocalSlotId>> {
        let Some(binding) = arm.binding else {
            return Some(None);
        };
        let discriminant = arm.discriminant?;
        let payload_layout = layout.variants.iter().find(|variant| variant.discriminant == discriminant)?.payload?;
        if payload_layout.value_type != binding.value_type || self.locals.contains_key(&binding.slot) {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
            return None;
        }
        let payload = self.builder.ins().load(
            payload_layout.value_type,
            MemFlags::new(),
            scrutinee,
            i32::try_from(payload_layout.offset).ok()?,
        );
        let variable = self.builder.declare_var(binding.value_type);
        self.builder.def_var(variable, payload);
        self.locals.insert(binding.slot, (variable, binding.value_type));
        Some(Some(binding.slot))
    }
}

macro_rules! generated_enum_methods {
    () => {
        fn emit_enum_literal(&mut self, key: AstNodeKey) -> Option<Value> {
            let layout = self.facts.enum_layout(key)?;
            if !layout.is_valid() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            }
            let variant_index = self.facts.enum_variant_index(key)?;
            let Some(variant) =
                usize::try_from(variant_index).ok().and_then(|index| layout.variants.get(index)).copied()
            else {
                self.pending_error =
                    Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumVariant(variant_index) });
                return None;
            };
            let allocation = self.facts.managed_struct_allocation(key)?;
            let pointer_type = self.facts.scalar_type(key)?;
            if !pointer_type.is_int() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            }
            let request = self.symbol_global(allocation.allocation_request_symbol.as_ref(), pointer_type)?;
            let allocate = self.import_runtime_helper(
                "beskid_rt_v5_managed_object_allocate",
                &[pointer_type],
                Some(pointer_type),
            )?;
            let allocation_call = self.builder.ins().call(allocate, &[request]);
            let object = self.builder.inst_results(allocation_call).first().copied()?;
            self.builder.ins().trapz(object, TrapCode::unwrap_user(5));
            let tag = self.builder.ins().iconst(layout.tag.value_type, variant.discriminant as i64);
            self.builder.ins().store(MemFlags::new(), tag, object, i32::try_from(layout.tag.offset).ok()?);
            match (variant.payload, self.facts.enum_payload(key)) {
                (Some(payload_layout), Some(payload_key)) => {
                    let payload = generated::constructor_lower_expression(self, payload_key)?;
                    if self.builder.func.dfg.value_type(payload) != payload_layout.value_type {
                        self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                        return None;
                    }
                    self.builder.ins().store(
                        MemFlags::new(),
                        payload,
                        object,
                        i32::try_from(payload_layout.offset).ok()?,
                    );
                }
                (None, None) => {}
                _ => {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                    return None;
                }
            }
            Some(object)
        }

        /// Branch over a syntax-proven `Result<T, E>` propagation expression.
        ///
        /// The error path returns the original managed enum object unchanged, while the success path
        /// loads the canonical first-variant payload. No runtime helper or replacement error object is
        /// synthesized at this boundary.
        fn emit_try_expression(&mut self, key: AstNodeKey) -> Option<Value> {
            let fact = self.facts.try_expression_fact(key)?;
            if fact.expression != key {
                return None;
            }
            let layout = self.facts.enum_layout(key)?;
            let Some(success) = layout.variants.first().copied() else {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            };
            let Some(payload) = success.payload else {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            };
            if !layout.is_valid() || layout.variants.len() != 2 || !layout.tag.value_type.is_int() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            }
            let operand = generated::constructor_lower_expression(self, fact.operand)?;
            let operand_type = self.builder.func.dfg.value_type(operand);
            let payload_type = self.facts.scalar_type(key)?;
            let return_type = self.builder.func.signature.returns.first()?.value_type;
            if !operand_type.is_int() || payload.value_type != payload_type || return_type != operand_type {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            }

            let tag = self.builder.ins().load(
                layout.tag.value_type,
                MemFlags::new(),
                operand,
                i32::try_from(layout.tag.offset).ok()?,
            );
            let success_tag = self.builder.ins().iconst(layout.tag.value_type, success.discriminant as i64);
            let is_success = self.builder.ins().icmp(IntCC::Equal, tag, success_tag);
            let success_block = self.builder.create_block();
            let error_block = self.builder.create_block();
            let merge_block = self.builder.create_block();
            self.builder.append_block_param(merge_block, payload_type);
            self.builder.ins().brif(is_success, success_block, &[], error_block, &[]);

            self.builder.switch_to_block(success_block);
            self.builder.seal_block(success_block);
            let value =
                self.builder.ins().load(payload_type, MemFlags::new(), operand, i32::try_from(payload.offset).ok()?);
            self.builder.ins().jump(merge_block, &[value.into()]);

            self.builder.switch_to_block(error_block);
            self.builder.seal_block(error_block);
            self.builder.ins().return_(&[operand]);

            self.builder.switch_to_block(merge_block);
            self.builder.seal_block(merge_block);
            self.builder.block_params(merge_block).first().copied()
        }

        fn emit_match(&mut self, key: AstNodeKey) -> Option<Value> {
            let layout = self.facts.enum_layout(key)?;
            if !layout.is_valid() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            }
            let arms = self.facts.match_arms(key)?;
            if arms.is_empty() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                return None;
            }
            let layout_discriminants =
                layout.variants.iter().map(|variant| variant.discriminant).collect::<HashSet<_>>();
            let mut covered = HashSet::with_capacity(arms.len());
            let wildcard_index = arms.iter().position(|arm| arm.discriminant.is_none());
            if wildcard_index.is_some_and(|index| index + 1 != arms.len())
                || arms.iter().filter(|arm| arm.discriminant.is_none()).count() > 1
                || arms
                    .iter()
                    .filter_map(|arm| arm.discriminant)
                    .any(|tag| !layout_discriminants.contains(&tag) || !covered.insert(tag))
            {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                return None;
            }
            if wildcard_index.is_none() && covered != layout_discriminants {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::NonExhaustiveMatch });
                return None;
            }

            let scrutinee_key = self.facts.child(key, 0)?;
            let scrutinee = generated::constructor_lower_expression(self, scrutinee_key)?;
            if !self.builder.func.dfg.value_type(scrutinee).is_int() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            }
            let tag = self.builder.ins().load(
                layout.tag.value_type,
                MemFlags::new(),
                scrutinee,
                i32::try_from(layout.tag.offset).ok()?,
            );
            let result_type =
                self.facts.scalar_type(key).or_else(|| arms.iter().find_map(|arm| self.facts.scalar_type(arm.body)))?;
            let merge = self.builder.create_block();
            self.builder.append_block_param(merge, result_type);
            let arm_blocks = arms.iter().map(|_| self.builder.create_block()).collect::<Vec<_>>();
            let trap = wildcard_index.is_none().then(|| self.builder.create_block());
            let default = wildcard_index
                .map_or_else(|| trap.expect("trap block exists without wildcard"), |index| arm_blocks[index]);
            let mut switch = Switch::new();
            for (arm, block) in arms.iter().zip(&arm_blocks) {
                if let Some(discriminant) = arm.discriminant {
                    switch.set_entry(u128::from(discriminant), *block);
                }
            }
            switch.emit(self.builder, tag, default);

            for (arm, block) in arms.into_iter().zip(arm_blocks) {
                self.builder.switch_to_block(block);
                self.builder.seal_block(block);
                let binding = self.bind_match_arm_payload(key, &layout, scrutinee, &arm)?;
                let value = generated::constructor_lower_expression(self, arm.body)?;
                if self.builder.func.dfg.value_type(value) != result_type {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                }
                if let Some(binding) = binding {
                    self.locals.remove(&binding);
                }
                self.builder.ins().jump(merge, &[value.into()]);
            }
            if let Some(trap) = trap {
                self.builder.switch_to_block(trap);
                self.builder.seal_block(trap);
                self.builder.ins().trap(TrapCode::unwrap_user(1));
            }
            self.builder.switch_to_block(merge);
            self.builder.seal_block(merge);
            self.builder.block_params(merge).first().copied()
        }

        fn emit_match_statement(&mut self, key: AstNodeKey) -> Option<()> {
            let layout = self.facts.enum_layout(key)?;
            if !layout.is_valid() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            }
            let arms = self.facts.match_arms(key)?;
            if arms.is_empty() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                return None;
            }
            let layout_discriminants =
                layout.variants.iter().map(|variant| variant.discriminant).collect::<HashSet<_>>();
            let mut covered = HashSet::with_capacity(arms.len());
            let wildcard_index = arms.iter().position(|arm| arm.discriminant.is_none());
            if wildcard_index.is_some_and(|index| index + 1 != arms.len())
                || arms.iter().filter(|arm| arm.discriminant.is_none()).count() > 1
                || arms
                    .iter()
                    .filter_map(|arm| arm.discriminant)
                    .any(|tag| !layout_discriminants.contains(&tag) || !covered.insert(tag))
            {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                return None;
            }
            if wildcard_index.is_none() && covered != layout_discriminants {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::NonExhaustiveMatch });
                return None;
            }

            let scrutinee_key = self.facts.child(key, 0)?;
            let scrutinee = generated::constructor_lower_expression(self, scrutinee_key)?;
            if !self.builder.func.dfg.value_type(scrutinee).is_int() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            }
            let tag = self.builder.ins().load(
                layout.tag.value_type,
                MemFlags::new(),
                scrutinee,
                i32::try_from(layout.tag.offset).ok()?,
            );
            let merge = self.builder.create_block();
            let arm_blocks = arms.iter().map(|_| self.builder.create_block()).collect::<Vec<_>>();
            let trap = wildcard_index.is_none().then(|| self.builder.create_block());
            let default = wildcard_index
                .map_or_else(|| trap.expect("trap block exists without wildcard"), |index| arm_blocks[index]);
            let mut switch = Switch::new();
            for (arm, block) in arms.iter().zip(&arm_blocks) {
                if let Some(discriminant) = arm.discriminant {
                    switch.set_entry(u128::from(discriminant), *block);
                }
            }
            switch.emit(self.builder, tag, default);

            let mut merge_reachable = false;
            for (arm, block) in arms.into_iter().zip(arm_blocks) {
                self.builder.switch_to_block(block);
                self.builder.seal_block(block);
                let binding = self.bind_match_arm_payload(key, &layout, scrutinee, &arm)?;
                generated::constructor_lower_statement(self, arm.body)?;
                if let Some(binding) = binding {
                    self.locals.remove(&binding);
                }
                if jump_from_current_if_unterminated(self.builder, merge) {
                    merge_reachable = true;
                }
            }
            if let Some(trap) = trap {
                self.builder.switch_to_block(trap);
                self.builder.seal_block(trap);
                self.builder.ins().trap(TrapCode::unwrap_user(1));
            }
            self.builder.switch_to_block(merge);
            self.builder.seal_block(merge);
            if !merge_reachable {
                self.builder.ins().trap(TrapCode::unwrap_user(1));
            }
            Some(())
        }
    };
}

pub(super) use generated_enum_methods;
