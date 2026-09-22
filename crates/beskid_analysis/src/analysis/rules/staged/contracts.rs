use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::resolve::Resolution;
use crate::syntax::Spanned;
use crate::syntax::{ContractNode, Node, Path, Program, Type};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// A contract method's parameter and return types, compared structurally (ignoring source
/// spans) so a return-type mismatch is caught, not just an arity mismatch.
///
/// This stage runs before type checking (`stage2_type_check`), so no `TypeId`s are available
/// yet; equality is over the declared syntax `Type` shape instead of the checker's
/// `TypeId`-based `FunctionSignature`. `This` substitution (a later slice) will need real
/// `TypeId` equality to see through `This`; until then this comparator treats `This` as an
/// ordinary, opaque type-position token like any other identifier.
#[derive(Debug, Clone)]
struct MethodSignature {
    parameter_types: Vec<Type>,
    return_type: Option<Type>,
}

impl MethodSignature {
    fn from_parameters_and_return(
        parameters: &[Spanned<crate::syntax::Parameter>],
        return_type: Option<&Spanned<Type>>,
    ) -> Self {
        Self {
            parameter_types: parameters.iter().map(|param| param.node.ty.node.clone()).collect(),
            return_type: return_type.map(|ty| ty.node.clone()),
        }
    }

    /// Replaces the contract's own generic parameter names (e.g. `T`) with the concrete type
    /// arguments supplied at the conformance site (`Box<i32>` -> `T` becomes `i32`), so a
    /// generic contract's expected signature is compared against the implementor's concrete
    /// one, not against the bare, unresolved generic-parameter name.
    fn substitute(&self, subst: &HashMap<String, Type>) -> Self {
        if subst.is_empty() {
            return self.clone();
        }
        Self {
            parameter_types: self.parameter_types.iter().map(|ty| substitute_type(ty, subst)).collect(),
            return_type: self.return_type.as_ref().map(|ty| substitute_type(ty, subst)),
        }
    }
}

/// Structural substitution of generic-parameter names in a declared syntax `Type`. Only the
/// contract's own generic parameters (single-segment, no-args `Type::Complex` paths named in
/// `subst`) are replaced; every other shape recurses.
fn substitute_type(ty: &Type, subst: &HashMap<String, Type>) -> Type {
    match ty {
        Type::Complex(path) => {
            if let [segment] = path.node.segments.as_slice()
                && segment.node.type_args.is_empty()
                && let Some(replacement) = subst.get(segment.node.name.node.name.as_str())
            {
                return replacement.clone();
            }
            let mut substituted_path = path.clone();
            for segment in &mut substituted_path.node.segments {
                for arg in &mut segment.node.type_args {
                    arg.node = substitute_type(&arg.node, subst);
                }
            }
            Type::Complex(substituted_path)
        }
        Type::Array(element) => {
            let mut element = element.clone();
            element.node = substitute_type(&element.node, subst);
            Type::Array(element)
        }
        Type::Function { return_type, parameters } => Type::Function {
            return_type: Box::new(Spanned::new(substitute_type(&return_type.node, subst), return_type.span)),
            parameters: parameters.iter().map(|p| Spanned::new(substitute_type(&p.node, subst), p.span)).collect(),
        },
        Type::Primitive(_) | Type::Associated { .. } => ty.clone(),
    }
}

