//! Generation-safe Salsa facts consumed by the generated ISLE lowering boundary.

use beskid_isle::{
    AstNodeKey, CallImporter, CallKind, DirectCallee, FunctionEmissionError, FunctionEmitter,
    LiteralKind, NodeFacts, NodeKind, OperatorFact, Signature,
};
use beskid_queries::{
    CallLowering, Db, ItemSignature, LiteralFact, SemanticTypeId, call_arguments, call_lowering,
    child_nodes, item_body, item_signature, literal_fact, local_slot, node_kind, node_type,
    operator_fact, resolved_local,
};
use cranelift_codegen::ir::{Type, UserFuncName, types};
use cranelift_codegen::isa::TargetIsa;

use crate::CodegenInput;

/// Query-backed facts for generated ISLE selection.
///
/// Every answer is read from the generation-safe syntax authority registered by the typed
/// program. Missing or not-yet-ported facts remain unavailable to ISLE instead of falling back
/// to HIR or hand-built test facts.
pub struct SyntaxNodeFacts<'db> {
    db: &'db dyn Db,
    isa: Option<&'db dyn TargetIsa>,
}

impl<'db> SyntaxNodeFacts<'db> {
    pub fn new(input: &CodegenInput<'db>) -> Self {
        Self {
            db: input.database(),
            isa: None,
        }
    }

    fn new_with_isa(input: &'db CodegenInput<'db>, isa: &'db dyn TargetIsa) -> Self {
        Self {
            db: input.database(),
            isa: Some(isa),
        }
    }

    fn query<T>(&self, result: beskid_queries::SemanticQueryResult<T>) -> Option<T> {
        result.ok().flatten()
    }
}

impl NodeFacts for SyntaxNodeFacts<'_> {
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        self.query(node_kind(self.db, key)).and_then(map_node_kind)
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        self.literal(key).map(|fact| match fact {
            LiteralFact::Integer(_) => LiteralKind::Integer,
            LiteralFact::Float(_) => LiteralKind::Float,
            LiteralFact::String(_) => LiteralKind::String,
            LiteralFact::Char(_) => LiteralKind::Char,
            LiteralFact::Bool(_) => LiteralKind::Boolean,
        })
    }

    fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
        self.query(operator_fact(self.db, key))
            .map(map_operator_fact)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        self.children(key).get(usize::from(index)).copied()
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        (self.node_kind(key) == Some(NodeKind::BlockExpression))
            .then(|| u8::try_from(self.children(key).len()).ok())?
    }

    fn let_initializer(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (self.node_kind(key) == Some(NodeKind::LetStatement))
            .then(|| self.children(key).last().copied())?
    }

    fn local_slot(&self, key: AstNodeKey) -> Option<u32> {
        match self.query(node_kind(self.db, key))? {
            beskid_queries::IndexedNodeKind::PathExpression => {
                let declaration = self.query(resolved_local(self.db, key))?.declaration;
                self.query(local_slot(self.db, declaration))
                    .map(|slot| slot.index)
            }
            beskid_queries::IndexedNodeKind::LetStatement => self
                .raw_children(key)
                .into_iter()
                .find(|child| {
                    self.query(node_kind(self.db, *child))
                        == Some(beskid_queries::IndexedNodeKind::Identifier)
                })
                .and_then(|identifier| self.query(local_slot(self.db, identifier)))
                .map(|slot| slot.index),
            _ => None,
        }
    }

    fn call_kind(&self, key: AstNodeKey) -> Option<CallKind> {
        matches!(
            self.query(call_lowering(self.db, key)),
            Some(CallLowering::Direct(_))
        )
        .then_some(CallKind::Direct)
    }

    fn direct_callee(&self, key: AstNodeKey) -> Option<DirectCallee> {
        let CallLowering::Direct(declaration) = self.query(call_lowering(self.db, key))? else {
            return None;
        };
        Some(DirectCallee::new(declaration.node.0))
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        let CallLowering::Direct(declaration) = self.query(call_lowering(self.db, key))? else {
            return None;
        };
        signature_for_item(self.isa?, self.query(item_signature(self.db, declaration))?)
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.query(call_arguments(self.db, key))
            .map(|arguments| arguments.to_vec())
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        let LiteralFact::Integer(text) = self.literal(key)? else {
            return None;
        };
        text.split_once('_')
            .map_or(text.as_ref(), |(value, _)| value)
            .parse()
            .ok()
    }

    fn boolean_literal(&self, key: AstNodeKey) -> Option<bool> {
        match self.literal(key)? {
            LiteralFact::Bool(value) => Some(value),
            _ => None,
        }
    }

    fn float_literal(&self, key: AstNodeKey) -> Option<f64> {
        let LiteralFact::Float(text) = self.literal(key)? else {
            return None;
        };
        text.parse().ok()
    }

    fn char_literal(&self, key: AstNodeKey) -> Option<char> {
        let LiteralFact::Char(text) = self.literal(key)? else {
            return None;
        };
        text.trim_matches('\'').chars().next()
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        let semantic = self.scalar_semantic_type(key)?;
        if semantic == SemanticTypeId::WORD {
            return self.isa.map(|isa| isa.pointer_type());
        }
        map_scalar_type(semantic)
    }
}

