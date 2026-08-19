//! Focused semantic-contract implementation cluster.

use super::*;

#[salsa::tracked(persist)]
pub(super) fn node_kind_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<IndexedNodeKind> {
    with_node(db, syntax, key, |_program, _index, node| Some(node.node_kind()))
}

#[salsa::tracked(persist)]
pub(super) fn child_nodes_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |_program, index, _node| {
        Some(index.children(key.node)?.iter().map(|node| AstNodeKey { node: *node, ..key }).collect::<Vec<_>>().into())
    })
}

#[salsa::tracked(persist)]
pub(super) fn literal_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<LiteralFact> {
    with_node(db, syntax, key, |_program, _index, node| {
        let literal = node.of::<beskid_analysis::syntax::Literal>()?;
        match literal {
            beskid_analysis::syntax::Literal::Integer(value) => Some(LiteralFact::Integer(Arc::from(value.as_str()))),
            beskid_analysis::syntax::Literal::Float(value) => Some(LiteralFact::Float(Arc::from(value.as_str()))),
            beskid_analysis::syntax::Literal::String(value) => Some(LiteralFact::String(Arc::from(value.as_str()))),
            beskid_analysis::syntax::Literal::Char(value) => Some(LiteralFact::Char(Arc::from(value.as_str()))),
            beskid_analysis::syntax::Literal::Bool(value) => Some(LiteralFact::Bool(*value)),
            beskid_analysis::syntax::Literal::Unit => None,
        }
    })
}

#[salsa::tracked(persist)]
pub(super) fn clif_block_body_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<str>> {
    with_node(db, syntax, key, |_program, _index, node| {
        let clif = node.of::<beskid_analysis::syntax::ClifBlockExpression>()?;
        Some(Arc::from(clif.body.as_str()))
    })
}

#[salsa::tracked(persist)]
pub(super) fn node_span_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<SourceSpan> {
    with_node(db, syntax, key, |_program, _index, node| node.span())
}

/// One built-in dispatch symbol resolved from syntax. The wrapped `&'static str` is borrowed
/// from the compile-time [`beskid_analysis::builtins`] table, so it cannot round-trip through
/// `serde_json` directly. Manual [`Serialize`]/[`Deserialize`] implementations emit the symbol as
/// an owned string and recover the canonical `&'static str` by matching against
/// [`beskid_analysis::builtins::builtin_specs`], failing closed with a serde error when no entry
/// matches (a tampered or unknown symbol).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DispatchBuiltinSymbol(pub &'static str);

impl serde::Serialize for DispatchBuiltinSymbol {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for DispatchBuiltinSymbol {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let symbol = beskid_abi::serde_support::recover_static_str(deserializer, "dispatch builtin symbol", |value| {
            beskid_analysis::builtins::builtin_specs()
                .iter()
                .find(|spec| spec.runtime_symbol == value)
                .map(|spec| spec.runtime_symbol)
        })?;
        Ok(DispatchBuiltinSymbol(symbol))
    }
}

#[salsa::tracked(persist)]
pub(super) fn dispatch_builtin_symbol_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<DispatchBuiltinSymbol> {
    with_node(db, syntax, key, |program, index, node| {
        let call = node.of::<beskid_analysis::syntax::CallExpression>()?;
        let lowering = call_lowering_for_node(db, program, index, key, node).and_then(|result| result.ok())?;
        if lowering != CallLowering::Dynamic {
            return None;
        }
        let beskid_analysis::syntax::Expression::Path(path) = &call.callee.node else {
            return None;
        };
        if path.node.path.node.segments.len() != 1 {
            return None;
        }
        let name = path.node.path.node.segments[0].node.name.node.name.as_str();
        let (_, spec) = beskid_analysis::builtins::builtin_for_path(&[name.to_owned()])?;
        let target = TargetMetadata::supported().into_iter().next()?;
        AbiManifestV5::canonical_runtime(target).intrinsic_metadata(spec.runtime_symbol)?;
        Some(Ok(DispatchBuiltinSymbol(spec.runtime_symbol)))
    })?
    .transpose()
}

pub fn dispatch_builtin_symbol(db: &dyn Db, key: AstNodeKey) -> SemanticQueryResult<DispatchBuiltinSymbol> {
    with_registered_syntax(db, key, dispatch_builtin_symbol_tracked)
}

#[salsa::tracked(persist)]
pub(super) fn operator_fact_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<OperatorFact> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryExpression>() {
            return operator_fact_for_binary(db, program, index, key, binary);
        }
        if let Some(unary) = node.of::<beskid_analysis::syntax::UnaryExpression>() {
            return Some(unary_operator(unary.op.node));
        }
        if let Some(binary) = node.of::<beskid_analysis::syntax::BinaryOp>() {
            return Some(binary_operator(*binary));
        }
        node.of::<beskid_analysis::syntax::UnaryOp>().copied().map(unary_operator)
    })
}

