//! `move` expression and `exchange` facts: the transfer or destructive-read plan of a `move`,
//! with the static metadata of its canonical empty value, and the operands of `exchange`.

use super::super::*;
use beskid_isle::{CanonicalEmptyPlan, ExchangePlan, ManagedArrayAllocation, MoveExpressionPlan};
use beskid_queries::{CanonicalEmptyValue, MoveExpressionForm};

impl SyntaxNodeFacts<'_> {
    pub(super) fn move_expression_impl(&self, key: AstNodeKey) -> Option<MoveExpressionPlan> {
        match self.query(beskid_queries::move_expression_form(self.db, key))? {
            MoveExpressionForm::Consume { operand } => Some(MoveExpressionPlan::Transfer { operand }),
            MoveExpressionForm::DestructiveRead { place, .. } => {
                let statics = self.input.canonical_empty_statics(key, self.current_item_specialization())?;
                let array = || {
                    Some(ManagedArrayAllocation {
                        allocation_request_symbol: statics.array.as_ref()?.allocation_request_symbol.as_str().into(),
                    })
                };
                let object = || {
                    Some(ManagedStructAllocation {
                        allocation_request_symbol: statics.object.as_ref()?.allocation_request_symbol.as_str().into(),
                    })
                };
                let empty = match &statics.value {
                    CanonicalEmptyValue::OptionNone { layout, variant_index } => CanonicalEmptyPlan::OptionNone {
                        layout: self.enum_layout_from_fact(layout)?,
                        allocation: object()?,
                        variant: *variant_index,
                    },
                    CanonicalEmptyValue::EmptyArray { .. } => CanonicalEmptyPlan::EmptyArray { allocation: array()? },
                    CanonicalEmptyValue::EmptyList { declaration, layout, storage_field, count_field, .. } => {
                        let object_layout = self.input.aggregate_object_layout_from_fact(*declaration, layout)?;
                        let offset = |field: u32| {
                            object_layout
                                .fields
                                .get(usize::try_from(field).ok()?)
                                .and_then(|field| i32::try_from(field.field_offset).ok())
                        };
                        CanonicalEmptyPlan::EmptyList {
                            storage: array()?,
                            list: object()?,
                            storage_offset: offset(*storage_field)?,
                            count_offset: offset(*count_field)?,
                            count_type: map_signature_type(self.isa?, SemanticTypeId::I64)?,
                        }
                    }
                };
                Some(MoveExpressionPlan::DestructiveRead { place, empty })
            }
        }
    }

    pub(super) fn exchange_impl(&self, key: AstNodeKey) -> Option<ExchangePlan> {
        self.query(beskid_queries::exchange_call(self.db, key))
            .map(|call| ExchangePlan { place: call.place, value: call.value })
    }
}
