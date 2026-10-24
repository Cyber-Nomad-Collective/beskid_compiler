use std::collections::HashMap;

use super::{
    collect::{collect_with_imports, dependency_requests},
    container::ServiceContainer,
    diagnostics::CompositionIssue,
    graph::{build_graph, topo_registration_order},
    host_chain::{build_host_chain, merge_host_registries, resolve_host_key},
    model::{ActivationPlanEntry, BindingPlan, CompositionHookPlan, PluralPlan, ScopeId, ServiceSlot, SingularPlan},
    resolve_inject::resolve_dependency_targets,
    scope_tree::{merge_host_scopes, scope_parent_map, validate_scope_tree},
    snapshot::CompositionSnapshot,
};
use crate::{
    syntax::{AstNodeId, Program, SpanInfo, Spanned},
    syntax_query::{NodeKind, SyntaxSnapshot},
};

#[derive(Clone, Copy)]
pub struct CompositionInput<'a> {
    /// Expanded syntax is the composition authority; this pass does not require syntax.
    pub program: &'a Spanned<Program>,
    pub is_mod_project: bool,
    /// The type authority every registry key, `inject` request and shape check resolves through.
    pub types: &'a dyn super::types::CompositionTypeResolver,
}

#[derive(Debug, Clone)]
pub struct CompositionResult {
    pub plan: BindingPlan,
    pub snapshot: CompositionSnapshot,
    /// Issues located in the unit under analysis.
    pub issues: Vec<CompositionIssue>,
    /// Issues located in an imported unit (a base host's registry, an imported service's
    /// `inject` field, a hook of an imported host), reported at that unit's own positions.
    pub foreign_issues: Vec<(std::sync::Arc<std::path::Path>, CompositionIssue)>,
    pub dependency_edges: Vec<(u32, u32)>,
}

pub fn resolve_composition(input: CompositionInput<'_>) -> CompositionResult {
    resolve_composition_with_imports(input, &[])
}

