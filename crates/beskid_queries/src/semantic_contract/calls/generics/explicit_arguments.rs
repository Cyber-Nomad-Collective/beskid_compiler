//! Explicit generic type arguments, expected call argument types, and substitution.

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

/// Prove the declared parameter type for the call argument containing `key`.
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
    let call_key = AstNodeKey { node: call_node, ..key };
    let Some(type_arguments) = explicit_generic_type_argument_syntax(&callee.node.path.node) else {
        if callee.node.path.node.segments.iter().any(|segment| !segment.node.type_args.is_empty()) {
            return None;
        }
        let declaration = resolve_item_declaration(db, program, index, call_key, &callee.node.path.node)?;
        let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
        let function = syntax
            .syntax_index(db)
            .node_at(syntax.expanded_program(db), declaration.node)?
            .of::<beskid_analysis::syntax::FunctionDefinition>()?;
        if !function.generics.is_empty() || function.parameters.len() != call.args.len() {
            return None;
        }
        let expected = &function.parameters.get(argument_index)?.node.ty.node;
        // Syntax returned to the caller must still denote the exact declared concrete type.
        // Reject unresolved parameters and shadowed/import-dependent spellings, never infer
        // their identity from an ABI pointer or from the constructor being contextualized.
        let declared = generic_source_type_identity(db, declaration, expected).ok()?;
        let contextual = generic_source_type_identity(db, call_key, expected).ok()?;
        return (declared == contextual).then(|| expected.clone());
    };
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

pub(in crate::semantic_contract) fn substitute_explicit_type(
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
        Type::This => Type::This,
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
    let Some(owner) = method_owner_node(program, index, enclosing)
        .and_then(|owner| index.node_at(program, owner))
        .and_then(|owner| owner.of::<beskid_analysis::syntax::TypeDefinition>())
    else {
        return false;
    };
    owner.generics.iter().any(|generic| generic.node.name == parameter_name)
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
