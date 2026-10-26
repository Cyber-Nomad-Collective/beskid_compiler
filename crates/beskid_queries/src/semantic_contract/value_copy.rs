//! Copy on assignment (v0.6.7, `language-meta--memory-model--memory-and-references` and
//! `core-library--foundation-and-primitives--core-collections`).
//!
//! A type marked `[CopyOnAssign]` is a value (Corelib marks only `List<T>`; the attribute is not
//! restricted to Corelib, so compiler tests use their own marked types): wherever a value of that
//! type gets a new name, the compiler calls the type's `This Copy()` method, so a write through
//! one name is never visible through another. `Copy` is copy-on-write: it returns a new value
//! that shares the storage and marks the storage shared, and the first mutation through either
//! name copies the storage. Read-only use costs one small allocation per new name and no
//! element copy.
//!
//! A new name is made by a read of a place (a local, a parameter, a field, an array element, or
//! the payload `r?` takes out of a `Result`/`Option` held in a place) in a copy position: a `let`
//! value, an assignment value, an argument of a parameter that is not `mut`, a struct field,
//! array element or enum payload of a construction, and a returned place that the callable does
//! not own (a field, an element, `this`, a parameter that is not `move`, a pattern binding, or a
//! local of an enclosing callable read in a lambda). The element an array `for` loop binds, every
//! `match` pattern binding and every closure capture are new names too. A method receiver, a
//! `mut` parameter argument, an operator operand and a match scrutinee read the place without a
//! copy, and so does an owned temporary: `?` on a call result, and a `let` local (or `move`
//! parameter) returned once as the field or payload of the returned value.
//!
//! Three hooks serve the positions (only `Copy` is required):
//! - `This Copy()` for every new name that may outlive the read;
//! - `This Borrow()` for an argument of a plain parameter of a direct call: the callee only
//!   borrows the value for the call, so the hook returns a lent view that copies before its
//!   first write and leaves the sharing state alone (a sole owner keeps writing in place after
//!   the call); a parameter that is not `move` is copied when the callee returns or keeps it;
//! - `This Detached()` for the operand of `move` at an isolate crossing: the value leaves with
//!   storage and sharing state of its own, so no sharing state is reachable from two isolates.
//!
//! Values of a type parameter are decided per specialization: inside `Get<T>` instantiated with
//! `List<i64>`, `return values[index]` copies.

use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

use super::contracts::concrete_declaration;
use super::*;

type Program = beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>;

/// The position that makes a place read store a copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ValueCopyPosition {
    /// The value of a `let`.
    Binding,
    /// The value of an assignment to a local, field or element.
    Assignment,
    /// An argument the callee may keep: of a `move` parameter, of a `spawn` target, or of a
    /// call whose parameters are not known (a function value or a dynamic dispatch).
    Argument,
    /// An argument of a plain (not `mut`, not `move`) parameter of a direct call: the callee
    /// borrows the value for the call (`Borrow` hook).
    LentArgument,
    /// A struct field, array element or enum payload of a construction.
    Construction,
    /// A returned place that the callable does not own.
    Return,
    /// The operand of `move` at an isolate crossing (`Detached` hook).
    Crossing,
}

/// The hook a copy position calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hook {
    Copy,
    Borrow,
    Detached,
}

impl Hook {
    fn of(position: ValueCopyPosition) -> Self {
        match position {
            ValueCopyPosition::LentArgument => Self::Borrow,
            ValueCopyPosition::Crossing => Self::Detached,
            _ => Self::Copy,
        }
    }

    fn method(self) -> &'static str {
        match self {
            Self::Copy => "Copy",
            Self::Borrow => "Borrow",
            Self::Detached => "Detached",
        }
    }
}

/// The copy position of the place expression `key`, or `None` when the read stores no copy.
/// Only the place node itself (a path, member or index expression) has a position; its grouping
/// and `Expression` wrappers have none, so a read is copied once.
pub fn value_copy_position(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ValueCopyPosition> {
    with_registered_syntax(db, key, value_copy_position_tracked)
}

#[salsa::tracked(persist)]
fn value_copy_position_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ValueCopyPosition> {
    let position = with_node(db, syntax, key, |program, index, _node| copy_position(db, key, program, index))?;
    Ok(position)
}

/// The exact `Copy` instance a place read at `key` calls in `enclosing` (the specialization of
/// the item being lowered, or `None` for a concrete item). `Ok(None)` when the read is in no copy
/// position or its type is not `[CopyOnAssign]`. A `[CopyOnAssign]` type without a valid
/// `This Copy()` method is an error, so the read never silently shares storage.
pub fn value_copy_specialization(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(position) = value_copy_position(db, key)? else {
        return Ok(None);
    };
    // A read whose type is not proven is not a `[CopyOnAssign]` value: every nominal place type
    // is proven by the same authority that types its method calls and field reads, which report
    // their own failures.
    let Ok(identity) = place_identity(db, key, enclosing) else {
        return Ok(None);
    };
    hook_instance(db, key, &identity, Hook::of(position))
}

/// The `Copy` instance the element read of an array `for` loop calls before it binds the loop
/// variable, so the variable is a copy of the element like any other binding.
pub fn for_element_copy_specialization(
    db: &dyn Db,
    statement: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(syntax) = db.syntax_unit(statement.unit).filter(|syntax| syntax.accepts_key(db, statement)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(for_statement) =
        index.node_at(program, statement.node).and_then(|node| node.of::<beskid_analysis::syntax::ForStatement>())
    else {
        return Ok(None);
    };
    let Some(iterable) = index.direct_child_id(program, statement.node, DynNodeRef::from(&for_statement.iterable))
    else {
        return Ok(None);
    };
    let Ok(GenericSourceTypeIdentity::Array(element)) =
        specialized_source_expression_identity(db, AstNodeKey { node: iterable, ..statement }, enclosing)
    else {
        return Ok(None);
    };
    hook_instance(db, statement, &element, Hook::Copy)
}

/// The `Copy` instance a closure calls when it captures the local `declaration` (its declaring
/// identifier), so the closure holds a copy like any other binding.
pub fn capture_copy_specialization(
    db: &dyn Db,
    declaration: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let identity = match enclosing {
        Some(enclosing) => match super::contracts::specialized_local_declaration_identity(db, declaration, enclosing) {
            Some(Ok(identity)) => Some(identity),
            _ => None,
        },
        None => None,
    };
    let identity = match identity {
        Some(identity) => identity,
        None => {
            let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))
            else {
                return Ok(None);
            };
            let Ok(identity) = generic_source_local_identity(
                db,
                syntax.expanded_program(db),
                syntax.syntax_index(db),
                declaration,
                declaration.node,
            ) else {
                return Ok(None);
            };
            identity
        }
    };
    hook_instance(db, declaration, &identity, Hook::Copy)
}

