//! Constraint-based type inference (replaces ad-hoc try_infer).

pub mod constraint;
pub mod generic;
pub mod solve;
pub mod unify;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::resolve::ItemId;
use crate::types::{TypeId, TypeTable};

pub use crate::types::result::FunctionSignature;
pub use constraint::{Constraint, ConstraintSet, TypeVar};
pub use generic::infer_generic_args_from_call_types;
pub use solve::solve_constraints;
pub use unify::{is_numeric, unify_numeric_types, unify_types};

#[derive(Debug, Default, Clone)]
pub struct InferenceResult {
    pub bindings: HashMap<TypeVar, TypeId>,
}

impl InferenceResult {
    pub fn resolve(&self, var: TypeVar) -> Option<TypeId> {
        self.bindings.get(&var).copied()
    }
}

#[derive(Debug)]
pub struct TypeEnv<'a> {
    table: &'a TypeTable,
    generic_items: Option<&'a HashMap<ItemId, Vec<String>>>,
    function_signatures: Option<&'a HashMap<ItemId, FunctionSignature>>,
    enum_variants: Option<&'a HashMap<ItemId, HashMap<String, Vec<TypeId>>>>,
    named_types: Option<&'a HashMap<ItemId, TypeId>>,
}

impl<'a> TypeEnv<'a> {
    pub fn new(table: &'a TypeTable) -> Self {
        Self {
            table,
            generic_items: None,
            function_signatures: None,
            enum_variants: None,
            named_types: None,
        }
    }

    pub fn with_generics(
        mut self,
        generic_items: &'a HashMap<ItemId, Vec<String>>,
        function_signatures: &'a HashMap<ItemId, FunctionSignature>,
    ) -> Self {
        self.generic_items = Some(generic_items);
        self.function_signatures = Some(function_signatures);
        self
    }

    pub fn with_enum_variants(
        mut self,
        enum_variants: &'a HashMap<ItemId, HashMap<String, Vec<TypeId>>>,
    ) -> Self {
        self.enum_variants = Some(enum_variants);
        self
    }

    pub fn with_named_types(mut self, named_types: &'a HashMap<ItemId, TypeId>) -> Self {
        self.named_types = Some(named_types);
        self
    }

    pub fn table(&self) -> &TypeTable {
        self.table
    }

    pub fn named_type(&self, item_id: ItemId) -> Option<TypeId> {
        self.named_types?.get(&item_id).copied()
    }

    pub(crate) fn generic_items(&self) -> Option<&'a HashMap<ItemId, Vec<String>>> {
        self.generic_items
    }

    pub(crate) fn function_signatures(&self) -> Option<&'a HashMap<ItemId, FunctionSignature>> {
        self.function_signatures
    }

    pub(crate) fn enum_variants(
        &self,
    ) -> Option<&'a HashMap<ItemId, HashMap<String, Vec<TypeId>>>> {
        self.enum_variants
    }
}
