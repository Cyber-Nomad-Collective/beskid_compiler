use beskid_abi::BeskidArray;

use super::alloc::alloc;

#[unsafe(no_mangle)]
pub extern "C-unwind" fn array_new(_elem_size: usize, len: usize) -> *mut BeskidArray {
    let size = std::mem::size_of::<BeskidArray>();
    let allocation = alloc(size, std::ptr::null());
    if allocation.is_null() {
        panic!("array allocation failed");
    }
    let target = allocation.cast::<BeskidArray>();
    unsafe {
        target.write(BeskidArray {
            ptr: std::ptr::null_mut(),
            len,
            cap: len,
        });
    }
    target
}
