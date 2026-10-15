//! 0.6.2 write-barrier emission (execution/runtime/memory-and-gc-runtime-contract, "Write
//! barrier emission in generated code"): a store that overwrites a GC-managed field or element
//! of an existing object loads `beskid_rt_v5_gc_barrier_active`, branches on it and calls
//! `beskid_rt_v5_gc_write_barrier(slot, value)` in a cold block before the store; scalar stores
//! and initialization stores carry no barrier, and a store target without a managed-reference
//! fact does not lower.

use super::support::*;
use beskid_isle::{
    AssignmentKind, FieldLayout, FunctionEmissionError, LoweringErrorKind, ManagedReferenceFact,
    ManagedStructAllocation, NodeKind, StructLayout,
};

const BARRIER: &str = "beskid_rt_v5_gc_write_barrier";
const ACTIVE: &str = "beskid_rt_v5_gc_barrier_active";

/// The CLIF of each lowered function whose symbol starts with one of `prefixes`, lowered as one
/// module (so module emission applies root elision), each checked by the stock verifier.
fn lower_module(source: &str, symbols: &[&str]) -> Vec<String> {
    let (input, isa, root) = item_fixture_with_root(source);
    let functions = find_function_definitions(input.database(), root);
    assert_eq!(functions.len(), symbols.len(), "fixture function count");
    let items = functions
        .iter()
        .zip(symbols)
        .map(|(key, symbol)| SyntaxModuleItem { key: *key, symbol: (*symbol).into() })
        .collect::<Vec<_>>();
    let artifact = lower_syntax_program(&input, isa.as_ref(), &items)
        .unwrap_or_else(|error| panic!("barrier fixture lowers: {error:?}"));
    symbols
        .iter()
        .map(|symbol| {
            let function = artifact
                .functions
                .iter()
                .find(|function| function.name.starts_with(symbol))
                .unwrap_or_else(|| panic!("lowered {symbol}"));
            cranelift_codegen::verify_function(&function.function, isa.as_ref())
                .unwrap_or_else(|errors| panic!("{symbol} is stock-verifier clean: {errors}"));
            function.function.display().to_string()
        })
        .collect()
}

/// The function reference that `clif` declares for `symbol` (`fnN = %symbol sigM`).
fn function_ref(clif: &str, symbol: &str) -> Option<String> {
    clif.lines()
        .find(|line| line.contains(&format!("= %{symbol} ")))
        .and_then(|line| line.split('=').next())
        .map(|name| name.trim().to_owned())
}

/// The blocks of `clif` as (header line, body lines).
fn blocks(clif: &str) -> Vec<(String, Vec<String>)> {
    let mut blocks: Vec<(String, Vec<String>)> = Vec::new();
    for line in clif.lines() {
        if line.starts_with("block") {
            blocks.push((line.to_owned(), Vec::new()));
        } else if let Some((_, body)) = blocks.last_mut() {
            // Drop the `; v5 = 9` constant annotations the display appends.
            let line = line.split(';').next().unwrap_or(line).trim();
            if !line.is_empty() && line != "}" {
                body.push(line.to_owned());
            }
        }
    }
    blocks
}

fn calls_to(clif: &str, symbol: &str) -> usize {
    let Some(name) = function_ref(clif, symbol) else { return 0 };
    clif.lines().filter(|line| line.contains(&format!("call {name}("))).count()
}

