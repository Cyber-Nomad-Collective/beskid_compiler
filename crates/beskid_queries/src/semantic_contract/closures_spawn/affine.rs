//! Affine ownership analysis at isolate crossings (`language-meta--memory-model--memory-and-references`,
//! 0.6.7 owner ruling amending the 0.6.2 decisions D1-D4; `language-meta--evaluation--fibers-and-spawn`,
//! "Fiber handles use the general affine analysis").
//!
//! Move semantics apply only where a value crosses to another isolate: a port send (the payload of
//! `__port_send` or the `move` parameter of the corelib `Port.Send`) and a `move` capture inside a
//! `[Captures]`-bounded lambda (`Scope.Spawn` tasks, `.Parallel()` steps; the captures themselves
//! are judged in `legality/captures.rs`). Ordinary local code never needs `move`: reads, aliases,
//! stores, returns and a local `move` neither consume nor report. At a crossing the analysis
//! still tells owning from borrowed names, so a borrowed value cannot be handed over (E1239,
//! E1241), an owning Moved name needs `move` (E1242), and a use after the crossing consumed the
//! name is E1234 (E1240 in a loop without reassignment). `move x.f` needs a canonical empty
//! value (E1243). A `Fiber` handle keeps the 0.6.0 handle rules (every use but `Cancel`
//! consumes). Branch arms (`if`/`else`, `match`) are alternatives, and an assignment to a Moved
//! place makes the name owning again.

use std::collections::HashMap;

use beskid_analysis::syntax::AstNodeId;
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

use super::super::transfer::{
    TransferClass, TransferNeverReason, marker_attributes, transfer_class,
    transfer_class_of_type,
};
use super::super::*;
use super::affine_ownership::{containing_fiber_callable, fiber_owner_seed};
use super::capture_bounds::{call_argument_captures_bound, declared_parameters, direct_callee, field_captures_bound};

type Program = beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>;
type BranchPath = Vec<(AstNodeId, AstNodeId)>;

struct Owner {
    name: String,
    fiber: bool,
    /// Owning at the declaration: at a crossing, the only name for its value in the callable
    /// (a `move` parameter, or a local initialized from an owned expression).
    owning: bool,
    /// Another name in this callable takes the value (`let y = x`, `y = x`, `h.f = x`, a literal
    /// element, a lambda capture): the name is never the only name, so it cannot cross.
    aliased: bool,
    /// Assignments in source order: their branch path and whether the assigned value is owned.
    /// The latest one on the path of a use decides whether the name is owning there.
    assignments: Vec<(BranchPath, bool)>,
    /// Prior consumptions, in source order.
    consumed: Vec<Consumption>,
}

/// One consuming use: its branch path, its site, and the source ranges it does not reach.
struct Consumption {
    branches: BranchPath,
    site: AstNodeId,
    /// `[start, end)` byte ranges after a `return`, `break` or `continue` that follows the
    /// consumption on its own path: a consumed state flows only along edges that do not diverge.
    unreachable: Vec<(usize, usize)>,
}

/// How a statement leaves the straight-line flow of its block on every path.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Divergence {
    /// Every path returns: control leaves the callable (or the enclosing lambda).
    Return,
    /// Every path leaves through `break`/`continue` of the innermost loop (or a `return`);
    /// `continues` when some path starts the loop's next iteration.
    Loop { continues: bool },
}

impl Divergence {
    /// Both arms of an `if`/`else` diverge: the weaker exit of the two.
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Return, Self::Return) => Self::Return,
            (Self::Return, Self::Loop { continues }) | (Self::Loop { continues }, Self::Return) => Self::Loop { continues },
            (Self::Loop { continues: left }, Self::Loop { continues: right }) => Self::Loop { continues: left || right },
        }
    }
}

fn statement_divergence(statement: &beskid_analysis::syntax::Statement) -> Option<Divergence> {
    use beskid_analysis::syntax::Statement;
    match statement {
        Statement::Return(_) => Some(Divergence::Return),
        Statement::Break(_) => Some(Divergence::Loop { continues: false }),
        Statement::Continue(_) => Some(Divergence::Loop { continues: true }),
        Statement::If(statement) => if_divergence(&statement.node),
        _ => None,
    }
}

fn if_divergence(statement: &beskid_analysis::syntax::IfStatement) -> Option<Divergence> {
    let then = block_divergence(&statement.then_block.node)?;
    let other = match &statement.else_branch.as_ref()?.node {
        beskid_analysis::syntax::ElseBranch::Block(block) => block_divergence(&block.node)?,
        beskid_analysis::syntax::ElseBranch::If(nested) => if_divergence(&nested.node)?,
    };
    Some(then.join(other))
}

fn block_divergence(block: &beskid_analysis::syntax::Block) -> Option<Divergence> {
    block.statements.iter().find_map(|statement| statement_divergence(&statement.node))
}

/// Where control goes after a consumption that a diverging statement follows.
#[derive(Default)]
struct Exits {
    unreachable: Vec<(usize, usize)>,
    /// Loops around the diverging statements whose next iteration the consumption does not
    /// reach: every loop a `return` leaves, the loop a `break` leaves.
    exited: Vec<AstNodeId>,
}

/// Where a use sits, after `Expression` and grouping wrappers.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UseContext {
    Callee(AstNodeId),
    Argument(AstNodeId, usize),
    LetValue(AstNodeId),
    ReturnValue,
    AssignValue { target_is_local: bool },
    AssignTarget,
    Construction,
    Move(AstNodeId),
    Other,
}

/// The lambdas a use crosses between itself and its binding.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum LambdaCrossing {
    None,
    /// A lambda handed directly to `spawn`: it runs once.
    OneShot,
    /// A lambda passed to a `[Captures]` parameter or assigned to a `[Captures]` field.
    Bounded,
    /// Any other lambda: repeatable, so its captures are borrows.
    Ordinary,
}

struct Analysis<'a> {
    db: &'a dyn Db,
    program: &'a Program,
    index: &'a SyntaxIndex,
    key: AstNodeKey,
    owners: HashMap<AstNodeId, Owner>,
    order: Vec<AstNodeId>,
    this_declaration: Option<AstNodeId>,
    findings: Vec<AffineFinding>,
}

enum Event {
    Use { position: usize, node: AstNodeId, declaration: AstNodeId, segments: Vec<String> },
    Revive { position: usize, node: AstNodeId, declaration: AstNodeId },
}

impl Event {
    fn position(&self) -> usize {
        match self {
            Self::Use { position, .. } | Self::Revive { position, .. } => *position,
        }
    }
}

