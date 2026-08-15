use super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(super) fn coerce_expression_to_string(&mut self, key: AstNodeKey) -> Option<Value> {
        use beskid_queries::SemanticTypeId;

        let value = generated::constructor_lower_expression(self, key)?;
        let semantic = self.facts.semantic_type(key)?;
        if semantic == SemanticTypeId::STRING {
            return Some(value);
        }
        let pointer_type = self.builder.func.dfg.value_type(value);
        let coerced = match semantic {
            SemanticTypeId::I64 => value,
            SemanticTypeId::I32 => self.builder.ins().sextend(types::I64, value),
            SemanticTypeId::U8 => self.builder.ins().uextend(types::I64, value),
            SemanticTypeId::BOOL => self.builder.ins().uextend(types::I64, value),
            _ => return None,
        };
        if pointer_type != types::I64 && semantic == SemanticTypeId::I64 {
            // keep i64 as-is
        }
        self.emit_corelib_service_call(key, "str_from_i64", &[coerced], &[types::I64], Some(dispatch::pointer_type()))
    }

    pub(super) fn emit_string_compare(&mut self, key: AstNodeKey, invert: bool) -> Option<Value> {
        let left_key = self.facts.child(key, 0)?;
        let right_key = self.facts.child(key, 1)?;
        let left = self.coerce_expression_to_string(left_key)?;
        let right = self.coerce_expression_to_string(right_key)?;
        let pointer = dispatch::pointer_type();
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
                Ok(value) => Some(value),
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
            let pointer = dispatch::pointer_type();
            self.emit_corelib_service_call(key, "str_concat", &[left, right], &[pointer, pointer], Some(pointer))
        }

        fn emit_string_eq(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_string_compare(key, false)
        }

        fn emit_string_ne(&mut self, key: AstNodeKey) -> Option<Value> {
            self.emit_string_compare(key, true)
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
            let ptr = self.builder.ins().load(pointer_type, MemFlags::new(), handle, 0);
            let len = self.builder.ins().load(pointer_type, MemFlags::new(), handle, 8);
            let out_of_bounds = self.builder.ins().icmp(IntCC::UnsignedGreaterThanOrEqual, index, len);
            self.builder.ins().trapnz(out_of_bounds, TrapCode::unwrap_user(2));
            let addr = self.builder.ins().iadd(ptr, index);
            Some(self.builder.ins().load(types::I8, MemFlags::new(), addr, 0))
        }
    };
}

pub(super) use generated_string_methods;
