use criterion::{Criterion, criterion_group, criterion_main};

use abfall::Heap;
use beskid_runtime::{
    RuntimeRoot, clear_current_heap, clear_current_root, enter_runtime_scope, leave_runtime_scope, set_current_heap,
    set_current_root, str_concat, str_new,
};

fn with_runtime_scope<R>(f: impl FnOnce() -> R) -> R {
    let heap = Heap::new();
    let mut root = RuntimeRoot::new(heap.clone());

    enter_runtime_scope();
    set_current_heap(&heap);
    set_current_root(&mut root as *mut _);
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            clear_current_heap();
            clear_current_root();
            leave_runtime_scope();
        }
    }
    let _guard = Guard;
    f()
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