pub(super) fn analyze(db: &dyn Db, program: &Program, index: &SyntaxIndex, key: AstNodeKey) -> AffineOwnership {
    let mut analysis = Analysis {
        db,
        program,
        index,
        key,
        owners: HashMap::new(),
        order: Vec::new(),
        this_declaration: None,
        findings: Vec::new(),
    };
    analysis.check_modifiers();
    analysis.seed();
    analysis.mark_aliases();
    analysis.walk();
    analysis.check_move_operands();
    analysis.check_exchange_places();
    analysis.findings.extend(super::affine_scope::scope_borrowed_findings(db, program, index, key));
    let bindings = analysis
        .order
        .iter()
        .filter_map(|declaration| {
            let owner = analysis.owners.get(declaration)?;
            Some(AffineBinding {
                declaration: analysis.at(*declaration),
                name: owner.name.clone(),
                owning: owner.owning && !owner.aliased,
                fiber: owner.fiber,
            })
        })
        .collect::<Vec<_>>();
    AffineOwnership { callable: key, bindings: bindings.into(), findings: analysis.findings.into() }
}

/// A `move` parameter or `move this` admits a Moved class, some Transferable (decided per
/// instantiation, v064 "per-instantiation move"), or a class that is Never only because a type
/// parameter is unbounded while the type is an array or an `[Isolated]` nominal.
fn rooted_in_type_parameter(reason: &TransferNeverReason) -> bool {
    match reason {
        TransferNeverReason::UnboundedTypeParameter { .. } => true,
        TransferNeverReason::ArrayElement(inner) => rooted_in_type_parameter(inner),
        TransferNeverReason::Field { class, .. } => {
            matches!(class.as_ref(), TransferClass::Never(inner) if rooted_in_type_parameter(inner))
        }
        _ => false,
    }
}

