//! Omitted optional arguments, filled once the callee of every call is known.
//!
//! A call may omit trailing optional arguments ([`beskid_analysis::parameter_defaults`]). Method
//! calls need the receiver's type to find their callee, so the filling runs after the assembly is
//! registered and typed: [`omitted_argument_programs`] asks `call_lowering` for the callee of
//! every call that may omit arguments, and returns the unit programs with the omitted arguments
//! appended. `build_typed_program` runs it on a scratch registration of the parsed assembly and
//! registers only the filled programs, so every later query and lowering sees ordinary explicit
//! arguments. This is the
//! only filling mechanism; functions, methods, contract methods and functions in inline modules
//! all go through it.
//!
//! A method that implements a contract method and declares no optional parameter of its own
//! inherits the contract method's defaults: default values belong to the contract declaration
//! (an implementation that repeats a different one is E1612).

use std::collections::{HashMap, HashSet};

use super::*;
use beskid_analysis::parameter_defaults::{OmittedArgument, fill_omitted_arguments, omitted_arguments};
use beskid_analysis::projects::ProgramAssembly;
use beskid_analysis::syntax::{
    CallExpression, ContractMethodSignature, Expression, FunctionDefinition, ImplBlock, MethodDefinition, Program,
    Spanned, TypeDefinition,
};
use beskid_analysis::syntax_query::{NodeKind, Query};

/// The arguments the compiler passes for the trailing optional parameters of `declaration` (a
/// function, method or contract method), and its parameter count (without the receiver). `None`
/// when the callable has no valid optional parameters.
pub(crate) fn declared_omitted_arguments(
    db: &dyn Db,
    declaration: AstNodeKey,
) -> Option<(Vec<OmittedArgument>, usize)> {
    let syntax = db.syntax_unit(declaration.unit).filter(|syntax| syntax.accepts_key(db, declaration))?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let node = index.node_at(program, declaration.node)?;
    if let Some(function) = node.of::<FunctionDefinition>() {
        return Some((omitted_arguments(&function.parameters, &program.node)?, function.parameters.len()));
    }
    if let Some(signature) = node.of::<ContractMethodSignature>() {
        return Some((omitted_arguments(&signature.parameters, &program.node)?, signature.parameters.len()));
    }
    let method = node.of::<MethodDefinition>()?;
    if method.parameters.iter().any(|parameter| parameter.node.default.is_some()) {
        return Some((omitted_arguments(&method.parameters, &program.node)?, method.parameters.len()));
    }
    let inherited = implemented_contract_method(db, declaration, method)?;
    Some((inherited, method.parameters.len()))
}

/// The defaults of the contract method that `method` (declared in a conforming `type` or `impl`)
/// implements: same name and parameter count, declared by one of the owner's contracts.
fn implemented_contract_method(
    db: &dyn Db,
    declaration: AstNodeKey,
    method: &MethodDefinition,
) -> Option<Vec<OmittedArgument>> {
    let syntax = db.syntax_unit(declaration.unit)?;
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let mut owner = super::locals::parent_node(index, declaration.node);
    while let Some(current) = owner {
        let node = index.node_at(program, current)?;
        let conformances = if let Some(definition) = node.of::<TypeDefinition>() {
            Some(&definition.conformances)
        } else {
            node.of::<ImplBlock>().map(|implementation| &implementation.conformances)
        };
        if let Some(conformances) = conformances {
            let owner_key = AstNodeKey { node: current, ..declaration };
            for path in conformances {
                let Some(contract) = super::contracts::resolve_contract(db, owner_key, &path.node) else { continue };
                let Ok(methods) = super::contracts::contract_methods(db, contract, &mut HashSet::new()) else {
                    continue;
                };
                for signature_key in methods {
                    let Some(signature_syntax) = db.syntax_unit(signature_key.unit) else { continue };
                    let signature_program = signature_syntax.expanded_program(db);
                    let Some(signature) = signature_syntax
                        .syntax_index(db)
                        .node_at(signature_program, signature_key.node)
                        .and_then(|node| node.of::<ContractMethodSignature>())
                    else {
                        continue;
                    };
                    if signature.name.node.name == method.name.node.name
                        && signature.parameters.len() == method.parameters.len()
                    {
                        return omitted_arguments(&signature.parameters, &signature_program.node);
                    }
                }
            }
            return None;
        }
        owner = super::locals::parent_node(index, current);
    }
    None
}

