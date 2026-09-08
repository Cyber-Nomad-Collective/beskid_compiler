//! Focused call-semantics implementation.

use super::super::*;

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
            CallLowering::Dynamic | CallLowering::Runtime(_) | CallLowering::CorelibService(_) => {
                return None;
            }
        };
        generic_callable_parameters(db, declaration)?;
        let instance = match generic_specialization_instance_for_call(db, key) {
            Ok(instance) => instance,
            Err(error) => return Some(Err(error)),
        };
        Some(Ok(GenericCallSpecialization {
            declaration: instance.declaration,
            signature: instance.signature,
            substitutions: instance.substitutions,
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
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        let (method, receiver) = nominal_local_member_receiver(db, program, index, key, &path.node.path.node)?;
        let method_syntax = db.syntax_unit(method.unit)?;
        let owner_node = parent_node(method_syntax.syntax_index(db), method.node)?;
        let owner = AstNodeKey { node: owner_node, ..method };
        let owner_definition = method_syntax
            .syntax_index(db)
            .node_at(method_syntax.expanded_program(db), owner_node)?
            .of::<beskid_analysis::syntax::TypeDefinition>()?;
        (!owner_definition.generics.is_empty()).then_some(())?;
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

/// Preserve the canonical source identity of one concrete generic argument independently of its
/// ABI representation. Nominal, array, and function types can all share the pointer ABI, so the
/// specialization key must retain their recursive source shape before ABI lowering.
pub(in crate::semantic_contract) fn generic_source_type_identity(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    use beskid_analysis::syntax::Type;

    Ok(match syntax_type {
        Type::Primitive(_) => GenericSourceTypeIdentity::Abi(abi_type_from_syntax(db, key, syntax_type)?),
        Type::Complex(path) => generic_source_path_identity(db, key, &path.node)?,
        Type::Array(element) => {
            GenericSourceTypeIdentity::Array(Box::new(generic_source_type_identity(db, key, &element.node)?))
        }
        Type::Function { return_type, parameters } => GenericSourceTypeIdentity::Function {
            parameters: parameters
                .iter()
                .map(|parameter| generic_source_type_identity(db, key, &parameter.node))
                .collect::<Result<Vec<_>, _>>()?
                .into(),
            result: Box::new(generic_source_type_identity(db, key, &return_type.node)?),
        },
        Type::Associated { .. } => return Err(SemanticError::unavailable("generic_source_type_identity")),
    })
}

fn generic_source_path_identity(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let declaration = resolve_type_declaration(db, key, path)
        .ok_or_else(|| SemanticError::unavailable("generic_source_type_identity"))?;
    let qualified_name = stable_declaration_identity(db, declaration)
        .ok_or_else(|| SemanticError::unavailable("generic_source_type_identity"))?;
    let arguments = path
        .segments
        .iter()
        .flat_map(|segment| segment.node.type_args.iter())
        .map(|argument| generic_source_type_identity(db, key, &argument.node))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GenericSourceTypeIdentity::Nominal { qualified_name, arguments: arguments.into() })
}

/// Recover an expression's exact source type shape when current syntax proves it.
/// Pointer ABI alone is deliberately rejected because native pointers, arrays, closures,
/// records, and enums all share that representation.
pub(in crate::semantic_contract) fn generic_source_expression_identity(
    db: &dyn Db,
    key: AstNodeKey,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let syntax = db.syntax_unit(key.unit).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    if !syntax.accepts_key(db, key) {
        return Err(SemanticError::unavailable("source_expression_type"));
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let normalized = normalized_expression_node(index, key.node);
    let node =
        index.node_at(program, normalized).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let normalized_key = AstNodeKey { node: normalized, ..key };

    if let Some(literal) = node.of::<beskid_analysis::syntax::LiteralExpression>() {
        return Ok(GenericSourceTypeIdentity::Abi(semantic_type_for_literal(&literal.literal.node)));
    }
    if let Some(literal) = node.of::<beskid_analysis::syntax::StructLiteralExpression>() {
        return generic_source_path_identity(db, normalized_key, &literal.path.node);
    }
    if let Some(constructor) = node.of::<beskid_analysis::syntax::EnumConstructorExpression>() {
        let path = contextual_enum_constructor_type_path(db, program, index, normalized_key, constructor)
            .unwrap_or_else(|| constructor.path.node.type_path.node.clone());
        return generic_source_path_identity(db, normalized_key, &path);
    }
    if let Some(path) = node.of::<beskid_analysis::syntax::PathExpression>() {
        if let Ok(Some(access)) = aggregate_field_access(db, normalized_key) {
            return generic_source_aggregate_field_identity(db, &access);
        }
        if let [segment] = path.path.node.segments.as_slice()
            && segment.node.type_args.is_empty()
            && let Some(declaration) =
                resolve_lexical_declaration(program, index, normalized, segment.node.name.node.name.as_str())
        {
            if let Ok(identity) = generic_source_local_identity(db, program, index, normalized_key, declaration) {
                return Ok(identity);
            }
            if let Some(binding) = pattern_binding_fact(db, index, normalized_key, declaration) {
                return generic_source_field_shape_identity(db, binding?.payload);
            }
        }
    }
    if node.of::<beskid_analysis::syntax::CallExpression>().is_some()
        && let Some(CallLowering::Direct(declaration)) = call_lowering(db, normalized_key)?
    {
        return generic_source_callable_result_identity(db, normalized_key, declaration);
    }

    let abi = abi_type(db, normalized_key)?.ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    if abi == SemanticTypeId::POINTER {
        return Err(SemanticError::unavailable("source_expression_type"));
    }
    Ok(GenericSourceTypeIdentity::Abi(abi))
}

fn generic_source_aggregate_field_identity(
    db: &dyn Db,
    access: &AggregateFieldAccess,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let syntax = db
        .syntax_unit(access.declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, access.declaration))
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), access.declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let index = usize::try_from(access.index).map_err(|_| SemanticError::unavailable("source_expression_type"))?;
    let field = definition.fields.get(index).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    if generic_parameter_reference_name(&field.node.ty.node).is_none() {
        return generic_source_type_identity(db, access.declaration, &field.node.ty.node);
    }
    let (_, shape) =
        access.layout.fields.get(index).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    generic_source_field_shape_identity(db, *shape)
}

fn generic_source_field_shape_identity(
    db: &dyn Db,
    shape: AggregateFieldShape,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    match shape {
        AggregateFieldShape::Scalar(semantic) if semantic != SemanticTypeId::POINTER => {
            Ok(GenericSourceTypeIdentity::Abi(semantic))
        }
        AggregateFieldShape::Nominal(declaration) => {
            let qualified_name = stable_declaration_identity(db, declaration)
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            Ok(GenericSourceTypeIdentity::Nominal { qualified_name, arguments: Arc::from([]) })
        }
        AggregateFieldShape::Scalar(_) => Err(SemanticError::unavailable("source_expression_type")),
    }
}

fn generic_source_callable_result_identity(
    db: &dyn Db,
    call: AstNodeKey,
    declaration: AstNodeKey,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let syntax = db
        .syntax_unit(declaration.unit)
        .filter(|syntax| syntax.accepts_key(db, declaration))
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let node = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let result = node
        .of::<beskid_analysis::syntax::FunctionDefinition>()
        .and_then(|function| function.return_type.as_ref())
        .or_else(|| {
            node.of::<beskid_analysis::syntax::MethodDefinition>().and_then(|method| method.return_type.as_ref())
        })
        .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    let specialization = generic_specialization_instance_for_call(db, call)?;
    let substitutions = specialization
        .substitutions
        .iter()
        .map(|binding| (binding.parameter.as_ref(), binding.source_identity()))
        .collect::<HashMap<_, _>>();
    generic_source_type_identity_with_substitutions(db, declaration, &result.node, &substitutions)
}

pub(in crate::semantic_contract) fn generic_source_type_identity_with_substitutions(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
    substitutions: &HashMap<&str, &GenericSourceTypeIdentity>,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    use beskid_analysis::syntax::Type;

    if let Some(parameter) = generic_parameter_reference_name(syntax_type)
        && let Some(identity) = substitutions.get(parameter)
    {
        return Ok((*identity).clone());
    }
    Ok(match syntax_type {
        Type::Primitive(_) => GenericSourceTypeIdentity::Abi(abi_type_from_syntax(db, key, syntax_type)?),
        Type::Complex(path) => {
            let declaration = resolve_type_declaration(db, key, &path.node)
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            let qualified_name = stable_declaration_identity(db, declaration)
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            let arguments = path
                .node
                .segments
                .iter()
                .flat_map(|segment| segment.node.type_args.iter())
                .map(|argument| generic_source_type_identity_with_substitutions(db, key, &argument.node, substitutions))
                .collect::<Result<Vec<_>, _>>()?;
            GenericSourceTypeIdentity::Nominal { qualified_name, arguments: arguments.into() }
        }
        Type::Array(element) => GenericSourceTypeIdentity::Array(Box::new(
            generic_source_type_identity_with_substitutions(db, key, &element.node, substitutions)?,
        )),
        Type::Function { return_type, parameters } => GenericSourceTypeIdentity::Function {
            parameters: parameters
                .iter()
                .map(|parameter| {
                    generic_source_type_identity_with_substitutions(db, key, &parameter.node, substitutions)
                })
                .collect::<Result<Vec<_>, _>>()?
                .into(),
            result: Box::new(generic_source_type_identity_with_substitutions(
                db,
                key,
                &return_type.node,
                substitutions,
            )?),
        },
        Type::Associated { .. } => return Err(SemanticError::unavailable("source_expression_type")),
    })
}

fn generic_source_local_identity(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    declaration: beskid_analysis::syntax::AstNodeId,
) -> Result<GenericSourceTypeIdentity, SemanticError> {
    let parent = parent_node(index, declaration).ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
    match index.kind(parent) {
        Some(beskid_analysis::syntax_query::NodeKind::Parameter) => index
            .node_at(program, parent)
            .and_then(|node| node.of::<beskid_analysis::syntax::Parameter>())
            .ok_or_else(|| SemanticError::unavailable("source_expression_type"))
            .and_then(|parameter| generic_source_type_identity(db, key, &parameter.ty.node)),
        Some(beskid_analysis::syntax_query::NodeKind::LetStatement) => {
            let statement = index
                .node_at(program, parent)
                .and_then(|node| node.of::<beskid_analysis::syntax::LetStatement>())
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            let annotation = statement
                .type_annotation
                .as_ref()
                .ok_or_else(|| SemanticError::unavailable("source_expression_type"))?;
            generic_source_type_identity(db, key, &annotation.node)
        }
        _ => Err(SemanticError::unavailable("source_expression_type")),
    }
}

/// Deterministic assembly-relative identity for a resolved nominal declaration.
///
/// The assembled module registry is the authority in production. Inline module names and the
/// declaration name complete the path without leaking an absolute checkout root into symbols.
pub(in crate::semantic_contract) fn stable_declaration_identity(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Option<Arc<str>> {
    let syntax = db.syntax_unit(declaration.unit)?;
    if !syntax.accepts_key(db, declaration) {
        return None;
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let declaration_node = index.node_at(program, declaration.node)?;
    let declaration_name = declaration_node
        .of::<beskid_analysis::syntax::TypeDefinition>()
        .map(|definition| definition.name.node.name.as_str())
        .or_else(|| {
            declaration_node
                .of::<beskid_analysis::syntax::EnumDefinition>()
                .map(|definition| definition.name.node.name.as_str())
        })
        .or_else(|| {
            declaration_node
                .of::<beskid_analysis::syntax::FunctionDefinition>()
                .map(|definition| definition.name.node.name.as_str())
        })
        .or_else(|| {
            declaration_node
                .of::<beskid_analysis::syntax::MethodDefinition>()
                .map(|definition| definition.name.node.name.as_str())
        })?;
    let mut path = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .modules
        .iter()
        .filter(|((generation, _), units)| *generation == declaration.generation && units.contains(&declaration.unit))
        .map(|((_, module_path), _)| module_path.clone())
        .min()
        .unwrap_or_default();
    let mut inline = Vec::new();
    let mut parent = parent_node(index, declaration.node);
    while let Some(node) = parent {
        if let Some(parent) = index.node_at(program, node) {
            if let Some(module) = parent.of::<beskid_analysis::syntax::InlineModule>() {
                inline.push(module.name.node.name.clone());
            } else if let Some(owner) = parent.of::<beskid_analysis::syntax::TypeDefinition>() {
                inline.push(owner.name.node.name.clone());
            }
        }
        parent = parent_node(index, node);
    }
    inline.reverse();
    path.extend(inline);
    path.push(declaration_name.to_owned());
    Some(Arc::from(path.join("::")))
}

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

/// Instantiate the declared parameter type for the explicit call argument containing `key`.
///
/// This is the source-type authority for contextual expression lowering. It deliberately uses
/// only the call and declaration syntax: consulting ABI specialization here would both erase
/// nominal arguments to pointers and create a query cycle through argument typing.
pub(in crate::semantic_contract) fn expected_explicit_call_argument_type(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
) -> Option<beskid_analysis::syntax::Type> {
    let mut argument_root = key.node;
    let call_node = loop {
        let parent = parent_node(index, argument_root)?;
        match index.kind(parent)? {
            beskid_analysis::syntax_query::NodeKind::Expression
            | beskid_analysis::syntax_query::NodeKind::Statement => argument_root = parent,
            beskid_analysis::syntax_query::NodeKind::CallExpression => break parent,
            _ => return None,
        }
    };
    let call = index.node_at(program, call_node)?.of::<beskid_analysis::syntax::CallExpression>()?;
    let argument_index = call.args.iter().position(|argument| {
        index.direct_child_id(program, call_node, beskid_analysis::syntax_query::DynNodeRef::from(argument))
            == Some(argument_root)
    })?;
    let beskid_analysis::syntax::Expression::Path(callee) = &call.callee.node else {
        return None;
    };
    let type_arguments = explicit_generic_type_argument_syntax(&callee.node.path.node)?;
    let call_key = AstNodeKey { node: call_node, ..key };
    let instantiation = generic_call_instantiation_for_node(db, program, index, call_key, &callee.node.path.node)?;
    if usize::from(instantiation.argument_count) != type_arguments.len()
        || instantiation.arguments.len() != type_arguments.len()
    {
        return None;
    }
    let declaration_syntax = db.syntax_unit(instantiation.declaration.unit)?;
    if !declaration_syntax.accepts_key(db, instantiation.declaration) {
        return None;
    }
    let function = declaration_syntax
        .syntax_index(db)
        .node_at(declaration_syntax.expanded_program(db), instantiation.declaration.node)?
        .of::<beskid_analysis::syntax::FunctionDefinition>()?;
    if function.generics.len() != type_arguments.len() || function.parameters.len() != call.args.len() {
        return None;
    }
    let substitutions = function
        .generics
        .iter()
        .zip(type_arguments)
        .map(|(parameter, argument)| (parameter.node.name.as_str(), argument))
        .collect::<HashMap<_, _>>();
    substitute_explicit_type(&function.parameters.get(argument_index)?.node.ty.node, &substitutions)
}

fn substitute_explicit_type(
    ty: &beskid_analysis::syntax::Type,
    substitutions: &HashMap<&str, &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>>,
) -> Option<beskid_analysis::syntax::Type> {
    use beskid_analysis::syntax::Type;

    if let Type::Complex(path) = ty
        && let [segment] = path.node.segments.as_slice()
        && segment.node.type_args.is_empty()
        && let Some(argument) = substitutions.get(segment.node.name.node.name.as_str())
    {
        return Some(argument.node.clone());
    }
    Some(match ty {
        Type::Primitive(primitive) => Type::Primitive(primitive.clone()),
        Type::Complex(path) => {
            let mut path = path.clone();
            for segment in &mut path.node.segments {
                for argument in &mut segment.node.type_args {
                    argument.node = substitute_explicit_type(&argument.node, substitutions)?;
                }
            }
            Type::Complex(path)
        }
        Type::Array(element) => {
            let mut element = element.clone();
            element.node = substitute_explicit_type(&element.node, substitutions)?;
            Type::Array(element)
        }
        Type::Function { return_type, parameters } => {
            let mut return_type = return_type.clone();
            return_type.node = substitute_explicit_type(&return_type.node, substitutions)?;
            let parameters = parameters
                .iter()
                .map(|parameter| {
                    let mut parameter = parameter.clone();
                    parameter.node = substitute_explicit_type(&parameter.node, substitutions)?;
                    Some(parameter)
                })
                .collect::<Option<Vec<_>>>()?;
            Type::Function { return_type, parameters }
        }
        Type::Associated { .. } => return None,
    })
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
    let Some(enclosing) =
        nearest_ancestor(index, key.node, |kind| kind == beskid_analysis::syntax_query::NodeKind::FunctionDefinition)
    else {
        return false;
    };
    index
        .node_at(syntax.expanded_program(db), enclosing)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
        .is_some_and(|function| function.generics.iter().any(|generic| generic.node.name == parameter_name))
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
    let Some(function) = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
    else {
        return false;
    };
    if function.generics.len() != type_arguments.len() {
        return false;
    }
    type_arguments.iter().zip(function.generics.iter()).all(|(argument, generic)| {
        abi_type_from_syntax(db, key, &argument.node).is_ok()
            || type_syntax_is_generic_parameter_reference(&argument.node, generic.node.name.as_str())
            || type_syntax_is_enclosing_generic_parameter_reference(db, key, &argument.node)
    })
}

/// A two-segment imported nominal call can spell either a module member or a static member on a
/// generic nominal type.  The latter has no concrete receiver ABI until the source supplies the
/// receiver arguments (`Hub<i64>.Create()`), so it must not be treated as the imported module's
/// direct function.  Terminal method arguments remain independently valid (`Hub.Create<i64>()`).
pub(in crate::semantic_contract) fn imported_generic_nominal_receiver_requires_instantiation(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let [receiver, method] = path.segments.as_slice() else {
        return false;
    };
    if !receiver.node.type_args.is_empty() || !method.node.type_args.is_empty() {
        return false;
    }
    let receiver_name = receiver.node.name.node.name.as_str();
    let targets = db
        .syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))
        .into_iter()
        .flatten()
        .filter(|import| import.binding == receiver_name)
        .map(|import| import.target)
        .collect::<Vec<_>>();
    let [target] = targets.as_slice() else {
        return false;
    };
    exported_generic_type_named(db, *target, key.generation, receiver_name)
}

pub(in crate::semantic_contract) fn exported_generic_type_named(
    db: &dyn Db,
    unit: SourceUnitId,
    generation: SyntaxGenerationId,
    name: &str,
) -> bool {
    let mut pending = vec![unit];
    let mut visited = std::collections::HashSet::new();
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        let Some(syntax) = db.syntax_unit(current) else {
            continue;
        };
        if syntax.generation(db) != generation {
            continue;
        }
        if syntax.syntax_index(db).ids_of_kind(beskid_analysis::syntax_query::NodeKind::TypeDefinition).any(
            |candidate| {
                syntax
                    .syntax_index(db)
                    .node_at(syntax.expanded_program(db), candidate)
                    .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())
                    .is_some_and(|definition| definition.name.node.name == name && !definition.generics.is_empty())
            },
        ) {
            return true;
        }
        pending.extend(public_reexport_units(db, current, generation));
    }
    false
}

