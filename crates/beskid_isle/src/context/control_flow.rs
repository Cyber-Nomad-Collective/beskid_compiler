use super::*;
use crate::context::generated::Context as _;

impl IsleContext<'_, '_, '_, '_> {
    /// The zero value a `for` loop variable is bound to before its first element is assigned;
    /// `None` for a CLIF type no loop variable can have.
    pub(super) fn loop_variable_initial_value(&mut self, element_type: Type) -> Option<Value> {
        if element_type == cranelift_codegen::ir::types::F32 {
            Some(self.builder.ins().f32const(cranelift_codegen::ir::immediates::Ieee32::with_bits(0)))
        } else if element_type == cranelift_codegen::ir::types::F64 {
            Some(self.builder.ins().f64const(cranelift_codegen::ir::immediates::Ieee64::with_bits(0)))
        } else if element_type.is_int() {
            Some(self.builder.ins().iconst(element_type, 0))
        } else {
            None
        }
    }

    /// Adapt one already-lowered scalar to a declared storage, parameter, or return boundary.
    ///
    /// Source typing has already authorized the assignment. This seam only reconciles the
    /// physical integer widths used by CLIF; pointer/float/vector mismatches remain unavailable.
    pub(super) fn adapt_scalar_boundary(&mut self, source: AstNodeKey, value: Value, expected: Type) -> Option<Value> {
        let actual = self.builder.func.dfg.value_type(value);
        let semantic_source = if self.facts.node_kind(source) == Some(NodeKind::BlockExpression) {
            self.facts.block_result(source)?
        } else {
            source
        };
        if actual == expected {
            Some(value)
        } else if actual.is_int() && expected.is_int() && actual.bits() < expected.bits() {
            match self.facts.semantic_type(semantic_source)? {
                beskid_queries::SemanticTypeId::U16
                | beskid_queries::SemanticTypeId::U64
                | beskid_queries::SemanticTypeId::U32
                | beskid_queries::SemanticTypeId::U8
                | beskid_queries::SemanticTypeId::BOOL => Some(self.builder.ins().uextend(expected, value)),
                beskid_queries::SemanticTypeId::I8
                | beskid_queries::SemanticTypeId::I16
                | beskid_queries::SemanticTypeId::I32
                | beskid_queries::SemanticTypeId::I64 => Some(self.builder.ins().sextend(expected, value)),
                _ => None,
            }
        } else if actual.is_int() && expected.is_int() && actual.bits() > expected.bits() {
            matches!(
                self.facts.semantic_type(semantic_source)?,
                beskid_queries::SemanticTypeId::I8
                    | beskid_queries::SemanticTypeId::I16
                    | beskid_queries::SemanticTypeId::U16
                    | beskid_queries::SemanticTypeId::U64
                    | beskid_queries::SemanticTypeId::U8
                    | beskid_queries::SemanticTypeId::U32
                    | beskid_queries::SemanticTypeId::BOOL
                    | beskid_queries::SemanticTypeId::I32
                    | beskid_queries::SemanticTypeId::I64
            )
            .then(|| self.builder.ins().ireduce(expected, value))
        } else if actual.is_float() && expected.is_float() {
            Some(if actual.bits() < expected.bits() {
                self.builder.ins().fpromote(expected, value)
            } else {
                self.builder.ins().fdemote(expected, value)
            })
        } else {
            None
        }
    }

