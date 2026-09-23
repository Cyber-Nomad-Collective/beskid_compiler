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
        // Each method's receiver type is re-spanned to the method itself, so the resolver's type
        // fact sits on the receiver path, not on this span. The declared receiver type is then
        // the authority, as in the unit surface (`surface/builder.rs::seed_method_receiver`).
        let receiver_item_id = match self.resolved_type_at(def.node.receiver_type.span) {
            Some(ResolvedType::Item(item_id)) => Some(item_id),
            _ => {
                // Seeding only looks the receiver up; the method's own typing pass reports an
                // unresolvable receiver.
                let errors_before = self.errors.len();
                let receiver = self.type_id_for_type(&def.node.receiver_type);
                self.errors.truncate(errors_before);
                receiver.and_then(|type_id| self.named_item_id(type_id))
            }
        };
        let Some(receiver_item_id) = receiver_item_id else {
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
                // `This` inside a contract's own signature (`This Method();`) is a synthetic,
                // always-present generic parameter, substituted with the conforming type's own
                // `TypeId` by `check_contract_conformances` below -- not the contract's own
                // nominal type (design.md: "`This` refers to the eventual implementing type,
                // NOT the contract's own nominal type").
                let this_type_id = self.type_table.intern(crate::types::TypeInfo::GenericParam("This".to_string()));
                self.generic_params.insert("This".to_string(), this_type_id);
                inserted.push("This".to_string());
                // A contract's own declared associated types (`type Item;`) are, like `This`,
                // synthetic generic parameters in scope for the contract's own method
                // signatures (design.md: "the bare associated-type name (`Item`) is in scope
                // and MAY be used directly in method signatures"). Substituted per-implementor
                // by `check_contract_conformances`.
                for node in &definition.node.items {
                    if let ContractNode::AssociatedType(assoc) = &node.node {
                        let name = assoc.node.name.node.name.clone();
                        let type_id = self.type_table.intern(crate::types::TypeInfo::GenericParam(name.clone()));
                        self.generic_params.insert(name.clone(), type_id);
                        inserted.push(name);
                    }
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
            // This unit's own contracts report unresolved signature types at the type itself
            // (`contract_signature_type_id`), not again at each conformance.
            self.contract_unresolved_methods.remove(&contract_item_id);
            for (method_name, signature) in signatures {
                self.contract_signatures.insert((contract_item_id, method_name), signature);
            }
            if let Some(definition) = definitions.get(&contract_name) {
                // Associated-type defaults resolve while the contract's own scope (generics,
                // `This`, sibling associated types) is still pushed.
                let mut associated = Vec::new();
                for node in &definition.node.items {
                    if let ContractNode::AssociatedType(assoc) = &node.node {
                        let default = assoc.node.default.as_ref().and_then(|ty| self.type_id_for_type(ty));
                        associated.push((assoc.node.name.node.name.clone(), default));
                    }
                }
                if associated.is_empty() {
                    self.contract_associated_types.remove(&contract_item_id);
                } else {
                    self.contract_associated_types.insert(contract_item_id, associated);
                }
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
        // The resolution's conformance table also carries every dependency unit's edges. Only
        // conformances declared in this unit's own syntax are checked here: the implementor
        // syntax (conformance type arguments, associated-type bindings) and the diagnostic span
        // both belong to this program. A dependency's conformances are checked when that unit is
        // itself the entry.
        let declared_here = Self::conformance_declarations(program);
        let conformances: Vec<(ItemId, ItemId, SpanInfo)> = self
            .resolution
            .tables
            .type_conformances
            .iter()
            .flat_map(|(type_id, edges)| edges.iter().map(move |(contract_id, span)| (*type_id, *contract_id, *span)))
            .filter(|(type_id, _, span)| {
                self.resolution
                    .items
                    .get(type_id.0)
                    .and_then(|item| item.name.rsplit("::").next())
                    .is_some_and(|name| declared_here.contains(&(name.to_string(), *span)))
            })
            .collect();

        for (type_item_id, contract_item_id, span) in conformances {
            let Some(type_name) = self.resolution.items.get(type_item_id.0).map(|item| item.name.clone()) else {
                continue;
            };
            let Some(contract_name) = self.resolution.items.get(contract_item_id.0).map(|item| item.name.clone())
            else {
                continue;
            };

            // The implementor's own declared generics (`type ArrayIterator<T> : Iterator<T>`)
            // must be in scope for the rest of this conformance check: they appear both in the
            // conformance's own type-argument list (`Iterator<T>`'s `T`) and inside any
            // associated-type binding syntax the implementor supplies (`type Item = T;`) -- both
            // resolved below via `type_id_for_type[_in_generic_scope]`, which consults
            // `self.generic_params` first. Popped at the end of this conformance's checks.
            let implementor_generics = self.generic_items.get(&type_item_id).cloned().unwrap_or_default();
            let mut implementor_generics_inserted = Vec::new();
            for name in &implementor_generics {
                let type_id = self.type_table.intern(crate::types::TypeInfo::GenericParam(name.clone()));
                self.generic_params.insert(name.clone(), type_id);
                implementor_generics_inserted.push(name.clone());
            }

            let generics = self.generic_items.get(&contract_item_id).cloned().unwrap_or_default();
            let mut subst: HashMap<String, TypeId> = if generics.is_empty() {
                HashMap::new()
            } else {
                let type_arg_ids = self.conformance_type_arg_ids(program, &type_name, &contract_name);
                generics.into_iter().zip(type_arg_ids).collect()
            };
            // `This` in the contract's own signature always substitutes to the conforming
            // type's own identity at this conformance site (direct-impl case; a bounded generic
            // `This` at a monomorphized call site is deferred to a later slice). When the
            // implementor itself declares generics (`type Box<T> : Factory { ... }`), "its own
            // identity" is *that type applied to its own generics* (`Box<T>`), not the bare
            // unparameterized name -- otherwise an implementor's own `Box<T> Make()` return type
            // (an `Applied` type) could never structurally equal the substituted `This` (a bare
            // `Named` type), and every generic conformance to a `This`-returning contract would
            // be spuriously rejected.
            if let Some(this_type_id) = self.named_types.get(&type_item_id).copied() {
                let own_generics = self.generic_items.get(&type_item_id).cloned().unwrap_or_default();
                let self_type_id = if own_generics.is_empty() {
                    this_type_id
                } else {
                    let args: Vec<TypeId> = own_generics
                        .iter()
                        .map(|name| self.type_table.intern(crate::types::TypeInfo::GenericParam(name.clone())))
                        .collect();
                    self.type_table.intern(crate::types::TypeInfo::Applied { base: type_item_id, args })
                };
                subst.insert("This".to_string(), self_type_id);
            }

            // Associated types (Gap 4 remainder): every associated type the contract declares
            // must resolve to a concrete `TypeId` at this conformance site, either via the
            // implementor's own `type Item = Concrete;` binding or the contract's declared
            // default (`type Item = T;`, itself resolved through the contract-generic
            // substitution already computed above). Missing both is a fail-closed diagnostic
            // (spec scenario: "Missing associated type binding without default is rejected").
            let contract_assoc_types: Vec<(String, Option<TypeId>)> =
                self.contract_associated_types.get(&contract_item_id).cloned().unwrap_or_default();

            for (assoc_name, default) in contract_assoc_types {
                let binding_syntax = self.associated_type_binding_syntax(program, &type_name, &assoc_name);
                let resolved = if let Some(ty_syntax) = binding_syntax {
                    self.type_id_for_type_in_generic_scope(&ty_syntax)
                } else if let Some(default) = default {
                    // The default was resolved in the contract's own scope (`type Item = T;`
                    // interns `GenericParam("T")`); substitute this conformance site's arguments.
                    Some(self.substitute_type_id(default, &subst))
                } else {
                    None
                };

                match resolved {
                    Some(type_id) => {
                        subst.insert(assoc_name.clone(), type_id);
                        self.associated_type_bindings.insert((type_item_id, assoc_name), type_id);
                    }
                    None => {
                        self.errors.push(TypeError::ContractAssociatedTypeMissingBinding {
                            span,
                            contract_name: contract_name.clone(),
                            assoc_name,
                        });
                    }
                }
            }

            // A dependency contract method whose signature did not resolve in its declaring
            // unit's surface has no signature to compare against: fail closed with E1201.
            let unresolved_methods = self.contract_unresolved_methods.get(&contract_item_id).cloned().unwrap_or_default();
            for method_name in unresolved_methods {
                self.errors.push(TypeError::UnknownType { span, name: format!("{contract_name}::{method_name} signature") });
            }

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

            for name in implementor_generics_inserted {
                self.generic_params.remove(&name);
            }
        }
    }

    /// `(implementor name, conformance path span)` for every `type X : C` / `impl X : C`
    /// conformance written in `program`. The span is the key the resolver records per edge.
    fn conformance_declarations(program: &Spanned<Program>) -> HashSet<(String, SpanInfo)> {
        let mut declared = HashSet::new();
        for item in &program.node.items {
            match &item.node {
                Node::TypeDefinition(def) => {
                    for conformance in &def.node.conformances {
                        declared.insert((def.node.name.node.name.clone(), conformance.span));
                    }
                }
                Node::ImplBlock(impl_block) => {
                    let Type::Complex(receiver_path) = &impl_block.node.receiver_type.node else {
                        continue;
                    };
                    let Some(receiver_name) = receiver_path.node.segments.last() else {
                        continue;
                    };
                    for conformance in &impl_block.node.conformances {
                        declared.insert((receiver_name.node.name.node.name.clone(), conformance.span));
                    }
                }
                _ => {}
            }
        }
        declared
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

    /// The implementor's `type <assoc_name> = <Type>;` binding syntax for `type_name`, found in
    /// either its `type X : Contract { }` body or a paired `impl X : Contract { }` body.
    fn associated_type_binding_syntax(
        &self,
        program: &Spanned<Program>,
        type_name: &str,
        assoc_name: &str,
    ) -> Option<Spanned<Type>> {
        for item in &program.node.items {
            match &item.node {
                Node::TypeDefinition(def) if def.node.name.node.name == type_name => {
                    for binding in &def.node.associated_type_bindings {
                        if binding.node.name.node.name == assoc_name {
                            return Some(binding.node.ty.clone());
                        }
                    }
                }
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
                    for binding in &impl_block.node.associated_type_bindings {
                        if binding.node.name.node.name == assoc_name {
                            return Some(binding.node.ty.clone());
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Resolves one type written in a contract method signature. When resolution fails without
    /// a diagnostic of its own, reports E1201 (`UnknownType`) at the type so the failure is never
    /// silent.
    fn contract_signature_type_id(&mut self, ty: &Spanned<Type>) -> Option<TypeId> {
        let errors_before = self.errors.len();
        let resolved = self.type_id_for_type(ty);
        if resolved.is_none() && self.errors.len() == errors_before {
            let name = match &ty.node {
                Type::Complex(path) => super::types::path_display_name(path),
                _ => "contract signature type".to_string(),
            };
            self.errors.push(TypeError::UnknownType { span: ty.span, name });
        }
        resolved
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
                    // Fail closed: a parameter or return type that does not resolve drops the
                    // method and reports E1201. An unresolved return type must never read as
                    // the omitted-return `unit`.
                    let mut params = Vec::new();
                    let mut valid = true;
                    for param in &signature.node.parameters {
                        let Some(type_id) = self.contract_signature_type_id(&param.node.ty) else {
                            valid = false;
                            break;
                        };
                        params.push(type_id);
                    }
                    if !valid {
                        continue;
                    }
                    let return_type = match signature.node.return_type.as_ref() {
                        Some(ty) => self.contract_signature_type_id(ty),
                        None => self.primitive_type_id(PrimitiveType::Unit),
                    };
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
                // Associated-type declarations are not methods; they are handled separately by
                // `seed_contract_signatures` (bare-name scope) and `check_contract_conformances`
                // (per-implementor binding).
                ContractNode::AssociatedType(_) => {}
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
        Type::This => "This is not permitted at the FFI boundary".to_string(),
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

    #[test]
    fn this_return_type_resolves_to_the_impl_site_receiver_type() {
        let source = r#"
            contract Factory {
                This Make();
            }

            type Widget {}

            impl Widget : Factory {
                Widget Make() {
                    return Widget {};
                }
            }
        "#;
        let result = resolve_and_type(source);
        assert!(result.is_ok(), "`This` at a direct impl site must substitute to the receiver type; got: {result:?}");
    }

    #[test]
    fn this_in_an_implementing_method_signature_is_the_receiver_type() {
        let source = r#"
            contract Step {
                This Next(This previous);
            }

            type Walker : Step {
                i64 steps,

                This Next(This previous) {
                    return Walker { steps: previous.steps + 1 };
                }
            }

            type Widget {}

            impl Widget : Step {
                This Next(This previous) {
                    return previous;
                }
            }

            unit Main(Walker walker, Widget widget) {
                Walker next = walker.Next(walker);
                Widget same = widget.Next(widget);
                return;
            }
        "#;
        let result = resolve_and_type(source);
        assert!(result.is_ok(), "`This` in an implementing method's signature must be its receiver type; got: {result:?}");
    }

    #[test]
    fn this_in_a_method_signature_rejects_a_different_type() {
        let source = r#"
            type Other {}

            type Walker {
                This Next() {
                    return Other {};
                }
            }
        "#;
        let errors = resolve_and_type(source).expect_err("returning another type from `This` must be rejected");
        assert!(
            !errors.iter().any(|error| matches!(error, TypeError::ThisUsedOutsideContractOrImpl { .. })),
            "`This` in a method signature is in scope; got: {errors:?}"
        );
    }

    #[test]
    fn this_return_type_rejects_the_wrong_concrete_type() {
        let source = r#"
            contract Factory {
                This Make();
            }

            type Widget {}
            type Other {}

            impl Widget : Factory {
                Other Make() {
                    return Other {};
                }
            }
        "#;
        let errors = resolve_and_type(source)
            .expect_err("implementing `This Make()` with the wrong concrete return type must be rejected");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                TypeError::ContractImplementationSignatureMismatch { method_name, .. } if method_name == "Make"
            )),
            "expected ContractImplementationSignatureMismatch for Make; got: {errors:?}"
        );
    }

    #[test]
    fn this_used_outside_contract_or_impl_is_rejected() {
        // A top-level function using `This` as a return type has no enclosing contract/impl
        // scope to substitute it from -- must fail closed with a diagnostic, not silently
        // vanish (slice 9 / task 1.7).
        let source = r#"
            This orphan() {
                return;
            }
        "#;
        let errors = resolve_and_type(source).expect_err("`This` used outside a contract or impl must be rejected");
        assert!(
            errors.iter().any(|error| matches!(error, TypeError::ThisUsedOutsideContractOrImpl { .. })),
            "expected ThisUsedOutsideContractOrImpl; got: {errors:?}"
        );
    }

    #[test]
    fn unresolved_associated_type_reference_is_rejected() {
        // `Something::Item` names a real, resolvable type (`Something`) that has no associated
        // type binding -- resolution succeeds (the path itself is valid), but type-checking
        // must still fail closed with a diagnostic rather than silently vanishing (slice 9 /
        // task 1.7): early binding-based resolution for `T::Item` is deferred to a future
        // slice (see the `Type::Associated` arm in `types/checker/types.rs`).
        let source = r#"
            type Something {}

            Something::Item orphan() {
                return;
            }
        "#;
        let errors = resolve_and_type(source).expect_err("an unresolved associated-type reference must be rejected");
        assert!(
            errors.iter().any(|error| matches!(error, TypeError::UnresolvedAssociatedType { name, .. } if name == "Item")),
            "expected UnresolvedAssociatedType; got: {errors:?}"
        );
    }

    #[test]
    fn associated_type_bare_name_is_in_scope_inside_its_own_contract() {
        let source = r#"
            type Nothing {}

            contract Iterator {
                type Item;
                Item Current();
            }

            type ArrayIterator : Iterator {
                type Item = Nothing;

                Nothing Current() {
                    return Nothing {};
                }
            }
        "#;
        let result = resolve_and_type(source);
        assert!(
            result.is_ok(),
            "a contract's own associated type must be in scope in its signatures, and an implementor's binding must \
             satisfy conformance; got: {result:?}"
        );
    }

    #[test]
    fn associated_type_missing_binding_without_default_is_rejected() {
        let source = r#"
            contract Iterator {
                type Item;
                Item Current();
            }

            type ArrayIterator : Iterator {
                i32 Current() {
                    return 0;
                }
            }
        "#;
        let errors = resolve_and_type(source)
            .expect_err("an associated type with no default and no implementor binding must be rejected");
        assert!(
            errors.iter().any(|error| matches!(
                error,
                TypeError::ContractAssociatedTypeMissingBinding { assoc_name, .. } if assoc_name == "Item"
            )),
            "expected ContractAssociatedTypeMissingBinding; got: {errors:?}"
        );
    }

    #[test]
    fn associated_type_default_is_used_when_implementor_omits_the_binding() {
        let source = r#"
            contract Box<T> {
                type Item = T;
                Item Get();
            }

            type Container : Box<i32> {
                i32 Get() {
                    return 0;
                }
            }
        "#;
        let result = resolve_and_type(source);
        assert!(result.is_ok(), "an omitted binding with a declared default must fall back to the default; got: {result:?}");
    }

    #[test]
    fn this_return_type_works_for_type_conformance_too() {
        // Same mechanism, `type X : Contract { }` syntax instead of `impl`.
        let source = r#"
            contract Factory {
                This Make();
            }

            type Widget : Factory {
                Widget Make() {
                    return Widget {};
                }
            }
        "#;
        let result = resolve_and_type(source);
        assert!(result.is_ok(), "`This` must resolve the same way for `type X : Contract`; got: {result:?}");
    }

    #[test]
    fn two_methods_in_a_generic_type_body_both_use_this() {
        // Isolates whether `this` resolution breaks specifically with >1 method in a generic
        // type body (no contracts/associated types involved at all).
        let source = r#"
            type Pair<T> {
                T left,
                T right,

                Pair<T> First() {
                    return this;
                }

                Pair<T> Second() {
                    return this;
                }
            }
        "#;
        let result = resolve_and_type(source);
        assert!(result.is_ok(), "`this` must resolve in every method of a generic type body; got: {result:?}");
    }

    #[test]
    fn generic_iterator_style_contract_with_applied_associated_type_and_this() {
        // Mirrors the real corelib shape (`Query.Iterator<T>` / `Query.ArrayIterator<T>`,
        // slice 11): a generic contract whose associated type appears applied as a type
        // argument (`Option<Item>`), implemented by a generic type binding `Item = T` and
        // returning `This` from a second method.
        let source = r#"
            enum Option<T> {
                Some(T value),
                None,
            }

            contract Iterator<T> {
                type Item;
                Option<Item> Current();
                This MoveNext();
            }

            type ArrayIterator<T> : Iterator<T> {
                T[] source,

                type Item = T;

                Option<T> Current() {
                    return Option::None();
                }

                ArrayIterator<T> MoveNext() {
                    return this;
                }
            }
        "#;
        let result = resolve_and_type(source);
        assert!(
            result.is_ok(),
            "a generic contract's associated type applied as a type argument (`Option<Item>`) must resolve and \
             compare correctly against the generic implementor's binding; got: {result:?}"
        );
    }

    #[test]
    fn this_return_type_resolves_to_the_generic_implementors_own_applied_type() {
        // A generic implementor's `This` must substitute to *its own* applied type (parameterized
        // by its own declared generics, e.g. `Box<T>`), not the bare unparameterized name --
        // otherwise every generic type implementing a `This`-returning contract is spuriously
        // rejected as a signature mismatch (slice 11 finding, exercised by `ArrayIterator<T> :
        // Iterator<T>`'s real `This MoveNext();` conformance).
        let source = r#"
            contract Factory {
                This Make();
            }

            type Box<T> : Factory {
                T value,

                Box<T> Make() {
                    return this;
                }
            }
        "#;
        let result = resolve_and_type(source);
        assert!(
            result.is_ok(),
            "`This` at a generic implementor must substitute to that implementor's own applied type; got: {result:?}"
        );
    }
}