/// Resolve the composition of the unit under analysis, with the composition declarations of the
/// other units of its assembly (`imported`: unit path and expanded program) available as bases.
pub fn resolve_composition_with_imports(
    input: CompositionInput<'_>,
    imported: &[(std::sync::Arc<std::path::Path>, &Spanned<Program>)],
) -> CompositionResult {
    let collected = collect_with_imports(input.program, imported, input.types);
    let mut issues = Vec::new();

    if input.is_mod_project {
        for host in collected.hosts.values() {
            issues.push((host.unit.clone(), CompositionIssue::HostInModProject { span: host.span }));
        }
    }

    let (launch_host, launch_span) = match collected.launches.as_slice() {
        [] => {
            // Only a host this unit declares needs a `launch`; base hosts of imported units (a
            // library's or Corelib's `ConsoleHost`) do not make every program a composition root.
            if collected.hosts.values().any(|host| host.unit.is_none()) {
                issues.push((None, CompositionIssue::MissingLaunchHost { span: Some(input.program.span) }));
            }
            (String::new(), SpanInfo::default())
        }
        [single] => (single.host_name.clone(), single.span),
        [first, second, ..] => {
            issues.push((
                None,
                CompositionIssue::MultipleLaunchHosts { first_span: Some(first.span), second_span: Some(second.span) },
            ));
            (first.host_name.clone(), first.span)
        }
    };

    let host_chain = if launch_host.is_empty() {
        Vec::new()
    } else {
        match build_host_chain(&collected.hosts, &launch_host, launch_span) {
            Ok(chain) => chain,
            Err(issue) => {
                // A cycle is reported at the host that closes it, in that host's unit.
                let unit = match &issue {
                    CompositionIssue::HostInheritanceCycle { host_name, .. } => {
                        collected.hosts.get(host_name).and_then(|host| host.unit.clone())
                    }
                    _ => None,
                };
                issues.push((unit, issue));
                Vec::new()
            }
        }
    };

    let merged_scopes = merge_host_scopes(&host_chain, &collected.host_scopes);
    for issue in validate_scope_tree(&merged_scopes) {
        let unit = match &issue {
            CompositionIssue::DuplicateScopeName { span, .. } | CompositionIssue::UnknownParentScope { span, .. } => {
                merged_scopes.iter().find(|scope| scope.span == *span).and_then(|scope| scope.unit.clone())
            }
            _ => None,
        };
        issues.push((unit, issue));
    }
    for with_site in &collected.with_sites {
        if !with_site.after_launch {
            issues.push((
                None,
                CompositionIssue::WithWithoutLaunch { scope_name: with_site.scope_name.clone(), span: with_site.span },
            ));
        }
        match merged_scopes.iter().find(|scope| scope.name == with_site.scope_name) {
            None => issues.push((
                None,
                CompositionIssue::WithArgsMismatch { scope_name: with_site.scope_name.clone(), span: with_site.span },
            )),
            // Expansion moved a matching argument list into the record local; anything else
            // (arguments left, or a scope with parameters and no record) does not match.
            Some(scope)
                if with_site.argument_count != 0 || (scope.parameter_count > 0 && !with_site.arguments_bound) =>
            {
                issues.push((
                    None,
                    CompositionIssue::WithArgumentCount {
                        scope_name: with_site.scope_name.clone(),
                        expected: scope.parameter_count,
                        found: with_site.argument_count,
                        span: with_site.span,
                    },
                ))
            }
            Some(_) => {}
        }
    }
    if let Some((launched, bases)) = host_chain.split_last() {
        for base in bases {
            if base.parameter_count > 0 || base.scope_parameters {
                issues.push((
                    base.unit.clone(),
                    CompositionIssue::BaseHostParameters { host_name: base.name.clone(), span: base.span },
                ));
            }
        }
        for site in collected.launches.iter().filter(|site| site.span == launch_span) {
            if site.argument_count != 0 || (launched.parameter_count > 0 && !site.arguments_bound) {
                issues.push((
                    None,
                    CompositionIssue::LaunchArgumentCount {
                        host_name: launched.name.clone(),
                        expected: launched.parameter_count,
                        found: site.argument_count,
                        cross_unit: launched.unit.is_some(),
                        span: site.span,
                    },
                ));
            }
        }
    }

    let (mut merged_registrations, merge_issues) =
        merge_host_registries(&host_chain, &collected.host_registries, &collected.host_scopes, &merged_scopes);
    issues.extend(merge_issues);
    // Registrations of imported units are checked too; their issues stay in their own unit.
    issues.extend(super::shapes::validate_registrations(&merged_registrations, input.program, imported, input.types));
    // Parser `Spanned.id` is deliberately unset. Freeze identities from the same deterministic
    // expanded-tree pre-order that the generation-bound query index uses; a non-unique source
    // span remains invalid and is rejected at the codegen boundary.
    let mut indexes = HashMap::new();
    indexes.insert(None, SyntaxSnapshot::from_program(input.program, 0));
    for (path, program) in imported {
        indexes.insert(Some(std::sync::Arc::clone(path)), SyntaxSnapshot::from_program(program, 0));
    }
    let source_node = |unit: &super::model::UnitPath, kind: NodeKind, span: SpanInfo| {
        indexes.get(unit).map_or(AstNodeId::INVALID, |index| unique_source_node(index, kind, span))
    };
    for registration in &mut merged_registrations {
        registration.source_node_id = source_node(&registration.unit, NodeKind::RegistryEntry, registration.span);
    }

    let scope_parents = scope_parent_map(&merged_scopes);
    let container = ServiceContainer::from_registrations(&merged_registrations);
    let requests = dependency_requests(&merged_registrations, &collected.type_inject_fields);

    let registration_scope: HashMap<u32, _> =
        merged_registrations.iter().map(|registration| (registration.id, registration.scope_id)).collect();

    let mut edges = Vec::new();
    let mut plural_registration_ids = Vec::new();
    let mut singular_registration_ids = Vec::new();
    for request in requests {
        let request_scope = registration_scope
            .get(&request.owner_registration_id)
            .copied()
            .unwrap_or(crate::composition::model::ScopeId::GLOBAL);
        match resolve_dependency_targets(&request, request_scope, &scope_parents, &container) {
            Ok(targets) => {
                for target in &targets {
                    // Build dependency -> dependent edges so topo order is init-safe.
                    edges.push((target.id, request.owner_registration_id));
                }
                if request.is_plural {
                    plural_registration_ids.push((
                        request.owner_registration_id,
                        request.span,
                        (request.field_node_id, request.field_unit.clone()),
                        targets.iter().map(|target| target.id).collect::<Vec<_>>(),
                    ));
                } else if let [target] = targets.as_slice() {
                    singular_registration_ids.push((
                        request.owner_registration_id,
                        request.span,
                        (request.field_node_id, request.field_unit.clone()),
                        target.id,
                    ));
                }
            }
            Err(issue) => issues.push((request.field_unit.clone(), issue)),
        }
    }

    let registration_order = match build_graph(&merged_registrations, &edges) {
        Ok(dag) => topo_registration_order(&dag),
        Err(issue) => {
            let unit = match &issue {
                CompositionIssue::DependencyCycle { from_id, .. } => merged_registrations
                    .iter()
                    .find(|registration| registration.id == *from_id)
                    .and_then(|registration| registration.unit.clone()),
                _ => None,
            };
            issues.push((unit, issue));
            Vec::new()
        }
    };

    let launched_host = host_chain
        .last()
        .map(|host| host.name.clone())
        .unwrap_or_else(|| resolve_host_key(&collected.hosts, &launch_host).unwrap_or(launch_host));

    let activation = registration_order
        .into_iter()
        .enumerate()
        .map(|(slot, registration_id)| ActivationPlanEntry {
            registration_id,
            slot: ServiceSlot(u32::try_from(slot).expect("composition registration count exceeds u32")),
        })
        .collect::<Vec<_>>();
    let slots = activation.iter().map(|entry| (entry.registration_id, entry.slot)).collect::<HashMap<_, _>>();
    let mut singulars = singular_registration_ids
        .into_iter()
        .map(|(owner_registration_id, field_span, (field_node_id, field_unit), target_id)| SingularPlan {
            owner_registration_id,
            field_span,
            field_node_id,
            field_unit,
            target_slot: slots[&target_id],
        })
        .collect::<Vec<_>>();
    singulars.sort_by_key(|singular| (singular.owner_registration_id, singular.field_span.start));
    for singular in &mut singulars {
        singular.field_node_id = source_node(&singular.field_unit, NodeKind::Field, singular.field_span);
    }
    let mut plurals = plural_registration_ids
        .into_iter()
        .map(|(owner_registration_id, field_span, (field_node_id, field_unit), targets)| PluralPlan {
            owner_registration_id,
            field_span,
            field_node_id,
            field_unit,
            target_slots: targets.into_iter().map(|target| slots[&target]).collect(),
        })
        .collect::<Vec<_>>();
    plurals.sort_by_key(|plural| (plural.owner_registration_id, plural.field_span.start));
    for plural in &mut plurals {
        plural.field_node_id = source_node(&plural.field_unit, NodeKind::Field, plural.field_span);
    }

    let mut hooks = Vec::new();
    for host in &host_chain {
        let local_scopes = collected.host_scopes.get(&host.name);
        if let Some(host_hooks) = collected.host_hooks.get(&host.name) {
            for hook in host_hooks {
                let scope_id = if hook.scope_id == ScopeId::GLOBAL {
                    Some(ScopeId::GLOBAL)
                } else {
                    local_scopes
                        .and_then(|scopes| scopes.iter().find(|scope| scope.id == hook.scope_id))
                        .and_then(|local| merged_scopes.iter().find(|scope| scope.name == local.name))
                        .map(|scope| scope.id)
                };
                let Some(scope_id) = scope_id else {
                    issues.push((
                        hook.unit.clone(),
                        CompositionIssue::UnknownParentScope {
                            scope_name: "<composition hook>".into(),
                            parent_scope: hook.scope_id,
                            span: hook.span,
                        },
                    ));
                    continue;
                };
                if hook.kind == crate::syntax::ScopeHookKind::Startup && scope_id != ScopeId::GLOBAL {
                    issues.push((hook.unit.clone(), CompositionIssue::StartupInNamedScope { span: hook.span }));
                    continue;
                }
                let arguments = super::hooks::hook_arguments_name(&host.name, hook.kind, hook.span);
                hooks.push(CompositionHookPlan {
                    scope_id,
                    kind: hook.kind,
                    source_node_id: source_node(&hook.unit, NodeKind::ScopeHook, hook.span),
                    span: hook.span,
                    unit: hook.unit.clone(),
                    function_node_id: source_node(&hook.unit, NodeKind::FunctionDefinition, hook.span),
                    arguments_registration_id: merged_registrations
                        .iter()
                        .find(|registration| registration.implementation == arguments && registration.unit == hook.unit)
                        .map(|registration| registration.id),
                    takes_arguments_record: hook.takes_record,
                });
            }
        }
    }
    let init_hooks = hooks.iter().filter(|hook| hook.kind == crate::syntax::ScopeHookKind::Init).cloned().collect();
    let startup_hooks =
        hooks.iter().filter(|hook| hook.kind == crate::syntax::ScopeHookKind::Startup).cloned().collect();
    let mut disposal_hooks =
        hooks.into_iter().filter(|hook| hook.kind == crate::syntax::ScopeHookKind::Dispose).collect::<Vec<_>>();
    disposal_hooks.reverse();

    let scope_names = merged_scopes.iter().map(|scope| (scope.id, scope.name.clone())).collect();
    let snapshot = CompositionSnapshot {
        version: 1,
        launched_host,
        source_unit_path: None,
        launch_span: if launch_span == SpanInfo::default() { None } else { Some(launch_span) },
        registrations: merged_registrations.clone(),
        scope_names,
    };
    let plan = BindingPlan {
        launched_host: snapshot.launched_host.clone(),
        activation,
        singulars,
        plurals,
        scope_parents,
        init_hooks,
        startup_hooks,
        disposal_hooks,
    };

    let mut entry_issues = Vec::new();
    let mut foreign_issues = Vec::new();
    for (unit, issue) in issues {
        match unit {
            None => entry_issues.push(issue),
            Some(path) => foreign_issues.push((path, issue)),
        }
    }
    CompositionResult { plan, snapshot, issues: entry_issues, foreign_issues, dependency_edges: edges }
}

