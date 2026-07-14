//! Public AST/Salsa semantic contracts used by later frontend and codegen replacement slices.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use beskid_analysis::projects::SyntaxProgramAssembly;
use beskid_analysis::syntax::SyntaxGenerationId;

use crate::db::Db;
use crate::inputs::ProjectSession;

/// Source-unit identity, interned by a normalized absolute logical path.
#[salsa::interned(constructor = intern_path, no_lifetime, debug)]
pub struct SourceUnitId {
    #[get(interned_path)]
    #[returns(ref)]
    path: PathBuf,
}

impl SourceUnitId {
    /// Normalize the deepest existing ancestor before interning the remaining logical suffix.
    ///
    /// This makes new LSP files stable when they are first named through a symlink and later
    /// created on disk.
    pub fn new(db: &dyn Db, path: PathBuf) -> Self {
        Self::intern_path(db, normalized_source_path(&path))
    }

    pub fn path(self, db: &dyn Db) -> &PathBuf {
        self.interned_path(db)
    }
}

fn normalized_source_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut ancestor = absolute.clone();
    let mut suffix = Vec::<OsString>::new();

    loop {
        if let Ok(mut canonical) = ancestor.canonicalize() {
            for component in suffix.iter().rev() {
                canonical.push(component);
            }
            return canonical;
        }
        let Some(leaf) = ancestor.file_name().map(ToOwned::to_owned) else {
            return absolute;
        };
        suffix.push(leaf);
        if !ancestor.pop() {
            return absolute;
        }
    }
}

/// Generation-safe key for a syntax node in an interned source unit.
pub type AstNodeKey = beskid_analysis::syntax::AstNodeKey<SourceUnitId>;

/// Typed frontend contract passed to later semantic consumers.
#[derive(Clone)]
pub struct TypedProgram {
    pub project: ProjectSession,
    pub entry: SourceUnitId,
    pub generation: SyntaxGenerationId,
    pub assembly: Arc<SyntaxProgramAssembly>,
}

/// Authoritative Salsa input for the current syntax generation of one source unit.
#[salsa::input]
pub struct SyntaxUnitInput {
    pub(crate) project: ProjectSession,
    pub(crate) unit: SourceUnitId,
    #[returns(ref)]
    pub(crate) revision: Arc<SyntaxUnitRevision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxUnitRevision {
    pub(crate) generation: SyntaxGenerationId,
    pub(crate) expanded_program:
        Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>>,
    pub(crate) syntax_index: Arc<beskid_analysis::syntax_query::SyntaxIndex>,
    pub(crate) source_fingerprint: Arc<str>,
    pub(crate) tree_fingerprint: Arc<str>,
    pub(crate) source_fingerprint_history: Arc<[Arc<str>]>,
    pub(crate) tree_fingerprint_history: Arc<[Arc<str>]>,
}

impl SyntaxUnitInput {
    pub(crate) fn generation(self, db: &dyn Db) -> SyntaxGenerationId {
        self.revision(db).generation
    }

    pub(crate) fn expanded_program(
        self,
        db: &dyn Db,
    ) -> &Arc<beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>> {
        &self.revision(db).expanded_program
    }

    pub(crate) fn syntax_index(
        self,
        db: &dyn Db,
    ) -> &Arc<beskid_analysis::syntax_query::SyntaxIndex> {
        &self.revision(db).syntax_index
    }

    pub(crate) fn source_fingerprint(self, db: &dyn Db) -> &Arc<str> {
        &self.revision(db).source_fingerprint
    }

    /// Whether `key` belongs to this authoritative unit revision.
    pub fn accepts_key(self, db: &dyn Db, key: AstNodeKey) -> bool {
        key.is_current(self.unit(db), self.generation(db))
    }
}

/// Resolution fact for an item reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedItem {
    pub declaration: AstNodeKey,
}

/// Resolution fact for a local reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedLocal {
    pub declaration: AstNodeKey,
}

/// Opaque semantic type identity owned by the query layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SemanticTypeId(pub u32);

