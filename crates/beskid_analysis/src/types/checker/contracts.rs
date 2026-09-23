use std::collections::{HashMap, HashSet};

use beskid_abi::abi_v5::AbiType;
use beskid_abi::interop::c_profile::{CAbiProfile, CProfileError};
use beskid_abi::interop::mapping::{SurfacePrimitive, surface_primitive_to_type_shape};
use beskid_abi::interop::{
    CallShapeClass, InteropParameter, InteropReturn, InteropSignature, OwnershipClass, ScalarShape, TypeShape,
};

use crate::resolve::{ItemId, ItemKind, ResolvedType};
use crate::syntax::{ContractNode, Expression, Literal, Node, Path, PrimitiveType, Program, Type};
use crate::syntax::{SpanInfo, Spanned};
use crate::types::TypeId;
use crate::types::result::{FunctionSignature, TypeError};

use super::TypeChecker;

impl<'a> TypeChecker<'a> {
    pub(super) fn seed_method_receiver(
        &mut self,
        method_span: SpanInfo,
        def: &Spanned<crate::syntax::MethodDefinition>,
    ) {
        let Some(method_item_id) = self.item_id_for_span(method_span) else {
            return;
        };
        let Some(ResolvedType::Item(receiver_item_id)) = self.resolved_type_at(def.node.receiver_type.span) else {
            return;
        };
        self.methods_by_receiver.insert((receiver_item_id, def.node.name.node.name.clone()), method_item_id);
    }

    pub(super) fn seed_contract_signatures(&mut self, program: &Spanned<Program>) {
        let definitions: HashMap<String, &Spanned<crate::syntax::ContractDefinition>> = program
            .node
            .items
            .iter()
            .filter_map(|item| match &item.node {
                Node::ContractDefinition(def) => Some((def.node.name.node.name.clone(), def)),
                _ => None,
            })
            .collect();
        let mut cache: HashMap<String, Vec<(String, FunctionSignature)>> = HashMap::new();
        let contract_names = definitions.keys().cloned().collect::<Vec<_>>();

        for contract_name in contract_names {
            // Push this contract's own generic parameters into scope before resolving its
            // signatures, so `T` inside `contract Box<T> { T Get(); }` interns to
            // `TypeInfo::GenericParam("T")` instead of failing resolution (which would
            // silently fall back to `unit` for a generic return type). Popped immediately
            // after, matching the push/pop pattern used everywhere else in the checker.
            let mut inserted = Vec::new();
            if let Some(definition) = definitions.get(contract_name.as_str()) {
                for generic in &definition.node.generics {
                    let name = generic.node.name.clone();
                    let type_id = self.type_table.intern(crate::types::TypeInfo::GenericParam(name.clone()));
                    self.generic_params.insert(name.clone(), type_id);
                    inserted.push(name);
                }
            }
            let signatures = self.collect_contract_signatures_recursive(
                contract_name.as_str(),
                &definitions,
                &mut cache,
                &mut HashSet::new(),
            );
            let Some(contract_item_id) = self.item_id_for_name(&contract_name, ItemKind::Contract) else {
                for name in inserted {
                    self.generic_params.remove(&name);
                }
                continue;
            };
            for (method_name, signature) in signatures {
                self.contract_signatures.insert((contract_item_id, method_name), signature);
            }
            if let Some(definition) = definitions.get(&contract_name) {
                self.validate_extern_contract(definition);
            }
            for name in inserted {
                self.generic_params.remove(&name);
            }
        }
    }

