//! Constraint vocabulary for local type inference.

use std::collections::HashSet;

use crate::resolve::ItemId;
use crate::syntax::SpanInfo;
use crate::types::TypeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeVar(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Constraint {
    Equal { var: TypeVar, ty: TypeId, span: SpanInfo },
    EqualVar { left: TypeVar, right: TypeVar, span: SpanInfo },
    ApplyGeneric { callee: ItemId, arg_types: Vec<TypeId>, result_vars: Vec<TypeVar>, span: SpanInfo },
    IsNumeric { var: TypeVar, span: SpanInfo, name: String },
    VariantOf { var: TypeVar, enum_item: ItemId, variant: String, span: SpanInfo },
}

#[derive(Debug, Default, Clone)]
pub struct ConstraintSet {
    constraints: Vec<Constraint>,
    next_var: u32,
    must_resolve: HashSet<TypeVar>,
}

impl ConstraintSet {
    pub fn fresh_var(&mut self) -> TypeVar {
        let var = TypeVar(self.next_var);
        self.next_var = self.next_var.saturating_add(1);
        var
    }

    pub fn push(&mut self, constraint: Constraint) {
        if let Constraint::IsNumeric { var, .. } = &constraint {
            self.must_resolve.insert(*var);
        }
        self.constraints.push(constraint);
    }

    pub fn mark_must_resolve(&mut self, var: TypeVar) {
        self.must_resolve.insert(var);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Constraint> {
        self.constraints.iter()
    }

    pub fn must_resolve(&self) -> impl Iterator<Item = &TypeVar> {
        self.must_resolve.iter()
    }

    pub fn equal(&mut self, var: TypeVar, ty: TypeId, span: SpanInfo) {
        self.push(Constraint::Equal { var, ty, span });
    }

    pub fn equal_var(&mut self, left: TypeVar, right: TypeVar, span: SpanInfo) {
        self.push(Constraint::EqualVar { left, right, span });
    }

    pub fn is_numeric(&mut self, var: TypeVar, span: SpanInfo, name: impl Into<String>) {
        self.push(Constraint::IsNumeric { var, span, name: name.into() });
    }

    pub fn variant_of(&mut self, var: TypeVar, enum_item: ItemId, variant: impl Into<String>, span: SpanInfo) {
        self.push(Constraint::VariantOf { var, enum_item, variant: variant.into(), span });
    }

    pub fn apply_generic(&mut self, callee: ItemId, arg_types: Vec<TypeId>, result_vars: Vec<TypeVar>, span: SpanInfo) {
        for var in &result_vars {
            self.must_resolve.insert(*var);
        }
        self.push(Constraint::ApplyGeneric { callee, arg_types, result_vars, span });
    }
}