pub(in crate::semantic_contract) fn generic_call_instantiation_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<GenericCallInstantiation> {
    let argument_syntax = explicit_generic_type_argument_syntax(path)?;
    let argument_count = u8::try_from(argument_syntax.len()).ok()?;
    (argument_count > 0).then_some(())?;
    let declaration = resolve_item_declaration_candidate(db, program, index, key, path)?;
    let syntax = db.syntax_unit(declaration.unit)?;
    syntax.accepts_key(db, declaration).then_some(())?;
    let function = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)?
        .of::<beskid_analysis::syntax::FunctionDefinition>()?;
    (function.generics.len() == usize::from(argument_count)).then_some(())?;
    let mut concrete_arguments = Vec::with_capacity(argument_syntax.len());
    for (argument, generic) in argument_syntax.iter().zip(function.generics.iter()) {
        match abi_type_from_syntax(db, key, &argument.node) {
            Ok(concrete) => concrete_arguments.push(concrete),
            Err(_) if type_syntax_is_generic_parameter_reference(&argument.node, generic.node.name.as_str()) => {}
            Err(_) => return None,
        }
    }
    Some(GenericCallInstantiation { declaration, argument_count, arguments: concrete_arguments.into() })
}

pub(in crate::semantic_contract) fn function_declares_generics(db: &dyn Db, declaration: AstNodeKey) -> bool {
    matches!(generic_callable_parameters(db, declaration), Some((parameters, false)) if !parameters.is_empty())
}