impl SyntaxNodeFacts<'_> {
    fn scalar_semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        self.query(node_type(self.db, key)).or_else(|| {
            (self.query(node_kind(self.db, key))
                == Some(beskid_queries::IndexedNodeKind::LetStatement))
            .then(|| {
                self.raw_children(key)
                    .into_iter()
                    .find(|child| {
                        self.query(node_kind(self.db, *child))
                            == Some(beskid_queries::IndexedNodeKind::Identifier)
                    })
                    .and_then(|identifier| self.query(node_type(self.db, identifier)))
            })?
        })
    }

    fn literal(&self, key: AstNodeKey) -> Option<LiteralFact> {
        self.query(literal_fact(self.db, key)).or_else(|| {
            self.query(child_nodes(self.db, key))?
                .iter()
                .find_map(|child| self.query(literal_fact(self.db, *child)))
        })
    }

    fn children(&self, key: AstNodeKey) -> Vec<AstNodeKey> {
        self.raw_children(key)
            .into_iter()
            .filter_map(|child| self.unwrap_transparent(child))
            .collect()
    }

    fn raw_children(&self, key: AstNodeKey) -> Vec<AstNodeKey> {
        self.query(child_nodes(self.db, key))
            .as_deref()
            .into_iter()
            .flatten()
            .copied()
            .collect()
    }

    fn unwrap_transparent(&self, mut key: AstNodeKey) -> Option<AstNodeKey> {
        loop {
            let kind = self.query(node_kind(self.db, key))?;
            if !matches!(
                kind,
                beskid_queries::IndexedNodeKind::Statement
                    | beskid_queries::IndexedNodeKind::Expression
            ) {
                return Some(key);
            }
            let children = self.query(child_nodes(self.db, key))?;
            key = *children.first()?;
        }
    }
}

/// Emit one parsed expanded-syntax expression through generated ISLE selection.
pub fn emit_isle_expression(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    body: AstNodeKey,
    result: Type,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new(input);
    emitter.emit_expression(
        UserFuncName::user(0, 0),
        emitter.signature([], [result]),
        &facts,
        body,
    )
}

/// Emit a zero-argument parsed function body through generated ISLE statement selection.
///
/// Parameters are intentionally rejected until local parameter materialization is represented by
/// the syntax facts, rather than falling back to the legacy HIR lowering context.
pub fn emit_isle_item(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::Verification("item has no syntax body".to_owned()))?;
    let signature = item_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| {
            FunctionEmissionError::Verification(
                "item signature is unavailable to syntax-only ISLE emission".to_owned(),
            )
        })?;
    if !signature.params.is_empty() {
        return Err(FunctionEmissionError::Verification(
            "syntax-only ISLE item emission does not yet materialize parameters".to_owned(),
        ));
    }
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new(input);
    emitter.emit_statement(UserFuncName::user(0, 0), signature, &facts, body)
}

/// Emit a syntax-only item with an explicit semantic-call importer.
pub fn emit_isle_item_with_call_importer(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::Verification("item has no syntax body".to_owned()))?;
    let signature = item_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| {
            FunctionEmissionError::Verification("item signature unavailable".to_owned())
        })?;
    if !signature.params.is_empty() {
        return Err(FunctionEmissionError::Verification(
            "syntax-only ISLE item emission does not yet materialize parameters".to_owned(),
        ));
    }
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_statement_with_call_importer(
        UserFuncName::user(0, 0),
        signature,
        &facts,
        body,
        importer,
    )
}

