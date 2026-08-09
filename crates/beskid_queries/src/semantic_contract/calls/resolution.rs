//! Focused call-semantics implementation.

use super::super::*;

pub(in crate::semantic_contract) fn call_lowering_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<CallLowering, SemanticError>> {
    let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
    Some(match &call.callee.node {
        expression if expression_is_lambda(expression) => Ok(CallLowering::Dynamic),
        beskid_analysis::syntax::Expression::Path(path) => {
            let path = &path.node.path.node;
            if imported_generic_nominal_receiver_requires_instantiation(db, key, path) {
                Err(SemanticError::unavailable("generic_receiver_instantiation"))
            } else if let Some(service) = corelib_service_for(db, key, path) {
                Ok(CallLowering::CorelibService(service))
            } else if let Some((declaration, _)) = nominal_local_member_receiver(db, program, index, key, path) {
                Ok(CallLowering::Direct(declaration))
            } else if path.segments.iter().any(|segment| !segment.node.type_args.is_empty()) {
                if let Some(instantiation) = generic_call_instantiation_for_node(db, program, index, key, path) {
                    Ok(CallLowering::Direct(instantiation.declaration))
                } else if let Some(declaration) = resolve_item_declaration_candidate(db, program, index, key, path)
                    && generic_call_uses_parameter_type_arguments(db, key, declaration, path)
                {
                    Ok(CallLowering::Direct(declaration))
                } else if imported_call_receiver_exists(db, key, path) {
                    Ok(CallLowering::Dynamic)
                } else {
                    Err(SemanticError::unavailable("generic_call_instantiation"))
                }
            } else if let Some(declaration) = resolve_item_declaration(db, program, index, key, path) {
                if function_declares_generics(db, declaration) && call.args.is_empty() {
                    Err(SemanticError::unavailable("generic_call_instantiation"))
                } else {
                    Ok(CallLowering::Direct(declaration))
                }
            } else if canonical_runtime_intrinsic_scope(db, key)
                && let Some(intrinsic) = runtime_intrinsic(db, key).ok().flatten()
            {
                // The manifest-owned builtin index is the Salsa fact that separates canonical
                // runtime intrinsics from ordinary Dynamic calls. Codegen still requires its
                // separate canonical-source capability before it can emit this classification.
                Ok(CallLowering::Runtime(intrinsic))
            } else if imported_call_receiver_exists(db, key, path)
                || (path.segments.iter().all(|segment| segment.node.type_args.is_empty())
                    && beskid_analysis::builtins::builtin_for_path(
                        &path.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>(),
                    )
                    .is_some())
            {
                Ok(CallLowering::Dynamic)
            } else if let Some(declaration) = resolve_local_extern_contract_method(program, index, key, path) {
                Ok(CallLowering::Direct(declaration))
            } else {
                Err(SemanticError::unavailable("call_lowering"))
            }
        }
        beskid_analysis::syntax::Expression::Member(member) => {
            // Nominal methods lower Direct when declaration authority exists.
            // Extern/contract members and other receivers without a syntax method
            // declaration remain Dynamic rather than unavailable, matching Path
            // import/builtin fallback so production JIT/AOT can emit the call.
            // Module-qualified calls like `Core.IsEmpty(text)` parse as Member
            // expressions — flatten the member into a module path for direct resolution.
            Ok(method_declaration_for_member_receiver(db, program, index, key, call, member)
                .map(CallLowering::Direct)
                .or_else(|| {
                    flatten_member_as_path_declaration(db, program, index, key, member).map(CallLowering::Direct)
                })
                .unwrap_or(CallLowering::Dynamic))
        }
        _ => Err(SemanticError::unavailable("call_lowering")),
    })
}

/// Runtime intrinsic lowering is available only to the exact embedded corpus. The typed-program
/// constructor installs this private scope after byte-for-byte corpus validation; app/corelib
/// source that merely resolves a builtin remains Dynamic.
pub(in crate::semantic_contract) fn canonical_runtime_intrinsic_scope(db: &dyn Db, key: AstNodeKey) -> bool {
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))
        .is_some_and(|imports| imports.iter().any(|entry| entry.binding == "__beskid_canonical_runtime"))
}

