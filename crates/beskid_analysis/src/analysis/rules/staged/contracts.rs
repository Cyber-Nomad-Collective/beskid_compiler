use super::SemanticPipelineRule;
use crate::analysis::diagnostic_kinds::SemanticIssueKind;
use crate::analysis::rules::RuleContext;
use crate::resolve::Resolution;
use crate::syntax::Spanned;
use crate::syntax::{ContractNode, Node, Program, Type};
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
                for (method_name, expected) in expected_methods {
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
                    if &actual != expected {
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
}