pub(super) fn binary_operator(operator: beskid_analysis::syntax::BinaryOp) -> OperatorFact {
    match operator {
        beskid_analysis::syntax::BinaryOp::Or => OperatorFact::Or,
        beskid_analysis::syntax::BinaryOp::And => OperatorFact::And,
        beskid_analysis::syntax::BinaryOp::BitOr => OperatorFact::BitOr,
        beskid_analysis::syntax::BinaryOp::BitAnd => OperatorFact::BitAnd,
        beskid_analysis::syntax::BinaryOp::Shl => OperatorFact::Shl,
        beskid_analysis::syntax::BinaryOp::Shr => OperatorFact::Shr,
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

pub(super) fn operator_fact_for_binary(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    key: AstNodeKey,
    binary: &beskid_analysis::syntax::BinaryExpression,
) -> Option<OperatorFact> {
    let left = index.direct_child_id(
        program,
        key.node,
        beskid_analysis::syntax_query::DynNodeRef::from(binary.left.as_ref()),
    )?;
    let right = index.direct_child_id(
        program,
        key.node,
        beskid_analysis::syntax_query::DynNodeRef::from(binary.right.as_ref()),
    )?;
    let left_key = AstNodeKey { node: left, ..key };
    let right_key = AstNodeKey { node: right, ..key };
    let left_type = abi_type(db, left_key).ok().flatten();
    let right_type = abi_type(db, right_key).ok().flatten();
    let result_type = abi_type(db, key).ok().flatten();
    let involves_string = [left_type, right_type, result_type].into_iter().any(|ty| ty == Some(SemanticTypeId::STRING));
    if involves_string {
        return Some(match binary.op.node {
            beskid_analysis::syntax::BinaryOp::Add => OperatorFact::StringAdd,
            beskid_analysis::syntax::BinaryOp::Eq => OperatorFact::StringEq,
            beskid_analysis::syntax::BinaryOp::NotEq => OperatorFact::StringNotEq,
            op => binary_operator(op),
        });
    }
    Some(binary_operator(binary.op.node))
}

pub(super) fn unary_operator(operator: beskid_analysis::syntax::UnaryOp) -> OperatorFact {
    match operator {
        beskid_analysis::syntax::UnaryOp::Neg => OperatorFact::Neg,
        beskid_analysis::syntax::UnaryOp::Not => OperatorFact::Not,
    }
}

#[salsa::tracked(persist)]
pub(super) fn item_body_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<AstNodeKey> {
    with_node(db, syntax, key, |program, index, node| {
        if let Some(function) = node.of::<beskid_analysis::syntax::FunctionDefinition>() {
            return index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&function.body))
                .map(|node| AstNodeKey { node, ..key });
        }
        if let Some(method) = node.of::<beskid_analysis::syntax::MethodDefinition>() {
            return index
                .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(&method.body))
                .map(|node| AstNodeKey { node, ..key });
        }
        if node.of::<beskid_analysis::syntax::TestDefinition>().is_some() {
            return Some(key);
        }
        None
    })
}

/// Return the executable statements of a test item in source order.
///
/// A test definition also owns visibility, name, and optional metadata children.  ISLE function
/// emission must enumerate only its statement body, never those declaration children.
#[salsa::tracked(persist)]
pub(super) fn test_statement_nodes_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |program, index, node| {
        let test = node.of::<beskid_analysis::syntax::TestDefinition>()?;
        Some(
            test.statements
                .iter()
                .map(|statement| {
                    let wrapper = index
                        .direct_child_id(program, key.node, beskid_analysis::syntax_query::DynNodeRef::from(statement))
                        .ok_or_else(|| SemanticError::unavailable("test_statement_nodes"))?;
                    let children =
                        index.children(wrapper).ok_or_else(|| SemanticError::unavailable("test_statement_nodes"))?;
                    let [statement] = children else {
                        return Err(SemanticError::unavailable("test_statement_nodes"));
                    };
                    Ok(AstNodeKey { node: *statement, ..key })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Arc::from),
        )
    })?
    .transpose()
}

/// Return executable block statements without syntax-index wrapper nodes.
#[salsa::tracked(persist)]
pub(super) fn block_statement_nodes_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |program, index, node| {
        let (block_key, block) = if let Some(block) = node.of::<beskid_analysis::syntax::Block>() {
            (key.node, block)
        } else {
            let expression = node.of::<beskid_analysis::syntax::BlockExpression>()?;
            let block_key = index.direct_child_id(
                program,
                key.node,
                beskid_analysis::syntax_query::DynNodeRef::from(&expression.block),
            )?;
            let block = index.node_at(program, block_key)?.of::<beskid_analysis::syntax::Block>()?;
            (block_key, block)
        };
        Some(
            block
                .statements
                .iter()
                .map(|statement| {
                    let wrapper = index
                        .direct_child_id(program, block_key, beskid_analysis::syntax_query::DynNodeRef::from(statement))
                        .ok_or_else(|| SemanticError::unavailable("block_statement_nodes"))?;
                    let [statement] =
                        index.children(wrapper).ok_or_else(|| SemanticError::unavailable("block_statement_nodes"))?
                    else {
                        return Err(SemanticError::unavailable("block_statement_nodes"));
                    };
                    Ok(AstNodeKey { node: *statement, ..key })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Arc::from),
        )
    })?
    .transpose()
}

