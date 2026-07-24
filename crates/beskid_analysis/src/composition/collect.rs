use std::collections::HashMap;

use crate::syntax::{
    FieldKind, HostBodyItem, HostDefinition, InjectQualifier, Node, Program, RegistrationLifetime, RegistryBlock,
    RegistryEntry, ScopeDefinition, ScopeHookKind, Spanned, Type,
};

use super::model::{
    CompositionHost, CompositionScope, InjectDependency, Registration, RegistrationKey,
    RegistrationLifetime as Lifetime, ScopeId,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInjectField {
    pub requested_type: String,
    pub qualifier: Option<InjectQualifier>,
    pub is_plural: bool,
    pub span: crate::syntax::SpanInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollectedComposition {
    pub hosts: HashMap<String, CompositionHost>,
    pub host_registries: HashMap<String, Vec<Registration>>,
    pub host_scopes: HashMap<String, Vec<CompositionScope>>,
    pub launches: Vec<LaunchSite>,
    pub with_sites: Vec<WithSite>,
    pub type_inject_fields: HashMap<String, Vec<TypeInjectField>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSite {
    pub host_name: String,
    pub span: crate::syntax::SpanInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithSite {
    pub scope_name: String,
    pub span: crate::syntax::SpanInfo,
}

/// Collect composition declarations directly from the expanded syntax authority.
///
/// Composition only consumes declarations and statement shape, so lowering it to HIR would add
/// no semantic information and would make this diagnostic path depend on the retired pipeline.
pub fn collect(program: &Spanned<Program>) -> CollectedComposition {
    let mut collected = CollectedComposition::default();
    let mut next_registration_id = 1_u32;
    let mut next_scope_id = 1_u32;

    for item in &program.node.items {
        match &item.node {
            Node::HostDefinition(host) => {
                collect_host(host, &mut collected, &mut next_registration_id, &mut next_scope_id);
            }
            Node::TypeDefinition(def) => {
                let key = def.node.name.node.name.clone();
                let injects = def
                    .node
                    .fields
                    .iter()
                    .filter(|field| field.node.kind == FieldKind::Injected)
                    .map(|field| TypeInjectField {
                        requested_type: type_name(&field.node.ty),
                        qualifier: field.node.inject_qualifier,
                        is_plural: type_is_plural(&field.node.ty),
                        span: field.span,
                    })
                    .collect::<Vec<_>>();
                if !injects.is_empty() {
                    collected.type_inject_fields.insert(key, injects);
                }
            }
            Node::Function(def) => {
                collect_launch_and_with_statements(&def.node.body.node.statements, &mut collected);
            }
            Node::Method(def) => {
                collect_launch_and_with_statements(&def.node.body.node.statements, &mut collected);
            }
            Node::TestDefinition(def) => {
                collect_launch_and_with_statements(&def.node.statements, &mut collected);
            }
            Node::InlineModule(module) => {
                collect_launch_and_with_statements_in_items(&module.node.items, &mut collected);
            }
            _ => {}
        }
    }

    collected
}

fn collect_launch_and_with_statements_in_items(items: &[Spanned<Node>], collected: &mut CollectedComposition) {
    for item in items {
        if let Some(statements) = item_statement_list(item) {
            collect_launch_and_with_statements(statements, collected);
            continue;
        }
        if let Node::InlineModule(module) = &item.node {
            collect_launch_and_with_statements_in_items(&module.node.items, collected);
        }
    }
}

fn item_statement_list(item: &Spanned<Node>) -> Option<&[Spanned<crate::syntax::Statement>]> {
    match &item.node {
        Node::Function(def) => Some(&def.node.body.node.statements),
        Node::Method(def) => Some(&def.node.body.node.statements),
        Node::TestDefinition(def) => Some(&def.node.statements),
        _ => None,
    }
}

fn collect_syntax_else_branch(else_branch: &Spanned<crate::syntax::ElseBranch>, collected: &mut CollectedComposition) {
    match &else_branch.node {
        crate::syntax::ElseBranch::Block(block) => {
            collect_launch_and_with_statements(&block.node.statements, collected);
        }
        crate::syntax::ElseBranch::If(nested) => {
            collect_launch_and_with_statements(&nested.node.then_block.node.statements, collected);
            if let Some(nested_else) = &nested.node.else_branch {
                collect_syntax_else_branch(nested_else, collected);
            }
        }
    }
}

fn collect_launch_and_with_statements(
    statements: &[Spanned<crate::syntax::Statement>],
    collected: &mut CollectedComposition,
) {
    for statement in statements {
        match &statement.node {
            crate::syntax::Statement::With(with_stmt) => {
                record_with_site(collected, &with_stmt.node.scope_name.node.name, with_stmt.span);
                collect_launch_and_with_statements(&with_stmt.node.body.node.statements, collected);
            }
            crate::syntax::Statement::Launch(launch_stmt) => {
                record_launch_site(collected, &launch_stmt.node.host_path, launch_stmt.span);
            }
            crate::syntax::Statement::If(if_stmt) => {
                collect_launch_and_with_statements(&if_stmt.node.then_block.node.statements, collected);
                if let Some(else_branch) = &if_stmt.node.else_branch {
                    collect_syntax_else_branch(else_branch, collected);
                }
            }
            crate::syntax::Statement::While(while_stmt) => {
                collect_launch_and_with_statements(&while_stmt.node.body.node.statements, collected);
            }
            crate::syntax::Statement::For(for_stmt) => {
                collect_launch_and_with_statements(&for_stmt.node.body.node.statements, collected);
            }
            _ => {}
        }
    }
}

fn record_with_site(collected: &mut CollectedComposition, scope_name: &str, span: crate::syntax::SpanInfo) {
    collected.with_sites.push(WithSite { scope_name: scope_name.to_string(), span });
}

fn record_launch_site(
    collected: &mut CollectedComposition,
    host_path: &Spanned<crate::syntax::Path>,
    span: crate::syntax::SpanInfo,
) {
    collected.launches.push(LaunchSite { host_name: path_name(host_path), span });
}

fn collect_host(
    host: &Spanned<HostDefinition>,
    collected: &mut CollectedComposition,
    next_registration_id: &mut u32,
    next_scope_id: &mut u32,
) {
    let host_name = host.node.name.node.name.clone();
    collected.hosts.insert(
        host_name.clone(),
        CompositionHost {
            name: host_name.clone(),
            base_host: host.node.base_host.as_ref().map(path_name),
            span: host.span,
        },
    );

    let mut host_regs = Vec::new();
    let mut host_scopes = Vec::new();
    for item in &host.node.body {
        match &item.node {
            HostBodyItem::Registry(registry) => {
                host_regs.extend(registrations_from_block(ScopeId::GLOBAL, registry, next_registration_id));
            }
            HostBodyItem::Scope(scope) => {
                collect_scope(
                    scope,
                    ScopeId::GLOBAL,
                    &mut host_scopes,
                    &mut host_regs,
                    next_registration_id,
                    next_scope_id,
                );
            }
            HostBodyItem::Hook(hook) => {
                if hook.node.kind == ScopeHookKind::Startup {
                    let _ = hook;
                }
            }
            HostBodyItem::Registration(entry) => {
                host_regs.push(registration_from_entry(ScopeId::GLOBAL, entry, next_registration_id));
            }
        }
    }

    collected.host_registries.insert(host_name.clone(), host_regs);
    collected.host_scopes.insert(host_name, host_scopes);
}

fn collect_scope(
    scope: &Spanned<ScopeDefinition>,
    parent_scope_id: ScopeId,
    scopes: &mut Vec<CompositionScope>,
    regs: &mut Vec<Registration>,
    next_registration_id: &mut u32,
    next_scope_id: &mut u32,
) {
    let scope_id = ScopeId(*next_scope_id);
    *next_scope_id += 1;
    scopes.push(CompositionScope {
        id: scope_id,
        name: scope.node.name.node.name.clone(),
        parent: Some(parent_scope_id),
        span: scope.span,
    });

    for item in &scope.node.body {
        match &item.node {
            HostBodyItem::Registry(registry) => {
                regs.extend(registrations_from_block(scope_id, registry, next_registration_id));
            }
            HostBodyItem::Registration(entry) => {
                regs.push(registration_from_entry(scope_id, entry, next_registration_id));
            }
            HostBodyItem::Scope(child_scope) => {
                collect_scope(child_scope, scope_id, scopes, regs, next_registration_id, next_scope_id);
            }
            HostBodyItem::Hook(_) => {}
        }
    }
}

fn registration_from_entry(
    scope_id: ScopeId,
    entry: &Spanned<RegistryEntry>,
    next_registration_id: &mut u32,
) -> Registration {
    let id = *next_registration_id;
    *next_registration_id += 1;
    let implementation = path_name(&entry.node.implementation);
    let key = entry
        .node
        .target
        .as_ref()
        .map(path_name)
        .map(RegistrationKey::Contract)
        .unwrap_or_else(|| RegistrationKey::SelfType(implementation.clone()));
    let lifetime = match (scope_id == ScopeId::GLOBAL, entry.node.lifetime) {
        (true, Some(RegistrationLifetime::Single)) => Lifetime::Single,
        (true, Some(RegistrationLifetime::Transient)) => Lifetime::Transient,
        (true, None) => Lifetime::Single,
        (false, Some(RegistrationLifetime::Single)) => Lifetime::Single,
        (false, Some(RegistrationLifetime::Transient)) => Lifetime::Transient,
        (false, None) => Lifetime::Scoped,
    };
    Registration { id, scope_id, key, implementation, lifetime, span: entry.span }
}

fn registrations_from_block(
    scope_id: ScopeId,
    registry: &Spanned<RegistryBlock>,
    next_registration_id: &mut u32,
) -> Vec<Registration> {
    registry.node.entries.iter().map(|entry| registration_from_entry(scope_id, entry, next_registration_id)).collect()
}

pub fn dependency_requests(
    registrations: &[Registration],
    type_inject_fields: &HashMap<String, Vec<TypeInjectField>>,
) -> Vec<InjectDependency> {
    let mut requests = Vec::new();
    for registration in registrations {
        if let Some(fields) = type_inject_fields.get(&registration.implementation) {
            for field in fields {
                requests.push(InjectDependency {
                    span: field.span,
                    owner_registration_id: registration.id,
                    requested_type: field.requested_type.clone(),
                    is_plural: field.is_plural,
                    qualifier: field.qualifier,
                });
            }
        }
    }
    requests
}

fn path_name(path: &Spanned<crate::syntax::Path>) -> String {
    path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>().join(".")
}

fn type_name(ty: &Spanned<Type>) -> String {
    match &ty.node {
        Type::Primitive(primitive) => format!("{:?}", primitive.node),
        Type::Complex(path) => {
            path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>().join(".")
        }
        Type::Array(inner) => type_name(inner),
        Type::Function { .. } => "Function".to_string(),
    }
}

fn type_is_plural(ty: &Spanned<Type>) -> bool {
    matches!(ty.node, Type::Array(_))
}