/// Resolve `Contract.method` when `Contract` is an `[Extern]` contract in the current unit.
///
/// Qualified paths parse as Path (not Member), so extern FFI calls like `C.getpid()` need this
/// authority before call_lowering fails closed as unavailable.
pub(in crate::semantic_contract) fn resolve_local_extern_contract_method(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let [contract_segment, method_segment] = path.segments.as_slice() else {
        return None;
    };
    if !contract_segment.node.type_args.is_empty() || !method_segment.node.type_args.is_empty() {
        return None;
    }
    let contract_name = contract_segment.node.name.node.name.as_str();
    let method_name = method_segment.node.name.node.name.as_str();
    let contracts = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::ContractDefinition)
        .filter(|candidate| {
            index
                .node_at(program, *candidate)
                .and_then(|node| node.of::<beskid_analysis::syntax::ContractDefinition>())
                .is_some_and(|contract| {
                    contract.name.node.name == contract_name
                        && contract.attributes.iter().any(|attribute| attribute.node.name.node.name == "Extern")
                })
        })
        .collect::<Vec<_>>();
    let [contract_id] = contracts.as_slice() else {
        return None;
    };
    let methods = index
        .ids_of_kind(beskid_analysis::syntax_query::NodeKind::ContractMethodSignature)
        .filter(|candidate| {
            let Some(metadata) = index.metadata_for(key.generation, *candidate) else {
                return false;
            };
            let mut parent = metadata.parent;
            let mut under_contract = false;
            while let Some(parent_id) = parent {
                if parent_id == *contract_id {
                    under_contract = true;
                    break;
                }
                parent = index.metadata_for(key.generation, parent_id).and_then(|node| node.parent);
            }
            under_contract
                && index
                    .node_at(program, *candidate)
                    .and_then(|node| node.of::<beskid_analysis::syntax::ContractMethodSignature>())
                    .is_some_and(|method| method.name.node.name == method_name)
        })
        .collect::<Vec<_>>();
    let [method_id] = methods.as_slice() else {
        return None;
    };
    Some(AstNodeKey { unit: key.unit, generation: key.generation, node: *method_id })
}

