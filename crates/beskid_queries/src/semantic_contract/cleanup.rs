//! Source-owned scoped disposal contracts. Lowering consumes these exact declarations.

use super::*;
use beskid_analysis::syntax::{
    ContractDefinition, ContractNode, FunctionDefinition, MethodDefinition, PrimitiveType, ScopedUseStatement, Type,
    TypeDefinition, Visibility,
};
use beskid_analysis::syntax_query::{DynNodeRef, NodeKind, SyntaxIndex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ScopedCleanupDiagnostic {
    NonResultCallable,
    NotDisposable,
    InvalidDisposeSignature,
    MissingConversion,
    AmbiguousConversion,
    InvalidConversion,
    ResourceEscapesScope,
    ExplicitDispose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ScopedAcquisition {
    FreshConstruction(AstNodeKey),
    FreshFactory(AstNodeKey),
    FreshTry(AstNodeKey),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ScopedCleanup {
    pub binding: AstNodeKey,
    pub body: Option<AstNodeKey>,
    pub acquisition: Option<ScopedAcquisition>,
    pub callable: Option<AstNodeKey>,
    pub dispose: Option<AstNodeKey>,
    pub conversion: Option<AstNodeKey>,
    pub dispose_result: Option<AstNodeKey>,
    pub enclosing_result: Option<AstNodeKey>,
    pub dispose_layout: Option<EnumLayoutFact>,
    pub enclosing_layout: Option<EnumLayoutFact>,
    pub converted_error_managed: bool,
    pub diagnostic: Option<ScopedCleanupDiagnostic>,
}

pub fn scoped_cleanup(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ScopedCleanup> {
    with_registered_syntax(db, key, scoped_cleanup_tracked)
}

#[salsa::tracked(persist)]
fn scoped_cleanup_tracked(db: &dyn Db, syntax: SyntaxUnitInput, key: AstNodeKey) -> SemanticQueryResult<ScopedCleanup> {
    with_node(db, syntax, key, |program, index, node| scoped_cleanup_for_node(db, program, index, key, node))?
        .transpose()
}

fn scoped_cleanup_for_node(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    node: DynNodeRef<'_>,
) -> Option<Result<ScopedCleanup, SemanticError>> {
    let scoped = node.of::<ScopedUseStatement>()?;
    let binding = index.direct_child_id(program, key.node, DynNodeRef::from(&scoped.binding))?;
    let mut fact = ScopedCleanup {
        binding: AstNodeKey { node: binding, ..key },
        body: scoped
            .body
            .as_ref()
            .and_then(|body| index.direct_child_id(program, key.node, DynNodeRef::from(body)))
            .map(|node| AstNodeKey { node, ..key }),
        acquisition: None,
        callable: None,
        dispose: None,
        conversion: None,
        dispose_result: None,
        enclosing_result: None,
        dispose_layout: None,
        enclosing_layout: None,
        converted_error_managed: false,
        diagnostic: None,
    };
    macro_rules! reject {
        ($kind:ident) => {{
            fact.diagnostic = Some(ScopedCleanupDiagnostic::$kind);
            return Some(Ok(fact));
        }};
    }
    let Some(callable) = parent_node(index, key.node).and_then(|parent| {
        nearest_ancestor(index, parent, |kind| {
            matches!(kind, NodeKind::FunctionDefinition | NodeKind::MethodDefinition | NodeKind::LambdaExpression)
        })
    }) else {
        reject!(NonResultCallable);
    };
    let callable_key = AstNodeKey { node: callable, ..key };
    fact.callable = Some(callable_key);
    let callable_node = index.node_at(program, callable)?;
    let return_type = callable_node
        .of::<FunctionDefinition>()
        .and_then(|function| function.return_type.as_ref())
        .or_else(|| callable_node.of::<MethodDefinition>().and_then(|method| method.return_type.as_ref()));
    let Some(return_type) = return_type else {
        reject!(NonResultCallable);
    };
    let Some((_, enclosing_error)) = result_type_parts(&return_type.node) else {
        reject!(NonResultCallable);
    };
    let Some(result_declaration) = canonical_result_definition_for_type(db, key, &return_type.node) else {
        reject!(NonResultCallable);
    };
    fact.enclosing_result =
        index.direct_child_id(program, callable, DynNodeRef::from(return_type)).map(|node| AstNodeKey { node, ..key });

    let Some(Type::Complex(resource_path)) = scoped.binding.node.type_annotation.as_ref().map(|ty| &ty.node) else {
        reject!(NotDisposable);
    };
    let Some(resource) = resolve_type_declaration(db, key, &resource_path.node) else {
        reject!(NotDisposable);
    };
    let resource_syntax = db.syntax_unit(resource.unit)?;
    let resource_program = resource_syntax.expanded_program(db);
    let resource_index = resource_syntax.syntax_index(db);
    let Some(definition) =
        resource_index.node_at(resource_program, resource.node).and_then(|node| node.of::<TypeDefinition>())
    else {
        reject!(NotDisposable);
    };
    let conformances = definition
        .conformances
        .iter()
        .filter_map(|conformance| cleanup_contract_declaration(db, resource, &conformance.node))
        .collect::<Vec<_>>();
    let [contract] = conformances.as_slice() else {
        reject!(NotDisposable);
    };
    let contract_syntax = db.syntax_unit(contract.unit)?;
    let contract_program = contract_syntax.expanded_program(db);
    let contract_index = contract_syntax.syntax_index(db);
    let contract_definition = contract_index.node_at(contract_program, contract.node)?.of::<ContractDefinition>()?;
    let signatures = contract_definition
        .items
        .iter()
        .filter_map(|item| match &item.node {
            ContractNode::MethodSignature(method) if method.node.name.node.name == "Dispose" => Some(method),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [signature] = signatures.as_slice() else {
        reject!(InvalidDisposeSignature);
    };
    let Some(contract_result) = signature.node.return_type.as_ref() else {
        reject!(InvalidDisposeSignature);
    };
    let Some((payload, dispose_error)) = result_type_parts(&contract_result.node) else {
        reject!(InvalidDisposeSignature);
    };
    if !signature.node.parameters.is_empty()
        || !matches!(&payload.node, Type::Primitive(primitive) if primitive.node == PrimitiveType::Unit)
        || cleanup_named_type(db, *contract, &dispose_error.node, "DisposeError").is_none()
        || canonical_result_definition_for_type(db, *contract, &contract_result.node) != Some(result_declaration)
    {
        reject!(InvalidDisposeSignature);
    }
    let Some(dispose) = unique_nominal_method_declaration(db, resource, "Dispose") else {
        reject!(InvalidDisposeSignature);
    };
    let method = resource_index.node_at(resource_program, dispose.node)?.of::<MethodDefinition>()?;
    let Some(method_result) = method.return_type.as_ref() else {
        reject!(InvalidDisposeSignature);
    };
    if !method.parameters.is_empty()
        || !same_cleanup_type(db, dispose, &method_result.node, *contract, &contract_result.node)
    {
        reject!(InvalidDisposeSignature);
    }
    fact.dispose = Some(dispose);
    fact.dispose_result = resource_index
        .direct_child_id(resource_program, dispose.node, DynNodeRef::from(method_result))
        .map(|node| AstNodeKey { node, ..dispose });
    if !same_cleanup_type(db, key, &enclosing_error.node, *contract, &dispose_error.node) {
        let (conversions, invalid) =
            cleanup_conversion_candidates(db, key, *contract, &dispose_error.node, &enclosing_error.node);
        if invalid {
            reject!(InvalidConversion);
        }
        match conversions.as_slice() {
            [] => reject!(MissingConversion),
            [conversion] => fact.conversion = Some(*conversion),
            _ => reject!(AmbiguousConversion),
        }
    }
    fact.acquisition = scoped_acquisition(db, program, index, &fact, resource);
    if fact.acquisition.is_none() {
        reject!(ResourceEscapesScope);
    }
    fact.diagnostic = scoped_resource_escape(db, program, index, key, &fact);
    if fact.diagnostic.is_none() {
        let Type::Complex(dispose_path) = &method_result.node else {
            reject!(InvalidDisposeSignature);
        };
        let Type::Complex(enclosing_path) = &return_type.node else {
            reject!(NonResultCallable);
        };
        fact.dispose_layout = match instantiated_enum_layout_for_path(db, dispose, &dispose_path.node) {
            Ok(layout) => Some(layout),
            Err(error) => return Some(Err(error)),
        };
        fact.enclosing_layout = match instantiated_enum_layout_for_path(db, key, &enclosing_path.node) {
            Ok(layout) => Some(layout),
            Err(error) => return Some(Err(error)),
        };
        fact.converted_error_managed =
            match super::typing::managed_reference_kind_for_syntax_type(&enclosing_error.node) {
                Ok(kind) => kind == ManagedReferenceKind::GcManaged,
                Err(error) => return Some(Err(error)),
            };
    }
    Some(Ok(fact))
}

/// Acquiring an alias is not ownership transfer. Follow only exact fresh constructors and
/// source-resolved factories whose every return is recursively fresh. A recursive component
/// must have a concrete construction leaf; a cycle alone supplies no ownership evidence.
fn scoped_acquisition(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    fact: &ScopedCleanup,
    resource: AstNodeKey,
) -> Option<ScopedAcquisition> {
    let binding = index.node_at(program, fact.binding.node)?.of::<beskid_analysis::syntax::LetStatement>()?;
    let node = index.direct_child_id(program, fact.binding.node, DynNodeRef::from(&binding.value))?;
    let expression = AstNodeKey { node: normalized_expression_node(index, node), ..fact.binding };
    let mut constructed = false;
    if !fresh_cleanup_expression(db, expression, resource, None, &mut HashSet::new(), &mut constructed) || !constructed
    {
        return None;
    }
    match index.kind(expression.node)? {
        NodeKind::StructLiteralExpression => Some(ScopedAcquisition::FreshConstruction(expression)),
        NodeKind::CallExpression => match call_lowering(db, expression).ok()?? {
            CallLowering::Direct(factory) => Some(ScopedAcquisition::FreshFactory(factory)),
            _ => None,
        },
        NodeKind::TryExpression => Some(ScopedAcquisition::FreshTry(expression)),
        _ => None,
    }
}

/// A validated `?` supplies the exact Result identity. Only its Ok payload can
/// acquire ownership; Error exits before there is a scoped resource to dispose.
struct FreshResultSuccess {
    identity: GenericSourceTypeIdentity,
    declaration: AstNodeKey,
}

fn fresh_cleanup_expression(
    db: &dyn Db,
    expression: AstNodeKey,
    resource: AstNodeKey,
    result: Option<&FreshResultSuccess>,
    visited: &mut HashSet<AstNodeKey>,
    constructed: &mut bool,
) -> bool {
    let Some(syntax) = db.syntax_unit(expression.unit).filter(|syntax| syntax.accepts_key(db, expression)) else {
        return false;
    };
    let index = syntax.syntax_index(db);
    let expression = AstNodeKey { node: normalized_expression_node(index, expression.node), ..expression };
    if let Some(result) = result {
        if generic_source_expression_identity(db, expression).ok().as_ref() != Some(&result.identity) {
            return false;
        }
        if index.kind(expression.node) == Some(NodeKind::EnumConstructorExpression) {
            let Some(constructor) = enum_constructor(db, expression).ok().flatten() else {
                return false;
            };
            if constructor.declaration != result.declaration {
                return false;
            }
            return match (constructor.variant_index, constructor.payloads.as_ref()) {
                (0, [payload]) => fresh_cleanup_expression(db, *payload, resource, None, visited, constructed),
                // A propagated error is not evidence of fresh ownership. The caller
                // still requires at least one concrete successful construction leaf.
                (1, [_]) => true,
                _ => false,
            };
        }
    } else if index.kind(expression.node) == Some(NodeKind::TryExpression) {
        let Some(fact) = try_expression_fact(db, expression).ok().flatten() else {
            return false;
        };
        if contracts::concrete_declaration(db, expression, &fact.payload_identity) != Some(resource) {
            return false;
        }
        let Ok(identity) = generic_source_expression_identity(db, fact.operand) else {
            return false;
        };
        let Ok((declaration, _)) = layouts::enum_layout_for_source_identity(db, fact.operand, &identity) else {
            return false;
        };
        return fresh_cleanup_expression(
            db,
            fact.operand,
            resource,
            Some(&FreshResultSuccess { identity, declaration }),
            visited,
            constructed,
        );
    } else if index.kind(expression.node) == Some(NodeKind::StructLiteralExpression) {
        let fresh = aggregate_literal_declaration(db, expression).ok().flatten() == Some(resource);
        *constructed |= fresh;
        return fresh;
    }
    let Some(CallLowering::Direct(factory)) = call_lowering(db, expression).ok().flatten() else {
        return false;
    };
    if !visited.insert(factory) {
        // Preserve the existing direct-factory policy, but do not infer a fresh
        // Result success through a cyclic fallible factory.
        return result.is_none();
    }
    let Some(syntax) = db.syntax_unit(factory.unit).filter(|syntax| syntax.accepts_key(db, factory)) else {
        return false;
    };
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    let Some(function) = index.node_at(program, factory.node).and_then(|node| node.of::<FunctionDefinition>()) else {
        return false;
    };
    if !function.generics.is_empty() {
        return false;
    }
    let returns = index
        .ids_of_kind(NodeKind::ReturnStatement)
        .filter(|node| {
            is_ancestor(index, factory.node, *node)
                && nearest_ancestor(index, *node, |kind| {
                    matches!(
                        kind,
                        NodeKind::FunctionDefinition | NodeKind::LambdaExpression | NodeKind::MethodDefinition
                    )
                }) == Some(factory.node)
        })
        .collect::<Vec<_>>();
    if returns.is_empty() {
        return false;
    }
    let fresh = returns.into_iter().all(|node| {
        let value = index
            .node_at(program, node)
            .and_then(|node| node.of::<beskid_analysis::syntax::ReturnStatement>())
            .and_then(|ret| ret.value.as_ref());
        let Some(value) = value else {
            return false;
        };
        let Some(value) = index.direct_child_id(program, node, DynNodeRef::from(value)) else {
            return false;
        };
        fresh_cleanup_expression(db, AstNodeKey { node: value, ..factory }, resource, result, visited, constructed)
    });
    if result.is_some() {
        visited.remove(&factory);
    }
    fresh
}

fn scoped_resource_escape(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    key: AstNodeKey,
    fact: &ScopedCleanup,
) -> Option<ScopedCleanupDiagnostic> {
    use ScopedCleanupDiagnostic::{ExplicitDispose, ResourceEscapesScope};
    let declaration = index
        .children(fact.binding.node)?
        .iter()
        .copied()
        .find(|child| index.kind(*child) == Some(NodeKind::Identifier))?;
    let callable = fact.callable?;
    let mut visited = HashSet::new();
    if let Some(diagnostic) = scoped_receiver_method_escape(db, fact.dispose?, fact.dispose?, &mut visited) {
        return Some(diagnostic);
    }
    for path_node in index.ids_of_kind(NodeKind::PathExpression).filter(|node| is_ancestor(index, callable.node, *node))
    {
        let path = index.node_at(program, path_node)?.of::<beskid_analysis::syntax::PathExpression>()?;
        let Some(first) = path.path.node.segments.first() else {
            continue;
        };
        if resolve_lexical_declaration(program, index, path_node, &first.node.name.node.name) != Some(declaration) {
            continue;
        }
        if nearest_ancestor(index, path_node, |kind| {
            matches!(kind, NodeKind::LambdaExpression | NodeKind::FunctionDefinition | NodeKind::MethodDefinition)
        }) != Some(callable.node)
        {
            return Some(ResourceEscapesScope);
        }
        // A bare resource value copies ownership into another value/argument/return.
        // Qualified paths may borrow the receiver only for a source-resolved method.
        if path.path.node.segments.len() == 1 {
            return Some(ResourceEscapesScope);
        }
        let path_key = AstNodeKey { node: path_node, ..key };
        let Some(call) = cleanup_path_call(program, index, path_key) else {
            if nominal_local_member_receiver(db, program, index, path_key, &path.path.node).is_some()
                || aggregate_field_access(db, path_key).ok().flatten().is_none()
            {
                return Some(ResourceEscapesScope);
            }
            continue;
        };
        let Some((method, _)) = nominal_local_member_receiver(db, program, index, call, &path.path.node) else {
            return Some(ResourceEscapesScope);
        };
        if Some(method) == fact.dispose {
            return Some(ExplicitDispose);
        }
        if let Some(diagnostic) = scoped_receiver_method_escape(db, method, fact.dispose?, &mut visited) {
            return Some(diagnostic);
        }
    }
    None
}

fn cleanup_path_call(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &SyntaxIndex,
    path: AstNodeKey,
) -> Option<AstNodeKey> {
    let mut parent = parent_node(index, path.node)?;
    while index.kind(parent) == Some(NodeKind::Expression) {
        parent = parent_node(index, parent)?;
    }
    let call = index.node_at(program, parent)?.of::<beskid_analysis::syntax::CallExpression>()?;
    let callee = index.direct_child_id(program, parent, DynNodeRef::from(call.callee.as_ref()))?;
    (normalized_expression_node(index, callee) == path.node).then_some(AstNodeKey { node: parent, ..path })
}

/// Follow the actual receiver call graph. Receiver values cannot become explicit
/// arguments, returns, or captures, including through sibling method chains.
fn scoped_receiver_method_escape(
    db: &dyn Db,
    method: AstNodeKey,
    dispose: AstNodeKey,
    visited: &mut HashSet<AstNodeKey>,
) -> Option<ScopedCleanupDiagnostic> {
    use ScopedCleanupDiagnostic::{ExplicitDispose, ResourceEscapesScope};
    if !visited.insert(method) {
        return None;
    }
    let Some(syntax) = db.syntax_unit(method.unit) else {
        return Some(ResourceEscapesScope);
    };
    if syntax.generation(db) != method.generation {
        return Some(ResourceEscapesScope);
    }
    let program = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    if index.node_at(program, method.node).and_then(|node| node.of::<MethodDefinition>()).is_none() {
        return Some(ResourceEscapesScope);
    }
    for node in index.ids_of_kind(NodeKind::PathExpression).filter(|node| is_ancestor(index, method.node, *node)) {
        let Some(path) =
            index.node_at(program, node).and_then(|node| node.of::<beskid_analysis::syntax::PathExpression>())
        else {
            return Some(ResourceEscapesScope);
        };
        let path_key = AstNodeKey { node, ..method };
        let Some(first) = path.path.node.segments.first() else {
            return Some(ResourceEscapesScope);
        };
        let is_self =
            first.node.name.node.name == "self" && resolve_lexical_declaration(program, index, node, "self").is_none();
        if is_self && path.path.node.segments.len() == 1 {
            return Some(ResourceEscapesScope);
        }
        let call = cleanup_path_call(program, index, path_key);
        let receiver_member = if resolve_lexical_declaration(program, index, node, &first.node.name.node.name).is_none()
        {
            unqualified_enclosing_method_call(program, index, path_key, &path.path.node)
        } else {
            None
        };
        if receiver_member.is_some() && call.is_none() {
            return Some(ResourceEscapesScope);
        }
        let receiver_call = call.and(receiver_member);
        let borrowed_field =
            aggregate_field_access(db, path_key).ok().flatten().is_some_and(|access| access.receiver == method);
        if (is_self || receiver_call.is_some() || borrowed_field)
            && nearest_ancestor(index, node, |kind| {
                matches!(kind, NodeKind::LambdaExpression | NodeKind::MethodDefinition)
            }) != Some(method.node)
        {
            return Some(ResourceEscapesScope);
        }
        if let Some((target, _)) = receiver_call {
            if target == dispose {
                return Some(ExplicitDispose);
            }
            if let Some(diagnostic) = scoped_receiver_method_escape(db, target, dispose, visited) {
                return Some(diagnostic);
            }
        } else if is_self && call.is_some() {
            // Explicit/dynamic receiver paths not proven by the current method resolver
            // cannot be assumed borrowing merely from their spelling.
            return Some(ResourceEscapesScope);
        }
    }
    None
}

fn cleanup_visible_units(db: &dyn Db, key: AstNodeKey) -> Vec<SourceUnitId> {
    let mut units = vec![key.unit];
    let registry = db.syntax_dependency_registry().lock().expect("syntax dependency registry");
    for import in registry.imports.get(&(key.unit, key.generation)).into_iter().flatten() {
        if !units.contains(&import.target) {
            units.push(import.target);
        }
    }
    units
}

fn cleanup_contract_declaration(
    db: &dyn Db,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<AstNodeKey> {
    let (name, modules) = path.segments.split_last()?;
    if name.node.name.node.name != "Disposable" || !name.node.type_args.is_empty() {
        return None;
    }
    let units = if modules.is_empty() {
        cleanup_visible_units(db, key)
    } else {
        vec![resolve_qualified_module_unit(
            db,
            key,
            &modules.iter().map(|segment| segment.node.name.node.name.clone()).collect::<Vec<_>>(),
        )?]
    };
    let mut candidates = Vec::new();
    for unit in units {
        let syntax = db.syntax_unit(unit)?;
        if syntax.generation(db) != key.generation {
            return None;
        }
        for node in syntax.syntax_index(db).ids_of_kind(NodeKind::ContractDefinition) {
            if !cleanup_declaration_in_scope(db, key, AstNodeKey { unit, node, ..key }) {
                continue;
            }
            let definition =
                syntax.syntax_index(db).node_at(syntax.expanded_program(db), node)?.of::<ContractDefinition>()?;
            if definition.name.node.name == "Disposable"
                && (unit == key.unit || definition.visibility.node == Visibility::Public)
            {
                candidates.push(AstNodeKey { unit, node, ..key });
            }
        }
    }
    match candidates.as_slice() {
        [candidate] => Some(*candidate),
        _ => None,
    }
}

fn cleanup_declaration_in_scope(db: &dyn Db, use_site: AstNodeKey, declaration: AstNodeKey) -> bool {
    let Some(syntax) = db.syntax_unit(declaration.unit) else {
        return false;
    };
    let index = syntax.syntax_index(db);
    let scope = module_scope(index, declaration.node);
    if declaration.unit == use_site.unit {
        scope == module_scope(index, use_site.node)
    } else {
        scope.is_some_and(|scope| index.kind(scope) == Some(NodeKind::Program))
    }
}

fn cleanup_named_type(db: &dyn Db, key: AstNodeKey, ty: &Type, name: &str) -> Option<AstNodeKey> {
    let Type::Complex(path) = ty else {
        return None;
    };
    (path.node.segments.last()?.node.name.node.name == name).then_some(())?;
    resolve_type_declaration(db, key, &path.node)
}

fn same_cleanup_type(db: &dyn Db, left_key: AstNodeKey, left: &Type, right_key: AstNodeKey, right: &Type) -> bool {
    match (left, right) {
        (Type::Complex(left), Type::Complex(right)) => {
            let left_definition = resolve_type_declaration(db, left_key, &left.node);
            let right_definition = resolve_type_declaration(db, right_key, &right.node);
            left_definition.is_some()
                && left_definition == right_definition
                && left.node.segments.last().zip(right.node.segments.last()).is_some_and(|(left, right)| {
                    left.node.type_args.len() == right.node.type_args.len()
                        && left
                            .node
                            .type_args
                            .iter()
                            .zip(&right.node.type_args)
                            .all(|(left, right)| same_cleanup_type(db, left_key, &left.node, right_key, &right.node))
                })
        }
        (Type::Primitive(left), Type::Primitive(right)) => left.node == right.node,
        (Type::Array(left), Type::Array(right)) => same_cleanup_type(db, left_key, &left.node, right_key, &right.node),
        _ => false,
    }
}

fn cleanup_conversion_candidates(
    db: &dyn Db,
    key: AstNodeKey,
    error_key: AstNodeKey,
    dispose_error: &Type,
    enclosing_error: &Type,
) -> (Vec<AstNodeKey>, bool) {
    let mut candidates = Vec::new();
    let mut invalid = false;
    for unit in cleanup_visible_units(db, key) {
        let Some(syntax) = db.syntax_unit(unit) else {
            invalid = true;
            continue;
        };
        if syntax.generation(db) != key.generation {
            invalid = true;
            continue;
        }
        for node in syntax.syntax_index(db).ids_of_kind(NodeKind::FunctionDefinition) {
            if !cleanup_declaration_in_scope(db, key, AstNodeKey { unit, node, ..key }) {
                continue;
            }
            let Some(function) = syntax
                .syntax_index(db)
                .node_at(syntax.expanded_program(db), node)
                .and_then(|node| node.of::<FunctionDefinition>())
            else {
                continue;
            };
            if !function.attributes.iter().any(|attribute| attribute.node.name.node.name == "CleanupConversion")
                || (unit != key.unit && function.visibility.node != Visibility::Public)
            {
                continue;
            }
            let candidate = AstNodeKey { unit, node, ..key };
            let eligible = function.generics.is_empty()
                && function.parameters.len() == 1
                && same_cleanup_type(db, candidate, &function.parameters[0].node.ty.node, error_key, dispose_error)
                && function
                    .return_type
                    .as_ref()
                    .is_some_and(|result| same_cleanup_type(db, candidate, &result.node, key, enclosing_error));
            if eligible {
                candidates.push(candidate);
            } else {
                invalid = true;
            }
        }
    }
    (candidates, invalid)
}
