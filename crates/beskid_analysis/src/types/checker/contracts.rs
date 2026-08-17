use std::collections::{HashMap, HashSet};

use beskid_abi::abi_v5::AbiType;
use beskid_abi::interop::c_profile::{CAbiProfile, CProfileError};
use beskid_abi::interop::mapping::{SurfacePrimitive, surface_primitive_to_type_shape};
use beskid_abi::interop::{
    CallShapeClass, InteropParameter, InteropReturn, InteropSignature, OwnershipClass, ScalarShape, TypeShape,
};

use crate::resolve::{ItemKind, ResolvedType};
use crate::syntax::{ContractNode, Expression, Literal, Node, PrimitiveType, Program, Type};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::result::{FunctionSignature, TypeError};

use super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(super) fn seed_method_receiver(
        &mut self,
        method_span: SpanInfo,
        def: &Spanned<crate::syntax::MethodDefinition>,
    ) {
        let Some(method_item_id) = self.item_id_for_span(method_span) else {
            return;
        };
        let Some(ResolvedType::Item(receiver_item_id)) = self.resolved_type_at(def.node.receiver_type.span) else {
            return;
        };
        self.methods_by_receiver.insert((receiver_item_id, def.node.name.node.name.clone()), method_item_id);
    }

    pub(super) fn seed_contract_signatures(&mut self, program: &Spanned<Program>) {
        let definitions: HashMap<String, &Spanned<crate::syntax::ContractDefinition>> = program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                Node::ContractDefinition(def) => Some((def.node.name.node.name.clone(), def)),
                _ => None,
            })
            .collect();
        let mut cache: HashMap<String, Vec<(String, FunctionSignature)>> = HashMap::new();
        let contract_names = definitions.keys().cloned().collect::<Vec<_>>();

        for contract_name in contract_names {
            let signatures = self.collect_contract_signatures_recursive(
                contract_name.as_str(),
                &definitions,
                &mut cache,
                &mut HashSet::new(),
            );
            let Some(contract_item_id) = self.item_id_for_name(&contract_name, ItemKind::Contract) else {
                continue;
            };
            for (method_name, signature) in signatures {
                self.contract_signatures.insert((contract_item_id, method_name), signature);
            }
            if let Some(definition) = definitions.get(&contract_name) {
                self.validate_extern_contract(definition);
            }
        }
    }

    fn collect_contract_signatures_recursive(
        &mut self,
        contract_name: &str,
        definitions: &HashMap<String, &Spanned<crate::syntax::ContractDefinition>>,
        cache: &mut HashMap<String, Vec<(String, FunctionSignature)>>,
        active: &mut HashSet<String>,
    ) -> Vec<(String, FunctionSignature)> {
        if let Some(cached) = cache.get(contract_name) {
            return cached.clone();
        }
        if !active.insert(contract_name.to_string()) {
            return Vec::new();
        }

        let mut methods = Vec::new();
        let Some(definition) = definitions.get(contract_name) else {
            active.remove(contract_name);
            return methods;
        };

        for node in &definition.node.items {
            match &node.node {
                ContractNode::MethodSignature(signature) => {
                    if methods.iter().any(|(name, _)| name == &signature.node.name.node.name) {
                        continue;
                    }
                    let mut params = Vec::new();
                    let mut valid = true;
                    for param in &signature.node.parameters {
                        let Some(type_id) = self.type_id_for_type(&param.node.ty) else {
                            valid = false;
                            break;
                        };
                        params.push(type_id);
                    }
                    if !valid {
                        continue;
                    }
                    let return_type = signature
                        .node
                        .return_type
                        .as_ref()
                        .and_then(|ty| self.type_id_for_type(ty))
                        .or_else(|| self.primitive_type_id(PrimitiveType::Unit));
                    let Some(return_type) = return_type else {
                        continue;
                    };
                    methods.push((signature.node.name.node.name.clone(), FunctionSignature { params, return_type }));
                }
                ContractNode::Embedding(embedding) => {
                    let embedded = self.collect_contract_signatures_recursive(
                        embedding.node.name.node.name.as_str(),
                        definitions,
                        cache,
                        active,
                    );
                    for (method_name, signature) in embedded {
                        if methods.iter().any(|(name, _)| name == &method_name) {
                            continue;
                        }
                        methods.push((method_name, signature));
                    }
                }
            }
        }

        active.remove(contract_name);
        cache.insert(contract_name.to_string(), methods.clone());
        methods
    }

    /// Validate an `[Extern(...)]` contract against the `Interop.Contracts` C
    /// ABI profile. Constructs [`TypeError::ExternInvalidAbi`],
    /// [`TypeError::ExternMissingLibrary`], [`TypeError::ExternDisallowedParamType`],
    /// or [`TypeError::ExternDisallowedReturnType`] when the contract violates
    /// the FFI boundary rules.
    fn validate_extern_contract(&mut self, definition: &Spanned<crate::syntax::ContractDefinition>) {
        let Some(extern_attr) = definition.node.attributes.iter().find(|attr| attr.node.name.node.name == "Extern")
        else {
            return;
        };

        let contract_span = definition.span;
        let (abi, library) = extract_extern_attr_args(extern_attr);

        match abi.as_deref() {
            Some("C") => {}
            other => {
                self.errors.push(TypeError::ExternInvalidAbi { span: contract_span, abi: other.map(str::to_owned) });
                return;
            }
        }

        if library.is_none() {
            self.errors.push(TypeError::ExternMissingLibrary { span: contract_span });
            return;
        }

        let profile = CAbiProfile;
        for node in &definition.node.items {
            if let ContractNode::MethodSignature(signature) = &node.node {
                self.validate_extern_method(&profile, signature);
            }
        }
    }

    fn validate_extern_method(
        &mut self,
        profile: &CAbiProfile,
        signature: &Spanned<crate::syntax::ContractMethodSignature>,
    ) {
        let method_name = signature.node.name.node.name.clone();
        let method_span = signature.span;

        let mut interop_params = Vec::new();
        for param in &signature.node.parameters {
            match surface_type_shape(&param.node.ty) {
                Some((shape, _)) => {
                    interop_params.push(InteropParameter {
                        name: param.node.name.node.name.clone(),
                        ty: shape,
                        call: CallShapeClass::Direct,
                        ownership: OwnershipClass::Borrow,
                    });
                }
                None => {
                    self.errors.push(TypeError::ExternDisallowedParamType {
                        span: param.node.ty.span,
                        method: method_name.clone(),
                        detail: extern_disallowed_detail(&param.node.ty, false),
                    });
                    return;
                }
            }
        }

        let return_type = signature.node.return_type.as_ref();
        let (return_shape, no_return) = match return_type {
            None => (None, false),
            Some(ty) => match &ty.node {
                Type::Primitive(pt) if pt.node == PrimitiveType::Unit => (None, false),
                Type::Primitive(pt) if pt.node == PrimitiveType::Never => (Some(TypeShape::Never), true),
                Type::Primitive(_) => match surface_type_shape(ty) {
                    Some((shape, _)) => (Some(shape), false),
                    None => {
                        self.errors.push(TypeError::ExternDisallowedReturnType {
                            span: ty.span,
                            method: method_name.clone(),
                            detail: extern_disallowed_detail(ty, true),
                        });
                        return;
                    }
                },
                _ => {
                    self.errors.push(TypeError::ExternDisallowedReturnType {
                        span: ty.span,
                        method: method_name.clone(),
                        detail: extern_disallowed_detail(ty, true),
                    });
                    return;
                }
            },
        };

        let return_shape = match return_shape {
            Some(shape) => shape,
            None => {
                let interop_sig = InteropSignature {
                    symbol: method_name.clone(),
                    parameters: interop_params,
                    returns: InteropReturn {
                        name: None,
                        ty: TypeShape::Scalar(ScalarShape { abi_type: AbiType::I32 }),
                        ownership: OwnershipClass::Borrow,
                    },
                    no_return,
                };
                if let Err(err) = profile.validate_signature(&interop_sig) {
                    self.emit_extern_profile_error(err, &method_name, method_span);
                }
                return;
            }
        };

        let interop_sig = InteropSignature {
            symbol: method_name.clone(),
            parameters: interop_params,
            returns: InteropReturn { name: None, ty: return_shape, ownership: OwnershipClass::Borrow },
            no_return,
        };

        if let Err(err) = profile.validate_signature(&interop_sig) {
            self.emit_extern_profile_error(err, &method_name, method_span);
        }
    }

    fn emit_extern_profile_error(&mut self, err: CProfileError, method_name: &str, method_span: SpanInfo) {
        match err {
            CProfileError::DisallowedShape { .. } | CProfileError::TransferRequiresDirectOrView { .. } => {
                self.errors.push(TypeError::ExternDisallowedParamType {
                    span: method_span,
                    method: method_name.to_owned(),
                    detail: err.to_string(),
                });
            }
            CProfileError::DisallowedReturn => {
                self.errors.push(TypeError::ExternDisallowedReturnType {
                    span: method_span,
                    method: method_name.to_owned(),
                    detail: err.to_string(),
                });
            }
        }
    }
}

