//! Registration shape checks: every registration that passes analysis is one the backend can
//! create. The container allocates a registered service itself, without a constructor, and fills
//! only its `inject` fields; so it must be a non-generic `type` without value fields, and a
//! `for Contract` registration must name a type that conforms to the contract.
//!
//! The checks use the declarations of the unit under analysis. A registration naming a type that
//! the unit does not declare (an imported one) is left to the resolver and the backend.

use std::collections::{HashMap, HashSet};

use super::diagnostics::{CompositionIssue, UnconstructibleReason};
use super::model::{Registration, RegistrationKey};
use crate::syntax::{ContractNode, FieldKind, Node, Program, Spanned, Type};

#[derive(Debug, Clone)]
enum Declaration {
    Contract { embeds: Vec<String> },
    Type { conformances: Vec<String>, value_field: Option<String>, generic: bool },
    Enum,
}

fn tail(path: &Spanned<crate::syntax::Path>) -> String {
    path.node.segments.last().map(|segment| segment.node.name.node.name.clone()).unwrap_or_default()
}

fn declarations(program: &Spanned<Program>) -> HashMap<String, Declaration> {
    let mut found = HashMap::new();
    let mut extra_conformances: HashMap<String, Vec<String>> = HashMap::new();
    collect_items(&program.node.items, &mut found, &mut extra_conformances);
    for (name, conformances) in extra_conformances {
        if let Some(Declaration::Type { conformances: existing, .. }) = found.get_mut(&name) {
            existing.extend(conformances);
        }
    }
    found
}

fn collect_items(
    items: &[Spanned<Node>],
    found: &mut HashMap<String, Declaration>,
    extra_conformances: &mut HashMap<String, Vec<String>>,
) {
    for item in items {
        match &item.node {
            Node::ContractDefinition(contract) => {
                let embeds = contract
                    .node
                    .items
                    .iter()
                    .filter_map(|node| match &node.node {
                        ContractNode::Embedding(embedding) => Some(embedding.node.name.node.name.clone()),
                        _ => None,
                    })
                    .collect();
                found.insert(contract.node.name.node.name.clone(), Declaration::Contract { embeds });
            }
            Node::TypeDefinition(definition) => {
                let value_field = definition
                    .node
                    .fields
                    .iter()
                    .find(|field| field.node.kind == FieldKind::Value)
                    .map(|field| field.node.name.node.name.clone());
                found.insert(
                    definition.node.name.node.name.clone(),
                    Declaration::Type {
                        conformances: definition.node.conformances.iter().map(tail).collect(),
                        value_field,
                        generic: !definition.node.generics.is_empty(),
                    },
                );
            }
            Node::EnumDefinition(definition) => {
                found.insert(definition.node.name.node.name.clone(), Declaration::Enum);
            }
            Node::ImplBlock(block) => {
                if let Type::Complex(path) = &block.node.receiver_type.node {
                    extra_conformances.entry(tail(path)).or_default().extend(block.node.conformances.iter().map(tail));
                }
            }
            Node::InlineModule(module) => collect_items(&module.node.items, found, extra_conformances),
            _ => {}
        }
    }
}

/// `Some(true)` when `type_name` conforms to `contract`, `Some(false)` when the unit proves it
/// does not, `None` when a conformance leaves the unit (an imported contract may embed it).
fn conforms(declarations: &HashMap<String, Declaration>, type_name: &str, contract: &str) -> Option<bool> {
    let Some(Declaration::Type { conformances, .. }) = declarations.get(type_name) else {
        return None;
    };
    let mut pending = conformances.clone();
    let mut seen = HashSet::new();
    let mut unknown = false;
    while let Some(name) = pending.pop() {
        if name == contract {
            return Some(true);
        }
        if !seen.insert(name.clone()) {
            continue;
        }
        match declarations.get(&name) {
            Some(Declaration::Contract { embeds }) => pending.extend(embeds.iter().cloned()),
            _ => unknown = true,
        }
    }
    if unknown { None } else { Some(false) }
}

