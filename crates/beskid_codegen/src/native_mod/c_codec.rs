//! CABI2 codec emission from private compiler-issued source/layout plans.
//! Managed allocations and constructors remain canonical provider/source calls.
use super::adapter_plan::*;
use anyhow::{Result, bail};
use beskid_queries::SemanticTypeId;
use std::collections::HashSet;

const C_TRANSPORT: &str = r#"
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#if defined(_WIN32)
#define MOD_EXPORT __declspec(dllexport)
#define MOD_TLS __declspec(thread)
#else
#define MOD_EXPORT __attribute__((visibility("default")))
#define MOD_TLS _Thread_local
#endif
typedef struct ModCallbacks {
 int32_t (*kind)(void*,uint64_t,uint32_t*);
 int32_t (*scalar)(void*,uint64_t,uint64_t*);
 int32_t (*text)(void*,uint64_t,const uint8_t**,size_t*);
 int32_t (*count)(void*,uint64_t,size_t*);
 int32_t (*child)(void*,uint64_t,size_t,uint64_t*);
 int32_t (*variant)(void*,uint64_t,uint32_t*);
 int32_t (*new_scalar)(void*,uint32_t,uint64_t,uint64_t*);
 int32_t (*new_text)(void*,const uint8_t*,size_t,uint64_t*);
 int32_t (*new_sequence)(void*,uint32_t,const uint64_t*,size_t,uint64_t*);
 int32_t (*new_variant)(void*,uint32_t,uint32_t,const uint64_t*,size_t,uint64_t*);
 int32_t (*service)(void*,const uint8_t*,size_t,const uint64_t*,size_t,uint64_t*);
} ModCallbacks;
typedef struct ModHeader {
 uint32_t version,bytes;
 uint64_t generation,invocation;
 void *context;
 const ModCallbacks *callbacks;
 uint64_t request,factory_request;
} ModHeader;
typedef union ModValue {
 uint8_t u8; int8_t i8; uint16_t u16; int16_t i16;
 uint32_t u32; int32_t i32; uint64_t u64; int64_t i64;
 float f32; double f64; void *pointer;
} ModValue;
typedef struct ModScope {
 const ModHeader *header;
 size_t roots[65536]; size_t root_count,work,bytes;
 int32_t failure;
 struct ModScope *previous;
} ModScope;
static MOD_TLS ModScope *mod_active;
static int mod_step(ModScope *s,unsigned depth) {
 if (!s || s->failure || depth>128 || s->work>=1048576) return 0;
 ++s->work; return 1;
}
static int mod_bytes(ModScope *s,size_t count) {
 if (count>16777216 || s->bytes>16777216-count) return 0;
 s->bytes+=count; return 1;
}
static int mod_utf8(const uint8_t *p,size_t n) {
 if (!p && n) return 0;
 for (size_t i=0;i<n;) {
  uint32_t c=p[i++],minimum; unsigned tail;
  if(c<128) continue;
  if(c>=194 && c<=223){c&=31;tail=1;minimum=128;}
  else if(c>=224 && c<=239){c&=15;tail=2;minimum=2048;}
  else if(c>=240 && c<=244){c&=7;tail=3;minimum=65536;}
  else return 0;
  if(tail>n-i) return 0;
  while(tail--){uint32_t t=p[i++];if((t&192)!=128)return 0;c=(c<<6)|(t&63);}
  if(c<minimum || c>1114111 || (c>=55296 && c<=57343))return 0;
 }
 return 1;
}
static int mod_header(const ModHeader *h) {
 if(!h || h->version!=2 || h->bytes!=sizeof(*h) || !h->generation || !h->invocation || !h->callbacks)return 0;
 const ModCallbacks *c=h->callbacks;
 return c->kind && c->scalar && c->text && c->count && c->child && c->variant &&
 c->new_scalar && c->new_text && c->new_sequence && c->new_variant && c->service;
}
"#;

fn emit_runtime_support(plan: &NativeAdapterPlan, output: &mut String) -> Result<()> {
    use std::fmt::Write;
    let runtime = &plan.runtime;
    for name in [
        &runtime.string_construct,
        &runtime.string_length,
        &runtime.root,
        &runtime.resolve_root,
        &runtime.unroot,
        &runtime.array_allocate_rooted,
        &runtime.construction_finish,
        &runtime.write_barrier,
        &runtime.array_write_barrier,
    ] {
        symbol(name)?;
    }
    output.push_str(C_TRANSPORT);
    writeln!(output, "_Static_assert(sizeof(void*)=={},\"adapter target mismatch\");", plan.pointer_bytes)?;
    writeln!(output, "extern void *{}(const uint8_t*,size_t);", runtime.string_construct)?;
    writeln!(output, "extern size_t {}(void*);", runtime.string_length)?;
    writeln!(output, "extern size_t {}(void*);", runtime.root)?;
    writeln!(output, "extern void *{}(size_t);", runtime.resolve_root)?;
    writeln!(output, "extern void {}(size_t);", runtime.unroot)?;
    writeln!(output, "extern void *{}(void*,void*);", runtime.array_allocate_rooted)?;
    writeln!(output, "extern uint8_t {}(void*);", runtime.construction_finish)?;
    writeln!(output, "extern void {}(void*,void*);", runtime.write_barrier)?;
    writeln!(output, "extern uint8_t {}(void*,void*);", runtime.array_write_barrier)?;
    writeln!(
        output,
        "static int mod_keep(ModScope*s,void*p){{if(!p || s->root_count>=65536)return 0;size_t h={}(p);if(!h)return 0;s->roots[s->root_count++]=h;return 1;}}",
        runtime.root
    )?;
    writeln!(
        output,
        "static void mod_drop(ModScope*s){{while(s->root_count){}(s->roots[--s->root_count]);}}",
        runtime.unroot
    )?;
    Ok(())
}

fn symbol(value: &str) -> Result<()> {
    let mut bytes = value.bytes();
    if !matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        bail!("invalid native adapter C symbol: {value:?}");
    }
    Ok(())
}

fn scalar_abi(ty: SemanticTypeId) -> Result<&'static str> {
    Ok(match ty {
        SemanticTypeId::UNIT => "uint8_t",
        SemanticTypeId::BOOL | SemanticTypeId::U8 => "uint8_t",
        SemanticTypeId::I8 => "int8_t",
        SemanticTypeId::I16 => "int16_t",
        SemanticTypeId::U16 => "uint16_t",
        SemanticTypeId::I32 => "int32_t",
        SemanticTypeId::U32 | SemanticTypeId::CHAR => "uint32_t",
        SemanticTypeId::I64 => "int64_t",
        SemanticTypeId::U64 => "uint64_t",
        SemanticTypeId::F32 => "float",
        SemanticTypeId::F64 => "double",
        SemanticTypeId::STRING => "void *",
        _ => bail!("unsupported native transport scalar {ty:?}"),
    })
}

