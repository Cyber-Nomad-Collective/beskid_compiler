//! `for x in iterator { ... }` over the nominal iterator protocol, lowered from an
//! `IteratorForFact` and executed through the JIT: the iterable is evaluated once, `Next()` runs
//! at the loop header (also the `continue` target) and the `Some` payload is bound to the loop
//! variable until `Next()` returns `None`.

use std::path::PathBuf;

use beskid_isle::callee::DirectCallee;
use beskid_isle::syntax_types::{LiteralKind, OperatorFact};
use beskid_isle::{
    AssignmentKind, AstNodeKey, CallImportError, CallImporter, EnumLayout, EnumVariantLayout, FieldLayout,
    FunctionEmissionError, FunctionEmitter, IteratorForFact, LocalSlotId, LoweringErrorKind, ManagedReferenceFact,
    NodeFacts, NodeKind,
};
use beskid_queries::{AstNodeId, BeskidDatabase, SourceUnitId, SyntaxGenerationId};
use cranelift_codegen::ir::{AbiParam, FuncRef, Function, Signature, Type, UserFuncName, types};
use cranelift_codegen::isa::{CallConv, TargetIsa};
use cranelift_codegen::settings;
use cranelift_frontend::FunctionBuilder;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use target_lexicon::Triple;

/// Iterator state the JIT-compiled loop receives as its iterable value.
#[repr(C)]
struct Counter {
    next: i32,
    last: i32,
    calls: i32,
}

/// The physical `Option<i32>` object `Next()` returns: tag at offset 0, `Some` payload at 8.
#[repr(C)]
struct OptionObject {
    tag: i32,
    padding: i32,
    value: i32,
    tail: i32,
}

const SOME: i32 = 0;
const NONE: i32 = 1;

