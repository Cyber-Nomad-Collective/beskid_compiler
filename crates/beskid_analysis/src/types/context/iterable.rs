use crate::hir::HirExpressionNode;
use crate::resolve::ItemKind;
use crate::syntax::Spanned;
use crate::types::{TypeId, TypeInfo};

use super::context::{TypeContext, TypeError};

impl<'a> TypeContext<'a> {
    pub(super) fn resolve_iterable_item_type(
        &mut self,
        iterable: &Spanned<HirExpressionNode>,
    ) -> Option<TypeId> {
        let iterable_type = self.type_expression(iterable)?;
        let Some(next_method_item_id) = self.method_item_for_receiver(iterable_type, "Next") else {
            self.errors.push(TypeError::NonIterableForTarget {
                span: iterable.span,
            });
            return None;
        };

        let Some(next_signature) = self.function_signatures.get(&next_method_item_id).cloned()
        else {
            self.errors.push(TypeError::NonIterableForTarget {
                span: iterable.span,
            });
            return None;
        };

        if !next_signature.params.is_empty() {
            self.errors.push(TypeError::IterableNextArityMismatch {
                span: iterable.span,
                expected: 0,
                actual: next_signature.params.len(),
            });
            return None;
        }

        self.option_payload_type(next_signature.return_type, iterable.span)
    }

    fn option_payload_type(
        &mut self,
        next_return_type: TypeId,
        span: crate::syntax::SpanInfo,
    ) -> Option<TypeId> {
        let Some(option_item_id) = self.item_id_for_name("Option", ItemKind::Enum) else {
            self.errors
                .push(TypeError::IterableNextReturnNotOption { span });
            return None;
        };

        match self.type_table.get(next_return_type).cloned() {
            Some(TypeInfo::Applied { base, args }) if base == option_item_id => {
                if args.len() == 1 {
                    Some(args[0])
                } else {
                    self.errors
                        .push(TypeError::IterableOptionSomeArityMismatch {
                            span,
                            expected: 1,
                            actual: args.len(),
                        });
                    None
                }
            }
            Some(TypeInfo::Named(item_id)) if item_id == option_item_id => {
                let some_fields =
                    self.enum_variants_ordered
                        .get(&option_item_id)
                        .and_then(|variants| {
                            variants
                                .iter()
                                .find(|(name, _)| name == "Some")
                                .map(|(_, fields)| fields.clone())
                        });
                let Some(fields) = some_fields else {
                    self.errors
                        .push(TypeError::IterableNextReturnNotOption { span });
                    return None;
                };
                if fields.len() == 1 {
                    Some(fields[0])
                } else {
                    self.errors
                        .push(TypeError::IterableOptionSomeArityMismatch {
                            span,
                            expected: 1,
                            actual: fields.len(),
                        });
                    None
                }
            }
            _ => {
                self.errors
                    .push(TypeError::IterableNextReturnNotOption { span });
                None
            }
        }
    }
}
