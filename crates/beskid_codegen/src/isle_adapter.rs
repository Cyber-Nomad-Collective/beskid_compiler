//! Generation-safe Salsa facts consumed by the generated ISLE lowering boundary.

use std::collections::HashMap;

use beskid_analysis::syntax::try_decode_string_literal_token;
use beskid_isle::{
    AstNodeKey, CallImporter, CallKind, DirectCallee, FunctionEmissionError, FunctionEmitter,
    LiteralKind, NodeFacts, NodeKind, OperatorFact, ParameterSlot, RuntimeIntrinsicKind, Signature, StringInterner,
};
use beskid_queries::{
    CallLowering, Db, ItemSignature, LiteralFact, SemanticTypeId, abi_type, block_statement_nodes, call_arguments,
    call_lowering, cast_intents, child_nodes, item_abi_signature, item_body, literal_fact,
    local_slot, node_kind, node_type, operator_fact, resolved_local, runtime_intrinsic_name,
    test_statement_nodes,
};
use cranelift_codegen::ir::{FuncRef, Type, UserFuncName, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_frontend::FunctionBuilder;
use cranelift_module::{FuncId, Module};

use crate::CodegenInput;

/// Query-backed facts for generated ISLE selection.
///
/// Every answer is read from the generation-safe syntax authority registered by the typed
/// program. Missing or not-yet-ported facts remain unavailable to ISLE instead of falling back
/// to HIR or hand-built test facts.
pub struct SyntaxNodeFacts<'db> {
    db: &'db dyn Db,
    input: &'db CodegenInput<'db>,
    isa: Option<&'db dyn TargetIsa>,
}

impl<'db> SyntaxNodeFacts<'db> {
    pub fn new(input: &'db CodegenInput<'db>) -> Self {
        Self {
            db: input.database(),
            input,
            isa: None,
        }
    }