fn unique_source_node(index: &SyntaxSnapshot<'_>, kind: NodeKind, span: SpanInfo) -> AstNodeId {
    let mut matches = (0..index.len())
        .filter(|id| index.kind_of(*id as u32) == Some(kind) && index.span_of(*id as u32) == Some(span))
        .filter_map(|id| u32::try_from(id).ok());
    match (matches.next(), matches.next()) {
        (Some(id), None) => AstNodeId(id),
        _ => AstNodeId::INVALID,
    }
}

#[cfg(test)]
mod tests {
    use super::{CompositionInput, resolve_composition, resolve_composition_with_imports};
    use crate::services::parse_program;

    #[test]
    fn resolves_composition_from_expanded_syntax() {
        let program = parse_program(
            r#"
host AppHost() {
    registry {
        single Logger;
    }
}

i32 Main() {
    launch AppHost();
    return 0;
}
"#,
        )
        .expect("composition source parses");

        let types = crate::composition::types::test_types::DeclaredTypes::new(&program, &[]);
        let result = resolve_composition(CompositionInput { program: &program, is_mod_project: false, types: &types });

        assert_eq!(result.snapshot.launched_host, "AppHost");
        assert_eq!(result.plan.activation.len(), 1);
        assert_eq!(result.plan.activation[0].registration_id, result.snapshot.registrations[0].id);
        assert_eq!(result.plan.activation[0].slot.0, 0);
        assert!(result.plan.plurals.is_empty());
        assert!(result.issues.is_empty(), "issues: {:?}", result.issues);
    }

