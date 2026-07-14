//! Generation-safe Salsa facts consumed by the generated ISLE lowering boundary.

use beskid_isle::{
    AstNodeKey, FunctionEmissionError, FunctionEmitter, LiteralKind, NodeFacts, NodeKind,
    OperatorFact,
};
use beskid_queries::{
    Db, LiteralFact, SemanticTypeId, child_nodes, literal_fact, node_kind, node_type, operator_fact,
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
}

impl<'db> SyntaxNodeFacts<'db> {
    pub fn new(input: &CodegenInput<'db>) -> Self {
        Self {
            db: input.database(),
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
        self.query(literal_fact(self.db, key))
            .map(|fact| match fact {
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
        self.query(child_nodes(self.db, key))
            .and_then(|children| children.get(usize::from(index)).copied())
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        let LiteralFact::Integer(text) = self.query(literal_fact(self.db, key))? else {
            return None;
        };
        text.split_once('_')
            .map_or(text.as_ref(), |(value, _)| value)
            .parse()
            .ok()
    }

    fn boolean_literal(&self, key: AstNodeKey) -> Option<bool> {
        match self.query(literal_fact(self.db, key))? {
            LiteralFact::Bool(value) => Some(value),
            _ => None,
        }
    }

    fn float_literal(&self, key: AstNodeKey) -> Option<f64> {
        let LiteralFact::Float(text) = self.query(literal_fact(self.db, key))? else {
            return None;
        };
        text.parse().ok()
    }

    fn char_literal(&self, key: AstNodeKey) -> Option<char> {
        let LiteralFact::Char(text) = self.query(literal_fact(self.db, key))? else {
            return None;
        };
        text.trim_matches('\'').chars().next()
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        self.query(node_type(self.db, key))
            .and_then(map_scalar_type)
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

fn map_node_kind(kind: beskid_queries::IndexedNodeKind) -> Option<NodeKind> {
    use beskid_queries::IndexedNodeKind as Syntax;

    Some(match kind {
        Syntax::Program => NodeKind::Program,
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
        SemanticTypeId::F64 => types::F64,
        SemanticTypeId::CHAR => types::I32,
        SemanticTypeId::UNIT | SemanticTypeId::STRING => return None,
        _ => return None,
    })
}
