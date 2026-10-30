//! Array growth never reaches another handle (v0.6.7, `language-meta--memory-model--memory-and-references`).
//!
//! A `T[]` value is a handle to one array header (data, length, capacity). `Array.Append` grows
//! that header in place while capacity remains, so a second handle to the same header would
//! observe the new length and every later write into the spare capacity. The rule is the
//! opposite: growing one handle never changes what another handle sees. The array therefore
//! carries a sharing mark (bit 0 of the hidden element-descriptor word before its header). The
//! compiler sets the mark wherever a second handle to an existing array is made; a growth of a
//! marked array copies it into a new, unmarked array that only the grower holds, and an unmarked
//! array (whose old handle is provably unused elsewhere) keeps growing in place.
//!
//! A second handle is made by:
//! - a read of an array place (local, parameter, field, element, `r?` payload) in a copy
//!   position of copy on assignment ([`value_copy_position`]): a `let` value, an assignment value,
//!   an argument the callee may keep, a construction field, element or payload, a crossing
//!   operand, and a returned place the callable does not own. A returned plain or `mut`
//!   parameter is not a new handle here: the caller that receives it decides (below);
//! - a read of a field or element as an argument of a plain or `mut` parameter when the call can
//!   reach the object that holds it (another argument or the receiver is an object, enum,
//!   closure or contract value): the callee could then grow the field while it holds the
//!   argument;
//! - a call result (an ordinary call, or the canonical `Append`, `Clear`, `RemoveLast`) in a copy
//!   position when the call takes an array argument, because the result may be that argument.
//!   `xs = F(xs, ..)` that only stores the result back into the place every array argument reads
//!   makes no second handle, and neither does a returned result whose array arguments are all
//!   locals or parameters of the returning callable.
//!
//! A read as an argument of a plain parameter of a call that cannot reach the holder is a loan
//! for the call, and the reads inside the `Copy`, `Borrow` and `Detached` hooks of a
//! `[CopyOnAssign]` type are governed by that type's own sharing state (`List` counts holders).

use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

use super::value_copy::{Program, place_identity, value_position};
use super::*;

/// Canonical `Core.Collections.Array` functions that return their first (array) argument.
const RETURNS_FIRST_ARGUMENT: &[&str] = &["Append", "Set", "Clear", "RemoveLast"];

/// Canonical `Core.Collections.Array` functions that return a new array no other handle holds.
const RETURNS_NEW_ARRAY: &[&str] = &["Empty", "Zeroed", "Of", "Copy", "CopyRange"];

/// Whether the expression `key` (a place read or a call) makes a second handle to an existing
/// array, so lowering marks the array shared after evaluating it. `enclosing` is the
/// specialization of the item being lowered. Unproven types are not arrays here: every place
/// type is proven by the authority that types its reads, which reports its own failures.
pub fn array_share(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
) -> Result<bool, SemanticError> {
    let Some(syntax) = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key)) else {
        return Ok(false);
    };
    let index = syntax.syntax_index(db);
    let program = syntax.expanded_program(db);
    let shares = match index.kind(key.node) {
        Some(
            NodeKind::PathExpression | NodeKind::MemberExpression | NodeKind::IndexExpression | NodeKind::TryExpression,
        ) => place_shares(db, key, enclosing, program, index),
        Some(NodeKind::CallExpression) => call_result_shares(db, key, enclosing, program, index),
        _ => Ok(false),
    };
    // Marking is always sound for an array; an unproven position of a proven array marks it.
    match shares {
        Err(_) if is_array(db, key, enclosing) => Ok(true),
        shares => shares,
    }
}

fn is_array(db: &dyn Db, key: AstNodeKey, enclosing: Option<&GenericSpecializationInstance>) -> bool {
    matches!(place_identity(db, key, enclosing), Ok(GenericSourceTypeIdentity::Array(_)))
}

