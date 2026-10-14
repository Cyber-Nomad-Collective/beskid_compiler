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
        let (left, right, root) = self.lower_string_operands(left_key, right_key)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let order =
            self.emit_corelib_service_call(key, "str_cmp", &[left, right], &[pointer, pointer], Some(types::I64))?;
        self.release_expression_root(root)?;
        Some(self.builder.ins().icmp_imm_s(condition, order, 0))
    }

    pub(super) fn emit_string_compare(&mut self, key: AstNodeKey, invert: bool) -> Option<Value> {
        let left_key = self.facts.child(key, 0)?;
        let right_key = self.facts.child(key, 1)?;
        if let Some((operand, literal)) = self.short_literal_operand(left_key, right_key) {
            let value = self.coerce_expression_to_string(operand)?;
            let equal = self.emit_string_equals_bytes(value, literal.as_bytes());
            return Some(if invert { self.builder.ins().icmp_imm_u(IntCC::Equal, equal, 0) } else { equal });
        }
        let (left, right, root) = self.lower_string_operands(left_key, right_key)?;
        let pointer = dispatch::pointer_type(self.frontend_config);
        let eq_flag =
            self.emit_corelib_service_call(key, "str_eq", &[left, right], &[pointer, pointer], Some(types::I64))?;
        self.release_expression_root(root)?;
        let zero = self.builder.ins().iconst(types::I64, 0);
        Some(if invert {
            self.builder.ins().icmp(IntCC::Equal, eq_flag, zero)
        } else {
            self.builder.ins().icmp(IntCC::NotEqual, eq_flag, zero)
        })
    }
}

/// Longest string literal compared inline by `==` and `!=`; a longer one goes through `str_eq`.
pub(super) const INLINE_LITERAL_COMPARE_BYTES: usize = 64;

impl IsleContext<'_, '_, '_, '_> {
    /// Lowers both operands of a binary string operation, each coerced to `string`. The left
    /// value stays rooted while the right operand is evaluated unless that evaluation is a plain
    /// read of a `string` local, which cannot allocate: in `"${n}|x"` the literal is materialized
    /// after `n` is formatted, and a collection there would otherwise free the formatted text
    /// before the runtime call reads it. The caller releases the root after that call.
    pub(crate) fn lower_string_operands(
        &mut self,
        left_key: AstNodeKey,
        right_key: AstNodeKey,
    ) -> Option<(Value, Value, Option<ScopedTemporaryRoot>)> {
        let left = self.coerce_expression_to_string(left_key)?;
        let right_cannot_allocate = self.facts.semantic_type(right_key) == Some(beskid_queries::SemanticTypeId::STRING)
            && self.is_trivial_call_argument(right_key);
        let root = if right_cannot_allocate { None } else { Some(self.root_expression_value(left)?) };
        let right = self.coerce_expression_to_string(right_key)?;
        Some((left, right, root))
    }

    /// For `==` / `!=` with a string literal of at most [`INLINE_LITERAL_COMPARE_BYTES`] bytes on
    /// one side and a `string` on the other: the non-literal operand and the literal text.
    fn short_literal_operand(&self, left: AstNodeKey, right: AstNodeKey) -> Option<(AstNodeKey, std::sync::Arc<str>)> {
        use beskid_queries::SemanticTypeId;

        let literal = |key: AstNodeKey| {
            (self.facts.semantic_type(key) == Some(SemanticTypeId::STRING))
                .then(|| self.facts.string_literal(key))
                .flatten()
                .filter(|text| text.len() <= INLINE_LITERAL_COMPARE_BYTES)
        };
        let string = |key: AstNodeKey| self.facts.semantic_type(key) == Some(SemanticTypeId::STRING);
        if let Some(text) = literal(right)
            && string(left)
        {
            return Some((left, text));
        }
        if let Some(text) = literal(left)
            && string(right)
        {
            return Some((right, text));
        }
        None
    }

    /// Compares the string `value` with `bytes` without materializing `bytes` as a managed
    /// string: a null check, a length check, then the data compared in 8-, 4-, 2- and 1-byte
    /// little-endian chunks against constants (every ABI-v5 target is little-endian). Equal to
    /// `str_eq(value, literal) != 0`, as an `i8` flag.
    pub(super) fn emit_string_equals_bytes(&mut self, value: Value, bytes: &[u8]) -> Value {
        let pointer = dispatch::pointer_type(self.frontend_config);
        let done = self.builder.create_block();
        self.builder.append_block_param(done, types::I8);
        let length_block = self.builder.create_block();
        let bytes_block = self.builder.create_block();
        let not_equal = self.builder.ins().iconst(types::I8, 0);
        let is_null = self.builder.ins().icmp_imm_u(IntCC::Equal, value, 0);
        self.builder.ins().brif(is_null, done, &[not_equal.into()], length_block, &[]);

        self.builder.switch_to_block(length_block);
        self.builder.seal_block(length_block);
        let length_offset = i32::try_from(pointer.bytes()).unwrap_or(8);
        let length = self.builder.ins().load(pointer, MemFlagsData::new(), value, length_offset);
        let length_matches = self.builder.ins().icmp_imm_u(IntCC::Equal, length, bytes.len() as i64);
        let not_equal = self.builder.ins().iconst(types::I8, 0);
        self.builder.ins().brif(length_matches, bytes_block, &[], done, &[not_equal.into()]);

        self.builder.switch_to_block(bytes_block);
        self.builder.seal_block(bytes_block);
        let equal = if bytes.is_empty() {
            self.builder.ins().iconst(types::I8, 1)
        } else {
            let data = self.builder.ins().load(pointer, MemFlagsData::new(), value, 0);
            let mut difference: Option<Value> = None;
            let mut offset = 0;
            while offset < bytes.len() {
                let width = [8, 4, 2, 1].into_iter().find(|width| offset + width <= bytes.len()).unwrap_or(1);
                let ty = match width {
                    8 => types::I64,
                    4 => types::I32,
                    2 => types::I16,
                    _ => types::I8,
                };
                let mut chunk = [0_u8; 8];
                chunk[..width].copy_from_slice(&bytes[offset..offset + width]);
                let expected = self.builder.ins().iconst(ty, u64::from_le_bytes(chunk) as i64);
                let loaded =
                    self.builder.ins().load(ty, MemFlagsData::new(), data, i32::try_from(offset).unwrap_or(i32::MAX));
                let mut diff = self.builder.ins().bxor(loaded, expected);
                if ty != types::I64 {
                    diff = self.builder.ins().uextend(types::I64, diff);
                }
                difference = Some(match difference {
                    Some(previous) => self.builder.ins().bor(previous, diff),
                    None => diff,
                });
                offset += width;
            }
            let difference = difference.unwrap_or_else(|| self.builder.ins().iconst(types::I64, 0));
            self.builder.ins().icmp_imm_u(IntCC::Equal, difference, 0)
        };
        self.builder.ins().jump(done, &[equal.into()]);

        self.builder.switch_to_block(done);
        self.builder.seal_block(done);
        self.builder.block_params(done)[0]
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
            let (left, right, root) = self.lower_string_operands(left_key, right_key)?;
            let pointer = dispatch::pointer_type(self.frontend_config);
            let result =
                self.emit_corelib_service_call(key, "str_concat", &[left, right], &[pointer, pointer], Some(pointer))?;
            self.release_expression_root(root)?;
            Some(result)
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
