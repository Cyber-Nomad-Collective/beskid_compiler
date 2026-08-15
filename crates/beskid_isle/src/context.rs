use std::collections::{HashMap, HashSet};

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::immediates::{Ieee32, Ieee64};
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::InstBuilder;
use cranelift_codegen::ir::{
    AbiParam, Block, FuncRef, MemFlags, Signature, StackSlotData, StackSlotKind, TrapCode, Type, Value,
};
use cranelift_codegen::ir::{ExternalName, GlobalValueData};
use cranelift_codegen::isa::CallConv;
use cranelift_frontend::{FunctionBuilder, Switch, Variable};

use crate::dispatch;
use crate::errors::{FunctionEmissionError, LoweringError, LoweringErrorKind, StringMaterializationError};
use crate::facts::{
    AstNodeKey, CallImportError, CallKind, CollectionMutationOwner, CollectionOperation, DirectCallee, ForIterableKind,
    IndexTarget, InlineClosureEnvironment, LiteralKind, LocalSlotId, MatchArmFact, NodeFacts, NodeKind, OperatorFact,
    RuntimeIntrinsicKind, Unit,
};
use crate::layout::EnumLayout;

mod aggregate;
mod calls;
mod control_flow;
mod enums;
mod intrinsics;
mod operators;
mod strings;

use operators::{primitive_numeric_conversion_type_matches, CompareOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StatementCursor {
    block: AstNodeKey,
    index: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorKind {
    More,
    End,
}

#[derive(Clone, Copy)]
struct LoopTargets {
    continue_block: Block,
    break_block: Block,
}

#[allow(
    dead_code,
    unused_imports,
    unreachable_patterns,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::len_without_is_empty,
    clippy::let_unit_value,
    clippy::match_ref_pats
)]
mod generated {
    use super::{AstNodeKey, CallKind, CursorKind, LiteralKind, NodeKind, OperatorFact, StatementCursor, Unit, Value};

    include!(concat!(env!("OUT_DIR"), "/beskid_lower.rs"));
}

pub trait StringInterner {
    fn intern(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        key: AstNodeKey,
        text: &str,
    ) -> Result<Value, StringMaterializationError>;
}

/// Caller-local import of a semantic callee after generated ISLE selection.
pub trait CallImporter {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        signature: &Signature,
    ) -> Result<FuncRef, CallImportError>;
}

/// Thin host for generated ISLE selection and stock CLIF instruction construction.
pub struct IsleContext<'builder, 'function, 'facts, 'interner> {
    pub(crate) builder: &'builder mut FunctionBuilder<'function>,
    pub(crate) facts: &'facts dyn NodeFacts,
    string_interner: Option<&'interner mut dyn StringInterner>,
    call_importer: Option<&'interner mut dyn CallImporter>,
    loop_stack: Vec<LoopTargets>,
    pub(crate) locals: HashMap<LocalSlotId, (Variable, Type)>,
    pub function_param_values: Vec<Value>,
    pending_error: Option<LoweringError>,
}

impl<'builder, 'function, 'facts, 'interner> IsleContext<'builder, 'function, 'facts, 'interner> {
    pub fn new(builder: &'builder mut FunctionBuilder<'function>, facts: &'facts dyn NodeFacts) -> Self {
        Self {
            builder,
            facts,
            string_interner: None,
            call_importer: None,
            loop_stack: Vec::new(),
            locals: HashMap::new(),
            function_param_values: Vec::new(),
            pending_error: None,
        }
    }

    pub fn new_with_string_interner(
        builder: &'builder mut FunctionBuilder<'function>,
        facts: &'facts dyn NodeFacts,
        string_interner: &'interner mut dyn StringInterner,
    ) -> Self {
        Self {
            builder,
            facts,
            string_interner: Some(string_interner),
            call_importer: None,
            loop_stack: Vec::new(),
            locals: HashMap::new(),
            function_param_values: Vec::new(),
            pending_error: None,
        }
    }

    pub fn new_with_call_importer(
        builder: &'builder mut FunctionBuilder<'function>,
        facts: &'facts dyn NodeFacts,
        call_importer: &'interner mut dyn CallImporter,
    ) -> Self {
        Self {
            builder,
            facts,
            string_interner: None,
            call_importer: Some(call_importer),
            loop_stack: Vec::new(),
            locals: HashMap::new(),
            function_param_values: Vec::new(),
            pending_error: None,
        }
    }

    pub(crate) fn new_with_services(
        builder: &'builder mut FunctionBuilder<'function>,
        facts: &'facts dyn NodeFacts,
        string_interner: Option<&'interner mut dyn StringInterner>,
        call_importer: Option<&'interner mut dyn CallImporter>,
    ) -> Self {
        Self {
            builder,
            facts,
            string_interner,
            call_importer,
            loop_stack: Vec::new(),
            locals: HashMap::new(),
            function_param_values: Vec::new(),
            pending_error: None,
        }
    }
}

