use std::collections::HashMap;

use super::model::{
    CompositionHost, CompositionScope, InjectDependency, Registration, RegistrationKey,
    RegistrationLifetime as Lifetime, ScopeId,
};
use super::types::{CompositionTypeResolver, TypeIdentity};
use crate::syntax::{
    FieldKind, HostBodyItem, HostDefinition, InjectQualifier, Node, Program, RegistrationLifetime, RegistryBlock,
    RegistryEntry, ScopeDefinition, ScopeHookKind, Spanned, Type,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInjectField {
    pub unit: super::model::UnitPath,
    pub requested_type: String,
    /// The declaration the requested type (the element type of a plural request) names.
    pub requested_identity: Option<TypeIdentity>,
    pub qualifier: Option<InjectQualifier>,
    pub is_plural: bool,
    pub span: crate::syntax::SpanInfo,
    pub field_node_id: crate::syntax::AstNodeId,
    pub field_name: String,
    pub name_span: crate::syntax::SpanInfo,
    pub type_span: crate::syntax::SpanInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollectedComposition {
    pub hosts: HashMap<String, CompositionHost>,
    pub host_registries: HashMap<String, Vec<Registration>>,
    pub host_scopes: HashMap<String, Vec<CompositionScope>>,
    pub host_hooks: HashMap<String, Vec<CollectedHook>>,
    pub launches: Vec<LaunchSite>,
    pub with_sites: Vec<WithSite>,
    /// `inject` fields of each type declaration, keyed by the declaration.
    pub type_inject_fields: HashMap<TypeIdentity, Vec<TypeInjectField>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectedHook {
    pub unit: super::model::UnitPath,
    pub scope_id: ScopeId,
    pub kind: ScopeHookKind,
    pub source_node_id: crate::syntax::AstNodeId,
    pub span: crate::syntax::SpanInfo,
    /// Whether the generated hook function takes the host or scope record first.
    pub takes_record: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSite {
    pub host_name: String,
    pub span: crate::syntax::SpanInfo,
    /// Arguments left on the statement: expansion moves a matching argument list into the
    /// record local, so any argument left here does not match the host's parameters.
    pub argument_count: usize,
    /// Whether the generated record local precedes the statement.
    pub arguments_bound: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithSite {
    pub scope_name: String,
    pub span: crate::syntax::SpanInfo,
    /// Arguments left on the statement (see [`LaunchSite::argument_count`]).
    pub argument_count: usize,
    /// Whether the generated record local precedes the statement.
    pub arguments_bound: bool,
    /// Whether a `launch` precedes this `with` in its own block or an enclosing block of the
    /// same function body. Lowering reaches the launched container only that way.
    pub after_launch: bool,
}

/// Collect composition declarations directly from the expanded syntax authority.
///
/// Composition only consumes declarations and statement shape, so lowering it to syntax would add
/// no semantic information and would make this diagnostic path depend on the retired pipeline.
/// The unit being collected and the type authority that resolves its paths.
pub struct UnitContext<'a> {
    pub unit: super::model::UnitPath,
    pub types: &'a dyn CompositionTypeResolver,
}

impl UnitContext<'_> {
    fn resolve(&self, path: &Spanned<crate::syntax::Path>) -> Option<TypeIdentity> {
        super::types::path_identity(self.types, &self.unit, path)
    }
}

/// The `inject` fields of the type declarations of `items`, at any inline-module depth.
fn collect_inject_fields(items: &[Spanned<Node>], context: &UnitContext<'_>, collected: &mut CollectedComposition) {
    for item in items {
        match &item.node {
            Node::TypeDefinition(def) => {
                let injects = def
                    .node
                    .fields
                    .iter()
                    .filter(|field| field.node.kind == FieldKind::Injected)
                    .map(|field| TypeInjectField {
                        unit: context.unit.clone(),
                        requested_type: type_name(&field.node.ty),
                        requested_identity: element_path(&field.node.ty).and_then(|path| context.resolve(path)),
                        qualifier: field.node.inject_qualifier,
                        is_plural: type_is_plural(&field.node.ty),
                        span: field.span,
                        field_node_id: field.id,
                        field_name: field.node.name.node.name.clone(),
                        name_span: field.node.name.span,
                        type_span: field.node.ty.span,
                    })
                    .collect::<Vec<_>>();
                if !injects.is_empty() {
                    let declaration =
                        TypeIdentity { unit: context.unit.clone(), declaration: def.node.name.span, arguments: Vec::new() };
                    collected.type_inject_fields.insert(declaration, injects);
                }
            }
            Node::InlineModule(module) => collect_inject_fields(&module.node.items, context, collected),
            _ => {}
        }
    }
}

/// The path of a requested service type: the type itself, or the element type of an array.
fn element_path(ty: &Spanned<Type>) -> Option<&Spanned<crate::syntax::Path>> {
    match &ty.node {
        Type::Complex(path) => Some(path),
        Type::Array(inner) => element_path(inner),
        _ => None,
    }
}

pub fn collect(program: &Spanned<Program>, context: &UnitContext<'_>) -> CollectedComposition {
    let mut collected = CollectedComposition::default();
    let mut next_registration_id = 1_u32;
    let mut next_scope_id = 1_u32;

    collect_inject_fields(&program.node.items, context, &mut collected);
    for item in &program.node.items {
        match &item.node {
            Node::HostDefinition(host) => {
                collect_host(host, context, &mut collected, &mut next_registration_id, &mut next_scope_id);
            }
            Node::Function(def) => {
                collect_launch_and_with_statements(&def.node.body.node.statements, &mut collected, false);
            }
            Node::Method(def) => {
                collect_launch_and_with_statements(&def.node.body.node.statements, &mut collected, false);
            }
            Node::TestDefinition(def) => {
                collect_launch_and_with_statements(&def.node.statements, &mut collected, false);
            }
            Node::InlineModule(module) => {
                collect_launch_and_with_statements_in_items(&module.node.items, &mut collected);
            }
            _ => {}
        }
    }
    collect_statements_outside_ordered_positions(&program.node.items, &mut collected);

    collected
}

/// Collect the unit under analysis together with the composition declarations of imported
/// units of the same assembly (library and Corelib hosts, the types they register). Imported
/// units contribute hosts, their registries, scopes and hooks, and inject fields; never `launch`
/// or `with` sites. A name the entry declares wins over an imported one.
pub fn collect_with_imports(
    program: &Spanned<Program>,
    imported: &[(std::sync::Arc<std::path::Path>, &Spanned<Program>)],
    types: &dyn CompositionTypeResolver,
) -> CollectedComposition {
    let mut collected = collect(program, &UnitContext { unit: None, types });
    for (path, unit_program) in imported {
        let mut foreign = collect(unit_program, &UnitContext { unit: Some(std::sync::Arc::clone(path)), types });
        let offset = collected
            .host_registries
            .values()
            .flatten()
            .map(|registration| registration.id)
            .max()
            .unwrap_or(0);
        for (name, host) in foreign.hosts.drain() {
            if collected.hosts.contains_key(&name) {
                continue;
            }
            let mut registrations = foreign.host_registries.remove(&name).unwrap_or_default();
            for registration in &mut registrations {
                registration.id += offset;
            }
            collected.host_registries.insert(name.clone(), registrations);
            collected.host_scopes.insert(name.clone(), foreign.host_scopes.remove(&name).unwrap_or_default());
            collected.host_hooks.insert(name.clone(), foreign.host_hooks.remove(&name).unwrap_or_default());
            collected.hosts.insert(name, host);
        }
        // Declarations are keyed by their own unit, so the units never collide.
        collected.type_inject_fields.extend(foreign.type_inject_fields.drain());
    }
    collected
}

/// `launch` and `with` are lowered only at statement level of a function body or inside its
/// nested `if`/`while`/`for`/`with` blocks. Any other position (a method body, a `match` arm, a
/// lambda) is still recorded, so analysis rejects it instead of lowering failing later: such a
/// `with` never counts as following a `launch`.
fn collect_statements_outside_ordered_positions(items: &[Spanned<Node>], collected: &mut CollectedComposition) {
    use crate::syntax_query::Query;
    for item in items {
        if matches!(item.node, Node::HostDefinition(_)) {
            continue;
        }
        for statement in Query::from(&item.node).of::<crate::syntax::Statement>() {
            match statement {
                crate::syntax::Statement::With(with_stmt)
                    if !collected.with_sites.iter().any(|site| site.span == with_stmt.span) =>
                {
                    collected.with_sites.push(WithSite {
                        scope_name: with_stmt.node.scope_name.node.name.clone(),
                        span: with_stmt.span,
                        argument_count: with_stmt.node.arguments.len(),
                        arguments_bound: false,
                        after_launch: false,
                    });
                }
                crate::syntax::Statement::Launch(launch_stmt)
                    if !collected.launches.iter().any(|site| site.span == launch_stmt.span) =>
                {
                    record_launch_site(collected, launch_stmt, false);
                }
                _ => {}
            }
        }
    }
}

fn collect_launch_and_with_statements_in_items(items: &[Spanned<Node>], collected: &mut CollectedComposition) {
    for item in items {
        if let Some(statements) = item_statement_list(item) {
            collect_launch_and_with_statements(statements, collected, false);
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

fn collect_syntax_else_branch(
    else_branch: &Spanned<crate::syntax::ElseBranch>,
    collected: &mut CollectedComposition,
    launched: bool,
) {
    match &else_branch.node {
        crate::syntax::ElseBranch::Block(block) => {
            collect_launch_and_with_statements(&block.node.statements, collected, launched);
        }
        crate::syntax::ElseBranch::If(nested) => {
            collect_launch_and_with_statements(&nested.node.then_block.node.statements, collected, launched);
            if let Some(nested_else) = &nested.node.else_branch {
                collect_syntax_else_branch(nested_else, collected, launched);
            }
        }
    }
}

/// `launched` says whether a `launch` precedes these statements in an enclosing block.
fn collect_launch_and_with_statements(
    statements: &[Spanned<crate::syntax::Statement>],
    collected: &mut CollectedComposition,
    mut launched: bool,
) {
    let mut previous: Option<&crate::syntax::Statement> = None;
    for statement in statements {
        let bound = |local: String| {
            matches!(previous, Some(crate::syntax::Statement::Let(binding)) if binding.node.name.node.name == local)
        };
        match &statement.node {
            crate::syntax::Statement::With(with_stmt) => {
                collected.with_sites.push(WithSite {
                    scope_name: with_stmt.node.scope_name.node.name.clone(),
                    span: with_stmt.span,
                    argument_count: with_stmt.node.arguments.len(),
                    arguments_bound: bound(super::hooks::scope_arguments_local(with_stmt.span)),
                    after_launch: launched,
                });
                collect_launch_and_with_statements(&with_stmt.node.body.node.statements, collected, launched);
            }
            crate::syntax::Statement::Launch(launch_stmt) => {
                record_launch_site(collected, launch_stmt, bound(super::hooks::launch_arguments_local(launch_stmt.span)));
                launched = true;
            }
            crate::syntax::Statement::If(if_stmt) => {
                collect_launch_and_with_statements(&if_stmt.node.then_block.node.statements, collected, launched);
                if let Some(else_branch) = &if_stmt.node.else_branch {
                    collect_syntax_else_branch(else_branch, collected, launched);
                }
            }
            crate::syntax::Statement::While(while_stmt) => {
                collect_launch_and_with_statements(&while_stmt.node.body.node.statements, collected, launched);
            }
            crate::syntax::Statement::For(for_stmt) => {
                collect_launch_and_with_statements(&for_stmt.node.body.node.statements, collected, launched);
            }
            _ => {}
        }
        previous = Some(&statement.node);
    }
}

fn record_launch_site(
    collected: &mut CollectedComposition,
    launch: &Spanned<crate::syntax::LaunchStatement>,
    arguments_bound: bool,
) {
    collected.launches.push(LaunchSite {
        host_name: path_name(&launch.node.host_path),
        span: launch.span,
        argument_count: launch.node.arguments.len(),
        arguments_bound,
    });
}

fn collect_host(
    host: &Spanned<HostDefinition>,
    context: &UnitContext<'_>,
    collected: &mut CollectedComposition,
    next_registration_id: &mut u32,
    next_scope_id: &mut u32,
) {
    let host_name = host.node.name.node.name.clone();
    collected.hosts.insert(
        host_name.clone(),
        CompositionHost {
            name: host_name.clone(),
            unit: context.unit.clone(),
            base_host: host.node.base_host.as_ref().map(path_name),
            span: host.span,
            parameter_count: host.node.parameters.len(),
            required_parameter_count: super::hooks::required_parameter_count(&host.node.parameters),
            scope_parameters: false,
        },
    );

    let mut host_regs = Vec::new();
    let mut host_scopes = Vec::new();
    let mut host_hooks = Vec::new();
    for item in &host.node.body {
        match &item.node {
            HostBodyItem::Registry(registry) => {
                host_regs.extend(registrations_from_block(context, ScopeId::GLOBAL, registry, next_registration_id));
            }
            HostBodyItem::Scope(scope) => {
                collect_scope(
                    scope,
                    context,
                    ScopeId::GLOBAL,
                    &mut host_scopes,
                    &mut host_regs,
                    &mut host_hooks,
                    next_registration_id,
                    next_scope_id,
                );
            }
            HostBodyItem::Hook(hook) => {
                host_hooks.push(CollectedHook {
                    unit: context.unit.clone(),
                    scope_id: ScopeId::GLOBAL,
                    kind: hook.node.kind,
                    source_node_id: hook.id,
                    span: hook.span,
                    takes_record: !host.node.parameters.is_empty(),
                });
            }
            HostBodyItem::Registration(entry) => {
                host_regs.push(registration_from_entry(context, ScopeId::GLOBAL, entry, next_registration_id));
            }
        }
    }

    if let Some(entry) = collected.hosts.get_mut(&host_name) {
        entry.scope_parameters = host_scopes.iter().any(|scope: &CompositionScope| scope.parameter_count > 0);
    }
    collected.host_registries.insert(host_name.clone(), host_regs);
    collected.host_scopes.insert(host_name.clone(), host_scopes);
    collected.host_hooks.insert(host_name, host_hooks);
}

fn collect_scope(
    scope: &Spanned<ScopeDefinition>,
    context: &UnitContext<'_>,
    parent_scope_id: ScopeId,
    scopes: &mut Vec<CompositionScope>,
    regs: &mut Vec<Registration>,
    hooks: &mut Vec<CollectedHook>,
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
        parameter_count: scope.node.parameters.len(),
        required_parameter_count: super::hooks::required_parameter_count(&scope.node.parameters),
        unit: context.unit.clone(),
    });

    for item in &scope.node.body {
        match &item.node {
            HostBodyItem::Registry(registry) => {
                regs.extend(registrations_from_block(context, scope_id, registry, next_registration_id));
            }
            HostBodyItem::Registration(entry) => {
                regs.push(registration_from_entry(context, scope_id, entry, next_registration_id));
            }
            HostBodyItem::Scope(child_scope) => {
                collect_scope(child_scope, context, scope_id, scopes, regs, hooks, next_registration_id, next_scope_id);
            }
            HostBodyItem::Hook(hook) => {
                hooks.push(CollectedHook {
                    unit: context.unit.clone(),
                    scope_id,
                    kind: hook.node.kind,
                    source_node_id: hook.id,
                    span: hook.span,
                    takes_record: !scope.node.parameters.is_empty(),
                })
            }
        }
    }
}

fn registration_from_entry(
    context: &UnitContext<'_>,
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
    let implementation_identity = context.resolve(&entry.node.implementation);
    let key_identity = match &entry.node.target {
        Some(target) => context.resolve(target),
        None => implementation_identity.clone(),
    };
    Registration {
        id,
        source_node_id: entry.id,
        unit: context.unit.clone(),
        key_identity,
        implementation_identity,
        scope_id,
        key,
        implementation,
        lifetime,
        span: entry.span,
        implementation_span: entry.node.implementation.span,
    }
}

fn registrations_from_block(
    context: &UnitContext<'_>,
    scope_id: ScopeId,
    registry: &Spanned<RegistryBlock>,
    next_registration_id: &mut u32,
) -> Vec<Registration> {
    registry
        .node
        .entries
        .iter()
        .map(|entry| registration_from_entry(context, scope_id, entry, next_registration_id))
        .collect()
}

pub fn dependency_requests(
    registrations: &[Registration],
    type_inject_fields: &HashMap<TypeIdentity, Vec<TypeInjectField>>,
) -> Vec<InjectDependency> {
    let mut requests = Vec::new();
    for registration in registrations {
        let Some(implementation) = &registration.implementation_identity else {
            continue;
        };
        if let Some(fields) = type_inject_fields.get(&implementation.declaration_only()) {
            for field in fields {
                requests.push(InjectDependency {
                    field_unit: field.unit.clone(),
                    span: field.span,
                    field_node_id: field.field_node_id,
                    owner_registration_id: registration.id,
                    requested_type: field.requested_type.clone(),
                    requested_identity: field.requested_identity.clone(),
                    is_plural: field.is_plural,
                    qualifier: field.qualifier,
                    field_name: field.field_name.clone(),
                    name_span: field.name_span,
                    type_span: field.type_span,
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
        Type::Associated { contract, name } => format!("{}::{}", path_name(contract), name.node.name),
        Type::This => "This".to_string(),
        Type::Array(inner) => type_name(inner),
        Type::Function { .. } => "Function".to_string(),
    }
}

fn type_is_plural(ty: &Spanned<Type>) -> bool {
    matches!(ty.node, Type::Array(_))
}