    fn new_with_isa(input: &'db CodegenInput<'db>, isa: &'db dyn TargetIsa) -> Self {
        Self {
            db: input.database(),
            input,
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
        let children = if self.node_kind(key) == Some(NodeKind::TestDefinition) {
            self.query(test_statement_nodes(self.db, key))?
        } else if self.node_kind(key) == Some(NodeKind::BlockExpression) {
            self.query(block_statement_nodes(self.db, key))?
        } else if self.node_kind(key) == Some(NodeKind::BinaryExpression) {
            self.children(key)
                .iter()
                .copied()
                .filter(|child| {
                    !matches!(
                        self.query(node_kind(self.db, *child)),
                        Some(beskid_queries::IndexedNodeKind::BinaryOp)
                    )
                })
                .collect()
        } else {
            self.children(key).into()
        };
        children
            .get(usize::from(index))
            .copied()
            .and_then(|child| self.unwrap_transparent(child))
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        matches!(
            self.node_kind(key),
            Some(NodeKind::BlockExpression | NodeKind::TestDefinition)
        )
        .then(|| {
            let length = if self.node_kind(key) == Some(NodeKind::TestDefinition) {
                let nodes = self.query(test_statement_nodes(self.db, key))?;
                nodes.len()
            } else if self.node_kind(key) == Some(NodeKind::BlockExpression) {
                let nodes = self.query(block_statement_nodes(self.db, key))?;
                nodes.len()
            } else {
                self.children(key).len()
            };
            u8::try_from(length).ok()
        })?
    }

    fn let_initializer(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (self.node_kind(key) == Some(NodeKind::LetStatement))
            .then(|| self.children(key).last().copied())?
            .and_then(|initializer| self.unwrap_transparent(initializer))
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
        if self.runtime_intrinsic(key).is_some() {
            return Some(CallKind::RuntimeIntrinsic);
        }
        matches!(
            self.query(call_lowering(self.db, key)),
            Some(CallLowering::Direct(_))
        )
        .then_some(CallKind::Direct)
    }

    fn runtime_intrinsic_kind(&self, key: AstNodeKey) -> Option<RuntimeIntrinsicKind> {
        let (_, intrinsic) = self.runtime_intrinsic(key)?;
        Some(match intrinsic.name.as_str() {
            "memory_copy" => RuntimeIntrinsicKind::MemoryCopy,
            "memory_set" => RuntimeIntrinsicKind::MemorySet,
            "native_word_from_pointer" => RuntimeIntrinsicKind::NativeWordFromPointer,
            "pointer_from_native_word" => RuntimeIntrinsicKind::PointerFromNativeWord,
            "pointer_add" => RuntimeIntrinsicKind::PointerAdd,
            "raw_word_load" => RuntimeIntrinsicKind::RawWordLoad,
            "raw_word_store" => RuntimeIntrinsicKind::RawWordStore,
            "raw_byte_load" => RuntimeIntrinsicKind::RawByteLoad,
            "raw_byte_store" => RuntimeIntrinsicKind::RawByteStore,
            "tls_get" => RuntimeIntrinsicKind::TlsGet,
            "tls_set" => RuntimeIntrinsicKind::TlsSet,
            _ => return None,
        })
    }

    fn direct_callee(&self, key: AstNodeKey) -> Option<DirectCallee> {
        if let Some((index, _)) = self.runtime_intrinsic(key) {
            return Some(DirectCallee::runtime_intrinsic(index));
        }
        let CallLowering::Direct(declaration) = self.query(call_lowering(self.db, key))? else {
            return None;
        };
        Some(DirectCallee::item(declaration))
    }

    fn call_signature(&self, key: AstNodeKey) -> Option<Signature> {
        if let Some((_, intrinsic)) = self.runtime_intrinsic(key) {
            return signature_for_runtime_intrinsic(self.isa?, intrinsic);
        }
        let CallLowering::Direct(declaration) = self.query(call_lowering(self.db, key))? else {
            return None;
        };
        signature_for_item(
            self.isa?,
            self.query(item_abi_signature(self.db, declaration))?,
        )
    }

    fn call_arguments(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        self.query(call_arguments(self.db, key))
            .and_then(|arguments| {
                arguments
                    .iter()
                    .copied()
                    .map(|argument| self.unwrap_transparent(argument))
                    .collect()
            })
    }

    fn function_parameters(&self, key: AstNodeKey) -> Option<Vec<ParameterSlot>> {
        let mut parameters = Vec::new();
        self.collect_function_parameters(key, &mut parameters)?;
        Some(parameters)
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

    fn string_literal(&self, key: AstNodeKey) -> Option<std::sync::Arc<str>> {
        let LiteralFact::String(text) = self.literal(key)? else {
            return None;
        };
        try_decode_string_literal_token(&text).map(Into::into)
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        let semantic = self.scalar_semantic_type(key).or_else(|| {
            let CallLowering::Direct(declaration) = self.query(call_lowering(self.db, key))? else {
                return None;
            };
            Some(self.query(item_abi_signature(self.db, declaration))?.result)
        })?;
        if matches!(
            semantic,
            SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING
        ) {
            return self.isa.map(|isa| isa.pointer_type());
        }
        map_scalar_type(semantic)
    }
}

impl SyntaxNodeFacts<'_> {
    fn runtime_intrinsic(
        &self,
        key: AstNodeKey,
    ) -> Option<(u32, &beskid_abi::abi_v5::RuntimeIntrinsic)> {
        let name = self.query(runtime_intrinsic_name(self.db, key))?;
        self.input.runtime_intrinsic_for(key, &name.0)
    }
    fn collect_function_parameters(
        &self,
        key: AstNodeKey,
        parameters: &mut Vec<ParameterSlot>,
    ) -> Option<()> {
        for child in self.raw_children(key) {
            match self.query(node_kind(self.db, child))? {
                beskid_queries::IndexedNodeKind::Block => continue,
                beskid_queries::IndexedNodeKind::Parameter => {
                    let identifier = self.raw_children(child).into_iter().find(|candidate| {
                        self.query(node_kind(self.db, *candidate))
                            == Some(beskid_queries::IndexedNodeKind::Identifier)
                    })?;
                    let slot = self.query(local_slot(self.db, identifier))?;
                    let value_type =
                        self.scalar_semantic_type(identifier).and_then(|semantic| {
                            if matches!(
                                semantic,
                                SemanticTypeId::WORD
                                    | SemanticTypeId::POINTER
                                    | SemanticTypeId::STRING
                            ) {
                                self.isa.map(|isa| isa.pointer_type())
                            } else {
                                map_scalar_type(semantic)
                            }
                        })?;
                    parameters.push(ParameterSlot {
                        slot: slot.index,
                        value_type,
                    });
                }
                _ => self.collect_function_parameters(child, parameters)?,
            }
        }
        Some(())
    }

    fn scalar_semantic_type(&self, key: AstNodeKey) -> Option<SemanticTypeId> {
        self.query(cast_intents(self.db, key))
            .and_then(|intents| intents.first().map(|intent| intent.to))
            .or_else(|| self.query(abi_type(self.db, key)))
            .or_else(|| self.query(node_type(self.db, key)))
            .or_else(|| {
                (self.query(node_kind(self.db, key))
                    == Some(beskid_queries::IndexedNodeKind::LetStatement))
                .then(|| {
                    self.raw_children(key)
                        .into_iter()
                        .find(|child| {
                            self.query(node_kind(self.db, *child))
                                == Some(beskid_queries::IndexedNodeKind::Identifier)
                        })
                        .and_then(|identifier| self.query(abi_type(self.db, identifier)))
                        .or_else(|| {
                            self.raw_children(key)
                                .into_iter()
                                .find(|child| {
                                    self.query(node_kind(self.db, *child))
                                        == Some(beskid_queries::IndexedNodeKind::Identifier)
                                })
                                .and_then(|identifier| self.query(node_type(self.db, identifier)))
                        })
                })?
            })
            .or_else(|| {
                let (_, intrinsic) = self.runtime_intrinsic(key)?;
                semantic_type_for_runtime_intrinsic(intrinsic)
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
        let children = self.raw_children(key);
        let children = if self.query(node_kind(self.db, key))
            == Some(beskid_queries::IndexedNodeKind::TestDefinition)
        {
            children
                .into_iter()
                .filter(|child| {
                    self.query(node_kind(self.db, *child))
                        == Some(beskid_queries::IndexedNodeKind::Statement)
                })
                .collect()
        } else {
            children
        };
        children
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

fn semantic_type_for_runtime_intrinsic(
    intrinsic: &beskid_abi::abi_v5::RuntimeIntrinsic,
) -> Option<SemanticTypeId> {
    use beskid_abi::abi_v5::AbiType;

    Some(match intrinsic.result {
        AbiType::Void => return None,
        AbiType::Pointer => SemanticTypeId::POINTER,
        AbiType::USize => SemanticTypeId::WORD,
        AbiType::I8 => SemanticTypeId::U8,
        AbiType::U8 => SemanticTypeId::U8,
        AbiType::I32 => SemanticTypeId::I32,
        AbiType::I64 => SemanticTypeId::I64,
        AbiType::F64 => SemanticTypeId::F64,
        _ => return None,
    })
}

/// Emit one parsed expanded-syntax expression through generated ISLE selection.
pub fn emit_isle_expression<'db>(
    input: &'db CodegenInput<'db>,
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
pub fn emit_isle_item<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::Verification("item has no syntax body".to_owned()))?;
    let signature = item_abi_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| {
            FunctionEmissionError::Verification(
                "item signature is unavailable to syntax-only ISLE emission".to_owned(),
            )
        })?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_item_statement(UserFuncName::user(0, 0), signature, &facts, item, body)
}

/// Read the generation-safe syntax signature required to predeclare an item in a module.
pub fn syntax_item_signature(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
) -> Result<Signature, FunctionEmissionError> {
    item_abi_signature(input.database(), item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| FunctionEmissionError::Verification("item signature unavailable".to_owned()))
}

/// Emit a syntax-only item with an explicit semantic-call importer.
pub fn emit_isle_item_with_call_importer<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::Verification("item has no syntax body".to_owned()))?;
    let signature = item_abi_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| {
            FunctionEmissionError::Verification("item signature unavailable".to_owned())
        })?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_item_statement_with_call_importer(
        UserFuncName::user(0, 0),
        signature,
        &facts,
        item,
        body,
        importer,
    )
}