impl PartialEq for MethodSignature {
    fn eq(&self, other: &Self) -> bool {
        self.parameter_types.len() == other.parameter_types.len()
            && self
                .parameter_types
                .iter()
                .zip(&other.parameter_types)
                .all(|(a, b)| types_structurally_equal(a, b))
            && match (&self.return_type, &other.return_type) {
                (Some(a), Some(b)) => types_structurally_equal(a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

impl fmt::Display for MethodSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(")?;
        for (i, param) in self.parameter_types.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", describe_type(param))?;
        }
        write!(f, ") -> {}", self.return_type.as_ref().map(describe_type).unwrap_or_else(|| "unit".to_string()))
    }
}

/// Structural equality over declared syntax types, ignoring spans (two identically-shaped
/// annotations at different source locations must compare equal).
fn types_structurally_equal(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Primitive(a), Type::Primitive(b)) => a.node == b.node,
        (Type::Complex(a), Type::Complex(b)) => paths_structurally_equal(&a.node, &b.node),
        (Type::Associated { contract: ac, name: an }, Type::Associated { contract: bc, name: bn }) => {
            paths_structurally_equal(&ac.node, &bc.node) && an.node.name == bn.node.name
        }
        (Type::Array(a), Type::Array(b)) => types_structurally_equal(&a.node, &b.node),
        (
            Type::Function { return_type: a_ret, parameters: a_params },
            Type::Function { return_type: b_ret, parameters: b_params },
        ) => {
            types_structurally_equal(&a_ret.node, &b_ret.node)
                && a_params.len() == b_params.len()
                && a_params.iter().zip(b_params).all(|(a, b)| types_structurally_equal(&a.node, &b.node))
        }
        _ => false,
    }
}

fn paths_structurally_equal(a: &crate::syntax::Path, b: &crate::syntax::Path) -> bool {
    a.segments.len() == b.segments.len()
        && a.segments.iter().zip(&b.segments).all(|(a, b)| {
            a.node.name.node.name == b.node.name.node.name
                && a.node.type_args.len() == b.node.type_args.len()
                && a.node.type_args.iter().zip(&b.node.type_args).all(|(a, b)| types_structurally_equal(&a.node, &b.node))
        })
}

fn describe_type(ty: &Type) -> String {
    match ty {
        Type::Primitive(primitive) => format!("{:?}", primitive.node),
        Type::Complex(path) => {
            path.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>().join(".")
        }
        Type::Associated { contract, name } => {
            let contract_name =
                contract.node.segments.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>().join(".");
            format!("{contract_name}::{}", name.node.name)
        }
        Type::Array(element) => format!("{}[]", describe_type(&element.node)),
        Type::Function { return_type, parameters } => {
            let params = parameters.iter().map(|p| describe_type(&p.node)).collect::<Vec<_>>().join(", ");
            format!("{}({params})", describe_type(&return_type.node))
        }
    }
}

impl SemanticPipelineRule {
    pub(super) fn stage6_contracts_and_methods(
        &self,
        ctx: &mut RuleContext,
        program: &Spanned<Program>,
        resolution: &Resolution,
    ) {
        let contracts = self.collect_contract_signatures(program);

        for (type_item_id, conformances) in &resolution.tables.type_conformances {
            let Some(type_name) = resolution.items.get(type_item_id.0).map(|item| item.name.clone()) else {
                continue;
            };
            for (contract_item_id, conformance_span) in conformances {
                let Some(contract_name) = resolution.items.get(contract_item_id.0).map(|item| item.name.clone()) else {
                    continue;
                };
                let Some(expected_methods) = contracts.get(&contract_name) else {
                    continue;
                };
                let generics = self.contract_generics(program, &contract_name);
                let expected_type_args = if generics.is_empty() {
                    Vec::new()
                } else {
                    self.conformance_type_args(program, &type_name, &contract_name)
                };
                let subst: HashMap<String, Type> = generics.into_iter().zip(expected_type_args).collect();
                for (method_name, expected) in expected_methods {
                    let expected = expected.substitute(&subst);
                    let actual = self.impl_method_signature_for_type(program, &type_name, method_name.as_str());
                    let Some(actual) = actual else {
                        ctx.emit_issue(
                            *conformance_span,
                            SemanticIssueKind::ContractMethodMissingImplementation {
                                contract_name: contract_name.clone(),
                                method_name: method_name.clone(),
                                expected: expected.to_string(),
                            },
                        );
                        continue;
                    };
                    if actual != expected {
                        ctx.emit_issue(
                            *conformance_span,
                            SemanticIssueKind::ContractImplementationSignatureMismatch {
                                method_name: method_name.clone(),
                                expected: expected.to_string(),
                                actual: actual.to_string(),
                            },
                        );
                    }
                }
            }
        }
    }