extern "C" fn counter_next(counter: *mut Counter) -> *mut OptionObject {
    let counter = unsafe { &mut *counter };
    counter.calls += 1;
    let object = if counter.next > counter.last {
        OptionObject { tag: NONE, padding: 0, value: 0, tail: 0 }
    } else {
        let value = counter.next;
        counter.next += 1;
        OptionObject { tag: SOME, padding: 0, value, tail: 0 }
    };
    Box::into_raw(Box::new(object))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Body {
    /// `{ sum = sum + x; }`
    Accumulate,
    /// `{ sum = sum + x; break; }`
    BreakAfterFirst,
    /// `{ continue; sum = sum + x; }`
    ContinueBeforeAccumulate,
}

/// `{ mut i32 sum = 0; for x in <iterator> { <body> } return sum; }`
struct IteratorForFacts {
    nodes: [AstNodeKey; 17],
    iterator: i64,
    body: Body,
    element_type: Type,
    payload_type: Type,
    call_conv: CallConv,
}

const SUM: LocalSlotId = LocalSlotId { owner_node: 0, index: 0 };
const ELEMENT: LocalSlotId = LocalSlotId { owner_node: 0, index: 1 };

impl IteratorForFacts {
    fn body_statements(&self) -> Vec<AstNodeKey> {
        match self.body {
            Body::Accumulate => vec![self.nodes[8]],
            Body::BreakAfterFirst => vec![self.nodes[8], self.nodes[16]],
            Body::ContinueBeforeAccumulate => vec![self.nodes[16], self.nodes[8]],
        }
    }

    fn callee(&self) -> DirectCallee {
        DirectCallee::item(AstNodeKey { node: AstNodeId(99), ..self.nodes[0] })
    }
}

impl NodeFacts for IteratorForFacts {
    fn assignment_kind(&self, key: AstNodeKey) -> Option<AssignmentKind> {
        (key == self.nodes[9]).then_some(AssignmentKind::Local)
    }

    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        let kind = if key == self.nodes[0] || key == self.nodes[7] {
            NodeKind::BlockExpression
        } else if key == self.nodes[1] {
            NodeKind::LetStatement
        } else if key == self.nodes[3] {
            NodeKind::ForStatement
        } else if [self.nodes[2], self.nodes[4]].contains(&key) {
            NodeKind::LiteralExpression
        } else if key == self.nodes[8] {
            NodeKind::ExpressionStatement
        } else if key == self.nodes[9] {
            NodeKind::AssignExpression
        } else if key == self.nodes[11] {
            NodeKind::BinaryExpression
        } else if [self.nodes[10], self.nodes[12], self.nodes[13], self.nodes[15]].contains(&key) {
            NodeKind::PathExpression
        } else if key == self.nodes[14] {
            NodeKind::ReturnStatement
        } else if key == self.nodes[16] {
            match self.body {
                Body::Accumulate => return None,
                Body::BreakAfterFirst => NodeKind::BreakStatement,
                Body::ContinueBeforeAccumulate => NodeKind::ContinueStatement,
            }
        } else {
            return None;
        };
        Some(kind)
    }

    fn literal_kind(&self, key: AstNodeKey) -> Option<LiteralKind> {
        [self.nodes[2], self.nodes[4]].contains(&key).then_some(LiteralKind::Integer)
    }

    fn operator_fact(&self, key: AstNodeKey) -> Option<OperatorFact> {
        (key == self.nodes[11]).then_some(OperatorFact::Add)
    }

    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        let children = if key == self.nodes[0] {
            vec![self.nodes[1], self.nodes[3], self.nodes[14]]
        } else if key == self.nodes[3] {
            vec![self.nodes[4], self.nodes[7]]
        } else if key == self.nodes[7] {
            self.body_statements()
        } else if key == self.nodes[8] {
            vec![self.nodes[9]]
        } else if key == self.nodes[9] {
            vec![self.nodes[10], self.nodes[11]]
        } else if key == self.nodes[11] {
            vec![self.nodes[12], self.nodes[13]]
        } else if key == self.nodes[14] {
            vec![self.nodes[15]]
        } else {
            return None;
        };
        children.get(usize::from(index)).copied()
    }

    fn statement_count(&self, key: AstNodeKey) -> Option<u8> {
        if key == self.nodes[0] {
            Some(3)
        } else if key == self.nodes[7] {
            u8::try_from(self.body_statements().len()).ok()
        } else {
            None
        }
    }

    fn let_initializer(&self, key: AstNodeKey) -> Option<AstNodeKey> {
        (key == self.nodes[1]).then_some(self.nodes[2])
    }

    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        if key == self.nodes[2] {
            Some(0)
        } else if key == self.nodes[4] {
            Some(self.iterator)
        } else {
            None
        }
    }

    fn scalar_type(&self, key: AstNodeKey) -> Option<Type> {
        if key == self.nodes[4] {
            Some(types::I64)
        } else if key == self.nodes[3] || key == self.nodes[13] {
            Some(self.element_type)
        } else {
            self.nodes.contains(&key).then_some(types::I32)
        }
    }

    fn local_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        if [self.nodes[1], self.nodes[10], self.nodes[12], self.nodes[15]].contains(&key) {
            Some(SUM)
        } else if key == self.nodes[3] || key == self.nodes[13] {
            Some(ELEMENT)
        } else {
            None
        }
    }

    fn mutable_local_assignment_slot(&self, key: AstNodeKey) -> Option<LocalSlotId> {
        (key == self.nodes[9]).then_some(SUM)
    }

    /// The iterable is a native address in this fixture, so the loop registers no GC root.
    fn managed_reference(&self, key: AstNodeKey) -> Option<ManagedReferenceFact> {
        (key == self.nodes[4]).then_some(ManagedReferenceFact::NativeOrScalar)
    }

    fn iterator_for_fact(&self, key: AstNodeKey) -> Option<IteratorForFact> {
        (key == self.nodes[3]).then(|| IteratorForFact {
            next: self.callee(),
            next_signature: Signature {
                params: vec![AbiParam::new(types::I64)],
                returns: vec![AbiParam::new(types::I64)],
                call_conv: self.call_conv,
            },
            option_layout: EnumLayout::new(
                16,
                3,
                FieldLayout::new(types::I32, 0),
                vec![
                    EnumVariantLayout::new(SOME as u64, vec![Some(FieldLayout::new(self.payload_type, 8))]),
                    EnumVariantLayout::new(NONE as u64, Vec::new()),
                ],
            ),
            some_variant: 0,
            element_managed: ManagedReferenceFact::NativeOrScalar,
        })
    }
}

struct NextImporter {
    module: JITModule,
    expected: DirectCallee,
}

impl CallImporter for NextImporter {
    fn import(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        callee: DirectCallee,
        signature: &Signature,
    ) -> Result<FuncRef, CallImportError> {
        if callee != self.expected {
            return Err(CallImportError::UnknownCallee);
        }
        let function = self
            .module
            .declare_function("counter_next", Linkage::Import, signature)
            .map_err(|_| CallImportError::UnknownCallee)?;
        Ok(self.module.declare_func_in_func(function, builder.func))
    }
}