/// Emit a syntax-only item with the shared artifact string pool and exact call imports.
pub fn emit_isle_item_with_services<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
    string_interner: &mut dyn StringInterner,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::Verification("item has no syntax body".to_owned()))?;
    let signature = item_abi_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| {
            FunctionEmissionError::Verification("item signature unavailable".to_owned())
        })?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_item_statement_with_services(
        UserFuncName::user(0, 0),
        signature,
        &facts,
        item,
        body,
        string_interner,
        importer,
    )
}

/// Explicit module importer keyed by syntax-resolved item identity.
///
/// Call lowering never guesses symbols: the host declares each item and supplies its exact
/// [`FuncId`] keyed by [`DirectCallee`].
pub struct ItemModuleImporter<'module, M: Module> {
    module: &'module mut M,
    functions: HashMap<DirectCallee, FuncId>,
}

impl<'module, M: Module> ItemModuleImporter<'module, M> {
    pub fn new(module: &'module mut M, functions: HashMap<DirectCallee, FuncId>) -> Self {
        Self { module, functions }
    }
}

impl<M: Module> CallImporter for ItemModuleImporter<'_, M> {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        _signature: &Signature,
    ) -> Result<FuncRef, beskid_isle::CallImportError> {
        let function = self
            .functions
            .get(&callee)
            .copied()
            .ok_or(beskid_isle::CallImportError::UnknownCallee)?;
        Ok(self.module.declare_func_in_func(function, builder.func))
    }
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
        SemanticTypeId::UNIT | SemanticTypeId::NEVER => Vec::new(),
        result => vec![map_signature_type(isa, result)?],
    };
    Some(emitter.signature(parameters, returns))
}