/// Every barrier call sits alone in a cold block that jumps to a block whose first instruction
/// stores the barrier's `value` operand to its `slot` operand; the block that branches to the
/// cold block loads the barrier-active word. Returns the number of guarded stores.
fn assert_guarded_stores(clif: &str) -> usize {
    let barrier = function_ref(clif, BARRIER).unwrap_or_else(|| panic!("barrier import: {clif}"));
    assert!(clif.contains(&format!("%{ACTIVE}")), "the barrier-active word is a symbol global: {clif}");
    let blocks = blocks(clif);
    let mut guarded = 0;
    for (header, body) in &blocks {
        let Some(call) = body.iter().find(|line| line.starts_with(&format!("call {barrier}("))) else {
            assert!(!body.iter().any(|line| line.contains(&format!("call {barrier}("))), "{clif}");
            continue;
        };
        assert!(header.contains(" cold"), "the barrier call is in a cold block: {header}\n{clif}");
        assert_eq!(body.len(), 2, "the cold block only calls the barrier and jumps: {body:?}");
        let operands = call
            .trim_start_matches(&format!("call {barrier}("))
            .trim_end_matches(')')
            .split(", ")
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let [slot, value] = operands.as_slice() else { panic!("barrier takes slot and value: {call}") };
        assert!(!call.contains(" = call"), "the barrier returns no value: {call}");
        let successor = body[1].strip_prefix("jump ").unwrap_or_else(|| panic!("cold block jumps: {body:?}"));
        let successor = successor.split('(').next().unwrap_or(successor);
        let (_, store_body) = blocks
            .iter()
            .find(|(candidate, _)| candidate.starts_with(&format!("{successor}:")) || candidate.starts_with(&format!("{successor}(")))
            .unwrap_or_else(|| panic!("successor {successor}: {clif}"));
        let store = store_body.first().unwrap_or_else(|| panic!("the successor stores: {clif}"));
        assert!(
            (store.starts_with("store ") || store.starts_with("store.")) && store.ends_with(&format!(" {value}, {slot}")),
            "the store of the barrier's value to its slot follows the barrier: {store}\n{clif}"
        );
        let (_, guard_body) = blocks
            .iter()
            .find(|(_, candidate)| {
                let cold = header.split([':', ' ']).next().unwrap_or("");
                candidate.last().is_some_and(|line| line.starts_with("brif ") && line.contains(&format!(" {cold}, ")))
            })
            .unwrap_or_else(|| panic!("a block branches to the cold block: {clif}"));
        let brif = guard_body.last().expect("guard branch");
        let flag = brif.trim_start_matches("brif ").split(',').next().expect("branch condition").trim().to_owned();
        let load = guard_body
            .iter()
            .position(|line| line.starts_with(&format!("{flag} = load.i64 ")) || line.starts_with(&format!("{flag} = load.i32 ")))
            .unwrap_or_else(|| panic!("the branch tests the loaded barrier-active word: {guard_body:?}"));
        assert!(
            !guard_body[load..].iter().any(|line| line.contains("call ")),
            "no call between the flag load and the branch: {guard_body:?}"
        );
        assert!(!store_body.iter().take(1).any(|line| line.contains("call ")), "the idle path stores without a call");
        guarded += 1;
    }
    guarded
}

#[test]
fn managed_field_store_is_guarded() {
    let set = &lower_module(
        "type Leaf { i64 v } type Node { Leaf next, i64 value } unit Set(mut Node a, Leaf b) { a.next = b; return; }",
        &["Set"],
    )[0];
    assert_eq!(assert_guarded_stores(set), 1, "{set}");
    assert_eq!(calls_to(set, BARRIER), 1, "{set}");
}

#[test]
fn scalar_field_and_literals_carry_no_barrier() {
    let build = &lower_module(
        r#"
type Leaf { i64 v }
type Node { Leaf next, i64 value }
i64 Build(mut Node a) {
    a.value = 1_i64;
    Node c = Node { next: a.next, value: 2_i64 };
    string[] xs = ["p", "q"];
    return c.value;
}
"#,
        &["Build"],
    )[0];
    assert!(!build.contains(ACTIVE), "{build}");
    assert!(!build.contains(BARRIER), "{build}");
    assert!(!build.contains("beskid_rt_v5_array_write_barrier"), "{build}");
}

#[test]
fn managed_index_store_is_guarded_without_the_array_barrier() {
    let put = &lower_module("unit Put(string[] xs, i64 i, string s) { xs[i] = s; return; }", &["Put"])[0];
    assert_eq!(assert_guarded_stores(put), 1, "{put}");
    assert!(put.contains("heap_oob"), "the element store stays bounds-checked: {put}");
    assert!(!put.contains("beskid_rt_v5_array_write_barrier"), "{put}");
}

#[test]
fn scalar_index_store_carries_no_barrier() {
    let put = &lower_module("unit Put(i64[] xs, i64 i) { xs[i] = 7_i64; return; }", &["Put"])[0];
    assert!(!put.contains(ACTIVE) && !put.contains(BARRIER), "{put}");
}

#[test]
fn leaf_root_elision_survives_the_barrier() {
    let set = &lower_module(
        "type Leaf { i64 v } type Node { Leaf next, i64 value } unit Set(mut Node a, Leaf b) { a.next = b; return; }",
        &["Set"],
    )[0];
    assert_eq!(calls_to(set, "gc_register_root") + calls_to(set, "gc_unregister_root"), 0, "{set}");
    assert_eq!(calls_to(set, BARRIER), 1, "{set}");
}

/// A pointer-typed field assignment `(Holder { slot: 7 }).slot = 9` whose target fact is chosen
/// by the test, lowered through the stock-verified function emitter.
struct FieldStoreFacts {
    nodes: [AstNodeKey; 5],
    pointer: cranelift_codegen::ir::Type,
    target: Option<ManagedReferenceFact>,
}