    /// Lower one expression reached from an enclosing statement while retaining
    /// its key when generated ISLE has neither a matching rule nor all required
    /// facts.  This is diagnostic-only: success and existing semantic errors are
    /// passed through unchanged.
    pub(super) fn lower_nested_expression(&mut self, key: AstNodeKey) -> Option<Value> {
        generated::constructor_lower_expression(self, key).or_else(|| {
            if self.pending_error.is_none() {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::MissingRuleOrFact });
            }
            None
        })
    }

    /// Lower an expression whose value is intentionally discarded while preserving its effects.
    ///
    /// This is the single effect-only expression path used by both ordinary expression statements
    /// and zero-sized enum payloads. Statement-capable expressions go through generated statement
    /// lowering first; value-producing expressions go through generated expression lowering and
    /// are discarded. ABI-v5 unit literals and unit paths have no physical value to materialize,
    /// while unsupported effectful unit shapes fail at their exact source key.
    pub(super) fn lower_expression_for_effect(&mut self, key: AstNodeKey) -> Option<()> {
        let kind = self.facts.node_kind(key)?;
        if kind == NodeKind::BlockExpression {
            self.begin_local_root_scope();
            let lowered = (|| {
                let prefix_count = self.facts.statement_count(key)?;
                let statement_count = prefix_count.checked_add(u8::from(self.facts.block_result(key).is_some()))?;
                for index in 0..statement_count {
                    let current = self.builder.current_block()?;
                    if block_is_terminated(self.builder, current) {
                        break;
                    }
                    let statement = self.facts.child(key, index)?;
                    self.lower_nested_statement(statement)?;
                }
                Some(())
            })();
            let scope_end = self.end_local_root_scope_for_current_block();
            lowered?;
            scope_end?;
            return Some(());
        }

        if kind == NodeKind::ClifBlock {
            return clif_block::lower_clif_block_for_effect(self, key);
        }
        if self.facts.semantic_type(key) == Some(beskid_queries::SemanticTypeId::UNIT)
            && matches!(kind, NodeKind::FieldExpression | NodeKind::AssignExpression)
        {
            let field = if kind == NodeKind::FieldExpression { key } else { self.facts.child(key, 0)? };
            let layout = self.facts.struct_layout(field)?;
            let index = usize::try_from(self.facts.field_index(field)?).ok()?;
            if !layout.is_valid() || !matches!(layout.fields.get(index), Some(None)) {
                return None;
            }
            // The receiver and RHS may have effects even though unit has no storage.
            self.field_base_pointer(field)?;
            if kind == NodeKind::AssignExpression {
                let rhs = self.facts.child(key, 1)?;
                if self.facts.semantic_type(rhs) != Some(beskid_queries::SemanticTypeId::UNIT) {
                    return None;
                }
                self.lower_expression_for_effect(rhs)?;
            }
            return Some(());
        }

        if generated::constructor_lower_statement(self, key).is_some() {
            return Some(());
        }
        if self.pending_error.is_some() {
            return None;
        }

        if self.facts.semantic_type(key) == Some(beskid_queries::SemanticTypeId::UNIT) {
            match kind {
                NodeKind::LiteralExpression | NodeKind::PathExpression => return Some(()),
                NodeKind::GroupedExpression => {
                    let expression = self.facts.child(key, 0)?;
                    return self.lower_expression_for_effect(expression);
                }
                _ => {}
            }
        }

        let value = self.lower_nested_expression(key)?;
        self.discard_value(value);
        Some(())
    }
}

