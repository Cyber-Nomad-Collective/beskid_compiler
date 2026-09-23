//! Generic ABI types, generic declaration instances, and declaration ABI resolution.

use super::super::super::layouts::unique_assembled_type_in_module;
use super::super::super::*;
use super::*;

pub(in crate::semantic_contract) fn generic_abi_type(
    db: &dyn Db,
    declaration: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
    substitutions: &HashMap<String, SemanticTypeId>,
) -> Result<SemanticTypeId, SemanticError> {
    if let beskid_analysis::syntax::Type::Complex(path) = syntax_type
        && path.node.segments.iter().any(|segment| {
            segment.node.type_args.iter().any(|argument| {
                type_syntax_mentions_generic_parameter(&argument.node, "T")
                    || substitutions.keys().any(|name| type_syntax_mentions_generic_parameter(&argument.node, name))
            })
        })
    {
        return Ok(SemanticTypeId::POINTER);
    }
    let generic = match syntax_type {
        beskid_analysis::syntax::Type::Complex(path) => {
            let [segment] = path.node.segments.as_slice() else {
                return abi_type_from_syntax(db, declaration, syntax_type);
            };
            segment.node.type_args.is_empty().then_some(segment.node.name.node.name.as_str())
        }
        _ => None,
    };
    generic
        .and_then(|name| substitutions.get(name).copied())
        .map(Ok)
        .unwrap_or_else(|| abi_type_from_syntax(db, declaration, syntax_type))
}

pub(in crate::semantic_contract) fn generic_type_name<'a>(
    syntax_type: &'a beskid_analysis::syntax::Type,
    generics: &[&str],
) -> Option<&'a str> {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return None;
    };
    let [segment] = path.node.segments.as_slice() else {
        return None;
    };
    let name = segment.node.name.node.name.as_str();
    segment.node.type_args.is_empty().then_some(name).filter(|name| generics.contains(name))
}

pub(in crate::semantic_contract) fn type_syntax_mentions_generic_parameter(
    syntax_type: &beskid_analysis::syntax::Type,
    parameter: &str,
) -> bool {
    match syntax_type {
        beskid_analysis::syntax::Type::Primitive(_) => false,
        beskid_analysis::syntax::Type::Complex(path) => path.node.segments.iter().any(|segment| {
            segment.node.name.node.name == parameter
                || segment
                    .node
                    .type_args
                    .iter()
                    .any(|argument| type_syntax_mentions_generic_parameter(&argument.node, parameter))
        }),
        beskid_analysis::syntax::Type::Associated { contract, .. } => contract.node.segments.iter().any(|segment| {
            segment.node.name.node.name == parameter
                || segment
                    .node
                    .type_args
                    .iter()
                    .any(|argument| type_syntax_mentions_generic_parameter(&argument.node, parameter))
        }),
        beskid_analysis::syntax::Type::Array(element) => {
            type_syntax_mentions_generic_parameter(&element.node, parameter)
        }
        beskid_analysis::syntax::Type::Function { return_type, parameters } => {
            type_syntax_mentions_generic_parameter(&return_type.node, parameter)
                || parameters
                    .iter()
                    .any(|parameter_type| type_syntax_mentions_generic_parameter(&parameter_type.node, parameter))
        }
        // `This` is substituted with the conforming type's own identity before a call reaches
        // ABI lowering (direct-impl sites, `beskid_analysis`'s typechecker); a bounded generic
        // `This` at a monomorphized call site is deferred to a later slice.
        beskid_analysis::syntax::Type::This => false,
    }
}

pub(in crate::semantic_contract) fn generic_parameter_reference_name(
    syntax_type: &beskid_analysis::syntax::Type,
) -> Option<&str> {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return None;
    };
    let [segment] = path.node.segments.as_slice() else {
        return None;
    };
    segment.node.type_args.is_empty().then_some(segment.node.name.node.name.as_str())
}

