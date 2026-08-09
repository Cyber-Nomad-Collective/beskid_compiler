use std::collections::HashMap;

use crate::resolve::ItemId;
use crate::types::result::FunctionSignature;
use crate::types::{TypeId, TypeTable};

/// Exported type metadata for one compilation unit (keyed by [`ItemId`]).
#[derive(Debug, Default, Clone)]
pub struct UnitTypeSurface {
    pub types: TypeTable,
    pub function_signatures: HashMap<ItemId, FunctionSignature>,
    pub method_function_signatures: HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: HashMap<ItemId, Vec<String>>,
    pub struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    pub contract_method_order: HashMap<ItemId, Vec<String>>,
    pub methods_by_receiver: HashMap<(ItemId, String), ItemId>,
    pub named_type_names: HashMap<ItemId, String>,
}

/// Merged dependency + entry surfaces for body checking and codegen metadata lookup.
#[derive(Debug, Default, Clone)]
pub struct MergedTypeEnv {
    pub function_signatures: HashMap<ItemId, FunctionSignature>,
    pub method_function_signatures: HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: HashMap<ItemId, Vec<String>>,
    pub struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    pub contract_method_order: HashMap<ItemId, Vec<String>>,
    pub methods_by_receiver: HashMap<(ItemId, String), ItemId>,
    pub named_type_names: HashMap<ItemId, String>,
}

impl MergedTypeEnv {
    /// Build a seed [`UnitTypeSurface`] for [`TypeChecker`](crate::types::TypeChecker) from merged metadata.
    pub fn to_unit_surface(&self, types: TypeTable) -> UnitTypeSurface {
        UnitTypeSurface {
            types,
            function_signatures: self.function_signatures.clone(),
            method_function_signatures: self.method_function_signatures.clone(),
            struct_fields_ordered: self.struct_fields_ordered.clone(),
            enum_variants_ordered: self.enum_variants_ordered.clone(),
            generic_items: self.generic_items.clone(),
            struct_event_fields: self.struct_event_fields.clone(),
            contract_signatures: self.contract_signatures.clone(),
            contract_method_order: self.contract_method_order.clone(),
            methods_by_receiver: self.methods_by_receiver.clone(),
            named_type_names: self.named_type_names.clone(),
        }
    }
}
