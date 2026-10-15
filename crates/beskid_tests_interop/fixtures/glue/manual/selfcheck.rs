#[path = "native.rs"] mod native;
use native::*;
#[test] fn fixed_integer_boundaries() {
    macro_rules! check { ($f:ident,$t:ty) => { for v in [<$t>::MIN,0,1,<$t>::MAX] { assert_eq!($f(v),v); } }; }
    check!(glue_i8,i8); check!(glue_i16,i16); check!(glue_i32,i32); check!(glue_i64,i64);
    check!(glue_u8,u8); check!(glue_u16,u16); check!(glue_u32,u32); check!(glue_u64,u64);
}
#[test] fn ieee_bits_preserved() {
    for b in [0u32,0x80000000,0x00000001,0x7f7fffff,0x7f800000,0xff800000,0x7fc12345] { assert_eq!(glue_f32(f32::from_bits(b)).to_bits(),b); }
    for b in [0u64,0x8000000000000000,1,0x7fefffffffffffff,0x7ff0000000000000,0xfff0000000000000,0x7ff8123456789abc] { assert_eq!(glue_f64(f64::from_bits(b)).to_bits(),b); }
}
#[test] fn checked_bool_char_native_width_unit() {
    assert_eq!(glue_bool(0),OK); assert_eq!(glue_bool(1),OK); assert_eq!(glue_bool(2),INVALID); assert_eq!(glue_bool(255),INVALID);
    for v in [0,65,0x1f600,0x10ffff] { assert_eq!(glue_char(v),OK); }
    for v in [0xd800,0xdfff,0x110000,u32::MAX] { assert_eq!(glue_char(v),INVALID); }
    assert_eq!(glue_native_width(usize::MAX,usize::BITS),OK); assert_eq!(glue_native_width(0,32),INVALID);
    glue_unit();
}
#[test] fn bounded_utf8_and_bytes() {
    unsafe {
      assert_eq!(glue_utf8(std::ptr::null(),0),OK); assert_eq!(glue_utf8(std::ptr::null(),1),INVALID);
      let s=b"a\0\xf0\x9f\x98\x80"; assert_eq!(glue_utf8(s.as_ptr(),s.len()),OK);
      assert_eq!(glue_utf8([0xff].as_ptr(),1),INVALID);
      assert_eq!(glue_bytes(s.as_ptr(),s.len()),OK); assert_eq!(glue_bytes(s.as_ptr(),LIMIT+1),INVALID);
      assert_eq!(glue_bytes(s.as_ptr(),0),OK);
    }
}
#[test] fn owner_generation_release_exactly_once() {
    let token=glue_allocate(19); assert_ne!(token,0); assert_eq!(glue_handle(token,SESSION,SHAPE),OK);
    assert_eq!(glue_handle(token,SESSION+1,SHAPE),INVALID); assert_eq!(glue_handle(token,SESSION,SHAPE+1),INVALID);
    assert_eq!(glue_release(token),OK); assert_eq!(glue_release(token),INVALID);
    let next=glue_allocate(19); assert_ne!(token,next); assert_eq!(glue_handle(token,SESSION,SHAPE),INVALID); assert_eq!(glue_release(next),OK);
    assert_eq!(glue_handle(0,SESSION,SHAPE),INVALID);
}
#[test] fn foreign_failure_and_panic_are_statuses() { assert_eq!(glue_failure(0),OK); assert_eq!(glue_failure(1),FOREIGN_ERROR); assert_eq!(glue_failure(2),PANIC); }
#[test] fn transferred_view_retains_owner_until_explicit_release() {
    let token=glue_allocate(4); let mut result=ByteView{pointer:std::ptr::null(),length:0};
    unsafe { assert_eq!(glue_owned_view(token,&mut result),OK); assert_eq!(result.length,4);
      assert_eq!(glue_bytes(result.pointer,result.length),OK); }
    assert_eq!(glue_release(token),OK);
    unsafe { assert_eq!(glue_owned_view(token,&mut result),INVALID); }
    // Released pointers are never dereferenced; a stale checked token rejects first.
}
