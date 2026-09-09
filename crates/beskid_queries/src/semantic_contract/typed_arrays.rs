use super::*;

/// Recognize the single typed allocation form embedded in canonical Foundation `Array.Empty`.
///
/// Source authority is installed by `build_typed_program_with_corelib_services` only after the
/// assembly has matched both the compiler-owned Array bytes and its trusted physical origin. The
/// fact deliberately retains the generic parameter name; concrete ABI identity belongs to the
/// caller-derived specialization environment consumed by codegen.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn typed_array_allocation_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<TypedArrayAllocation> {
    let authorized = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .corelib_services
        .contains_key(&(key.unit, key.generation));
    if !authorized {
        return Ok(None);
    }

    with_node(db, syntax, key, |_program, _index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        let [callee] = path.node.path.node.segments.as_slice() else {
            return None;
        };
        if callee.node.name.node.name != "__array_new" {
            return None;
        }
        let [element] = callee.node.type_args.as_slice() else {
            return None;
        };
        let element_parameter = generic_parameter_reference_name(&element.node)?;
        let [length] = call.args.as_slice() else {
            return None;
        };
        let beskid_analysis::syntax::Expression::Literal(literal) = &length.node else {
            return None;
        };
        let beskid_analysis::syntax::Literal::Integer(text) = &literal.node.literal.node else {
            return None;
        };
        let length = integer_literal_u64(text)?;
        (length == 0).then(|| TypedArrayAllocation { element_parameter: element_parameter.into(), length })
    })
}
