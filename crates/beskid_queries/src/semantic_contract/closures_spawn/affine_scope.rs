//! Escape rules of corelib `[ScopeBorrowed]` values (v0.6.7, ruling "Scope-borrowed values do
//! not escape"; `language-meta--memory-model--memory-and-references`).
//!
//! A value of a corelib `[ScopeBorrowed]` type borrows storage for the scope that uses it. One
//! pass per callable reports E1241 when such a value leaves that scope:
//!
//! - it is returned (`return v`, or the expression body of a lambda);
//! - it is stored: the value of an assignment to a field or an element, a field initializer of a
//!   struct literal whose type is not itself `[ScopeBorrowed]`, an array literal element or an
//!   enum constructor payload;
//! - it is an argument of a `spawn` target call, or of a parameter that may keep it: a `move`
//!   parameter, or a parameter whose type is a type parameter of the callee (a generic body
//!   `Store<T>(T v)` cannot know that it holds a borrowed view);
//! - an escaping lambda captures it. A lambda escapes when it is returned, is the value of an
//!   assignment to a field or an element, is an array literal element or an enum constructor
//!   payload, initializes a struct literal field without a `[Captures]` bound, or is the operand
//!   of `spawn`; a lambda bound to a local (by `let` or an assignment) escapes when a use of
//!   that local does, followed through further locals and captures (a flow deeper than eight
//!   locals counts as escaping). A lambda that is called in place or passed as a call argument
//!   stays in the scope; so does a lambda that initializes a `[Captures]`-bounded field of a
//!   struct literal, which is how a plan carries its borrowed source to the terminal that runs it.
//!
//! The finding is reported by the callable that holds the escaping site; a capture is reported by
//! the callable that holds the escaping lambda.

use beskid_analysis::syntax::AstNodeId;
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

use super::super::contracts::concrete_type_or_enum_declaration;
use super::super::transfer::corelib_scope_borrowed;
use super::super::*;
use super::affine::expression_spelling;
use super::affine_ownership::containing_fiber_callable;
use super::capture_bounds::{declared_parameters, direct_callee, field_captures_bound, with_declaration};

type Program = beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>;

/// How far an unannotated `let` chain is followed to its initializer.
const ALIAS_DEPTH: u8 = 8;

/// How many locals a function value is followed through before it counts as escaping.
const LAMBDA_FLOW_DEPTH: u8 = 8;

struct ScopeCheck<'a> {
    db: &'a dyn Db,
    program: &'a Program,
    index: &'a SyntaxIndex,
    key: AstNodeKey,
    findings: Vec<AffineFinding>,
}

/// The E1241 findings of `[ScopeBorrowed]` escapes in the callable `key`.
pub(super) fn scope_borrowed_findings(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
) -> Vec<AffineFinding> {
    let mut check = ScopeCheck { db, program, index, key, findings: Vec::new() };
    check.values();
    check.spawn_arguments();
    check.kept_arguments();
    check.captures();
    check.findings
}

/// Whether the lambda `lambda` of the callable `key` escapes it (see the module documentation):
/// the kept-parameter rule of `affine.rs` uses the same notion for a captured borrowed parameter.
pub(super) fn lambda_escapes(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
    lambda: AstNodeId,
) -> bool {
    ScopeCheck { db, program, index, key, findings: Vec::new() }.lambda_escapes(lambda)
}

/// The type parameters a callable's parameter types may name: its own and those of the type
/// that declares it.
pub(super) fn callee_type_parameters(db: &dyn Db, declaration: AstNodeKey) -> Vec<String> {
    let mut names = with_declaration(db, declaration, |node| {
        if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            return Some(function.generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>());
        }
        node.of::<beskid_analysis::syntax::MethodDefinition>()
            .map(|method| method.generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>())
    })
    .unwrap_or_default();
    if let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) {
        let index = syntax.syntax_index(db);
        if let Some(owner) = nearest_ancestor(index, declaration.node, |kind| kind == NodeKind::TypeDefinition)
            && let Some(definition) = index
                .node_at(syntax.expanded_program(db), owner)
                .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        {
            names.extend(definition.generics.iter().map(|generic| generic.node.name.clone()));
        }
    }
    names
}