/// The `Copy` instance a `match` arm calls when it binds the pattern variable `declaration` (its
/// declaring identifier), so a write through the binding never reaches the matched value.
/// `Ok(None)` for an identifier that is not a pattern binding.
pub fn pattern_binding_copy_specialization(
    db: &dyn Db,
    declaration: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    if index.kind(declaration.node) != Some(NodeKind::Identifier)
        || parent_node(index, declaration.node).and_then(|parent| index.kind(parent)) != Some(NodeKind::Pattern)
    {
        return Ok(None);
    }
    capture_copy_specialization(db, declaration, enclosing)
}

/// The exact `hook` instance of a value of `identity`, or `None` when its type is not
/// `[CopyOnAssign]` (or declares no `Detached` for a crossing). A type without `Borrow` lends a
/// `Copy`.
fn hook_instance(
    db: &dyn Db,
    site: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
    hook: Hook,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let GenericSourceTypeIdentity::Nominal { arguments, .. } = identity else {
        return Ok(None);
    };
    let Some(declaration) = concrete_declaration(db, site, identity) else {
        return Ok(None);
    };
    let found = match hook {
        Hook::Copy => copy_hook(db, declaration)?,
        Hook::Borrow => match named_hook(db, declaration, Hook::Borrow)? {
            Some(found) => Some(found),
            None => copy_hook(db, declaration)?,
        },
        Hook::Detached => named_hook(db, declaration, Hook::Detached)?,
    };
    let Some((hook, generics)) = found else {
        return Ok(None);
    };
    if generics.len() != arguments.len() {
        return Err(SemanticError::unavailable_at("value_copy_specialization", site));
    }
    let declaration_identity = stable_declaration_identity(db, hook)
        .ok_or_else(|| SemanticError::unavailable_at("value_copy_specialization", site))?;
    let substitutions = generics
        .iter()
        .zip(arguments.iter())
        .map(|(name, argument)| GenericSubstitution::from_source(name.clone(), argument.abi_type(), argument.clone()))
        .collect::<Vec<_>>();
    Ok(Some(GenericSpecializationInstance {
        declaration: hook,
        declaration_identity,
        signature: ItemSignature { parameters: Arc::from([SemanticTypeId::POINTER]), result: SemanticTypeId::POINTER },
        substitutions: substitutions.into(),
        contract_witnesses: Arc::from([]),
    }))
}

/// The source identity of the place `key` in `enclosing`. An element read takes the element of
/// its array target's identity in the same environment, so `values[index]` in `Get<T>` is
/// `List<i64>` inside `Get<List<i64>>`.
fn place_identity(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    if enclosing.is_some()
        && let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key))
    {
        let index = syntax.syntax_index(db);
        let program = syntax.expanded_program(db);
        if let Some(indexed) =
            index.node_at(program, key.node).and_then(|node| node.of::<beskid_analysis::syntax::IndexExpression>())
            && let Some(target) = index.direct_child_id(program, key.node, DynNodeRef::from(indexed.target.as_ref()))
        {
            return match specialized_source_expression_identity(db, AstNodeKey { node: target, ..key }, enclosing)? {
                GenericSourceTypeIdentity::Array(element) => Ok(*element),
                _ => Err(SemanticError::unavailable_at("value_copy_specialization", key)),
            };
        }
    }
    specialized_source_expression_identity(db, key, enclosing)
}

/// Whether a value of `identity` is `[CopyOnAssign]`: every new name of it is a copy, so keeping
/// it never shares the value itself. A marked type without a valid hook counts as marked (its
/// copies fail closed where they are lowered).
pub(in crate::semantic_contract) fn is_copy_on_assign(
    db: &dyn Db,
    site: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
) -> bool {
    if !matches!(identity, GenericSourceTypeIdentity::Nominal { .. }) {
        return false;
    }
    let Some(declaration) = concrete_declaration(db, site, identity) else {
        return false;
    };
    !matches!(copy_hook(db, declaration), Ok(None))
}