/// Extract `(Abi, Library)` string values from an `[Extern(...)]` attribute.
fn extract_extern_attr_args(attr: &Spanned<crate::syntax::Attribute>) -> (Option<String>, Option<String>) {
    let mut abi = None;
    let mut library = None;
    for argument in &attr.node.arguments {
        let value = match &argument.node.value.node {
            Expression::Literal(literal) => match &literal.node.literal.node {
                Literal::String(raw) => raw.strip_prefix('"').and_then(|v| v.strip_suffix('"')).map(str::to_owned),
                _ => None,
            },
            _ => None,
        };
        match argument.node.name.node.name.as_str() {
            "Abi" => abi = value,
            "Library" => library = value,
            _ => {}
        }
    }
    (abi, library)
}

/// Map a surface [`Type`] to its FFI [`TypeShape`]. Returns `None` for types
/// not permitted at the FFI boundary (`Char`, `String`, `Unit`, and all
/// non-primitive types). The second tuple element is the [`SurfacePrimitive`]
/// when the type is a primitive.
fn surface_type_shape(ty: &Spanned<Type>) -> Option<(TypeShape, Option<SurfacePrimitive>)> {
    let Type::Primitive(pt) = &ty.node else {
        return None;
    };
    let surface = primitive_to_surface(pt.node);
    surface_primitive_to_type_shape(surface).map(|shape| (shape, Some(surface)))
}