fn place_shares(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
    program: &Program,
    index: &SyntaxIndex,
) -> Result<bool, SemanticError> {
    let position = value_copy_position(db, key)?;
    // Without a copy position only a `mut` parameter argument can make a second handle (and a
    // `?` that reads no place has no position at all).
    if position.is_none()
        && (index.kind(key.node) == Some(NodeKind::TryExpression) || argument_of(program, index, key.node).is_none())
    {
        return Ok(false);
    }
    if !is_array(db, key, enclosing) || inside_copy_on_assign_hook(db, key, program, index) {
        return Ok(false);
    }
    // The array operand of a canonical `Core.Collections.Array` function (`Len`, `Capacity`, the
    // owner of `Append`, ...) stays with the caller: those functions run no user code and keep no
    // handle. A call result that returns the operand is decided at the call (below).
    if let Some((call, 0)) = argument_of(program, index, key.node)
        && super::queries::canonical_array_function(db, AstNodeKey { node: call, ..key })?.is_some()
    {
        return Ok(false);
    }
    // A runtime intrinsic (`__array_len(values)`, an I/O read into a buffer) is a primitive of
    // the runtime: it reads or fills the array for the call and keeps no handle.
    if let Some((call, _)) = argument_of(program, index, key.node)
        && is_intrinsic_call(program, index, call)
    {
        return Ok(false);
    }
    match position {
        Some(ValueCopyPosition::Return) => Ok(!returns_parameter(db, key, program, index, key.node)),
        Some(ValueCopyPosition::LentArgument) | None => lent_place_shares(db, key, enclosing, program, index),
        Some(_) => Ok(true),
    }
}

/// A plain or `mut` argument read from a field or element makes a second handle only when the
/// call can reach the holder. A local or parameter is out of the callee's reach.
fn lent_place_shares(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
    program: &Program,
    index: &SyntaxIndex,
) -> Result<bool, SemanticError> {
    if local_declaration(db, key, program, index, key.node).is_some() {
        return Ok(false);
    }
    let Some((call, position)) = argument_of(program, index, key.node) else {
        return Ok(true);
    };
    let call = AstNodeKey { node: call, ..key };
    // `exchange(place, move value)` writes its first argument in place and hands the old value
    // over: no second handle.
    if position == 0 && exchange_call_for_node(program, index, call).is_some() {
        return Ok(false);
    }
    // A canonical `Core.Collections.Array` function runs no user code: the owner of a growth and
    // the array of a `Len`, `Get` or `Set` stay with the caller.
    if super::queries::canonical_array_function(db, call)?.is_some() {
        return Ok(false);
    }
    let Some(declaration) = direct_callee(db, call) else {
        return Ok(true);
    };
    // A method reaches its receiver (`this`).
    if with_declaration(db, declaration, |node| node.of::<beskid_analysis::syntax::MethodDefinition>().map(|_| ()))
        .is_some()
    {
        return Ok(true);
    }
    let Some(arguments) = call_arguments(db, call)? else {
        return Ok(true);
    };
    for (other, argument) in arguments.iter().enumerate() {
        if other == position {
            continue;
        }
        match specialized_source_expression_identity(db, *argument, enclosing) {
            Ok(identity) if !reaches_objects(&identity) => {}
            _ => return Ok(true),
        }
    }
    Ok(false)
}

/// Whether a value of `identity` is or directly holds a reference that can lead to an object:
/// an object, struct or enum value, a closure, a contract value. Primitives, strings and arrays
/// (whose elements are reached only by reading them) do not.
fn reaches_objects(identity: &GenericSourceTypeIdentity) -> bool {
    !matches!(identity, GenericSourceTypeIdentity::Abi(_) | GenericSourceTypeIdentity::Array(_))
}