fn host_isa() -> std::sync::Arc<dyn TargetIsa> {
    cranelift_codegen::isa::lookup(Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags")
}

fn facts(isa: &dyn TargetIsa, body: Body, iterator: i64, generation: u64) -> IteratorForFacts {
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, PathBuf::from("/tmp/IteratorFor.bd"));
    IteratorForFacts {
        nodes: std::array::from_fn(|index| AstNodeKey {
            unit,
            generation: SyntaxGenerationId(generation),
            node: AstNodeId(index as u32 + 1),
        }),
        iterator,
        body,
        element_type: types::I32,
        payload_type: types::I32,
        call_conv: isa.default_call_conv(),
    }
}

/// Lower and run the loop over a counter yielding `1..=last`; returns the loop result, the
/// number of `Next()` calls and the emitted CLIF.
fn run(body: Body, last: i32, index: u32) -> (i32, i32, Function) {
    let isa = host_isa();
    assert_eq!(isa.pointer_type(), types::I64, "the fixture passes the iterator as a 64-bit address");
    let counter = Box::into_raw(Box::new(Counter { next: 1, last, calls: 0 }));
    let facts = facts(isa.as_ref(), body, counter as i64, 40 + u64::from(index));
    let emitter = FunctionEmitter::new(isa.as_ref());
    let signature = emitter.signature([], [types::I32]);
    let mut builder = JITBuilder::with_isa(isa.clone(), default_libcall_names());
    builder.symbol("counter_next", counter_next as *const u8);
    let mut importer = NextImporter { module: JITModule::new(builder), expected: facts.callee() };
    let function = emitter
        .emit_statement_with_call_importer(
            UserFuncName::user(0, index),
            signature.clone(),
            &facts,
            facts.nodes[0],
            &mut importer,
        )
        .expect("verified iterator for");
    let function_id = importer.module.declare_function("iterator_for", Linkage::Local, &signature).expect("declare");
    let mut context = importer.module.make_context();
    context.func = function.clone();
    importer.module.define_function(function_id, &mut context).expect("define");
    importer.module.finalize_definitions().expect("finalize");
    let code = importer.module.get_finalized_function(function_id);
    let run: extern "C" fn() -> i32 = unsafe { std::mem::transmute(code) };
    let result = run();
    let calls = unsafe { (*counter).calls };
    drop(unsafe { Box::from_raw(counter) });
    (result, calls, function)
}

#[test]
fn iterator_for_binds_every_some_payload_until_none() {
    let (result, calls, function) = run(Body::Accumulate, 4, 60);
    assert_eq!(result, 10, "1 + 2 + 3 + 4");
    assert_eq!(calls, 5, "four `Some` results and the terminating `None`");
    let clif = function.display().to_string();
    assert!(clif.contains("call fn"), "the header calls Next():\n{clif}");
    assert!(clif.contains("brif"), "the header branches on the Option tag:\n{clif}");
    assert!(clif.contains("trapz"), "a null iterator traps before the first Next():\n{clif}");
}

#[test]
fn iterator_for_over_an_immediately_empty_iterator_skips_the_body() {
    let (result, calls, _) = run(Body::Accumulate, 0, 61);
    assert_eq!(result, 0);
    assert_eq!(calls, 1, "only the terminating `None`");
}

#[test]
fn break_leaves_the_iterator_loop_without_another_next_call() {
    let (result, calls, _) = run(Body::BreakAfterFirst, 4, 62);
    assert_eq!(result, 1, "the body ran once");
    assert_eq!(calls, 1, "break does not call Next() again");
}

#[test]
fn continue_returns_to_the_next_call() {
    let (result, calls, _) = run(Body::ContinueBeforeAccumulate, 4, 63);
    assert_eq!(result, 0, "continue skips the accumulation every iteration");
    assert_eq!(calls, 5, "continue still drives the iterator to `None`");
}

#[test]
fn a_some_payload_that_differs_from_the_loop_variable_is_an_exact_keyed_error() {
    let isa = host_isa();
    let mut facts = facts(isa.as_ref(), Body::Accumulate, 0, 64);
    facts.payload_type = types::I64;
    let emitter = FunctionEmitter::new(isa.as_ref());
    let mut builder = JITBuilder::with_isa(isa.clone(), default_libcall_names());
    builder.symbol("counter_next", counter_next as *const u8);
    let mut importer = NextImporter { module: JITModule::new(builder), expected: facts.callee() };
    let error = emitter
        .emit_statement_with_call_importer(
            UserFuncName::user(0, 64),
            emitter.signature([], [types::I32]),
            &facts,
            facts.nodes[0],
            &mut importer,
        )
        .expect_err("an Option payload of another type must not lower");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected lowering error");
    };
    assert_eq!(error.key(), facts.nodes[3]);
    assert_eq!(error.kind(), LoweringErrorKind::InvalidIteratorFor);
}