    #[test]
    fn freezes_singular_injection_and_declared_lifecycle_hooks() {
        let program = parse_program(
            r#"
type Logger {}
type Worker { inject Logger logger; }
host AppHost() {
    registry { single Logger; single Worker; }
    startup() {}
    dispose() {}
}
i32 Main() { launch AppHost(); return 0; }
"#,
        )
        .expect("composition source parses");
        let types = crate::composition::types::test_types::DeclaredTypes::new(&program, &[]);
        let result = resolve_composition(CompositionInput { program: &program, is_mod_project: false, types: &types });
        assert!(result.issues.is_empty(), "issues: {:?}", result.issues);
        assert_eq!(result.plan.activation.len(), 2);
        assert_eq!(result.plan.singulars.len(), 1);
        assert_eq!(result.plan.singulars[0].owner_registration_id, result.snapshot.registrations[1].id);
        assert_eq!(result.plan.singulars[0].target_slot.0, 0);
        assert_eq!(result.plan.startup_hooks.len(), 1, "declared startup must not be dropped");
        assert_eq!(result.plan.disposal_hooks.len(), 1, "declared disposal must not be dropped");
    }

    #[test]
    fn freezes_each_plural_injection_field_without_overwriting_siblings() {
        let program = parse_program(
            r#"
type Logger {}
type Worker {
    inject Logger[] primary;
    inject Logger[] secondary;
}
host AppHost() { registry { single Logger; single Worker; } }
i32 Main() { launch AppHost(); return 0; }
"#,
        )
        .expect("composition source parses");
        let types = crate::composition::types::test_types::DeclaredTypes::new(&program, &[]);
        let result = resolve_composition(CompositionInput { program: &program, is_mod_project: false, types: &types });
        assert!(result.issues.is_empty(), "issues: {:?}", result.issues);
        assert_eq!(result.plan.plurals.len(), 2, "both plural fields require independent plans");
        assert_ne!(result.plan.plurals[0].field_span, result.plan.plurals[1].field_span);
        assert_eq!(result.plan.plurals[0].target_slots, vec![super::ServiceSlot(0)]);
        assert_eq!(result.plan.plurals[1].target_slots, vec![super::ServiceSlot(0)]);
    }