/// The type shape a crossing hands over, as far as nested `[CopyOnAssign]` values matter.
#[derive(Clone)]
enum Shape {
    Declaration(AstNodeKey, Vec<Shape>),
    Array(Box<Shape>),
    Leaf,
}

/// The name of a `[CopyOnAssign]` type reachable from a value of `identity` below its top level
/// (a field, an element, an enum payload, or the element type of a top-level `List`), or `None`.
/// A crossing detaches only the top-level list (`Detached`); a nested list may share its array
/// with a list the sending isolate keeps, and its holder is reachable from the sender's other
/// names too, so the send cannot detach it in place.
pub(in crate::semantic_contract) fn nested_copy_on_assign(
    db: &dyn Db,
    site: AstNodeKey,
    identity: &GenericSourceTypeIdentity,
) -> Option<String> {
    let shape = identity_shape(db, site, identity);
    nested_in(db, &shape, true, &mut Vec::new())
}

fn identity_shape(db: &dyn Db, site: AstNodeKey, identity: &GenericSourceTypeIdentity) -> Shape {
    match identity {
        GenericSourceTypeIdentity::Array(element) => Shape::Array(Box::new(identity_shape(db, site, element))),
        GenericSourceTypeIdentity::Nominal { arguments, .. } => {
            match super::contracts::concrete_type_or_enum_declaration(db, site, identity) {
                Some(declaration) => Shape::Declaration(
                    declaration,
                    arguments.iter().map(|argument| identity_shape(db, site, argument)).collect(),
                ),
                None => Shape::Leaf,
            }
        }
        _ => Shape::Leaf,
    }
}

fn syntax_shape(
    db: &dyn Db,
    context: AstNodeKey,
    ty: &beskid_analysis::syntax::Type,
    environment: &[(String, Shape)],
) -> Shape {
    match ty {
        beskid_analysis::syntax::Type::Array(element) => {
            Shape::Array(Box::new(syntax_shape(db, context, &element.node, environment)))
        }
        beskid_analysis::syntax::Type::Complex(path) => {
            if let [segment] = path.node.segments.as_slice()
                && segment.node.type_args.is_empty()
                && let Some((_, shape)) = environment.iter().find(|(name, _)| *name == segment.node.name.node.name)
            {
                return shape.clone();
            }
            match resolve_type_declaration(db, context, &path.node) {
                Some(declaration) => Shape::Declaration(
                    declaration,
                    path.node
                        .segments
                        .iter()
                        .flat_map(|segment| segment.node.type_args.iter())
                        .map(|argument| syntax_shape(db, context, &argument.node, environment))
                        .collect(),
                ),
                None => Shape::Leaf,
            }
        }
        _ => Shape::Leaf,
    }
}

fn nested_in(db: &dyn Db, shape: &Shape, top: bool, visiting: &mut Vec<AstNodeKey>) -> Option<String> {
    match shape {
        Shape::Leaf => None,
        Shape::Array(element) => nested_in(db, element, false, visiting),
        Shape::Declaration(declaration, arguments) => {
            let marked = !matches!(copy_hook(db, *declaration), Ok(None));
            let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, *declaration))
            else {
                return None;
            };
            let program = syntax.expanded_program(db);
            let node = syntax.syntax_index(db).node_at(program, declaration.node)?;
            let (name, generics, fields) = if let Some(definition) = node.of::<beskid_analysis::syntax::TypeDefinition>() {
                (
                    definition.name.node.name.clone(),
                    definition.generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>(),
                    definition.fields.iter().map(|field| field.node.ty.node.clone()).collect::<Vec<_>>(),
                )
            } else if let Some(definition) = node.of::<beskid_analysis::syntax::EnumDefinition>() {
                (
                    definition.name.node.name.clone(),
                    definition.generics.iter().map(|generic| generic.node.name.clone()).collect::<Vec<_>>(),
                    definition
                        .variants
                        .iter()
                        .flat_map(|variant| variant.node.fields.iter().map(|field| field.node.ty.node.clone()))
                        .collect::<Vec<_>>(),
                )
            } else {
                return None;
            };
            if marked {
                if !top {
                    return Some(name);
                }
                // The top-level list is detached; its elements are not.
                return arguments.iter().find_map(|argument| nested_in(db, argument, false, visiting));
            }
            if visiting.contains(declaration) || visiting.len() > 32 {
                return None;
            }
            visiting.push(*declaration);
            let environment = generics.into_iter().zip(arguments.iter().cloned()).collect::<Vec<_>>();
            let found = fields
                .iter()
                .find_map(|field| nested_in(db, &syntax_shape(db, *declaration, field, &environment), false, visiting));
            visiting.pop();
            found
        }
    }
}

/// The `Copy` method and the owner's generic parameter names of a `[CopyOnAssign]` type, or
/// `None` for any other type.
fn copy_hook(db: &dyn Db, declaration: AstNodeKey) -> Result<Option<(AstNodeKey, Vec<Arc<str>>)>, SemanticError> {
    named_hook(db, declaration, Hook::Copy)
}