fn scalar_transport(ty: SemanticTypeId) -> Result<(u32, &'static str)> {
    Ok(match ty {
        SemanticTypeId::UNIT => (0, "u8"),
        SemanticTypeId::BOOL => (1, "u8"),
        SemanticTypeId::I8 => (2, "i8"),
        SemanticTypeId::I16 => (2, "i16"),
        SemanticTypeId::I32 => (2, "i32"),
        SemanticTypeId::I64 => (2, "i64"),
        SemanticTypeId::U8 => (3, "u8"),
        SemanticTypeId::U16 => (3, "u16"),
        SemanticTypeId::U32 | SemanticTypeId::CHAR => (3, "u32"),
        SemanticTypeId::U64 => (3, "u64"),
        SemanticTypeId::F32 => (4, "f32"),
        SemanticTypeId::F64 => (4, "f64"),
        SemanticTypeId::STRING => (5, "pointer"),
        _ => bail!("unsupported native transport scalar {ty:?}"),
    })
}

fn emit_scalar_codec(
    plan: &NativeAdapterPlan,
    id: u32,
    ty: SemanticTypeId,
    decode: bool,
    encode: bool,
    output: &mut String,
) -> Result<()> {
    use std::fmt::Write;
    let (kind, slot) = scalar_transport(ty)?;
    let mut decoded = String::new();
    let mut encoded = String::new();
    writeln!(
        decoded,
        "static int mod_decode_{id}(ModScope*s,uint64_t h,unsigned d,ModValue*out){{uint32_t k=0;uint64_t bits=0;memset(out,0,sizeof(*out));if(!mod_step(s,d)||s->header->callbacks->kind(s->header->context,h,&k)||k!={kind})return 0;"
    )?;
    if ty == SemanticTypeId::STRING {
        writeln!(
            decoded,
            "const uint8_t*p=NULL;size_t n=0;if(s->header->callbacks->text(s->header->context,h,&p,&n)||!mod_bytes(s,n)||!mod_utf8(p,n))return 0;out->pointer={}(p,n);return mod_keep(s,out->pointer);}}",
            plan.runtime.string_construct
        )?;
        writeln!(
            encoded,
            "static int mod_encode_{id}(ModScope*s,ModValue value,unsigned d,uint64_t*out){{*out=0;if(!mod_step(s,d)||!value.pointer)return 0;size_t n={}(value.pointer);const uint8_t*p=NULL;memcpy(&p,(const uint8_t*)value.pointer+{},sizeof(p));if(!mod_bytes(s,n)||!mod_utf8(p,n))return 0;return !s->header->callbacks->new_text(s->header->context,p,n,out)&&*out;}}",
            plan.runtime.string_length, plan.runtime.string_data_offset
        )?;
        if decode {
            output.push_str(&decoded);
        }
        if encode {
            output.push_str(&encoded);
        }
        return Ok(());
    }
    if ty != SemanticTypeId::UNIT {
        decoded.push_str("if(s->header->callbacks->scalar(s->header->context,h,&bits))return 0;\n");
    }
    let guard = match ty {
        SemanticTypeId::BOOL => "bits>1",
        SemanticTypeId::U8 => "bits>UINT8_MAX",
        SemanticTypeId::U16 => "bits>UINT16_MAX",
        SemanticTypeId::U32 => "bits>UINT32_MAX",
        SemanticTypeId::CHAR => "bits>1114111 || (bits>=55296 && bits<=57343)",
        SemanticTypeId::I8 => "(int64_t)bits<INT8_MIN || (int64_t)bits>INT8_MAX",
        SemanticTypeId::I16 => "(int64_t)bits<INT16_MIN || (int64_t)bits>INT16_MAX",
        SemanticTypeId::I32 => "(int64_t)bits<INT32_MIN || (int64_t)bits>INT32_MAX",
        SemanticTypeId::F32 => "bits>UINT32_MAX",
        _ => "0",
    };
    writeln!(decoded, "if({guard})return 0;")?;
    if ty == SemanticTypeId::F32 {
        decoded.push_str("uint32_t raw=(uint32_t)bits;memcpy(&out->f32,&raw,sizeof(raw));\n");
    } else if ty == SemanticTypeId::F64 {
        decoded.push_str("memcpy(&out->f64,&bits,sizeof(bits));\n");
    } else {
        writeln!(decoded, "out->{slot}=({})bits;", scalar_abi(ty)?)?;
    }
    decoded.push_str("return 1;}\n");
    writeln!(
        encoded,
        "static int mod_encode_{id}(ModScope*s,ModValue value,unsigned d,uint64_t*out){{uint64_t bits=0;*out=0;if(!mod_step(s,d))return 0;"
    )?;
    if ty == SemanticTypeId::F32 {
        encoded.push_str("uint32_t raw=0;memcpy(&raw,&value.f32,sizeof(raw));bits=raw;\n");
    } else if ty == SemanticTypeId::F64 {
        encoded.push_str("memcpy(&bits,&value.f64,sizeof(bits));\n");
    } else if ty != SemanticTypeId::UNIT {
        writeln!(encoded, "bits=(uint64_t)value.{slot};if({guard})return 0;")?;
    }
    writeln!(encoded, "return !s->header->callbacks->new_scalar(s->header->context,{kind},bits,out)&&*out;}}")?;
    if decode {
        output.push_str(&decoded);
    }
    if encode {
        output.push_str(&encoded);
    }
    Ok(())
}