fn last_segment(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

/// Check every merged registration against the unit's declarations.
pub fn validate_registrations(registrations: &[Registration], program: &Spanned<Program>) -> Vec<CompositionIssue> {
    let declarations = declarations(program);
    let mut issues = Vec::new();
    for registration in registrations {
        if super::hooks::is_hook_item_name(&registration.implementation) {
            continue;
        }
        let implementation = last_segment(&registration.implementation);
        match declarations.get(implementation) {
            Some(Declaration::Contract { .. }) => {
                let mut candidates = declarations
                    .keys()
                    .filter(|name| conforms(&declarations, name, implementation) == Some(true))
                    .filter(|name| {
                        matches!(
                            declarations.get(name.as_str()),
                            Some(Declaration::Type { value_field: None, generic: false, .. })
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                candidates.sort();
                issues.push(CompositionIssue::RegisteredContract {
                    contract: implementation.to_string(),
                    target: match &registration.key {
                        RegistrationKey::Contract(target) => Some(target.clone()),
                        RegistrationKey::SelfType(_) => None,
                    },
                    span: registration.span,
                    implementation_span: registration.implementation_span,
                    candidates,
                });
            }
            Some(Declaration::Enum) => issues.push(CompositionIssue::UnconstructibleRegistration {
                implementation: implementation.to_string(),
                reason: UnconstructibleReason::Enum,
                span: registration.implementation_span,
            }),
            Some(Declaration::Type { generic: true, .. }) => issues.push(CompositionIssue::UnconstructibleRegistration {
                implementation: implementation.to_string(),
                reason: UnconstructibleReason::Generic,
                span: registration.implementation_span,
            }),
            Some(Declaration::Type { value_field: Some(field), .. }) => {
                issues.push(CompositionIssue::UnconstructibleRegistration {
                    implementation: implementation.to_string(),
                    reason: UnconstructibleReason::ValueField(field.clone()),
                    span: registration.implementation_span,
                })
            }
            Some(Declaration::Type { .. }) => {
                if let RegistrationKey::Contract(contract) = &registration.key {
                    let contract = last_segment(contract);
                    if matches!(declarations.get(contract), Some(Declaration::Contract { .. }))
                        && conforms(&declarations, implementation, contract) == Some(false)
                    {
                        issues.push(CompositionIssue::NonConformingRegistration {
                            implementation: implementation.to_string(),
                            contract: contract.to_string(),
                            span: registration.implementation_span,
                        });
                    }
                }
            }
            None => {}
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use crate::composition::{CompositionInput, CompositionIssue, lower_host_hooks, resolve_composition};
    use crate::services::parse_program;

    fn issues(source: &str) -> Vec<CompositionIssue> {
        let program = lower_host_hooks(parse_program(source).expect("parse"));
        resolve_composition(CompositionInput { program: &program, is_mod_project: false }).issues
    }

    #[test]
    fn a_contract_registered_as_itself_names_the_conforming_types() {
        let found = issues(
            r#"
contract Greeter { string Greet(); }
type PoliteGreeter : Greeter { pub string Greet() { return "hello"; } }
type Reception { inject Greeter greeter }
host AppHost() { registry { single Greeter; single Reception; } }
i32 Main() { launch AppHost(); return 0; }
"#,
        );
        let registered = found
            .iter()
            .find_map(|issue| match issue {
                CompositionIssue::RegisteredContract { contract, candidates, .. } => Some((contract, candidates)),
                _ => None,
            })
            .unwrap_or_else(|| panic!("contract registration must be rejected: {found:?}"));
        assert_eq!(registered.0, "Greeter");
        assert_eq!(registered.1, &vec!["PoliteGreeter".to_string()]);
    }

    #[test]
    fn value_fields_generics_enums_and_non_conforming_types_are_rejected() {
        let found = issues(
            r#"
contract Clock { i64 Now(); }
type Prefixed { string prefix }
type Boxed<T> { }
enum Mode { Fast(), Slow(), }
type Plain { }
host AppHost()
{
    registry { single Prefixed; single Boxed; single Mode; single Plain for Clock; }
}
i32 Main() { launch AppHost(); return 0; }
"#,
        );
        let unconstructible =
            found.iter().filter(|issue| matches!(issue, CompositionIssue::UnconstructibleRegistration { .. })).count();
        assert_eq!(unconstructible, 3, "{found:?}");
        assert!(
            found.iter().any(|issue| matches!(issue, CompositionIssue::NonConformingRegistration { .. })),
            "{found:?}"
        );
    }

    #[test]
    fn with_needs_a_launch_before_it_in_the_same_body() {
        let found = issues(
            r#"
host AppHost() { scope Visit() { } }
unit Visit() { with Visit() { } }
i32 Main(bool flag)
{
    if flag { launch AppHost(); }
    with Visit() { }
    return 0;
}
"#,
        );
        let misplaced = found.iter().filter(|issue| matches!(issue, CompositionIssue::WithWithoutLaunch { .. })).count();
        assert_eq!(misplaced, 2, "{found:?}");
    }
}
