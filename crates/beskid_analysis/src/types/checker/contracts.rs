use std::collections::{HashMap, HashSet};

use crate::hir::{
    HirContractNode, HirItem, HirPrimitiveType, HirProgram,
};
use crate::resolve::{ItemKind, ResolvedType};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::result::{FunctionSignature, TypeError};

use super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(super) fn seed_method_receiver(
        &mut self,
        method_span: SpanInfo,
        def: &Spanned<crate::hir::HirMethodDefinition>,
    ) {
        let Some(method_item_id) = self.item_id_for_span(method_span) else {
            return;
        };
        let Some(ResolvedType::Item(receiver_item_id)) =
            self.resolved_type_at(def.node.receiver_type.span)
        else {
            return;
        };
        self.methods_by_receiver.insert(
            (receiver_item_id, def.node.name.node.name.clone()),
            method_item_id,
        );
    }

    pub(super) fn seed_contract_signatures(&mut self, program: &Spanned<HirProgram>) {
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
            for (method_name, signature) in signatures {
                self.contract_signatures
                    .insert((contract_item_id, method_name), signature);
            }

            // If this contract has an extern interface, perform static validation.
            if let Some(def) = definitions.get(&contract_name)
                && let Some(ext) = &def.node.extern_interface
            {
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
                        if let Some(ret) = &sig.node.return_type
                            && !self.is_allowed_ffi_return(ret)
                        {
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
        use crate::hir::HirType;
        match &param.node.ty.node {
            HirType::Primitive(p) => Self::is_allowed_ffi_primitive(p.node),
            _ => false,
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