impl SemanticTypeId {
    pub const UNIT: Self = Self(0);
    pub const BOOL: Self = Self(1);
    pub const I32: Self = Self(2);
    pub const I64: Self = Self(3);
    pub const U8: Self = Self(4);
    pub const F64: Self = Self(5);
    pub const CHAR: Self = Self(6);
    pub const STRING: Self = Self(7);
}

/// Backend-relevant call classification, detached from legacy HIR nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallLowering {
    Direct(AstNodeKey),
    Dynamic,
    Runtime(RuntimeIntrinsic),
}

/// One semantic cast required while lowering an AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CastIntent {
    pub from: SemanticTypeId,
    pub to: SemanticTypeId,
}

/// Control-flow facts established for one AST node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlFlow {
    pub may_fall_through: bool,
}

/// Callable item signature expressed entirely in semantic type identities.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ItemSignature {
    pub parameters: Arc<[SemanticTypeId]>,
    pub result: SemanticTypeId,
}

/// Trusted runtime operation selected by semantic analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuntimeIntrinsic(pub u32);

pub type IndexedNodeKind = beskid_analysis::syntax_query::NodeKind;
pub type SourceSpan = beskid_analysis::syntax::SpanInfo;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiteralFact {
    Integer(Arc<str>),
    Float(Arc<str>),
    String(Arc<str>),
    Char(Arc<str>),
    Bool(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperatorFact {
    Or,
    And,
    IdentityEq,
    IdentityNotEq,
    Eq,
    NotEq,
    Lt,
    Lte,
    Gt,
    Gte,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct SemanticError {
    message: Arc<str>,
    diagnostics: Arc<[Arc<str>]>,
    unavailable: bool,
}

impl SemanticError {
    pub(crate) fn new(message: impl Into<Arc<str>>) -> Self {
        let message = message.into();
        Self {
            diagnostics: Arc::from([Arc::clone(&message)]),
            message,
            unavailable: false,
        }
    }

    pub(crate) fn from_diagnostics(messages: impl IntoIterator<Item = String>) -> Self {
        let diagnostics = messages
            .into_iter()
            .map(Arc::<str>::from)
            .collect::<Vec<_>>();
        let message = diagnostics
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            message: Arc::from(message),
            diagnostics: diagnostics.into(),
            unavailable: false,
        }
    }

    pub fn unavailable(query: &str) -> Self {
        let message = Arc::<str>::from(format!(
            "semantic query `{query}` is unavailable until its AST/Salsa port is complete"
        ));
        Self {
            diagnostics: Arc::from([Arc::clone(&message)]),
            message,
            unavailable: true,
        }
    }

    pub fn is_unavailable(&self) -> bool {
        self.unavailable
    }

    pub fn diagnostics(&self) -> &[Arc<str>] {
        &self.diagnostics
    }
}

fn unavailable_for_current_key<T>(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    query: &str,
) -> SemanticQueryResult<T> {
    if !syntax.accepts_key(db, key)
        || syntax
            .syntax_index(db)
            .metadata_for(key.generation, key.node)
            .is_none()
    {
        return Ok(None);
    }
    Err(SemanticError::unavailable(query))
}

pub type SemanticQueryResult<T> = Result<Option<T>, SemanticError>;

fn with_registered_syntax<T>(
    db: &dyn Db,
    key: AstNodeKey,
    query: impl FnOnce(&dyn Db, SyntaxUnitInput, AstNodeKey) -> SemanticQueryResult<T>,
) -> SemanticQueryResult<T> {
    let Some(syntax) = db.syntax_unit(key.unit) else {
        return Ok(None);
    };
    query(db, syntax, key)
}

#[salsa::tracked]
fn resolved_item_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ResolvedItem> {
    unavailable_for_current_key(db, syntax, key, "resolved_item")
}

#[salsa::tracked]
fn resolved_local_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ResolvedLocal> {
    with_node(db, syntax, key, |program, index, node| {
        let path = node.of::<beskid_analysis::syntax::PathExpression>()?;
        let [segment] = path.path.node.segments.as_slice() else {
            return None;
        };
        if !segment.node.type_args.is_empty() {
            return None;
        }
        let declaration = resolve_lexical_declaration(
            program,
            index,
            key.node,
            segment.node.name.node.name.as_str(),
        )?;
        Some(ResolvedLocal {
            declaration: AstNodeKey {
                node: declaration,
                ..key
            },
        })
    })
}

fn resolve_lexical_declaration(
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    reference: beskid_analysis::syntax::AstNodeId,
    name: &str,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut best: Option<(usize, u32, beskid_analysis::syntax::AstNodeId)> = None;
    for declaration in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::Identifier) {
        let Some(identifier) = index
            .node_at(program, declaration)
            .and_then(|node| node.of::<beskid_analysis::syntax::Identifier>())
        else {
            continue;
        };
        if identifier.name != name {
            continue;
        }
        let Some(scope) = local_declaration_scope(index, declaration, reference) else {
            continue;
        };
        let Some(distance) = ancestor_distance(index, scope, reference) else {
            continue;
        };
        let rank = (distance, u32::MAX - declaration.0, declaration);
        if best.is_none_or(|current| rank < current) {
            best = Some(rank);
        }
    }
    best.map(|(_, _, declaration)| declaration)
}

fn local_declaration_scope(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    declaration: beskid_analysis::syntax::AstNodeId,
    reference: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let parent = parent_node(index, declaration)?;
    match index.kind(parent)? {
        beskid_analysis::syntax_query::NodeKind::LetStatement => {
            if declaration.0 >= reference.0 || is_ancestor(index, parent, reference) {
                return None;
            }
            nearest_ancestor(index, parent, |kind| {
                kind == beskid_analysis::syntax_query::NodeKind::Block
            })
            .filter(|scope| is_ancestor(index, *scope, reference))
        }
        beskid_analysis::syntax_query::NodeKind::Parameter => {
            nearest_ancestor(index, parent, |kind| {
                matches!(
                    kind,
                    beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                        | beskid_analysis::syntax_query::NodeKind::MethodDefinition
                )
            })
            .filter(|scope| is_ancestor(index, *scope, reference))
        }
        beskid_analysis::syntax_query::NodeKind::LambdaParameter => {
            nearest_ancestor(index, parent, |kind| {
                kind == beskid_analysis::syntax_query::NodeKind::LambdaExpression
            })
            .filter(|scope| is_ancestor(index, *scope, reference))
        }
        beskid_analysis::syntax_query::NodeKind::ForStatement => index
            .children(parent)?
            .iter()
            .copied()
            .find(|child| {
                index.kind(*child) == Some(beskid_analysis::syntax_query::NodeKind::Block)
            })
            .filter(|scope| is_ancestor(index, *scope, reference)),
        beskid_analysis::syntax_query::NodeKind::Pattern => {
            nearest_ancestor(index, parent, |kind| {
                kind == beskid_analysis::syntax_query::NodeKind::MatchArm
            })
            .filter(|scope| is_ancestor(index, *scope, reference))
        }
        _ => None,
    }
}

fn parent_node(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    index.metadata().get(node.0 as usize)?.parent
}

fn nearest_ancestor(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    node: beskid_analysis::syntax::AstNodeId,
    predicate: impl Fn(beskid_analysis::syntax_query::NodeKind) -> bool,
) -> Option<beskid_analysis::syntax::AstNodeId> {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if index.kind(candidate).is_some_and(&predicate) {
            return Some(candidate);
        }
        current = parent_node(index, candidate);
    }
    None
}

