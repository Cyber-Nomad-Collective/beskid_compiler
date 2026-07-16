//! Extract `(contractId, typeId, entrySymbol)` tuples from a compiled Mod unit.
//!
//! Registrations are discovered from type conformances to SDK hub contracts
//! (`Beskid.Compiler.Collect.*`) and optional `[InternalSymbol]` metadata on
//! contract entry methods when present in the syntax tree.

use std::collections::HashMap;

use crate::resolve::{ItemId, ItemKind, Resolution, qualified_name};
use crate::syntax::{Node, Program, Spanned, TypeDefinition};

use super::types::ContractRegistration;

const _SDK_CONTRACT_HUB: &str = "Beskid.Compiler.Collect";

struct SdkContractSpec {
    contract_id: &'static str,
    entry_method: &'static str,
}

const SDK_MOD_CONTRACTS: &[SdkContractSpec] = &[
    SdkContractSpec {
        contract_id: "Beskid.Compiler.Collect.Collector",
        entry_method: "Collect",
    },
    SdkContractSpec {
        contract_id: "Beskid.Compiler.Collect.Generator",
        entry_method: "Generate",
    },
    SdkContractSpec {
        contract_id: "Beskid.Compiler.Collect.AttributeGenerator",
        entry_method: "Attributes",
    },
    SdkContractSpec {
        contract_id: "Beskid.Compiler.Collect.Analyzer",
        entry_method: "Analyze",
    },
    SdkContractSpec {
        contract_id: "Beskid.Compiler.Collect.Rewriter",
        entry_method: "Rewrite",
    },
    SdkContractSpec {
        contract_id: "Beskid.Compiler.Collect.GrammarGenerator",
        entry_method: "Generate",
    },
];

/// Discover mod SDK contract registrations from resolved type conformances.
pub fn extract_mod_contract_registrations(
    package_id: &str,
    resolution: &Resolution,
) -> Vec<ContractRegistration> {
    extract_mod_contract_registrations_with_program(package_id, resolution, None)
}

/// Discover Mod SDK registrations directly from parsed source syntax.
///
/// This is the syntax-only counterpart of [`extract_mod_contract_registrations`]. It keeps the
/// established short-name compatibility for self-contained Mod fixtures while accepting fully
/// qualified SDK contract paths when source imports expose them.
pub fn extract_mod_contract_registrations_from_syntax(
    package_id: &str,
    program: &Spanned<Program>,
) -> Vec<ContractRegistration> {
    let internal_symbols = collect_internal_symbol_entry_methods(program).unwrap_or_default();
    let mut registrations = Vec::new();
    collect_syntax_registrations_from_items(
        package_id,
        &program.node.items,
        &internal_symbols,
        &mut registrations,
    );
    registrations.sort_by(|left, right| {
        left.contract_id
            .cmp(&right.contract_id)
            .then_with(|| left.type_id.cmp(&right.type_id))
            .then_with(|| left.entry_symbol.cmp(&right.entry_symbol))
    });
    registrations.dedup();
    registrations
}

fn collect_syntax_registrations_from_items(
    package_id: &str,
    items: &[Spanned<Node>],
    internal_symbols: &HashMap<(String, String), String>,
    registrations: &mut Vec<ContractRegistration>,
) {
    for item in items {
        match &item.node {
            Node::TypeDefinition(definition) => {
                let type_name = &definition.node.name.node.name;
                for conformance in &definition.node.conformances {
                    let Some(spec) = sdk_contract_spec_for_path(&conformance.node) else {
                        continue;
                    };
                    let entry_symbol = internal_symbols
                        .get(&(type_name.clone(), spec.entry_method.to_string()))
                        .map(|method_name| mod_contract_entry_symbol(package_id, method_name))
                        .unwrap_or_else(|| {
                            mod_contract_entry_symbol(package_id, spec.entry_method)
                        });
                    registrations.push(ContractRegistration {
                        contract_id: spec.contract_id.to_string(),
                        type_id: format!("{package_id}.{type_name}"),
                        entry_symbol,
                    });
                }
            }
            Node::InlineModule(module) => collect_syntax_registrations_from_items(
                package_id,
                &module.node.items,
                internal_symbols,
                registrations,
            ),
            _ => {}
        }
    }
}

fn sdk_contract_spec_for_path(path: &crate::syntax::Path) -> Option<&'static SdkContractSpec> {
    let qualified = path
        .segments
        .iter()
        .map(|segment| segment.node.name.node.name.as_str())
        .collect::<Vec<_>>()
        .join(".");
    SDK_MOD_CONTRACTS.iter().find(|spec| {
        spec.contract_id == qualified
            || spec
                .contract_id
                .rsplit('.')
                .next()
                .is_some_and(|short| short == qualified)
    })
}

