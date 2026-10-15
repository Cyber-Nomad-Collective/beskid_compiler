//! Source-issued SDK callbacks are not runtime intrinsics or ordinary user externs.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum NativeModCallbackOperation {
    ResolveType,
    ResolveSyntaxType,
    ResolveSyntaxTemplate,
    PlanCanonicalPaths,
    ResolveFunction,
    CheckSerializable,
    TypeShape,
    CaptureCatchall,
    ValidateCatchall,
    Query,
}
impl NativeModCallbackOperation {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::PlanCanonicalPaths => "__beskid_mod_semantic_plan_canonical_paths_v2",
            Self::ResolveFunction => "__beskid_mod_semantic_resolve_function_v2",
            Self::CheckSerializable => "__beskid_mod_semantic_check_serializable_v2",
            Self::ResolveSyntaxTemplate => "__beskid_mod_semantic_resolve_syntax_template_v2",
            Self::ResolveSyntaxType => "__beskid_mod_semantic_resolve_syntax_type_v2",
            Self::ResolveType => "__beskid_mod_semantic_resolve_type_v2",
            Self::TypeShape => "__beskid_mod_semantic_type_shape_v2",
            Self::CaptureCatchall => "__beskid_mod_semantic_capture_catchall_v2",
            Self::ValidateCatchall => "__beskid_mod_semantic_validate_catchall_v2",
            Self::Query => "__beskid_mod_query_v2",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct NativeModCallback {
    wrapper: AstNodeKey,
    operation: NativeModCallbackOperation,
}
impl NativeModCallback {
    pub fn wrapper(self) -> AstNodeKey {
        self.wrapper
    }
    pub fn operation(self) -> NativeModCallbackOperation {
        self.operation
    }
}

pub(super) fn native_mod_callback_for(
    db: &dyn Db,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    path: &beskid_analysis::syntax::Path,
) -> Option<NativeModCallback> {
    let [segment] = path.segments.as_slice() else {
        return None;
    };
    if !segment.node.type_args.is_empty() {
        return None;
    }
    let (operation, wrapper_name, source_path) = match segment.node.name.node.name.as_str() {
        "__mod_semantic_plan_canonical_paths" => {
            (NativeModCallbackOperation::PlanCanonicalPaths, "PlanCanonicalPaths", "src/Beskid/Compiler/Semantic.bd")
        }
        "__mod_semantic_resolve_function" => {
            (NativeModCallbackOperation::ResolveFunction, "ResolveFunction", "src/Beskid/Compiler/Semantic.bd")
        }
        "__mod_semantic_check_serializable" => {
            (NativeModCallbackOperation::CheckSerializable, "CheckSerializable", "src/Beskid/Compiler/Semantic.bd")
        }
        "__mod_semantic_resolve_syntax_template" => (
            NativeModCallbackOperation::ResolveSyntaxTemplate,
            "ResolveSyntaxTemplate",
            "src/Beskid/Compiler/Semantic.bd",
        ),
        "__mod_semantic_resolve_syntax_type" => {
            (NativeModCallbackOperation::ResolveSyntaxType, "ResolveSyntaxType", "src/Beskid/Compiler/Semantic.bd")
        }
        "__mod_semantic_resolve_type" => {
            (NativeModCallbackOperation::ResolveType, "ResolveType", "src/Beskid/Compiler/Semantic.bd")
        }
        "__mod_semantic_type_shape" => {
            (NativeModCallbackOperation::TypeShape, "TypeShape", "src/Beskid/Compiler/Semantic.bd")
        }
        "__mod_semantic_capture_catchall" => {
            (NativeModCallbackOperation::CaptureCatchall, "CaptureCatchall", "src/Beskid/Compiler/Semantic.bd")
        }
        "__mod_semantic_validate_catchall" => {
            (NativeModCallbackOperation::ValidateCatchall, "ValidateCatchall", "src/Beskid/Compiler/Semantic.bd")
        }
        name if name.starts_with("__mod_query_") => {
            (NativeModCallbackOperation::Query, name.strip_prefix("__mod_query_")?, "src/Beskid/Compiler/Query.bd")
        }
        _ => return None,
    };
    let syntax = db.syntax_unit(key.unit)?;
    if !syntax.accepts_key(db, key) || syntax.revision(db).sdk_source_authority.as_deref() != Some(source_path) {
        return None;
    }
    let node = locals::enclosing_executable_callable(index, key.node)?;
    let wrapper = AstNodeKey { node, ..key };
    if node_kind(db, wrapper).ok().flatten() != Some(IndexedNodeKind::FunctionDefinition)
        || item_name(db, wrapper).ok().flatten().as_deref() != Some(wrapper_name)
    {
        return None;
    }
    Some(NativeModCallback { wrapper, operation })
}

/// The symbol is derived from a source-issued wrapper, never from an arbitrary caller string.
pub fn native_mod_callback_symbol(db: &dyn Db, callback: NativeModCallback) -> Result<String, SemanticError> {
    if callback.operation != NativeModCallbackOperation::Query {
        return Ok(callback.operation.symbol().to_owned());
    }
    let name = item_name(db, callback.wrapper)?
        .ok_or_else(|| SemanticError::new("native SDK callback wrapper has no current name"))?;
    Ok(format!("__beskid_mod_query_v2_{name}"))
}

/// A closed SDK contract role issued only from the current canonical contract source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeModContractFamily {
    Factory,
    Collector,
    Generator,
    GrammarGenerator,
    Analyzer,
    Rewriter,
    AttributeGenerator,
}
impl NativeModContractFamily {
    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::Factory => "ModFactory",
            Self::Collector => "Collector",
            Self::Generator => "Generator",
            Self::GrammarGenerator => "GrammarGenerator",
            Self::Analyzer => "Analyzer",
            Self::Rewriter => "Rewriter",
            Self::AttributeGenerator => "AttributeGenerator",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeModContractDeclaration {
    key: AstNodeKey,
    family: NativeModContractFamily,
}
impl NativeModContractDeclaration {
    pub fn key(self) -> AstNodeKey {
        self.key
    }
    pub fn family(self) -> NativeModContractFamily {
        self.family
    }
}
pub fn native_mod_contract_declaration(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<NativeModContractDeclaration> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Ok(None) };
    if !syntax.accepts_key(db, key)
        || syntax.revision(db).sdk_source_authority.as_deref() != Some("src/Beskid/Compiler/Collect.bd")
        || node_kind(db, key)? != Some(IndexedNodeKind::ContractDefinition)
    {
        return Ok(None);
    }
    // `item_name` names only callables and test items; a contract's name comes from its own
    // declaration node. A ContractDefinition kind without that node is a broken index.
    let name = syntax
        .syntax_index(db)
        .node_at(syntax.expanded_program(db), key.node)
        .and_then(|node| node.of::<beskid_analysis::syntax::ContractDefinition>())
        .map(|contract| contract.name.node.name.clone())
        .ok_or_else(|| SemanticError::new("canonical SDK contract declaration has no current contract syntax"))?;
    let family = [
        NativeModContractFamily::Factory,
        NativeModContractFamily::Collector,
        NativeModContractFamily::Generator,
        NativeModContractFamily::GrammarGenerator,
        NativeModContractFamily::Analyzer,
        NativeModContractFamily::Rewriter,
        NativeModContractFamily::AttributeGenerator,
    ]
    .into_iter()
    .find(|family| name == family.canonical_name());
    Ok(family.map(|family| NativeModContractDeclaration { key, family }))
}

