//! Manual C ABI fixture, not a production Glue implementation.
use std::{collections::BTreeMap, panic::{catch_unwind, AssertUnwindSafe}, sync::{Mutex, OnceLock}};
pub const OK:i32=0; pub const INVALID:i32=1; pub const FOREIGN_ERROR:i32=2; pub const PANIC:i32=3;
pub const LIMIT:usize=16*1024*1024; pub const SESSION:u64=7; pub const SHAPE:u64=11;
macro_rules! scalar { ($name:ident,$ty:ty) => { #[unsafe(no_mangle)] pub extern "C" fn $name(value:$ty)->$ty { value } }; }
scalar!(glue_i8,i8); scalar!(glue_i16,i16); scalar!(glue_i32,i32); scalar!(glue_i64,i64);
scalar!(glue_u8,u8); scalar!(glue_u16,u16); scalar!(glue_u32,u32); scalar!(glue_u64,u64);
scalar!(glue_f32,f32); scalar!(glue_f64,f64);
#[unsafe(no_mangle)] pub extern "C" fn glue_bool(value:u8)->i32 { if value<=1 {OK} else {INVALID} }
#[unsafe(no_mangle)] pub extern "C" fn glue_char(value:u32)->i32 { if char::from_u32(value).is_some() {OK} else {INVALID} }
#[unsafe(no_mangle)] pub extern "C" fn glue_native_width(_value:usize,bits:u32)->i32 { if bits==usize::BITS && bits==64 {OK} else {INVALID} }
#[unsafe(no_mangle)] pub extern "C" fn glue_unit() {}
/// Caller supplies readable call-scoped storage covering len bytes. Size/null checks
/// cannot prove arbitrary addresses valid; this unsafe local ABI is never wire data.
unsafe fn view<'a>(pointer:*const u8,len:usize)->Result<&'a [u8],i32> {
    if len>LIMIT || len>isize::MAX as usize || (pointer.is_null() && len!=0) {return Err(INVALID)}
    if len==0 {return Ok(&[])}
    Ok(unsafe {std::slice::from_raw_parts(pointer,len)})
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn glue_bytes(pointer:*const u8,len:usize)->i32 { match unsafe {view(pointer,len)} {Ok(_)=>OK,Err(e)=>e} }
#[unsafe(no_mangle)] pub unsafe extern "C" fn glue_utf8(pointer:*const u8,len:usize)->i32 { match unsafe {view(pointer,len)} {Ok(v) if std::str::from_utf8(v).is_ok()=>OK,_=>INVALID} }
struct Registry { next:u64, owned:BTreeMap<u64,Vec<u8>> }
fn registry()->&'static Mutex<Registry> { static R:OnceLock<Mutex<Registry>>=OnceLock::new(); R.get_or_init(||Mutex::new(Registry{next:1,owned:BTreeMap::new()})) }
#[unsafe(no_mangle)] pub extern "C" fn glue_allocate(len:usize)->u64 {
    if len>LIMIT {return 0} let Ok(mut r)=registry().lock() else{return 0};
    let token=r.next; let Some(next)=token.checked_add(1) else{return 0};
    let mut bytes=Vec::new(); if bytes.try_reserve_exact(len).is_err() {return 0} bytes.resize(len,0);
    r.next=next; r.owned.insert(token,bytes); token
}
#[unsafe(no_mangle)] pub extern "C" fn glue_handle(token:u64,session:u64,shape:u64)->i32 {
    if session!=SESSION || shape!=SHAPE || token==0 {return INVALID}
    match registry().lock() {Ok(r) if r.owned.contains_key(&token)=>OK,_=>INVALID}
}
#[unsafe(no_mangle)] pub extern "C" fn glue_release(token:u64)->i32 {
    match registry().lock() {Ok(mut r)=>if r.owned.remove(&token).is_some(){OK}else{INVALID},_=>INVALID}
}
#[unsafe(no_mangle)] pub extern "C" fn glue_failure(mode:u32)->i32 {
    match catch_unwind(AssertUnwindSafe(||match mode {0=>OK,1=>FOREIGN_ERROR,_=>panic!("retained foreign panic fixture")})) {Ok(status)=>status,Err(_)=>PANIC}
}
#[repr(C)] pub struct ByteView {pub pointer:*const u8,pub length:usize}
/// Owner must serialize release with use of this borrowed view. Output must be
/// writable/aligned ByteView storage. No pointer is emitted into stdio messages.
#[unsafe(no_mangle)] pub unsafe extern "C" fn glue_owned_view(token:u64,output:*mut ByteView)->i32 {
    if output.is_null() || (output as usize)%std::mem::align_of::<ByteView>()!=0 {return INVALID}
    let Ok(r)=registry().lock() else{return INVALID}; let Some(bytes)=r.owned.get(&token) else{return INVALID};
    unsafe {output.write(ByteView{pointer:bytes.as_ptr(),length:bytes.len()})}; OK
}

#[unsafe(no_mangle)] pub extern "C" fn glue_f32_bits(value: f32) -> u32 { value.to_bits() }
#[unsafe(no_mangle)] pub extern "C" fn glue_f64_bits(value: f64) -> u64 { value.to_bits() }
#[unsafe(no_mangle)] pub extern "C" fn glue_f32_from_bits(value: u32) -> f32 { f32::from_bits(value) }
#[unsafe(no_mangle)] pub extern "C" fn glue_f64_from_bits(value: u64) -> f64 { f64::from_bits(value) }
#[unsafe(no_mangle)] pub unsafe extern "C" fn glue_utf8_exact(pointer: *const u8, length: usize) -> i32 {
    match unsafe { view(pointer, length) } { Ok(b"A\0\xf0\x9f\x98\x80") => OK, _ => INVALID }
}
#[unsafe(no_mangle)] pub unsafe extern "C" fn glue_bytes_exact(pointer: *const u8, length: usize) -> i32 {
    match unsafe { view(pointer, length) } { Ok([0, 128, 255, 65]) => OK, _ => INVALID }
}