/// For every callable name that declares an optional parameter, the largest parameter count
/// declared under that name (a contract method's name also covers the methods that implement it).
fn optional_callable_names(assembly: &ProgramAssembly) -> HashMap<String, usize> {
    let mut names = HashMap::<String, usize>::new();
    let mut record = |name: &str, parameters: &[Spanned<beskid_analysis::syntax::Parameter>]| {
        if parameters.iter().any(|parameter| parameter.node.default.is_some()) {
            let count = names.entry(name.to_string()).or_default();
            *count = (*count).max(parameters.len());
        }
    };
    for unit in assembly.units.iter() {
        let query = || Query::from(&unit.program.node);
        for function in query().of::<FunctionDefinition>() {
            record(&function.name.node.name, &function.parameters);
        }
        for method in query().of::<MethodDefinition>() {
            record(&method.name.node.name, &method.parameters);
        }
        for signature in query().of::<ContractMethodSignature>() {
            record(&signature.name.node.name, &signature.parameters);
        }
    }
    names
}

/// Whether some call of `assembly` may omit an optional argument: it names a callable with
/// optional parameters and passes fewer arguments than that name declares. A cheap syntax check
/// that decides whether the typed filling pass runs at all.
pub(crate) fn may_omit_arguments(assembly: &ProgramAssembly) -> bool {
    let names = optional_callable_names(assembly);
    !names.is_empty()
        && assembly.units.iter().any(|unit| {
            Query::from(&unit.program.node).of::<CallExpression>().any(|call| {
                callee_name(call).and_then(|name| names.get(name)).is_some_and(|count| call.args.len() < *count)
            })
        })
}

fn callee_name(call: &CallExpression) -> Option<&str> {
    match &call.callee.node {
        Expression::Path(path) => {
            path.node.path.node.segments.last().map(|segment| segment.node.name.node.name.as_str())
        }
        Expression::Member(member) => Some(member.node.member.node.name.as_str()),
        _ => None,
    }
}

/// The unit programs of `assembly` (registered in `db`) with every omitted optional argument
/// filled, or `None` when no call omits one.
pub(crate) fn omitted_argument_programs(
    db: &dyn Db,
    assembly: &ProgramAssembly,
) -> Result<Option<Vec<Spanned<Program>>>, SemanticError> {
    let names = optional_callable_names(assembly);
    if names.is_empty() {
        return Ok(None);
    }
    let mut programs = Vec::with_capacity(assembly.units.len());
    let mut changed = false;
    for unit in assembly.units.iter() {
        let unit_id = SourceUnitId::new(db, unit.path.clone());
        let Some(syntax) = db.syntax_unit(unit_id) else {
            programs.push(unit.program.clone());
            continue;
        };
        let program = syntax.expanded_program(db);
        let index = syntax.syntax_index(db);
        let mut fills = HashMap::new();
        for node in index.ids_of_kind(NodeKind::CallExpression) {
            let Some(reference) = index.node_at(program, node) else { continue };
            let Some(call) = reference.of::<CallExpression>() else { continue };
            if !callee_name(call).and_then(|name| names.get(name)).is_some_and(|count| call.args.len() < *count) {
                continue;
            }
            let key = AstNodeKey { unit: unit_id, generation: assembly.generation, node };
            // A call whose callee is not a declaration (dynamic, function value) or not resolved
            // keeps its arguments; checking reports it.
            let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, key) else { continue };
            let Some((omitted, parameter_count)) = declared_omitted_arguments(db, declaration) else { continue };
            let passed = call.args.len();
            let required = parameter_count - omitted.len();
            if passed < required || passed >= parameter_count {
                continue;
            }
            let Some(span) = reference.span() else { continue };
            fills.insert((span.start, span.end), omitted[passed - required..].to_vec());
        }
        if fills.is_empty() {
            programs.push(unit.program.clone());
        } else {
            changed = true;
            programs.push(fill_omitted_arguments(program, &unit.origin_path.display().to_string(), &fills));
        }
    }
    Ok(changed.then_some(programs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callee_names_cover_paths_and_members() {
        let program = beskid_analysis::services::parse_program("unit Run() { A.B.Say(1); box.Go(); }").unwrap();
        let names = Query::from(&program.node)
            .of::<CallExpression>()
            .filter_map(|call| callee_name(call).map(str::to_string))
            .collect::<Vec<_>>();
        assert_eq!(names, ["Say", "Go"]);
    }
}
