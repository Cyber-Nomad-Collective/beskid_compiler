use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::resolve::Resolution;
use crate::hir::{HirContractNode, HirItem, HirProgram, HirType};
use crate::syntax::Spanned;
use std::collections::{HashMap, HashSet};

impl SemanticPipelineRule {
    pub(super) fn stage6_contracts_and_methods(
        &self,
        ctx: &mut RuleContext,
        hir: &Spanned<HirProgram>,
        resolution: &Resolution,
    ) {
        let contracts = self.collect_contract_signatures(hir);

        for (type_item_id, conformances) in &resolution.tables.type_conformances {
            let Some(type_name) = resolution.items.get(type_item_id.0).map(|item| item.name.clone())
            else {
                continue;
            };
            for (contract_item_id, conformance_span) in conformances {
                let Some(contract_name) = resolution
                    .items
                    .get(contract_item_id.0)
                    .map(|item| item.name.clone())
                else {
                    continue;
                };
                let Some(expected_methods) = contracts.get(&contract_name) else {
                    continue;
                };
                for (method_name, expected) in expected_methods {
                    let actual =
                        self.impl_method_signature_for_type(hir, &type_name, method_name.as_str());
                    let Some(actual) = actual else {
                        ctx.emit_issue(
                            *conformance_span,
                            SemanticIssueKind::ContractMethodMissingImplementation {
                                contract_name: contract_name.clone(),
                                method_name: method_name.clone(),
                                expected: expected.clone(),
                            },
                        );
                        continue;
                    };
                    if &actual != expected {
                        ctx.emit_issue(
                            *conformance_span,
                            SemanticIssueKind::ContractImplementationSignatureMismatch {
                                method_name: method_name.clone(),
                                expected: expected.clone(),
                                actual,
                            },
                        );
                    }
                }
            }
        }
    }

    fn collect_contract_signatures(
        &self,
        hir: &Spanned<HirProgram>,
    ) -> HashMap<String, HashMap<String, String>> {
        let definitions: HashMap<String, &Spanned<crate::hir::HirContractDefinition>> = hir
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                HirItem::ContractDefinition(definition) if definition.node.extern_interface.is_none() => {
                    Some((definition.node.name.node.name.clone(), definition))
                }
                _ => None,
            })
            .collect();

        let mut cache = HashMap::new();
        for contract_name in definitions.keys() {
            let _ = self.collect_contract_methods_recursive(
                contract_name,
                &definitions,
                &mut cache,
                &mut HashSet::new(),
            );
        }
        cache
    }

    fn collect_contract_methods_recursive(
        &self,
        contract_name: &str,
        definitions: &HashMap<String, &Spanned<crate::hir::HirContractDefinition>>,
        cache: &mut HashMap<String, HashMap<String, String>>,
        active: &mut HashSet<String>,
    ) -> HashMap<String, String> {
        if let Some(cached) = cache.get(contract_name) {
            return cached.clone();
        }
        if !active.insert(contract_name.to_string()) {
            return HashMap::new();
        }

        let mut methods = HashMap::new();
        let Some(definition) = definitions.get(contract_name) else {
            active.remove(contract_name);
            return methods;
        };

        for node in &definition.node.items {
            match &node.node {
                HirContractNode::MethodSignature(signature) => {
                    methods.insert(
                        signature.node.name.node.name.clone(),
                        self.method_signature_string(
                            signature.node.parameters.len(),
                            signature.node.return_type.is_some(),
                        ),
                    );
                }
                HirContractNode::Embedding(embedding) => {
                    let embedded_name = embedding.node.name.node.name.clone();
                    let embedded = self.collect_contract_methods_recursive(
                        embedded_name.as_str(),
                        definitions,
                        cache,
                        active,
                    );
                    for (method_name, signature) in embedded {
                        methods.entry(method_name).or_insert(signature);
                    }
                }
            }
        }

        active.remove(contract_name);
        cache.insert(contract_name.to_string(), methods.clone());
        methods
    }

    fn impl_method_signature_for_type(
        &self,
        hir: &Spanned<HirProgram>,
        type_name: &str,
        contract_name: &str,
    ) -> Option<String> {
        for item in &hir.node.items {
            let HirItem::MethodDefinition(method) = &item.node else {
                continue;
            };
            let HirType::Complex(receiver_path) = &method.node.receiver_type.node else {
                continue;
            };
            let Some(receiver_name) = receiver_path
                .node
                .segments
                .last()
                .map(|segment| segment.node.name.node.name.as_str())
            else {
                continue;
            };
            if receiver_name == type_name && method.node.name.node.name == contract_name {
                return Some(self.method_signature_string(
                    method.node.parameters.len(),
                    method.node.return_type.is_some(),
                ));
            }
        }
        None
    }

    fn method_signature_string(&self, parameter_count: usize, has_return_type: bool) -> String {
        let return_marker = if has_return_type { "ret" } else { "unit" };
        format!("{return_marker}({parameter_count})")
    }
}