fn is_ancestor(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    ancestor: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> bool {
    ancestor_distance(index, ancestor, node).is_some()
}

fn ancestor_distance(
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    ancestor: beskid_analysis::syntax::AstNodeId,
    node: beskid_analysis::syntax::AstNodeId,
) -> Option<usize> {
    let mut current = Some(node);
    let mut distance = 0usize;
    while let Some(candidate) = current {
        if candidate == ancestor {
            return Some(distance);
        }
        current = parent_node(index, candidate);
        distance += 1;
    }
    None
}

#[salsa::tracked]
fn node_type_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SemanticTypeId> {
    unavailable_for_current_key(db, syntax, key, "node_type")
}

#[salsa::tracked]
fn call_lowering_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<CallLowering> {
    with_node(db, syntax, key, |_program, _index, node| {
        call_lowering_for_node(node)
    })?
    .transpose()
}

fn call_lowering_for_node(
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<CallLowering, SemanticError>> {
    let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
    Some(if expression_is_lambda(&call.callee.node) {
        Ok(CallLowering::Dynamic)
    } else {
        Err(SemanticError::unavailable("call_lowering"))
    })
}

fn expression_is_lambda(expression: &beskid_analysis::syntax::Expression) -> bool {
    match expression {
        beskid_analysis::syntax::Expression::Lambda(_) => true,
        beskid_analysis::syntax::Expression::Grouped(grouped) => {
            expression_is_lambda(&grouped.node.expr.node)
        }
        _ => false,
    }
}

#[salsa::tracked]
fn cast_intents_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[CastIntent]>> {
    unavailable_for_current_key(db, syntax, key, "cast_intents")
}

#[salsa::tracked]
fn control_flow_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ControlFlow> {
    with_node(db, syntax, key, |_program, _index, node| {
        control_flow_for_node(node).map(|may_fall_through| ControlFlow { may_fall_through })
    })
}

fn control_flow_for_node(node: beskid_analysis::syntax_query::DynNodeRef<'_>) -> Option<bool> {
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return Some(block_may_fall_through(&function.body.node));
    }
    if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        return Some(block_may_fall_through(&method.body.node));
    }
    if let Some(test) = node.of::<beskid_analysis::syntax::TestDefinition>() {
        return Some(statements_may_fall_through(&test.statements));
    }
    if let Some(block) = node.of::<beskid_analysis::syntax::Block>() {
        return Some(block_may_fall_through(block));
    }
    if let Some(statement) = node.of::<beskid_analysis::syntax::Statement>() {
        return Some(statement_may_fall_through(statement));
    }
    if let Some(if_statement) = node.of::<beskid_analysis::syntax::IfStatement>() {
        return Some(if_may_fall_through(if_statement));
    }
    if let Some(with_statement) = node.of::<beskid_analysis::syntax::WithStatement>() {
        return Some(block_may_fall_through(&with_statement.body.node));
    }
    if node
        .of::<beskid_analysis::syntax::ReturnStatement>()
        .is_some()
        || node
            .of::<beskid_analysis::syntax::BreakStatement>()
            .is_some()
        || node
            .of::<beskid_analysis::syntax::ContinueStatement>()
            .is_some()
    {
        return Some(false);
    }
    None
}