    #[test]
    fn a_base_host_from_another_unit_contributes_its_registrations() {
        let library = parse_program(
            r#"
pub type CommandLine { }
host ConsoleHost() { registry { single CommandLine; } }
"#,
        )
        .expect("library parses");
        let program = crate::composition::lower_host_hooks(
            parse_program(
                r#"
type Greeter { }
host AppHost() : ConsoleHost
{
    registry { single Greeter; }
    startup(Hosting.CommandLine commandLine, Greeter greeter) { }
}
i32 Main() { launch AppHost(); return 0; }
"#,
            )
            .expect("application parses"),
        );
        let path: std::sync::Arc<std::path::Path> = std::sync::Arc::from(std::path::Path::new("/corelib/Hosting.bd"));
        let imported = [(std::sync::Arc::clone(&path), &library)];
        let types = crate::composition::types::test_types::DeclaredTypes::new(&program, &imported);
        let result =
            resolve_composition_with_imports(CompositionInput { program: &program, is_mod_project: false, types: &types }, &imported);
        assert!(result.issues.is_empty(), "issues: {:?}", result.issues);
        let imported = result
            .snapshot
            .registrations
            .iter()
            .find(|registration| registration.implementation == "CommandLine")
            .expect("base registration merged");
        assert_eq!(imported.unit.as_deref(), Some(path.as_ref()));
        assert!(imported.source_node_id.is_valid(), "the base registration keeps its own unit's identity");
        assert_eq!(result.plan.startup_hooks.len(), 1);
        let ids = result.snapshot.registrations.iter().map(|registration| registration.id).collect::<std::collections::HashSet<_>>();
        assert_eq!(ids.len(), result.snapshot.registrations.len(), "registration ids stay unique across units");
    }
    /// Resolves `Module.Type` paths to the type declared in that inline module: enough to tell
    /// two declarations with one simple name apart.
    struct QualifiedTypes(std::collections::HashMap<String, crate::composition::TypeIdentity>);

