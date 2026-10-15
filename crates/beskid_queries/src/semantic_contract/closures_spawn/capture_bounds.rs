//! `[Captures(Bound: C)]` capture bounds of function-typed parameters and fields
//! (`language-meta--evaluation--lambdas-and-closures`, "Capture bounds on function-typed
//! parameters"). The bound is part of the callable signature fact: it is read from the
//! declaration of the parameter (or field) every time a call or initializer is judged.

use super::super::*;
use super::super::transfer::{TransferMarker, recognized_attribute_use, transfer_marker_contract};

/// The bound of one `[Captures]` attribute use: a marker contract, or the spelling of a bound
/// that is not one (`None` when the `Bound` argument is missing).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::semantic_contract) enum CapturesBound {
    Marker(TransferMarker),
    NotMarker(Option<String>),
}

/// The `[Captures]` bound among `attributes`, written in the unit of `key`.
pub(in crate::semantic_contract) fn captures_bound(
    db: &dyn Db,
    key: AstNodeKey,
    attributes: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Attribute>],
) -> Option<(CapturesBound, beskid_analysis::syntax::AstNodeId)> {
    let attribute = recognized_attribute_use(db, key, attributes, "Captures")?;
    let Some(argument) = attribute.node.arguments.iter().find(|argument| argument.node.name.node.name == "Bound") else {
        return Some((CapturesBound::NotMarker(None), attribute.id));
    };
    let beskid_analysis::syntax::Expression::Path(path) = &argument.node.value.node else {
        return Some((CapturesBound::NotMarker(Some("<expression>".to_owned())), attribute.id));
    };
    let spelling =
        path.node.path.node.segments.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>().join(".");
    let marker = super::super::contracts::resolve_contract(db, key, &path.node.path.node)
        .and_then(|contract| transfer_marker_contract(db, contract));
    Some((marker.map_or(CapturesBound::NotMarker(Some(spelling)), CapturesBound::Marker), attribute.id))
}

/// Read the syntax node of a declaration `key` in its own unit.
pub(in crate::semantic_contract) fn with_declaration<R>(
    db: &dyn Db,
    key: AstNodeKey,
    read: impl FnOnce(beskid_analysis::syntax_query::DynNodeRef<'_>) -> Option<R>,
) -> Option<R> {
    let syntax = db.syntax_unit(key.unit).filter(|syntax| syntax.accepts_key(db, key))?;
    let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), key.node)?;
    read(node)
}

/// The explicit parameters of a callable declaration (function, method or contract method), and
/// whether a method declares `move this`.
pub(in crate::semantic_contract) fn declared_parameters(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Option<(Vec<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>>, bool)> {
    with_declaration(db, declaration, |node| {
        if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            return Some((function.parameters.clone(), false));
        }
        if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
            let owning = method.receiver.as_ref().is_some_and(|receiver| receiver.node.movable);
            return Some((method.parameters.clone(), owning));
        }
        node.of::<beskid_analysis::syntax::ContractMethodSignature>().map(|signature| (signature.parameters.clone(), false))
    })
}

/// The directly called declaration of the call expression `call`, when it is a source item.
pub(in crate::semantic_contract) fn direct_callee(db: &dyn Db, call: AstNodeKey) -> Option<AstNodeKey> {
    match call_lowering(db, call).ok().flatten()? {
        CallLowering::Direct(declaration) => Some(declaration),
        _ => None,
    }
}

/// The capture bound of the parameter at explicit argument `position` of the call `call`.
pub(in crate::semantic_contract) fn call_argument_captures_bound(
    db: &dyn Db,
    call: AstNodeKey,
    position: usize,
) -> Option<CapturesBound> {
    let declaration = direct_callee(db, call)?;
    let (parameters, _) = declared_parameters(db, declaration)?;
    let parameter = parameters.get(position)?;
    captures_bound(db, declaration, &parameter.node.attributes).map(|(bound, _)| bound)
}

/// The capture bound of the field `field` of the nominal type that `literal_or_place` names.
pub(in crate::semantic_contract) fn field_captures_bound(
    db: &dyn Db,
    owner: AstNodeKey,
    field: &str,
) -> Option<CapturesBound> {
    with_declaration(db, owner, |node| {
        let definition = node.of::<beskid_analysis::syntax::TypeDefinition>()?;
        let declared = definition.fields.iter().find(|candidate| candidate.node.name.node.name == field)?;
        captures_bound(db, owner, &declared.node.attributes).map(|(bound, _)| bound)
    })
}
