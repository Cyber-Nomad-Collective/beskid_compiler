//! Compound assignment `place op= value`: the place is evaluated once by the caller, which passes
//! its current value here; the operator is applied with the binary-expression rules and the result
//! is brought back to the place's storage type.

use super::*;
use crate::context::generated::Context as _;
use beskid_queries::{CompoundAssignmentOperator, SemanticTypeId};

impl IsleContext<'_, '_, '_, '_> {
    /// `current op value` for the compound assignment `key`, as a value of `storage_type`.
    /// `string +=` concatenates (the current string stays rooted while the operand is evaluated).
    /// Analysis rejects every operator a type lacks (`%=` on a float, E1209), so a `None` here is a
    /// compiler gap.
    pub(super) fn compound_assignment_value(
        &mut self,
        key: AstNodeKey,
        operator: CompoundAssignmentOperator,
        storage: SemanticTypeId,
        current: Value,
        value_key: AstNodeKey,
        storage_type: Type,
    ) -> Option<Value> {
        if storage == SemanticTypeId::STRING {
            if operator != CompoundAssignmentOperator::Add {
                return None;
            }
            let root = self.root_expression_value(current)?;
            let operand = self.coerce_expression_to_string(value_key)?;
            let pointer = dispatch::pointer_type(self.frontend_config);
            let result = self.emit_corelib_service_call(
                key,
                "str_concat",
                &[current, operand],
                &[pointer, pointer],
                Some(pointer),
            )?;
            self.release_expression_root(Some(root))?;
            return Some(result);
        }
        let operand = self.lower_nested_expression(value_key)?;
        let result = match operator {
            CompoundAssignmentOperator::Add => self.clif_iadd(current, operand),
            CompoundAssignmentOperator::Sub => self.clif_isub(current, operand),
            CompoundAssignmentOperator::Mul => self.clif_imul(current, operand),
            CompoundAssignmentOperator::Div => self.clif_div_trapz(key, current, operand)?,
            CompoundAssignmentOperator::Rem => self.clif_srem(key, current, operand)?,
        };
        self.to_storage_type(result, storage_type, storage)
    }

    /// `value` as `storage_type`: an integer computed wider than its place (an operand widened
    /// the operation) is truncated back, a narrower one is extended by the place's signedness, and
    /// a float is promoted or demoted.
    fn to_storage_type(&mut self, value: Value, storage_type: Type, storage: SemanticTypeId) -> Option<Value> {
        let actual = self.builder.func.dfg.value_type(value);
        if actual == storage_type {
            return Some(value);
        }
        let unsigned = matches!(
            storage,
            SemanticTypeId::U8 | SemanticTypeId::U16 | SemanticTypeId::U32 | SemanticTypeId::U64 | SemanticTypeId::WORD
        );
        Some(match (actual.is_int(), storage_type.is_int(), actual.is_float(), storage_type.is_float()) {
            (true, true, _, _) if actual.bits() > storage_type.bits() => {
                self.builder.ins().ireduce(storage_type, value)
            }
            (true, true, _, _) if unsigned => self.builder.ins().uextend(storage_type, value),
            (true, true, _, _) => self.builder.ins().sextend(storage_type, value),
            (_, _, true, true) if actual.bits() > storage_type.bits() => {
                self.builder.ins().fdemote(storage_type, value)
            }
            (_, _, true, true) => self.builder.ins().fpromote(storage_type, value),
            _ => return None,
        })
    }
}
