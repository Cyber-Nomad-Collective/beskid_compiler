//! Explicit generic type arguments on call paths and member names.

use super::super::super::*;
use super::*;

pub(in crate::semantic_contract) fn explicit_generic_type_argument_syntax(
    path: &beskid_analysis::syntax::Path,
) -> Option<&[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>]> {
    let terminal = path.segments.last()?;
    let receiver = path.segments.get(..path.segments.len().checked_sub(1)?)?;
    let receiver_with_arguments =
        receiver.iter().filter(|segment| !segment.node.type_args.is_empty()).collect::<Vec<_>>();
    let terminal_has_arguments = !terminal.node.type_args.is_empty();
    match (terminal_has_arguments, receiver_with_arguments.as_slice()) {
        (true, []) => Some(terminal.node.type_args.as_slice()),
        (false, [receiver]) => Some(receiver.node.type_args.as_slice()),
        _ => None,
    }
}

pub(in crate::semantic_contract) fn type_syntax_is_generic_parameter_reference(
    syntax_type: &beskid_analysis::syntax::Type,
    parameter_name: &str,
) -> bool {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return false;
    };
    let [segment] = path.node.segments.as_slice() else {
        return false;
    };
    segment.node.type_args.is_empty() && segment.node.name.node.name == parameter_name
}

pub(in crate::semantic_contract) fn type_syntax_is_enclosing_generic_parameter_reference(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> bool {
    let Some(parameter_name) = generic_parameter_reference_name(syntax_type) else {
        return false;
    };
    let Some(syntax) = db.syntax_unit(key.unit) else {
        return false;
    };
    if !syntax.accepts_key(db, key) {
        return false;
    }
    let index = syntax.syntax_index(db);
    let Some(enclosing) = nearest_ancestor(index, key.node, |kind| {
        matches!(
            kind,
            beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                | beskid_analysis::syntax_query::NodeKind::MethodDefinition
        )
    }) else {
        return false;
    };
    let program = syntax.expanded_program(db);
    let Some(enclosing_node) = index.node_at(program, enclosing) else {
        return false;
    };
    if let Some(function) = enclosing_node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return function.generics.iter().any(|generic| generic.node.name == parameter_name);
    }
    // A method sees its owner's type parameters and its own.
    generic_callable_parameters(db, AstNodeKey { node: enclosing, ..key })
        .is_some_and(|(names, _)| names.contains(&parameter_name))
}

pub(in crate::semantic_contract) fn generic_call_uses_parameter_type_arguments(
    db: &dyn Db,
    key: AstNodeKey,
    declaration: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let Some(type_arguments) = explicit_generic_type_argument_syntax(path) else {
        return false;
    };
    let Some(syntax) = db.syntax_unit(declaration.unit) else {
        return false;
    };
    if !syntax.accepts_key(db, declaration) {
        return false;
    }
    let Some((generic_names, _)) = generic_callable_parameters(db, declaration) else {
        return false;
    };
    if generic_names.len() != type_arguments.len() {
        return false;
    }
    type_arguments.iter().zip(generic_names.iter()).all(|(argument, generic)| {
        abi_type_from_syntax(db, key, &argument.node).is_ok()
            || type_syntax_is_generic_parameter_reference(&argument.node, generic)
            || type_syntax_is_enclosing_generic_parameter_reference(db, key, &argument.node)
    })
}

/// The explicit type arguments written on a method call's member name: the last segment of a
/// path callee (`s.Spawn<i64>(...)`) or a member access (`values.Parallel().Map<i64>(...)`).
pub(in crate::semantic_contract) fn call_explicit_member_type_arguments(
    db: &dyn Db,
    key: AstNodeKey,
) -> Option<Vec<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>>> {
    let syntax = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key))?;
    let call = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), key.node)?
        .of::<beskid_analysis::syntax::CallExpression>()?;
    let mut callee = &call.callee.node;
    while let beskid_analysis::syntax::Expression::Grouped(grouped) = callee {
        callee = &grouped.node.expr.node;
    }
    let arguments = match callee {
        beskid_analysis::syntax::Expression::Path(path) => &path.node.path.node.segments.last()?.node.type_args,
        beskid_analysis::syntax::Expression::Member(member) => &member.node.type_args,
        _ => return None,
    };
    (!arguments.is_empty()).then(|| arguments.clone())
}