fn validate_plan(plan: &NativeAdapterPlan) -> Result<()> {
    if !matches!(plan.pointer_bytes, 4 | 8) || plan.types.len() > 65536 {
        bail!("unsupported native adapter target or type budget");
    }
    symbol(&plan.discriminator_symbol)?;
    let type_exists = |id: u32| -> Result<()> {
        if plan.types.get(id as usize).is_none() {
            bail!("missing transport type {id}");
        }
        Ok(())
    };
    for (index, ty) in plan.types.iter().enumerate() {
        if ty.id as usize != index {
            bail!("noncanonical transport type ordering");
        }
        match &ty.kind {
            NativeAdapterKind::Scalar(ty) => {
                scalar_abi(*ty)?;
            }
            NativeAdapterKind::Array { element, stride, request_getter, request_bytes, count_offset, .. } => {
                type_exists(*element)?;
                symbol(request_getter)?;
                if *stride == 0
                    || *request_bytes > 65536
                    || count_offset.checked_add(u64::from(plan.pointer_bytes)).is_none_or(|end| end > *request_bytes)
                {
                    bail!("invalid source-issued array allocation geometry");
                }
            }
            NativeAdapterKind::Record { fields } => {
                for field in fields {
                    type_exists(field.type_id)?;
                }
            }
            NativeAdapterKind::Enum { tag_bytes, variants, .. } => {
                if !matches!(tag_bytes, 1 | 2 | 4 | 8) {
                    bail!("unsupported enum tag width");
                }
                let mut tags = HashSet::new();
                for variant in variants {
                    if !tags.insert(variant.tag) || (*tag_bytes < 8 && variant.tag >= (1u64 << (*tag_bytes * 8))) {
                        bail!("invalid source-issued enum tag");
                    }
                    for field in &variant.fields {
                        type_exists(field.type_id)?;
                    }
                }
            }
        }
    }
    let mut names = HashSet::new();
    let mut constructor_results = HashSet::new();
    for constructor in &plan.constructors {
        symbol(&constructor.symbol)?;
        type_exists(constructor.result_type)?;
        if !names.insert(&constructor.symbol) {
            bail!("duplicate native constructor symbol");
        }
        if !constructor_results.insert((constructor.result_type, constructor.variant)) {
            bail!("ambiguous source constructor for native transport type");
        }
        if constructor.parameters.len() != constructor.bindings.len() {
            bail!("constructor binding arity mismatch");
        }
        let fields = match &plan.types[constructor.result_type as usize].kind {
            NativeAdapterKind::Record { fields } if constructor.variant.is_none() => fields,
            NativeAdapterKind::Enum { variants, .. } => {
                &variants
                    .get(constructor.variant.ok_or_else(|| anyhow::anyhow!("enum constructor lacks variant"))? as usize)
                    .ok_or_else(|| anyhow::anyhow!("constructor variant unavailable"))?
                    .fields
            }
            _ => bail!("constructor lacks nominal source result"),
        };
        for (parameter, binding) in constructor.parameters.iter().zip(&constructor.bindings) {
            type_exists(*parameter)?;
            let (field, expected) = match binding {
                NativeConstructorArgument::Field(index) => {
                    let field =
                        fields.get(*index as usize).ok_or_else(|| anyhow::anyhow!("constructor field unavailable"))?;
                    (field, field.type_id)
                }
                NativeConstructorArgument::ArrayElement { field, .. } => {
                    let field = fields
                        .get(*field as usize)
                        .ok_or_else(|| anyhow::anyhow!("constructor array field unavailable"))?;
                    let NativeAdapterKind::Array { element, .. } = &plan.types[field.type_id as usize].kind else {
                        bail!("constructor element binding is not an array");
                    };
                    (field, *element)
                }
            };
            if *parameter != expected {
                bail!("constructor argument differs from source field {}", field.name);
            }
        }
    }
    for entry in &plan.entries {
        if !names.insert(&entry.symbol) || entry.symbol == plan.discriminator_symbol {
            bail!("duplicate native adapter entry symbol");
        }
        for name in [&entry.symbol, &entry.method_symbol, &entry.factory_symbol] {
            symbol(name)?;
        }
        for id in [entry.receiver_type, entry.factory_request_type, entry.request_type, entry.result_type] {
            type_exists(id)?;
        }
    }
    for callback in &plan.callbacks {
        if !names.insert(&callback.symbol) || callback.symbol == plan.discriminator_symbol {
            bail!("duplicate native adapter callback symbol");
        }
        if callback.operation.is_empty() || callback.operation.len() > 4096 || callback.parameters.len() > 4096 {
            bail!("native callback operation or arity exceeds transport bounds");
        }
        symbol(&callback.symbol)?;
        type_exists(callback.result_type)?;
        for parameter in &callback.parameters {
            type_exists(*parameter)?;
        }
    }
    Ok(())
}

fn is_unit(plan: &NativeAdapterPlan, id: u32) -> bool {
    matches!(plan.types.get(id as usize).map(|t| &t.kind), Some(NativeAdapterKind::Scalar(ty)) if *ty == SemanticTypeId::UNIT)
}

