use criterion::{Criterion, criterion_group, criterion_main};

use beskid_runtime::{
    RuntimeRoot, RuntimeState, clear_current_mutation, clear_current_root, enter_runtime_scope,
    leave_runtime_scope, set_current_mutation, set_current_root, str_concat, str_new,
};
use gc_arena::{Arena, DynamicRootSet, Rootable};

fn with_runtime_scope<R>(f: impl for<'gc> FnOnce() -> R) -> R {
    type BenchArena = Arena<Rootable![RuntimeRoot<'_>]>;
    let mut arena = BenchArena::new(|mc| RuntimeRoot {
        globals: Vec::new(),
        dynamic_roots: DynamicRootSet::new(mc),
        runtime_state: RuntimeState::default(),
    });

    arena.mutate_root(|mc, root| {
        enter_runtime_scope();
        set_current_mutation(mc as *const _ as *mut _);
        set_current_root(root as *mut _);
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                clear_current_mutation();
                clear_current_root();
                leave_runtime_scope();
            }
        }
        let _guard = Guard;
        f()
    })
}

fn bench_string_concat(c: &mut Criterion) {
    c.bench_function("runtime/str_concat_64b", |b| {
        b.iter(|| {
            with_runtime_scope(|| {
                let left = str_new(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".as_ptr(), 32);
                let right = str_new(b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".as_ptr(), 32);
                let _ = str_concat(left, right);
            });
        })
    });
}

criterion_group!(runtime_micro, bench_string_concat);
criterion_main!(runtime_micro);