fn call_result_shares(
    db: &dyn Db,
    key: AstNodeKey,
    enclosing: Option<&GenericSpecializationInstance>,
    program: &Program,
    index: &SyntaxIndex,
) -> Result<bool, SemanticError> {
    let position = value_position(db, key, program, index, key.node);
    let Some(position) = position else {
        return Ok(false);
    };
    if position == ValueCopyPosition::LentArgument
        || !matches!(
            specialized_source_expression_identity(db, key, enclosing),
            Ok(GenericSourceTypeIdentity::Array(_))
        )
    {
        return Ok(false);
    }
    let Some(arguments) = call_arguments(db, key)? else {
        return Ok(true);
    };
    let canonical = super::queries::canonical_array_function(db, key)?;
    let candidates: Vec<AstNodeKey> = match canonical.as_deref() {
        Some(name) if RETURNS_NEW_ARRAY.contains(&name) => return Ok(false),
        Some(name) if RETURNS_FIRST_ARGUMENT.contains(&name) => arguments.first().copied().into_iter().collect(),
        _ => arguments.iter().copied().filter(|argument| is_array(db, *argument, enclosing)).collect(),
    };
    if candidates.is_empty() {
        return Ok(false);
    }
    let candidates: Vec<_> =
        candidates.iter().map(|argument| normalized_expression_node(index, argument.node)).collect();
    match position {
        ValueCopyPosition::Assignment => {
            let Some(target) = assignment_target(program, index, key.node) else {
                return Ok(true);
            };
            Ok(!candidates.iter().all(|argument| same_place(program, index, *argument, target)))
        }
        ValueCopyPosition::Return => {
            Ok(!candidates.iter().all(|argument| returns_local(db, key, program, index, *argument)))
        }
        _ => Ok(true),
    }
}

/// The declaration of a single-segment path read of a local binding or parameter of the callable
/// that holds the read (no lambda between the read and its declaration).
fn local_declaration(
    db: &dyn Db,
    key: AstNodeKey,
    program: &Program,
    index: &SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let node = normalized_expression_node(index, node);
    index.node_at(program, node)?.of::<beskid_analysis::syntax::PathExpression>()?;
    let resolved = resolved_local(db, AstNodeKey { node, ..key }).ok()??;
    if resolved.declaration.unit != key.unit || resolved.declaration.generation != key.generation {
        return None;
    }
    if super::value_copy::crosses_lambda(index, node, resolved.declaration.node) {
        return None;
    }
    Some(resolved.declaration.node)
}

/// A returned read of a plain or `mut` parameter of the same callable: the handle came from the
/// caller, which decides whether the call result makes a second handle.
fn returns_parameter(
    db: &dyn Db,
    key: AstNodeKey,
    program: &Program,
    index: &SyntaxIndex,
    place: beskid_analysis::syntax::AstNodeId,
) -> bool {
    local_declaration(db, key, program, index, place)
        .and_then(|declaration| parent_node(index, declaration))
        .is_some_and(|declaring| index.kind(declaring) == Some(NodeKind::Parameter))
}

/// A `let` local, or a parameter, of the callable that returns `place`.
fn returns_local(
    db: &dyn Db,
    key: AstNodeKey,
    program: &Program,
    index: &SyntaxIndex,
    place: beskid_analysis::syntax::AstNodeId,
) -> bool {
    local_declaration(db, key, program, index, place)
        .and_then(|declaration| parent_node(index, declaration))
        .is_some_and(|declaring| matches!(index.kind(declaring), Some(NodeKind::Parameter | NodeKind::LetStatement)))
}