fn block_may_fall_through(block: &beskid_analysis::syntax::Block) -> bool {
    statements_may_fall_through(&block.statements)
}

fn statements_may_fall_through(
    statements: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Statement>],
) -> bool {
    statements
        .iter()
        .all(|statement| statement_may_fall_through(&statement.node))
}

fn statement_may_fall_through(statement: &beskid_analysis::syntax::Statement) -> bool {
    match statement {
        beskid_analysis::syntax::Statement::Return(_)
        | beskid_analysis::syntax::Statement::Break(_)
        | beskid_analysis::syntax::Statement::Continue(_) => false,
        beskid_analysis::syntax::Statement::If(if_statement) => {
            if_may_fall_through(&if_statement.node)
        }
        beskid_analysis::syntax::Statement::With(with_statement) => {
            block_may_fall_through(&with_statement.node.body.node)
        }
        beskid_analysis::syntax::Statement::Let(_)
        | beskid_analysis::syntax::Statement::While(_)
        | beskid_analysis::syntax::Statement::For(_)
        | beskid_analysis::syntax::Statement::Launch(_)
        | beskid_analysis::syntax::Statement::Expression(_) => true,
    }
}

fn if_may_fall_through(if_statement: &beskid_analysis::syntax::IfStatement) -> bool {
    if block_may_fall_through(&if_statement.then_block.node) {
        return true;
    }
    let Some(else_branch) = &if_statement.else_branch else {
        return true;
    };
    match &else_branch.node {
        beskid_analysis::syntax::ElseBranch::If(nested) => if_may_fall_through(&nested.node),
        beskid_analysis::syntax::ElseBranch::Block(block) => block_may_fall_through(&block.node),
    }
}