/// Typed request constructors from the selected canonical SDK source, not caller symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NativeModRequestFactory {
    Compilation,
    Member,
    Workspace,
    Registration,
    Package,
    Catalog,
    Collect,
    Targets,
    Generation,
    Analysis,
    Attribute,
    AttributeDeclarations,
}
impl NativeModRequestFactory {
    pub fn canonical_name(self) -> &'static str {
        match self {
            Self::Compilation => "CompilationValue",
            Self::Member => "MemberValue",
            Self::Workspace => "WorkspaceValue",
            Self::Registration => "RegistrationValue",
            Self::Package => "PackageValue",
            Self::Catalog => "CatalogValue",
            Self::Collect => "CollectValue",
            Self::Targets => "TargetsValue",
            Self::Generation => "GenerationValue",
            Self::Analysis => "AnalysisValue",
            Self::Attribute => "AttributeValue",
            Self::AttributeDeclarations => "AttributeDeclarationsValue",
        }
    }
    pub fn all() -> [Self; 12] {
        [
            Self::Compilation,
            Self::Member,
            Self::Workspace,
            Self::Registration,
            Self::Package,
            Self::Catalog,
            Self::Collect,
            Self::Targets,
            Self::Generation,
            Self::Analysis,
            Self::Attribute,
            Self::AttributeDeclarations,
        ]
    }
}
#[derive(Debug, Clone)]
pub struct NativeModRequestConstructor {
    key: AstNodeKey,
    role: NativeModRequestFactory,
    signature: ItemSignature,
}
impl NativeModRequestConstructor {
    pub fn key(&self) -> AstNodeKey {
        self.key
    }
    pub fn role(&self) -> NativeModRequestFactory {
        self.role
    }
    pub fn signature(&self) -> &ItemSignature {
        &self.signature
    }
}
pub fn native_mod_request_constructor(
    db: &dyn Db,
    key: AstNodeKey,
) -> SemanticQueryResult<NativeModRequestConstructor> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Ok(None) };
    if !syntax.accepts_key(db, key)
        || syntax.revision(db).sdk_source_authority.as_deref() != Some("src/Beskid/Compiler/NativeRequests.bd")
        || node_kind(db, key)? != Some(IndexedNodeKind::FunctionDefinition)
    {
        return Ok(None);
    }
    let Some(name) = item_name(db, key)? else { return Ok(None) };
    let Some(role) = NativeModRequestFactory::all().into_iter().find(|role| name.as_ref() == role.canonical_name())
    else {
        return Ok(None);
    };
    let signature = item_abi_signature(db, key)?
        .ok_or_else(|| SemanticError::new("canonical native SDK request constructor has no current signature"))?;
    Ok(Some(NativeModRequestConstructor { key, role, signature }))
}

/// A concrete generated syntax constructor from the immutable current SDK source.
/// This witness alone does not grant native invocation or managed layout authority.
#[derive(Debug, Clone)]
pub struct NativeModSyntaxConstructor {
    key: AstNodeKey,
    name: String,
    signature: ItemSignature,
}
impl NativeModSyntaxConstructor {
    pub fn key(&self) -> AstNodeKey {
        self.key
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn signature(&self) -> &ItemSignature {
        &self.signature
    }
}
pub fn native_mod_syntax_constructor(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<NativeModSyntaxConstructor> {
    let Some(syntax) = db.syntax_unit(key.unit) else { return Ok(None) };
    if !syntax.accepts_key(db, key)
        || syntax.revision(db).sdk_source_authority.as_deref() != Some("src/Beskid/Syntax/NativeFactories.bd")
        || node_kind(db, key)? != Some(IndexedNodeKind::FunctionDefinition)
    {
        return Ok(None);
    }
    let Some(name) = item_name(db, key)? else { return Ok(None) };
    let signature = item_abi_signature(db, key)?
        .ok_or_else(|| SemanticError::new("canonical generated syntax constructor lacks a current signature"))?;
    Ok(Some(NativeModSyntaxConstructor { key, name: name.to_string(), signature }))
}