    /// Validates `type X : Contract { }` / `impl X : Contract { }` conformance using
    /// `TypeId`-based `FunctionSignature` equality (Gap 2 #4 / Gap 4 #8), reusing
    /// `self.contract_signatures` (already generation-bound and embedding-recursive, built by
    /// `seed_contract_signatures` above) and each implementor's own `self.function_signatures`
    /// (populated per-item during the main typing pass). Must run after every top-level item has
    /// been typed -- an implementing method may be typed at a different point in the per-item
    /// loop than the `type X : Contract` / `impl X : Contract` node that names it -- which is
    /// why this is a separate, later pass rather than inline in `type_item`.
    pub(super) fn check_contract_conformances(&mut self, program: &Spanned<Program>) {
        let conformances: Vec<(ItemId, ItemId, SpanInfo)> = self
            .resolution
            .tables
            .type_conformances
            .iter()
            .flat_map(|(type_id, edges)| edges.iter().map(move |(contract_id, span)| (*type_id, *contract_id, *span)))
            .collect();

        for (type_item_id, contract_item_id, span) in conformances {
            let Some(type_name) = self.resolution.items.get(type_item_id.0).map(|item| item.name.clone()) else {
                continue;
            };
            let Some(contract_name) = self.resolution.items.get(contract_item_id.0).map(|item| item.name.clone())
            else {
                continue;
            };

            let generics = self.generic_items.get(&contract_item_id).cloned().unwrap_or_default();
            let subst: HashMap<String, TypeId> = if generics.is_empty() {
                HashMap::new()
            } else {
                let type_arg_ids = self.conformance_type_arg_ids(program, &type_name, &contract_name);
                generics.into_iter().zip(type_arg_ids).collect()
            };

            let method_names: Vec<String> = self
                .contract_signatures
                .keys()
                .filter(|(id, _)| *id == contract_item_id)
                .map(|(_, name)| name.clone())
                .collect();

            for method_name in method_names {
                let Some(expected) = self.contract_signatures.get(&(contract_item_id, method_name.clone())).cloned()
                else {
                    continue;
                };
                let expected = FunctionSignature {
                    params: expected.params.iter().map(|param| self.substitute_type_id(*param, &subst)).collect(),
                    return_type: self.substitute_type_id(expected.return_type, &subst),
                };

                let actual = self
                    .item_id_for_name(&format!("{type_name}::{method_name}"), ItemKind::Method)
                    .and_then(|item_id| self.function_signatures.get(&item_id).cloned());
                let Some(actual) = actual else {
                    self.errors.push(TypeError::ContractMethodMissingImplementation {
                        span,
                        contract_name: contract_name.clone(),
                        method_name: method_name.clone(),
                        expected,
                    });
                    continue;
                };

                if actual != expected {
                    self.errors.push(TypeError::ContractImplementationSignatureMismatch {
                        span,
                        method_name: method_name.clone(),
                        expected,
                        actual,
                    });
                }
            }
        }
    }

