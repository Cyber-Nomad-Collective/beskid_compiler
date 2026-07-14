//! Human-readable [`TypeId`] labels for diagnostics and error messages.

use crate::hir::HirPrimitiveType;
use crate::resolve::{ItemId, Resolution};
use crate::types::{TypeId, TypeInfo, TypeResult};

fn primitive_type_name(primitive: HirPrimitiveType) -> &'static str {
    match primitive {
        HirPrimitiveType::Bool => "bool",
        HirPrimitiveType::I32 => "i32",
        HirPrimitiveType::I64 => "i64",
        HirPrimitiveType::U8 => "u8",
        HirPrimitiveType::Word => "word",
        HirPrimitiveType::F64 => "f64",
        HirPrimitiveType::Char => "char",
        HirPrimitiveType::String => "string",
        HirPrimitiveType::Unit => "unit",
        HirPrimitiveType::Never => "never",
    }
}

fn named_type_label(
    result: &TypeResult,
    resolution: Option<&Resolution>,
    item_id: ItemId,
) -> String {
    if let Some(name) = result.named_type_names.get(&item_id) {
        return name.clone();
    }
    if let Some(res) = resolution
        && let Some(name) = crate::resolve::qualified_name(res, item_id)
    {
        return name;
    }
    format!("type#{}", item_id.0)
}

/// Format a checked [`TypeId`] for user-facing diagnostics.
pub fn format_type_id(
    result: &TypeResult,
    resolution: Option<&Resolution>,
    type_id: TypeId,
) -> String {
    let Some(info) = result.types.get(type_id) else {
        return format!("type#{}", type_id.0);
    };
    match info {
        TypeInfo::Primitive(primitive) => primitive_type_name(*primitive).to_string(),
        TypeInfo::Named(item_id) => named_type_label(result, resolution, *item_id),
        TypeInfo::GenericParam(name) => name.clone(),
        TypeInfo::Applied { base, args } => {
            let base_name = named_type_label(result, resolution, *base);
            if args.is_empty() {
                return base_name;
            }
            let args = args
                .iter()
                .map(|arg| format_type_id(result, resolution, *arg))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{base_name}<{args}>")
        }
        TypeInfo::Function {
            params,
            return_type,
        } => {
            let params = params
                .iter()
                .map(|param| format_type_id(result, resolution, *param))
                .collect::<Vec<_>>()
                .join(", ");
            let return_name = format_type_id(result, resolution, *return_type);
            format!("{return_name}({params})")
        }
        TypeInfo::Array(element) => {
            let inner = format_type_id(result, resolution, *element);
            format!("{inner}[]")
        }
        TypeInfo::Fiber(payload) => {
            let inner = format_type_id(result, resolution, *payload);
            format!("Fiber<{inner}>")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::types::TypeTable;

    fn type_result_with_primitives() -> (TypeResult, TypeId, TypeId) {
        let mut types = TypeTable::new();
        let string = types.intern(TypeInfo::Primitive(HirPrimitiveType::String));
        let i32 = types.intern(TypeInfo::Primitive(HirPrimitiveType::I32));
        let result = TypeResult {
            types,
            named_type_names: HashMap::new(),
            node_types: HashMap::new(),
            local_types: HashMap::new(),
            unit_surfaces: HashMap::new(),
            function_signatures: HashMap::new(),
            method_function_signatures: HashMap::new(),
            struct_fields_ordered: HashMap::new(),
            struct_event_fields: HashMap::new(),
            enum_variants_ordered: HashMap::new(),
            generic_items: HashMap::new(),
            lowering: crate::types::LoweringPrep::default(),
        };
        (result, string, i32)
    }

    #[test]
    fn formats_primitive_types() {
        let (result, string, i32) = type_result_with_primitives();
        assert_eq!(format_type_id(&result, None, string), "string");
        assert_eq!(format_type_id(&result, None, i32), "i32");
    }
}
