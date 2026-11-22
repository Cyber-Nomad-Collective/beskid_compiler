//! A GC-traced handle space per exported invocation or suspended continuation.
//! Saving an arena captures its allocation cursor as well as its object references.
use crate::browser_heap::{Functions, NEXT_HANDLE, reference};
use wasm_encoder::{BlockType, ConstExpr, FieldType, GlobalSection, GlobalType, HeapType, RefType, StorageType, ValType};

const CURRENT: u32 = 1;
const SLOTS: u32 = 0;
const CURSOR: u32 = 1;
pub(super) const LOOKUP: u32 = 1;
const NEW: u32 = 2;
const GET: u32 = 3;
const SET: u32 = 4;
pub(super) const INTERN: u32 = 6;
pub(super) const REF_GET: &str = "__beskid_reference_get";
pub(super) const REF_INTERN: &str = "__beskid_reference_intern";
pub(super) const REFERENCE_EXPORTS: [&str; 2] = [REF_GET, REF_INTERN];
pub(super) const FIBER_GET: &str = "__beskid_fiber_current_get";
pub(super) const FIBER_SET: &str = "__beskid_fiber_current_set";
pub(super) const FIBER_EXPORTS: [&str; 2] = [FIBER_GET, FIBER_SET];
pub(super) const SCOPE_NEW: &str = "__beskid_scope_new";
pub(super) const SCOPE_GET: &str = "__beskid_scope_get";
pub(super) const SCOPE_SET: &str = "__beskid_scope_set";
pub(super) const EXPORTS: [&str; 3] = [SCOPE_NEW, SCOPE_GET, SCOPE_SET];

pub(super) fn define(f: &mut Functions) -> GlobalSection {
    use ValType::I32;
    let nullable = ValType::Ref(RefType::EQREF);
    let slots = f.types.len();
    f.types.ty().array(&StorageType::Val(nullable), true);
    let arena = f.types.len();
    f.types.ty().struct_([
        FieldType { element_type: StorageType::Val(reference(slots, false)), mutable: true },
        FieldType { element_type: StorageType::Val(I32), mutable: true },
    ]);
    let mut globals = GlobalSection::new();
    globals.global(GlobalType { val_type: I32, mutable: true, shared: false }, &ConstExpr::i32_const(1));
    globals.global(GlobalType { val_type: reference(arena, true), mutable: true, shared: false }, &ConstExpr::ref_null(HeapType::Concrete(arena)));

    globals.global(GlobalType { val_type: ValType::I64, mutable: true, shared: false }, &ConstExpr::i64_const(0));

    // REMEMBER is function zero. Before any wrapper has been entered, a standalone
    // heap fixture may allocate directly, in which case create its initial arena lazily.
    f.defineWithLocals(None, [ValType::Ref(RefType::EQREF.nullable(false))], [I32], [reference(slots, true), I32], |b| {
        b.global_get(CURRENT).ref_is_null().if_(BlockType::Empty).call(NEW).call(SET).end();
        b.global_get(CURRENT).struct_get(arena, SLOTS).local_tee(1).array_len().local_tee(2);
        b.global_get(NEXT_HANDLE).i32_eq().if_(BlockType::Empty);
        // Double the backing array, then copy live entries once. The engine checks
        // its own array limit; the compiler imposes no additional handle quota.
        b.global_get(CURRENT).local_get(2).i32_const(2).i32_mul().array_new_default(slots).struct_set(arena, SLOTS);
        b.global_get(CURRENT).struct_get(arena, SLOTS).i32_const(0).local_get(1).i32_const(0).local_get(2).array_copy(slots, slots);
        b.end();
        b.global_get(CURRENT).struct_get(arena, SLOTS).global_get(NEXT_HANDLE).local_get(0).array_set(slots);
        b.global_get(NEXT_HANDLE).global_get(NEXT_HANDLE).i32_const(1).i32_add().global_set(NEXT_HANDLE);
    });
    f.define(Some(REF_GET), [I32], [nullable], |b| {
        b.global_get(CURRENT).struct_get(arena, SLOTS).local_get(0).array_get(slots);
    });
    f.define(Some(EXPORTS[0]), [], [nullable], |b| {
        b.i32_const(1).array_new_default(slots).i32_const(1).struct_new(arena);
    });
    f.define(Some(EXPORTS[1]), [], [nullable], |b| {
        b.global_get(CURRENT).ref_is_null().i32_eqz().if_(BlockType::Empty);
        b.global_get(CURRENT).global_get(NEXT_HANDLE).struct_set(arena, CURSOR).end().global_get(CURRENT);
    });
    f.define(Some(EXPORTS[2]), [nullable], [], |b| {
        b.call(GET).drop();
        b.local_get(0).ref_cast_nullable(HeapType::Concrete(arena)).global_set(CURRENT);
        b.global_get(CURRENT).ref_is_null().if_(BlockType::Result(I32)).i32_const(1).else_();
        b.global_get(CURRENT).struct_get(arena, CURSOR).end().global_set(NEXT_HANDLE);
    });
    f.define(Some(crate::browser_heap::RESET_HANDLES), [], [], |b| {
        b.ref_null(RefType::EQREF.heap_type).call(SET);
    });
    // An exported graph may re-enter a different invocation. A cached handle is usable
    // only if that arena still owns the same object. Recover aliases by reference identity.
    f.defineWithLocals(Some(REF_INTERN), [nullable, I32], [I32], [I32], |b| {
        b.local_get(0).ref_is_null().if_(BlockType::Empty).unreachable().end();
        b.local_get(1).global_get(NEXT_HANDLE).i32_lt_u().if_(BlockType::Empty);
        b.local_get(1).call(LOOKUP).local_get(0).ref_eq().if_(BlockType::Empty).local_get(1).return_().end().end();
        b.i32_const(1).local_set(2);
        b.block(BlockType::Empty).loop_(BlockType::Empty);
        b.local_get(2).global_get(NEXT_HANDLE).i32_ge_u().br_if(1);
        b.local_get(2).call(LOOKUP).local_get(0).ref_eq().if_(BlockType::Empty).local_get(2).return_().end();
        b.local_get(2).i32_const(1).i32_add().local_set(2).br(0).end().end();
        b.local_get(0).ref_as_non_null().call(crate::browser_heap::REMEMBER);
    });
    f.define(Some(FIBER_GET), [], [ValType::I64], |b| { b.global_get(2); });
    f.define(Some(FIBER_SET), [ValType::I64], [], |b| { b.local_get(0).global_set(2); });
    globals
}

pub(super) fn release(f: &mut Functions) {
    use ValType::I32;
    // The storage array and arena are the first two heap types declared in define().
    let slots = 0;
    let arena = 1;
    f.define(Some(crate::browser_heap::RELEASE_HANDLE), [I32], [], |b| {
        b.local_get(0).call(LOOKUP).ref_is_null().if_(BlockType::Empty).unreachable().end();
        b.global_get(CURRENT).struct_get(arena, SLOTS).local_get(0).ref_null(RefType::EQREF.heap_type).array_set(slots);
        b.block(BlockType::Empty).loop_(BlockType::Empty);
        b.global_get(NEXT_HANDLE).i32_const(1).i32_le_u().br_if(1);
        b.global_get(NEXT_HANDLE).i32_const(1).i32_sub().call(LOOKUP).ref_is_null().i32_eqz().br_if(1);
        b.global_get(NEXT_HANDLE).i32_const(1).i32_sub().global_set(NEXT_HANDLE).br(0);
        b.end().end();
    });
}