    /// The concrete type arguments (already resolved to `TypeId`s) supplied at a
    /// `type X : Contract<Arg, ...>` or `impl X : Contract<Arg, ...>` conformance site.
    fn conformance_type_arg_ids(&mut self, program: &Spanned<Program>, type_name: &str, contract_name: &str) -> Vec<TypeId> {
        let mut arg_type_syntax: Vec<Spanned<Type>> = Vec::new();
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
                    arg_type_syntax = last_segment.node.type_args.clone();
                    break;
                }
            }
        }
        arg_type_syntax.iter().filter_map(|ty| self.type_id_for_type_in_generic_scope(ty)).collect()
    }

    fn collect_contract_signatures_recursive(
        &mut self,
        contract_name: &str,
        definitions: &HashMap<String, &Spanned<crate::syntax::ContractDefinition>>,
        cache: &mut HashMap<String, Vec<(String, FunctionSignature)>>,
        active: &mut HashSet<String>,
    ) -> Vec<(String, FunctionSignature)> {
        if let Some(cached) = cache.get(contract_name) {
            return cached.clone();
        }
        if !active.insert(contract_name.to_string()) {
            return Vec::new();
        }

        let mut methods = Vec::new();
        let Some(definition) = definitions.get(contract_name) else {
            active.remove(contract_name);
            return methods;
        };

        for node in &definition.node.items {
            match &node.node {
                ContractNode::MethodSignature(signature) => {
                    if methods.iter().any(|(name, _)| name == &signature.node.name.node.name) {
                        continue;
                    }
                    let mut params = Vec::new();
                    let mut valid = true;
                    for param in &signature.node.parameters {
                        let Some(type_id) = self.type_id_for_type(&param.node.ty) else {
                            valid = false;
                            break;
                        };
                        params.push(type_id);
                    }
                    if !valid {
                        continue;
                    }
                    let return_type = signature
                        .node
                        .return_type
                        .as_ref()
                        .and_then(|ty| self.type_id_for_type(ty))
                        .or_else(|| self.primitive_type_id(PrimitiveType::Unit));
                    let Some(return_type) = return_type else {
                        continue;
                    };
                    methods.push((signature.node.name.node.name.clone(), FunctionSignature { params, return_type }));
                }
                ContractNode::Embedding(embedding) => {
                    let embedded = self.collect_contract_signatures_recursive(
                        embedding.node.name.node.name.as_str(),
                        definitions,
                        cache,
                        active,
                    );
                    for (method_name, signature) in embedded {
                        if methods.iter().any(|(name, _)| name == &method_name) {
                            continue;
                        }
                        methods.push((method_name, signature));
                    }
                }
            }
        }

        active.remove(contract_name);
        cache.insert(contract_name.to_string(), methods.clone());
        methods
    }

    /// Validate an `[Extern(...)]` contract against the `Interop.Contracts` C
    /// ABI profile. Constructs [`TypeError::ExternInvalidAbi`],
    /// [`TypeError::ExternMissingLibrary`], [`TypeError::ExternDisallowedParamType`],
    /// or [`TypeError::ExternDisallowedReturnType`] when the contract violates
    /// the FFI boundary rules.
    fn validate_extern_contract(&mut self, definition: &Spanned<crate::syntax::ContractDefinition>) {
        let Some(extern_attr) = definition.node.attributes.iter().find(|attr| attr.node.name.node.name == "Extern")
        else {
            return;
        };

        let contract_span = definition.span;
        let (abi, library) = extract_extern_attr_args(extern_attr);

        match abi.as_deref() {
            Some("C") => {}
            other => {
                self.errors.push(TypeError::ExternInvalidAbi { span: contract_span, abi: other.map(str::to_owned) });
                return;
            }
        }

        if library.is_none() {
            self.errors.push(TypeError::ExternMissingLibrary { span: contract_span });
            return;
        }

        let profile = CAbiProfile;
        for node in &definition.node.items {
            if let ContractNode::MethodSignature(signature) = &node.node {
                self.validate_extern_method(&profile, signature);
            }
        }
    }

    fn validate_extern_method(
        &mut self,
        profile: &CAbiProfile,
        signature: &Spanned<crate::syntax::ContractMethodSignature>,
    ) {
        let method_name = signature.node.name.node.name.clone();
        let method_span = signature.span;

        let mut interop_params = Vec::new();
        for param in &signature.node.parameters {
            match surface_type_shape(&param.node.ty) {
                Some((shape, _)) => {
                    interop_params.push(InteropParameter {
                        name: param.node.name.node.name.clone(),
                        ty: shape,
                        call: CallShapeClass::Direct,
                        ownership: OwnershipClass::Borrow,
                    });
                }
                None => {
                    self.errors.push(TypeError::ExternDisallowedParamType {
                        span: param.node.ty.span,
                        method: method_name.clone(),
                        detail: extern_disallowed_detail(&param.node.ty, false),
                    });
                    return;
                }
            }
        }

        let return_type = signature.node.return_type.as_ref();
        let (return_shape, no_return) = match return_type {
            None => (None, false),
            Some(ty) => match &ty.node {
                Type::Primitive(pt) if pt.node == PrimitiveType::Unit => (None, false),
                Type::Primitive(pt) if pt.node == PrimitiveType::Never => (Some(TypeShape::Never), true),
                Type::Primitive(_) => match surface_type_shape(ty) {
                    Some((shape, _)) => (Some(shape), false),
                    None => {
                        self.errors.push(TypeError::ExternDisallowedReturnType {
                            span: ty.span,
                            method: method_name.clone(),
                            detail: extern_disallowed_detail(ty, true),
                        });
                        return;
                    }
                },
                _ => {
                    self.errors.push(TypeError::ExternDisallowedReturnType {
                        span: ty.span,
                        method: method_name.clone(),
                        detail: extern_disallowed_detail(ty, true),
                    });
                    return;
                }
            },
        };

        let return_shape = match return_shape {
            Some(shape) => shape,
            None => {
                let interop_sig = InteropSignature {
                    symbol: method_name.clone(),
                    parameters: interop_params,
                    returns: InteropReturn {
                        name: None,
                        ty: TypeShape::Scalar(ScalarShape { abi_type: AbiType::I32 }),
                        ownership: OwnershipClass::Borrow,
                    },
                    no_return,
                };
                if let Err(err) = profile.validate_signature(&interop_sig) {
                    self.emit_extern_profile_error(err, &method_name, method_span);
                }
                return;
            }
        };

        let interop_sig = InteropSignature {
            symbol: method_name.clone(),
            parameters: interop_params,
            returns: InteropReturn { name: None, ty: return_shape, ownership: OwnershipClass::Borrow },
            no_return,
        };

        if let Err(err) = profile.validate_signature(&interop_sig) {
            self.emit_extern_profile_error(err, &method_name, method_span);
        }
    }

    fn emit_extern_profile_error(&mut self, err: CProfileError, method_name: &str, method_span: SpanInfo) {
        match err {
            CProfileError::DisallowedShape { .. } | CProfileError::TransferRequiresDirectOrView { .. } => {
                self.errors.push(TypeError::ExternDisallowedParamType {
                    span: method_span,
                    method: method_name.to_owned(),
                    detail: err.to_string(),
                });
            }
            CProfileError::DisallowedReturn => {
                self.errors.push(TypeError::ExternDisallowedReturnType {
                    span: method_span,
                    method: method_name.to_owned(),
                    detail: err.to_string(),
                });
            }
        }
    }
}