macro_rules! generated_control_flow_methods {
    () => {
        fn emit_expression_statement(&mut self, key: AstNodeKey) -> Option<()> {
            let expression = self.facts.child(key, 0)?;
            self.lower_expression_for_effect(expression)
        }
        fn discard_value(&mut self, _value: Value) {}

        fn emit_method_body(&mut self, key: AstNodeKey) -> Option<()> {
            materialize_parameters(self, key).ok()?;
            let body_key = self.facts.child(key, 0)?;
            let cursor = self.statement_cursor(body_key)?;
            generated::constructor_lower_statement_cursor(self, cursor)?;
            Some(())
        }

        fn emit_block_expression(&mut self, key: AstNodeKey) -> Option<Value> {
            self.begin_local_root_scope();
            let lowered = (|| {
                let count = self.facts.statement_count(key)?;
                for index in 0..count {
                    let statement = self.facts.child(key, index)?;
                    generated::constructor_lower_statement(self, statement)?;
                    let current = self.builder.current_block()?;
                    if block_is_terminated(self.builder, current) {
                        self.pending_error =
                            Some(LoweringError { key, kind: LoweringErrorKind::InvalidBlockExpression });
                        return None;
                    }
                }
                let result_key = self.facts.block_result(key)?;
                let value = self.lower_nested_expression(result_key)?;
                let expected = self.facts.scalar_type(key).or_else(|| self.facts.scalar_type(result_key))?;
                let Some(value) = self.adapt_scalar_boundary(result_key, value, expected) else {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidBlockExpression });
                    return None;
                };
                let root = if self.facts.managed_reference(result_key)? == ManagedReferenceFact::GcManaged {
                    Some(self.root_temporary(value)?)
                } else {
                    None
                };
                self.end_local_root_scope(true, root)?;
                self.release_temporary_root(root)?;
                Some(value)
            })();
            lowered
        }

        fn emit_clif_block(&mut self, key: AstNodeKey) -> Option<Value> {
            clif_block::lower_clif_block(self, key)
        }

        fn emit_return(&mut self, key: AstNodeKey) -> Option<()> {
            // A `unit` value (`return value;` of a `unit` binding, `return ();`) has no storage:
            // it is evaluated for its effects and the function returns nothing.
            if let Some(value_key) = self.facts.child(key, 0)
                && self.builder.func.signature.returns.is_empty()
                && self.facts.semantic_type(value_key) == Some(beskid_queries::SemanticTypeId::UNIT)
            {
                self.lower_expression_for_effect(value_key)?;
                self.emit_composition_cleanup_from(0)?;
                self.release_managed_local_roots()?;
                self.builder.ins().return_(&[]);
                return Some(());
            }
            if let Some(value_key) = self.facts.child(key, 0) {
                let value = self.lower_nested_expression(value_key)?;
                let expected = self.builder.func.signature.returns.first()?.value_type;
                let value = self.adapt_scalar_boundary(value_key, value, expected)?;
                self.return_with_cleanup(value)?;
            } else {
                self.emit_composition_cleanup_from(0)?;
                self.release_managed_local_roots()?;
                self.builder.ins().return_(&[]);
            }
            Some(())
        }

        fn emit_local_let(&mut self, key: AstNodeKey) -> Option<()> {
            let slot = self.facts.local_slot(key)?;
            if self.locals.contains_key(&slot) {
                return None;
            }
            let initializer = self.facts.let_initializer(key)?;
            if self.facts.node_kind(initializer) == Some(NodeKind::LambdaExpression) {
                if let Some(plan) = self.facts.event_handler_local(key) {
                    self.emit_event_handler_local(key, plan)?;
                    return Some(());
                }
                if !self.facts.lambda_value_required(initializer)? {
                    // An immutable lambda local used only as a callee stays a syntax-level
                    // binding: each call lowers its source lambda inline at the call site.
                    self.facts.lambda_entry(initializer)?;
                    return Some(());
                }
                let record = generated::constructor_lower_expression(self, initializer)?;
                let pointer = dispatch::pointer_type(self.frontend_config);
                return self.bind_local(slot, record, pointer, ManagedReferenceFact::GcManaged);
            }
            let value_type = self.facts.scalar_type(key)?;
            let managed_reference = self.local_managed_reference(key, value_type)?;
            let admitted = self.prepare_checked_destination(value_type, managed_reference)?;
            let value = self.lower_nested_expression(initializer)?;
            let value = self.adapt_scalar_boundary(initializer, value, value_type)?;
            self.bind_local_in_destination(slot, value, value_type, managed_reference, admitted)
        }

        fn emit_scoped_use(&mut self, key: AstNodeKey) -> Option<()> {
            let plan = self.facts.scoped_cleanup(key)?;
            let body = plan.body;
            if body.is_some() {
                self.begin_local_root_scope();
            }
            self.emit_local_let(plan.binding)?;
            self.local_root_scopes.last_mut()?.cleanups.push(plan);
            if let Some(body) = body {
                self.lower_nested_statement(body)?;
                self.end_local_root_scope_for_current_block()?;
            }
            Some(())
        }

        fn emit_if_else(&mut self, key: AstNodeKey) -> Option<()> {
            let condition_key = self.facts.child(key, 0)?;
            let then_key = self.facts.child(key, 1)?;
            let else_key = self.facts.child(key, 2);
            let condition = self.lower_nested_expression(condition_key)?;
            let then_block = self.builder.create_block();
            let else_block = self.builder.create_block();
            let merge_block = self.builder.create_block();
            self.builder.ins().brif(condition, then_block, &[], else_block, &[]);

            // Track whether any arm jumps into the merge. Both arms often terminate with a
            // jump to merge (ordinary fallthrough); that must not be treated as unreachable.
            // Only when every arm returns/traps without reaching merge is the merge dead,
            // and only then may we plant a trap. Planting a trap on a reachable merge used
            // to swallow later statements (e.g. `return 0` after a bare `if`) as UDF/SIGILL
            // on macOS arm64 exact-kit smokes (CYB-129).
            //
            // Arm bodies may themselves contain nested control flow and leave the builder on
            // a descendant block. Always terminate the *current* block before switching arms.
            let mut merge_reachable = false;

            self.builder.switch_to_block(then_block);
            self.builder.seal_block(then_block);
            self.begin_local_root_scope();
            generated::constructor_lower_statement(self, then_key)?;
            self.end_local_root_scope_for_current_block()?;
            if jump_from_current_if_unterminated(self.builder, merge_block) {
                merge_reachable = true;
            }

            self.builder.switch_to_block(else_block);
            self.builder.seal_block(else_block);
            self.begin_local_root_scope();
            if let Some(else_key) = else_key {
                generated::constructor_lower_statement(self, else_key)?;
            }
            self.end_local_root_scope_for_current_block()?;
            if jump_from_current_if_unterminated(self.builder, merge_block) {
                merge_reachable = true;
            }
            self.builder.switch_to_block(merge_block);
            self.builder.seal_block(merge_block);
            if !merge_reachable {
                self.builder.ins().trap(TrapCode::unwrap_user(1));
            }
            Some(())
        }

        fn emit_while(&mut self, key: AstNodeKey) -> Option<()> {
            let condition_key = self.facts.child(key, 0)?;
            let body_key = self.facts.child(key, 1)?;
            let header = self.builder.create_block();
            let body = self.builder.create_block();
            let exit = self.builder.create_block();
            self.builder.ins().jump(header, &[]);

            self.builder.switch_to_block(header);
            let condition = generated::constructor_lower_expression(self, condition_key)?;
            self.builder.ins().brif(condition, body, &[], exit, &[]);

            self.builder.switch_to_block(body);
            self.builder.seal_block(body);
            let root_scope_depth = self.local_root_scope_depth();
            self.begin_local_root_scope();
            self.loop_stack.push(LoopTargets {
                continue_block: header,
                break_block: exit,
                root_scope_depth,
                break_reached: false,
            });
            let lowered = generated::constructor_lower_statement(self, body_key);
            let break_reached = self.loop_stack.pop().is_some_and(|targets| targets.break_reached);
            lowered?;
            self.end_local_root_scope_for_current_block()?;
            // Body may nest `if`/`for` and leave the builder on a descendant block.
            let _ = jump_from_current_if_unterminated(self.builder, header);

            self.builder.seal_block(header);
            self.builder.switch_to_block(exit);
            self.builder.seal_block(exit);
            // A statically-true condition with no reachable `break` makes `exit` dead: every
            // real execution either loops forever or escapes through a `return` inside the
            // body. `brif` still gives `exit` a predecessor edge (the header's false arm), so
            // CLIF requires it to end with a terminator. Plant the same "unreachable" trap
            // `emit_if_else` uses for its own dead-merge case (CYB-129) instead of leaving
            // `exit` unterminated for the caller.
            if !break_reached && self.facts.boolean_literal(condition_key) == Some(true) {
                self.builder.ins().trap(TrapCode::unwrap_user(1));
            }
            Some(())
        }

        fn emit_range_for(&mut self, key: AstNodeKey) -> Option<()> {
            let iterable = self.facts.child(key, 0)?;
            let body_key = self.facts.child(key, 1)?;
            let range = self.facts.range_fact(iterable)?;
            let iterator_type = self.facts.scalar_type(key)?;
            let slot = self.facts.local_slot(key)?;
            if range.step == 0 || !iterator_type.is_int() || self.locals.contains_key(&slot) {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidRangeFor });
                return None;
            }
            let start = generated::constructor_lower_expression(self, range.start)?;
            let end = generated::constructor_lower_expression(self, range.end)?;
            if self.builder.func.dfg.value_type(start) != iterator_type
                || self.builder.func.dfg.value_type(end) != iterator_type
            {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidRangeFor });
                return None;
            }
            self.bind_local(slot, start, iterator_type, ManagedReferenceFact::NativeOrScalar)?;
            let iterator = self.locals.get(&slot)?.variable;

            let header = self.builder.create_block();
            let body = self.builder.create_block();
            let latch = self.builder.create_block();
            let exit = self.builder.create_block();
            self.builder.ins().jump(header, &[]);

            self.builder.switch_to_block(header);
            let current = self.builder.use_var(iterator);
            let condition = match (range.step.is_positive(), range.inclusive) {
                (true, false) => self.builder.ins().icmp(IntCC::SignedLessThan, current, end),
                (true, true) => self.builder.ins().icmp(IntCC::SignedLessThanOrEqual, current, end),
                (false, false) => self.builder.ins().icmp(IntCC::SignedGreaterThan, current, end),
                (false, true) => self.builder.ins().icmp(IntCC::SignedGreaterThanOrEqual, current, end),
            };
            self.builder.ins().brif(condition, body, &[], exit, &[]);

            self.builder.switch_to_block(body);
            self.builder.seal_block(body);
            let root_scope_depth = self.local_root_scope_depth();
            self.begin_local_root_scope();
            self.loop_stack.push(LoopTargets {
                continue_block: latch,
                break_block: exit,
                root_scope_depth,
                break_reached: false,
            });
            let lowered = generated::constructor_lower_statement(self, body_key);
            self.loop_stack.pop();
            if lowered.is_none() {
                let _ = self.end_local_root_scope_for_current_block();
                self.locals.remove(&slot);
                return None;
            }
            self.end_local_root_scope_for_current_block()?;
            // Body may nest control flow and leave the builder on a descendant block.
            let _ = jump_from_current_if_unterminated(self.builder, latch);

            self.builder.switch_to_block(latch);
            self.builder.seal_block(latch);
            let current = self.builder.use_var(iterator);
            let next = self.builder.ins().iadd_imm_s(current, range.step);
            self.builder.def_var(iterator, next);
            self.builder.ins().jump(header, &[]);

            self.builder.seal_block(header);
            self.builder.switch_to_block(exit);
            self.builder.seal_block(exit);
            self.locals.remove(&slot);
            Some(())
        }

        /// `for id in array { body }`: the array value is evaluated once, checked non-null and
        /// rooted for the whole loop; its length is read once from the header; every iteration
        /// reloads the data pointer (the header is the rooted authority), loads the element at the
        /// layout stride and assigns it to the loop variable, a scalar or a GC-rooted pointer local.
        /// The loop scope owns the array root and the loop variable; the body runs in a nested
        /// scope, so `break` and `continue` release only the body's roots, `return` releases all.
        fn emit_array_for(&mut self, key: AstNodeKey) -> Option<()> {
            let iterable = self.facts.child(key, 0)?;
            let body_key = self.facts.child(key, 1)?;
            let fact = self.facts.array_for_fact(key)?;
            let element_type = self.facts.scalar_type(key)?;
            let slot = self.facts.local_slot(key)?;
            let pointer = dispatch::pointer_type(self.frontend_config);
            if !fact.layout.is_valid()
                || fact.layout.element_type != element_type
                || (fact.element_managed == ManagedReferenceFact::GcManaged && element_type != pointer)
                || self.locals.contains_key(&slot)
            {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayFor });
                return None;
            }
            let array = generated::constructor_lower_expression(self, iterable)?;
            if self.builder.func.dfg.value_type(array) != pointer {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayFor });
                return None;
            }
            self.builder.ins().trapz(array, TrapCode::unwrap_user(1));

            self.begin_local_root_scope();
            let _array_root = self.root_expression_value_if_needed(iterable, array)?;
            let length = self.builder.ins().load(
                pointer,
                MemFlagsData::new(),
                array,
                i32::try_from(pointer.bytes()).ok()?,
            );
            let index = self.builder.declare_var(pointer);
            let zero = self.builder.ins().iconst(pointer, 0);
            self.builder.def_var(index, zero);
            let Some(initial) = self.loop_variable_initial_value(element_type) else {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidArrayFor });
                return None;
            };
            self.bind_local(slot, initial, element_type, fact.element_managed)?;

            let header = self.builder.create_block();
            let body = self.builder.create_block();
            let latch = self.builder.create_block();
            let exit = self.builder.create_block();
            self.builder.ins().jump(header, &[]);

            self.builder.switch_to_block(header);
            let current = self.builder.use_var(index);
            let in_bounds = self.builder.ins().icmp(IntCC::UnsignedLessThan, current, length);
            self.builder.ins().brif(in_bounds, body, &[], exit, &[]);

            self.builder.switch_to_block(body);
            self.builder.seal_block(body);
            let data = self.builder.ins().load(pointer, MemFlagsData::new(), array, 0);
            let offset = if fact.layout.stride == 1 {
                current
            } else {
                self.builder.ins().imul_imm_s(current, i64::from(fact.layout.stride))
            };
            let address = self.builder.ins().iadd(data, offset);
            let element = self.builder.ins().load(element_type, MemFlagsData::new(), address, 0);
            self.assign_local(slot, element)?;
            let body_scope_depth = self.local_root_scope_depth();
            self.begin_local_root_scope();
            self.loop_stack.push(LoopTargets {
                continue_block: latch,
                break_block: exit,
                root_scope_depth: body_scope_depth,
                break_reached: false,
            });
            let lowered = generated::constructor_lower_statement(self, body_key);
            self.loop_stack.pop();
            if lowered.is_none() {
                let _ = self.end_local_root_scope_for_current_block();
                let _ = self.end_local_root_scope_for_current_block();
                return None;
            }
            self.end_local_root_scope_for_current_block()?;
            // Body may nest control flow and leave the builder on a descendant block.
            let _ = jump_from_current_if_unterminated(self.builder, latch);

            self.builder.switch_to_block(latch);
            self.builder.seal_block(latch);
            let current = self.builder.use_var(index);
            let next = self.builder.ins().iadd_imm_s(current, 1);
            self.builder.def_var(index, next);
            self.builder.ins().jump(header, &[]);

            self.builder.seal_block(header);
            self.builder.switch_to_block(exit);
            self.builder.seal_block(exit);
            // Leaving the loop scope releases the array root and the loop variable on the normal
            // and the `break` edges alike.
            self.end_local_root_scope_for_current_block()?;
            Some(())
        }

        /// `for id in iterator { body }` over the nominal iterator protocol (`Next()` returning
        /// `Option<T>`): the iterable is evaluated once, checked non-null and rooted for the whole
        /// loop. The header, which is also the `continue` target, calls `Next()` on the iterator
        /// and reads the tag of the returned `Option<T>`: `None` leaves the loop, `Some` loads its
        /// payload into the loop variable, a scalar or a GC-rooted pointer local. As in
        /// `emit_array_for`, the loop scope owns the iterator root and the loop variable and the
        /// body runs in a nested scope, so `break` and `continue` release only the body's roots.
        fn emit_iterator_for(&mut self, key: AstNodeKey) -> Option<()> {
            let iterable = self.facts.child(key, 0)?;
            let body_key = self.facts.child(key, 1)?;
            let fact = self.facts.iterator_for_fact(key)?;
            let element_type = self.facts.scalar_type(key)?;
            let slot = self.facts.local_slot(key)?;
            let pointer = dispatch::pointer_type(self.frontend_config);
            let some = usize::try_from(fact.some_variant).ok().and_then(|index| fact.option_layout.variants.get(index));
            let payload = some.and_then(|variant| match variant.payload_fields.as_slice() {
                [Some(field)] => Some(*field),
                _ => None,
            });
            let signature_is_receiver_to_pointer = matches!(
                (fact.next_signature.params.as_slice(), fact.next_signature.returns.as_slice()),
                ([receiver], [result]) if receiver.value_type == pointer && result.value_type == pointer
            );
            let (Some(some), Some(payload)) = (some, payload) else {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidIteratorFor });
                return None;
            };
            let some_discriminant = i64::try_from(some.discriminant).ok();
            if !fact.option_layout.is_valid()
                || !signature_is_receiver_to_pointer
                || payload.value_type != element_type
                || some_discriminant.is_none()
                || (fact.element_managed == ManagedReferenceFact::GcManaged && element_type != pointer)
                || self.locals.contains_key(&slot)
            {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidIteratorFor });
                return None;
            }
            let some_discriminant = some_discriminant?;
            let tag = fact.option_layout.tag;
            let next = match self.call_importer.as_deref_mut()?.import(self.builder, fact.next.clone(), &fact.next_signature)
            {
                Ok(function) => function,
                Err(CallImportError::UnknownCallee) => {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::UnknownCallee(fact.next) });
                    return None;
                }
            };
            let iterator = generated::constructor_lower_expression(self, iterable)?;
            if self.builder.func.dfg.value_type(iterator) != pointer {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidIteratorFor });
                return None;
            }
            self.builder.ins().trapz(iterator, TrapCode::unwrap_user(1));

            self.begin_local_root_scope();
            let _iterator_root = self.root_expression_value_if_needed(iterable, iterator)?;
            let Some(initial) = self.loop_variable_initial_value(element_type) else {
                self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidIteratorFor });
                return None;
            };
            self.bind_local(slot, initial, element_type, fact.element_managed)?;

            let header = self.builder.create_block();
            let body = self.builder.create_block();
            let exit = self.builder.create_block();
            self.builder.ins().jump(header, &[]);

            self.builder.switch_to_block(header);
            let call = self.builder.ins().call(next, &[iterator]);
            self.guard_checked_allocation()?;
            let option = self.builder.inst_results(call).first().copied()?;
            let tag_value =
                self.builder.ins().load(tag.value_type, MemFlagsData::new(), option, i32::try_from(tag.offset).ok()?);
            let is_some = self.builder.ins().icmp_imm_s(IntCC::Equal, tag_value, some_discriminant);
            self.builder.ins().brif(is_some, body, &[], exit, &[]);

            self.builder.switch_to_block(body);
            self.builder.seal_block(body);
            // No call runs between `Next()` returning and this load, so the `Option` object needs
            // no root of its own; the loaded element is published into the loop variable's root.
            let element =
                self.builder.ins().load(element_type, MemFlagsData::new(), option, i32::try_from(payload.offset).ok()?);
            self.assign_local(slot, element)?;
            let body_scope_depth = self.local_root_scope_depth();
            self.begin_local_root_scope();
            self.loop_stack.push(LoopTargets {
                continue_block: header,
                break_block: exit,
                root_scope_depth: body_scope_depth,
                break_reached: false,
            });
            let lowered = generated::constructor_lower_statement(self, body_key);
            self.loop_stack.pop();
            if lowered.is_none() {
                let _ = self.end_local_root_scope_for_current_block();
                let _ = self.end_local_root_scope_for_current_block();
                return None;
            }
            self.end_local_root_scope_for_current_block()?;
            // Body may nest control flow and leave the builder on a descendant block.
            let _ = jump_from_current_if_unterminated(self.builder, header);

            self.builder.seal_block(header);
            self.builder.switch_to_block(exit);
            self.builder.seal_block(exit);
            // Leaving the loop scope releases the iterator root and the loop variable on the
            // normal (`None`) and the `break` edges alike.
            self.end_local_root_scope_for_current_block()?;
            Some(())
        }

        fn emit_break(&mut self, _key: AstNodeKey) -> Option<()> {
            let targets = *self.loop_stack.last()?;
            self.cleanup_scope_exit(targets.root_scope_depth, None)?;
            self.release_local_roots_from(targets.root_scope_depth)?;
            let target = targets.break_block;
            self.builder.ins().jump(target, &[]);
            self.loop_stack.last_mut()?.break_reached = true;
            Some(())
        }

        fn emit_continue(&mut self, _key: AstNodeKey) -> Option<()> {
            let targets = *self.loop_stack.last()?;
            self.cleanup_scope_exit(targets.root_scope_depth, None)?;
            self.release_local_roots_from(targets.root_scope_depth)?;
            let target = targets.continue_block;
            self.builder.ins().jump(target, &[]);
            Some(())
        }

        fn statement_cursor(&mut self, key: AstNodeKey) -> Option<StatementCursor> {
            self.facts.statement_count(key)?;
            self.begin_local_root_scope();
            Some(StatementCursor { block: key, index: 0 })
        }

        /// Lower one statement reached through a generated cursor while preserving the
        /// leaf key when no generated rule or fact matches.  Generated ISLE partial
        /// rules otherwise return `None` through the cursor and the public entrypoint
        /// can only report the enclosing block.
        fn lower_nested_statement(&mut self, key: AstNodeKey) -> Option<()> {
            generated::constructor_lower_statement(self, key).or_else(|| {
                if self.pending_error.is_none() {
                    self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::MissingRuleOrFact });
                }
                None
            })
        }

        fn cursor_kind(&mut self, cursor: StatementCursor) -> Option<CursorKind> {
            let current = self.builder.current_block()?;
            if block_is_terminated(self.builder, current) {
                return Some(CursorKind::End);
            }
            let count = self.facts.statement_count(cursor.block)?;
            Some(if cursor.index < count { CursorKind::More } else { CursorKind::End })
        }

        fn cursor_head(&mut self, cursor: StatementCursor) -> Option<AstNodeKey> {
            self.facts.child(cursor.block, cursor.index)
        }

        fn cursor_tail(&mut self, cursor: StatementCursor) -> StatementCursor {
            StatementCursor { block: cursor.block, index: cursor.index.saturating_add(1) }
        }

        fn finish_statements(&mut self) -> Option<()> {
            self.end_local_root_scope_for_current_block()
        }

        fn sequence_statements(&mut self, _head: (), _tail: ()) {}

        fn emit_local_read(&mut self, key: AstNodeKey) -> Option<Value> {
            if let Some(value) = self.facts.constant_integer(key) {
                let value_type = self.facts.scalar_type(key)?;
                return Some(self.builder.ins().iconst(value_type, value));
            }
            // A named function read as a value is a closure record with the item's own entry
            // and a null environment, allocated where the path is evaluated.
            if let Some(value) = self.facts.function_item_value(key) {
                return self.emit_closure_record(key, &value.callee, None, &value.parameters, value.result);
            }
            let slot = self.facts.local_slot(key)?;
            let binding = self.locals.get(&slot).copied()?;
            Some(self.builder.use_var(binding.variable))
        }

        fn emit_local_assign(&mut self, key: AstNodeKey) -> Option<Value> {
            let target = self.facts.child(key, 0)?;
            let value_key = self.facts.child(key, 1)?;
            let slot = self.facts.mutable_local_assignment_slot(key)?;
            if self.facts.local_slot(target)? != slot {
                return None;
            }
            let binding = self.locals.get(&slot).copied()?;
            let value = self.lower_nested_expression(value_key)?;
            let actual_type = self.builder.func.dfg.value_type(value);
            if actual_type != binding.value_type {
                return None;
            }
            self.assign_local(slot, value)?;
            Some(value)
        }
    };
}

pub(super) use generated_control_flow_methods;