pub(crate) fn materialize_parameters(
    context: &mut IsleContext<'_, '_, '_, '_>,
    item: AstNodeKey,
) -> Result<(), FunctionEmissionError> {
    let parameters = context
        .facts
        .function_parameters(item)
        .ok_or_else(|| FunctionEmissionError::verification(item, "item parameter facts are unavailable"))?;
    let incoming = context
        .builder
        .block_params(
            context
                .builder
                .current_block()
                .ok_or_else(|| FunctionEmissionError::verification(item, "function has no entry block"))?,
        )
        .to_vec();
    if parameters.len() != incoming.len() {
        return Err(FunctionEmissionError::verification(
            item,
            "item parameter facts do not match function signature".to_owned(),
        ));
    }
    for (parameter, value) in parameters.into_iter().zip(incoming) {
        if context.locals.contains_key(&parameter.slot)
            || context.builder.func.dfg.value_type(value) != parameter.value_type
        {
            return Err(FunctionEmissionError::verification(item, "item parameter slot or type is invalid".to_owned()));
        }
        let variable = context.builder.declare_var(parameter.value_type);
        context.builder.def_var(variable, value);
        context.locals.insert(parameter.slot, (variable, parameter.value_type));
        context.function_param_values.push(value);
    }
    Ok(())
}

pub(crate) fn block_is_terminated(builder: &FunctionBuilder<'_>, block: Block) -> bool {
    builder.func.layout.last_inst(block).is_some_and(|inst| builder.func.dfg.insts[inst].opcode().is_terminator())
}

/// If the builder's current block is unterminated, jump to `target` and return true.
///
/// Nested control-flow lowering can leave the cursor on a descendant block that is
/// different from the arm/body block originally switched into. Callers must terminate
/// the *current* block before `switch_to_block`, or Cranelift panics.
pub(crate) fn jump_from_current_if_unterminated(builder: &mut FunctionBuilder<'_>, target: Block) -> bool {
    let Some(current) = builder.current_block() else {
        return false;
    };
    if block_is_terminated(builder, current) {
        return false;
    }
    builder.ins().jump(target, &[]);
    true
}

impl generated::Context for IsleContext<'_, '_, '_, '_> {
    fn node_kind(&mut self, key: AstNodeKey) -> Option<NodeKind> {
        self.facts.node_kind(key)
    }

    fn literal_kind(&mut self, key: AstNodeKey) -> Option<LiteralKind> {
        self.facts.literal_kind(key)
    }

    fn operator_fact(&mut self, key: AstNodeKey) -> Option<OperatorFact> {
        self.facts.operator_fact(key)
    }

    fn call_kind(&mut self, key: AstNodeKey) -> Option<CallKind> {
        self.facts.call_kind(key)
    }

    fn assignment_target_kind(&mut self, key: AstNodeKey) -> Option<NodeKind> {
        self.facts.child(key, 0).and_then(|target| self.facts.node_kind(target))
    }

    fn for_iterable_kind(&mut self, key: AstNodeKey) -> Option<NodeKind> {
        self.facts.child(key, 0).and_then(|iterable| self.facts.node_kind(iterable))
    }

    fn for_iterable_class(&mut self, key: AstNodeKey) -> Option<ForIterableKind> {
        let iterable = self.facts.child(key, 0)?;
        let kind = self.facts.node_kind(iterable)?;
        if kind == NodeKind::RangeExpression {
            Some(ForIterableKind::Range)
        } else {
            Some(ForIterableKind::Other)
        }
    }

    fn child_at(&mut self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        self.facts.child(key, index)
    }

    fn emit_integer(&mut self, key: AstNodeKey) -> Option<Value> {
        let immediate = self.facts.integer_literal(key)?;
        let value_type = self.facts.scalar_type(key)?;
        Some(self.builder.ins().iconst(value_type, immediate))
    }

    fn emit_boolean(&mut self, key: AstNodeKey) -> Option<Value> {
        let immediate = i64::from(self.facts.boolean_literal(key)?);
        let value_type = self.facts.scalar_type(key)?;
        Some(self.builder.ins().iconst(value_type, immediate))
    }

    fn emit_float(&mut self, key: AstNodeKey) -> Option<Value> {
        let immediate = self.facts.float_literal(key)?;
        match self.facts.scalar_type(key)? {
            types::F32 => Some(self.builder.ins().f32const(Ieee32::with_float(immediate as f32))),
            types::F64 => Some(self.builder.ins().f64const(Ieee64::with_float(immediate))),
            _ => None,
        }
    }

    fn emit_char(&mut self, key: AstNodeKey) -> Option<Value> {
        let immediate = i64::from(u32::from(self.facts.char_literal(key)?));
        let value_type = self.facts.scalar_type(key)?;
        Some(self.builder.ins().iconst(value_type, immediate))
    }

    strings::generated_string_methods!();
    operators::generated_operator_methods!();
    calls::generated_call_methods!();
    intrinsics::generated_intrinsic_methods!();
    control_flow::generated_control_flow_methods!();
    aggregate::generated_aggregate_methods!();
    enums::generated_enum_methods!();
}

pub fn lower_expression(context: &mut IsleContext<'_, '_, '_, '_>, key: AstNodeKey) -> Result<Value, LoweringError> {
    generated::constructor_lower_expression(context, key).ok_or_else(|| {
        context.pending_error.take().unwrap_or(LoweringError { key, kind: LoweringErrorKind::MissingRuleOrFact })
    })
}

pub fn lower_statement(context: &mut IsleContext<'_, '_, '_, '_>, key: AstNodeKey) -> Result<(), LoweringError> {
    generated::constructor_lower_statement(context, key).ok_or_else(|| {
        context.pending_error.take().unwrap_or(LoweringError { key, kind: LoweringErrorKind::MissingRuleOrFact })
    })
}