fn type_abi(plan: &NativeAdapterPlan, id: u32) -> Result<&'static str> {
    match plan.types.get(id as usize).map(|t| &t.kind) {
        Some(NativeAdapterKind::Scalar(ty)) if *ty == SemanticTypeId::UNIT => Ok("void"),
        Some(NativeAdapterKind::Scalar(ty)) => scalar_abi(*ty),
        Some(_) => Ok("void *"),
        None => bail!("missing native type {id}"),
    }
}
fn type_slot(plan: &NativeAdapterPlan, id: u32) -> Result<&'static str> {
    match plan.types.get(id as usize).map(|t| &t.kind) {
        Some(NativeAdapterKind::Scalar(ty)) => Ok(scalar_transport(*ty)?.1),
        Some(_) => Ok("pointer"),
        None => bail!("missing native type {id}"),
    }
}
fn emit_fields_encode(plan: &NativeAdapterPlan, fields: &[NativeAdapterField], output: &mut String) -> Result<()> {
    use std::fmt::Write;
    writeln!(output, "uint64_t children[{}]={{0}};", fields.len().max(1))?;
    for (index, field) in fields.iter().enumerate() {
        let slot = type_slot(plan, field.type_id)?;
        if is_unit(plan, field.type_id) {
            writeln!(
                output,
                "ModValue f{index}={{0}};if(!mod_encode_{}(s,f{index},d+1,&children[{index}]))return 0;",
                field.type_id
            )?;
            continue;
        }
        writeln!(
            output,
            "ModValue f{index}={{0}};memcpy(&f{index}.{slot},(const uint8_t*)value.pointer+{},sizeof(f{index}.{slot}));if(!mod_encode_{}(s,f{index},d+1,&children[{index}]))return 0;",
            field.offset, field.type_id
        )?;
    }
    Ok(())
}
fn emit_nominal_decode(
    plan: &NativeAdapterPlan,
    id: u32,
    variant: Option<u32>,
    fields: &[NativeAdapterField],
    output: &mut String,
) -> Result<()> {
    use std::fmt::Write;
    let constructor = plan
        .constructors
        .iter()
        .find(|c| c.result_type == id && c.variant == variant)
        .ok_or_else(|| anyhow::anyhow!("native type {id} lacks source constructor for {variant:?}"))?;
    writeln!(
        output,
        "size_t n=0;if(s->header->callbacks->count(s->header->context,h,&n)||n!={})return 0;",
        fields.len()
    )?;
    for (index, field) in fields.iter().enumerate() {
        writeln!(
            output,
            "uint64_t h{index}=0;ModValue f{index}={{0}};if(s->header->callbacks->child(s->header->context,h,{index},&h{index})||!mod_decode_{}(s,h{index},d+1,&f{index}))return 0;",
            field.type_id
        )?;
    }
    let mut args = Vec::new();
    for (index, (ty, binding)) in constructor.parameters.iter().zip(&constructor.bindings).enumerate() {
        if is_unit(plan, *ty) {
            continue;
        }
        let slot = type_slot(plan, *ty)?;
        match binding {
            NativeConstructorArgument::Field(field) => args.push(format!("f{field}.{slot}")),
            NativeConstructorArgument::ArrayElement { field, index: element } => {
                let NativeAdapterKind::Array { stride, length_offset, data_offset, .. } =
                    &plan.types[fields[*field as usize].type_id as usize].kind
                else {
                    bail!("source array binding unavailable")
                };
                writeln!(
                    output,
                    "size_t count{index}=0;void*data{index}=NULL;memcpy(&count{index},(uint8_t*)f{field}.pointer+{length_offset},sizeof(size_t));memcpy(&data{index},(uint8_t*)f{field}.pointer+{data_offset},sizeof(void*));if(count{index}!={}||!data{index})return 0;ModValue a{index}={{0}};memcpy(&a{index}.{slot},(uint8_t*)data{index}+{}ULL,sizeof(a{index}.{slot}));",
                    u64::from(*element) + 1,
                    u64::from(*element) * stride
                )?;
                args.push(format!("a{index}.{slot}"));
            }
        }
    }
    writeln!(
        output,
        "out->pointer={}({});return !s->failure&&mod_keep(s,out->pointer);",
        constructor.symbol,
        args.join(",")
    )?;
    Ok(())
}
fn emit_aggregate_codec(
    plan: &NativeAdapterPlan,
    ty: &NativeAdapterType,
    decode: bool,
    encode: bool,
    output: &mut String,
) -> Result<()> {
    use std::fmt::Write;
    let id = ty.id;
    match &ty.kind {
        NativeAdapterKind::Scalar(scalar) => return emit_scalar_codec(plan, id, *scalar, decode, encode, output),
        NativeAdapterKind::Array {
            element,
            stride,
            data_offset,
            length_offset,
            request_getter,
            request_bytes,
            count_offset,
        } => {
            let slot = type_slot(plan, *element)?;
            if decode {
                writeln!(
                    output,
                    "static int mod_decode_{id}(ModScope*s,uint64_t h,unsigned d,ModValue*out){{uint32_t k=0;size_t n=0;memset(out,0,sizeof(*out));if(!mod_step(s,d)||s->header->callbacks->kind(s->header->context,h,&k)||k!=6||s->header->callbacks->count(s->header->context,h,&n)||n>1048576||n>SIZE_MAX/{stride}ULL||!mod_bytes(s,n*{stride}ULL)||s->root_count>=65536)return 0;_Alignas(max_align_t) uint8_t request[{request_bytes}];const void*prototype={request_getter}();if(!prototype)return 0;memcpy(request,prototype,sizeof(request));memcpy(request+{count_offset},&n,sizeof(n));size_t root=0;out->pointer={}(request,&root);if(!out->pointer||!root)return 0;s->roots[s->root_count++]=root;for(size_t i=0;i<n;i++){{uint64_t child=0;ModValue value={{0}};if(s->header->callbacks->child(s->header->context,h,i,&child)||!mod_decode_{element}(s,child,d+1,&value))return 0;out->pointer={}(root);if(!out->pointer)return 0;void*data=NULL;memcpy(&data,(uint8_t*)out->pointer+{data_offset},sizeof(data));if(!data)return 0;memcpy((uint8_t*)data+i*{stride}ULL,&value.{slot},sizeof(value.{slot}));",
                    plan.runtime.array_allocate_rooted, plan.runtime.resolve_root
                )?;
                if slot == "pointer" {
                    writeln!(output, "if(!{}(out->pointer,value.pointer))return 0;", plan.runtime.array_write_barrier)?;
                }
                writeln!(
                    output,
                    "}}return {}((void*)(uintptr_t)root)&&!s->failure;}}",
                    plan.runtime.construction_finish
                )?;
            }
            if encode {
                writeln!(
                    output,
                    "static int mod_encode_{id}(ModScope*s,ModValue value,unsigned d,uint64_t*out){{*out=0;if(!mod_step(s,d)||!value.pointer)return 0;size_t n=0;void*data=NULL;memcpy(&n,(const uint8_t*)value.pointer+{length_offset},sizeof(n));memcpy(&data,(const uint8_t*)value.pointer+{data_offset},sizeof(data));if(n>1048576||n>SIZE_MAX/{stride}ULL||n>SIZE_MAX/sizeof(uint64_t)||(!data&&n)||!mod_bytes(s,n*sizeof(uint64_t)))return 0;uint64_t*children=n?calloc(n,sizeof(uint64_t)):NULL;if(n&&!children)return 0;int ok=1;for(size_t i=0;i<n;i++){{ModValue child={{0}};memcpy(&child.{slot},(const uint8_t*)data+i*{stride}ULL,sizeof(child.{slot}));if(!mod_encode_{element}(s,child,d+1,&children[i])){{ok=0;break;}}}}if(ok)ok=!s->header->callbacks->new_sequence(s->header->context,{id},children,n,out)&&*out;free(children);return ok;}}"
                )?;
            }
        }
        NativeAdapterKind::Record { fields } => {
            if decode {
                writeln!(
                    output,
                    "static int mod_decode_{id}(ModScope*s,uint64_t h,unsigned d,ModValue*out){{uint32_t k=0;memset(out,0,sizeof(*out));if(!mod_step(s,d)||s->header->callbacks->kind(s->header->context,h,&k)||k!=7)return 0;"
                )?;
                emit_nominal_decode(plan, id, None, fields, output)?;
                output.push_str("}\n");
            }
            if encode {
                writeln!(
                    output,
                    "static int mod_encode_{id}(ModScope*s,ModValue value,unsigned d,uint64_t*out){{*out=0;if(!mod_step(s,d)||!value.pointer)return 0;"
                )?;
                emit_fields_encode(plan, fields, output)?;
                writeln!(
                    output,
                    "return !s->header->callbacks->new_sequence(s->header->context,{id},children,{},out)&&*out;}}",
                    fields.len()
                )?;
            }
        }
        NativeAdapterKind::Enum { tag_offset, tag_bytes, variants } => {
            if decode {
                writeln!(
                    output,
                    "static int mod_decode_{id}(ModScope*s,uint64_t h,unsigned d,ModValue*out){{uint32_t k=0,v=0;memset(out,0,sizeof(*out));if(!mod_step(s,d)||s->header->callbacks->kind(s->header->context,h,&k)||k!=8||s->header->callbacks->variant(s->header->context,h,&v))return 0;switch(v){{"
                )?;
                for (index, variant) in variants.iter().enumerate() {
                    writeln!(output, "case {index}:{{")?;
                    emit_nominal_decode(plan, id, Some(index as u32), &variant.fields, output)?;
                    output.push_str("}\n");
                }
                output.push_str("default:return 0;}}\n");
            }
            if encode {
                writeln!(
                    output,
                    "static int mod_encode_{id}(ModScope*s,ModValue value,unsigned d,uint64_t*out){{*out=0;if(!mod_step(s,d)||!value.pointer)return 0;uint{}_t tag=0;memcpy(&tag,(const uint8_t*)value.pointer+{tag_offset},sizeof(tag));switch(tag){{",
                    u32::from(*tag_bytes) * 8
                )?;
                for (index, variant) in variants.iter().enumerate() {
                    writeln!(output, "case {}ULL:{{", variant.tag)?;
                    emit_fields_encode(plan, &variant.fields, output)?;
                    writeln!(
                        output,
                        "return !s->header->callbacks->new_variant(s->header->context,{id},{index},children,{},out)&&*out;}}",
                        variant.fields.len()
                    )?;
                }
                output.push_str("default:return 0;}}\n");
            }
        }
    }
    Ok(())
}

