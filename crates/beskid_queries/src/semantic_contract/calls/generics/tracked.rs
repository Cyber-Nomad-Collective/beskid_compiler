//! Tracked generic call instantiation, specialization, template, and nominal receiver facts.

use super::super::super::*;
use super::*;

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn generic_call_instantiation_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallInstantiation> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        generic_call_instantiation_for_node(db, program, index, key, &path.node.path.node).map(Ok)
    })?
    .transpose()
}
#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn generic_call_specialization_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallSpecialization> {
    with_node(db, syntax, key, |_program, _index, node| {
        node.of::<beskid_analysis::syntax::CallExpression>()?;
        let lowering = match call_lowering(db, key) {
            Ok(Some(lowering)) => lowering,
            Ok(None) => return None,
            // Unavailable call sites cannot contribute call-derived ABI specializations.
            // Propagating the error aborted whole-module emission for Core.Output (enum
            // constructors / unresolved paths in the reachable Syscall body).
            Err(error) if error.is_unavailable() => return None,
            Err(error) => return Some(Err(error)),
        };
        let declaration = match lowering {
            CallLowering::Direct(declaration) => declaration,
            CallLowering::Dynamic
            | CallLowering::ManifestBuiltin(_)
            | CallLowering::Runtime(_)
            | CallLowering::CorelibService(_) => {
                return None;
            }
        };
        if generic_callable_parameters(db, declaration).is_none()
            && contract_parameter_declarations(db, declaration).is_empty()
        {
            return None;
        }
        let instance = match generic_specialization_instance_for_call(db, key) {
            Ok(instance) => instance,
            Err(error) => return Some(Err(error)),
        };
        Some(Ok(GenericCallSpecialization {
            declaration: instance.declaration,
            signature: instance.signature,
            substitutions: instance.substitutions,
            contract_witnesses: instance.contract_witnesses,
        }))
    })?
    .transpose()
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn generic_call_template_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericCallTemplate> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        let argument_syntax = explicit_generic_type_argument_syntax(&path.node.path.node)?;
        let declaration = resolve_item_declaration_candidate(db, program, index, key, &path.node.path.node)?;
        let declaration_syntax = db.syntax_unit(declaration.unit)?;
        let function = declaration_syntax
            .syntax_index(db)
            .node_at(declaration_syntax.expanded_program(db), declaration.node)?
            .of::<beskid_analysis::syntax::FunctionDefinition>()?;
        (function.generics.len() == argument_syntax.len()).then_some(())?;
        let parameter_arguments = argument_syntax
            .iter()
            .map(|argument| generic_parameter_reference_name(&argument.node).map(Arc::<str>::from))
            .collect::<Option<Vec<_>>>()?;
        let enclosing = nearest_ancestor(index, key.node, |kind| {
            matches!(
                kind,
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::TypeDefinition
            )
        })?;
        let enclosing = index.node_at(program, enclosing)?;
        let enclosing_generics = if let Some(function) = enclosing.of::<beskid_analysis::syntax::FunctionDefinition>() {
            &function.generics
        } else {
            &enclosing.of::<beskid_analysis::syntax::TypeDefinition>()?.generics
        };
        if !parameter_arguments
            .iter()
            .all(|argument| enclosing_generics.iter().any(|generic| generic.node.name.as_str() == argument.as_ref()))
        {
            return None;
        }
        let parameters =
            function.generics.iter().map(|generic| Arc::<str>::from(generic.node.name.as_str())).collect::<Vec<_>>();
        Some(Ok(GenericCallTemplate {
            declaration,
            parameters: parameters.into(),
            parameter_arguments: parameter_arguments.into(),
        }))
    })?
    .transpose()
}

#[salsa::tracked(persist)]
pub(in crate::semantic_contract) fn generic_nominal_method_receiver_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<GenericNominalMethodReceiver> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let (method, receiver) = match &call.callee.node {
            beskid_analysis::syntax::Expression::Path(path) => {
                nominal_local_member_receiver(db, program, index, key, &path.node.path.node)?
            }
            beskid_analysis::syntax::Expression::Member(member) => {
                let method = method_declaration_for_member_receiver(db, program, index, key, call, member)?;
                let receiver = *call_arguments(db, key).ok()??.first()?;
                (method, receiver)
            }
            _ => return None,
        };
        let method_syntax = db.syntax_unit(method.unit)?;
        let owner_node =
            method_owner_node(method_syntax.expanded_program(db), method_syntax.syntax_index(db), method.node)?;
        let owner = AstNodeKey { node: owner_node, ..method };
        if let Some(handle) = inferred_spawn_handle(db, receiver) {
            return Some(Ok(GenericNominalMethodReceiver {
                method,
                receiver,
                owner,
                substitutions: Arc::from([handle.payload]),
            }));
        }
        let owner_definition = method_syntax
            .syntax_index(db)
            .node_at(method_syntax.expanded_program(db), owner_node)?
            .of::<beskid_analysis::syntax::TypeDefinition>()?;
        (!owner_definition.generics.is_empty()).then_some(())?;
        if index.kind(receiver.node) != Some(beskid_analysis::syntax_query::NodeKind::Identifier) {
            return Some(generic_source_expression_identity(db, receiver).and_then(|identity| {
                let GenericSourceTypeIdentity::Nominal { arguments, .. } = identity else {
                    return Err(SemanticError::unavailable("generic_nominal_method_receiver"));
                };
                if arguments.len() != owner_definition.generics.len() {
                    return Err(SemanticError::unavailable("generic_nominal_method_receiver"));
                }
                let substitutions = owner_definition
                    .generics
                    .iter()
                    .zip(arguments.iter())
                    .map(|(generic, argument)| {
                        GenericSubstitution::from_source(
                            generic.node.name.as_str(),
                            argument.abi_type(),
                            argument.clone(),
                        )
                    })
                    .collect::<Vec<_>>();
                Ok(GenericNominalMethodReceiver { method, receiver, owner, substitutions: substitutions.into() })
            }));
        }
        let receiver_parent = parent_node(index, receiver.node)?;
        let annotation = match index.kind(receiver_parent)? {
            beskid_analysis::syntax_query::NodeKind::Parameter => index
                .node_at(program, receiver_parent)?
                .of::<beskid_analysis::syntax::Parameter>()
                .map(|parameter| &parameter.ty.node),
            beskid_analysis::syntax_query::NodeKind::LetStatement => index
                .node_at(program, receiver_parent)?
                .of::<beskid_analysis::syntax::LetStatement>()
                .and_then(|statement| statement.type_annotation.as_ref())
                .map(|annotation| &annotation.node),
            _ => None,
        }?;
        let beskid_analysis::syntax::Type::Complex(applied) = annotation else {
            return None;
        };
        (resolve_type_declaration(db, key, &applied.node) == Some(owner)).then_some(())?;
        let terminal = applied.node.segments.last()?;
        (terminal.node.type_args.len() == owner_definition.generics.len()).then_some(())?;
        let substitutions = owner_definition
            .generics
            .iter()
            .zip(terminal.node.type_args.iter())
            .map(|(generic, argument)| {
                let abi = abi_type_from_syntax(db, key, &argument.node)?;
                let source_identity = generic_source_type_identity(db, key, &argument.node)?;
                Ok(GenericSubstitution::from_source(generic.node.name.as_str(), abi, source_identity))
            })
            .collect::<Result<Vec<_>, _>>();
        Some(substitutions.map(|substitutions| GenericNominalMethodReceiver {
            method,
            receiver,
            owner,
            substitutions: substitutions.into(),
        }))
    })?
    .transpose()
}
