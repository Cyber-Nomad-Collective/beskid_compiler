use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn coerce_expression_to_string(&mut self, key: AstNodeKey) -> Option<Value> {
        use beskid_queries::SemanticTypeId;

        let value = generated::constructor_lower_expression(self, key)?;
        let semantic = self.facts.semantic_type(key)?;
        if semantic == SemanticTypeId::STRING {
            return Some(value);
        }
        if semantic == SemanticTypeId::BOOL {
            return self.bool_to_string(key, value);
        }
        if matches!(semantic, SemanticTypeId::F32 | SemanticTypeId::F64) {
            return self.float_to_string(key, value);
        }
        let coerced = match semantic {
            SemanticTypeId::I64 => value,
            SemanticTypeId::I8 | SemanticTypeId::I16 => self.builder.ins().sextend(types::I64, value),
            SemanticTypeId::U16 => self.builder.ins().uextend(types::I64, value),
            SemanticTypeId::I32 => self.builder.ins().sextend(types::I64, value),
            SemanticTypeId::U32 => self.builder.ins().uextend(types::I64, value),
            SemanticTypeId::U8 => self.builder.ins().uextend(types::I64, value),
            _ => return None,
        };
        self.emit_corelib_service_call(
            key,
            "str_from_i64",
            &[coerced],
            &[types::I64],
            Some(dispatch::pointer_type(self.frontend_config)),
        )
    }

    /// Interpolates a `bool` as `true` or `false`. Each branch materializes only its own literal.
    fn bool_to_string(&mut self, key: AstNodeKey, flag: Value) -> Option<Value> {
        let pointer = dispatch::pointer_type(self.frontend_config);
        let true_block = self.builder.create_block();
        let false_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, pointer);
        self.builder.ins().brif(flag, true_block, &[], false_block, &[]);
        for (block, text) in [(true_block, "true"), (false_block, "false")] {
            self.builder.switch_to_block(block);
            self.builder.seal_block(block);
            let interned = match self.string_interner.as_deref_mut()?.intern(self.builder, key, text) {
                Ok(value) => value,
                Err(error) => {
                    self.pending_error =
                        Some(LoweringError { key, kind: LoweringErrorKind::StringMaterialization(error) });
                    return None;
                }
            };
            self.builder.ins().jump(merge_block, &[interned.into()]);
        }
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        self.guard_checked_allocation()?;
        self.builder.block_params(merge_block).first().copied()
    }

    /// Interpolates an `f32` or `f64` as the shortest decimal text that reads back as the same
    /// value. The runtime has no float operations, so the value passes as its IEEE-754 binary64
    /// bits; an `f32` widens exactly to binary64 first. The binary64 shortest text of a widened
    /// `f32` can be longer than the binary32 shortest text, but it always round-trips.
    fn float_to_string(&mut self, key: AstNodeKey, value: Value) -> Option<Value> {
        let value = match self.builder.func.dfg.value_type(value) {
            types::F64 => value,
            types::F32 => self.builder.ins().fpromote(types::F64, value),
            _ => return None,
        };
        let bits = self.builder.ins().bitcast(types::I64, MemFlagsData::new(), value);
        self.emit_corelib_service_call(
            key,
            "str_from_f64_bits",
            &[bits],
            &[types::I64],
            Some(dispatch::pointer_type(self.frontend_config)),
        )
    }

    /// `<`, `<=`, `>`, `>=` on strings: ordinal UTF-8 byte order through `str_cmp`, which returns
    /// -1, 0, or 1.
    pub(super) fn emit_string_ordering(&mut self, key: AstNodeKey) -> Option<Value> {
        let condition = match self.facts.operator_fact(key)? {
            OperatorFact::StringLt => IntCC::SignedLessThan,
            OperatorFact::StringLte => IntCC::SignedLessThanOrEqual,
            OperatorFact::StringGt => IntCC::SignedGreaterThan,
            OperatorFact::StringGte => IntCC::SignedGreaterThanOrEqual,
            _ => return None,
        };
        let left_key = self.facts.child(key, 0)?;
        let right_key = self.facts.child(key, 1)?;
        let left = self.coerce_expression_to_string(left_key)?;
        let right = self.coerce_expression_to_string(right_key)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let order =
            self.emit_corelib_service_call(key, "str_cmp", &[left, right], &[pointer, pointer], Some(types::I64))?;
        Some(self.builder.ins().icmp_imm_s(condition, order, 0))
    }

    pub(super) fn emit_string_compare(&mut self, key: AstNodeKey, invert: bool) -> Option<Value> {
        let left_key = self.facts.child(key, 0)?;
        let right_key = self.facts.child(key, 1)?;
        let left = self.coerce_expression_to_string(left_key)?;
        let right = self.coerce_expression_to_string(right_key)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let eq_flag =
            self.emit_corelib_service_call(key, "str_eq", &[left, right], &[pointer, pointer], Some(types::I64))?;
        let zero = self.builder.ins().iconst(types::I64, 0);
        Some(if invert {
            self.builder.ins().icmp(IntCC::Equal, eq_flag, zero)
        } else {
            self.builder.ins().icmp(IntCC::NotEqual, eq_flag, zero)
        })
    }
}

macro_rules! generated_string_methods {
    () => {
        fn emit_string(&mut self, key: AstNodeKey) -> Option<Value> {
            let text = self.facts.string_literal(key)?;
            match self.string_interner.as_deref_mut()?.intern(self.builder, key, &text) {
                Ok(value) => {
                    self.guard_checked_allocation()?;
                    Some(value)
                }
                Err(error) => {
                    self.pending_error =
                        Some(LoweringError { key, kind: LoweringErrorKind::StringMaterialization(error) });
                    None
                }
            }
        }
        fn emit_string_concat(&mut self, key: AstNodeKey) -> Option<Value> {
            let left_key = self.facts.child(key, 0)?;
            let right_key = self.facts.child(key, 1)?;
            let left = self.coerce_expression_to_string(left_key)?;
            let right = self.coerce_expression_to_string(right_key)?;
            let pointer = dispatch::pointer_type(self.frontend_config);
            self.emit_corelib_service_call(key, "str_concat", &[left, right], &[pointer, pointer], Some(pointer))
        }

        fn emit_string_eq(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_string_compare(key, false)
        }

        fn emit_string_ne(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_string_compare(key, true)
        }

        fn emit_string_order(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_string_ordering(key)
        }

        fn emit_string_index_read(&mut self, key: AstNodeKey) -> Option<Value> {
            let base_key = self.facts.child(key, 0)?;
            let index_key = self.facts.child(key, 1)?;
            let handle = generated::constructor_lower_expression(self, base_key)?;
            let index = generated::constructor_lower_expression(self, index_key)?;
            let pointer_type = self.builder.func.dfg.value_type(handle);
            if !pointer_type.is_int() || !self.builder.func.dfg.value_type(index).is_int() {
                return None;
            }
            let ptr = self.builder.ins().load(pointer_type, MemFlagsData::new(), handle, 0);
            let len = self.builder.ins().load(pointer_type, MemFlagsData::new(), handle, 8);
            let out_of_bounds = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, index, len);
            self.builder.ins().trapnz(out_of_bounds, TrapCode::unwrap_user(2));
            let addr = self.builder.ins().iadd(ptr, index);
            Some(self.builder.ins().load(types::I8, MemFlagsData::new(), addr, 0))
        }
    };
}

pub(super) use generated_string_methods;