/// Return declaration-ordered generic names and whether the callable has an implicit receiver.
/// Generic methods inherit their parameters from exactly one owning generic type definition.
pub(in crate::semantic_contract) fn generic_callable_parameters(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Option<(Vec<&str>, bool)> {
    let syntax = db.syntax_unit(declaration.unit)?;
    if !syntax.accepts_key(db, declaration) {
        return None;
    }
    let node = syntax.syntax_index(db).node_at(syntax.expanded_program(db), declaration.node)?;
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return (!function.generics.is_empty())
            .then(|| (function.generics.iter().map(|generic| generic.node.name.as_str()).collect(), false));
    }
    node.of::<beskid_analysis::syntax::MethodDefinition>()?;
    let owner = parent_node(syntax.syntax_index(db), declaration.node)
        .and_then(|parent| syntax.syntax_index(db).node_at(syntax.expanded_program(db), parent))
        .and_then(|node| node.of::<beskid_analysis::syntax::TypeDefinition>())?;
    (!owner.generics.is_empty())
        .then(|| (owner.generics.iter().map(|generic| generic.node.name.as_str()).collect(), true))
}

/// Whether a qualified call's receiver is an exact current import target.
/// Imported type/module member calls have no direct item edge; unknown qualified calls remain
/// unavailable instead of being guessed.
pub(in crate::semantic_contract) fn imported_call_receiver_exists(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> bool {
    let Some((_member, receiver)) = path.segments.split_last() else {
        return false;
    };
    if receiver.is_empty() {
        return false;
    }
    let receiver = receiver.iter().map(|segment| segment.node.name.node.name.as_str()).collect::<Vec<_>>();
    db.syntax_dependency_registry()
        .lock()
        .expect("syntax dependency registry")
        .imports
        .get(&(key.unit, key.generation))
        .is_some_and(|imports| {
            imports
                .iter()
                .filter(|import| {
                    (receiver.len() == 1 && import.binding == receiver[0])
                        || (import.path.len() >= receiver.len()
                            && import.path[import.path.len() - receiver.len()..]
                                .iter()
                                .map(String::as_str)
                                .eq(receiver.iter().copied()))
                })
                .take(2)
                .count()
                == 1
        })
}

pub(in crate::semantic_contract) fn expression_is_lambda(expression: &beskid_analysis::syntax::Expression) -> bool {
    match expression {
        beskid_analysis::syntax::Expression::Lambda(_) => true,
        beskid_analysis::syntax::Expression::Grouped(grouped) => expression_is_lambda(&grouped.node.expr.node),
        _ => false,
    }
}
