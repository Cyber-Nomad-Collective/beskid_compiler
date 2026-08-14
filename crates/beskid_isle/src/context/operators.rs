use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn short_circuit(&mut self, key: AstNodeKey, branch_on_true: bool) -> Option<Value> {
        let left_key = self.facts.child(key, 0)?;
        let right_key = self.facts.child(key, 1)?;
        let left = generated::constructor_lower_expression(self, left_key)?;
        let value_type = self.builder.func.dfg.value_type(left);
        let right_block = self.builder.create_block();
        let merge_block = self.builder.create_block();
        self.builder.append_block_param(merge_block, value_type);

        if branch_on_true {
            self.builder.ins().brif(left, merge_block, &[left.into()], right_block, &[]);
        } else {
            self.builder.ins().brif(left, right_block, &[], merge_block, &[left.into()]);
        }

        self.builder.switch_to_block(right_block);
        self.builder.seal_block(right_block);
        let right = generated::constructor_lower_expression(self, right_key)?;
        self.builder.ins().jump(merge_block, &[right.into()]);
        self.builder.switch_to_block(merge_block);
        self.builder.seal_block(merge_block);
        self.builder.block_params(merge_block).first().copied()
    }
}

pub(super) fn primitive_numeric_conversion_type_matches(ty: Type, semantic: beskid_queries::SemanticTypeId) -> bool {
    match semantic {
        beskid_queries::SemanticTypeId::I32 => ty == types::I32,
        beskid_queries::SemanticTypeId::I64 => ty == types::I64,
        beskid_queries::SemanticTypeId::U8 => ty == types::I8,
        beskid_queries::SemanticTypeId::WORD => ty.is_int() && matches!(ty.bits(), 32 | 64),
        _ => false,
    }
}

pub(super) enum CompareOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

impl CompareOp {
    pub(super) fn intcc(self, ty: Type) -> IntCC {
        use IntCC::*;
        let signed = ty != types::I8;
        match self {
            CompareOp::Eq => Equal,
            CompareOp::Ne => NotEqual,
            CompareOp::Lt if signed => SignedLessThan,
            CompareOp::Lt => UnsignedLessThan,
            CompareOp::Lte if signed => SignedLessThanOrEqual,
            CompareOp::Lte => UnsignedLessThanOrEqual,
            CompareOp::Gt if signed => SignedGreaterThan,
            CompareOp::Gt => UnsignedGreaterThan,
            CompareOp::Gte if signed => SignedGreaterThanOrEqual,
            CompareOp::Gte => UnsignedGreaterThanOrEqual,
        }
    }

    pub(super) fn fcmpcc(self) -> FloatCC {
        match self {
            CompareOp::Eq => FloatCC::Equal,
            CompareOp::Ne => FloatCC::NotEqual,
            CompareOp::Lt => FloatCC::LessThan,
            CompareOp::Lte => FloatCC::LessThanOrEqual,
            CompareOp::Gt => FloatCC::GreaterThan,
            CompareOp::Gte => FloatCC::GreaterThanOrEqual,
        }
    }
}

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn lower_compare(&mut self, left: Value, right: Value, op: CompareOp) -> Value {
        if let Some((left, right)) = self.common_float_operands(left, right) {
            return self.builder.ins().fcmp(op.fcmpcc(), left, right);
        }
        let (left, right) = self.common_integer_operands(left, right);
        let ty = self.builder.func.dfg.value_type(left);
        self.builder.ins().icmp(op.intcc(ty), left, right)
    }

    pub(super) fn lower_enum_discriminant_compare(&mut self, key: AstNodeKey, invert: bool) -> Option<Value> {
        let layout = self.facts.binary_enum_layout(key)?;
        if !layout.is_valid() {
            self.pending_error = Some(LoweringError { key, kind: LoweringErrorKind::InvalidEnumLayout });
            return None;
        }
        let left_key = self.facts.child(key, 0)?;
        let right_key = self.facts.child(key, 1)?;
        let left = generated::constructor_lower_expression(self, left_key)?;
        let right = generated::constructor_lower_expression(self, right_key)?;
        if !self.builder.func.dfg.value_type(left).is_int() || !self.builder.func.dfg.value_type(right).is_int() {
            return None;
        }
        let left_tag = self.builder.ins().load(
            layout.tag.value_type,
            MemFlags::new(),
            left,
            i32::try_from(layout.tag.offset).ok()?,
        );
        let right_tag = self.builder.ins().load(
            layout.tag.value_type,
            MemFlags::new(),
            right,
            i32::try_from(layout.tag.offset).ok()?,
        );
        let cc = if invert { IntCC::NotEqual } else { IntCC::Equal };
        Some(self.builder.ins().icmp(cc, left_tag, right_tag))
    }

    pub(super) fn common_integer_operands(&mut self, left: Value, right: Value) -> (Value, Value) {
        let left_type = self.builder.func.dfg.value_type(left);
        let right_type = self.builder.func.dfg.value_type(right);
        if !left_type.is_int() || !right_type.is_int() || left_type == right_type {
            return (left, right);
        }
        let target = if left_type.bits() >= right_type.bits() { left_type } else { right_type };
        let widen = |builder: &mut FunctionBuilder<'_>, value: Value, source: Type| {
            if source == target {
                value
            } else if source == types::I8 {
                builder.ins().uextend(target, value)
            } else {
                builder.ins().sextend(target, value)
            }
        };
        (widen(&mut self.builder, left, left_type), widen(&mut self.builder, right, right_type))
    }

    pub(super) fn common_float_operands(&mut self, left: Value, right: Value) -> Option<(Value, Value)> {
        let left_type = self.builder.func.dfg.value_type(left);
        let right_type = self.builder.func.dfg.value_type(right);
        if !left_type.is_float() || !right_type.is_float() {
            return None;
        }
        if left_type == right_type {
            return Some((left, right));
        }
        if left_type.bits() >= right_type.bits() {
            Some((left, self.builder.ins().fpromote(left_type, right)))
        } else {
            Some((self.builder.ins().fpromote(right_type, left), right))
        }
    }
}

