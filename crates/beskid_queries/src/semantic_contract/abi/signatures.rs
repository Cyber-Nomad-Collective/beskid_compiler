//! Focused ABI semantic implementation.

use super::super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn item_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |_program, _index, node| item_signature_for_node(node))?.transpose()
}

pub(in crate::semantic_contract) fn item_signature_for_node(
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<ItemSignature, SemanticError>> {
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return Some(signature_from_syntax(&function.parameters, function.return_type.as_ref()));
    }
    if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        return Some(signature_from_syntax(&method.parameters, method.return_type.as_ref()));
    }
    if node.of::<beskid_analysis::syntax::TestDefinition>().is_some() {
        return Some(Ok(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::UNIT }));
    }
    if let Some(contract) = node.of::<beskid_analysis::syntax::ContractMethodSignature>() {
        return Some(signature_from_syntax(&contract.parameters, contract.return_type.as_ref()));
    }
    None
}

pub(in crate::semantic_contract) fn signature_from_syntax(
    parameters: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>],
    return_type: Option<&beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>>,
) -> Result<ItemSignature, SemanticError> {
    let parameters = parameters
        .iter()
        .map(|parameter| semantic_type_from_syntax(&parameter.node.ty.node))
        .collect::<Result<Vec<_>, _>>()?;
    let result =
        return_type.map_or(Ok(SemanticTypeId::UNIT), |return_type| semantic_type_from_syntax(&return_type.node))?;
    Ok(ItemSignature { parameters: parameters.into(), result })
}

/// ABI-representation signature for syntax-only lowering.
///
/// Nominal source identity remains in [`item_signature`]. ABI v5 passes every declared nominal
/// aggregate by reference, represented as one target-sized pointer; only source declaration
/// resolution is needed to prove that representation.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn item_abi_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            // Generic declarations have no single item ABI. Call sites must prove a concrete
            // specialization; otherwise module emission would register `Item` while calls import
            // `SpecializedItem` (for example `Channel<T> Create<T>()` collapsing to POINTER).
            if !function.generics.is_empty() {
                return None;
            }
            return Some(abi_signature_from_syntax(db, key, &function.parameters, function.return_type.as_ref()));
        }
        if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
            let generic_owner = parent_node(index, key.node)
                .and_then(|parent| index.node_at(program, parent))
                .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
                .is_some_and(|definition| !definition.generics.is_empty());
            if generic_owner {
                // A method inherits its owning type's substitutions. It can only be emitted after
                // a direct receiver call proves that concrete owner environment.
                return None;
            }
            let mut signature =
                match abi_signature_from_syntax(db, key, &method.parameters, method.return_type.as_ref()) {
                    Ok(signature) => signature,
                    Err(error) => return Some(Err(error)),
                };
            let mut parameters = Vec::with_capacity(signature.parameters.len() + 1);
            parameters.push(SemanticTypeId::POINTER);
            parameters.extend(signature.parameters.iter().copied());
            signature.parameters = parameters.into();
            return Some(Ok(signature));
        }
        if let Some(contract) = node.of::<beskid_analysis::syntax::ContractMethodSignature>() {
            return Some(abi_signature_from_syntax(db, key, &contract.parameters, contract.return_type.as_ref()));
        }
        node.of::<beskid_analysis::syntax::TestDefinition>()
            .map(|_| Ok(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::UNIT }))
    })?
    .transpose()
}

/// Derive one direct call's ABI signature from its declaration and exact source arguments.
///
/// Generic declaration parameters are substituted only when every use is constrained by a
/// current argument with a generation-safe ABI type. This intentionally does not introduce
/// general inference or monomorphization: unsupported generic shapes remain unavailable.
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn call_abi_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |_program, _index, node| {
        node.of::<beskid_analysis::syntax::CallExpression>()?;
        Some(call_abi_signature_for_call(db, key))
    })?
    .transpose()
}

pub(in crate::semantic_contract) fn call_abi_signature_for_call(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<ItemSignature, SemanticError> {
    match call_lowering(db, key)? {
        Some(CallLowering::CorelibService(service)) => {
            return corelib_service_abi_signature(service)
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"));
        }
        Some(CallLowering::Dynamic) => {
            return dispatch_builtin_abi_signature(db, key)
                .ok_or_else(|| SemanticError::unavailable("call_abi_signature"));
        }
        Some(CallLowering::Runtime(RuntimeIntrinsic(index))) => return call_abi::runtime_intrinsic_signature(index),
        None => {
            return Err(SemanticError::unavailable("call_abi_signature"));
        }
        Some(CallLowering::Direct(_)) => {}
    }
    Ok(generic_specialization_instance_for_call(db, key)?.signature)
}