#[salsa::tracked]
fn item_signature_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ItemSignature> {
    with_node(db, syntax, key, |_program, _index, node| {
        item_signature_for_node(node)
    })?
    .transpose()
}

fn item_signature_for_node(
    node: beskid_analysis::syntax_query::DynNodeRef<'_>,
) -> Option<Result<ItemSignature, SemanticError>> {
    if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
        return Some(signature_from_syntax(
            &function.parameters,
            function.return_type.as_ref(),
        ));
    }
    if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
        return Some(signature_from_syntax(
            &method.parameters,
            method.return_type.as_ref(),
        ));
    }
    if let Some(contract) = node.of::<beskid_analysis::syntax::ContractMethodSignature>() {
        return Some(signature_from_syntax(
            &contract.parameters,
            contract.return_type.as_ref(),
        ));
    }
    None
}

fn signature_from_syntax(
    parameters: &[beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Parameter>],
    return_type: Option<&beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Type>>,
) -> Result<ItemSignature, SemanticError> {
    let parameters = parameters
        .iter()
        .map(|parameter| semantic_type_from_syntax(&parameter.node.ty.node))
        .collect::<Result<Vec<_>, _>>()?;
    let result = return_type.map_or(Ok(SemanticTypeId::UNIT), |return_type| {
        semantic_type_from_syntax(&return_type.node)
    })?;
    Ok(ItemSignature {
        parameters: parameters.into(),
        result,
    })
}

fn semantic_type_from_syntax(
    syntax_type: &beskid_analysis::syntax::Type,
) -> Result<SemanticTypeId, SemanticError> {
    use beskid_analysis::syntax::{PrimitiveType, Type};

    match syntax_type {
        Type::Primitive(primitive) => Ok(match primitive.node {
            PrimitiveType::Bool => SemanticTypeId::BOOL,
            PrimitiveType::I32 => SemanticTypeId::I32,
            PrimitiveType::I64 => SemanticTypeId::I64,
            PrimitiveType::U8 => SemanticTypeId::U8,
            PrimitiveType::F64 => SemanticTypeId::F64,
            PrimitiveType::Char => SemanticTypeId::CHAR,
            PrimitiveType::String => SemanticTypeId::STRING,
            PrimitiveType::Unit => SemanticTypeId::UNIT,
        }),
        Type::Complex(_) | Type::Array(_) | Type::Function { .. } => {
            Err(SemanticError::unavailable("item_signature"))
        }
    }
}

#[salsa::tracked]
fn runtime_intrinsic_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<RuntimeIntrinsic> {
    unavailable_for_current_key(db, syntax, key, "runtime_intrinsic")
}

#[salsa::tracked]
fn node_kind_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<IndexedNodeKind> {
    with_node(db, syntax, key, |_program, _index, node| {
        Some(node.node_kind())
    })
}

#[salsa::tracked]
fn child_nodes_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |_program, index, _node| {
        Some(
            index
                .children(key.node)?
                .iter()
                .map(|node| AstNodeKey { node: *node, ..key })
                .collect::<Vec<_>>()
                .into(),
        )
    })
}