impl ScopeCheck<'_> {
    fn at(&self, node: AstNodeId) -> AstNodeKey {
        AstNodeKey { node, ..self.key }
    }

    fn child(&self, parent: AstNodeId, child: DynNodeRef<'_>) -> Option<AstNodeId> {
        self.index.direct_child_id(self.program, parent, child)
    }

    fn owned_here(&self, node: AstNodeId) -> bool {
        containing_fiber_callable(self.index, node) == Some(self.key.node)
    }

    fn push(&mut self, site: AstNodeId, name: String, type_name: String, related: Option<AstNodeId>) {
        let Some(span) = self.index.node_at(self.program, site).and_then(|node| node.span()) else { return };
        if self.findings.iter().any(|finding| finding.site.node == site) {
            return;
        }
        self.findings.push(AffineFinding {
            kind: AffineFindingKind::ScopeBorrowedEscapes,
            site: self.at(site),
            span,
            name,
            detail: Some(type_name),
            related: related.map(|related| self.at(related)),
            fiber: false,
            fix_at: None,
        });
    }

    /// Escaping values: returns, stores and construction into anything but a scope-borrowed type.
    fn values(&mut self) {
        let mut sites = Vec::new();
        for node in self.index.ids_of_kind(NodeKind::ReturnStatement).filter(|node| self.owned_here(*node)) {
            let Some(statement) = self
                .index
                .node_at(self.program, node)
                .and_then(|node| node.of::<beskid_analysis::syntax::ReturnStatement>())
            else {
                continue;
            };
            sites.extend(statement.value.as_ref().and_then(|value| self.child(node, DynNodeRef::from(value))));
        }
        // The expression body of this lambda is its result.
        if self.index.kind(self.key.node) == Some(NodeKind::LambdaExpression)
            && let Some(lambda) = self
                .index
                .node_at(self.program, self.key.node)
                .and_then(|node| node.of::<beskid_analysis::syntax::LambdaExpression>())
            && let Some(body) = self.child(self.key.node, DynNodeRef::from(lambda.body.as_ref()))
            && self.index.kind(normalized_expression_node(self.index, body)) != Some(NodeKind::BlockExpression)
        {
            sites.push(body);
        }
        for node in self.index.ids_of_kind(NodeKind::AssignExpression).filter(|node| self.owned_here(*node)) {
            if let Some(value) = self.stored_assignment_value(node) {
                sites.push(value);
            }
        }
        for node in self.index.ids_of_kind(NodeKind::StructLiteralField).filter(|node| self.owned_here(*node)) {
            if self.literal_of_field(node).is_some_and(|literal| !self.scope_borrowed_literal(literal))
                && let Some(value) = self.field_value(node)
            {
                sites.push(value);
            }
        }
        for node in self.index.ids_of_kind(NodeKind::ArrayLiteralExpression).filter(|node| self.owned_here(*node)) {
            if let Some(literal) = self
                .index
                .node_at(self.program, node)
                .and_then(|node| node.of::<beskid_analysis::syntax::ArrayLiteralExpression>())
            {
                sites.extend(literal.elements.iter().filter_map(|element| self.child(node, DynNodeRef::from(element))));
            }
        }
        for node in self.index.ids_of_kind(NodeKind::EnumConstructorExpression).filter(|node| self.owned_here(*node)) {
            if let Some(constructor) = self
                .index
                .node_at(self.program, node)
                .and_then(|node| node.of::<beskid_analysis::syntax::EnumConstructorExpression>())
            {
                sites.extend(
                    constructor.args.iter().filter_map(|argument| self.child(node, DynNodeRef::from(argument))),
                );
            }
        }
        for site in sites {
            let value = normalized_expression_node(self.index, site);
            if let Some((type_name, related)) = self.view_of_expression(value, ALIAS_DEPTH) {
                let name = self.spelling(value);
                self.push(value, name, type_name, related);
            }
        }
    }

    /// Arguments of a `spawn` target call are transferred to the new fiber.
    fn spawn_arguments(&mut self) {
        let spawns =
            self.index.ids_of_kind(NodeKind::SpawnExpression).filter(|node| self.owned_here(*node)).collect::<Vec<_>>();
        for spawn in spawns {
            let Ok(Some(target)) = spawn_target(self.db, self.at(spawn)) else { continue };
            for argument in target.arguments.iter() {
                let value = normalized_expression_node(self.index, argument.node);
                if let Some((type_name, related)) = self.view_of_expression(value, ALIAS_DEPTH) {
                    let name = self.spelling(value);
                    self.push(value, name, type_name, related);
                }
            }
        }
    }

    /// Arguments of a parameter that may keep its value: a `move` parameter, or one typed by a
    /// type parameter of the callee (its generic body stores a `T` like any other value).
    fn kept_arguments(&mut self) {
        let calls =
            self.index.ids_of_kind(NodeKind::CallExpression).filter(|node| self.owned_here(*node)).collect::<Vec<_>>();
        for call in calls {
            let Some(syntax) = self
                .index
                .node_at(self.program, call)
                .and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
            else {
                continue;
            };
            let Some(declaration) = direct_callee(self.db, self.at(call)) else { continue };
            let Some((parameters, _)) = declared_parameters(self.db, declaration) else { continue };
            let generics = callee_type_parameters(self.db, declaration);
            for (position, argument) in syntax.args.iter().enumerate() {
                let Some(parameter) = parameters.get(position) else { continue };
                let generic = matches!(&parameter.node.ty.node, beskid_analysis::syntax::Type::Complex(path)
                    if path.node.segments.len() == 1
                        && generics.iter().any(|name| *name == path.node.segments[0].node.name.node.name));
                if !parameter.node.movable && !generic {
                    continue;
                }
                let Some(argument) = self.child(call, DynNodeRef::from(argument)) else { continue };
                let value = normalized_expression_node(self.index, argument);
                if let Some((type_name, related)) = self.view_of_expression(value, ALIAS_DEPTH) {
                    let name = self.spelling(value);
                    self.push(value, name, type_name, related);
                }
            }
        }
    }

    /// Uses inside an escaping lambda of this callable of a scope-borrowed binding declared
    /// outside that lambda.
    fn captures(&mut self) {
        let lambdas = self
            .index
            .ids_of_kind(NodeKind::LambdaExpression)
            // `containing_fiber_callable` counts a lambda as its own callable: judge the lambda
            // in the callable that holds it.
            .filter(|node| *node != self.key.node)
            .filter(|node| parent_node(self.index, *node).is_some_and(|parent| self.owned_here(parent)))
            .filter(|node| self.lambda_escapes(*node))
            .collect::<Vec<_>>();
        for lambda in lambdas {
            let uses = self
                .index
                .ids_of_kind(NodeKind::PathExpression)
                .filter(|node| is_ancestor(self.index, lambda, *node))
                .collect::<Vec<_>>();
            for node in uses {
                let Some(path) = self
                    .index
                    .node_at(self.program, node)
                    .and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
                else {
                    continue;
                };
                let Some(first) = path.path.node.segments.first() else { continue };
                let name = first.node.name.node.name.clone();
                let found = if name == "this" {
                    self.this_view(node).map(|type_name| (type_name, None))
                } else {
                    let Some(declaration) = resolve_lexical_declaration(self.program, self.index, node, &name) else {
                        continue;
                    };
                    if is_ancestor(self.index, lambda, declaration) {
                        continue;
                    }
                    self.view_of_binding(declaration, ALIAS_DEPTH).map(|type_name| (type_name, Some(declaration)))
                };
                if let Some((type_name, related)) = found {
                    self.push(node, name, type_name, related);
                }
            }
        }
    }

    /// Whether the lambda leaves the scope of this callable (see the module documentation).
    fn lambda_escapes(&self, lambda: AstNodeId) -> bool {
        self.value_escapes(lambda, LAMBDA_FLOW_DEPTH)
    }

    /// Whether the function value at `node` (a lambda, or a read of a local that holds one)
    /// leaves the callable: it is the operand of `spawn`, returned, the value of an assignment to
    /// a field or an element, an array literal element or an enum payload, or a struct literal
    /// field without a `[Captures]` bound. A value bound to a local (by `let` or an assignment)
    /// escapes when any use of that local does, including a capture by a lambda that escapes.
    /// The flow is followed `depth` locals deep; beyond that it counts as escaping (fail closed).
    fn value_escapes(&self, node: AstNodeId, depth: u8) -> bool {
        if let Some(spawn) = nearest_ancestor(self.index, node, |kind| kind == NodeKind::SpawnExpression)
            && spawn_target(self.db, self.at(spawn)).ok().flatten().is_some_and(|target| target.callee.node == node)
        {
            return true;
        }
        let mut child = node;
        while let Some(parent) = parent_node(self.index, child) {
            return match self.index.kind(parent) {
                Some(NodeKind::Expression | NodeKind::GroupedExpression) => {
                    child = parent;
                    continue;
                }
                Some(NodeKind::SpawnExpression) => true,
                Some(NodeKind::ReturnStatement | NodeKind::LambdaExpression) => true,
                Some(NodeKind::AssignExpression) => match self.assignment_parts(parent) {
                    Some((target, value)) if value == child => match self.local_target(target) {
                        Some(local) => self.local_escapes(local, depth),
                        None => true,
                    },
                    _ => false,
                },
                Some(NodeKind::LetStatement) => match self.let_binding(parent) {
                    Some(local) => self.local_escapes(local, depth),
                    None => true,
                },
                Some(NodeKind::ArrayLiteralExpression | NodeKind::EnumConstructorExpression) => true,
                Some(NodeKind::StructLiteralField) => !self.bounded_field(parent),
                _ => false,
            };
        }
        false
    }

    /// Whether a function value held by the local `declaration` escapes through one of its uses.
    fn local_escapes(&self, declaration: AstNodeId, depth: u8) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return true };
        let Some(name) = self
            .index
            .node_at(self.program, declaration)
            .and_then(|node| node.of::<beskid_analysis::syntax::Identifier>().map(|identifier| identifier.name.clone()))
        else {
            return true;
        };
        let uses = self
            .index
            .ids_of_kind(NodeKind::PathExpression)
            .filter(|node| is_ancestor(self.index, self.key.node, *node))
            .filter(|node| {
                self.index
                    .node_at(self.program, *node)
                    .and_then(|syntax| syntax.of::<beskid_analysis::syntax::PathExpression>())
                    .is_some_and(|path| {
                        matches!(path.path.node.segments.as_slice(), [segment] if segment.node.name.node.name == name)
                    })
                    && resolve_lexical_declaration(self.program, self.index, *node, &name) == Some(declaration)
            })
            .collect::<Vec<_>>();
        uses.into_iter().any(|use_node| {
            if self.value_escapes(use_node, depth) {
                return true;
            }
            // A capture by a lambda that escapes takes the value along.
            let mut current = use_node;
            while let Some(parent) = parent_node(self.index, current) {
                if parent == self.key.node {
                    break;
                }
                if self.index.kind(parent) == Some(NodeKind::LambdaExpression)
                    && !is_ancestor(self.index, parent, declaration)
                    && self.value_escapes(parent, depth)
                {
                    return true;
                }
                current = parent;
            }
            false
        })
    }

    /// The target and value of an assignment expression.
    fn assignment_parts(&self, assignment: AstNodeId) -> Option<(AstNodeId, AstNodeId)> {
        let syntax = self
            .index
            .node_at(self.program, assignment)
            .and_then(|node| node.of::<beskid_analysis::syntax::AssignExpression>())?;
        Some((
            self.child(assignment, DynNodeRef::from(syntax.target.as_ref()))?,
            self.child(assignment, DynNodeRef::from(syntax.value.as_ref()))?,
        ))
    }

    /// The declaration of the local an assignment target names, when it is a plain local.
    fn local_target(&self, target: AstNodeId) -> Option<AstNodeId> {
        let target = normalized_expression_node(self.index, target);
        let path = self.index.node_at(self.program, target)?.of::<beskid_analysis::syntax::PathExpression>()?;
        let [segment] = path.path.node.segments.as_slice() else { return None };
        resolve_lexical_declaration(self.program, self.index, target, &segment.node.name.node.name)
    }

    /// The declaring identifier of a `let` statement.
    fn let_binding(&self, statement: AstNodeId) -> Option<AstNodeId> {
        let syntax = self
            .index
            .node_at(self.program, statement)
            .and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())?;
        self.child(statement, DynNodeRef::from(&syntax.name))
    }

    /// The value of an assignment whose target is a field or an element (not a local).
    fn stored_assignment_value(&self, assignment: AstNodeId) -> Option<AstNodeId> {
        let syntax = self
            .index
            .node_at(self.program, assignment)
            .and_then(|node| node.of::<beskid_analysis::syntax::AssignExpression>())?;
        let local = matches!(&syntax.target.node,
            beskid_analysis::syntax::Expression::Path(path) if path.node.path.node.segments.len() == 1);
        if local {
            return None;
        }
        self.child(assignment, DynNodeRef::from(syntax.value.as_ref()))
    }

    fn field_value(&self, field: AstNodeId) -> Option<AstNodeId> {
        let syntax = self
            .index
            .node_at(self.program, field)
            .and_then(|node| node.of::<beskid_analysis::syntax::StructLiteralField>())?;
        self.child(field, DynNodeRef::from(&syntax.value))
    }

    fn literal_of_field(&self, field: AstNodeId) -> Option<AstNodeKey> {
        let literal = parent_node(self.index, field)?;
        let syntax = self
            .index
            .node_at(self.program, literal)
            .and_then(|node| node.of::<beskid_analysis::syntax::StructLiteralExpression>())?;
        resolve_type_declaration(self.db, self.at(literal), &syntax.path.node)
    }

    /// Construction of a scope-borrowed type keeps the borrow in the scope (v0.6.5 ruling
    /// "Borrowed storage in a scope-borrowed view").
    fn scope_borrowed_literal(&self, declaration: AstNodeKey) -> bool {
        corelib_scope_borrowed(self.db, declaration)
    }

    /// A struct literal field with a `[Captures]` bound.
    fn bounded_field(&self, field: AstNodeId) -> bool {
        let Some(owner) = self.literal_of_field(field) else { return false };
        let Some(syntax) = self
            .index
            .node_at(self.program, field)
            .and_then(|node| node.of::<beskid_analysis::syntax::StructLiteralField>())
        else {
            return false;
        };
        field_captures_bound(self.db, owner, &syntax.name.node.name).is_some()
    }

    fn spelling(&self, node: AstNodeId) -> String {
        self.index
            .node_at(self.program, node)
            .and_then(|node| node.of::<beskid_analysis::syntax::Expression>().map(expression_spelling))
            .filter(|spelling| spelling != "..")
            .or_else(|| {
                self.index.node_at(self.program, node).and_then(|node| {
                    node.of::<beskid_analysis::syntax::PathExpression>().map(|path| {
                        path.path
                            .node
                            .segments
                            .iter()
                            .map(|segment| segment.node.name.node.name.as_str())
                            .collect::<Vec<_>>()
                            .join(".")
                    })
                })
            })
            .unwrap_or_else(|| "value".to_owned())
    }

    /// The scope-borrowed type name of the value at `node`, with the binding it reads.
    fn view_of_expression(&self, node: AstNodeId, depth: u8) -> Option<(String, Option<AstNodeId>)> {
        let syntax = self.index.node_at(self.program, node)?;
        match self.index.kind(node) {
            Some(NodeKind::PathExpression) => {
                let path = syntax.of::<beskid_analysis::syntax::PathExpression>()?;
                if let [segment] = path.path.node.segments.as_slice() {
                    let name = segment.node.name.node.name.as_str();
                    if name == "this" {
                        return self.this_view(node).map(|type_name| (type_name, None));
                    }
                    if let Some(declaration) = resolve_lexical_declaration(self.program, self.index, node, name) {
                        return self
                            .view_of_binding(declaration, depth)
                            .map(|type_name| (type_name, Some(declaration)));
                    }
                }
                self.view_of_identity(node).map(|type_name| (type_name, None))
            }
            Some(NodeKind::StructLiteralExpression) => {
                let literal = syntax.of::<beskid_analysis::syntax::StructLiteralExpression>()?;
                let declaration = resolve_type_declaration(self.db, self.at(node), &literal.path.node)?;
                self.view_name(declaration).map(|type_name| (type_name, None))
            }
            Some(NodeKind::LambdaExpression | NodeKind::ArrayLiteralExpression | NodeKind::LiteralExpression) => None,
            _ => self.view_of_identity(node).map(|type_name| (type_name, None)),
        }
    }

    fn view_of_identity(&self, node: AstNodeId) -> Option<String> {
        let identity = generic_source_expression_identity(self.db, self.at(node)).ok()?;
        if !matches!(identity, GenericSourceTypeIdentity::Nominal { .. }) {
            return None;
        }
        let declaration = concrete_type_or_enum_declaration(self.db, self.at(node), &identity)?;
        self.view_name(declaration)
    }

    /// The scope-borrowed type of a lexical binding: its declared type, or the initializer of an
    /// unannotated `let`.
    fn view_of_binding(&self, declaration: AstNodeId, depth: u8) -> Option<String> {
        let parent = parent_node(self.index, declaration)?;
        let syntax = self.index.node_at(self.program, parent)?;
        let annotation = match self.index.kind(parent) {
            Some(NodeKind::Parameter) => Some(syntax.of::<beskid_analysis::syntax::Parameter>()?.ty.node.clone()),
            Some(NodeKind::LetStatement) => {
                let statement = syntax.of::<beskid_analysis::syntax::LetStatement>()?;
                match statement.type_annotation.as_ref() {
                    Some(annotation) => Some(annotation.node.clone()),
                    None => {
                        let initializer = self.child(parent, DynNodeRef::from(&statement.value))?;
                        let initializer = normalized_expression_node(self.index, initializer);
                        return depth
                            .checked_sub(1)
                            .and_then(|depth| self.view_of_expression(initializer, depth))
                            .map(|(type_name, _)| type_name);
                    }
                }
            }
            _ => None,
        };
        match annotation {
            Some(beskid_analysis::syntax::Type::Complex(path)) => {
                let declaration = resolve_type_declaration(self.db, self.at(parent), &path.node)?;
                self.view_name(declaration)
            }
            Some(_) => None,
            None => {
                let identity =
                    generic_source_local_identity(self.db, self.program, self.index, self.at(declaration), declaration)
                        .ok()?;
                if !matches!(identity, GenericSourceTypeIdentity::Nominal { .. }) {
                    return None;
                }
                let declaration = concrete_type_or_enum_declaration(self.db, self.at(declaration), &identity)?;
                self.view_name(declaration)
            }
        }
    }

    /// `this` inside a method of a scope-borrowed type.
    fn this_view(&self, node: AstNodeId) -> Option<String> {
        let method = nearest_ancestor(self.index, node, |kind| kind == NodeKind::MethodDefinition)?;
        let declaration = match method_this_type(self.db, self.at(method)) {
            Some(beskid_analysis::syntax::Type::Complex(path)) => {
                resolve_type_declaration(self.db, self.at(method), &path.node)?
            }
            Some(_) => return None,
            None => self.at(nearest_ancestor(self.index, method, |kind| kind == NodeKind::TypeDefinition)?),
        };
        self.view_name(declaration)
    }

    fn view_name(&self, declaration: AstNodeKey) -> Option<String> {
        if !corelib_scope_borrowed(self.db, declaration) {
            return None;
        }
        with_declaration(self.db, declaration, |node| {
            node.of::<beskid_analysis::syntax::TypeDefinition>().map(|definition| definition.name.node.name.clone())
        })
    }
}