/// The canonical empty value of a field type, for a destructive read.
fn canonical_empty(identity: &GenericSourceTypeIdentity) -> Option<CanonicalEmpty> {
    match identity {
        GenericSourceTypeIdentity::Array(_) => Some(CanonicalEmpty::EmptyArray),
        GenericSourceTypeIdentity::Nominal { qualified_name, .. } => {
            match qualified_name.rsplit(['.', ':']).next().unwrap_or(qualified_name) {
                "Option" => Some(CanonicalEmpty::OptionNone),
                "List" => Some(CanonicalEmpty::EmptyList),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(in crate::semantic_contract) fn identity_spelling(identity: &GenericSourceTypeIdentity) -> String {
    match identity {
        GenericSourceTypeIdentity::Abi(ty) => format!("{ty:?}").to_lowercase(),
        GenericSourceTypeIdentity::Array(element) => format!("{}[]", identity_spelling(element)),
        GenericSourceTypeIdentity::Nominal { qualified_name, arguments } => {
            let name = qualified_name.rsplit('.').next().unwrap_or(qualified_name);
            if arguments.is_empty() {
                name.to_owned()
            } else {
                format!("{name}<{}>", arguments.iter().map(identity_spelling).collect::<Vec<_>>().join(", "))
            }
        }
        GenericSourceTypeIdentity::Function { .. } => "function".to_owned(),
        GenericSourceTypeIdentity::Contract { qualified_name, .. } => {
            qualified_name.rsplit('.').next().unwrap_or(qualified_name).to_owned()
        }
    }
}

/// A short spelling of an index expression for a diagnostic (`xs[i]`, `xs[0]`); anything but a
/// path or a literal is `..`.
pub(super) fn expression_spelling(expression: &beskid_analysis::syntax::Expression) -> String {
    use beskid_analysis::syntax::{Expression, Literal};
    match expression {
        Expression::Path(path) => {
            path.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>().join(".")
        }
        Expression::Literal(literal) => match &literal.node.literal.node {
            Literal::Integer(text) | Literal::Float(text) => text.clone(),
            _ => "..".to_owned(),
        },
        Expression::Grouped(grouped) => expression_spelling(&grouped.node.expr.node),
        _ => "..".to_owned(),
    }
}

impl<'a> Analysis<'a> {
    fn at(&self, node: AstNodeId) -> AstNodeKey {
        AstNodeKey { node, ..self.key }
    }

    fn span(&self, node: AstNodeId) -> Option<SourceSpan> {
        self.index.node_at(self.program, node).and_then(|node| node.span())
    }

    fn child(&self, parent: AstNodeId, child: DynNodeRef<'_>) -> Option<AstNodeId> {
        self.index.direct_child_id(self.program, parent, child)
    }

    fn push(
        &mut self,
        kind: AffineFindingKind,
        site: AstNodeId,
        name: &str,
        detail: Option<String>,
        related: Option<AstNodeId>,
        fiber: bool,
    ) {
        let Some(span) = self.span(site) else { return };
        if self.findings.iter().any(|finding| finding.kind == kind && finding.site.node == site) {
            return;
        }
        self.findings.push(AffineFinding {
            kind,
            site: self.at(site),
            span,
            name: name.to_owned(),
            detail,
            related: related.map(|related| self.at(related)),
            fiber,
        });
    }

    fn type_class(&self, site: AstNodeId, ty: &beskid_analysis::syntax::Type) -> Option<TransferClass> {
        transfer_class_of_type(self.db, self.at(site), ty).ok()
    }

    fn local_class(&self, declaration: AstNodeId) -> Option<TransferClass> {
        let identity =
            generic_source_local_identity(self.db, self.program, self.index, self.at(declaration), declaration).ok()?;
        transfer_class(self.db, self.at(declaration), &identity).ok()
    }

    /// E1239 on a `move` modifier whose parameter type is not Moved, and on `move this` outside an
    /// `[Isolated]` or Moved-class owner.
    fn check_modifiers(&mut self) {
        let (db, program, index, key) = (self.db, self.program, self.index, self.key);
        let Some(node) = index.node_at(program, key.node) else { return };
        let (parameters, receiver) = if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            (&function.parameters, None)
        } else if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
            (&method.parameters, method.receiver.as_ref())
        } else {
            return;
        };
        for parameter in parameters.iter().filter(|parameter| parameter.node.movable) {
            let Some(site) = index.direct_child_id(program, key.node, DynNodeRef::from(parameter)) else { continue };
            let Some(class) = self.type_class(site, &parameter.node.ty.node) else { continue };
            if !self.movable_type(site, &parameter.node.ty.node, &class) {
                let name = parameter.node.name.node.name.clone();
                self.push(AffineFindingKind::MoveOfNonOwningName, site, &name, Some(class.name().to_owned()), None, false);
            }
        }
        if let Some(receiver) = receiver.filter(|receiver| receiver.node.movable)
            && let Some(site) = index.direct_child_id(program, key.node, DynNodeRef::from(receiver))
            && let Some(owner) = method_this_type(db, key)
            && let Some(class) = self.type_class(key.node, &owner)
            && !self.movable_type(key.node, &owner, &class)
        {
            self.push(AffineFindingKind::MoveOfNonOwningName, site, "this", Some(class.name().to_owned()), None, false);
        }
    }

    fn movable_type(&self, site: AstNodeId, ty: &beskid_analysis::syntax::Type, class: &TransferClass) -> bool {
        match class {
            TransferClass::Moved | TransferClass::SomeTransferable => true,
            // An `[Isolated]` nominal whose class depends on unbounded type parameters is decided
            // per instantiation (`List<T>`, `HashTable<K, V>` inside their own definitions).
            TransferClass::Never(reason) if rooted_in_type_parameter(reason) => {
                matches!(ty, beskid_analysis::syntax::Type::Complex(path) if self.isolated_nominal(site, &path.node))
            }
            _ => false,
        }
    }

    fn isolated_nominal(&self, site: AstNodeId, path: &beskid_analysis::syntax::Path) -> bool {
        let Some(declaration) = resolve_type_declaration(self.db, self.at(site), path) else { return false };
        super::capture_bounds::with_declaration(self.db, declaration, |node| {
            let definition = node.of::<beskid_analysis::syntax::TypeDefinition>()?;
            Some(
                marker_attributes(self.db, declaration, &definition.attributes)
                    .iter()
                    .any(|(marker, _)| *marker == super::super::transfer::TransferMarkerAttribute::Isolated),
            )
        })
        .unwrap_or(false)
    }

    fn insert(&mut self, declaration: AstNodeId, name: String, fiber: bool, owning: bool) {
        if self
            .owners
            .insert(declaration, Owner { name, fiber, owning, aliased: false, assignments: Vec::new(), consumed: Vec::new() })
            .is_none()
        {
            self.order.push(declaration);
        }
    }

    fn seed(&mut self) {
        let (db, program, index, key) = (self.db, self.program, self.index, self.key);
        if index.kind(key.node) == Some(NodeKind::MethodDefinition)
            && let Some(method) = index.node_at(program, key.node).and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
            && let Some(owner) = method_this_type(db, key)
            && self.type_class(key.node, &owner) == Some(TransferClass::Moved)
        {
            let declaration = method
                .receiver
                .as_ref()
                .and_then(|receiver| self.child(key.node, DynNodeRef::from(receiver)))
                .unwrap_or(key.node);
            let owning = method.receiver.as_ref().is_some_and(|receiver| receiver.node.movable);
            self.this_declaration = Some(declaration);
            self.insert(declaration, "this".to_owned(), false, owning);
        }
        let mut declarations = Vec::new();
        for kind in [NodeKind::Parameter, NodeKind::LetStatement, NodeKind::LambdaParameter, NodeKind::ForStatement] {
            declarations.extend(
                index.ids_of_kind(kind).filter(|node| containing_fiber_callable(index, *node) == Some(key.node)),
            );
        }
        declarations.sort_by_key(|node| self.span(*node).map_or(usize::MAX, |span| span.start));
        for node in declarations {
            if let Some(owner) = fiber_owner_seed(db, program, index, self.at(node)) {
                let name = self.identifier_name(owner);
                self.insert(owner, name, true, true);
                continue;
            }
            let Some(syntax) = index.node_at(program, node) else { continue };
            if let Some(parameter) = syntax.of::<beskid_analysis::syntax::Parameter>() {
                let Some(name) = self.child(node, DynNodeRef::from(&parameter.name)) else { continue };
                // A `move T` parameter over a `Transferable` type parameter is owning inside the
                // generic definition (v064 "per-instantiation move").
                match self.type_class(node, &parameter.ty.node) {
                    Some(TransferClass::Moved) => {
                        self.insert(name, parameter.name.node.name.clone(), false, parameter.movable);
                    }
                    Some(TransferClass::SomeTransferable) if parameter.movable => {
                        self.insert(name, parameter.name.node.name.clone(), false, true);
                    }
                    _ => {}
                }
            } else if let Some(statement) = syntax.of::<beskid_analysis::syntax::LetStatement>() {
                let Some(name) = self.child(node, DynNodeRef::from(&statement.name)) else { continue };
                let class = match statement.type_annotation.as_ref() {
                    Some(annotation) => self.type_class(node, &annotation.node),
                    None => self.local_class(name),
                };
                if class == Some(TransferClass::Moved) {
                    let owning = self.initializer_is_owned(node, &statement.value);
                    self.insert(name, statement.name.node.name.clone(), false, owning);
                }
            } else if let Some(parameter) = syntax.of::<beskid_analysis::syntax::LambdaParameter>() {
                let Some(name) = self.child(node, DynNodeRef::from(&parameter.name)) else { continue };
                if self.local_class(name) == Some(TransferClass::Moved) {
                    self.insert(name, parameter.name.node.name.clone(), false, false);
                }
            } else if let Some(statement) = syntax.of::<beskid_analysis::syntax::ForStatement>() {
                let Some(name) = self.child(node, DynNodeRef::from(&statement.iterator)) else { continue };
                if self.local_class(name) == Some(TransferClass::Moved) {
                    self.insert(name, statement.iterator.node.name.clone(), false, false);
                }
            }
        }
    }

    /// Marks every owner whose value another name of this callable takes: the value of a `let`,
    /// of an assignment or of a literal element, or a capture by a lambda without `move`.
    fn mark_aliases(&mut self) {
        let mut aliased = Vec::new();
        for node in self.index.ids_of_kind(NodeKind::PathExpression) {
            if !is_ancestor(self.index, self.key.node, node) {
                continue;
            }
            let Some(path) =
                self.index.node_at(self.program, node).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
            else {
                continue;
            };
            if path.path.node.segments.len() != 1 {
                continue;
            }
            let Some(declaration) = self.declaration_of(node, &path.path.node) else { continue };
            if self.owners.get(&declaration).is_none_or(|owner| owner.fiber) {
                continue;
            }
            let takes = match self.context(node) {
                UseContext::LetValue(_) | UseContext::AssignValue { .. } | UseContext::Construction => true,
                UseContext::Move(_) => false,
                // A capture by a `[Captures]`-bounded lambda is itself a crossing operand
                // (judged in `legality/captures.rs`); any other closure keeps a name.
                _ => matches!(self.lambda_crossing(node, declaration), LambdaCrossing::OneShot | LambdaCrossing::Ordinary),
            };
            if takes {
                aliased.push(declaration);
            }
        }
        for declaration in aliased {
            if let Some(owner) = self.owners.get_mut(&declaration) {
                owner.aliased = true;
            }
        }
    }

    fn identifier_name(&self, node: AstNodeId) -> String {
        self.index
            .node_at(self.program, node)
            .and_then(|node| node.of::<beskid_analysis::syntax::Identifier>().map(|identifier| identifier.name.clone()))
            .unwrap_or_default()
    }

    /// An owned expression makes the new local the only name for its value: a call result, a
    /// literal or an element or member of one (`Make()[0]`). Any read of a name or a place, and
    /// any other expression, is conservatively not owned (the crossing fails closed).
    fn initializer_is_owned(
        &self,
        statement: AstNodeId,
        value: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
    ) -> bool {
        let Some(mut node) = self.child(statement, DynNodeRef::from(value)) else { return false };
        while matches!(self.index.kind(node), Some(NodeKind::Expression | NodeKind::GroupedExpression)) {
            let Some(child) = self.index.children(node).and_then(|children| children.first()).copied() else {
                break;
            };
            node = child;
        }
        match self.index.kind(node) {
            Some(NodeKind::IndexExpression | NodeKind::MemberExpression) => self.projection_of_temporary(node),
            Some(
                NodeKind::CallExpression
                | NodeKind::LiteralExpression
                | NodeKind::StructLiteralExpression
                | NodeKind::ArrayLiteralExpression
                | NodeKind::EnumConstructorExpression,
            ) => true,
            _ => false,
        }
    }

    /// Whether the member/index chain at `node` is rooted in an owned temporary: a call result
    /// or a literal (v065 ruling "Stored projections").
    fn projection_of_temporary(&self, node: AstNodeId) -> bool {
        let mut current = node;
        loop {
            let Some(syntax) = self.index.node_at(self.program, current) else { return false };
            let target = match self.index.kind(current) {
                Some(NodeKind::Expression | NodeKind::GroupedExpression) => {
                    let Some(child) = self.index.children(current).and_then(|children| children.first()).copied() else {
                        return false;
                    };
                    current = child;
                    continue;
                }
                Some(NodeKind::IndexExpression) => {
                    syntax.of::<beskid_analysis::syntax::IndexExpression>().map(|index| DynNodeRef::from(index.target.as_ref()))
                }
                Some(NodeKind::MemberExpression) => {
                    syntax.of::<beskid_analysis::syntax::MemberExpression>().map(|member| DynNodeRef::from(member.target.as_ref()))
                }
                Some(
                    NodeKind::CallExpression
                    | NodeKind::StructLiteralExpression
                    | NodeKind::ArrayLiteralExpression
                    | NodeKind::EnumConstructorExpression,
                ) => return true,
                _ => return false,
            };
            let Some(next) = target.and_then(|target| self.child(current, target)) else { return false };
            current = next;
        }
    }

    /// The outermost place `node[..]`, `node[..].f`, ... that projects the use `node` through
    /// element and member reads, with its spelling; `None` when the use is not projected.
    fn projected_place(&self, node: AstNodeId, root: &str) -> Option<(AstNodeId, String)> {
        let mut spelling = root.to_owned();
        let mut top = None;
        let mut child = node;
        while let Some(parent) = parent_node(self.index, child) {
            let syntax = self.index.node_at(self.program, parent)?;
            match self.index.kind(parent) {
                Some(NodeKind::Expression | NodeKind::GroupedExpression) => {}
                Some(NodeKind::IndexExpression) => {
                    let index = syntax.of::<beskid_analysis::syntax::IndexExpression>()?;
                    if self.child(parent, DynNodeRef::from(index.target.as_ref())) != Some(child) {
                        break;
                    }
                    spelling = format!("{spelling}[{}]", expression_spelling(&index.index.node));
                    top = Some(parent);
                }
                Some(NodeKind::MemberExpression) => {
                    let member = syntax.of::<beskid_analysis::syntax::MemberExpression>()?;
                    if self.child(parent, DynNodeRef::from(member.target.as_ref())) != Some(child) {
                        break;
                    }
                    spelling = format!("{spelling}.{}", member.member.node.name);
                    top = Some(parent);
                }
                _ => break,
            }
            child = parent;
        }
        top.map(|top| (top, spelling))
    }

    /// The binding the first segment of `path` names: a lexical local or the method receiver.
    fn declaration_of(&self, reference: AstNodeId, path: &beskid_analysis::syntax::Path) -> Option<AstNodeId> {
        let first = path.segments.first()?;
        let name = first.node.name.node.name.as_str();
        if name == "this" {
            return self.this_declaration;
        }
        resolve_lexical_declaration(self.program, self.index, reference, name)
    }

    fn walk(&mut self) {
        let mut events = Vec::new();
        for node in self.index.ids_of_kind(NodeKind::PathExpression) {
            if !is_ancestor(self.index, self.key.node, node) {
                continue;
            }
            let Some(path) =
                self.index.node_at(self.program, node).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
            else {
                continue;
            };
            // Every lexical use is collected: a `Fiber` alias (`let next = handle`) becomes an
            // owner only when its initializer is visited.
            let Some(declaration) = self.declaration_of(node, &path.path.node) else { continue };
            let Some(span) = self.span(node) else { continue };
            let segments =
                path.path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
            if segments.len() == 1 && self.context(node) == UseContext::AssignTarget {
                let assignment = self.enclosing_assignment(node);
                if let Some(assignment) = assignment
                    && let Some(span) = self.span(assignment)
                {
                    events.push(Event::Revive { position: span.end, node: assignment, declaration });
                }
                continue;
            }
            events.push(Event::Use { position: span.start, node, declaration, segments });
        }
        events.sort_by_key(Event::position);
        for event in events {
            match event {
                Event::Use { node, declaration, segments, .. } => self.visit_use(node, declaration, &segments),
                Event::Revive { node, declaration, .. } => {
                    if !self.owners.contains_key(&declaration) {
                        continue;
                    }
                    let (path, _) = self.path_to_callable(node, declaration);
                    let owned = self.assigned_value_is_owned(node);
                    if let Some(owner) = self.owners.get_mut(&declaration) {
                        owner.consumed.retain(|consumption| !path.iter().all(|step| consumption.branches.contains(step)));
                        owner.assignments.push((path, owned));
                    }
                }
            }
        }
    }

    /// The value of the assignment `assignment` is owned (see [`Self::initializer_is_owned`]):
    /// `buf = Join(buf, data)` makes a borrowed `mut` local owning, `buf = s.f` borrowed.
    fn assigned_value_is_owned(&self, assignment: AstNodeId) -> bool {
        self.index
            .node_at(self.program, assignment)
            .and_then(|node| node.of::<beskid_analysis::syntax::AssignExpression>())
            .is_none_or(|syntax| self.initializer_is_owned(assignment, &syntax.value))
    }

    /// Whether the name is owning at a use on `branches`: the latest assignment whose arms
    /// all enclose the use decides; an assignment in an arm the use is not inside does not
    /// reach it on every path, so the earlier state stays.
    fn owning_at(&self, owner: &Owner, branches: &BranchPath) -> bool {
        !owner.aliased
            && owner
            .assignments
            .iter()
            .rev()
            .find(|(path, _)| path.iter().all(|step| branches.contains(step)))
            .map_or(owner.owning, |(_, owned)| *owned)
    }

    fn enclosing_assignment(&self, node: AstNodeId) -> Option<AstNodeId> {
        let mut current = node;
        while let Some(parent) = parent_node(self.index, current) {
            match self.index.kind(parent) {
                Some(NodeKind::Expression | NodeKind::GroupedExpression) => current = parent,
                Some(NodeKind::AssignExpression) => return Some(parent),
                _ => return None,
            }
        }
        None
    }

    fn context(&self, node: AstNodeId) -> UseContext {
        let mut child = node;
        while let Some(parent) = parent_node(self.index, child) {
            let Some(syntax) = self.index.node_at(self.program, parent) else { return UseContext::Other };
            return match self.index.kind(parent) {
                Some(NodeKind::Expression | NodeKind::GroupedExpression) => {
                    child = parent;
                    continue;
                }
                Some(NodeKind::MoveExpression) => UseContext::Move(parent),
                Some(NodeKind::CallExpression) => {
                    let Some(call) = syntax.of::<beskid_analysis::syntax::CallExpression>() else {
                        return UseContext::Other;
                    };
                    if self.child(parent, DynNodeRef::from(call.callee.as_ref())) == Some(child) {
                        UseContext::Callee(parent)
                    } else {
                        call.args
                            .iter()
                            .position(|argument| self.child(parent, DynNodeRef::from(argument)) == Some(child))
                            .map_or(UseContext::Other, |position| UseContext::Argument(parent, position))
                    }
                }
                Some(NodeKind::LetStatement) => UseContext::LetValue(parent),
                Some(NodeKind::ReturnStatement | NodeKind::LambdaExpression) => UseContext::ReturnValue,
                Some(NodeKind::AssignExpression) => {
                    let Some(assignment) = syntax.of::<beskid_analysis::syntax::AssignExpression>() else {
                        return UseContext::Other;
                    };
                    if self.child(parent, DynNodeRef::from(assignment.target.as_ref())) == Some(child) {
                        UseContext::AssignTarget
                    } else {
                        let target_is_local = matches!(&assignment.target.node,
                            beskid_analysis::syntax::Expression::Path(path) if path.node.path.node.segments.len() == 1);
                        UseContext::AssignValue { target_is_local }
                    }
                }
                Some(
                    NodeKind::StructLiteralField
                    | NodeKind::ArrayLiteralExpression
                    | NodeKind::EnumConstructorExpression,
                ) => UseContext::Construction,
                _ => UseContext::Other,
            };
        }
        UseContext::Other
    }

    /// The branch arms between `node` and the callable, and the loops between `node` and the
    /// declaration (loops entered after the binding was declared).
    fn path_to_callable(&self, node: AstNodeId, declaration: AstNodeId) -> (BranchPath, Vec<AstNodeId>) {
        let declaration_start = self.span(declaration).map_or(0, |span| span.start);
        let mut branches = BranchPath::new();
        let mut loops = Vec::new();
        let mut child = node;
        while let Some(parent) = parent_node(self.index, child) {
            match self.index.kind(parent) {
                Some(NodeKind::IfStatement)
                    if matches!(self.index.kind(child), Some(NodeKind::Block | NodeKind::ElseBranch)) =>
                {
                    branches.push((parent, child));
                }
                Some(NodeKind::MatchExpression) if self.index.kind(child) == Some(NodeKind::MatchArm) => {
                    branches.push((parent, child));
                }
                Some(NodeKind::WhileStatement | NodeKind::ForStatement)
                    if self.span(parent).is_some_and(|span| declaration_start < span.start) =>
                {
                    loops.push(parent);
                }
                _ => {}
            }
            if parent == self.key.node {
                break;
            }
            child = parent;
        }
        (branches, loops)
    }

    fn lambda_crossing(&self, node: AstNodeId, declaration: AstNodeId) -> LambdaCrossing {
        let mut crossing = LambdaCrossing::None;
        let mut current = node;
        while let Some(parent) = parent_node(self.index, current) {
            if parent == self.key.node {
                break;
            }
            if self.index.kind(parent) == Some(NodeKind::LambdaExpression) && !is_ancestor(self.index, parent, declaration) {
                crossing = crossing.max(self.lambda_kind(parent));
            }
            current = parent;
        }
        crossing
    }

    fn lambda_kind(&self, lambda: AstNodeId) -> LambdaCrossing {
        let one_shot = nearest_ancestor(self.index, lambda, |kind| kind == NodeKind::SpawnExpression)
            .and_then(|node| spawn_target(self.db, self.at(node)).ok().flatten())
            .is_some_and(|target| target.callee.node == lambda);
        if one_shot {
            return LambdaCrossing::OneShot;
        }
        if self.lambda_is_bounded(lambda) { LambdaCrossing::Bounded } else { LambdaCrossing::Ordinary }
    }

    /// A lambda literal passed to a `[Captures]` parameter or initializing a `[Captures]` field.
    fn lambda_is_bounded(&self, lambda: AstNodeId) -> bool {
        match self.context(lambda) {
            // The body of `__isolate_spawn` runs on another isolate, like a bounded task body.
            UseContext::Argument(call, position) => {
                call_argument_captures_bound(self.db, self.at(call), position).is_some()
                    || self.callee_spelling(call).as_deref() == Some("__isolate_spawn")
            }
            UseContext::Construction => self.literal_field_bound(lambda),
            _ => false,
        }
    }

    fn literal_field_bound(&self, value: AstNodeId) -> bool {
        let Some(field) = nearest_ancestor(self.index, value, |kind| kind == NodeKind::StructLiteralField) else {
            return false;
        };
        let Some(literal) = parent_node(self.index, field) else { return false };
        let (Some(field_node), Some(literal_node)) = (
            self.index.node_at(self.program, field).and_then(|node| node.of::<beskid_analysis::syntax::StructLiteralField>()),
            self.index.node_at(self.program, literal).and_then(|node| node.of::<beskid_analysis::syntax::StructLiteralExpression>()),
        ) else {
            return false;
        };
        resolve_type_declaration(self.db, self.at(literal), &literal_node.path.node)
            .and_then(|owner| field_captures_bound(self.db, owner, &field_node.name.node.name))
            .is_some()
    }

    /// The latest consumption that reaches a use at `position` on `branches`: not in an
    /// alternative arm, and not cut off by a diverging statement between the two.
    fn prior_consumption(&self, declaration: AstNodeId, branches: &BranchPath, position: usize) -> Option<AstNodeId> {
        self.owners.get(&declaration)?.consumed.iter().rev().find_map(|consumption| {
            let alternative = consumption
                .branches
                .iter()
                .any(|(condition, arm)| branches.iter().any(|(other, other_arm)| condition == other && arm != other_arm));
            let cut_off = consumption.unreachable.iter().any(|(start, end)| (*start..*end).contains(&position));
            (!alternative && !cut_off).then_some(consumption.site)
        })
    }

    /// The diverging statements that follow `site` on its own path, innermost first: a
    /// `return` ends the search (it leaves the callable or the enclosing lambda); a `break` or
    /// `continue` cuts off the rest of its loop body and the search resumes after the loop.
    fn exits_after(&self, site: AstNodeId) -> Exits {
        let mut exits = Exits::default();
        let Some(site_start) = self.span(site).map(|span| span.start) else { return exits };
        let mut child = site;
        while let Some(parent) = parent_node(self.index, child) {
            if parent == self.key.node || self.index.kind(parent) == Some(NodeKind::LambdaExpression) {
                break;
            }
            let divergence = self
                .index
                .node_at(self.program, parent)
                .and_then(|node| node.of::<beskid_analysis::syntax::Block>())
                .and_then(|block| {
                    block
                        .statements
                        .iter()
                        .filter(|statement| statement.span.end > site_start)
                        .find_map(|statement| statement_divergence(&statement.node).map(|kind| (kind, statement.span.end)))
                });
            match divergence {
                Some((Divergence::Return, end)) => {
                    let boundary = nearest_ancestor(self.index, parent, |kind| kind == NodeKind::LambdaExpression)
                        .filter(|lambda| *lambda != self.key.node && is_ancestor(self.index, self.key.node, *lambda))
                        .unwrap_or(self.key.node);
                    let boundary_end =
                        if boundary == self.key.node { usize::MAX } else { self.span(boundary).map_or(usize::MAX, |span| span.end) };
                    exits.unreachable.push((end, boundary_end));
                    let mut current = parent;
                    while let Some(lp) = self.innermost_loop(current) {
                        exits.exited.push(lp);
                        current = lp;
                    }
                    return exits;
                }
                Some((Divergence::Loop { continues }, end)) => {
                    let Some(loop_node) = self.innermost_loop(parent) else { return exits };
                    let loop_end = self.span(loop_node).map_or(usize::MAX, |span| span.end);
                    exits.unreachable.push((end, loop_end));
                    if !continues {
                        exits.exited.push(loop_node);
                    }
                    child = loop_node;
                }
                None => child = parent,
            }
        }
        exits
    }

    /// The innermost `while`/`for` around `node` inside this callable, not across a lambda.
    fn innermost_loop(&self, node: AstNodeId) -> Option<AstNodeId> {
        let mut current = node;
        while let Some(parent) = parent_node(self.index, current) {
            match self.index.kind(parent) {
                Some(NodeKind::WhileStatement | NodeKind::ForStatement) => return Some(parent),
                Some(NodeKind::LambdaExpression) => return None,
                _ if parent == self.key.node => return None,
                _ => current = parent,
            }
        }
        None
    }

    fn exchange_call(&self, call: AstNodeId) -> bool {
        let Some(call_node) =
            self.index.node_at(self.program, call).and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
        else {
            return false;
        };
        let beskid_analysis::syntax::Expression::Path(path) = &call_node.callee.node else { return false };
        matches!(path.node.path.node.segments.as_slice(), [segment] if segment.node.name.node.name == "exchange")
            && resolve_lexical_declaration(self.program, self.index, call, "exchange").is_none()
            && direct_callee(self.db, self.at(call)).is_none()
    }

    fn callee_spelling(&self, call: AstNodeId) -> Option<String> {
        let call_node =
            self.index.node_at(self.program, call).and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())?;
        let beskid_analysis::syntax::Expression::Path(path) = &call_node.callee.node else { return None };
        Some(path.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>().join("."))
    }

    /// E1239 for `move` on anything but a tracked owning name: a Shared, Copied or Never binding,
    /// a call result or a literal. Bindings of this callable only; `move` of an outer binding
    /// inside a lambda is judged by the callable that declares it.
    fn check_move_operands(&mut self) {
        let moves = self
            .index
            .ids_of_kind(NodeKind::MoveExpression)
            .filter(|node| is_ancestor(self.index, self.key.node, *node))
            .collect::<Vec<_>>();
        for node in moves {
            let Some(moved) =
                self.index.node_at(self.program, node).and_then(|node| node.of::<beskid_analysis::syntax::MoveExpression>())
            else {
                continue;
            };
            let class = match &moved.operand.node {
                beskid_analysis::syntax::Expression::Path(path) => {
                    // A destructive read `move x.f` is judged with its root binding.
                    let segments = &path.node.path.node.segments;
                    if segments.len() > 1 || segments.first().is_some_and(|segment| segment.node.name.node.name == "this") {
                        continue;
                    }
                    let Some(declaration) = self.declaration_of(node, &path.node.path.node) else {
                        if containing_fiber_callable(self.index, node) == Some(self.key.node) {
                            let name = path.node.path.node.segments.first().map(|segment| segment.node.name.node.name.clone());
                            self.push(AffineFindingKind::MoveOfNonOwningName, node, &name.unwrap_or_default(), Some("unknown".to_owned()), None, false);
                        }
                        continue;
                    };
                    if self.owners.contains_key(&declaration)
                        || containing_fiber_callable(self.index, declaration) != Some(self.key.node)
                    {
                        continue;
                    }
                    let name = path.node.path.node.segments.first().map(|segment| segment.node.name.node.name.clone()).unwrap_or_default();
                    let class = self.declaration_class(declaration).map_or_else(|| "unknown".to_owned(), |class| class.name().to_owned());
                    self.push(AffineFindingKind::MoveOfNonOwningName, node, &name, Some(class), None, false);
                    continue;
                }
                _ => {
                    if containing_fiber_callable(self.index, node) != Some(self.key.node) {
                        continue;
                    }
                    let Some(operand) = self.child(node, DynNodeRef::from(moved.operand.as_ref())) else { continue };
                    generic_source_expression_identity(self.db, self.at(operand))
                        .ok()
                        .and_then(|identity| transfer_class(self.db, self.at(operand), &identity).ok())
                        .map_or_else(|| "unknown".to_owned(), |class| class.name().to_owned())
                }
            };
            self.push(AffineFindingKind::MoveOfNonOwningName, node, "expression", Some(class), None, false);
        }
    }

    /// The class of a binding of this callable: its declared parameter type or its local identity.
    fn declaration_class(&self, declaration: AstNodeId) -> Option<TransferClass> {
        let parent = parent_node(self.index, declaration)?;
        if let Some(parameter) =
            self.index.node_at(self.program, parent).and_then(|node| node.of::<beskid_analysis::syntax::Parameter>())
        {
            return self.type_class(parent, &parameter.ty.node);
        }
        self.local_class(declaration)
    }

    fn visit_use(&mut self, node: AstNodeId, declaration: AstNodeId, segments: &[String]) {
        if !self.owners.contains_key(&declaration) {
            return;
        }
        let context = self.context(node);
        let (explicit, move_node, destination) = match context {
            UseContext::Move(move_node) => (true, Some(move_node), self.context(move_node)),
            other => (false, None, other),
        };
        let (branches, loops) = self.path_to_callable(node, declaration);
        let crossing = self.lambda_crossing(node, declaration);
        let Some(owner) = self.owners.get(&declaration) else { return };
        let (name, fiber, owning) = (owner.name.clone(), owner.fiber, self.owning_at(owner, &branches));
        if fiber {
            self.visit_fiber_use(node, declaration, segments, destination, explicit.then_some(move_node).flatten(), branches, &loops, crossing, &name);
            return;
        }
        // Only an isolate crossing consumes (0.6.7 owner ruling), so every consumption on record
        // is a crossing and a later use of the name is a use after move.
        let position = self.span(node).map_or(0, |span| span.start);
        if let Some(prior) = self.prior_consumption(declaration, &branches, position) {
            self.push(AffineFindingKind::UseAfterMove, node, &name, None, Some(prior), false);
            return;
        }
        let site = move_node.unwrap_or(node);
        let sent = self.crossing_argument(destination);
        // A `move` of an outer binding inside a `[Captures]`-bounded lambda hands it to the task.
        let captured = explicit && crossing == LambdaCrossing::Bounded;
        // `move x.f` takes the field out at a crossing and leaves its canonical empty value
        // (E1243 without one); anywhere else `move` has no meaning (E1239).
        if explicit && segments.len() > 1 {
            if !sent {
                let place = segments.join(".");
                self.push(AffineFindingKind::MoveOfNonOwningName, site, &place, None, None, false);
                return;
            }
            if let Ok(identity) = generic_source_expression_identity(self.db, self.at(node))
                && canonical_empty(&identity).is_none()
            {
                let field = segments.last().cloned().unwrap_or_default();
                self.push(
                    AffineFindingKind::DestructiveReadWithoutEmpty,
                    site,
                    &field,
                    Some(identity_spelling(&identity)),
                    None,
                    false,
                );
            }
            return;
        }
        let place = segments.join(".");
        // `xs[i]`, `xs[i].f`, `x.f[i]` and `x.f` handed to a crossing: the place stays reachable
        // from its root, so it is not the only name for the value (E1241, copy it first).
        if let Some((top, spelling)) = self.projected_place(node, &place) {
            if self.crossing_argument(self.context(top)) {
                self.visit_crossing_place(top, declaration, spelling);
            }
            return;
        }
        if segments.len() > 1 {
            if sent {
                self.visit_crossing_place(node, declaration, place);
            }
            return;
        }
        if explicit && !sent && !captured {
            // `move` outside a crossing (0.6.7 owner ruling).
            self.push(AffineFindingKind::MoveOfNonOwningName, site, &name, None, None, false);
            return;
        }
        if !sent && !captured {
            // Ordinary local code: reads, aliases, stores, returns and calls never consume.
            return;
        }
        if !explicit {
            // A crossing operand of Moved class is written with `move`.
            self.push(AffineFindingKind::ImplicitAliasOfMovedValue, site, &name, None, None, false);
            return;
        }
        // External uniqueness: the operand is the only name for its value in this callable. A
        // borrowed parameter, an aliased local or a capture of a repeatable closure is not.
        if !owning || crossing == LambdaCrossing::Ordinary || crossing == LambdaCrossing::OneShot {
            self.push(AffineFindingKind::BorrowEscapes, site, &name, None, Some(declaration), false);
            return;
        }
        self.consume(declaration, site, &name, branches, &loops);
    }

    /// Whether a use in `context` is the value handed to another isolate by a port send: the
    /// payload of `__port_send`, or the `move` parameter of the corelib `Port.Send`.
    fn crossing_argument(&self, context: UseContext) -> bool {
        let UseContext::Argument(call, position) = context else { return false };
        if self.callee_spelling(call).as_deref() == Some("__port_send") {
            return position == 1;
        }
        let Some(declaration) = direct_callee(self.db, self.at(call)) else { return false };
        if !super::super::transfer::unit_in_core_module(self.db, declaration) {
            return false;
        }
        let port_send = super::capture_bounds::with_declaration(self.db, declaration, |node| {
            node.of::<beskid_analysis::syntax::MethodDefinition>().map(|method| method.name.node.name == "Send")
        })
        .unwrap_or(false)
            && self.owner_type_name(declaration).as_deref() == Some("Port");
        port_send
            && declared_parameters(self.db, declaration)
                .and_then(|(parameters, _)| parameters.get(position).map(|parameter| parameter.node.movable))
                .unwrap_or(false)
    }

    /// The name of the type declaration that holds the method `declaration`.
    fn owner_type_name(&self, declaration: AstNodeKey) -> Option<String> {
        let syntax = self.db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(self.db, declaration))?;
        let index = syntax.syntax_index(self.db);
        let owner = nearest_ancestor(index, declaration.node, |kind| kind == NodeKind::TypeDefinition)?;
        index
            .node_at(syntax.expanded_program(self.db), owner)?
            .of::<beskid_analysis::syntax::TypeDefinition>()
            .map(|definition| definition.name.node.name.clone())
    }

    /// A field or element read handed to a crossing (`out.Send(x.bytes)`, `out.Send(xs[i])`)
    /// would leave the Moved value reachable from its root: E1241, with the help to copy it first
    /// or take it out (`move x.f`, `exchange`). A place of any other class crosses as a value.
    fn visit_crossing_place(&mut self, node: AstNodeId, declaration: AstNodeId, place: String) {
        let moved_place = generic_source_expression_identity(self.db, self.at(node))
            .ok()
            .and_then(|identity| transfer_class(self.db, self.at(node), &identity).ok())
            == Some(TransferClass::Moved);
        if moved_place {
            self.push(AffineFindingKind::BorrowEscapes, node, &place, None, Some(declaration), false);
        }
    }

    /// Records a consumption. A diverging statement after it on its path (`return F(move x)`,
    /// an arm that consumes then returns, `break` out of a loop) keeps the consumed state from
    /// the code it cuts off and from the next iteration of the loops it leaves.
    fn consume(&mut self, declaration: AstNodeId, site: AstNodeId, name: &str, branches: BranchPath, loops: &[AstNodeId]) {
        let position = self.span(site).map_or(0, |span| span.end);
        let exits = self.exits_after(site);
        if loops.iter().any(|lp| {
            !exits.exited.contains(lp)
                && !self.revived_in_loop_body(*lp, declaration, position)
                && !self.revived_by_enclosing_assignment(*lp, declaration, site)
        }) {
            self.push(AffineFindingKind::MoveInLoopWithoutReassignment, site, name, None, None, false);
        }
        if let Some(owner) = self.owners.get_mut(&declaration) {
            owner.consumed.push(Consumption { branches, site, unreachable: exits.unreachable });
        }
    }

    /// The consumption is the value side of `x = ...` that rebinds the same name inside the loop
    /// (`x = x.Push(v)` nested in an `if` or `match` arm): the store follows the consumption in the
    /// same expression, so the next iteration sees a live binding.
    fn revived_by_enclosing_assignment(&self, loop_node: AstNodeId, declaration: AstNodeId, site: AstNodeId) -> bool {
        let mut child = site;
        while let Some(parent) = parent_node(self.index, child) {
            if parent == loop_node {
                return false;
            }
            match self.index.kind(parent) {
                Some(NodeKind::LambdaExpression) => return false,
                Some(NodeKind::AssignExpression) => {
                    let Some(assignment) = self
                        .index
                        .node_at(self.program, parent)
                        .and_then(|node| node.of::<beskid_analysis::syntax::AssignExpression>())
                    else {
                        return false;
                    };
                    let beskid_analysis::syntax::Expression::Path(target) = &assignment.target.node else {
                        return false;
                    };
                    if self.child(parent, DynNodeRef::from(assignment.target.as_ref())) == Some(child)
                        || target.node.path.node.segments.len() != 1
                    {
                        return false;
                    }
                    return self.declaration_of(parent, &target.node.path.node) == Some(declaration);
                }
                _ => child = parent,
            }
        }
        false
    }

    /// The loop body assigns the binding in a statement of its own block after `position`.
    fn revived_in_loop_body(&self, loop_node: AstNodeId, declaration: AstNodeId, position: usize) -> bool {
        let Some(syntax) = self.index.node_at(self.program, loop_node) else { return false };
        let body = if let Some(statement) = syntax.of::<beskid_analysis::syntax::WhileStatement>() {
            &statement.body
        } else if let Some(statement) = syntax.of::<beskid_analysis::syntax::ForStatement>() {
            &statement.body
        } else {
            return false;
        };
        let Some(block) = self.child(loop_node, DynNodeRef::from(body)) else { return false };
        body.node.statements.iter().any(|statement| {
            let beskid_analysis::syntax::Statement::Expression(expression) = &statement.node else { return false };
            let beskid_analysis::syntax::Expression::Assign(assignment) = &expression.node.expression.node else {
                return false;
            };
            let beskid_analysis::syntax::Expression::Path(target) = &assignment.node.target.node else { return false };
            // `x = f(move x)` revives in the same statement: the store follows the consumption.
            if target.node.path.node.segments.len() != 1 || statement.span.end < position {
                return false;
            }
            let Some(statement_node) = self.child(block, DynNodeRef::from(statement)) else { return false };
            self.declaration_of(statement_node, &target.node.path.node) == Some(declaration)
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_fiber_use(
        &mut self,
        node: AstNodeId,
        declaration: AstNodeId,
        segments: &[String],
        destination: UseContext,
        move_node: Option<AstNodeId>,
        branches: BranchPath,
        loops: &[AstNodeId],
        crossing: LambdaCrossing,
        name: &str,
    ) {
        let method = segments.get(1).map(String::as_str);
        let repeated = !loops.is_empty() || matches!(crossing, LambdaCrossing::Ordinary | LambdaCrossing::Bounded);
        let position = self.span(node).map_or(0, |span| span.start);
        let prior = self.prior_consumption(declaration, &branches, position);
        if prior.is_some() || (repeated && method != Some("Cancel")) {
            self.push(AffineFindingKind::UseAfterMove, node, name, None, prior, true);
            return;
        }
        if method == Some("Cancel") {
            return;
        }
        if let Some(move_node) = move_node {
            self.push(AffineFindingKind::MoveOfNonOwningName, move_node, name, Some("Never".to_owned()), None, true);
        }
        let site = move_node.unwrap_or(node);
        let unreachable = self.exits_after(site).unreachable;
        if let Some(owner) = self.owners.get_mut(&declaration) {
            owner.consumed.push(Consumption { branches, site, unreachable });
        }
        // A local initializer transfers the join capability to the new binding.
        if method.is_none()
            && let UseContext::LetValue(statement) = destination
            && let Some(binding) =
                self.index.node_at(self.program, statement).and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())
            && let Some(alias) = self.child(statement, DynNodeRef::from(&binding.name))
        {
            let alias_name = binding.name.node.name.clone();
            self.insert(alias, alias_name, true, true);
        }
    }

    /// `exchange(place, move value)`: the place is a field or element path of Moved class.
    fn check_exchange_places(&mut self) {
        let calls = self
            .index
            .ids_of_kind(NodeKind::CallExpression)
            .filter(|call| containing_fiber_callable(self.index, *call) == Some(self.key.node))
            .filter(|call| self.exchange_call(*call))
            .collect::<Vec<_>>();
        for call in calls {
            let Some(call_node) =
                self.index.node_at(self.program, call).and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
            else {
                continue;
            };
            let Some(place) = call_node.args.first() else { continue };
            let Some(site) = self.child(call, DynNodeRef::from(place)) else { continue };
            let spelling = match &place.node {
                beskid_analysis::syntax::Expression::Path(path) if path.node.path.node.segments.len() > 1 => Some(
                    path.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>().join("."),
                ),
                beskid_analysis::syntax::Expression::Index(_) => Some("element".to_owned()),
                _ => None,
            };
            let class = generic_source_expression_identity(self.db, self.at(site))
                .ok()
                .and_then(|identity| transfer_class(self.db, self.at(site), &identity).ok());
            match (spelling, class) {
                (Some(_), Some(TransferClass::Moved)) => {}
                (Some(spelling), Some(class)) => {
                    self.push(AffineFindingKind::MoveOfNonOwningName, site, &spelling, Some(class.name().to_owned()), None, false);
                }
                (None, class) => {
                    let class = class.map_or_else(|| "unknown".to_owned(), |class| class.name().to_owned());
                    self.push(AffineFindingKind::MoveOfNonOwningName, site, "place", Some(class), None, false);
                }
                (Some(_), None) => {}
            }
        }
    }
}

/// The evaluation form of a `move` expression: a consumption, or a destructive field read with
/// the canonical empty value left behind.
pub(in crate::semantic_contract) fn move_expression_form_for_node(
    db: &dyn Db,
    program: &Program,
    index: &SyntaxIndex,
    key: AstNodeKey,
) -> Option<Result<MoveExpressionForm, SemanticError>> {
    let moved = index.node_at(program, key.node)?.of::<beskid_analysis::syntax::MoveExpression>()?;
    let operand = index.direct_child_id(program, key.node, DynNodeRef::from(moved.operand.as_ref()))?;
    let operand = normalized_expression_node(index, operand);
    let operand_key = AstNodeKey { node: operand, ..key };
    let beskid_analysis::syntax::Expression::Path(path) = &moved.operand.node else {
        return Some(Ok(MoveExpressionForm::Consume { operand: operand_key }));
    };
    if path.node.path.node.segments.len() < 2 {
        return Some(Ok(MoveExpressionForm::Consume { operand: operand_key }));
    }
    let field = path.node.path.node.segments.last()?.node.name.node.name.clone();
    Some(generic_source_expression_identity(db, operand_key).and_then(|identity| {
        canonical_empty(&identity)
            .map(|empty| MoveExpressionForm::DestructiveRead { place: operand_key, field, empty })
            .ok_or_else(|| SemanticError::unavailable_at("move_expression_form", key))
    }))
}
