//! WebAssembly GC storage for the shared observer-list lowering.

use super::*;

pub(crate) const SUBSCRIBE: &str = "observer_subscribe";
pub(crate) const TAIL: &str = "observer_tail";
pub(crate) const WASM_SUBSCRIBE: &str = "__beskid_browser_observer_subscribe";
pub(crate) const WASM_TAIL: &str = "__beskid_browser_observer_tail";

pub(crate) const NEXT: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observers {
    pub(crate) node: usize,
    pub(crate) callback: usize,
    /// (record layout, GC field index, native ABI offset)
    pub(crate) owners: Vec<(usize, u32, u32)>,
}

pub(super) fn emit(functions: &mut Functions, observers: &Observers, recordTypes: &[u32]) {
    use ValType::{I32, I64};
    let node = recordTypes[observers.node];
    let callback = recordTypes[observers.callback];
    let nodeRef = reference(node, true);
    let eqRef = ValType::Ref(RefType::EQREF);

    // The shared native lowering reads the tail before delivering callbacks. Keep the same
    // list snapshot here so reentrant appends only affect the next delivery.
    let tail = functions.declarations.len();
    functions.defineWithLocals(None, [nodeRef], [nodeRef], [nodeRef], |body| {
        body.block(BlockType::Empty).loop_(BlockType::Empty);
        body.local_get(0).ref_is_null().br_if(1);
        body.local_get(0).struct_get(node, NEXT).local_tee(1).ref_is_null().br_if(1);
        body.local_get(1).local_set(0).br(0);
        body.end().end().local_get(0);
    });
    functions.defineWithLocals(Some(WASM_TAIL), [I32], [I32], [nodeRef], |body| {
        nullableHandleToReference(body, 0, node);
        body.call(tail);
        nullableReferenceToHandle(body, 1, node);
    });
    functions.defineWithLocals(
        Some(WASM_SUBSCRIBE), [I32, I64, I64, I32], [I32], [eqRef, nodeRef, nodeRef],
        |body| {
            body.local_get(0).call(crate::arena::LOOKUP).local_set(4);
            for &(record, field, offset) in &observers.owners {
                let owner = recordTypes[record];
                body.local_get(1).i64_const(i64::from(offset)).i64_eq();
                body.local_get(4).ref_test_non_null(HeapType::Concrete(owner)).i32_and();
                body.if_(BlockType::Empty);
                body.global_get(NEXT_HANDLE);
                loadHandle(body, 3, callback);
                body.ref_null(HeapType::Concrete(node)).local_get(2).struct_new(node);
                body.local_tee(5).ref_as_non_null().call(REMEMBER).drop();
                body.local_get(4).ref_cast_non_null(HeapType::Concrete(owner));
                body.struct_get(owner, field).call(tail).local_tee(6).ref_is_null();
                body.if_(BlockType::Empty);
                body.local_get(4).ref_cast_non_null(HeapType::Concrete(owner));
                body.local_get(5).struct_set(owner, field);
                body.else_();
                body.local_get(6).local_get(5).struct_set(node, NEXT);
                body.end().i32_const(1).return_().end();
            }
            body.i32_const(0);
        },
    );
}