fn signature_for_item(isa: &dyn TargetIsa, item: ItemSignature) -> Option<beskid_isle::Signature> {
    let emitter = FunctionEmitter::new(isa);
    let parameters = item
        .parameters
        .iter()
        .copied()
        .map(|semantic| map_signature_type(isa, semantic))
        .collect::<Option<Vec<_>>>()?;
    let returns = match item.result {
        SemanticTypeId::UNIT => Vec::new(),
        result => vec![map_signature_type(isa, result)?],
    };
    Some(emitter.signature(parameters, returns))
}

fn map_node_kind(kind: beskid_queries::IndexedNodeKind) -> Option<NodeKind> {
    use beskid_queries::IndexedNodeKind as Syntax;

    Some(match kind {
        Syntax::Program => NodeKind::Program,
        Syntax::Block => NodeKind::BlockExpression,
        Syntax::FunctionDefinition => NodeKind::FunctionDefinition,
        Syntax::ExpressionStatement => NodeKind::ExpressionStatement,
        Syntax::ReturnStatement => NodeKind::ReturnStatement,
        Syntax::LetStatement => NodeKind::LetStatement,
        Syntax::IfStatement => NodeKind::IfStatement,
        Syntax::WhileStatement => NodeKind::WhileStatement,
        Syntax::BreakStatement => NodeKind::BreakStatement,
        Syntax::ContinueStatement => NodeKind::ContinueStatement,
        Syntax::LiteralExpression | Syntax::Literal => NodeKind::LiteralExpression,
        Syntax::GroupedExpression => NodeKind::GroupedExpression,
        Syntax::UnaryExpression => NodeKind::UnaryExpression,
        Syntax::BinaryExpression => NodeKind::BinaryExpression,
        Syntax::AssignExpression => NodeKind::AssignExpression,
        Syntax::CallExpression => NodeKind::CallExpression,
        Syntax::PathExpression => NodeKind::PathExpression,
        Syntax::IndexExpression => NodeKind::IndexExpression,
        Syntax::ArrayLiteralExpression => NodeKind::ArrayLiteralExpression,
        Syntax::MemberExpression => NodeKind::FieldExpression,
        Syntax::StructLiteralExpression => NodeKind::StructLiteralExpression,
        Syntax::EnumConstructorExpression => NodeKind::EnumLiteralExpression,
        Syntax::MatchExpression => NodeKind::MatchExpression,
        Syntax::RangeExpression => NodeKind::RangeExpression,
        Syntax::BlockExpression => NodeKind::BlockExpression,
        Syntax::ForStatement => NodeKind::ForStatement,
        _ => return None,
    })
}

fn map_operator_fact(operator: beskid_queries::OperatorFact) -> OperatorFact {
    use beskid_queries::OperatorFact as Syntax;

    match operator {
        Syntax::Or => OperatorFact::Or,
        Syntax::And => OperatorFact::And,
        Syntax::IdentityEq => OperatorFact::IdentityEq,
        Syntax::IdentityNotEq => OperatorFact::IdentityNotEq,
        Syntax::Eq => OperatorFact::Eq,
        Syntax::NotEq => OperatorFact::NotEq,
        Syntax::Lt => OperatorFact::Lt,
        Syntax::Lte => OperatorFact::Lte,
        Syntax::Gt => OperatorFact::Gt,
        Syntax::Gte => OperatorFact::Gte,
        Syntax::Add => OperatorFact::Add,
        Syntax::Sub => OperatorFact::Sub,
        Syntax::Mul => OperatorFact::Mul,
        Syntax::Div => OperatorFact::Div,
        Syntax::Mod => OperatorFact::Mod,
        Syntax::Neg => OperatorFact::Neg,
        Syntax::Not => OperatorFact::Not,
    }
}

fn map_scalar_type(semantic: SemanticTypeId) -> Option<Type> {
    Some(match semantic {
        SemanticTypeId::BOOL | SemanticTypeId::U8 => types::I8,
        SemanticTypeId::I32 => types::I32,
        SemanticTypeId::I64 => types::I64,
        SemanticTypeId::WORD => return None,
        SemanticTypeId::F64 => types::F64,
        SemanticTypeId::CHAR => types::I32,
        SemanticTypeId::UNIT | SemanticTypeId::STRING => return None,
        _ => return None,
    })
}

fn map_signature_type(isa: &dyn TargetIsa, semantic: SemanticTypeId) -> Option<Type> {
    if semantic == SemanticTypeId::WORD {
        Some(isa.pointer_type())
    } else {
        map_scalar_type(semantic)
    }
}
