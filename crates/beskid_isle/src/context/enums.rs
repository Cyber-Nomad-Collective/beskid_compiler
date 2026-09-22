use super::*;

#[derive(Clone, Copy)]
struct MatchDestinations {
    success: Block,
    failure: Block,
}

#[derive(Clone, Copy)]
struct MatchedEnum<'a> {
    object: Value,
    layout: &'a EnumLayout,
}

fn pattern_is_irrefutable(pattern: &MatchPayloadPatternFact) -> bool {
    matches!(
        pattern,
        MatchPayloadPatternFact::Ignore | MatchPayloadPatternFact::Unit | MatchPayloadPatternFact::Binding(_)
    )
}

fn pattern_rows_are_exhaustive(rows: &[Vec<MatchPayloadPatternFact>]) -> bool {
    let Some(arity) = rows.first().map(Vec::len) else {
        return false;
    };
    if rows.iter().any(|row| row.len() != arity) {
        return false;
    }
    if arity == 0 {
        return true;
    }

    let nested_layout = rows.iter().find_map(|row| match &row[0] {
        MatchPayloadPatternFact::Enum { layout, .. } => Some(layout),
        _ => None,
    });
    let Some(layout) = nested_layout else {
        let irrefutable =
            rows.iter().filter(|row| pattern_is_irrefutable(&row[0])).map(|row| row[1..].to_vec()).collect::<Vec<_>>();
        return pattern_rows_are_exhaustive(&irrefutable);
    };
    if !layout.is_valid()
        || rows.iter().any(|row| match &row[0] {
            MatchPayloadPatternFact::Enum { layout: candidate, discriminant, .. } => {
                candidate != layout || !layout.variants.iter().any(|variant| variant.discriminant == *discriminant)
            }
            MatchPayloadPatternFact::Ignore | MatchPayloadPatternFact::Unit | MatchPayloadPatternFact::Binding(_) => {
                false
            }
            MatchPayloadPatternFact::Fields(_) | MatchPayloadPatternFact::ScalarLiteral { .. } => true,
        })
    {
        return false;
    }

    layout.variants.iter().all(|variant| {
        let specialized = rows
            .iter()
            .filter_map(|row| {
                let mut fields = match &row[0] {
                    pattern if pattern_is_irrefutable(pattern) => {
                        vec![MatchPayloadPatternFact::Ignore; variant.payload_fields.len()]
                    }
                    MatchPayloadPatternFact::Enum { discriminant, payload, .. }
                        if *discriminant == variant.discriminant =>
                    {
                        let MatchPayloadPatternFact::Fields(fields) = payload.as_ref() else {
                            return None;
                        };
                        if fields.len() != variant.payload_fields.len() {
                            return None;
                        }
                        fields.clone()
                    }
                    _ => return None,
                };
                fields.extend_from_slice(&row[1..]);
                Some(fields)
            })
            .collect::<Vec<_>>();
        pattern_rows_are_exhaustive(&specialized)
    })
}

