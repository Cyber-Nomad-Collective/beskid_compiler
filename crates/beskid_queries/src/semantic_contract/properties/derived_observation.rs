//! Receiver-local dependencies an observed derived getter can read without hidden effects.

use std::collections::HashSet;

use beskid_analysis::syntax::{Expression, MethodDefinition, Program, Spanned, Statement};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

use super::{Accessor, property_access};
use crate::semantic_contract::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::semantic_contract) enum ObservationDependencyKind {
    Opaque,
    Cycle,
}

pub(in crate::semantic_contract) struct ObservationDependencyError {
    pub(in crate::semantic_contract) kind: ObservationDependencyKind,
    pub(in crate::semantic_contract) site: AstNodeKey,
}

impl ObservationDependencyError {
    fn opaque(site: AstNodeKey) -> Self {
        Self { kind: ObservationDependencyKind::Opaque, site }
    }
}

/// Unique, transitive backing fields of a pure derived getter in source traversal order.
/// Ordinary stored fields have no derived-dependency fact of their own.
pub fn observation_stored_dependencies(db: &dyn Db, getter: AstNodeKey) -> SemanticQueryResult<Vec<AstNodeKey>> {
    checked_observation_dependencies(db, getter).map_err(|error| {
        let context = match error.kind {
            ObservationDependencyKind::Opaque => "observe.opaque_dependency",
            ObservationDependencyKind::Cycle => "observe.dependency_cycle",
        };
        SemanticError::unavailable_at(context, error.site)
    })
}

pub(in crate::semantic_contract) fn checked_observation_dependencies(
    db: &dyn Db,
    getter: AstNodeKey,
) -> Result<Option<Vec<AstNodeKey>>, ObservationDependencyError> {
    if node_kind(db, getter).ok().flatten() != Some(NodeKind::MethodDefinition) {
        return Ok(None);
    }
    let owner = method_owner_declaration(db, getter).ok_or_else(|| ObservationDependencyError::opaque(getter))?;
    let mut walk = Dependencies { db, owner, active: HashSet::new(), visited: HashSet::new(), fields: Vec::new() };
    walk.getter(getter, getter)?;
    Ok(Some(walk.fields))
}

struct Dependencies<'db> {
    db: &'db dyn Db,
    owner: AstNodeKey,
    active: HashSet<AstNodeKey>,
    visited: HashSet<AstNodeKey>,
    fields: Vec<AstNodeKey>,
}

impl Dependencies<'_> {
    fn getter(&mut self, getter: AstNodeKey, site: AstNodeKey) -> Result<(), ObservationDependencyError> {
        if self.active.contains(&getter) {
            return Err(ObservationDependencyError { kind: ObservationDependencyKind::Cycle, site });
        }
        if !self.visited.insert(getter) {
            return Ok(());
        }
        let opaque = || ObservationDependencyError::opaque(site);
        if method_owner_declaration(self.db, getter) != Some(self.owner) {
            return Err(opaque());
        }
        let syntax =
            self.db.syntax_unit(getter.unit).filter(|syntax| syntax.accepts_key(self.db, getter)).ok_or_else(opaque)?;
        let index = syntax.syntax_index(self.db);
        let program = syntax.expanded_program(self.db);
        let method = index
            .node_at(program, getter.node)
            .and_then(|node| node.of::<MethodDefinition>())
            .filter(|method| method.is_property_getter)
            .ok_or_else(opaque)?;
        // The parser represents `=> expression` as one authored expression in a synthetic return.
        let [statement] = method.body.node.statements.as_slice() else { return Err(opaque()) };
        let Statement::Return(returned) = &statement.node else { return Err(opaque()) };
        let expression = returned.node.value.as_ref().ok_or_else(opaque)?;
        let body = child_site(program, index, getter, DynNodeRef::from(&method.body))?;
        let statement = child_site(program, index, body, DynNodeRef::from(statement))?;
        let returned = child_site(program, index, statement, DynNodeRef::from(returned))?;
        let expression_site = child_site(program, index, returned, DynNodeRef::from(expression))?;

        self.active.insert(getter);
        self.expression(getter, program, index, expression_site, expression)?;
        self.active.remove(&getter);
        Ok(())
    }

    fn expression(
        &mut self,
        getter: AstNodeKey,
        program: &Spanned<Program>,
        index: &SyntaxIndex,
        site: AstNodeKey,
        expression: &Spanned<Expression>,
    ) -> Result<(), ObservationDependencyError> {
        let expression_node = match &expression.node {
            Expression::Literal(_) => return Ok(()),
            Expression::Path(path) => DynNodeRef::from(path),
            Expression::Member(member) => DynNodeRef::from(member),
            Expression::Grouped(grouped) => DynNodeRef::from(grouped),
            Expression::Unary(unary) => DynNodeRef::from(unary),
            Expression::Binary(binary) => DynNodeRef::from(binary),
            Expression::Conditional(conditional) => DynNodeRef::from(conditional),
            _ => return Err(ObservationDependencyError::opaque(site)),
        };
        let site = child_site(program, index, site, expression_node)?;
        let mut visit = |child: &Spanned<Expression>| {
            let key = child_site(program, index, site, DynNodeRef::from(child))?;
            self.expression(getter, program, index, key, child)
        };
        match &expression.node {
            Expression::Grouped(grouped) => visit(&grouped.node.expr),
            Expression::Unary(unary) => visit(&unary.node.expr),
            Expression::Binary(binary) => {
                visit(&binary.node.left)?;
                visit(&binary.node.right)
            }
            Expression::Conditional(conditional) => {
                visit(&conditional.node.condition)?;
                visit(&conditional.node.then_branch)?;
                visit(&conditional.node.else_branch)
            }
            Expression::Path(_) | Expression::Member(_) => self.read(getter, site),
            _ => Err(ObservationDependencyError::opaque(site)),
        }
    }

    fn read(&mut self, getter: AstNodeKey, site: AstNodeKey) -> Result<(), ObservationDependencyError> {
        let Some(property) = property_access(self.db, site).ok().flatten().filter(|access| access.accessible) else {
            // Integer constants contribute no mutable dependency.
            return if constant_integer_literal(self.db, site).ok().flatten().is_some() {
                Ok(())
            } else {
                Err(ObservationDependencyError::opaque(site))
            };
        };
        let receiver = property.reference.receiver;
        if receiver != getter && implicit_method_receiver(self.db, receiver).ok().flatten() != Some(getter) {
            return Err(ObservationDependencyError::opaque(site));
        }
        let declaration = property.reference.declaration;
        match property.get {
            Some(Accessor::Automatic) => {
                if self.visited.insert(declaration) {
                    self.fields.push(declaration);
                }
                Ok(())
            }
            Some(Accessor::Custom(other)) if other == declaration => self.getter(other, site),
            _ => Err(ObservationDependencyError::opaque(site)),
        }
    }
}

fn child_site(
    program: &Spanned<Program>,
    index: &SyntaxIndex,
    parent: AstNodeKey,
    child: DynNodeRef<'_>,
) -> Result<AstNodeKey, ObservationDependencyError> {
    index
        .direct_child_id(program, parent.node, child)
        .map(|node| AstNodeKey { node, ..parent })
        .ok_or_else(|| ObservationDependencyError::opaque(parent))
}