/// Like [`extract_mod_contract_registrations`] but accepts optional mod source for
/// `[InternalSymbol]` attribute scanning on contract entry methods.
pub fn extract_mod_contract_registrations_with_program(
    package_id: &str,
    resolution: &Resolution,
    program: Option<&Spanned<Program>>,
) -> Vec<ContractRegistration> {
    let mut contract_specs_by_id: HashMap<&str, &SdkContractSpec> = HashMap::new();
    let mut contract_specs_by_short: HashMap<&str, &SdkContractSpec> = HashMap::new();
    for spec in SDK_MOD_CONTRACTS {
        contract_specs_by_id.insert(spec.contract_id, spec);
        if let Some(short) = spec.contract_id.rsplit('.').next() {
            contract_specs_by_short.insert(short, spec);
        }
    }

    let mut contract_item_specs: HashMap<ItemId, &SdkContractSpec> = HashMap::new();
    for item in &resolution.items {
        if item.kind != ItemKind::Contract {
            continue;
        }
        if let Some(qualified) = qualified_name(resolution, item.id)
            && let Some(spec) = contract_specs_by_id.get(qualified.as_str())
        {
            contract_item_specs.insert(item.id, spec);
            continue;
        }
        if let Some(spec) = contract_specs_by_short.get(item.name.as_str()) {
            contract_item_specs.insert(item.id, spec);
        }
    }

    let internal_symbols = program
        .and_then(|program| collect_internal_symbol_entry_methods(program).ok())
        .unwrap_or_default();

    let mut registrations = Vec::new();
    for (type_item_id, conformances) in &resolution.tables.type_conformances {
        let Some(type_item) = resolution.items.get(type_item_id.0) else {
            continue;
        };
        if type_item.kind != ItemKind::Type {
            continue;
        }
        for (contract_item_id, _) in conformances {
            let Some(spec) = contract_item_specs.get(contract_item_id) else {
                continue;
            };
            let type_id = format!("{package_id}.{}", type_item.name);
            let entry_symbol = internal_symbols
                .get(&(type_item.name.clone(), spec.entry_method.to_string()))
                .map(|method_name| mod_contract_entry_symbol(package_id, method_name))
                .unwrap_or_else(|| mod_contract_entry_symbol(package_id, spec.entry_method));
            registrations.push(ContractRegistration {
                contract_id: spec.contract_id.to_string(),
                type_id,
                entry_symbol,
            });
        }
    }

    registrations.sort_by(|left, right| {
        left.contract_id
            .cmp(&right.contract_id)
            .then_with(|| left.type_id.cmp(&right.type_id))
            .then_with(|| left.entry_symbol.cmp(&right.entry_symbol))
    });
    registrations.dedup();
    registrations
}

fn collect_internal_symbol_entry_methods(
    program: &Spanned<Program>,
) -> Result<HashMap<(String, String), String>, ()> {
    let mut internal_symbols = HashMap::new();
    collect_internal_symbol_entry_methods_from_items(&program.node.items, &mut internal_symbols);
    Ok(internal_symbols)
}

fn collect_internal_symbol_entry_methods_from_items(
    items: &[Spanned<Node>],
    internal_symbols: &mut HashMap<(String, String), String>,
) {
    for item in items {
        match &item.node {
            Node::TypeDefinition(definition) => {
                collect_internal_symbol_entry_methods_from_type(&definition.node, internal_symbols);
            }
            Node::InlineModule(module) => {
                collect_internal_symbol_entry_methods_from_items(
                    &module.node.items,
                    internal_symbols,
                );
            }
            _ => {}
        }
    }
}

fn collect_internal_symbol_entry_methods_from_type(
    definition: &TypeDefinition,
    internal_symbols: &mut HashMap<(String, String), String>,
) {
    let type_name = definition.name.node.name.clone();
    for method in &definition.methods {
        if !method_has_attribute(&method.node.attributes, "InternalSymbol") {
            continue;
        }
        let method_name = method.node.name.node.name.clone();
        internal_symbols.insert((type_name.clone(), method_name.clone()), method_name);
    }
}

fn method_has_attribute(attributes: &[Spanned<crate::syntax::Attribute>], name: &str) -> bool {
    attributes
        .iter()
        .any(|attribute| attribute.node.name.node.name == name)
}