/// Return Extern import metadata for a [`ContractMethodSignature`] declaration key.
pub fn extern_contract_import_for_declaration(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Option<(String, Option<String>, Option<String>)> {
    let syntax = db.syntax_unit(declaration.unit)?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let method = index.node_at(program, declaration.node)?.of::<beskid_analysis::syntax::ContractMethodSignature>()?;
    let mut parent = index.metadata_for(declaration.generation, declaration.node)?.parent;
    let mut contract = None;
    while let Some(parent_id) = parent {
        if let Some(definition) =
            index.node_at(program, parent_id).and_then(|node| node.of::<beskid_analysis::syntax::ContractDefinition>())
        {
            contract = Some(definition);
            break;
        }
        parent = index.metadata_for(declaration.generation, parent_id).and_then(|node| node.parent);
    }
    let contract = contract?;
    let extern_attr = contract.attributes.iter().find(|attribute| attribute.node.name.node.name == "Extern")?;
    let mut abi = None;
    let mut library = None;
    for argument in &extern_attr.node.arguments {
        let value = match &argument.node.value.node {
            beskid_analysis::syntax::Expression::Literal(literal) => match &literal.node.literal.node {
                beskid_analysis::syntax::Literal::String(raw) => {
                    raw.strip_prefix('"').and_then(|value| value.strip_suffix('"')).map(str::to_owned)
                }
                _ => None,
            },
            _ => None,
        };
        match argument.node.name.node.name.as_str() {
            "Abi" => abi = value,
            "Library" => library = value,
            _ => {}
        }
    }
    Some((method.name.node.name.clone(), abi, library))
}

/// Resolve one syntax-authorized nominal receiver and its uniquely declared method. Struct
/// literals and unqualified locals with explicit nominal parameter or let annotations provide
/// the required declaration authority. Inferred locals, extensions, overloads, and chained
/// receivers remain unavailable rather than reconstructing retired HIR type information.
pub(in crate::semantic_contract) fn method_declaration_for_member_receiver(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    call: &beskid_analysis::syntax::CallExpression,
    member: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::MemberExpression>,
) -> Option<AstNodeKey> {
    let callee = index.direct_child_id(
        program,
        key.node,
        beskid_analysis::syntax_query::DynNodeRef::from(call.callee.as_ref()),
    )?;
    let callee = normalized_expression_node(index, callee);
    let receiver = index.direct_child_id(
        program,
        callee,
        beskid_analysis::syntax_query::DynNodeRef::from(member.node.target.as_ref()),
    )?;
    let receiver = AstNodeKey { node: normalized_expression_node(index, receiver), ..key };
    let declaration = aggregate_literal_declaration(db, receiver).ok().flatten().or_else(|| {
        let receiver_node = index.node_at(program, receiver.node)?;
        let path = receiver_node.of::<beskid_analysis::syntax::PathExpression>()?;
        let [segment] = path.path.node.segments.as_slice() else {
            return None;
        };
        if !segment.node.type_args.is_empty() {
            return None;
        }
        nominal_local_receiver_declaration(db, program, index, key, segment.node.name.node.name.as_str())
            .map(|(declaration, _)| declaration)
    })?;
    unique_nominal_method_declaration(db, declaration, &member.node.member.node.name)
}

/// Module-qualified calls like `Core.IsEmpty(text)` parse as `MemberExpression` where the
/// receiver is a `PathExpression` (e.g., `Core`) and the member is the callee name (e.g.,
/// `IsEmpty`). When the receiver isn't a nominal type, flatten the expression into a
/// two-segment path and resolve it as a direct item declaration.
pub(in crate::semantic_contract) fn flatten_member_as_path_declaration(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    member: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::MemberExpression>,
) -> Option<AstNodeKey> {
    let beskid_analysis::syntax::Expression::Path(receiver) = &member.node.target.node else {
        return None;
    };
    let mut segments = receiver.node.path.node.segments.clone();
    segments.push(beskid_analysis::syntax::Spanned::new(
        beskid_analysis::syntax::PathSegment { name: member.node.member.clone(), type_args: Vec::new() },
        member.node.member.span,
    ));
    let path = beskid_analysis::syntax::Spanned::new(beskid_analysis::syntax::Path { segments }, member.span);
    resolve_item_declaration(db, program, index, key, &path.node)
}

/// Resolve an ordinary `local.Method()` spelling only when `local` resolves to an explicitly
/// annotated nominal parameter or let. Qualified static paths and inferred locals are left to
/// their existing path rules or remain unavailable.
pub(in crate::semantic_contract) fn nominal_local_member_receiver(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<(AstNodeKey, AstNodeKey)> {
    let [receiver, member] = path.segments.as_slice() else {
        return None;
    };
    if !receiver.node.type_args.is_empty() || !member.node.type_args.is_empty() {
        return None;
    }
    let (declaration, receiver) =
        nominal_local_receiver_declaration(db, program, index, key, receiver.node.name.node.name.as_str())?;
    unique_nominal_method_declaration(db, declaration, &member.node.name.node.name).map(|method| (method, receiver))
}
#[salsa::tracked]
pub(in crate::semantic_contract) fn nominal_member_receiver_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        nominal_local_member_receiver(db, program, index, key, &path.path.node).map(|(_, receiver)| Ok(receiver))
    })?
    .transpose()
}

pub(in crate::semantic_contract) fn unique_nominal_method_declaration(db: &dyn Db, declaration: AstNodeKey, method_name: &str) -> Option<AstNodeKey> {
    let declaration_syntax = db.syntax_unit(declaration.unit)?;
    let declaration_program = declaration_syntax.expanded_program(db);
    let declaration_index = declaration_syntax.syntax_index(db);
    declaration_index
        .node_at(declaration_program, declaration.node)?
        .of::<beskid_analysis::syntax::TypeDefinition>()?;
    let methods = declaration_index
        .children(declaration.node)?
        .iter()
        .copied()
        .filter(|candidate| {
            declaration_index
                .node_at(declaration_program, *candidate)
                .and_then(|node| node.of::<beskid_analysis::syntax::MethodDefinition>())
                .is_some_and(|method| method.name.node.name == method_name)
        })
        .map(|node| AstNodeKey { unit: declaration.unit, generation: declaration.generation, node })
        .collect::<Vec<_>>();
    (methods.len() == 1).then(|| methods[0])
}

/// Resolve a legacy syscall spelling only when its current source unit was admitted by the
/// compiler-minted Corelib service constructor. The same builtins remain dynamic everywhere else.
pub(in crate::semantic_contract) fn corelib_service_for(db: &dyn Db, key: AstNodeKey, path: &beskid_analysis::syntax::Path) -> Option<CorelibService> {
    let [segment] = path.segments.as_slice() else {
        return None;
    };
    let name = segment.node.name.node.name.as_str();
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .corelib_services
        .get(&(key.unit, key.generation))?
        .iter()
        .copied()
        .find(|service| service.name == name)
}
