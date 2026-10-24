//! Registration shape checks: every registration that passes analysis is one the backend can
//! create. The container allocates a registered service itself, without a constructor, and fills
//! only its `inject` fields; so it must be a non-generic `type` without value fields, and a
//! `for Contract` registration must name a type that conforms to the contract.
//!
//! The checks cover every merged registration, including those of base hosts in imported units;
//! an issue is reported in the unit that declares the registry line.

use std::collections::{HashMap, HashSet};

use super::diagnostics::{CompositionIssue, UnconstructibleReason};
use super::model::{Registration, RegistrationKey, UnitPath};
use super::types::{CompositionTypeResolver, TypeIdentity};
use crate::syntax::{ContractNode, FieldKind, Identifier, Node, Path, PathSegment, Program, Spanned, Type};

#[derive(Debug, Clone)]
enum Declaration {
    Contract { name: String, embeds: Vec<Option<TypeIdentity>> },
    Type { name: String, conformances: Vec<Option<TypeIdentity>>, value_field: Option<String>, generic: bool },
    Enum { name: String },
}

impl Declaration {
    fn name(&self) -> &str {
        match self {
            Self::Contract { name, .. } | Self::Type { name, .. } | Self::Enum { name } => name,
        }
    }
}

/// The type, contract and enum declarations of every unit, keyed by their identity; each
/// conformance and embedding is resolved in the unit that writes it.
fn declarations(
    program: &Spanned<Program>,
    imported: &[(std::sync::Arc<std::path::Path>, &Spanned<Program>)],
    types: &dyn CompositionTypeResolver,
) -> HashMap<TypeIdentity, Declaration> {
    let mut found = HashMap::new();
    let mut extra_conformances: HashMap<TypeIdentity, Vec<Option<TypeIdentity>>> = HashMap::new();
    collect_items(&program.node.items, &None, types, &mut found, &mut extra_conformances);
    for (path, unit) in imported {
        collect_items(&unit.node.items, &Some(std::sync::Arc::clone(path)), types, &mut found, &mut extra_conformances);
    }
    for (declaration, conformances) in extra_conformances {
        if let Some(Declaration::Type { conformances: existing, .. }) = found.get_mut(&declaration) {
            existing.extend(conformances);
        }
    }
    found
}

fn collect_items(
    items: &[Spanned<Node>],
    unit: &UnitPath,
    types: &dyn CompositionTypeResolver,
    found: &mut HashMap<TypeIdentity, Declaration>,
    extra_conformances: &mut HashMap<TypeIdentity, Vec<Option<TypeIdentity>>>,
) {
    let resolve = |path: &Spanned<Path>| super::types::path_identity(types, unit, path);
    let identity =
        |name: &Spanned<Identifier>| TypeIdentity { unit: unit.clone(), declaration: name.span, arguments: Vec::new() };
    for item in items {
        match &item.node {
            Node::ContractDefinition(contract) => {
                let embeds = contract
                    .node
                    .items
                    .iter()
                    .filter_map(|node| match &node.node {
                        ContractNode::Embedding(embedding) => {
                            Some(resolve(&single_path(&embedding.node.name, &embedding.node.type_args)))
                        }
                        _ => None,
                    })
                    .collect();
                found.insert(
                    identity(&contract.node.name),
                    Declaration::Contract { name: contract.node.name.node.name.clone(), embeds },
                );
            }
            Node::TypeDefinition(definition) => {
                let value_field = definition
                    .node
                    .fields
                    .iter()
                    .find(|field| field.node.kind == FieldKind::Value)
                    .map(|field| field.node.name.node.name.clone());
                found.insert(
                    identity(&definition.node.name),
                    Declaration::Type {
                        name: definition.node.name.node.name.clone(),
                        conformances: definition.node.conformances.iter().map(resolve).collect(),
                        value_field,
                        generic: !definition.node.generics.is_empty(),
                    },
                );
            }
            Node::EnumDefinition(definition) => {
                found.insert(
                    identity(&definition.node.name),
                    Declaration::Enum { name: definition.node.name.node.name.clone() },
                );
            }
            Node::ImplBlock(block) => {
                if let Type::Complex(path) = &block.node.receiver_type.node
                    && let Some(receiver) = resolve(path)
                {
                    extra_conformances.entry(receiver.declaration_only()).or_default().extend(block.node.conformances.iter().map(resolve));
                }
            }
            Node::InlineModule(module) => collect_items(&module.node.items, unit, types, found, extra_conformances),
            _ => {}
        }
    }
}

/// A one-segment path naming `name` with `type_args` at the name's span (a contract embedding is
/// written as a name and its type arguments).
fn single_path(name: &Spanned<Identifier>, type_args: &[Spanned<Type>]) -> Spanned<Path> {
    Spanned::new(
        Path {
            segments: vec![Spanned::new(PathSegment { name: name.clone(), type_args: type_args.to_vec() }, name.span)],
        },
        name.span,
    )
}