macro_rules! generated_operator_methods {
    () => {
        fn clif_iadd(&mut self, left: Value, right: Value) -> Value {
            if self.builder.func.dfg.value_type(left).is_float() {
                self.builder.ins().fadd(left, right)
            } else {
                self.builder.ins().iadd(left, right)
            }
        }
        fn clif_isub(&mut self, left: Value, right: Value) -> Value {
            if self.builder.func.dfg.value_type(left).is_float() {
                self.builder.ins().fsub(left, right)
            } else {
                self.builder.ins().isub(left, right)
            }
        }
        fn clif_imul(&mut self, left: Value, right: Value) -> Value {
            if self.builder.func.dfg.value_type(left).is_float() {
                self.builder.ins().fmul(left, right)
            } else {
                self.builder.ins().imul(left, right)
            }
        }
        fn clif_band(&mut self, left: Value, right: Value) -> Value {
            self.builder.ins().band(left, right)
        }
        fn clif_bor(&mut self, left: Value, right: Value) -> Value {
            self.builder.ins().bor(left, right)
        }
        fn clif_ishl(&mut self, left: Value, right: Value) -> Value {
            self.builder.ins().ishl(left, right)
        }
        fn clif_ushr(&mut self, left: Value, right: Value) -> Value {
            self.builder.ins().ushr(left, right)
        }
        fn clif_sdiv(&mut self, left: Value, right: Value) -> Value {
            if self.builder.func.dfg.value_type(left).is_float() {
                self.builder.ins().fdiv(left, right)
            } else {
                let ty = self.builder.func.dfg.value_type(left);
                let zero = self.builder.ins().iconst(ty, 0);
                let is_zero = self.builder.ins().icmp(IntCC::Equal, right, zero);
                self.builder.ins().trapnz(is_zero, TrapCode::INTEGER_DIVISION_BY_ZERO);
                self.builder.ins().sdiv(left, right)
            }
        }
        fn clif_srem(&mut self, left: Value, right: Value) -> Option<Value> {
            let ty = self.builder.func.dfg.value_type(left);
            if ty.is_float() {
                return None;
            }
            let zero = self.builder.ins().iconst(ty, 0);
            let is_zero = self.builder.ins().icmp(IntCC::Equal, right, zero);
            self.builder.ins().trapnz(is_zero, TrapCode::INTEGER_DIVISION_BY_ZERO);
            Some(self.builder.ins().srem(left, right))
        }
        fn clif_div_trapz(&mut self, value: Value, divisor: Value) -> Value {
            let ty = self.builder.func.dfg.value_type(value);
            let zero = self.builder.ins().iconst(ty, 0);
            let is_zero = self.builder.ins().icmp(IntCC::Equal, divisor, zero);
            self.builder.ins().trapnz(is_zero, TrapCode::INTEGER_DIVISION_BY_ZERO);
            self.builder.ins().sdiv(value, divisor)
        }
        fn clif_iadd_imm(&mut self, value: Value, imm: i64) -> Value {
            self.builder.ins().iadd_imm(value, imm)
        }
        fn clif_imul_imm(&mut self, value: Value, imm: i64) -> Value {
            self.builder.ins().imul_imm(value, imm)
        }

        fn clif_eq(&mut self, left: Value, right: Value) -> Value {
            self.lower_compare(left, right, CompareOp::Eq)
        }

        fn clif_eq_discriminant(
            &mut self,
            left: Value,
            right: Value,
            left_key: AstNodeKey,
            right_key: AstNodeKey,
        ) -> Value {
            if let (Some(left_layout), Some(right_layout)) =
                (self.facts.enum_layout(left_key), self.facts.enum_layout(right_key))
                && left_layout.variants == right_layout.variants
                && left_layout.tag.value_type == right_layout.tag.value_type
                && let (Ok(left_offset), Ok(right_offset)) =
                    (i32::try_from(left_layout.tag.offset), i32::try_from(right_layout.tag.offset))
            {
                let left_tag = self.builder.ins().load(left_layout.tag.value_type, MemFlags::new(), left, left_offset);
                let right_tag =
                    self.builder.ins().load(right_layout.tag.value_type, MemFlags::new(), right, right_offset);
                return self.builder.ins().icmp(IntCC::Equal, left_tag, right_tag);
            }
            self.clif_eq(left, right)
        }

        fn clif_ne(&mut self, left: Value, right: Value) -> Value {
            self.lower_compare(left, right, CompareOp::Ne)
        }

        fn clif_ne_discriminant(
            &mut self,
            left: Value,
            right: Value,
            left_key: AstNodeKey,
            right_key: AstNodeKey,
        ) -> Value {
            if let (Some(left_layout), Some(right_layout)) =
                (self.facts.enum_layout(left_key), self.facts.enum_layout(right_key))
                && left_layout.variants == right_layout.variants
                && left_layout.tag.value_type == right_layout.tag.value_type
                && let (Ok(left_offset), Ok(right_offset)) =
                    (i32::try_from(left_layout.tag.offset), i32::try_from(right_layout.tag.offset))
            {
                let left_tag = self.builder.ins().load(left_layout.tag.value_type, MemFlags::new(), left, left_offset);
                let right_tag =
                    self.builder.ins().load(right_layout.tag.value_type, MemFlags::new(), right, right_offset);
                return self.builder.ins().icmp(IntCC::NotEqual, left_tag, right_tag);
            }
            self.clif_ne(left, right)
        }

        fn clif_slt(&mut self, left: Value, right: Value) -> Value {
            self.lower_compare(left, right, CompareOp::Lt)
        }

        fn clif_sle(&mut self, left: Value, right: Value) -> Value {
            self.lower_compare(left, right, CompareOp::Lte)
        }

        fn clif_sgt(&mut self, left: Value, right: Value) -> Value {
            self.lower_compare(left, right, CompareOp::Gt)
        }

        fn clif_sge(&mut self, left: Value, right: Value) -> Value {
            self.lower_compare(left, right, CompareOp::Gte)
        }

        fn clif_enum_eq(&mut self, key: AstNodeKey) -> Option<Value> {
            self.lower_enum_discriminant_compare(key, false)
        }

        fn clif_enum_ne(&mut self, key: AstNodeKey) -> Option<Value> {
            self.lower_enum_discriminant_compare(key, true)
        }

        fn clif_short_circuit_or(&mut self, key: AstNodeKey) -> Option<Value> {
            self.short_circuit(key, true)
        }

        fn clif_short_circuit_and(&mut self, key: AstNodeKey) -> Option<Value> {
            self.short_circuit(key, false)
        }

        fn clif_ineg(&mut self, value: Value) -> Value {
            let ty = self.builder.func.dfg.value_type(value);
            if ty.is_float() { self.builder.ins().fneg(value) } else { self.builder.ins().ineg(value) }
        }

        fn clif_logical_not(&mut self, value: Value) -> Value {
            self.builder.ins().icmp_imm(IntCC::Equal, value, 0)
        }

        fn emit_primitive_numeric_conversion(&mut self, key: AstNodeKey) -> Option<Value> {
            let Some((from, to)) = self.facts.primitive_numeric_conversion(key) else {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidPrimitiveNumericConversion("conversion fact is unavailable"),
                });
                return None;
            };
            let Some(argument) = self.facts.call_arguments(key).and_then(|arguments| arguments.into_iter().next())
            else {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidPrimitiveNumericConversion("single argument fact is unavailable"),
                });
                return None;
            };
            let value = generated::constructor_lower_expression(self, argument)?;
            let actual = self.builder.func.dfg.value_type(value);
            let Some(target) = self.facts.scalar_type(key) else {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidPrimitiveNumericConversion("target scalar type is unavailable"),
                });
                return None;
            };
            if self.facts.semantic_type(argument) != Some(from) || self.facts.semantic_type(key) != Some(to) {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidPrimitiveNumericConversion(
                        "semantic facts differ from conversion fact",
                    ),
                });
                return None;
            }
            if !primitive_numeric_conversion_type_matches(actual, from)
                || !primitive_numeric_conversion_type_matches(target, to)
            {
                self.pending_error = Some(LoweringError {
                    key,
                    kind: LoweringErrorKind::InvalidPrimitiveNumericConversion(
                        "CLIF types do not match conversion fact",
                    ),
                });
                return None;
            }
            if actual == target {
                Some(value)
            } else if actual.bits() < target.bits() {
                if from == beskid_queries::SemanticTypeId::U8 {
                    Some(self.builder.ins().uextend(target, value))
                } else {
                    Some(self.builder.ins().sextend(target, value))
                }
            } else {
                Some(self.builder.ins().ireduce(target, value))
            }
        }
    };
}

pub(super) use generated_operator_methods;