fn emit_bootstrap(
    plan: &NativeAdapterPlan,
    bootstrap: &NativeReceiverBootstrap,
    output: &mut String,
    next: &mut usize,
    depth: usize,
) -> Result<String> {
    use std::fmt::Write;
    if depth > 128 {
        bail!("factory bootstrap depth exceeded")
    }
    let index = *next;
    *next += 1;
    let name = format!("bootstrap{index}");
    match bootstrap {
        NativeReceiverBootstrap::FactoryRequest { type_id } => {
            if type_slot(plan, *type_id)? != "pointer" {
                bail!("factory bootstrap request must be a source object");
            }
            writeln!(
                output,
                "ModValue {name}_value={{0}};if(!mod_decode_{type_id}(s,h->factory_request,0,&{name}_value))goto failed;void*{name}={name}_value.pointer;"
            )?;
        }
        NativeReceiverBootstrap::EmptyRecord { request_getter } => {
            symbol(request_getter)?;
            writeln!(
                output,
                "extern void*{request_getter}(void);void*{name}={}( {request_getter}());if(!mod_keep(s,{name}))goto failed;",
                plan.runtime.record_allocate
            )?;
        }
        NativeReceiverBootstrap::SourceCall { symbol: name_source, arguments } => {
            symbol(name_source)?;
            let args = arguments
                .iter()
                .map(|arg| emit_bootstrap(plan, arg, output, next, depth + 1))
                .collect::<Result<Vec<_>>>()?;
            let signature = if args.is_empty() { "void".to_owned() } else { vec!["void*"; args.len()].join(",") };
            writeln!(
                output,
                "extern void*{name_source}({signature});void*{name}={name_source}({});if(!mod_keep(s,{name}))goto failed;",
                args.join(",")
            )?;
        }
    }
    Ok(name)
}

