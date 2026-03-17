use crate::gc::{RawAllocation, with_current_mutation_and_root};

#[unsafe(no_mangle)]
pub extern "C-unwind" fn alloc(size: usize, type_desc_ptr: *const u8) -> *mut u8 {
    with_current_mutation_and_root(|mc, root| {
        let data = vec![0u8; size].into_boxed_slice();
        let allocation = RawAllocation { data };
        let gc_alloc = gc_arena::Gc::new(mc, allocation);
        let ptr = gc_alloc.data.as_ptr() as *mut u8;
        if !type_desc_ptr.is_null() {
            unsafe {
                std::ptr::write_unaligned(ptr.cast::<*const u8>(), type_desc_ptr);
            }
        }
        root.runtime_state.allocation_counter += 1;
        root.globals.push(gc_alloc);
        ptr
    })
}
