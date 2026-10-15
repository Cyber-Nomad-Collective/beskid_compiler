#include "boundary.h"
#include <assert.h>
#include <string.h>
#define CHECK(f,T,lo,hi) do { assert(f((T)(lo))==(T)(lo)); assert(f((T)(hi))==(T)(hi)); assert(f(0)==0); } while(0)
int main(void) {
 CHECK(glue_i8,int8_t,INT8_MIN,INT8_MAX); CHECK(glue_i16,int16_t,INT16_MIN,INT16_MAX);
 CHECK(glue_i32,int32_t,INT32_MIN,INT32_MAX); CHECK(glue_i64,int64_t,INT64_MIN,INT64_MAX);
 CHECK(glue_u8,uint8_t,0,UINT8_MAX); CHECK(glue_u16,uint16_t,0,UINT16_MAX);
 CHECK(glue_u32,uint32_t,0,UINT32_MAX); CHECK(glue_u64,uint64_t,0,UINT64_MAX);
 uint32_t fb=0x80000000u, fr; float f; memcpy(&f,&fb,4); f=glue_f32(f); memcpy(&fr,&f,4); assert(fr==fb);
 uint64_t db=UINT64_C(0x7ff8123456789abc),dr; double d; memcpy(&d,&db,8); d=glue_f64(d); memcpy(&dr,&d,8); assert(dr==db);
 assert(glue_bool(1)==0 && glue_bool(255)!=0); assert(glue_char(0x1f600)==0 && glue_char(0xd800)!=0);
 const uint8_t text[]={65,0,0xf0,0x9f,0x98,0x80}; assert(glue_utf8(text,sizeof(text))==0);
 assert(glue_bytes(text,sizeof(text))==0); glue_unit(); assert(glue_native_width(UINTPTR_MAX,64)==0);
 uint64_t owner=glue_allocate(4); GlueByteView v={0}; assert(owner!=0 && glue_owned_view(owner,&v)==0 && v.length==4);
 assert(glue_handle(owner,7,11)==0); assert(glue_release(owner)==0 && glue_release(owner)!=0);
 assert(glue_failure(1)==2 && glue_failure(2)==3);
 return 0;
}