/// Stable AOT export symbol for a mod contract entrypoint (`{package}_{method}`).
pub fn mod_contract_entry_symbol(package_id: &str, entry_method: &str) -> String {
    let suffix = match entry_method {
        "Attributes" => "attribute".to_string(),
        other => other.to_ascii_lowercase(),
    };
    if package_id.is_empty() {
        return suffix;
    }
    format!("{}_{}", package_id.to_ascii_lowercase(), suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::{lower_normalize_resolve_type_spanned, parse_program_with_source_name};

    #[test]
    fn extracts_sdk_contract_registrations_from_type_conformances() {
        let source = r#"
type CollectRequest {}
type CollectTargetSet {}
contract Collector { CollectTargetSet Collect(CollectRequest request); }
type DemoCollect : Collector {
    CollectTargetSet Collect(CollectRequest request) {
        return CollectTargetSet {};
    }

}
"#;
        let program = parse_program_with_source_name("Mod.bd", source).expect("parse");
        let (_hir, resolution, _typed) =
            lower_normalize_resolve_type_spanned(&program).expect("lower mod registration fixture");
        let registrations =
            extract_mod_contract_registrations_with_program("DemoMod", &resolution, Some(&program));
        assert_eq!(registrations.len(), 1);
        assert_eq!(
            registrations[0].contract_id,
            "Beskid.Compiler.Collect.Collector"
        );
        assert_eq!(registrations[0].type_id, "DemoMod.DemoCollect");
        assert_eq!(registrations[0].entry_symbol, "demomod_collect");
    }

    #[test]
    fn extracts_sdk_contract_registrations_from_syntax_without_resolution() {
        let source = r#"
contract Collector { i32 Collect(i32 request); }
type DemoCollect : Collector {
    i32 Collect(i32 request) { return request; }
}
"#;
        let program = parse_program_with_source_name("Mod.bd", source).expect("parse");
        let registrations = extract_mod_contract_registrations_from_syntax("DemoMod", &program);
        assert_eq!(registrations.len(), 1);
        assert_eq!(
            registrations[0].contract_id,
            "Beskid.Compiler.Collect.Collector"
        );
        assert_eq!(registrations[0].type_id, "DemoMod.DemoCollect");
        assert_eq!(registrations[0].entry_symbol, "demomod_collect");
    }

    #[test]
    fn entry_symbol_maps_attributes_method_to_attribute_suffix() {
        assert_eq!(
            mod_contract_entry_symbol("SampleMod", "Attributes"),
            "samplemod_attribute"
        );
    }

    #[test]
    fn internal_symbol_attribute_overrides_entry_method_suffix() {
        let source = r#"
type CollectRequest {}
type CollectTargetSet {}
contract Collector { CollectTargetSet Collect(CollectRequest request); }
type DemoCollect : Collector {
    [InternalSymbol]
    CollectTargetSet Collect(CollectRequest request) => (CollectTargetSet {})
}
"#;
        let program = parse_program_with_source_name("Mod.bd", source).expect("parse");
        let symbols = collect_internal_symbol_entry_methods(&program).expect("scan");
        assert_eq!(
            symbols.get(&("DemoCollect".to_string(), "Collect".to_string())),
            Some(&"Collect".to_string())
        );
        let (_hir, resolution, _typed) =
            lower_normalize_resolve_type_spanned(&program).expect("lower mod registration fixture");
        let registrations =
            extract_mod_contract_registrations_with_program("DemoMod", &resolution, Some(&program));
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].entry_symbol, "demomod_collect");
    }

    #[test]
    fn expression_bodied_inline_method_parses_for_mod_registration_fixture() {
        let source = r#"
type CollectRequest {}
type CollectTargetSet {}
contract Collector { CollectTargetSet Collect(CollectRequest request); }
type DemoCollect : Collector {
    CollectTargetSet Collect(CollectRequest request) => (CollectTargetSet {})
}
"#;
        let program = parse_program_with_source_name("Mod.bd", source).expect("parse");
        let (_hir, resolution, _typed) =
            lower_normalize_resolve_type_spanned(&program).expect("lower");
        let registrations =
            extract_mod_contract_registrations_with_program("DemoMod", &resolution, Some(&program));
        assert_eq!(registrations.len(), 1);
    }

    #[test]
    fn expression_bodied_inline_method_allows_trailing_semicolon() {
        let source = r#"
pub type List<T> {
    pub List<T> Push(T value) => List<T> { storage: storage, count: count + 1 };
}
"#;
        parse_program_with_source_name("List.bd", source).expect("parse");
    }

    #[test]
    fn expression_bodied_method_with_qualified_generic_call_parses() {
        let source = r#"
pub type QueryState<T> {
    pub QueryState<T> Where(bool predicate) =>
        Query.Operators.Where<T>(this, predicate);
}
"#;
        parse_program_with_source_name("QueryState.bd", source).expect("parse");
    }
}