/// The call and argument position whose argument is `node` (through transparent wrappers).
fn argument_of(
    program: &Program,
    index: &SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<(beskid_analysis::syntax::AstNodeId, usize)> {
    let mut child = node;
    loop {
        let parent = parent_node(index, child)?;
        match index.kind(parent)? {
            NodeKind::Expression | NodeKind::GroupedExpression => child = parent,
            NodeKind::CallExpression => {
                let call = index.node_at(program, parent)?.of::<beskid_analysis::syntax::CallExpression>()?;
                let position = call.args.iter().position(|argument| {
                    index.direct_child_id(program, parent, DynNodeRef::from(argument)) == Some(child)
                })?;
                return Some((parent, position));
            }
            _ => return None,
        }
    }
}

/// The target of the assignment whose value is `node` (through transparent wrappers).
fn assignment_target(
    program: &Program,
    index: &SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut child = node;
    loop {
        let parent = parent_node(index, child)?;
        match index.kind(parent)? {
            NodeKind::Expression | NodeKind::GroupedExpression => child = parent,
            NodeKind::AssignExpression => {
                let assignment = index.node_at(program, parent)?.of::<beskid_analysis::syntax::AssignExpression>()?;
                if assignment.op.node != beskid_analysis::syntax::AssignOp::Assign {
                    return None;
                }
                let target = index.direct_child_id(program, parent, DynNodeRef::from(assignment.target.as_ref()))?;
                return Some(normalized_expression_node(index, target));
            }
            _ => return None,
        }
    }
}

/// Whether two expressions name the same place: the same local, or the same field of the same
/// place (`this.items`, `a.b.c`).
fn same_place(
    program: &Program,
    index: &SyntaxIndex,
    left: beskid_analysis::syntax::AstNodeId,
    right: beskid_analysis::syntax::AstNodeId,
) -> bool {
    let left = normalized_expression_node(index, left);
    let right = normalized_expression_node(index, right);
    let (Some(left_node), Some(right_node)) = (index.node_at(program, left), index.node_at(program, right)) else {
        return false;
    };
    if let (Some(a), Some(b)) = (
        left_node.of::<beskid_analysis::syntax::PathExpression>(),
        right_node.of::<beskid_analysis::syntax::PathExpression>(),
    ) {
        let names = |path: &beskid_analysis::syntax::PathExpression| {
            path.path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>()
        };
        let (a, b) = (names(a), names(b));
        if a != b {
            return false;
        }
        let Some(first) = a.first() else { return false };
        return resolve_lexical_declaration(program, index, left, first)
            == resolve_lexical_declaration(program, index, right, first);
    }
    if let (Some(a), Some(b)) = (
        left_node.of::<beskid_analysis::syntax::MemberExpression>(),
        right_node.of::<beskid_analysis::syntax::MemberExpression>(),
    ) {
        if a.member.node.name != b.member.node.name {
            return false;
        }
        let (Some(a_target), Some(b_target)) = (
            index.direct_child_id(program, left, DynNodeRef::from(a.target.as_ref())),
            index.direct_child_id(program, right, DynNodeRef::from(b.target.as_ref())),
        ) else {
            return false;
        };
        return same_place(program, index, a_target, b_target);
    }
    false
}

/// The read is inside the `Copy`, `Borrow` or `Detached` hook of a `[CopyOnAssign]` type.
fn inside_copy_on_assign_hook(db: &dyn Db, key: AstNodeKey, program: &Program, index: &SyntaxIndex) -> bool {
    let Some(callable) = super::locals::enclosing_executable_callable(index, key.node) else {
        return false;
    };
    let Some(method) =
        index.node_at(program, callable).and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
    else {
        return false;
    };
    if !matches!(method.name.node.name.as_str(), "Copy" | "Borrow" | "Detached") {
        return false;
    }
    let Some(owner) = parent_node(index, callable) else {
        return false;
    };
    let Some(definition) =
        index.node_at(program, owner).and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
    else {
        return false;
    };
    has_recognized_attribute(db, AstNodeKey { node: owner, ..key }, &definition.attributes, "CopyOnAssign")
}

/// A call of a compiler or runtime intrinsic: its callee is a single-segment path spelled `__name`.
fn is_intrinsic_call(program: &Program, index: &SyntaxIndex, call: beskid_analysis::syntax::AstNodeId) -> bool {
    let Some(call) = index.node_at(program, call).and_then(|node| node.of::<beskid_analysis::syntax::CallExpression>())
    else {
        return false;
    };
    let mut callee = &call.callee.node;
    while let beskid_analysis::syntax::Expression::Grouped(inner) = callee {
        callee = &inner.node.expr.node;
    }
    match callee {
        beskid_analysis::syntax::Expression::Path(path) => match path.node.path.node.segments.as_slice() {
            [segment] => segment.node.name.node.name.starts_with("__"),
            _ => false,
        },
        _ => false,
    }
}