/// `Some(true)` when `type_id` conforms to `contract`, `Some(false)` when the declarations prove
/// it does not, `None` when a conformance names no declaration the assembly holds.
fn conforms(declarations: &HashMap<TypeIdentity, Declaration>, type_id: &TypeIdentity, contract: &TypeIdentity) -> Option<bool> {
    let Some(Declaration::Type { conformances, .. }) = declarations.get(&type_id.declaration_only()) else {
        return None;
    };
    let mut pending = conformances.clone();
    let mut seen = HashSet::new();
    let mut unknown = false;
    while let Some(next) = pending.pop() {
        let Some(next) = next else {
            unknown = true;
            continue;
        };
        if &next == contract {
            return Some(true);
        }
        if !seen.insert(next.clone()) {
            continue;
        }
        match declarations.get(&next.declaration_only()) {
            Some(Declaration::Contract { embeds, .. }) => pending.extend(embeds.iter().cloned()),
            _ => unknown = true,
        }
    }
    if unknown { None } else { Some(false) }
}

/// Check every merged registration against the declaration its paths name. A registration whose
/// implementation names no declaration is left to the type checker, which reports the path.
pub fn validate_registrations(
    registrations: &[Registration],
    program: &Spanned<Program>,
    imported: &[(std::sync::Arc<std::path::Path>, &Spanned<Program>)],
    types: &dyn CompositionTypeResolver,
) -> Vec<(UnitPath, CompositionIssue)> {
    let declarations = declarations(program, imported, types);
    let mut located = Vec::new();
    for registration in registrations {
        // Generated hook arguments are always constructible.
        if super::hooks::is_hook_item_name(&registration.implementation) {
            continue;
        }
        let Some(implementation_id) = &registration.implementation_identity else {
            continue;
        };
        let Some(declaration) = declarations.get(&implementation_id.declaration_only()) else {
            continue;
        };
        let implementation = declaration.name().to_string();
        let issue = match declaration {
            Declaration::Contract { .. } => {
                let mut candidates = declarations
                    .iter()
                    .filter(|(id, candidate)| {
                        matches!(candidate, Declaration::Type { value_field: None, generic: false, .. })
                            && conforms(&declarations, id, implementation_id) == Some(true)
                    })
                    .map(|(_, candidate)| candidate.name().to_string())
                    .collect::<Vec<_>>();
                candidates.sort();
                candidates.dedup();
                Some(CompositionIssue::RegisteredContract {
                    contract: implementation,
                    target: match &registration.key {
                        RegistrationKey::Contract(target) => Some(target.clone()),
                        RegistrationKey::SelfType(_) => None,
                    },
                    span: registration.span,
                    implementation_span: registration.implementation_span,
                    candidates,
                })
            }
            Declaration::Enum { .. } => Some(CompositionIssue::UnconstructibleRegistration {
                implementation,
                reason: UnconstructibleReason::Enum,
                span: registration.implementation_span,
            }),
            Declaration::Type { generic: true, .. } => Some(CompositionIssue::UnconstructibleRegistration {
                implementation,
                reason: UnconstructibleReason::Generic,
                span: registration.implementation_span,
            }),
            Declaration::Type { value_field: Some(field), .. } => Some(CompositionIssue::UnconstructibleRegistration {
                implementation,
                reason: UnconstructibleReason::ValueField(field.clone()),
                span: registration.implementation_span,
            }),
            Declaration::Type { .. } => match (&registration.key, &registration.key_identity) {
                (RegistrationKey::Contract(_), Some(contract_id)) => match declarations.get(&contract_id.declaration_only()) {
                    Some(Declaration::Contract { name, .. })
                        if conforms(&declarations, implementation_id, contract_id) == Some(false) =>
                    {
                        Some(CompositionIssue::NonConformingRegistration {
                            implementation,
                            contract: name.clone(),
                            span: registration.implementation_span,
                        })
                    }
                    _ => None,
                },
                _ => None,
            },
        };
        located.extend(issue.map(|issue| (registration.unit.clone(), issue)));
    }
    located
}

#[cfg(test)]
mod tests {
    use crate::composition::{CompositionInput, CompositionIssue, lower_host_hooks, resolve_composition};
    use crate::services::parse_program;

    fn issues(source: &str) -> Vec<CompositionIssue> {
        let program = lower_host_hooks(parse_program(source).expect("parse"));
        let types = crate::composition::types::test_types::DeclaredTypes::new(&program, &[]);
        resolve_composition(CompositionInput { program: &program, is_mod_project: false, types: &types }).issues
    }

    #[test]
    fn a_contract_registered_as_itself_names_the_conforming_types() {
        let found = issues(
            r#"
contract Greeter { string Greet(); }
type PoliteGreeter : Greeter { pub string Greet() { return "hello"; } }
type Reception { inject Greeter greeter; }
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
type Prefixed { string prefix; }
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