fn payload_patterns_are_exhaustive(patterns: &[&MatchPayloadPatternFact]) -> bool {
    let rows = patterns
        .iter()
        .filter_map(|pattern| match pattern {
            MatchPayloadPatternFact::Fields(fields) => Some(fields.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    rows.len() == patterns.len() && pattern_rows_are_exhaustive(&rows)
}

impl IsleContext<'_, '_, '_, '_> {
    /// Propagate the exact error payload through the enclosing Result layout.
    /// Identical layouts can forward the immutable value; distinct layouts use
    /// the same rooted enum construction as an explicit Error constructor.
    pub(super) fn emit_try_dispatch(&mut self, key: AstNodeKey) -> Option<Option<Value>> {
        let fact = self.facts.try_expression_fact(key)?;
        if fact.expression != key {
            return None;
        }
        let layout = self.facts.enum_layout(key)?;
        let returned = self.facts.try_return_layout(key)?;
        if !layout.is_valid() || !returned.is_valid() || layout.variants.len() != 2 || returned.variants.len() != 2 {
            return None;
        }
        let success = layout.variants.first()?;
        let [payload] = success.payload_fields.as_slice() else {
            return None;
        };
        let operand = generated::constructor_lower_expression(self, fact.operand)?;
        let operand_type = self.builder.func.dfg.value_type(operand);
        if self.builder.func.signature.returns.first()?.value_type != operand_type {
            return None;
        }
        let payload_type = if fact.payload_type == beskid_queries::SemanticTypeId::UNIT {
            if payload.is_some() {
                return None;
            }
            None
        } else {
            let ty = self.facts.scalar_type(key)?;
            if payload.as_ref()?.value_type != ty {
                return None;
            }
            Some(ty)
        };
        let tag = self.builder.ins().load(
            layout.tag.value_type,
            MemFlagsData::new(),
            operand,
            i32::try_from(layout.tag.offset).ok()?,
        );
        let is_success = self.builder.ins().icmp_imm_s(IntCC::Equal, tag, success.discriminant as i64);
        let success_block = self.builder.create_block();
        let error_block = self.builder.create_block();
        let merge = self.builder.create_block();
        if let Some(ty) = payload_type {
            self.builder.append_block_param(merge, ty);
        }
        self.builder.ins().brif(is_success, success_block, &[], error_block, &[]);
        self.builder.switch_to_block(success_block);
        self.builder.seal_block(success_block);
        if let Some(ty) = payload_type {
            let value = self.builder.ins().load(
                ty,
                MemFlagsData::new(),
                operand,
                i32::try_from(payload.as_ref()?.offset).ok()?,
            );
            self.builder.ins().jump(merge, &[value.into()]);
        } else {
            self.builder.ins().jump(merge, &[]);
        }
        self.builder.switch_to_block(error_block);
        self.builder.seal_block(error_block);
        let error_result = if fact.operand_layout == fact.return_layout {
            operand
        } else {
            let source_error = layout.variants.get(1)?;
            let target_error = returned.variants.get(1)?;
            let [source_field] = source_error.payload_fields.as_slice() else {
                return None;
            };
            let [target_field] = target_error.payload_fields.as_slice() else {
                return None;
            };
            let error_value = match (source_field, target_field) {
                (Some(source), Some(target)) if source.value_type == target.value_type => {
                    Some(self.builder.ins().load(
                        source.value_type,
                        MemFlagsData::new(),
                        operand,
                        i32::try_from(source.offset).ok()?,
                    ))
                }
                (None, None) => None,
                _ => return None,
            };
            let root = if fact.error_managed { Some(self.root_temporary(error_value?)?) } else { None };
            let allocation = self.facts.managed_struct_allocation(key)?;
            let result = self.allocate_enum_variant(&allocation, &returned, 1)?;
            if let (Some(value), Some(field)) = (error_value, target_field) {
                self.builder.ins().store(MemFlagsData::new(), value, result, i32::try_from(field.offset).ok()?);
            }
            self.release_temporary_root(root)?;
            result
        };
        self.return_with_cleanup(error_result)?;
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        Some(payload_type.and_then(|_| self.builder.block_params(merge).first().copied()))
    }

    /// Shared managed enum construction for source constructors and cleanup errors.
    pub(super) fn allocate_enum_variant(
        &mut self,
        allocation: &crate::ManagedStructAllocation,
        layout: &EnumLayout,
        variant: usize,
    ) -> Option<Value> {
        if !layout.is_valid() {
            return None;
        }
        let variant = layout.variants.get(variant)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let request = self.symbol_global(allocation.allocation_request_symbol.as_ref(), pointer)?;
        let allocate = self.import_runtime_helper("beskid_rt_v5_managed_object_allocate", &[pointer], Some(pointer))?;
        let call = self.builder.ins().call(allocate, &[request]);
        let object = self.builder.inst_results(call).first().copied()?;
        self.builder.ins().trapz(object, TrapCode::unwrap_user(5));
        let tag = self.builder.ins().iconst(layout.tag.value_type, variant.discriminant as i64);
        self.builder.ins().store(MemFlagsData::new(), tag, object, i32::try_from(layout.tag.offset).ok()?);
        Some(object)
    }

    fn emit_match_payload_branch(
        &mut self,
        key: AstNodeKey,
        object: Value,
        payload_layout: &[Option<FieldLayout>],
        pattern: MatchPayloadPatternFact,
        destinations: MatchDestinations,
        bindings: &mut Vec<(MatchArmBindingFact, Value)>,
    ) -> Option<()> {
        match pattern {
            MatchPayloadPatternFact::Fields(patterns) => {
                if patterns.len() != payload_layout.len() {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                }
                if patterns.is_empty() {
                    self.builder.ins().jump(destinations.success, &[]);
                    return Some(());
                }
                for (index, (pattern, layout)) in patterns.into_iter().zip(payload_layout).enumerate() {
                    let success = if index + 1 == payload_layout.len() {
                        destinations.success
                    } else {
                        self.builder.create_block()
                    };
                    self.emit_match_payload_branch(
                        key,
                        object,
                        std::slice::from_ref(layout),
                        pattern,
                        MatchDestinations { success, failure: destinations.failure },
                        bindings,
                    )?;
                    if success != destinations.success {
                        self.builder.switch_to_block(success);
                        self.builder.seal_block(success);
                    }
                }
            }
            MatchPayloadPatternFact::Ignore => {
                (payload_layout.len() == 1).then_some(())?;
                self.builder.ins().jump(destinations.success, &[]);
            }
            MatchPayloadPatternFact::Unit => {
                matches!(payload_layout, [None]).then_some(())?;
                self.builder.ins().jump(destinations.success, &[]);
            }
            MatchPayloadPatternFact::Binding(binding) => {
                let [Some(layout)] = payload_layout else {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                };
                if layout.value_type != binding.value_type || self.locals.contains_key(&binding.slot) {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                }
                let value = self.builder.ins().load(
                    layout.value_type,
                    MemFlagsData::new(),
                    object,
                    i32::try_from(layout.offset).ok()?,
                );
                bindings.push((binding, value));
                self.builder.ins().jump(destinations.success, &[]);
            }
            MatchPayloadPatternFact::ScalarLiteral { expression, value_type } => {
                let [Some(layout)] = payload_layout else {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                };
                if layout.value_type != value_type {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                }
                let actual = self.builder.ins().load(
                    value_type,
                    MemFlagsData::new(),
                    object,
                    i32::try_from(layout.offset).ok()?,
                );
                let expected = generated::constructor_lower_expression(self, expression)?;
                if self.builder.func.dfg.value_type(expected) != value_type {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                }
                let matches = if value_type.is_int() {
                    self.builder.ins().icmp(IntCC::Equal, actual, expected)
                } else if value_type.is_float() {
                    self.builder.ins().fcmp(FloatCC::Equal, actual, expected)
                } else {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                };
                self.builder.ins().brif(matches, destinations.success, &[], destinations.failure, &[]);
            }
            MatchPayloadPatternFact::Enum { layout, discriminant, payload } => {
                let [Some(parent_layout)] = payload_layout else {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                };
                if !parent_layout.value_type.is_int() || !layout.is_valid() {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                }
                let nested = self.builder.ins().load(
                    parent_layout.value_type,
                    MemFlagsData::new(),
                    object,
                    i32::try_from(parent_layout.offset).ok()?,
                );
                self.emit_enum_pattern_branch(
                    key,
                    MatchedEnum { object: nested, layout: &layout },
                    discriminant,
                    *payload,
                    destinations,
                    bindings,
                )?;
            }
        }
        Some(())
    }

    fn emit_enum_pattern_branch(
        &mut self,
        key: AstNodeKey,
        matched: MatchedEnum<'_>,
        discriminant: u64,
        payload: MatchPayloadPatternFact,
        destinations: MatchDestinations,
        bindings: &mut Vec<(MatchArmBindingFact, Value)>,
    ) -> Option<()> {
        let variant = matched.layout.variants.iter().find(|variant| variant.discriminant == discriminant)?;
        let tag = self.builder.ins().load(
            matched.layout.tag.value_type,
            MemFlagsData::new(),
            matched.object,
            i32::try_from(matched.layout.tag.offset).ok()?,
        );
        let expected = self.builder.ins().iconst(matched.layout.tag.value_type, discriminant as i64);
        let matches = self.builder.ins().icmp(IntCC::Equal, tag, expected);
        let payload_block = self.builder.create_block();
        self.builder.ins().brif(matches, payload_block, &[], destinations.failure, &[]);
        self.builder.switch_to_block(payload_block);
        self.builder.seal_block(payload_block);
        self.emit_match_payload_branch(key, matched.object, &variant.payload_fields, payload, destinations, bindings)
    }

    fn install_match_bindings(
        &mut self,
        key: AstNodeKey,
        bindings: &[(MatchArmBindingFact, Value)],
    ) -> Option<Vec<LocalSlotId>> {
        let mut installed = Vec::with_capacity(bindings.len());
        for (binding, value) in bindings {
            if self.locals.contains_key(&binding.slot) || self.builder.func.dfg.value_type(*value) != binding.value_type
            {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                return None;
            }
            self.bind_local(binding.slot, *value, binding.value_type, binding.managed_reference)?;
            installed.push(binding.slot);
        }
        Some(installed)
    }

    pub(super) fn emit_match_dispatch(&mut self, key: AstNodeKey, result_type: Option<Type>) -> Option<Option<Value>> {
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
        let layout_discriminants = layout.variants.iter().map(|variant| variant.discriminant).collect::<HashSet<_>>();
        if arms
            .iter()
            .filter_map(|arm| arm.discriminant)
            .any(|discriminant| !layout_discriminants.contains(&discriminant))
        {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
            return None;
        }
        let has_wildcard = arms.iter().any(|arm| arm.discriminant.is_none());
        let exhaustively_covers_variants = layout_discriminants.iter().all(|discriminant| {
            let payloads = arms
                .iter()
                .filter_map(|arm| (arm.discriminant == Some(*discriminant)).then_some(&arm.payload))
                .collect::<Vec<_>>();
            payload_patterns_are_exhaustive(&payloads)
        });
        if !has_wildcard && !exhaustively_covers_variants {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::NonExhaustiveMatch });
            return None;
        }

        let scrutinee_key = self.facts.child(key, 0)?;
        let scrutinee = generated::constructor_lower_expression(self, scrutinee_key)?;
        if !self.builder.func.dfg.value_type(scrutinee).is_int() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
            return None;
        }

        let merge = self.builder.create_block();
        if let Some(result_type) = result_type {
            self.builder.append_block_param(merge, result_type);
        }
        let test_blocks = (0..arms.len()).map(|_| self.builder.create_block()).collect::<Vec<_>>();
        let body_blocks = (0..arms.len()).map(|_| self.builder.create_block()).collect::<Vec<_>>();
        let failure = self.builder.create_block();
        self.builder.ins().jump(test_blocks[0], &[]);

        let mut merge_reachable = false;
        for (index, arm) in arms.into_iter().enumerate() {
            let next = test_blocks.get(index + 1).copied().unwrap_or(failure);
            let mut bindings = Vec::new();
            self.builder.switch_to_block(test_blocks[index]);
            self.builder.seal_block(test_blocks[index]);
            match arm.discriminant {
                Some(discriminant) => self.emit_enum_pattern_branch(
                    key,
                    MatchedEnum { object: scrutinee, layout: &layout },
                    discriminant,
                    arm.payload,
                    MatchDestinations { success: body_blocks[index], failure: next },
                    &mut bindings,
                )?,
                None if matches!(arm.payload, MatchPayloadPatternFact::Ignore) => {
                    self.builder.ins().jump(body_blocks[index], &[]);
                }
                None => {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                }
            }

            self.builder.switch_to_block(body_blocks[index]);
            self.builder.seal_block(body_blocks[index]);
            self.begin_local_root_scope();
            let _installed = self.install_match_bindings(key, &bindings)?;
            if let Some(result_type) = result_type {
                if self.facts.semantic_type(arm.body) == Some(beskid_queries::SemanticTypeId::NEVER) {
                    self.lower_expression_for_effect(arm.body)?;
                    self.end_local_root_scope_for_current_block()?;
                    continue;
                }
                let value = generated::constructor_lower_expression(self, arm.body)?;
                if self.builder.func.dfg.value_type(value) != result_type {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidMatchArms });
                    return None;
                }
                self.end_local_root_scope(true, None)?;
                self.builder.ins().jump(merge, &[value.into()]);
                merge_reachable = true;
            } else {
                self.lower_expression_for_effect(arm.body)?;
                self.end_local_root_scope_for_current_block()?;
                if jump_from_current_if_unterminated(self.builder, merge) {
                    merge_reachable = true;
                }
            }
        }

        self.builder.switch_to_block(failure);
        self.builder.seal_block(failure);
        self.builder.ins().trap(TrapCode::unwrap_user(1));
        self.builder.switch_to_block(merge);
        self.builder.seal_block(merge);
        if result_type.is_none() && !merge_reachable {
            self.builder.ins().trap(TrapCode::unwrap_user(1));
        }
        Some(result_type.and_then(|_| self.builder.block_params(merge).first().copied()))
    }

    pub(super) fn emit_enum_constructor_payloads(
        &mut self,
        key: AstNodeKey,
        object: Value,
        fields: &[Option<FieldLayout>],
        payloads: &[AstNodeKey],
    ) -> Option<()> {
        if fields.len() != payloads.len() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
            return None;
        }
        for (field, payload_key) in fields.iter().zip(payloads.iter().copied()) {
            if let Some(field) = field {
                let payload = self.lower_nested_expression(payload_key)?;
                if self.builder.func.dfg.value_type(payload) != field.value_type {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                    return None;
                }
                self.builder.ins().store(MemFlagsData::new(), payload, object, i32::try_from(field.offset).ok()?);
            } else if self.facts.semantic_type(payload_key) == Some(beskid_queries::SemanticTypeId::UNIT) {
                self.lower_expression_for_effect(payload_key)?;
            } else {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
                return None;
            }
        }
        Some(())
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
            let Some(variant) = usize::try_from(variant_index).ok().and_then(|index| layout.variants.get(index)) else {
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
            let object = self.allocate_enum_variant(&allocation, &layout, variant_index as usize)?;
            let root = self.root_expression_value(object)?;
            let payloads = self.facts.enum_payloads(key)?;
            self.emit_enum_constructor_payloads(key, object, &variant.payload_fields, &payloads)?;
            self.release_expression_root(Some(root))?;
            Some(object)
        }

        fn emit_try_expression(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_try_dispatch(key)?
        }

        fn emit_try_statement(&mut self, key: AstNodeKey) -> Option<()> {
            self.emit_try_dispatch(key).map(|_| ())
        }

        fn emit_match(&mut self, key: AstNodeKey) -> Option<Value> {
            let arms = self.facts.match_arms(key)?;
            let result_type =
                self.facts.scalar_type(key).or_else(|| arms.iter().find_map(|arm| self.facts.scalar_type(arm.body)))?;
            self.emit_match_dispatch(key, Some(result_type))?
        }

        fn emit_match_statement(&mut self, key: AstNodeKey) -> Option<()> {
            self.emit_match_dispatch(key, None).map(|_| ())
        }
    };
}

pub(super) use generated_enum_methods;
