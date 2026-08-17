//! `bulk` parameter calling-convention semantic fact.
//!
//! A `bulk` parameter lowers as a single array parameter at the callee (the signature shape is
//! unchanged), so this fact deliberately does not alter [`item_signature`] / [`item_abi_signature`].
//! It only marks which parameter is bulk and its declared element ABI type, so a later bulk-call
//! lowering slice can pack N scalar arguments into a fresh rooted array before the direct call.
//!
//! [`item_signature`]: super::abi::item_signature_tracked
//! [`item_abi_signature`]: super::abi::item_abi_signature_tracked

use super::*;

/// Return the bulk calling-convention fact for one parameter node.
///
/// The fact exists only when the parameter carries the `bulk` modifier and its declared type is
/// an array `T[]`; the element ABI is taken from the declared element type (the same
/// declared-type-over-inferred authority used by `empty_array_literal_element_abi_type`). The
/// parameter index is the position of the parameter in its enclosing callable's parameter list
/// (declaration order). Stale, unregistered, non-parameter, non-bulk, and non-array parameters
/// contain no fact.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn bulk_parameter_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<BulkParameterFact> {
    with_node(db, syntax, key, |program, index, node| {
        let parameter = node.of::<beskid_analysis::syntax::Parameter>()?;
        if !parameter.bulk {
            return None;
        }
        let beskid_analysis::syntax::Type::Array(element) = &parameter.ty.node else {
            // `bulk` is only meaningful on an array parameter; a non-array bulk declaration is a
            // source error surfaced elsewhere, so this fact fails closed rather than inferring.
            return None;
        };
        let element_abi_type = match abi_type_from_syntax(db, key, &element.node) {
            Ok(element_abi_type) => element_abi_type,
            Err(error) => return Some(Err(error)),
        };
        let parent = parent_node(index, key.node)?;
        let parent_node_ref = index.node_at(program, parent)?;
        let parameters = parent_node_ref
            .of::<beskid_analysis::syntax::FunctionDefinition>()
            .map(|function| function.parameters.as_slice())
            .or_else(|| {
                parent_node_ref
                    .of::<beskid_analysis::syntax::MethodDefinition>()
                    .map(|method| method.parameters.as_slice())
            })
            .or_else(|| {
                parent_node_ref
                    .of::<beskid_analysis::syntax::ContractMethodSignature>()
                    .map(|contract| contract.parameters.as_slice())
            })?;
        let mut parameter_index = 0u32;
        let mut found = false;
        for candidate in parameters {
            let Some(candidate_id) =
                index.direct_child_id(program, parent, beskid_analysis::syntax_query::DynNodeRef::from(candidate))
            else {
                parameter_index = parameter_index.saturating_add(1);
                continue;
            };
            if candidate_id == key.node {
                found = true;
                break;
            }
            parameter_index = parameter_index.saturating_add(1);
        }
        if !found {
            return None;
        }
        Some(Ok(BulkParameterFact { parameter: key, parameter_index, element_abi_type }))
    })?
    .transpose()
}