impl NodeFacts for FieldStoreFacts {
    fn assignment_kind(&self, key: AstNodeKey) -> Option<AssignmentKind> {
        (key == self.nodes[0]).then_some(AssignmentKind::Field)
    }
    fn node_kind(&self, key: AstNodeKey) -> Option<NodeKind> {
        let index = self.nodes.iter().position(|node| *node == key)?;
        Some(match index {
            0 => NodeKind::AssignExpression,
            1 => NodeKind::FieldExpression,
            2 => NodeKind::StructLiteralExpression,
            _ => NodeKind::LiteralExpression,
        })
    }
    fn literal_kind(&self, key: AstNodeKey) -> Option<beskid_isle::LiteralKind> {
        (key == self.nodes[3] || key == self.nodes[4]).then_some(beskid_isle::LiteralKind::Integer)
    }
    fn integer_literal(&self, key: AstNodeKey) -> Option<i64> {
        (key == self.nodes[3]).then_some(7).or_else(|| (key == self.nodes[4]).then_some(9))
    }
    fn child(&self, key: AstNodeKey, index: u8) -> Option<AstNodeKey> {
        if key == self.nodes[0] {
            [self.nodes[1], self.nodes[4]].get(usize::from(index)).copied()
        } else if key == self.nodes[1] {
            (index == 0).then_some(self.nodes[2])
        } else {
            None
        }
    }
    fn scalar_type(&self, key: AstNodeKey) -> Option<cranelift_codegen::ir::Type> {
        self.nodes.contains(&key).then_some(self.pointer)
    }
    fn struct_fields(&self, key: AstNodeKey) -> Option<Vec<AstNodeKey>> {
        (key == self.nodes[2]).then(|| vec![self.nodes[3]])
    }
    fn struct_layout(&self, key: AstNodeKey) -> Option<StructLayout> {
        (key == self.nodes[1] || key == self.nodes[2])
            .then(|| StructLayout::new(24, 3, vec![FieldLayout::new(self.pointer, 16)]))
    }
    fn managed_struct_allocation(&self, key: AstNodeKey) -> Option<ManagedStructAllocation> {
        (key == self.nodes[2])
            .then(|| ManagedStructAllocation { allocation_request_symbol: "__barrier_holder_request".into() })
    }
    fn field_index(&self, key: AstNodeKey) -> Option<u32> {
        (key == self.nodes[1]).then_some(0)
    }
    fn managed_reference(&self, key: AstNodeKey) -> Option<ManagedReferenceFact> {
        if key == self.nodes[1] {
            self.target
        } else {
            (key == self.nodes[3] || key == self.nodes[4]).then_some(ManagedReferenceFact::NativeOrScalar)
        }
    }
}

fn emit_field_store(target: Option<ManagedReferenceFact>) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let isa = isa::lookup(target_lexicon::Triple::host())
        .expect("host ISA")
        .finish(settings::Flags::new(settings::builder()))
        .expect("host flags");
    let db = BeskidDatabase::default();
    let unit = SourceUnitId::new(&db, std::path::PathBuf::from("/tmp/WriteBarrier.bd"));
    let generation = SyntaxGenerationId(62);
    let facts = FieldStoreFacts {
        nodes: std::array::from_fn(|index| AstNodeKey { unit, generation, node: AstNodeId(index as u32 + 1) }),
        pointer: isa.pointer_type(),
        target,
    };
    let emitter = FunctionEmitter::new(isa.as_ref());
    emitter.emit_expression(UserFuncName::user(0, 62), emitter.signature([], [isa.pointer_type()]), &facts, facts.nodes[0])
}

#[test]
fn guarded_store_shape_from_the_managed_reference_fact() {
    let managed = emit_field_store(Some(ManagedReferenceFact::GcManaged)).expect("verified guarded field store");
    let clif = managed.display().to_string();
    assert_eq!(assert_guarded_stores(&clif), 1, "{clif}");
    let native = emit_field_store(Some(ManagedReferenceFact::NativeOrScalar)).expect("verified plain field store");
    let clif = native.display().to_string();
    assert!(!clif.contains(ACTIVE) && !clif.contains(BARRIER), "{clif}");
}

#[test]
fn field_store_without_a_managed_reference_fact_is_a_lowering_error() {
    let error = emit_field_store(None).expect_err("a store target without a managed-reference fact must not lower");
    let FunctionEmissionError::Lowering(error) = error else {
        panic!("expected a lowering error, got {error:?}");
    };
    assert_eq!(error.kind(), LoweringErrorKind::MissingManagedReferenceFact);
}