/// Convert a surface [`PrimitiveType`] to the ABI-layer [`SurfacePrimitive`]
/// mirror (avoids a `beskid_abi → beskid_analysis` dependency).
fn primitive_to_surface(primitive: PrimitiveType) -> SurfacePrimitive {
    match primitive {
        PrimitiveType::Bool => SurfacePrimitive::Bool,
        PrimitiveType::I32 => SurfacePrimitive::I32,
        PrimitiveType::I64 => SurfacePrimitive::I64,
        PrimitiveType::U8 => SurfacePrimitive::U8,
        PrimitiveType::Pointer => SurfacePrimitive::Pointer,
        PrimitiveType::Word => SurfacePrimitive::Word,
        PrimitiveType::F64 => SurfacePrimitive::F64,
        PrimitiveType::Char => SurfacePrimitive::Char,
        PrimitiveType::String => SurfacePrimitive::String,
        PrimitiveType::Unit => SurfacePrimitive::Unit,
        PrimitiveType::Never => SurfacePrimitive::Never,
    }
}

/// Produce a human-readable detail string for a disallowed FFI type.
fn extern_disallowed_detail(ty: &Spanned<Type>, is_return: bool) -> String {
    match &ty.node {
        Type::Primitive(pt) => match pt.node {
            PrimitiveType::Char => "char is not permitted at the FFI boundary".to_string(),
            PrimitiveType::String => "string is a GC reference; use CStringView at the FFI boundary".to_string(),
            PrimitiveType::Unit if is_return => "unit is the void-return marker; omit the return type".to_string(),
            PrimitiveType::Unit => "unit is not a valid FFI parameter type".to_string(),
            PrimitiveType::Word => {
                "word (pointer-width unsigned) is not in the C profile permitted scalars; use pointer instead"
                    .to_string()
            }
            _ => "type is not permitted at the FFI boundary".to_string(),
        },
        Type::Array(_) => "array types must use CBuffer or CArrayView at the FFI boundary".to_string(),
        Type::Complex(_) => "only primitive types are permitted at the FFI boundary".to_string(),
        Type::Function { .. } => "function types are not permitted at the FFI boundary".to_string(),
    }
}
