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
        let (declaration, parameters, owner_arguments) =
            non_generic_callee_parameters(db, program, index, call_key, &callee.node.path.node)?;
        if parameters.len() != call.args.len() {
            return None;
        }
        let expected = &parameters.get(argument_index)?.node.ty.node;
        // A method of a generic owner (`Send(T value)` on a `Port<Item<T>> output`) has its owner
        // parameters applied from the receiver's written type. Every other name in the declared
        // type must denote the same declaration at the call as at the method.
        if !owner_arguments.is_empty() {
            if !declared_names_denote_the_same_types(db, declaration, expected, call_key, &owner_arguments) {
                return None;
            }
            let substitutions = owner_arguments.iter().map(|(name, argument)| (name.as_str(), argument)).collect();
            return substitute_explicit_type(expected, &substitutions);
        }
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

type OwnerArguments = Vec<(String, beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>)>;

/// Declared parameters of a non-generic direct callee: a module function, or a method called on
/// an explicitly typed nominal receiver (`holder.Pick(...)`). Both resolve from syntax alone, so
/// contextual argument typing never consults call lowering. A method of a generic owner also
/// returns the owner's parameter names applied to the receiver's written type arguments; a
/// receiver whose type is not written (a field chain, an inferred local) has none and is declined.
fn non_generic_callee_parameters(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    call_key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<(AstNodeKey, Vec<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>>, OwnerArguments)> {
    if let Some(declaration) = resolve_item_declaration(db, program, index, call_key, path) {
        let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
        let function = syntax
            .syntax_index(db)
            .node_at(syntax.expanded_program(db), declaration.node)?
            .of::<beskid_analysis::syntax::FunctionDefinition>()?;
        return function.generics.is_empty().then(|| (declaration, function.parameters.clone(), Vec::new()));
    }
    let (method, receiver) =
        super::super::resolution::nominal_local_member_receiver(db, program, index, call_key, path)?;
    let syntax = db.syntax_unit(method.unit).filter(|syntax| syntax.accepts_key(db, method))?;
    let definition = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), method.node)?
        .of::<beskid_analysis::syntax::MethodDefinition>()?;
    if !definition.generics.is_empty() {
        return None;
    }
    let owner = crate::semantic_contract::locals::method_owner_declaration(db, method);
    let owner_syntax = owner.and_then(|owner| db.syntax_unit(owner.unit).filter(|syntax| syntax.accepts_key(db, owner)));
    let owner_generics = owner
        .zip(owner_syntax)
        .and_then(|(owner, syntax)| {
            syntax
                .syntax_index(db)
                .node_at(syntax.expanded_program(db), owner.node)?
                .of::<beskid_analysis::syntax::TypeDefinition>()
                .map(|definition| definition.generics.clone())
        })
        .unwrap_or_default();
    if owner_generics.is_empty() {
        return Some((method, definition.parameters.clone(), Vec::new()));
    }
    (receiver.unit == call_key.unit).then_some(())?;
    let written = crate::semantic_contract::layouts::explicit_local_complex_type_path(program, index, receiver.node)?;
    let arguments = &written.segments.last()?.node.type_args;
    (arguments.len() == owner_generics.len()).then_some(())?;
    let owner_arguments = owner_generics
        .iter()
        .zip(arguments.iter())
        .map(|(generic, argument)| (generic.node.name.to_string(), argument.clone()))
        .collect();
    Some((method, definition.parameters.clone(), owner_arguments))
}

/// Whether every type name in `declared` (read at the method `declaration`) other than a bare
/// owner parameter resolves to the same declaration at `call`, so substituting the owner's
/// applied arguments yields syntax that denotes the declared type at the call site.
fn declared_names_denote_the_same_types(
    db: &dyn Db,
    declaration: AstNodeKey,
    declared: &beskid_analysis::syntax::Type,
    call: AstNodeKey,
    owner_arguments: &OwnerArguments,
) -> bool {
    use beskid_analysis::syntax::Type;

    match declared {
        Type::Primitive(_) => true,
        Type::Complex(path) => {
            if let [segment] = path.node.segments.as_slice()
                && segment.node.type_args.is_empty()
                && owner_arguments.iter().any(|(name, _)| *name == segment.node.name.node.name)
            {
                return true;
            }
            let (Some(at_declaration), Some(at_call)) =
                (resolve_type_declaration(db, declaration, &path.node), resolve_type_declaration(db, call, &path.node))
            else {
                return false;
            };
            at_declaration == at_call
                && path.node.segments.iter().flat_map(|segment| segment.node.type_args.iter()).all(|argument| {
                    declared_names_denote_the_same_types(db, declaration, &argument.node, call, owner_arguments)
                })
        }
        Type::Array(element) => declared_names_denote_the_same_types(db, declaration, &element.node, call, owner_arguments),
        Type::Function { return_type, parameters } => {
            declared_names_denote_the_same_types(db, declaration, &return_type.node, call, owner_arguments)
                && parameters.iter().all(|parameter| {
                    declared_names_denote_the_same_types(db, declaration, &parameter.node, call, owner_arguments)
                })
        }
        Type::Associated { .. } | Type::This => false,
    }
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
