//! Scoped cleanup facts and the per-node disposal contract walk.

use super::super::*;
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
            match super::super::typing::managed_reference_kind_for_syntax_type(&enclosing_error.node) {
                Ok(kind) => kind == ManagedReferenceKind::GcManaged,
                Err(error) => return Some(Err(error)),
            };
    }
    Some(Ok(fact))
}
