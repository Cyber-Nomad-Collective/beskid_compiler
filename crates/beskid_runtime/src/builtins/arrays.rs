use beskid_abi::BeskidArray;

use super::alloc::alloc;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn array_new(elem_size: usize, len: usize) -> *mut BeskidArray {
    let size = std::mem::size_of::<BeskidArray>();
    let allocation = alloc(size, std::ptr::null());
    if allocation.is_null() {
        panic!("array allocation failed");
    }
    let target = allocation.cast::<BeskidArray>();

    // Avoid unused warnings when backing storage is disabled.
    let _ = elem_size;

    #[cfg(feature = "arrays_backing")]
    let data_ptr = {
        let bytes = elem_size.saturating_mul(len);
        let ptr = alloc(bytes, std::ptr::null());
        if ptr.is_null() && bytes > 0 {
            panic!("array backing allocation failed");
        }
        ptr
    };

    #[cfg(not(feature = "arrays_backing"))]
    let data_ptr = std::ptr::null_mut();

    unsafe {
        target.write(BeskidArray {
            ptr: data_ptr,
            len,
            cap: len,
        });
    }
    target
}