fn signature_for_runtime_intrinsic(
    isa: &dyn TargetIsa,
    intrinsic: &beskid_abi::abi_v5::RuntimeIntrinsic,
) -> Option<beskid_isle::Signature> {
    let emitter = FunctionEmitter::new(isa);
    let parameters = intrinsic
        .params
        .iter()
        .copied()
        .map(|ty| map_abi_type(isa, ty))
        .collect::<Option<Vec<_>>>()?;
    let returns = if intrinsic.noreturn || intrinsic.result == beskid_abi::abi_v5::AbiType::Void {
        Vec::new()
    } else {
        vec![map_abi_type(isa, intrinsic.result)?]
    };
    Some(emitter.signature(parameters, returns))
}

fn map_node_kind(kind: beskid_queries::IndexedNodeKind) -> Option<NodeKind> {
    use beskid_queries::IndexedNodeKind as Syntax;

    Some(match kind {
        Syntax::Program => NodeKind::Program,
        Syntax::Block => NodeKind::BlockExpression,
        Syntax::FunctionDefinition => NodeKind::FunctionDefinition,
        Syntax::TestDefinition => NodeKind::TestDefinition,
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
        SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::NEVER => return None,
        SemanticTypeId::F64 => types::F64,
        SemanticTypeId::CHAR => types::I32,
        SemanticTypeId::UNIT | SemanticTypeId::STRING => return None,
        _ => return None,
    })
}

fn map_signature_type(isa: &dyn TargetIsa, semantic: SemanticTypeId) -> Option<Type> {
    if matches!(
        semantic,
        SemanticTypeId::WORD | SemanticTypeId::POINTER | SemanticTypeId::STRING
    ) {
        Some(isa.pointer_type())
    } else {
        map_scalar_type(semantic)
    }
}

fn map_abi_type(isa: &dyn TargetIsa, ty: beskid_abi::abi_v5::AbiType) -> Option<Type> {
    use beskid_abi::abi_v5::AbiType;
    Some(match ty {
        AbiType::Pointer | AbiType::USize | AbiType::ISize => isa.pointer_type(),
        AbiType::I8 | AbiType::U8 => types::I8,
        AbiType::I16 | AbiType::U16 => types::I16,
        AbiType::I32 | AbiType::U32 => types::I32,
        AbiType::I64 | AbiType::U64 => types::I64,
        AbiType::V128 => types::I8X16,
        AbiType::F32 => types::F32,
        AbiType::F64 => types::F64,
        AbiType::Void => return None,
    })
}
