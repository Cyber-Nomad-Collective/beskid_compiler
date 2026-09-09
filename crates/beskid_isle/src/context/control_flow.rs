use super::*;
use crate::context::generated::Context as _;

impl IsleContext<'_, '_, '_, '_> {
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
                beskid_queries::SemanticTypeId::U32
                | beskid_queries::SemanticTypeId::U8
                | beskid_queries::SemanticTypeId::BOOL => Some(self.builder.ins().uextend(expected, value)),
                beskid_queries::SemanticTypeId::I32 | beskid_queries::SemanticTypeId::I64 => {
                    Some(self.builder.ins().sextend(expected, value))
                }
                _ => None,
            }
        } else if actual.is_int() && expected.is_int() && actual.bits() > expected.bits() {
            matches!(
                self.facts.semantic_type(semantic_source)?,
                beskid_queries::SemanticTypeId::U8
                    | beskid_queries::SemanticTypeId::U32
                    | beskid_queries::SemanticTypeId::BOOL
                    | beskid_queries::SemanticTypeId::I32
                    | beskid_queries::SemanticTypeId::I64
            )
            .then(|| self.builder.ins().ireduce(expected, value))
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
            let saved_locals = self.locals.clone();
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
                Some(value)
            })();
            self.locals = saved_locals;
            lowered
        }

        fn emit_clif_block(&mut self, key: AstNodeKey) -> Option<Value> {
            let body = self.facts.clif_block_body(key)?;
            let result_type = self.facts.scalar_type(key).unwrap_or_else(|| {
                self.builder.func.signature.returns.first().map(|r| r.value_type).unwrap_or(types::I64)
            });

            let mut result: Option<Value> = None;

            for line in body.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                if let Some(rest) = line.strip_prefix("return") {
                    let rest = rest.trim();
                    if let Some(param_ref) = rest.strip_prefix('%')
                        && let Ok(index) = param_ref.trim().parse::<usize>()
                    {
                        result = self.function_param_values.get(index).copied();
                    }
                } else if let Some(rest) = line.strip_prefix("call") {
                    let rest = rest.trim();
                    if let Some(symbol_part) = rest.strip_prefix('@') {
                        let symbol_end =
                            symbol_part.find(|c: char| c.is_whitespace() || c == '(').unwrap_or(symbol_part.len());
                        let symbol = &symbol_part[..symbol_end];
                        let args_str = symbol_part[symbol_end..].trim();
                        let args_str = args_str.strip_prefix('(').unwrap_or(args_str);
                        let args_str = args_str.strip_suffix(')').unwrap_or(args_str);

                        let mut args = Vec::new();
                        for arg in args_str.split(',') {
                            let arg = arg.trim();
                            if let Some(num) = arg.strip_prefix('%')
                                && let Ok(index) = num.trim().parse::<usize>()
                                && let Some(value) = self.function_param_values.get(index).copied()
                            {
                                args.push(value);
                            }
                        }

                        let mut signature = Signature::new(self.builder.func.signature.call_conv);
                        for arg in &args {
                            signature.params.push(AbiParam::new(self.builder.func.dfg.value_type(*arg)));
                        }
                        signature.returns.push(AbiParam::new(result_type));

                        let sig_ref = self.builder.func.import_signature(signature);
                        let func_ref = self.builder.func.import_function(cranelift_codegen::ir::ExtFuncData {
                            name: ExternalName::testcase(symbol),
                            signature: sig_ref,
                            colocated: false,
                            patchable: false,
                        });

                        let call = self.builder.ins().call(func_ref, &args);
                        result = self.builder.inst_results(call).first().copied();
                    }
                }
            }

            result
        }

        fn emit_return(&mut self, key: AstNodeKey) -> Option<()> {
            if let Some(value_key) = self.facts.child(key, 0) {
                let value = self.lower_nested_expression(value_key)?;
                let expected = self.builder.func.signature.returns.first()?.value_type;
                let value = self.adapt_scalar_boundary(value_key, value, expected)?;
                self.release_managed_local_roots()?;
                self.builder.ins().return_(&[value]);
            } else {
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
            if self.facts.node_kind(initializer) == Some(NodeKind::LambdaExpression)
                && self.facts.lambda_entry(initializer)?.closure_environment.is_none()
            {
                return Some(());
            }
            let value = self.lower_nested_expression(initializer)?;
            let value_type = self.facts.scalar_type(key)?;
            let value = self.adapt_scalar_boundary(initializer, value, value_type)?;
            self.bind_local(slot, value, value_type, self.local_managed_reference(key, value_type)?)
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
            self.loop_stack.push(LoopTargets { continue_block: header, break_block: exit, root_scope_depth });
            let lowered = generated::constructor_lower_statement(self, body_key);
            self.loop_stack.pop();
            lowered?;
            self.end_local_root_scope_for_current_block()?;
            // Body may nest `if`/`for` and leave the builder on a descendant block.
            let _ = jump_from_current_if_unterminated(self.builder, header);

            self.builder.seal_block(header);
            self.builder.switch_to_block(exit);
            self.builder.seal_block(exit);
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
            self.loop_stack.push(LoopTargets { continue_block: latch, break_block: exit, root_scope_depth });
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
            let next = self.builder.ins().iadd_imm(current, range.step);
            self.builder.def_var(iterator, next);
            self.builder.ins().jump(header, &[]);

            self.builder.seal_block(header);
            self.builder.switch_to_block(exit);
            self.builder.seal_block(exit);
            self.locals.remove(&slot);
            Some(())
        }

        fn emit_iterator_for(&mut self, key: AstNodeKey) -> Option<()> {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidRangeFor });
            None
        }

        fn emit_break(&mut self, _key: AstNodeKey) -> Option<()> {
            let targets = *self.loop_stack.last()?;
            self.release_local_roots_from(targets.root_scope_depth)?;
            let target = targets.break_block;
            self.builder.ins().jump(target, &[]);
            Some(())
        }

        fn emit_continue(&mut self, _key: AstNodeKey) -> Option<()> {
            let targets = *self.loop_stack.last()?;
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
