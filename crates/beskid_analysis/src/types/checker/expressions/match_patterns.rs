use crate::hir::{HirMatchArm, HirMatchExpression, HirPattern};
use crate::syntax::Spanned;
use crate::types::TypeId;
use crate::types::result::TypeError;

use super::super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(super) fn type_match_expression(&mut self, match_expr: &Spanned<HirMatchExpression>) -> Option<TypeId> {
        self.type_match_expression_with_expected(match_expr, self.contextual_expected_type)
    }

    pub(in crate::types::checker) fn type_match_expression_with_expected(
        &mut self,
        match_expr: &Spanned<HirMatchExpression>,
        outer_expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let scrutinee_type = self.type_expression(&match_expr.node.scrutinee);
        let mut expected = outer_expected;
        for arm in &match_expr.node.arms {
            self.type_match_arm(scrutinee_type, arm, &mut expected);
        }
        expected
    }

    fn type_match_arm(
        &mut self,
        scrutinee_type: Option<TypeId>,
        arm: &Spanned<HirMatchArm>,
        expected: &mut Option<TypeId>,
    ) {
        if let Some(guard) = &arm.node.guard {
            self.require_bool(guard.span, guard);
        }
        self.type_pattern(scrutinee_type, &arm.node.pattern);
        let previous_expected = self.contextual_expected_type;
        self.contextual_expected_type = *expected;
        let arm_type = self.type_expression(&arm.node.value);
        self.contextual_expected_type = previous_expected;
        if let Some(actual) = arm_type {
            if let Some(expected_type) = *expected {
                if !self.is_never(expected_type) && !self.is_never(actual) {
                    self.require_same_type(arm.node.value.span, expected_type, actual);
                }
            } else {
                *expected = Some(actual);
            }
        }
    }

    fn type_pattern(&mut self, scrutinee_type: Option<TypeId>, pattern: &Spanned<HirPattern>) {
        let Some(scrutinee_type) = scrutinee_type else {
            return;
        };
        match &pattern.node {
            HirPattern::Enum(enum_pattern) => {
                let enum_type = self.type_id_for_enum_path(enum_pattern.node.path.span, &enum_pattern.node.path);
                if let Some(enum_type) = enum_type {
                    let compatible_enum = enum_type == scrutinee_type
                        || (self.named_item_id(enum_type).is_some()
                            && self.named_item_id(enum_type) == self.named_item_id(scrutinee_type));
                    if !compatible_enum {
                        self.errors.push(TypeError::TypeMismatch {
                            span: pattern.span,
                            expected: scrutinee_type,
                            actual: enum_type,
                        });
                    }
                    if let Some(item_id) = self.named_item_id(enum_type)
                        && let Some(variants) = self.enum_variants.get(&item_id)
                    {
                        let variant_name = enum_pattern.node.path.node.variant.node.name.as_str();
                        if let Some(fields) = variants.get(variant_name).cloned() {
                            let mapping_source = match self.type_table.get(scrutinee_type) {
                                Some(crate::types::TypeInfo::Applied { .. }) => scrutinee_type,
                                _ => enum_type,
                            };
                            let mapping = self.generic_mapping_for_type_id(mapping_source);
                            let fields = if mapping.is_empty() {
                                fields
                            } else {
                                fields.iter().map(|field| self.substitute_type_id(*field, &mapping)).collect::<Vec<_>>()
                            };
                            if fields.len() != enum_pattern.node.items.len() {
                                self.errors.push(TypeError::EnumConstructorMismatch {
                                    span: pattern.span,
                                    expected: fields.len(),
                                    actual: enum_pattern.node.items.len(),
                                });
                            }
                            for (item, expected_type) in enum_pattern.node.items.iter().zip(fields.iter()) {
                                self.type_pattern_with_expected(*expected_type, item);
                            }
                        } else {
                            self.errors.push(TypeError::UnknownEnumVariant {
                                span: enum_pattern.node.path.node.variant.span,
                                name: enum_pattern.node.path.node.variant.node.name.clone(),
                            });
                        }
                    }
                }
            }
            HirPattern::Identifier(_) | HirPattern::Wildcard | HirPattern::Literal(_) => {
                self.type_pattern_with_expected(scrutinee_type, pattern);
            }
        }
    }

    fn type_pattern_with_expected(&mut self, expected_type: TypeId, pattern: &Spanned<HirPattern>) {
        match &pattern.node {
            HirPattern::Identifier(identifier) => {
                self.insert_local_type(identifier.span, expected_type);
            }
            HirPattern::Literal(literal) => {
                if let Some(actual) = self.type_id_for_literal(literal) {
                    self.require_same_type(pattern.span, expected_type, actual);
                }
            }
            HirPattern::Wildcard => {}
            HirPattern::Enum(enum_pattern) => {
                let enum_type = self.type_id_for_enum_path(enum_pattern.node.path.span, &enum_pattern.node.path);
                if let Some(enum_type) = enum_type {
                    let compatible_enum = enum_type == expected_type
                        || (self.named_item_id(enum_type).is_some()
                            && self.named_item_id(enum_type) == self.named_item_id(expected_type));
                    if !compatible_enum {
                        self.require_same_type(pattern.span, expected_type, enum_type);
                    }
                }
                for item in &enum_pattern.node.items {
                    self.type_pattern(None, item);
                }
            }
        }
    }
}
