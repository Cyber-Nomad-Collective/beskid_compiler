use crate::resolve::ResolvedType;
use crate::syntax::{AstNodeId, Expression, MatchExpression, Pattern, Spanned, StructLiteralExpression};
use crate::types::TypeId;
use crate::types::path_value::{generic_mapping_for_type_id, named_item_id, struct_fields_for_item};

use super::super::compatibility::{is_never, literal_type_id};
use super::super::substitution::substitute_type_id;
use super::state::PrepWalker;

impl<'a> PrepWalker<'a> {
    pub(super) fn prep_struct_literal_casts(&mut self, expr_id: AstNodeId, lit: &Spanned<StructLiteralExpression>) {
        let Some(type_id) = self.node_type(expr_id).or_else(|| self.type_id_for_type_path(&lit.node.path)) else {
            return;
        };
        let Some(item_id) = named_item_id(&self.surfaces.path_env(), type_id) else {
            return;
        };
        let mapping = generic_mapping_for_type_id(&self.surfaces.path_env(), type_id);
        let path_env = self.surfaces.path_env();
        let Some(fields) =
            struct_fields_for_item(&path_env, self.resolution, item_id, self.current_source_path.as_ref())
        else {
            return;
        };
        for field in &lit.node.fields {
            let name = field.node.name.node.name.as_str();
            let Some((_, expected)) = fields.iter().find(|(n, _)| n.as_str() == name) else {
                continue;
            };
            let expected =
                if mapping.is_empty() { *expected } else { substitute_type_id(self.surfaces, *expected, &mapping) };
            if let Some(actual) = self.expr_type(&field.node.value) {
                self.record_numeric_cast(field.node.value.id, field.node.value.span, expected, actual);
            }
        }
    }

    pub(super) fn prep_enum_ctor_casts(&mut self, ctor: &Spanned<crate::syntax::EnumConstructorExpression>) {
        let Some(type_id) =
            self.resolution.tables.resolved_type_at(ctor.node.path.span, self.current_source_path.as_ref()).and_then(
                |r| match r {
                    ResolvedType::Item(id) => self.named_type_id(crate::resolve::canonical_item_id(self.resolution, id)),
                    _ => None,
                },
            )
        else {
            return;
        };
        let Some(item_id) = named_item_id(&self.surfaces.path_env(), type_id) else {
            return;
        };
        let variant = ctor.node.path.node.variant.node.name.as_str();
        let mapping = generic_mapping_for_type_id(&self.surfaces.path_env(), type_id);
        let Some(fields) = self
            .surfaces
            .enum_variants_ordered
            .get(&item_id)
            .and_then(|vars| vars.iter().find(|(n, _)| n == variant))
            .map(|(_, fs)| {
                if mapping.is_empty() {
                    fs.clone()
                } else {
                    fs.iter().map(|f| substitute_type_id(self.surfaces, *f, &mapping)).collect()
                }
            })
        else {
            return;
        };
        for (arg, expected) in ctor.node.args.iter().zip(fields.iter()) {
            if let Some(actual) = self.expr_type(arg) {
                self.record_numeric_cast(arg.id, arg.span, *expected, actual);
            }
        }
    }

    pub(super) fn prep_match(&mut self, m: &Spanned<MatchExpression>) {
        let scrutinee = self.expr_type(&m.node.scrutinee);
        self.walk_expression(&m.node.scrutinee);
        let mut expected = self.contextual_expected_type;
        for arm in &m.node.arms {
            if let Some(g) = &arm.node.guard {
                self.walk_expression(g);
            }
            self.prep_pattern_casts(scrutinee, &arm.node.pattern);
            let prev = self.contextual_expected_type;
            self.contextual_expected_type = expected;
            self.walk_expression(&arm.node.value);
            self.contextual_expected_type = prev;
            if let Some(actual) = self.expr_type(&arm.node.value) {
                if let Some(e) = expected {
                    if !is_never(self.surfaces.types, e) && !is_never(self.surfaces.types, actual) {
                        self.record_numeric_cast(arm.node.value.id, arm.node.value.span, e, actual);
                    }
                } else {
                    expected = Some(actual);
                }
            }
        }
    }

    pub(super) fn prep_pattern_casts(&mut self, scrutinee: Option<TypeId>, pattern: &Spanned<Pattern>) {
        let Some(expected) = scrutinee else {
            return;
        };
        match &pattern.node {
            Pattern::Literal(lit) => {
                if let Some(actual) = literal_type_id(self.surfaces.types, &lit.node) {
                    self.record_numeric_cast(pattern.id, pattern.span, expected, actual);
                }
            }
            Pattern::Enum(ep) => {
                if let Some(actual) = self
                    .resolution
                    .tables
                    .resolved_type_at(ep.node.path.span, self.current_source_path.as_ref())
                    .and_then(|r| match r {
                        ResolvedType::Item(id) => self.named_type_id(crate::resolve::canonical_item_id(self.resolution, id)),
                        _ => None,
                    })
                {
                    let ok = actual == expected
                        || named_item_id(&self.surfaces.path_env(), actual)
                            == named_item_id(&self.surfaces.path_env(), expected);
                    if !ok {
                        self.record_numeric_cast(pattern.id, pattern.span, expected, actual);
                    }
                }
                for p in &ep.node.items {
                    self.prep_pattern_casts(scrutinee, p);
                }
            }
            _ => {}
        }
    }

    pub(super) fn prep_arg_casts(&mut self, args: &[Spanned<Expression>], params: &[TypeId]) {
        for (arg, expected) in args.iter().zip(params.iter()) {
            if let Some(actual) = self.expr_type(arg) {
                self.record_numeric_cast(arg.id, arg.span, *expected, actual);
            }
        }
    }
}
