//! Generation-bound composition statement shape. The adapter validates it against the frozen graph.

use std::sync::Arc;

use beskid_analysis::{
    syntax::{FieldKind, LaunchStatement, RegistryEntry, WithStatement},
    syntax_query::DynNodeRef,
};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompositionLaunchFact {
    pub site: AstNodeKey,
    pub host: Arc<str>,
    /// The generated `let` that holds the host record built from the `launch` arguments
    /// (`beskid_analysis::composition::hooks`); `None` when the host has no parameters.
    pub arguments: Option<AstNodeKey>,
}

/// Syntax names a scope; only the assembled frontend snapshot assigns its authoritative ID.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompositionScopeFact {
    pub site: AstNodeKey,
    pub scope_name: Arc<str>,
    pub body: AstNodeKey,
    /// The generated `let` that holds the scope record built from the `with` arguments.
    pub arguments: Option<AstNodeKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompositionRegistrationFact {
    pub site: AstNodeKey,
    pub implementation: Arc<str>,
    pub declaration: AstNodeKey,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompositionInjectionFieldFact {
    pub field: AstNodeKey,
    pub owner_type: AstNodeKey,
    pub ordinal: u32,
    pub is_plural: bool,
}

/// A source-proven read of an injected field, separate from value-field projection.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CompositionInjectedAccessFact {
    pub site: AstNodeKey,
    pub receiver: AstNodeKey,
    pub owner_type: AstNodeKey,
    pub field: AstNodeKey,
}

pub fn composition_launch(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CompositionLaunchFact> {
    with_registered_syntax(db, key, composition_launch_tracked)
}

pub fn composition_scope(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CompositionScopeFact> {
    with_registered_syntax(db, key, composition_scope_tracked)
}

pub fn composition_registration(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CompositionRegistrationFact> {
    with_registered_syntax(db, key, composition_registration_tracked)
}

pub fn composition_injection_field(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CompositionInjectionFieldFact> {
    with_registered_syntax(db, key, composition_injection_field_tracked)
}

pub fn composition_injected_field_access(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<CompositionInjectedAccessFact> {
    with_registered_syntax(db, key, composition_injected_field_access_tracked)
}

#[salsa::tracked(persist)]
fn composition_injected_field_access_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CompositionInjectedAccessFact> {
    with_node(db, syntax, key, |program, index, node| {
        let resolved = field_access_receiver(db, program, index, key, node, None, None)?.ok()?;
        let declaration = resolved.declaration;
        let target = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
        let target_index = target.syntax_index(db);
        let target_program = target.expanded_program(db);
        let definition =
            target_index.node_at(target_program, declaration.node)?.of::<beskid_analysis::syntax::TypeDefinition>()?;
        let matches = definition
            .fields
            .iter()
            .filter(|field| field.node.kind == FieldKind::Injected && field.node.name.node.name == resolved.field_name)
            .collect::<Vec<_>>();
        let [field] = matches.as_slice() else {
            return None;
        };
        if declaration.unit != key.unit && field.node.visibility.node != beskid_analysis::syntax::Visibility::Public {
            return None;
        }
        let field_id = target_index.direct_child_id(target_program, declaration.node, DynNodeRef::from(*field))?;
        let field = AstNodeKey { unit: declaration.unit, generation: declaration.generation, node: field_id };
        let injection = composition_injection_field(db, field).ok().flatten()?;
        (injection.owner_type == declaration).then_some(())?;
        Some(CompositionInjectedAccessFact { site: key, receiver: resolved.receiver, owner_type: declaration, field })
    })
}

#[salsa::tracked(persist)]
fn composition_registration_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CompositionRegistrationFact> {
    with_node(db, syntax, key, |_program, _index, node| {
        let registration = node.of::<RegistryEntry>()?;
        let declaration = resolve_type_declaration(db, key, &registration.implementation.node)?;
        let implementation = registration
            .implementation
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.as_str())
            .collect::<Vec<_>>()
            .join(".");
        Some(CompositionRegistrationFact { site: key, implementation: Arc::from(implementation), declaration })
    })
}

#[salsa::tracked(persist)]
fn composition_injection_field_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CompositionInjectionFieldFact> {
    with_node(db, syntax, key, |program, index, node| {
        let field = node.of::<beskid_analysis::syntax::Field>()?;
        (field.kind == FieldKind::Injected).then_some(())?;
        let mut parent = key.node;
        let owner = loop {
            parent = parent_node(index, parent)?;
            if index.kind(parent)? == beskid_analysis::syntax_query::NodeKind::TypeDefinition {
                break parent;
            }
        };
        let definition = index.node_at(program, owner)?.of::<beskid_analysis::syntax::TypeDefinition>()?;
        let ordinal =
            definition.fields.iter().filter(|candidate| candidate.node.kind == FieldKind::Injected).position(
                |candidate| index.direct_child_id(program, owner, DynNodeRef::from(candidate)) == Some(key.node),
            )?;
        Some(CompositionInjectionFieldFact {
            field: key,
            owner_type: AstNodeKey { node: owner, ..key },
            ordinal: u32::try_from(ordinal).ok()?,
            is_plural: matches!(&field.ty.node, beskid_analysis::syntax::Type::Array(_)),
        })
    })
}

#[salsa::tracked(persist)]
fn composition_launch_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CompositionLaunchFact> {
    with_node(db, syntax, key, |program, index, node| {
        let launch = node.of::<LaunchStatement>()?;
        let host = launch
            .host_path
            .node
            .segments
            .iter()
            .map(|segment| segment.node.name.node.name.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let local = beskid_analysis::composition::hooks::launch_arguments_local(node.span()?);
        Some(CompositionLaunchFact {
            site: key,
            host: Arc::from(host),
            arguments: arguments_local(program, index, key, &local)?,
        })
    })
}

#[salsa::tracked(persist)]
fn composition_scope_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CompositionScopeFact> {
    with_node(db, syntax, key, |program, index, node| {
        let with_statement = node.of::<WithStatement>()?;
        let scope_name = &with_statement.scope_name.node.name;
        let body = index.direct_child_id(program, key.node, DynNodeRef::from(&with_statement.body))?;
        let local = beskid_analysis::composition::hooks::scope_arguments_local(node.span()?);
        Some(CompositionScopeFact {
            site: key,
            scope_name: Arc::from(scope_name.as_str()),
            body: AstNodeKey { node: body, ..key },
            arguments: arguments_local(program, index, key, &local)?,
        })
    })
}

/// The generated record `let` named `local` in the unit of `site`: `Some(None)` when there is
/// none, `None` (no fact) when the name is not unique.
fn arguments_local(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    site: AstNodeKey,
    local: &str,
) -> Option<Option<AstNodeKey>> {
    let mut matches = index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::LetStatement).filter(|id| {
        index
            .node_at(program, *id)
            .and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())
            .is_some_and(|binding| binding.name.node.name == local)
    });
    match (matches.next(), matches.next()) {
        (None, _) => Some(None),
        (Some(id), None) => Some(Some(AstNodeKey { node: id, ..site })),
        (Some(_), Some(_)) => None,
    }
}