struct CodecDirections {
    decode: HashSet<u32>,
    encode: HashSet<u32>,
}
fn type_closure(plan: &NativeAdapterPlan, mut pending: Vec<u32>) -> Result<HashSet<u32>> {
    let mut selected = HashSet::new();
    while let Some(id) = pending.pop() {
        if !selected.insert(id) {
            continue;
        }
        let ty = plan.types.get(id as usize).ok_or_else(|| anyhow::anyhow!("codec closure type unavailable"))?;
        match &ty.kind {
            NativeAdapterKind::Scalar(_) => {}
            NativeAdapterKind::Array { element, .. } => pending.push(*element),
            NativeAdapterKind::Record { fields } => pending.extend(fields.iter().map(|field| field.type_id)),
            NativeAdapterKind::Enum { variants, .. } => {
                pending.extend(variants.iter().flat_map(|variant| variant.fields.iter().map(|field| field.type_id)))
            }
        }
    }
    Ok(selected)
}
fn codec_directions(plan: &NativeAdapterPlan) -> Result<CodecDirections> {
    let mut decode = Vec::new();
    let mut encode = Vec::new();
    for entry in &plan.entries {
        decode.extend([entry.request_type, entry.factory_request_type]);
        encode.push(entry.result_type);
        let mut bootstraps = vec![(&entry.factory_receiver, 0usize)];
        let mut count = 0;
        while let Some((bootstrap, depth)) = bootstraps.pop() {
            count += 1;
            if depth > 32 || count > 65536 {
                bail!("native factory bootstrap exceeds bounds");
            }
            match bootstrap {
                NativeReceiverBootstrap::FactoryRequest { type_id } => decode.push(*type_id),
                NativeReceiverBootstrap::SourceCall { arguments, .. } => {
                    bootstraps.extend(arguments.iter().map(|argument| (argument, depth + 1)))
                }
                NativeReceiverBootstrap::EmptyRecord { .. } => {}
            }
        }
    }
    for callback in &plan.callbacks {
        decode.push(callback.result_type);
        encode.extend(&callback.parameters);
    }
    Ok(CodecDirections { decode: type_closure(plan, decode)?, encode: type_closure(plan, encode)? })
}
fn emit_codecs(plan: &NativeAdapterPlan, directions: &CodecDirections, output: &mut String) -> Result<()> {
    use std::fmt::Write;
    for ty in &plan.types {
        if directions.decode.contains(&ty.id) {
            writeln!(output, "static int mod_decode_{}(ModScope*,uint64_t,unsigned,ModValue*);", ty.id)?;
        }
        if directions.encode.contains(&ty.id) {
            writeln!(output, "static int mod_encode_{}(ModScope*,ModValue,unsigned,uint64_t*);", ty.id)?;
        }
        if directions.decode.contains(&ty.id)
            && let NativeAdapterKind::Array { request_getter, .. } = &ty.kind
        {
            writeln!(output, "extern void*{request_getter}(void);")?;
        }
    }
    for constructor in &plan.constructors {
        if !directions.decode.contains(&constructor.result_type) {
            continue;
        }
        let args = constructor
            .parameters
            .iter()
            .filter(|id| !is_unit(plan, **id))
            .map(|id| type_abi(plan, *id))
            .collect::<Result<Vec<_>>>()?;
        writeln!(
            output,
            "extern void*{}({});",
            constructor.symbol,
            if args.is_empty() { "void".to_owned() } else { args.join(",") }
        )?;
    }
    for ty in &plan.types {
        let decode = directions.decode.contains(&ty.id);
        let encode = directions.encode.contains(&ty.id);
        if decode || encode {
            emit_aggregate_codec(plan, ty, decode, encode, output)?;
        }
    }
    Ok(())
}
pub(crate) fn emit_native_adapters(plan: &NativeAdapterPlan) -> Result<Vec<u8>> {
    use std::fmt::Write;
    validate_plan(plan)?;
    let directions = codec_directions(plan)?;
    let mut output = String::new();
    emit_runtime_support(plan, &mut output)?;
    symbol(&plan.runtime.record_allocate)?;
    writeln!(output, "extern void*{}(void*);", plan.runtime.record_allocate)?;
    writeln!(output, "MOD_EXPORT uint32_t {}(void){{return 2;}}", plan.discriminator_symbol)?;
    emit_codecs(plan, &directions, &mut output)?;
    for callback in &plan.callbacks {
        let result_abi = type_abi(plan, callback.result_type)?;
        let result_slot = type_slot(plan, callback.result_type)?;
        let return_value = if is_unit(plan, callback.result_type) {
            "return;".to_owned()
        } else {
            format!("return result.{result_slot};")
        };
        let failed_return =
            if is_unit(plan, callback.result_type) { "return;".to_owned() } else { format!("return ({result_abi})0;") };
        let parameters = callback
            .parameters
            .iter()
            .enumerate()
            .filter(|(_, id)| !is_unit(plan, **id))
            .map(|(index, id)| Ok(format!("{} p{index}", type_abi(plan, *id)?)))
            .collect::<Result<Vec<_>>>()?;
        // Bytes are emitted as a literal numeric initializer, never interpolated as C source.
        let operation = callback.operation.bytes().map(|b| b.to_string()).collect::<Vec<_>>().join(",");
        writeln!(
            output,
            "MOD_EXPORT {result_abi} {}({}){{ModScope*s=mod_active;ModValue result={{0}};if(!s||s->failure){{{return_value}}}uint64_t args[{}]={{0}};static const uint8_t operation[]={{ {operation}{} }};",
            callback.symbol,
            if parameters.is_empty() { "void".to_owned() } else { parameters.join(",") },
            callback.parameters.len().max(1),
            if operation.is_empty() { "0" } else { "" }
        )?;
        for (index, id) in callback.parameters.iter().enumerate() {
            let slot = type_slot(plan, *id)?;
            writeln!(
                output,
                "ModValue a{index}={{0}};{}if(!mod_encode_{id}(s,a{index},0,&args[{index}]))goto failed;",
                if is_unit(plan, *id) { String::new() } else { format!("a{index}.{slot}=p{index};") }
            )?;
        }
        writeln!(
            output,
            "uint64_t handle=0;if(s->header->callbacks->service(s->header->context,operation,{},args,{},&handle)||!mod_decode_{}(s,handle,0,&result))goto failed;{return_value}failed:s->failure=1;{failed_return}}}",
            callback.operation.len(),
            callback.parameters.len(),
            callback.result_type
        )?;
    }
    for entry in &plan.entries {
        let request_abi = type_abi(plan, entry.request_type)?;
        let request_slot = type_slot(plan, entry.request_type)?;
        let factory_abi = type_abi(plan, entry.factory_request_type)?;
        let factory_slot = type_slot(plan, entry.factory_request_type)?;
        let result_abi = type_abi(plan, entry.result_type)?;
        let result_slot = type_slot(plan, entry.result_type)?;
        let factory_parameter =
            if is_unit(plan, entry.factory_request_type) { String::new() } else { format!(",{factory_abi}") };
        let request_parameter =
            if is_unit(plan, entry.request_type) { String::new() } else { format!(",{request_abi}") };
        let factory_argument = if is_unit(plan, entry.factory_request_type) {
            String::new()
        } else {
            format!(",mod_factory_input.{factory_slot}")
        };
        let request_argument =
            if is_unit(plan, entry.request_type) { String::new() } else { format!(",request.{request_slot}") };
        writeln!(
            output,
            "extern void*{}(void*{factory_parameter});extern {result_abi} {}(void*{request_parameter});",
            entry.factory_symbol, entry.method_symbol
        )?;
        writeln!(
            output,
            "MOD_EXPORT int32_t {}(const ModHeader*h,uint64_t*out){{if(!out)return 1;*out=0;if(!mod_header(h)||mod_active)return 1;ModScope* s=calloc(1,sizeof(*s));if(!s)return 1;s->header=h;s->previous=mod_active;mod_active=s;ModValue request={{0}},mod_factory_input={{0}},result={{0}};if(!mod_decode_{}(s,h->factory_request,0,&mod_factory_input)||!mod_decode_{}(s,h->request,0,&request))goto failed;",
            entry.symbol, entry.factory_request_type, entry.request_type
        )?;
        let bootstrap = emit_bootstrap(plan, &entry.factory_receiver, &mut output, &mut 0, 0)?;
        let result_assignment =
            if is_unit(plan, entry.result_type) { String::new() } else { format!("result.{result_slot}=") };
        writeln!(
            output,
            "void*receiver={}({bootstrap}{factory_argument});if(s->failure||!mod_keep(s,receiver))goto failed;{result_assignment}{}(receiver{request_argument});if(s->failure)goto failed;",
            entry.factory_symbol, entry.method_symbol
        )?;
        if result_slot == "pointer" {
            output.push_str("if(!mod_keep(s,result.pointer))goto failed;\n");
        }
        writeln!(
            output,
            "if(!mod_encode_{}(s,result,0,out)||s->failure)goto failed;mod_active=s->previous;mod_drop(s);free(s);return 0;failed:*out=0;mod_active=s->previous;mod_drop(s);free(s);return 1;}}",
            entry.result_type
        )?;
    }
    Ok(output.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> NativeAdapterPlan {
        NativeAdapterPlan {
            syntax_types: Vec::new(),
            pointer_bytes: std::mem::size_of::<usize>() as u8,
            discriminator_symbol: "mod_abi".into(),
            types: vec![
                NativeAdapterType {
                    id: 0,
                    name: "number".into(),
                    kind: NativeAdapterKind::Scalar(SemanticTypeId::I32),
                },
                NativeAdapterType {
                    id: 1,
                    name: "text".into(),
                    kind: NativeAdapterKind::Scalar(SemanticTypeId::STRING),
                },
                NativeAdapterType {
                    id: 2,
                    name: "numbers".into(),
                    kind: NativeAdapterKind::Array {
                        element: 0,
                        stride: 4,
                        data_offset: 0,
                        length_offset: 8,
                        request_getter: "array_request".into(),
                        request_bytes: 64,
                        count_offset: 16,
                    },
                },
                NativeAdapterType {
                    id: 3,
                    name: "request".into(),
                    kind: NativeAdapterKind::Record {
                        fields: vec![
                            NativeAdapterField { name: "name".into(), type_id: 1, offset: 16 },
                            NativeAdapterField { name: "numbers".into(), type_id: 2, offset: 24 },
                        ],
                    },
                },
                NativeAdapterType {
                    id: 4,
                    name: "result".into(),
                    kind: NativeAdapterKind::Enum {
                        tag_offset: 16,
                        tag_bytes: 8,
                        variants: vec![
                            NativeAdapterVariant {
                                name: "Ok".into(),
                                tag: 0,
                                fields: vec![NativeAdapterField { name: "value".into(), type_id: 0, offset: 24 }],
                            },
                            NativeAdapterVariant { name: "None".into(), tag: 1, fields: vec![] },
                        ],
                    },
                },
            ],
            constructors: vec![
                NativeConstructor {
                    symbol: "request_new".into(),
                    result_type: 3,
                    variant: None,
                    parameters: vec![1, 2],
                    bindings: vec![NativeConstructorArgument::Field(0), NativeConstructorArgument::Field(1)],
                },
                NativeConstructor {
                    symbol: "result_ok".into(),
                    result_type: 4,
                    variant: Some(0),
                    parameters: vec![0],
                    bindings: vec![NativeConstructorArgument::Field(0)],
                },
                NativeConstructor {
                    symbol: "result_none".into(),
                    result_type: 4,
                    variant: Some(1),
                    parameters: vec![],
                    bindings: vec![],
                },
            ],
            entries: vec![NativeEntryPlan {
                symbol: "invoke".into(),
                method_symbol: "generate".into(),
                family: "test".into(),
                receiver_type: 3,
                factory_symbol: "factory_create".into(),
                factory_receiver: NativeReceiverBootstrap::SourceCall {
                    symbol: "factory_bootstrap".into(),
                    arguments: vec![NativeReceiverBootstrap::EmptyRecord { request_getter: "factory_request".into() }],
                },
                factory_request_type: 3,
                request_type: 3,
                result_type: 4,
            }],
            callbacks: vec![NativeCallbackPlan {
                symbol: "query".into(),
                operation: "resolve\"name".into(),
                parameters: vec![0],
                result_type: 3,
            }],
            runtime: NativeRuntimeHooks {
                string_construct: "str_new".into(),
                string_length: "str_len".into(),
                string_data_offset: 0,
                root: "gc_root_handle".into(),
                resolve_root: "gc_resolve_handle".into(),
                unroot: "gc_unroot_handle".into(),
                array_allocate_rooted: "array_allocate_rooted".into(),
                construction_finish: "array_finish".into(),
                record_allocate: "record_allocate".into(),
                write_barrier: "gc_write_barrier".into(),
                array_write_barrier: "array_write_barrier".into(),
            },
        }
    }
    #[test]
    fn emitted_full_codec_is_valid_c11() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("adapter.c");
        let mut plan = fixture();
        plan.types.push(NativeAdapterType {
            id: 5,
            name: "unit".into(),
            kind: NativeAdapterKind::Scalar(SemanticTypeId::UNIT),
        });
        plan.callbacks.push(NativeCallbackPlan {
            symbol: "query_unit".into(),
            operation: "notify".into(),
            parameters: vec![5],
            result_type: 5,
        });
        std::fs::write(&path, emit_native_adapters(&plan).unwrap()).unwrap();
        let output = std::process::Command::new("clang")
            .args(["-std=c11", "-Wall", "-Wextra", "-fsyntax-only"])
            .arg(&path)
            .output()
            .expect("clang required to verify generated native adapter");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }
    #[test]
    fn output_only_nominal_values_do_not_require_source_constructors() {
        let mut plan = fixture();
        plan.constructors.retain(|constructor| constructor.result_type == 3);
        let source = String::from_utf8(emit_native_adapters(&plan).unwrap()).unwrap();
        assert!(source.contains("static int mod_encode_4("));
        assert!(!source.contains("static int mod_decode_4("));
        assert!(source.contains("static int mod_decode_3("));
        // Callback result request type still needs its actual source constructor.
        plan.constructors.clear();
        assert!(emit_native_adapters(&plan).is_err());
    }
    #[test]
    fn source_constructor_argument_mismatch_is_rejected() {
        let mut plan = fixture();
        plan.constructors[0].parameters[0] = 0;
        assert!(emit_native_adapters(&plan).is_err());
    }
    fn codec_fixture_source(plan: &NativeAdapterPlan, decode: &[u32], encode: &[u32]) -> String {
        validate_plan(plan).unwrap();
        let directions = CodecDirections {
            decode: type_closure(plan, decode.to_vec()).unwrap(),
            encode: type_closure(plan, encode.to_vec()).unwrap(),
        };
        let mut source = String::new();
        emit_runtime_support(plan, &mut source).unwrap();
        emit_codecs(plan, &directions, &mut source).unwrap();
        source
    }
    #[test]
    fn emitted_scalar_codec_preserves_ieee_and_checks_bool_unicode() {
        let mut plan = fixture();
        plan.types = [
            SemanticTypeId::F32,
            SemanticTypeId::F64,
            SemanticTypeId::BOOL,
            SemanticTypeId::CHAR,
            SemanticTypeId::UNIT,
        ]
        .into_iter()
        .enumerate()
        .map(|(id, ty)| NativeAdapterType {
            id: id as u32,
            name: format!("scalar{id}"),
            kind: NativeAdapterKind::Scalar(ty),
        })
        .collect();
        plan.constructors.clear();
        plan.entries.clear();
        plan.callbacks.clear();
        let mut source = codec_fixture_source(&plan, &[0, 1, 2, 3, 4], &[0, 1, 2, 3, 4]);
        source.push_str(r#"
size_t gc_root_handle(void*p){return (size_t)p;}
void gc_unroot_handle(size_t p){(void)p;}
static uint32_t current_kind;static uint64_t current_bits,encoded_bits;
static int32_t test_kind(void*c,uint64_t h,uint32_t*out){(void)c;(void)h;*out=current_kind;return 0;}
static int32_t test_scalar(void*c,uint64_t h,uint64_t*out){(void)c;(void)h;*out=current_bits;return 0;}
static int32_t test_new(void*c,uint32_t k,uint64_t b,uint64_t*out){(void)c;if(k!=current_kind)return 1;encoded_bits=b;*out=1;return 0;}
int main(void){
 ModCallbacks callbacks={0};callbacks.kind=test_kind;callbacks.scalar=test_scalar;callbacks.new_scalar=test_new;
 ModHeader h={0};h.callbacks=&callbacks;ModScope scope={0};scope.header=&h;
 ModValue value={0};uint64_t output=0;current_kind=4;
 const uint64_t f32[]={0,0x80000000ULL,0x7f800000ULL,0x7fc01234ULL,0xffc05678ULL};
 for(size_t i=0;i<5;i++){current_bits=f32[i];if(!mod_decode_0(&scope,1,0,&value)||!mod_encode_0(&scope,value,0,&output)||encoded_bits!=current_bits)return 1;}
 const uint64_t f64[]={0,0x8000000000000000ULL,0x7ff0000000000000ULL,0x7ff8000000001234ULL};
 for(size_t i=0;i<4;i++){current_bits=f64[i];if(!mod_decode_1(&scope,1,0,&value)||!mod_encode_1(&scope,value,0,&output)||encoded_bits!=current_bits)return 2;}
 current_kind=1;current_bits=2;if(mod_decode_2(&scope,1,0,&value))return 3;
 current_bits=1;if(!mod_decode_2(&scope,1,0,&value))return 4;
 current_kind=3;current_bits=0xd800;if(mod_decode_3(&scope,1,0,&value))return 5;
 current_bits=0x110000;if(mod_decode_3(&scope,1,0,&value))return 6;
 current_bits=0x10ffff;if(!mod_decode_3(&scope,1,0,&value))return 7;
 current_kind=0;value.u8=255;if(!mod_encode_4(&scope,value,0,&output)||encoded_bits)return 8;
 return 0;
}
"#);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scalar.c");
        let executable = dir.path().join("scalar");
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("clang")
            .args(["-std=c11", "-O2"])
            .arg(path)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(std::process::Command::new(executable).status().unwrap().success());
    }
    #[test]
    fn symbols_cannot_inject_c_source() {
        for name in ["", "7start", "x;abort()", "x\n#define y", "a-b"] {
            assert!(symbol(name).is_err(), "{name:?}");
        }
        assert!(symbol("__beskid_mod_0123").is_ok());
    }
    #[test]
    fn emitted_record_builder_receives_source_type_id() {
        let mut plan = fixture();
        plan.types.truncate(1);
        plan.types.push(NativeAdapterType {
            id: 1,
            name: "record".into(),
            kind: NativeAdapterKind::Record {
                fields: vec![NativeAdapterField { name: "value".into(), type_id: 0, offset: 16 }],
            },
        });
        plan.constructors = vec![NativeConstructor {
            symbol: "record_new".into(),
            result_type: 1,
            variant: None,
            parameters: vec![0],
            bindings: vec![NativeConstructorArgument::Field(0)],
        }];
        plan.entries.clear();
        plan.callbacks.clear();
        let mut source = codec_fixture_source(&plan, &[], &[1]);
        source.push_str(
            r#"
size_t gc_root_handle(void*p){return (size_t)p;}
void gc_unroot_handle(size_t p){(void)p;}
static int32_t scalar_builder(void*c,uint32_t kind,uint64_t bits,uint64_t*out){
 (void)c;if(kind!=2||bits!=42)return 1;*out=11;return 0;
}
static int32_t record_builder(void*c,uint32_t type,const uint64_t*children,size_t n,uint64_t*out){
 (void)c;if(type!=1||n!=1||children[0]!=11)return 1;*out=12;return 0;
}
int main(void){
 uint8_t record[24]={0};int32_t field=42;memcpy(record+16,&field,sizeof(field));
 ModCallbacks callbacks={0};callbacks.new_scalar=scalar_builder;callbacks.new_sequence=record_builder;
 ModHeader header={0};header.callbacks=&callbacks;ModScope scope={0};scope.header=&header;
 ModValue value={0};value.pointer=record;uint64_t output=0;
 return !mod_encode_1(&scope,value,0,&output)||output!=12;
}
"#,
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("record.c");
        let executable = dir.path().join("record");
        std::fs::write(&path, source).unwrap();
        let result = std::process::Command::new("clang")
            .args(["-std=c11", "-O2"])
            .arg(path)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        assert!(std::process::Command::new(executable).status().unwrap().success());
    }
    #[test]
    fn flat_array_length_does_not_consume_nesting_depth() {
        let mut plan = fixture();
        plan.types.truncate(3);
        plan.constructors.clear();
        plan.entries.clear();
        plan.callbacks.clear();
        let mut source = codec_fixture_source(&plan, &[], &[2]);
        source.push_str(
            r#"
size_t gc_root_handle(void*p){return (size_t)p;}
void gc_unroot_handle(size_t p){(void)p;}
static int32_t scalar_builder(void*c,uint32_t kind,uint64_t bits,uint64_t*out){
 (void)c;if(kind!=2||bits>=256)return 1;*out=bits+1;return 0;
}
static int32_t array_builder(void*c,uint32_t type,const uint64_t*children,size_t n,uint64_t*out){
 (void)c;if(type!=2||n!=256)return 1;
 for(size_t i=0;i<n;i++)if(children[i]!=i+1)return 1;
 *out=900;return 0;
}
int main(void){
 int32_t elements[256];for(size_t i=0;i<256;i++)elements[i]=(int32_t)i;
 struct {void*data;size_t count;} array={elements,256};
 ModCallbacks callbacks={0};callbacks.new_scalar=scalar_builder;callbacks.new_sequence=array_builder;
 ModHeader header={0};header.callbacks=&callbacks;ModScope scope={0};scope.header=&header;
 ModValue value={0};value.pointer=&array;uint64_t output=0;
 return !mod_encode_2(&scope,value,0,&output)||output!=900;
}
"#,
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("array.c");
        let executable = dir.path().join("array");
        std::fs::write(&path, source).unwrap();
        let result = std::process::Command::new("clang")
            .args(["-std=c11", "-O2"])
            .arg(path)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
        assert!(std::process::Command::new(executable).status().unwrap().success());
    }
    #[test]
    fn raw_pointers_have_no_transport_scalar_abi() {
        assert!(scalar_abi(SemanticTypeId::POINTER).is_err());
        assert_eq!(scalar_abi(SemanticTypeId::F32).unwrap(), "float");
        assert_eq!(scalar_abi(SemanticTypeId::STRING).unwrap(), "void *");
    }
}