    impl crate::composition::CompositionTypeResolver for QualifiedTypes {
        fn resolve_type(
            &self,
            _unit: &crate::composition::UnitPath,
            _site: crate::syntax::SpanInfo,
            path: &crate::syntax::Path,
        ) -> Option<crate::composition::TypeIdentity> {
            let name =
                path.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>().join(".");
            self.0.get(&name).cloned()
        }
    }

    #[test]
    fn keys_and_requests_compare_by_declaration_not_by_simple_name() {
        use crate::syntax::Node;
        let issues = |requested: &str| {
            let program = parse_program(&format!(
                r#"
mod A {{ pub type Clock {{ }} }}
mod B {{ pub type Clock {{ }} }}
type Report {{ inject {requested} clock; }}
host AppHost() {{ registry {{ single A.Clock; single Report; }} }}
i32 Main() {{ launch AppHost(); return 0; }}
"#
            ))
            .expect("parse");
            let mut declared = std::collections::HashMap::new();
            for item in &program.node.items {
                match &item.node {
                    Node::InlineModule(module) => {
                        for inner in &module.node.items {
                            if let Node::TypeDefinition(definition) = &inner.node {
                                declared.insert(
                                    format!("{}.{}", module.node.name.node.name, definition.node.name.node.name),
                                    crate::composition::TypeIdentity { unit: None, declaration: definition.node.name.span, arguments: Vec::new() },
                                );
                            }
                        }
                    }
                    Node::TypeDefinition(definition) => {
                        declared.insert(
                            definition.node.name.node.name.clone(),
                            crate::composition::TypeIdentity { unit: None, declaration: definition.node.name.span, arguments: Vec::new() },
                        );
                    }
                    _ => {}
                }
            }
            let types = QualifiedTypes(declared);
            resolve_composition(CompositionInput { program: &program, is_mod_project: false, types: &types }).issues
        };
        assert!(issues("A.Clock").is_empty(), "`A.Clock` is the registered declaration");
        let other = issues("B.Clock");
        assert!(
            matches!(
                other.as_slice(),
                [crate::composition::CompositionIssue::UnresolvedInject { requested_type, .. }] if requested_type == "B.Clock"
            ),
            "`B.Clock` has the same simple name but is another declaration: {other:?}"
        );
    }
    #[test]
    fn generic_contract_keys_compare_type_arguments() {
        let issues = |requested: &str| {
            let program = crate::composition::lower_host_hooks(
                parse_program(&format!(
                    r#"
contract Repo<T> {{ i64 Count(); }}
type User {{ }}
type Order {{ }}
type SqlRepo : Repo<User> {{ pub i64 Count() {{ return 0; }} }}
type Service {{ inject {requested} repo }}
host AppHost() {{ registry {{ single SqlRepo for Repo<User>; single Service; }} }}
i32 Main() {{ launch AppHost(); return 0; }}
"#
                ))
                .expect("parse"),
            );
            let types = crate::composition::types::test_types::DeclaredTypes::new(&program, &[]);
            resolve_composition(CompositionInput { program: &program, is_mod_project: false, types: &types }).issues
        };
        assert!(issues("Repo<User>").is_empty(), "same declaration and type arguments");
        let other = issues("Repo<Order>");
        assert!(
            matches!(other.as_slice(), [crate::composition::CompositionIssue::UnresolvedInject { .. }]),
            "other type arguments are another key: {other:?}"
        );
    }
}