#[salsa::tracked(persist)]
pub(super) fn item_name_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<str>> {
    with_node(db, syntax, key, |_program, _index, node| {
        node.of::<beskid_analysis::syntax::FunctionDefinition>()
            .map(|definition| Arc::from(definition.name.node.name.as_str()))
            .or_else(|| {
                node.of::<beskid_analysis::syntax::MethodDefinition>()
                    .map(|definition| Arc::from(definition.name.node.name.as_str()))
            })
            .or_else(|| {
                node.of::<beskid_analysis::syntax::TestDefinition>()
                    .map(|definition| Arc::from(definition.name.node.name.as_str()))
            })
            .or_else(|| {
                node.of::<beskid_analysis::syntax::ContractMethodSignature>()
                    .map(|definition| Arc::from(definition.name.node.name.as_str()))
            })
    })
}

#[salsa::tracked(persist)]
pub(super) fn item_export_symbol_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<ExportSymbol> {
    with_node(db, syntax, key, |_program, _index, node| {
        let definition = node.of::<beskid_analysis::syntax::FunctionDefinition>()?;
        let export = definition.attributes.iter().find(|attribute| attribute.node.name.node.name == "Export")?;
        if definition.visibility.node != beskid_analysis::syntax::Visibility::Public {
            return Some(Err(SemanticError::new("`[Export]` applies to `pub` functions only")));
        }
        let raw = export.node.arguments.iter().find_map(|argument| {
            if argument.node.name.node.name != "Symbol" {
                return None;
            }
            let beskid_analysis::syntax::Expression::Literal(literal) = &argument.node.value.node else {
                return None;
            };
            let beskid_analysis::syntax::Literal::String(value) = &literal.node.literal.node else {
                return None;
            };
            value.strip_prefix('"')?.strip_suffix('"')
        })?;
        Some(Ok(ExportSymbol(Arc::from(raw))))
    })
    .and_then(|export| export.transpose())
}

#[salsa::tracked(persist)]
pub(super) fn test_item_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<TestItem> {
    with_node(db, syntax, key, |_program, _index, node| {
        let definition = node.of::<beskid_analysis::syntax::TestDefinition>()?;
        let mut module_path = Vec::new();
        let mut parent = parent_node(_index, key.node);
        while let Some(current) = parent {
            if let Some(module) =
                _index.node_at(_program, current).and_then(|node| node.of::<beskid_analysis::syntax::InlineModule>())
            {
                module_path.push(module.name.node.name.clone());
            }
            parent = parent_node(_index, current);
        }
        module_path.reverse();
        let qualified_name = if module_path.is_empty() {
            definition.name.node.name.clone()
        } else {
            format!("{}::{}", module_path.join("::"), definition.name.node.name)
        };
        let mut tags = Vec::new();
        let mut group = None;
        if let Some(meta) = &definition.meta {
            for entry in &meta.node.entries {
                match entry.node.name.node.name.as_str() {
                    "group" => group = test_string_literal(&entry.node.value),
                    "tags" => {
                        tags = test_string_literal(&entry.node.value)
                            .into_iter()
                            .flat_map(|value| {
                                value
                                    .split(',')
                                    .map(str::trim)
                                    .filter(|tag| !tag.is_empty())
                                    .map(Arc::<str>::from)
                                    .collect::<Vec<_>>()
                            })
                            .collect();
                    }
                    _ => {}
                }
            }
        }
        let mut skip_condition = None;
        let mut skip_reason = None;
        if let Some(skip) = &definition.skip {
            for entry in &skip.node.entries {
                match entry.node.name.node.name.as_str() {
                    "condition" => skip_condition = test_bool_literal(&entry.node.value),
                    "reason" => skip_reason = test_string_literal(&entry.node.value),
                    _ => {}
                }
            }
        }
        Some(TestItem {
            name: Arc::from(definition.name.node.name.as_str()),
            qualified_name: Arc::from(qualified_name),
            tags: Arc::from(tags),
            group,
            skip_condition,
            skip_reason,
            selection_span: definition.name.span,
        })
    })
}