/// The `hook` method of a `[CopyOnAssign]` type with the owner's generic parameter names, or
/// `None` for any other type. A missing or ill-shaped `Copy` is an error; a missing `Borrow` or
/// `Detached` is `None` (they are optional).
fn named_hook(
    db: &dyn Db,
    declaration: AstNodeKey,
    hook: Hook,
) -> Result<Option<(AstNodeKey, Vec<Arc<str>>)>, SemanticError> {
    let Some(syntax) = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration)) else {
        return Ok(None);
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let Some(definition) =
        index.node_at(program, declaration.node).and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
    else {
        return Ok(None);
    };
    if !has_recognized_attribute(db, declaration, &definition.attributes, "CopyOnAssign") {
        return Ok(None);
    }
    let type_name = definition.name.node.name.as_str();
    let wanted = hook.method();
    let found = definition.methods.iter().find(|method| {
        let method = &method.node;
        method.name.node.name == wanted
            && method.parameters.is_empty()
            && method.generics.is_empty()
            && method.receiver.as_ref().is_none_or(|receiver| !receiver.node.mutable && !receiver.node.movable)
            && method.return_type.as_ref().is_some_and(|result| returns_owner(&result.node, type_name))
    });
    let Some(found) = found else {
        if hook != Hook::Copy {
            return Ok(None);
        }
        return Err(SemanticError::new(format!(
            "`[CopyOnAssign]` type `{type_name}` needs a method `{type_name} Copy()` with a borrowed receiver and no \
             parameters"
        )));
    };
    let hook = index
        .direct_child_id(program, declaration.node, DynNodeRef::from(found))
        .map(|node| AstNodeKey { node, ..declaration })
        .ok_or_else(|| SemanticError::unavailable_at("value_copy_specialization", declaration))?;
    let generics = definition.generics.iter().map(|generic| Arc::<str>::from(generic.node.name.as_str())).collect();
    Ok(Some((hook, generics)))
}

/// Whether a `Copy` result type names its owner (`This`, `List<T>`).
fn returns_owner(result: &beskid_analysis::syntax::Type, type_name: &str) -> bool {
    match result {
        beskid_analysis::syntax::Type::Complex(path) => {
            path.node.segments.last().is_some_and(|segment| segment.node.name.node.name == type_name)
        }
        beskid_analysis::syntax::Type::This => true,
        _ => false,
    }
}

fn copy_position(db: &dyn Db, key: AstNodeKey, program: &Program, index: &SyntaxIndex) -> Option<ValueCopyPosition> {
    let place = key.node;
    match index.kind(place)? {
        NodeKind::PathExpression | NodeKind::MemberExpression | NodeKind::IndexExpression => {}
        // `r?` names the payload of a result held in a place; `Make()?` takes it out of an owned
        // temporary, which no other name holds.
        NodeKind::TryExpression if try_reads_a_place(index, place) => {}
        _ => return None,
    }
    let child_of = |parent, target: DynNodeRef<'_>| index.direct_child_id(program, parent, target);
    let mut child = place;
    loop {
        let parent = parent_node(index, child)?;
        let syntax = index.node_at(program, parent)?;
        match index.kind(parent)? {
            NodeKind::Expression | NodeKind::GroupedExpression => {}
            NodeKind::LetStatement => return Some(ValueCopyPosition::Binding),
            NodeKind::AssignExpression => {
                let assignment = syntax.of::<beskid_analysis::syntax::AssignExpression>()?;
                if child_of(parent, DynNodeRef::from(assignment.target.as_ref())) == Some(child) {
                    return None;
                }
                return Some(ValueCopyPosition::Assignment);
            }
            NodeKind::CallExpression => {
                let call = syntax.of::<beskid_analysis::syntax::CallExpression>()?;
                let position = call
                    .args
                    .iter()
                    .position(|argument| child_of(parent, DynNodeRef::from(argument)) == Some(child))?;
                let call_key = AstNodeKey { node: parent, ..key };
                // `exchange(place, move value)` writes its first argument in place.
                if position == 0 && exchange_call_for_node(program, index, call_key).is_some() {
                    return None;
                }
                let parameter = direct_callee(db, call_key)
                    .and_then(|declaration| declared_parameters(db, declaration))
                    .and_then(|(parameters, _)| parameters.get(position).map(|parameter| parameter.node.clone()));
                // A compiler-lowered collection operation (`Array.Append` and its kin) stores
                // its element argument without running the callee's body: a copy.
                let lowered = matches!(super::collection_operation(db, call_key), Ok(Some(_)));
                return match parameter {
                    // A `mut` parameter shares the caller's value.
                    Some(parameter) if parameter.mutable => None,
                    // A plain parameter borrows the value for the call, unless the call runs
                    // after the caller continues (a `spawn` target).
                    Some(parameter)
                        if !parameter.movable && !parameter.bulk && !lowered && !spawn_target_call(index, parent) =>
                    {
                        Some(ValueCopyPosition::LentArgument)
                    }
                    _ => Some(ValueCopyPosition::Argument),
                };
            }
            NodeKind::MoveExpression => {
                return crossing_operand(db, program, index, AstNodeKey { node: parent, ..key })
                    .then_some(ValueCopyPosition::Crossing);
            }
            NodeKind::StructLiteralField | NodeKind::ArrayLiteralExpression | NodeKind::EnumConstructorExpression => {
                if returned_once_as_part(program, index, place, parent) {
                    return None;
                }
                return Some(ValueCopyPosition::Construction);
            }
            NodeKind::ReturnStatement | NodeKind::LambdaExpression => {
                return return_copies(program, index, place).then_some(ValueCopyPosition::Return);
            }
            // A match arm's value flows to wherever the match expression's value goes.
            NodeKind::MatchArm => {
                let arm = syntax.of::<beskid_analysis::syntax::MatchArm>()?;
                if child_of(parent, DynNodeRef::from(&arm.value)) != Some(child) {
                    return None;
                }
            }
            NodeKind::MatchExpression => {
                if index.kind(child)? != NodeKind::MatchArm {
                    return None;
                }
            }
            _ => return None,
        }
        child = parent;
    }
}

