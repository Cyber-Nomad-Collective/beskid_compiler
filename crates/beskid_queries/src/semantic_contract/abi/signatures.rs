//! Focused ABI semantic implementation.

use super::super::*;

/// Declared return authority for executable functions and methods, not contract signatures.
pub(in crate::semantic_contract) fn declared_callable_return_type<'a>(
    node: beskid_analysis::syntax_query::DynNodeRef<'a>,
) -> Option<&'a beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>> {
    node.of::<beskid_analysis::syntax::FunctionDefinition>().and_then(|function| function.return_type.as_ref()).or_else(
        || node.of::<beskid_analysis::syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref()),
    )
}

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
    let return_type = declared_callable_return_type(node);
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return Some(signature_from_syntax(&function.parameters, return_type));
    }
    if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        return Some(signature_from_syntax(&method.parameters, return_type));
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
        let return_type = declared_callable_return_type(node);
        if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            // Generic declarations have no single item ABI. Call sites must prove a concrete
            // specialization; otherwise module emission would register `Item` while calls import
            // `SpecializedItem` (for example `Channel<T> Create<T>()` collapsing to POINTER).
            if !function.generics.is_empty() {
                return None;
            }
            match contract_template_signature(db, key, &function.parameters, return_type) {
                Ok(true) => return None,
                Ok(false) => (),
                Err(error) => return Some(Err(error)),
            }
            return Some(abi_signature_from_syntax(db, key, &function.parameters, return_type));
        }
        if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
            let generic_owner = method_owner_node(program, index, key.node)
                .and_then(|parent| index.node_at(program, parent))
                .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
                .is_some_and(|definition| !definition.generics.is_empty());
            if generic_owner {
                // A method inherits its owning type's substitutions. It can only be emitted after
                // a direct receiver call proves that concrete owner environment.
                return None;
            }
            match contract_template_signature(db, key, &method.parameters, return_type) {
                Ok(true) => return None,
                Ok(false) => (),
                Err(error) => return Some(Err(error)),
            }
            let mut signature = match abi_signature_from_syntax(db, key, &method.parameters, return_type) {
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
            let mut signature =
                match abi_signature_from_syntax(db, key, &contract.parameters, contract.return_type.as_ref()) {
                    Ok(signature) => signature,
                    Err(error) => return Some(Err(error)),
                };
            if extern_contract_import_for_declaration(db, key).is_none() {
                let mut parameters = vec![SemanticTypeId::POINTER];
                parameters.extend(signature.parameters.iter().copied());
                signature.parameters = parameters.into();
            }
            return Some(Ok(signature));
        }
        node.of::<beskid_analysis::syntax::TestDefinition>()
            .map(|_| Ok(ItemSignature { parameters: Arc::from([]), result: SemanticTypeId::UNIT }))
    })?
    .transpose()
}

/// Defer only proven contract parameters. An unrelated unresolved type is still an error.
fn contract_template_signature(
    db: &dyn Db,
    key: AstNodeKey,
    parameters: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>],
    result: Option<&beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>>,
) -> Result<bool, SemanticError> {
    let contracts = contract_parameter_declarations(db, key);
    if contracts.is_empty() {
        return Ok(false);
    }
    for (position, parameter) in parameters.iter().enumerate() {
        if !contracts.iter().any(|(_, candidate, _)| *candidate as usize == position) {
            item_abi_type_from_syntax(db, key, &parameter.node.ty.node)?;
        }
    }
    if let Some(result) = result {
        item_abi_type_from_syntax(db, key, &result.node)?;
    }
    Ok(true)
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
        Some(CallLowering::ManifestBuiltin(builtin)) => return manifest_builtin_abi_signature(builtin),
        Some(CallLowering::Dynamic) => return Err(SemanticError::unavailable("call_abi_signature")),
        Some(CallLowering::Runtime(RuntimeIntrinsic(index))) => return call_abi::runtime_intrinsic_signature(index),
        None => {
            return Err(SemanticError::unavailable("call_abi_signature"));
        }
        Some(CallLowering::Direct(declaration)) => {
            // Contract signatures describe the visible call shape, not executable bodies.
            // Ordinary implementations are selected later by the enclosing witness; extern
            // signatures remain concrete import declarations.
            if node_kind(db, declaration)? == Some(IndexedNodeKind::ContractMethodSignature) {
                return item_abi_signature(db, declaration)?
                    .ok_or_else(|| SemanticError::unavailable("call_abi_signature"));
            }
        }
    }
    Ok(generic_specialization_instance_for_call(db, key)?.signature)
}