pub(super) fn test_string_literal(
    expression: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
) -> Option<Arc<str>> {
    let beskid_analysis::syntax::Expression::Literal(literal) = &expression.node else {
        return None;
    };
    beskid_analysis::syntax::try_decode_string_literal(&literal.node.literal.node).map(Arc::from)
}

pub(super) fn test_bool_literal(
    expression: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Expression>,
) -> Option<bool> {
    let beskid_analysis::syntax::Expression::Literal(literal) = &expression.node else {
        return None;
    };
    let beskid_analysis::syntax::Literal::Bool(value) = &literal.node.literal.node else {
        return None;
    };
    Some(*value)
}

#[salsa::tracked(persist)]
pub(super) fn direct_callees_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    key: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    with_node(db, syntax, key, |program, index, node| {
        if node.of::<beskid_analysis::syntax::FunctionDefinition>().is_none()
            && node.of::<beskid_analysis::syntax::TestDefinition>().is_none()
            && node.of::<beskid_analysis::syntax::MethodDefinition>().is_none()
        {
            return None;
        }
        Some(direct_callees_for_item(db, program, index, key))
    })?
    .transpose()
}

pub(super) fn direct_callees_for_item(
    db: &dyn Db,
    program: &beskid_analysis::syntax::Spanned<beskid_analysis::syntax::Program>,
    index: &beskid_analysis::syntax_query::SyntaxIndex,
    item: AstNodeKey,
) -> Result<Arc<[AstNodeKey]>, SemanticError> {
    let mut callees = Vec::new();
    for call_id in index.ids_of_kind(beskid_analysis::syntax_query::NodeKind::CallExpression) {
        if !is_ancestor(index, item.node, call_id) {
            continue;
        }
        let Some(call_node) = index.node_at(program, call_id) else {
            continue;
        };
        // Reachability enumerates Direct callees only. Unavailable/dynamic call
        // classifications (e.g. extern contract members) are not Direct edges and
        // must not fail closed the whole entrypoint walk.
        let Some(Ok(lowering)) =
            call_lowering_for_node(db, program, index, AstNodeKey { node: call_id, ..item }, call_node)
        else {
            continue;
        };
        if let CallLowering::Direct(declaration) = lowering
            && !callees.contains(&declaration)
        {
            // Extern contract methods are import leaves (no syntax body). Including them
            // as Direct reachability edges makes reachable_items fail closed.
            if extern_contract_import_for_declaration(db, declaration).is_some() {
                continue;
            }
            callees.push(declaration);
        }
    }
    Ok(callees.into())
}

#[salsa::tracked(persist)]
pub(super) fn reachable_items_tracked(
    db: &dyn Db,
    syntax: SyntaxUnitInput,
    program: AstNodeKey,
    entry: AstNodeKey,
) -> SemanticQueryResult<Arc<[AstNodeKey]>> {
    if !syntax.accepts_key(db, program)
        || syntax.syntax_index(db).metadata_for(program.generation, program.node).is_none()
    {
        return Ok(None);
    }
    let Some(entry_syntax) = db.syntax_unit(entry.unit) else {
        return Ok(None);
    };
    if !entry_syntax.accepts_key(db, entry)
        || entry_syntax.syntax_index(db).metadata_for(entry.generation, entry.node).is_none()
    {
        return Ok(None);
    }
    if syntax.syntax_index(db).kind(program.node) != Some(beskid_analysis::syntax_query::NodeKind::Program)
        || !matches!(
            entry_syntax.syntax_index(db).kind(entry.node),
            Some(
                beskid_analysis::syntax_query::NodeKind::FunctionDefinition
                    | beskid_analysis::syntax_query::NodeKind::TestDefinition
            )
        )
    {
        return Ok(None);
    }

    fn visit(db: &dyn Db, item: AstNodeKey, reachable: &mut Vec<AstNodeKey>) -> Result<(), SemanticError> {
        if reachable.contains(&item) {
            return Ok(());
        }
        reachable.push(item);
        let item_syntax = db.syntax_unit(item.unit).ok_or_else(|| SemanticError::unavailable("reachable_items"))?;
        if !item_syntax.accepts_key(db, item) {
            return Err(SemanticError::unavailable("reachable_items"));
        }
        let callees = direct_callees_tracked(db, item_syntax, item)?
            .ok_or_else(|| SemanticError::unavailable("reachable_items"))?;
        for callee in callees.iter().copied() {
            visit(db, callee, reachable)?;
        }
        Ok(())
    }

    let mut reachable = Vec::new();
    visit(db, entry, &mut reachable)?;
    Ok(Some(reachable.into()))
}

pub(super) fn with_node<T>(
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
    if index.generation() != key.generation || index.metadata_for(key.generation, key.node).is_none() {
        return Ok(None);
    }
    let Some(node) = index.node_at(expanded, key.node) else {
        return Ok(None);
    };
    Ok(query(expanded, index, node))
}