/// The operand of the `?` at `node` is a place read (or a `?` of one), not an owned temporary.
fn try_reads_a_place(index: &SyntaxIndex, node: beskid_analysis::syntax::AstNodeId) -> bool {
    let Some(operand) = index.children(node).and_then(|children| children.first()).copied() else {
        return false;
    };
    let operand = normalized_expression_node(index, operand);
    match index.kind(operand) {
        Some(NodeKind::PathExpression | NodeKind::MemberExpression | NodeKind::IndexExpression) => true,
        Some(NodeKind::TryExpression) => try_reads_a_place(index, operand),
        _ => false,
    }
}

/// The call at `call` is the target of a `spawn` expression: it runs after the caller goes on.
fn spawn_target_call(index: &SyntaxIndex, call: beskid_analysis::syntax::AstNodeId) -> bool {
    let mut child = call;
    while let Some(parent) = parent_node(index, child) {
        match index.kind(parent) {
            Some(NodeKind::Expression | NodeKind::GroupedExpression) => child = parent,
            Some(NodeKind::SpawnExpression) => return true,
            _ => return false,
        }
    }
    false
}

/// The `move` expression at `key` is the value handed over by a port send.
fn crossing_operand(db: &dyn Db, program: &Program, index: &SyntaxIndex, key: AstNodeKey) -> bool {
    let mut child = key.node;
    while let Some(parent) = parent_node(index, child) {
        match index.kind(parent) {
            Some(NodeKind::Expression | NodeKind::GroupedExpression) => child = parent,
            Some(NodeKind::CallExpression) => {
                let Some(call) =
                    index.node_at(program, parent).and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
                else {
                    return false;
                };
                let Some(position) = call
                    .args
                    .iter()
                    .position(|argument| index.direct_child_id(program, parent, DynNodeRef::from(argument)) == Some(child))
                else {
                    return false;
                };
                return super::closures_spawn::port_send_argument(db, program, index, AstNodeKey { node: parent, ..key }, position);
            }
            _ => return false,
        }
    }
    false
}

/// A `let` local or a `move` parameter of this callable read once in a construction whose value
/// is returned (`return Result::Ok(items)`, `return Pair { left: items, .. }`): the local dies at
/// the return, so the returned value takes it over without a copy.
fn returned_once_as_part(
    program: &Program,
    index: &SyntaxIndex,
    place: beskid_analysis::syntax::AstNodeId,
    construction: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let Some(path) = index.node_at(program, place).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
    else {
        return false;
    };
    let [segment] = path.path.node.segments.as_slice() else {
        return false;
    };
    let name = segment.node.name.node.name.as_str();
    let mut current = construction;
    let statement = loop {
        let Some(parent) = parent_node(index, current) else { return false };
        match index.kind(parent) {
            Some(
                NodeKind::Expression
                | NodeKind::GroupedExpression
                | NodeKind::StructLiteralField
                | NodeKind::StructLiteralExpression
                | NodeKind::ArrayLiteralExpression
                | NodeKind::EnumConstructorExpression,
            ) => current = parent,
            Some(NodeKind::ReturnStatement) => break parent,
            _ => return false,
        }
    };
    if !owned_by_callable(program, index, place, name) {
        return false;
    }
    let Some(span) = index.node_at(program, statement).and_then(|node| node.span()) else { return false };
    index
        .ids_of_kind(NodeKind::PathExpression)
        .filter(|node| {
            index.node_at(program, *node).and_then(|node| node.span()).is_some_and(|inner| {
                inner.start >= span.start && inner.end <= span.end
            })
        })
        .filter(|node| {
            index
                .node_at(program, *node)
                .and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
                .and_then(|path| path.path.node.segments.first().map(|segment| segment.node.name.node.name == name))
                .unwrap_or(false)
        })
        .count()
        == 1
}

/// The single-segment name `name` read at `place` is a `let` local or a `move` parameter of the
/// callable that holds the read (no lambda between the read and the declaration).
fn owned_by_callable(
    program: &Program,
    index: &SyntaxIndex,
    place: beskid_analysis::syntax::AstNodeId,
    name: &str,
) -> bool {
    let Some(declaration) = resolve_lexical_declaration(program, index, place, name) else {
        return false;
    };
    if crosses_lambda(index, place, declaration) {
        return false;
    }
    match parent_node(index, declaration).and_then(|declaring| Some((declaring, index.kind(declaring)?))) {
        Some((_, NodeKind::LetStatement)) => true,
        Some((declaring, NodeKind::Parameter)) => index
            .node_at(program, declaring)
            .and_then(|node| node.of::<beskid_analysis::syntax::Parameter>())
            .is_some_and(|parameter| parameter.movable),
        _ => false,
    }
}

/// A lambda encloses `place` but not `declaration`: the read is of a captured binding.
fn crosses_lambda(
    index: &SyntaxIndex,
    place: beskid_analysis::syntax::AstNodeId,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let mut current = place;
    while let Some(parent) = parent_node(index, current) {
        if index.kind(parent) == Some(NodeKind::LambdaExpression) {
            return !is_ancestor(index, parent, declaration);
        }
        current = parent;
    }
    false
}