/// Materialize a generic declaration with an already-proven immutable environment.
///
/// The caller must obtain `substitutions` from a source call fact or an enclosing instance;
/// this function checks arity and derives the ABI directly from the declaration syntax.
pub fn generic_specialization_instance(
    db: &dyn Db,
    declaration: AstNodeKey,
    substitutions: Arc<[GenericSubstitution]>,
) -> SemanticQueryResult<GenericSpecializationInstance> {
    let Some(syntax) = db.syntax_unit(declaration.unit) else { return Ok(None) };
    if !syntax.accepts_key(db, declaration) {
        return Ok(None);
    }
    let Some(function) = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), declaration.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::FunctionDefinition>())
    else {
        return Ok(None);
    };
    if substitutions.len() > function.generics.len() {
        return Ok(None);
    }
    let mut environment = HashMap::with_capacity(substitutions.len());
    for binding in substitutions.iter() {
        if !function.generics.iter().any(|generic| generic.node.name.as_str() == binding.parameter.as_ref())
            || environment.insert(binding.parameter.to_string(), binding.argument).is_some()
        {
            return Ok(None);
        }
    }
    let parameters = function
        .parameters
        .iter()
        .map(|parameter| generic_abi_type(db, declaration, &parameter.node.ty.node, &environment))
        .collect::<Result<Vec<_>, _>>()?;
    let result = function.return_type.as_ref().map_or(Ok(SemanticTypeId::UNIT), |return_type| {
        generic_abi_type(db, declaration, &return_type.node, &environment)
    })?;
    Ok(Some(GenericSpecializationInstance {
        declaration,
        declaration_identity: stable_declaration_identity(db, declaration)
            .ok_or_else(|| SemanticError::unavailable("generic_specialization_identity"))?,
        signature: ItemSignature { parameters: parameters.into(), result },
        substitutions,
        contract_witnesses: Arc::from([]),
    }))
}

pub(in crate::semantic_contract) fn abi_signature_from_syntax(
    db: &dyn Db,
    key: AstNodeKey,
    parameters: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>],
    return_type: Option<&beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>>,
) -> Result<ItemSignature, SemanticError> {
    let parameters = parameters
        .iter()
        .map(|parameter| item_abi_type_from_syntax(db, key, &parameter.node.ty.node))
        .collect::<Result<Vec<_>, _>>()?;
    let result = return_type
        .map_or(Ok(SemanticTypeId::UNIT), |return_type| item_abi_type_from_syntax(db, key, &return_type.node))?;
    Ok(ItemSignature { parameters: parameters.into(), result })
}

/// Resolve one declaration ABI type without broadening ordinary source lookup.
///
/// A fully qualified nominal envelope can appear in a public signature without a matching `use`
/// declaration (`Console.ConsoleSize` or
/// `Core.Results.Result<i64, Core.Syscall.SyscallError>`). Its outer declaration is nevertheless
/// exact assembly authority, and ABI v5 passes every nominal aggregate by pointer regardless of
/// its payload arguments. Exact declaration lookup is shared with qualified enum facts, while this
/// pointer-ABI fallback stays item-local; completion and ABI-varying bare generics remain closed.
pub(in crate::semantic_contract) fn item_abi_type_from_syntax(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<SemanticTypeId, SemanticError> {
    abi_type_from_syntax(db, key, syntax_type)
        .or_else(|error| exact_assembled_nominal_envelope(db, key, syntax_type).ok_or(error))
}

pub(in crate::semantic_contract) fn exact_assembled_nominal_envelope(
    db: &dyn Db,
    key: AstNodeKey,
    syntax_type: &beskid_analysis::syntax::Type,
) -> Option<SemanticTypeId> {
    let beskid_analysis::syntax::Type::Complex(path) = syntax_type else {
        return None;
    };
    let (nominal, module_path) = path.node.segments.split_last()?;
    if module_path.is_empty() || module_path.iter().any(|segment| !segment.node.type_args.is_empty()) {
        return None;
    }
    let module_path = module_path.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>();
    unique_assembled_type_in_module(db, key, &module_path, &nominal.node.name.node.name, nominal.node.type_args.len())?;
    Some(SemanticTypeId::POINTER)
}
