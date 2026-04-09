use std::collections::{HashMap, HashSet};

use crate::builtins::{BuiltinType, builtin_specs};
use crate::hir::{HirContractNode, HirItem, HirPrimitiveType, HirProgram};
use crate::resolve::{ItemId, ItemKind, LocalId, Resolution, ResolvedType};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::{TypeId, TypeTable};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeError {
    UnknownType {
        span: SpanInfo,
    },
    UnknownValueType {
        span: SpanInfo,
    },
    UnknownStructType {
        span: SpanInfo,
    },
    InvalidMemberTarget {
        span: SpanInfo,
    },
    UnknownEnumType {
        span: SpanInfo,
    },
    UnknownStructField {
        span: SpanInfo,
        name: String,
    },
    UnknownEnumVariant {
        span: SpanInfo,
        name: String,
    },
    MissingStructField {
        span: SpanInfo,
        name: String,
    },
    MissingTypeAnnotation {
        span: SpanInfo,
        name: String,
    },
    TypeMismatch {
        span: SpanInfo,
        expected: TypeId,
        actual: TypeId,
    },
    MatchArmTypeMismatch {
        span: SpanInfo,
        expected: TypeId,
        actual: TypeId,
    },
    CallArityMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    CallArgumentMismatch {
        span: SpanInfo,
        expected: TypeId,
        actual: TypeId,
    },
    EnumConstructorMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    UnknownCallTarget {
        span: SpanInfo,
    },
    InvalidBinaryOp {
        span: SpanInfo,
    },
    InvalidUnaryOp {
        span: SpanInfo,
    },
    NonBoolCondition {
        span: SpanInfo,
    },
    UnsupportedExpression {
        span: SpanInfo,
    },
    InvalidTryTarget {
        span: SpanInfo,
    },
    InvalidEventInvocationScope {
        span: SpanInfo,
    },
    InvalidEventCapacity {
        span: SpanInfo,
    },
    InvalidEventSubscriptionTarget {
        span: SpanInfo,
    },
    ReturnTypeMismatch {
        span: SpanInfo,
        expected: TypeId,
        actual: Option<TypeId>,
    },
    MissingTypeArguments {
        span: SpanInfo,
    },
    GenericArgumentMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    NonIterableForTarget {
        span: SpanInfo,
    },
    IterableNextArityMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    IterableNextReturnNotOption {
        span: SpanInfo,
    },
    IterableOptionSomeArityMismatch {
        span: SpanInfo,
        expected: usize,
        actual: usize,
    },
    // Extern interface validation errors
    ExternInvalidAbi {
        span: SpanInfo,
        abi: Option<String>,
    },
    ExternMissingLibrary {
        span: SpanInfo,
    },
    ExternDisallowedParamType {
        span: SpanInfo,
        method: String,
    },
    ExternDisallowedReturnType {
        span: SpanInfo,
        method: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodReceiverSource {
    Expression(SpanInfo),
    Local(LocalId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallLoweringKind {
    MethodDispatch {
        method_item_id: ItemId,
        receiver_source: MethodReceiverSource,
        receiver_type: TypeId,
    },
    ContractDispatch {
        contract_item_id: ItemId,
        receiver_source: MethodReceiverSource,
        receiver_type: TypeId,
    },
    ItemCall {
        item_id: ItemId,
    },
    EventInvoke {
        receiver_source: MethodReceiverSource,
        receiver_type: TypeId,
    },
    CallableValueCall,
}

#[derive(Debug)]
pub struct TypeResult {
    pub types: TypeTable,
    pub named_type_names: HashMap<ItemId, String>,
    pub expr_types: HashMap<SpanInfo, TypeId>,
    pub local_types: HashMap<LocalId, TypeId>,
    pub function_signatures: HashMap<ItemId, FunctionSignature>,
    pub struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub generic_items: HashMap<ItemId, Vec<String>>,
    pub call_kinds: HashMap<SpanInfo, CallLoweringKind>,
    pub contract_method_order: HashMap<ItemId, Vec<String>>,
    pub contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    // Canonical output contract for safe implicit numeric conversions.
    // Invariants (normalized in `TypeContext::type_program`):
    // - sorted by (span.start, span.end, from, to)
    // - exact duplicates removed
    // - conflicting reverse intents for the same span are rejected upstream
    pub cast_intents: Vec<CastIntent>,
}

impl TypeResult {
    pub fn cast_intent_for_span(&self, span: SpanInfo) -> Option<&CastIntent> {
        self.cast_intents.iter().find(|intent| intent.span == span)
    }

    pub fn cast_intents_for_span(&self, span: SpanInfo) -> impl Iterator<Item = &CastIntent> {
        self.cast_intents
            .iter()
            .filter(move |intent| intent.span == span)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    pub params: Vec<TypeId>,
    pub return_type: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CastIntent {
    pub span: SpanInfo,
    pub from: TypeId,
    pub to: TypeId,
}

pub struct TypeContext<'a> {
    pub(super) resolution: &'a Resolution,
    pub(super) type_table: TypeTable,
    pub(super) primitive_types: HashMap<HirPrimitiveType, TypeId>,
    pub(super) named_types: HashMap<ItemId, TypeId>,
    pub(super) struct_fields: HashMap<ItemId, HashMap<String, TypeId>>,
    pub(super) struct_fields_ordered: HashMap<ItemId, Vec<(String, TypeId)>>,
    pub(super) struct_event_fields: HashMap<ItemId, HashMap<String, Option<usize>>>,
    pub(super) enum_variants: HashMap<ItemId, HashMap<String, Vec<TypeId>>>,
    pub(super) enum_variants_ordered: HashMap<ItemId, Vec<(String, Vec<TypeId>)>>,
    pub(super) expr_types: HashMap<SpanInfo, TypeId>,
    pub(super) local_types: HashMap<LocalId, TypeId>,
    pub(super) function_signatures: HashMap<ItemId, FunctionSignature>,
    pub(super) cast_intents: Vec<CastIntent>,
    pub(super) errors: Vec<TypeError>,
    pub(super) current_return_type: Option<TypeId>,
    pub(super) generic_params: HashMap<String, TypeId>,
    pub(super) generic_items: HashMap<ItemId, Vec<String>>,
    pub(super) call_kinds: HashMap<SpanInfo, CallLoweringKind>,
    pub(super) methods_by_receiver: HashMap<(ItemId, String), ItemId>,
    pub(super) contract_method_order: HashMap<ItemId, Vec<String>>,
    pub(super) contract_signatures: HashMap<(ItemId, String), FunctionSignature>,
    pub(super) current_receiver_item_id: Option<ItemId>,
}

impl<'a> TypeContext<'a> {
    pub fn new(resolution: &'a Resolution) -> Self {
        let mut context = Self {
            resolution,
            type_table: TypeTable::new(),
            primitive_types: HashMap::new(),
            named_types: HashMap::new(),
            struct_fields: HashMap::new(),
            struct_fields_ordered: HashMap::new(),
            struct_event_fields: HashMap::new(),
            enum_variants: HashMap::new(),
            enum_variants_ordered: HashMap::new(),
            expr_types: HashMap::new(),
            local_types: HashMap::new(),
            function_signatures: HashMap::new(),
            cast_intents: Vec::new(),
            errors: Vec::new(),
            current_return_type: None,
            generic_params: HashMap::new(),
            generic_items: HashMap::new(),
            call_kinds: HashMap::new(),
            methods_by_receiver: HashMap::new(),
            contract_method_order: HashMap::new(),
            contract_signatures: HashMap::new(),
            current_receiver_item_id: None,
        };
        context.seed_types();
        context.seed_builtin_signatures();
        context
    }

    fn seed_builtin_signatures(&mut self) {
        for (item_id, index) in &self.resolution.builtin_items {
            let Some(spec) = builtin_specs().get(*index) else {
                continue;
            };
            let mut params = Vec::with_capacity(spec.params.len());
            for param in spec.params {
                if let Some(type_id) = self.builtin_type_id(*param) {
                    params.push(type_id);
                }
            }
            let return_type = self.builtin_type_id(spec.returns);
            let Some(return_type) = return_type else {
                continue;
            };
            self.function_signatures.insert(
                *item_id,
                FunctionSignature {
                    params,
                    return_type,
                },
            );
        }
    }

    fn builtin_type_id(&self, builtin: BuiltinType) -> Option<TypeId> {
        match builtin {
            BuiltinType::String => self.primitive_type_id(HirPrimitiveType::String),
            BuiltinType::Unit => self.primitive_type_id(HirPrimitiveType::Unit),
            BuiltinType::Never => self.primitive_type_id(HirPrimitiveType::Never),
            BuiltinType::Usize | BuiltinType::U64 => self.primitive_type_id(HirPrimitiveType::I64),
            BuiltinType::Ptr => None,
        }
    }

    pub fn type_program(self, program: &Spanned<HirProgram>) -> Result<TypeResult, Vec<TypeError>> {
        let (result, errors) = self.type_program_with_errors(program);
        if errors.is_empty() {
            Ok(result)
        } else {
            Err(errors)
        }
    }

    pub fn type_program_with_errors(
        mut self,
        program: &Spanned<HirProgram>,
    ) -> (TypeResult, Vec<TypeError>) {
        for item in &program.node.items {
            let (span, generics) = match &item.node {
                HirItem::FunctionDefinition(def) => (item.span, &def.node.generics),
                HirItem::TypeDefinition(def) => (item.span, &def.node.generics),
                HirItem::EnumDefinition(def) => (item.span, &def.node.generics),
                _ => continue,
            };
            if let Some(item_id) = self.item_id_for_span(span) {
                let names = generics
                    .iter()
                    .map(|generic| generic.node.name.clone())
                    .collect::<Vec<_>>();
                self.generic_items.insert(item_id, names);
            }
        }
        self.seed_contract_signatures(program);
        for item in &program.node.items {
            let HirItem::MethodDefinition(def) = &item.node else {
                continue;
            };
            let Some(method_item_id) = self.item_id_for_span(item.span) else {
                continue;
            };
            let Some(ResolvedType::Item(receiver_item_id)) = self
                .resolution
                .tables
                .resolved_types
                .get(&def.node.receiver_type.span)
            else {
                continue;
            };
            self.methods_by_receiver.insert(
                (*receiver_item_id, def.node.name.node.name.clone()),
                method_item_id,
            );
        }
        for item in &program.node.items {
            self.type_item(item);
        }
        self.cast_intents.sort_by_key(|intent| {
            (
                intent.span.start,
                intent.span.end,
                intent.from.0,
                intent.to.0,
            )
        });
        self.cast_intents.dedup_by(|left, right| {
            left.span == right.span && left.from == right.from && left.to == right.to
        });
        let result = TypeResult {
            types: self.type_table,
            named_type_names: self
                .resolution
                .items
                .iter()
                .map(|item| (item.id, item.name.clone()))
                .collect(),
            expr_types: self.expr_types,
            local_types: self.local_types,
            function_signatures: self.function_signatures,
            struct_fields_ordered: self.struct_fields_ordered,
            struct_event_fields: self.struct_event_fields,
            enum_variants_ordered: self.enum_variants_ordered,
            generic_items: self.generic_items,
            call_kinds: self.call_kinds,
            contract_method_order: self.contract_method_order,
            contract_signatures: self.contract_signatures,
            cast_intents: self.cast_intents,
        };
        let errors = std::mem::take(&mut self.errors);
        (result, errors)
    }

    fn seed_contract_signatures(&mut self, program: &Spanned<HirProgram>) {
        let definitions: HashMap<String, &Spanned<crate::hir::HirContractDefinition>> = program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                HirItem::ContractDefinition(def) => Some((def.node.name.node.name.clone(), def)),
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
            let Some(contract_item_id) = self.item_id_for_name(&contract_name, ItemKind::Contract)
            else {
                continue;
            };
            self.contract_method_order.insert(
                contract_item_id,
                signatures.iter().map(|(name, _)| name.clone()).collect(),
            );
            for (method_name, signature) in signatures {
                self.contract_signatures
                    .insert((contract_item_id, method_name), signature);
            }

            // If this contract has an extern interface, perform static validation.
            if let Some(def) = definitions.get(&contract_name) {
                if let Some(ext) = &def.node.extern_interface {
                    // ABI must be exactly "C"
                    let abi_ok = ext
                        .abi
                        .as_ref()
                        .map(|s| s.eq_ignore_ascii_case("C"))
                        .unwrap_or(false);
                    if !abi_ok {
                        self.errors.push(TypeError::ExternInvalidAbi {
                            span: def.node.name.span,
                            abi: ext.abi.clone(),
                        });
                    }
                    // Library must be present and non-empty
                    let lib_ok = ext
                        .library
                        .as_ref()
                        .map(|s| !s.trim().is_empty())
                        .unwrap_or(false);
                    if !lib_ok {
                        self.errors.push(TypeError::ExternMissingLibrary {
                            span: def.node.name.span,
                        });
                    }

                    // Validate method signatures declared directly in this contract
                    for node in &def.node.items {
                        if let HirContractNode::MethodSignature(sig) = &node.node {
                            // Params
                            for param in &sig.node.parameters {
                                if !self.is_allowed_ffi_param(param) {
                                    self.errors.push(TypeError::ExternDisallowedParamType {
                                        span: param.span,
                                        method: sig.node.name.node.name.clone(),
                                    });
                                }
                            }
                            // Return type
                            if let Some(ret) = &sig.node.return_type {
                                if !self.is_allowed_ffi_return(ret) {
                                    self.errors.push(TypeError::ExternDisallowedReturnType {
                                        span: ret.span,
                                        method: sig.node.name.node.name.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn collect_contract_signatures_recursive(
        &mut self,
        contract_name: &str,
        definitions: &HashMap<String, &Spanned<crate::hir::HirContractDefinition>>,
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
                HirContractNode::MethodSignature(signature) => {
                    if methods
                        .iter()
                        .any(|(name, _)| name == &signature.node.name.node.name)
                    {
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
                        .or_else(|| self.primitive_type_id(HirPrimitiveType::Unit));
                    let Some(return_type) = return_type else {
                        continue;
                    };
                    methods.push((
                        signature.node.name.node.name.clone(),
                        FunctionSignature {
                            params,
                            return_type,
                        },
                    ));
                }
                HirContractNode::Embedding(embedding) => {
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

    fn is_allowed_ffi_primitive(prim: crate::hir::HirPrimitiveType) -> bool {
        use crate::hir::HirPrimitiveType::*;
        matches!(prim, Bool | U8 | I32 | I64 | F64)
    }

    fn is_allowed_ffi_param(&self, param: &Spanned<crate::hir::HirParameter>) -> bool {
        use crate::hir::{HirParameterModifier, HirPrimitiveType, HirType};
        match &param.node.modifier {
            Some(modif) if matches!(modif.node, HirParameterModifier::Ref) => {
                // Only allow ref u8
                match &param.node.ty.node {
                    HirType::Primitive(p) => matches!(p.node, HirPrimitiveType::U8),
                    _ => false,
                }
            }
            Some(_) => false, // disallow other modifiers (e.g., out) in v0.1
            None => match &param.node.ty.node {
                HirType::Primitive(p) => Self::is_allowed_ffi_primitive(p.node),
                _ => false,
            },
        }
    }

    fn is_allowed_ffi_return(&self, ret: &Spanned<crate::hir::HirType>) -> bool {
        // Allow: primitives (Bool, U8, I32, I64, F64), or Unit if unspecified upstream
        use crate::hir::{HirPrimitiveType, HirType};
        match &ret.node {
            HirType::Primitive(p) => {
                Self::is_allowed_ffi_primitive(p.node) || matches!(p.node, HirPrimitiveType::Unit)
            }
            _ => false,
        }
    }
}

pub fn type_program(
    program: &Spanned<HirProgram>,
    resolution: &Resolution,
) -> Result<TypeResult, Vec<TypeError>> {
    TypeContext::new(resolution).type_program(program)
}

pub fn type_program_with_errors(
    program: &Spanned<HirProgram>,
    resolution: &Resolution,
) -> (TypeResult, Vec<TypeError>) {
    TypeContext::new(resolution).type_program_with_errors(program)
}