    /// The contract's own generic parameter names, in declaration order (empty for a
    /// non-generic contract).
    fn contract_generics(&self, program: &Spanned<Program>, contract_name: &str) -> Vec<String> {
        program
            .node
            .items
            .iter()
            .find_map(|item| match &item.node {
                Node::ContractDefinition(def) if def.node.name.node.name == contract_name => {
                    Some(def.node.generics.iter().map(|generic| generic.node.name.clone()).collect())
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The concrete type arguments supplied at a `type X : Contract<Arg, ...>` or
    /// `impl X : Contract<Arg, ...>` conformance site, in declaration order (empty when the
    /// contract is not generic or was embedded without type arguments).
    fn conformance_type_args(&self, program: &Spanned<Program>, type_name: &str, contract_name: &str) -> Vec<Type> {
        for item in &program.node.items {
            let conformances: &[Spanned<Path>] = match &item.node {
                Node::TypeDefinition(def) if def.node.name.node.name == type_name => &def.node.conformances,
                Node::ImplBlock(impl_block) => {
                    let Type::Complex(receiver_path) = &impl_block.node.receiver_type.node else {
                        continue;
                    };
                    let Some(receiver_name) =
                        receiver_path.node.segments.last().map(|segment| segment.node.name.node.name.as_str())
                    else {
                        continue;
                    };
                    if receiver_name != type_name {
                        continue;
                    }
                    &impl_block.node.conformances
                }
                _ => continue,
            };
            for conformance in conformances {
                if let Some(last_segment) = conformance.node.segments.last()
                    && last_segment.node.name.node.name == contract_name
                {
                    return last_segment.node.type_args.iter().map(|arg| arg.node.clone()).collect();
                }
            }
        }
        Vec::new()
    }

    fn collect_contract_signatures(&self, program: &Spanned<Program>) -> HashMap<String, HashMap<String, MethodSignature>> {
        let definitions: HashMap<String, &Spanned<crate::syntax::ContractDefinition>> = program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                Node::ContractDefinition(definition) => Some((definition.node.name.node.name.clone(), definition)),
                _ => None,
            })
            .collect();

        let mut cache = HashMap::new();
        for contract_name in definitions.keys() {
            let _ =
                self.collect_contract_methods_recursive(contract_name, &definitions, &mut cache, &mut HashSet::new());
        }
        cache
    }

    fn collect_contract_methods_recursive(
        &self,
        contract_name: &str,
        definitions: &HashMap<String, &Spanned<crate::syntax::ContractDefinition>>,
        cache: &mut HashMap<String, HashMap<String, MethodSignature>>,
        active: &mut HashSet<String>,
    ) -> HashMap<String, MethodSignature> {
        if let Some(cached) = cache.get(contract_name) {
            return cached.clone();
        }
        if !active.insert(contract_name.to_string()) {
            return HashMap::new();
        }

        let mut methods = HashMap::new();
        let Some(definition) = definitions.get(contract_name) else {
            active.remove(contract_name);
            return methods;
        };

        for node in &definition.node.items {
            match &node.node {
                ContractNode::MethodSignature(signature) => {
                    methods.insert(
                        signature.node.name.node.name.clone(),
                        MethodSignature::from_parameters_and_return(
                            &signature.node.parameters,
                            signature.node.return_type.as_ref(),
                        ),
                    );
                }
                ContractNode::Embedding(embedding) => {
                    let embedded_name = embedding.node.name.node.name.clone();
                    let embedded =
                        self.collect_contract_methods_recursive(embedded_name.as_str(), definitions, cache, active);
                    for (method_name, signature) in embedded {
                        methods.entry(method_name).or_insert(signature);
                    }
                }
            }
        }

        active.remove(contract_name);
        cache.insert(contract_name.to_string(), methods.clone());
        methods
    }

    fn impl_method_signature_for_type(
        &self,
        program: &Spanned<Program>,
        type_name: &str,
        method_name: &str,
    ) -> Option<MethodSignature> {
        for item in &program.node.items {
            match &item.node {
                Node::Method(method) => {
                    let Type::Complex(receiver_path) = &method.node.receiver_type.node else {
                        continue;
                    };
                    let Some(receiver_name) =
                        receiver_path.node.segments.last().map(|segment| segment.node.name.node.name.as_str())
                    else {
                        continue;
                    };
                    if receiver_name == type_name && method.node.name.node.name == method_name {
                        return Some(MethodSignature::from_parameters_and_return(
                            &method.node.parameters,
                            method.node.return_type.as_ref(),
                        ));
                    }
                }
                Node::TypeDefinition(definition) if definition.node.name.node.name == type_name => {
                    if let Some(method) =
                        definition.node.methods.iter().find(|method| method.node.name.node.name == method_name)
                    {
                        return Some(MethodSignature::from_parameters_and_return(
                            &method.node.parameters,
                            method.node.return_type.as_ref(),
                        ));
                    }
                }
                // `impl T { ... }` (with or without a `: Contract` conformance clause) is a
                // first-class `Node::ImplBlock`, not flattened `Node::Method` items -- look up
                // its methods the same way as `Node::TypeDefinition`'s inline methods.
                Node::ImplBlock(impl_block) => {
                    let Type::Complex(receiver_path) = &impl_block.node.receiver_type.node else {
                        continue;
                    };
                    let Some(receiver_name) =
                        receiver_path.node.segments.last().map(|segment| segment.node.name.node.name.as_str())
                    else {
                        continue;
                    };
                    if receiver_name != type_name {
                        continue;
                    }
                    if let Some(method) =
                        impl_block.node.methods.iter().find(|method| method.node.name.node.name == method_name)
                    {
                        return Some(MethodSignature::from_parameters_and_return(
                            &method.node.parameters,
                            method.node.return_type.as_ref(),
                        ));
                    }
                }
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::analysis::{AnalysisOptions, builtin_rules, run_rules};
    use crate::parser::{BeskidParser, Rule};
    use crate::parsing::parsable::Parsable;
    use crate::syntax::Program;
    use pest::Parser;

    fn analyze(source: &str) -> crate::analysis::AnalysisResult {
        let pair =
            BeskidParser::parse(Rule::Program, source).expect("source should parse").next().expect("program pair");
        let program = Program::parse(pair).expect("source should build AST");
        run_rules(&program.node, "test.bd", source, &builtin_rules(), AnalysisOptions::default())
    }

    #[test]
    fn contract_conformance_recognizes_method_owned_by_type_definition() {
        let source = r#"
            type Request {}
            type Response {}

            contract Analyzer {
                Response Analyze(Request request);
            }

            type ConcreteAnalyzer : Analyzer {
                Response Analyze(Request request) {
                    return Response {};
                }
            }
        "#;

        let result = analyze(source);

        assert!(
            !result.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1601")),
            "nested type method must satisfy Analyzer.Analyze; got: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn impl_block_conformance_satisfied_produces_no_missing_implementation_diagnostic() {
        let source = r#"
            type Request {}
            type Response {}

            contract Analyzer {
                Response Analyze(Request request);
            }

            type ConcreteAnalyzer {}

            impl ConcreteAnalyzer : Analyzer {
                Response Analyze(Request request) {
                    return Response {};
                }
            }
        "#;

        let result = analyze(source);

        assert!(
            !result.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1601")),
            "impl-block method must satisfy Analyzer.Analyze; got: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn impl_block_conformance_missing_method_is_flagged_e1601() {
        let source = r#"
            type Request {}
            type Response {}

            contract Analyzer {
                Response Analyze(Request request);
            }

            type ConcreteAnalyzer {}

            impl ConcreteAnalyzer : Analyzer {
            }
        "#;

        let result = analyze(source);

        assert!(
            result.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1601")),
            "impl-block conformance without the required method must be flagged E1601; got: {:?}",
            result.diagnostics
        );
    }

    #[test]
    fn conformance_flags_a_return_type_mismatch_with_equal_arity() {
        // Same parameter count (0) as the contract signature, but a different return type.
        // The arity-only comparator (`ret(0)` for both) cannot see this; it must be replaced
        // with a real structural comparison for this to be flagged.
        let source = r#"
            type Response {}
            type Other {}

            contract Analyzer {
                Response Analyze();
            }

            type ConcreteAnalyzer : Analyzer {
                Other Analyze() {
                    return Other {};
                }
            }
        "#;

        let result = analyze(source);

        assert!(
            result.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1602")),
            "a return-type mismatch with equal arity must be flagged E1602 (signature mismatch); got: {:?}",
            result.diagnostics
        );
    }

    /// `stage2_type_check` in the `run_rules`/`analyze()` diagnostics pipeline is structural
    /// immutability checks only ("full type-check runs in the lower spine" -- its own doc
    /// comment); the real `TypeChecker` (where `type_id_for_path_with_args`'s generic-arity
    /// check lives) is driven separately through `resolve_and_type_program`. Arity assertions
    /// use that driver directly instead of `analyze()`, which can never see a `TypeError`.
    fn resolve_and_type(source: &str) -> Result<crate::types::TypeResult, Vec<crate::types::result::TypeError>> {
        let program = crate::services::parse_program(source).expect("source should parse");
        match crate::services::resolve_and_type_program(&program) {
            Ok((_, _, typed)) => Ok(typed),
            Err(crate::services::SemanticFactsError::Type { errors, .. }) => Err(errors),
            Err(other) => panic!("expected type-check to run (resolution must succeed first); got: {other:?}"),
        }
    }

    #[test]
    fn generic_contract_conformance_with_too_many_type_arguments_is_rejected() {
        // `Box<T>` declares exactly one generic parameter; supplying two type arguments at the
        // conformance site must be caught as a generic-argument-count mismatch (E1204 at the
        // diagnostics layer), the same mechanism `type X<T> { }` already uses for its own
        // generic arity (task 3.1's "reuses the same mechanism as Gap 2 GenericArgumentMismatch").
        let source = r#"
            contract Box<T> {
                T Get();
            }

            type Container : Box<i32, i32> {
                i32 Get() {
                    return 0;
                }
            }
        "#;

        let errors = resolve_and_type(source).expect_err(
            "a 1-param generic contract conformance supplied with 2 type arguments must be rejected",
        );

        assert!(
            errors.iter().any(|error| matches!(
                error,
                crate::types::result::TypeError::GenericArgumentMismatch { expected: 1, actual: 2, .. }
            )),
            "expected a GenericArgumentMismatch{{expected: 1, actual: 2}}; got: {errors:?}"
        );
    }

    #[test]
    fn generic_contract_conformance_with_the_correct_arity_type_checks_cleanly() {
        // The positive case for the same mechanism: `Box<i32>` supplies exactly the one type
        // argument `Box<T>` declares, so no generic-argument-count mismatch is raised.
        let source = r#"
            contract Box<T> {
                T Get();
            }

            type Container : Box<i32> {
                i32 Get() {
                    return 0;
                }
            }
        "#;

        let result = resolve_and_type(source);
        assert!(
            result.is_ok(),
            "a correctly-arity'd generic contract conformance must type-check cleanly; got: {result:?}"
        );
    }

    #[test]
    fn generic_contract_type_parameter_resolves_inside_its_own_method_signatures() {
        // `T` inside `Box<T>`'s own method signature must resolve to the contract's own generic
        // parameter (not an unresolved/global type), so a correctly-arity'd conformance produces
        // no spurious generic-argument-count diagnostic.
        let source = r#"
            contract Box<T> {
                T Get();
            }

            type Container : Box<i32> {
                i32 Get() {
                    return 0;
                }
            }
        "#;

        let result = analyze(source);

        assert!(
            !result.diagnostics.iter().any(|diagnostic| diagnostic.code.as_deref() == Some("E1204")),
            "a correctly-arity'd generic contract conformance must not be flagged; got: {:?}",
            result.diagnostics
        );
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|diagnostic| matches!(diagnostic.code.as_deref(), Some("E1005") | Some("E1201"))),
            "`T` inside `Box<T>`'s own method signature must resolve to the contract's own \
             generic parameter, not an unresolved/global type; got: {:?}",
            result.diagnostics
        );
    }
}