#[salsa::tracked]
fn literal_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<LiteralFact> {
    with_node(db, syntax, key, |_program, _index, node| {
        let literal = node.of::<beskid_analysis::syntax::Literal>()?;
        Some(match literal {
            beskid_analysis::syntax::Literal::Integer(value) => {
                LiteralFact::Integer(Arc::from(value.as_str()))
            }
            beskid_analysis::syntax::Literal::Float(value) => {
                LiteralFact::Float(Arc::from(value.as_str()))
            }
            beskid_analysis::syntax::Literal::String(value) => {
                LiteralFact::String(Arc::from(value.as_str()))
            }
            beskid_analysis::syntax::Literal::Char(value) => {
                LiteralFact::Char(Arc::from(value.as_str()))
            }
            beskid_analysis::syntax::Literal::Bool(value) => LiteralFact::Bool(*value),
        })
    })
}

#[salsa::tracked]
fn node_span_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SourceSpan> {
    with_node(db, syntax, key, |_program, _index, node| node.span())
}

#[salsa::tracked]
fn operator_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<OperatorFact> {
    with_node(db, syntax, key, |_program, _index, node| {
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return Some(binary_operator(binary.op.node));
        }
        if let Some(unary) = node.of::<beskid_analysis::syntax::UnaryExpression>() {
            return Some(unary_operator(unary.op.node));
        }
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryOp>() {
            return Some(binary_operator(*binary));
        }
        node.of::<beskid_analysis::syntax::UnaryOp>()
            .copied()
            .map(unary_operator)
    })
}

fn binary_operator(operator: beskid_analysis::syntax::BinaryOp) -> OperatorFact {
    match operator {
        beskid_analysis::syntax::BinaryOp::Or => OperatorFact::Or,
        beskid_analysis::syntax::BinaryOp::And => OperatorFact::And,
        beskid_analysis::syntax::BinaryOp::IdentityEq => OperatorFact::IdentityEq,
        beskid_analysis::syntax::BinaryOp::IdentityNotEq => OperatorFact::IdentityNotEq,
        beskid_analysis::syntax::BinaryOp::Eq => OperatorFact::Eq,
        beskid_analysis::syntax::BinaryOp::NotEq => OperatorFact::NotEq,
        beskid_analysis::syntax::BinaryOp::Lt => OperatorFact::Lt,
        beskid_analysis::syntax::BinaryOp::Lte => OperatorFact::Lte,
        beskid_analysis::syntax::BinaryOp::Gt => OperatorFact::Gt,
        beskid_analysis::syntax::BinaryOp::Gte => OperatorFact::Gte,
        beskid_analysis::syntax::BinaryOp::Add => OperatorFact::Add,
        beskid_analysis::syntax::BinaryOp::Sub => OperatorFact::Sub,
        beskid_analysis::syntax::BinaryOp::Mul => OperatorFact::Mul,
        beskid_analysis::syntax::BinaryOp::Div => OperatorFact::Div,
        beskid_analysis::syntax::BinaryOp::Mod => OperatorFact::Mod,
    }
}

fn unary_operator(operator: beskid_analysis::syntax::UnaryOp) -> OperatorFact {
    match operator {
        beskid_analysis::syntax::UnaryOp::Neg => OperatorFact::Neg,
        beskid_analysis::syntax::UnaryOp::Not => OperatorFact::Not,
    }
}

#[salsa::tracked]
fn item_body_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            return index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(&function.body),
                )
                .map(|node| AstNodeKey { node, ..key });
        }
        if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
            return index
                .direct_child_id(
                    program,
                    key.node,
                    beskid_analysis::syntax_query::DynNodeRef::from(&method.body),
                )
                .map(|node| AstNodeKey { node, ..key });
        }
        None
    })
}

#[salsa::tracked]
fn direct_callees_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    unavailable_for_current_key(db, syntax, key, "direct_callees")
}

#[salsa::tracked]
fn reachable_items_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    program: AstNodeKey,
    entry: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    if !syntax.accepts_key(db, program)
        || syntax
            .syntax_index(db)
            .metadata_for(program.generation, program.node)
            .is_none()
    {
        return Ok(None);
    }
    let Some(entry_syntax) = db.syntax_unit(entry.unit) else {
        return Ok(None);
    };
    if !entry_syntax.accepts_key(db, entry)
        || entry_syntax
            .syntax_index(db)
            .metadata_for(entry.generation, entry.node)
            .is_none()
    {
        return Ok(None);
    }
    Err(SemanticError::unavailable("reachable_items"))
}