/// Extract `(Abi, Library)` string values from an `[Extern(...)]` attribute.
fn extract_extern_attr_args(attr: &Spanned<crate::syntax::Attribute>) -> (Option<String>, Option<String>) {
    let mut abi = None;
    let mut library = None;
    for argument in &attr.node.arguments {
        let value = match &argument.node.value.node {
            Expression::Literal(literal) => match &literal.node.literal.node {
                Literal::String(raw) => raw.strip_prefix('"').and_then(|v| v.strip_suffix('"')).map(str::to_owned),
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
    (abi, library)
}

/// Map a surface [`Type`] to its FFI [`TypeShape`]. Returns `None` for types
/// not permitted at the FFI boundary (`Char`, `String`, `Unit`, and all
/// non-primitive types). The second tuple element is the [`SurfacePrimitive`]
/// when the type is a primitive.
fn surface_type_shape(ty: &Spanned<Type>) -> Option<(TypeShape, Option<SurfacePrimitive>)> {
    let Type::Primitive(pt) = &ty.node else {
        return None;
    };
    let surface = primitive_to_surface(pt.node);
    surface_primitive_to_type_shape(surface).map(|shape| (shape, Some(surface)))
}

/// Convert a surface [`PrimitiveType`] to the ABI-layer [`SurfacePrimitive`]
/// mirror (avoids a `beskid_abi → beskid_analysis` dependency).
fn primitive_to_surface(primitive: PrimitiveType) -> SurfacePrimitive {
    match primitive {
        PrimitiveType::Bool => SurfacePrimitive::Bool,
        PrimitiveType::I32 => SurfacePrimitive::I32,
        PrimitiveType::I64 => SurfacePrimitive::I64,
        PrimitiveType::U32 => SurfacePrimitive::U32,
        PrimitiveType::U8 => SurfacePrimitive::U8,
        PrimitiveType::Pointer => SurfacePrimitive::Pointer,
        PrimitiveType::Word => SurfacePrimitive::Word,
        PrimitiveType::F64 => SurfacePrimitive::F64,
        PrimitiveType::Char => SurfacePrimitive::Char,
        PrimitiveType::String => SurfacePrimitive::String,
        PrimitiveType::Unit => SurfacePrimitive::Unit,
        PrimitiveType::Never => SurfacePrimitive::Never,
    }
}

/// Produce a human-readable detail string for a disallowed FFI type.
fn extern_disallowed_detail(ty: &Spanned<Type>, is_return: bool) -> String {
    match &ty.node {
        Type::Primitive(pt) => match pt.node {
            PrimitiveType::Char => "char is not permitted at the FFI boundary".to_string(),
            PrimitiveType::String => "string is a GC reference; use CStringView at the FFI boundary".to_string(),
            PrimitiveType::Unit if is_return => "unit is the void-return marker; omit the return type".to_string(),
            PrimitiveType::Unit => "unit is not a valid FFI parameter type".to_string(),
            PrimitiveType::Word => {
                "word (pointer-width unsigned) is not in the C profile permitted scalars; use pointer instead"
                    .to_string()
            }
            _ => "type is not permitted at the FFI boundary".to_string(),
        },
        Type::Array(_) => "array types must use CBuffer or CArrayView at the FFI boundary".to_string(),
        Type::Complex(_) => "only primitive types are permitted at the FFI boundary".to_string(),
        Type::Associated { .. } => "associated types are not permitted at the FFI boundary".to_string(),
        Type::Function { .. } => "function types are not permitted at the FFI boundary".to_string(),
    }
}

#[cfg(test)]
mod conformance_tests {
    use crate::services::{SemanticFactsError, parse_program, resolve_and_type_program};
    use crate::types::result::TypeError;

    fn resolve_and_type(source: &str) -> Result<(), Vec<TypeError>> {
        let program = parse_program(source).expect("source should parse");
        match resolve_and_type_program(&program) {
            Ok(_) => Ok(()),
            Err(SemanticFactsError::Type { errors, .. }) => Err(errors),
            Err(other) => panic!("expected type-check to run (resolution must succeed first); got: {other:?}"),
        }
    }

    #[test]
    fn missing_contract_method_implementation_is_rejected_with_typeid_signature() {
        let source = r#"
            type Request {}
            type Response {}

            contract Analyzer {
                Response Analyze(Request request);
            }

            type ConcreteAnalyzer : Analyzer {}
        "#;
        let errors = resolve_and_type(source).expect_err("missing method must be rejected");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                TypeError::ContractMethodMissingImplementation { method_name, .. } if method_name == "Analyze"
            )),
            "expected ContractMethodMissingImplementation for Analyze; got: {errors:?}"
        );
    }

    #[test]
    fn return_type_mismatch_with_equal_arity_is_rejected_via_typeid_equality() {
        // Same parameter count (0) as the contract signature but a different return type --
        // exactly the case an arity-only comparator would miss.
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
        let errors = resolve_and_type(source).expect_err("return-type mismatch must be rejected");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                TypeError::ContractImplementationSignatureMismatch { method_name, .. } if method_name == "Analyze"
            )),
            "expected ContractImplementationSignatureMismatch for Analyze; got: {errors:?}"
        );
    }

    #[test]
    fn satisfied_conformance_type_checks_cleanly() {
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
        let result = resolve_and_type(source);
        assert!(result.is_ok(), "a correctly-implemented conformance must type-check cleanly; got: {result:?}");
    }

    #[test]
    fn impl_block_conformance_missing_method_is_rejected() {
        let source = r#"
            type Request {}
            type Response {}

            contract Analyzer {
                Response Analyze(Request request);
            }

            type ConcreteAnalyzer {}

            impl ConcreteAnalyzer : Analyzer {}
        "#;
        let errors = resolve_and_type(source).expect_err("missing impl-block method must be rejected");
        assert!(
            errors.iter().any(|error| matches!(error, TypeError::ContractMethodMissingImplementation { .. })),
            "expected ContractMethodMissingImplementation; got: {errors:?}"
        );
    }

    #[test]
    fn impl_block_conformance_satisfied_type_checks_cleanly() {
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
        let result = resolve_and_type(source);
        assert!(result.is_ok(), "a correctly-implemented impl-block conformance must type-check cleanly; got: {result:?}");
    }

    #[test]
    fn generic_contract_conformance_with_correct_substitution_type_checks_cleanly() {
        // `T` in `Box<T>`'s own signature must be substituted with `i32` (the conformance
        // site's concrete argument) before comparing against the implementor's `i32 Get()`.
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
            "a generic contract conformance with correctly-substituted types must type-check cleanly; got: {result:?}"
        );
    }

    #[test]
    fn generic_contract_conformance_with_wrong_concrete_type_is_rejected() {
        // `Box<i32>` expects `i32 Get()` after substitution; implementing `string Get()` must
        // be rejected by real TypeId equality (not just arity).
        let source = r#"
            contract Box<T> {
                T Get();
            }

            type Container : Box<i32> {
                string Get() {
                    return "wrong";
                }
            }
        "#;
        let errors =
            resolve_and_type(source).expect_err("a substituted-type mismatch after generic substitution must be rejected");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                TypeError::ContractImplementationSignatureMismatch { method_name, .. } if method_name == "Get"
            )),
            "expected ContractImplementationSignatureMismatch for Get; got: {errors:?}"
        );
    }
}