/// A returned place copies unless the callable owns it: a `let` local or a `move` parameter of
/// the same callable. A plain parameter is lent by the caller (`Borrow`), so returning it copies.
fn return_copies(program: &Program, index: &SyntaxIndex, place: beskid_analysis::syntax::AstNodeId) -> bool {
    let Some(path) =
        index.node_at(program, place).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
    else {
        return true;
    };
    let [segment] = path.path.node.segments.as_slice() else {
        return true;
    };
    !owned_by_callable(program, index, place, &segment.node.name.node.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BeskidDatabase, ProjectSession, build_typed_program};
    use beskid_analysis::projects::{
        AssemblyDiscovery, EffectiveCompilationRoots, ModuleIndex, ProgramAssembly, RootEntry, SourceUnit,
    };
    use beskid_analysis::services::parse_program_with_source_name;

    fn assembly(source: &str, generation: u64) -> (BeskidDatabase, AstNodeKey) {
        let generation = SyntaxGenerationId(generation);
        let mut db = BeskidDatabase::default();
        let directory = tempfile::tempdir().expect("project").keep();
        let source_path = directory.join("Main.bd");
        std::fs::write(&source_path, source).expect("source");
        let program =
            parse_program_with_source_name(source_path.to_str().expect("path"), source).expect("parse source");
        let entry = SourceUnitId::new(&db, source_path.clone());
        let project = ProjectSession::new(&db, directory.clone(), source_path.clone(), "App".into(), "lock".into());
        let assembly = Arc::new(ProgramAssembly::new(
            EffectiveCompilationRoots {
                host: RootEntry { dependency_name: None, source_root: directory },
                dependencies: Vec::new(),
            },
            Arc::new(vec![SourceUnit {
                logical_name: "Main".into(),
                origin_path: source_path.clone(),
                path: source_path,
                source: source.into(),
                program,
            }]),
            0,
            AssemblyDiscovery::ImportClosure,
            Arc::new(ModuleIndex::empty()),
            false,
            generation,
        ));
        build_typed_program(&mut db, project, generation, assembly).expect("typed syntax program");
        (db, AstNodeKey { unit: entry, generation, node: beskid_analysis::syntax::AstNodeId(0) })
    }

    /// The place expression whose source text is exactly `text` (its first occurrence).
    fn place(db: &dyn Db, source: &str, key: AstNodeKey, text: &str) -> Option<AstNodeKey> {
        let kind = node_kind(db, key).expect("node kind");
        if matches!(
            kind,
            Some(
                IndexedNodeKind::PathExpression | IndexedNodeKind::MemberExpression | IndexedNodeKind::IndexExpression
            )
        ) && (node_span(db, key)
            .ok()
            .flatten()
            .is_some_and(|span| source.get(span.start..span.end).map(str::trim) == Some(text))
            || path_spelling(db, key).as_deref() == Some(text))
        {
            return Some(key);
        }
        child_nodes(db, key).expect("child nodes")?.iter().find_map(|child| place(db, source, *child, text))
    }

    /// `a.b` for a path expression with segments `a` and `b`.
    fn path_spelling(db: &dyn Db, key: AstNodeKey) -> Option<String> {
        let syntax = db.syntax_unit(key.unit)?;
        let path = syntax
            .syntax_index(db)
            .node_at(syntax.expanded_program(db), key.node)?
            .of::<beskid_analysis::syntax::PathExpression>()?;
        Some(
            path.path
                .node
                .segments
                .iter()
                .map(|segment| segment.node.name.node.name.as_str())
                .collect::<Vec<_>>()
                .join("."),
        )
    }

    const BAG: &str = "[CopyOnAssign] pub type Bag<T> { T[] items; i64 count; \
        pub Bag<T> Copy() { return Bag<T> { items: this.items, count: this.count }; } \
        pub unit Put(mut this, T value) { this.count = this.count + 1_i64; return; } } \
        pub type Holder { Bag<i64> bag; } \
        unit Keep(Bag<i64> kept) { return; } \
        unit Share(mut Bag<i64> shared) { return; } \
        Bag<i64> Own(Bag<i64> owned) { return owned; } \
        Bag<i64> Lend(mut Bag<i64> lent) { return lent; } \
        Bag<i64> Field(Holder holder) { return holder.bag; } ";

    const MAIN: &str = "i64 Main(Bag<i64> bound, Bag<i64> assigned, Bag<i64> passed, Bag<i64> sharedArg, \
        Bag<i64> receiver, Bag<i64> built, Bag<i64> counted, Bag<i64> moved) { \
        Bag<i64> b = bound; \
        mut Bag<i64> c = b; \
        c = assigned; \
        Keep(passed); \
        Share(sharedArg); \
        receiver.Put(1_i64); \
        Holder made = Holder { bag: built }; \
        Keep(move moved); \
        i64 n = counted.count; \
        return n; }";

    #[test]
    fn copy_positions_follow_the_destination_of_a_place_read() {
        let source = format!("{BAG}{MAIN}");
        let (db, root) = assembly(&source, 6700);
        let position = |text: &str| {
            let key = place(&db, &source, root, text).unwrap_or_else(|| panic!("place `{text}`"));
            value_copy_position(&db, key).expect("copy position")
        };
        assert_eq!(position("bound"), Some(ValueCopyPosition::Binding));
        assert_eq!(position("assigned"), Some(ValueCopyPosition::Assignment));
        assert_eq!(position("passed"), Some(ValueCopyPosition::LentArgument), "a plain parameter borrows");
        assert_eq!(position("built"), Some(ValueCopyPosition::Construction));
        assert_eq!(position("sharedArg"), None, "a `mut` parameter shares the caller's value");
        assert_eq!(position("moved"), None, "a `move` operand hands the value over");
        assert_eq!(position("owned"), Some(ValueCopyPosition::Return), "a plain parameter is lent by the caller");
        assert_eq!(position("lent"), Some(ValueCopyPosition::Return), "a returned `mut` parameter is shared");
        assert_eq!(position("holder.bag"), Some(ValueCopyPosition::Return), "a returned field is shared");
        assert_eq!(position("counted.count"), Some(ValueCopyPosition::Binding));
        let receiver = place(&db, &source, root, "receiver.Put").or_else(|| place(&db, &source, root, "receiver"));
        assert_eq!(value_copy_position(&db, receiver.expect("receiver")).expect("copy position"), None);
    }

    #[test]
    fn a_copy_position_calls_the_copy_hook_only_for_a_copy_on_assign_type() {
        let source = format!("{BAG}{MAIN}");
        let (db, root) = assembly(&source, 6701);
        for text in ["bound", "assigned", "passed", "built", "holder.bag"] {
            let key = place(&db, &source, root, text).expect("place");
            let instance = value_copy_specialization(&db, key, None)
                .expect("copy specialization")
                .unwrap_or_else(|| panic!("`{text}` calls the hook"));
            assert_eq!(item_name(&db, instance.declaration).expect("hook name").as_deref(), Some("Copy"), "{text}");
            let substitutions = instance
                .substitutions
                .iter()
                .map(|binding| format!("{}={}", binding.parameter, binding.argument.display_name()))
                .collect::<Vec<_>>();
            assert_eq!(substitutions, ["T=i64"], "{text}");
            assert_eq!(instance.signature.parameters.as_ref(), [SemanticTypeId::POINTER], "{text}");
        }
        let count = place(&db, &source, root, "counted.count").expect("member read");
        assert_eq!(value_copy_specialization(&db, count, None).expect("scalar read"), None, "an i64 is not copied");
    }

    #[test]
    fn a_copy_on_assign_type_without_a_copy_method_is_rejected() {
        let source = "[CopyOnAssign] pub type Raw { i64 x; } i64 Main(Raw r) { Raw s = r; return s.x; }";
        let (db, root) = assembly(source, 6702);
        let key = place(&db, source, root, "r").expect("place");
        let error = value_copy_specialization(&db, key, None).expect_err("missing hook");
        assert!(!error.is_unavailable(), "{error}");
        assert!(error.to_string().contains("Raw Copy()"), "{error}");
    }

    #[test]
    fn a_type_without_the_attribute_is_shared() {
        let source = "pub type Plain { i64 x; pub Plain Copy() { return Plain { x: this.x }; } } \
            i64 Main(Plain p) { Plain q = p; return q.x; }";
        let (db, root) = assembly(source, 6703);
        let key = place(&db, source, root, "p").expect("place");
        assert_eq!(value_copy_position(&db, key).expect("position"), Some(ValueCopyPosition::Binding));
        assert_eq!(value_copy_specialization(&db, key, None).expect("specialization"), None);
    }

    fn first_of_kind(db: &dyn Db, key: AstNodeKey, kind: IndexedNodeKind) -> Option<AstNodeKey> {
        if node_kind(db, key).expect("node kind") == Some(kind) {
            return Some(key);
        }
        child_nodes(db, key).expect("child nodes")?.iter().find_map(|child| first_of_kind(db, *child, kind))
    }

    #[test]
    fn a_for_element_a_capture_and_a_try_payload_are_new_names() {
        let source = format!(
            "{BAG}i64 Loop(Bag<i64>[] bags) {{ for b in bags {{ b.Put(1_i64); }} return 0_i64; }} \
             i64 Capture(Bag<i64> captured) {{ let f = () => captured.count; return f(); }} \
             i64 Main() {{ return 0_i64; }}"
        );
        let (db, root) = assembly(&source, 6704);
        let for_statement = first_of_kind(&db, root, IndexedNodeKind::ForStatement).expect("for statement");
        let element = for_element_copy_specialization(&db, for_statement, None)
            .expect("for element copy")
            .expect("the loop variable is a copy of the element");
        assert_eq!(item_name(&db, element.declaration).expect("hook name").as_deref(), Some("Copy"));
        let lambda = first_of_kind(&db, root, IndexedNodeKind::LambdaExpression).expect("lambda");
        let environment = closure_environment(&db, lambda).expect("closure environment").expect("captures");
        let capture = environment.captures.first().expect("one capture");
        let copied = capture_copy_specialization(&db, capture.declaration, None)
            .expect("capture copy")
            .expect("a captured list is a copy");
        assert_eq!(item_name(&db, copied.declaration).expect("hook name").as_deref(), Some("Copy"));
    }

    #[test]
    fn a_match_pattern_binding_is_a_new_name() {
        let source = format!(
            "{BAG}pub enum Slot {{ Full(Bag<i64> bag), Empty }} \
             i64 Main(Slot slot) {{ return match slot {{ Slot::Full(held) => held.count, Slot::Empty => 0_i64, }}; }}"
        );
        let (db, root) = assembly(&source, 6706);
        fn identifiers(db: &dyn Db, key: AstNodeKey, found: &mut Vec<AstNodeKey>) {
            if node_kind(db, key).expect("node kind") == Some(IndexedNodeKind::Identifier) {
                found.push(key);
            }
            for child in child_nodes(db, key).expect("child nodes").iter().flat_map(|children| children.iter()) {
                identifiers(db, *child, found);
            }
        }
        let matched = first_of_kind(&db, root, IndexedNodeKind::MatchExpression).expect("match");
        let mut names = Vec::new();
        identifiers(&db, matched, &mut names);
        let copies = names
            .iter()
            .filter_map(|name| pattern_binding_copy_specialization(&db, *name, None).expect("pattern binding copy"))
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 1, "only the binding `held` is a new name");
        let copied = &copies[0];
        assert_eq!(item_name(&db, copied.declaration).expect("hook name").as_deref(), Some("Copy"));
        let not_a_binding = first_of_kind(&db, root, IndexedNodeKind::LetStatement)
            .or_else(|| first_of_kind(&db, root, IndexedNodeKind::FunctionDefinition))
            .expect("other node");
        assert_eq!(pattern_binding_copy_specialization(&db, not_a_binding, None).expect("not a binding"), None);
    }

    const HOOKS: &str = "[CopyOnAssign] pub type Bag<T> { T[] items; i64 count; \
        pub Bag<T> Copy() { return Bag<T> { items: this.items, count: this.count }; } \
        pub Bag<T> Borrow() { return Bag<T> { items: this.items, count: this.count }; } \
        pub Bag<T> Detached() { return Bag<T> { items: this.items, count: this.count }; } } \
        pub enum Wrapped { Full(Bag<i64> bag), Empty } \
        unit Keep(Bag<i64> kept) { return; } \
        unit Store(move Bag<i64> stored) { return; } \
        Bag<i64> Make() { return Bag<i64> { items: [1_i64], count: 1_i64 }; } ";

    /// L1: a plain parameter borrows (`Borrow`), a `move` parameter and a `spawn` target keep
    /// (`Copy`); an owned temporary needs no copy; a `move` operand of a port send is `Detached`.
    #[test]
    fn lent_kept_owned_and_crossing_reads_pick_their_hook() {
        let source = format!(
            "{HOOKS}Wrapped Build() {{ Bag<i64> built = Make(); return Wrapped::Full(built); }} \
             unit Main(Bag<i64> lent, Bag<i64> kept, Bag<i64> spawned) {{ Keep(lent); Store(kept); spawn Keep(spawned); \
             Bag<i64> sent = Make(); i64 r = __port_send(1_i64, move sent, 0_i64); return; }} \
             i64 Lambda() {{ Bag<i64> outer = Make(); let f = () => outer; return 0_i64; }}"
        );
        let (db, root) = assembly(&source, 6710);
        let position = |text: &str| {
            let key = place(&db, &source, root, text).unwrap_or_else(|| panic!("place `{text}`"));
            (value_copy_position(&db, key).expect("copy position"), key)
        };
        let hook = |key: AstNodeKey| {
            value_copy_specialization(&db, key, None)
                .expect("hook")
                .and_then(|instance| item_name(&db, instance.declaration).expect("hook name"))
                .map(|name| name.to_string())
        };
        let (lent, key) = position("lent");
        assert_eq!(lent, Some(ValueCopyPosition::LentArgument));
        assert_eq!(hook(key).as_deref(), Some("Borrow"));
        let (kept, key) = position("kept");
        assert_eq!(kept, Some(ValueCopyPosition::Argument), "a `move` parameter keeps its argument");
        assert_eq!(hook(key).as_deref(), Some("Copy"));
        assert_eq!(position("spawned").0, Some(ValueCopyPosition::Argument), "a fiber outlives the call");
        let (sent, key) = position("sent");
        assert_eq!(sent, Some(ValueCopyPosition::Crossing));
        assert_eq!(hook(key).as_deref(), Some("Detached"));
        assert_eq!(position("built").0, None, "a local returned as a payload is taken over");
        assert_eq!(position("outer").0, Some(ValueCopyPosition::Return), "a captured local is not the lambda's");
    }

    #[test]
    fn a_try_on_a_call_result_is_an_owned_temporary() {
        let source = format!(
            "{BAG}Core.Results.Result<Bag<i64>, i64> Make(Bag<i64> b) {{ return Core.Results.Result::Ok(b); }} \
             Core.Results.Result<i64, i64> Main(Bag<i64> a, Core.Results.Result<Bag<i64>, i64> held) {{ \
             Bag<i64> taken = Make(a)?; Bag<i64> named = held?; return Core.Results.Result::Ok(taken.count + named.count); }}"
        );
        let (db, root) = assembly(&source, 6711);
        fn tries(db: &dyn Db, key: AstNodeKey, found: &mut Vec<AstNodeKey>) {
            if node_kind(db, key).expect("node kind") == Some(IndexedNodeKind::TryExpression) {
                found.push(key);
            }
            for child in child_nodes(db, key).expect("child nodes").iter().flat_map(|children| children.iter()) {
                tries(db, *child, found);
            }
        }
        let mut found = Vec::new();
        tries(&db, root, &mut found);
        let positions =
            found.iter().map(|key| value_copy_position(&db, *key).expect("position")).collect::<Vec<_>>();
        assert_eq!(positions, [None, Some(ValueCopyPosition::Binding)], "`Make(a)?` is owned, `held?` is a new name");
    }
}