fn with_node<T>(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
    query: impl FnOnce(
        &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
        &beskid_analysis::syntax_query::SyntaxIndex,
        beskid_analysis::syntax_query::DynNodeRef<'_>,
    ) -> Option<T>,
) -> SemanticQueryResult<T> {
    if !syntax.accepts_key(db, key) {
        return Ok(None);
    }
    let expanded = syntax.expanded_program(db);
    let index = syntax.syntax_index(db);
    if index.generation() != key.generation
        || index.metadata_for(key.generation, key.node).is_none()
    {
        return Ok(None);
    }
    let Some(node) = index.node_at(expanded, key.node) else {
        return Ok(None);
    };
    Ok(query(expanded, index, node))
}

pub fn resolved_item(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ResolvedItem> {
    with_registered_syntax(db, key, resolved_item_tracked)
}

/// Resolve a single-segment value path to its generation-safe lexical declaration key.
///
/// Function and method parameters, lets, lambda parameters, for iterators, and match bindings are
/// supported. Out-of-scope, self-initializing, qualified, generic, and unresolved paths contain no
/// local fact.
pub fn resolved_local(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ResolvedLocal> {
    with_registered_syntax(db, key, resolved_local_tracked)
}

pub fn node_type(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SemanticTypeId> {
    with_registered_syntax(db, key, node_type_tracked)
}

/// Classify call shapes whose lowering is certain from expanded syntax alone.
///
/// Immediate lambda calls are dynamic. Named, member, runtime, and other call shapes remain
/// unavailable until their resolution and type facts are ported. Stale, unregistered, and
/// non-call nodes contain no fact.
pub fn call_lowering(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<CallLowering> {
    with_registered_syntax(db, key, call_lowering_tracked)
}

/// Current keys report Task-2 unavailability; stale or unregistered keys contain no fact.
pub fn cast_intents(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[CastIntent]>> {
    with_registered_syntax(db, key, cast_intents_tracked)
}

/// Return AST-derived fall-through facts for executable nodes in the current generation.
///
/// Loops are conservative because their body may execute zero times. Stale, unregistered, and
/// non-executable nodes contain no fact.
pub fn control_flow(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ControlFlow> {
    with_registered_syntax(db, key, control_flow_tracked)
}

/// Return exact callable signatures whose types have generation-independent primitive identities.
///
/// Complex, array, and function types remain unavailable until their Salsa type identities are
/// ported. Stale, unregistered, and non-callable nodes contain no fact.
pub fn item_signature(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<ItemSignature> {
    with_registered_syntax(db, key, item_signature_tracked)
}

/// Current keys report Task-2 unavailability; stale or unregistered keys contain no fact.
pub fn runtime_intrinsic(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<RuntimeIntrinsic> {
    with_registered_syntax(db, key, runtime_intrinsic_tracked)
}

pub fn node_kind(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<IndexedNodeKind> {
    with_registered_syntax(db, key, node_kind_tracked)
}

pub fn child_nodes(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, child_nodes_tracked)
}

pub fn literal_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<LiteralFact> {
    with_registered_syntax(db, key, literal_fact_tracked)
}

pub fn node_span(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<SourceSpan> {
    with_registered_syntax(db, key, node_span_tracked)
}

pub fn operator_fact(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<OperatorFact> {
    with_registered_syntax(db, key, operator_fact_tracked)
}

pub fn item_body(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<AstNodeKey> {
    with_registered_syntax(db, key, item_body_tracked)
}

pub fn direct_callees(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_registered_syntax(db, key, direct_callees_tracked)
}

pub fn reachable_items(
    db: &dyn Db,
    program: AstNodeKey,
    entry: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    let Some(syntax) = db.syntax_unit(program.unit) else {
        return Ok(None);
    };
    reachable_items_tracked(db, syntax, program, entry)
}
