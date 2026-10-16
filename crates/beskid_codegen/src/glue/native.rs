//! Normalized C wrappers. This source is compiled and linked with canonical AOT
//! bodies; it never interprets or reparses Beskid source.
use super::artifact::*;
use std::collections::BTreeSet;

fn managed(ty: &GluePhysicalType) -> bool {
    matches!(ty.logical.as_str(), "utf8" | "bytes")
}
/// Records and enums cross as C values and as managed objects on the Beskid side.
fn value_transport(ty: &GluePhysicalType) -> bool {
    ty.record.is_some() || ty.enumeration.is_some()
}
fn rooted(ty:&GluePhysicalType)->bool {managed(ty)||ty.opaque.is_some()||value_transport(ty)}
fn body_type(ty: &GluePhysicalType) -> &str {
    if rooted(ty) || ty.callback.is_some() { "void*" } else { ty.slots.first().map(String::as_str).unwrap_or("void") }
}
/// C test that rejects (`fail`) a foreign record whose Bool or `char` field is out of domain.
fn record_domain_checks(record: &GlueRecordType, value: &str, fail: &str) -> String {
    let mut checks = String::new();
    for field in &record.fields {
        let name = format!("{value}.field_{}", field.name);
        match field.logical.as_str() {
            "bool" => checks.push_str(&format!(" if({name}>1) {{{fail}}}\n")),
            "char" => checks.push_str(&format!(
                " if({name}>UINT32_C(0x10ffff) || ({name}>=UINT32_C(0xd800) && {name}<=UINT32_C(0xdfff))) {{{fail}}}\n"
            )),
            _ => {}
        }
    }
    checks
}
/// C test that rejects (`fail`) a scalar or discriminant value outside its domain.
fn scalar_domain_check(ty: &GluePhysicalType, value: &str, fail: &str) -> String {
    if let Some(enumeration) = &ty.enumeration {
        return format!(" if({value}>=UINT32_C({})) {{{fail}}}\n", enumeration.variants.len());
    }
    match ty.logical.as_str() {
        "bool" => format!(" if({value}>1) {{{fail}}}\n"),
        "char" => format!(
            " if({value}>UINT32_C(0x10ffff) || ({value}>=UINT32_C(0xd800) && {value}<=UINT32_C(0xdfff))) {{{fail}}}\n"
        ),
        _ => String::new(),
    }
}
/// File-scope prototypes of the Cranelift record and enum transports the adapters call.
fn emit_value_transport_prototypes(manifest: &GlueManifest, c: &mut String) {
    let mut seen = BTreeSet::new();
    for binding in &manifest.bindings {
        for ty in nominal_values(binding) {
            if let Some(record) = &ty.record
                && seen.insert(record.record_sha256.clone())
            {
                c.push_str(&format!(
                    "extern void *{}(const {}*);\nextern int32_t {}(void*,{}*);\n",
                    record.constructor, record.c_name, record.reader, record.c_name
                ));
            }
            if let Some(enumeration) = &ty.enumeration
                && seen.insert(enumeration.enum_sha256.clone())
            {
                c.push_str(&format!(
                    "extern void *{}(uint32_t);\nextern int32_t {}(void*,uint32_t*);\n",
                    enumeration.constructor, enumeration.reader
                ));
            }
        }
    }
}
/// Per-thread callback frames. An import adapter links one frame per callback argument for the
/// duration of its foreign call; an invocation finds its frame by token in the calling thread's
/// list only, so a retained token or another thread finds none and gets status 2.
fn emit_callback_services(c: &mut String) {
    crate::native_tls::emit_thread_slot("glue_callback_frames", "void *", c);
    c.push_str("typedef struct GlueCallbackFrame { struct GlueCallbackFrame *previous; uint64_t token; size_t root; } GlueCallbackFrame;\nstatic _Atomic uint64_t glue_callback_tokens=0;\nstatic int glue_callback_push(GlueCallbackFrame *frame,size_t root) {\n uint64_t token=atomic_fetch_add_explicit(&glue_callback_tokens,1,memory_order_relaxed)+1; if(!token) return 0;\n frame->previous=(GlueCallbackFrame*)glue_callback_frames_get(); frame->token=token; frame->root=root;\n return glue_callback_frames_set(frame);\n}\nstatic void glue_callback_pop(GlueCallbackFrame *frame) { glue_callback_frames_set(frame->previous); frame->token=0; }\nstatic GlueCallbackFrame *glue_callback_find(uint64_t token) {\n if(!token) return NULL;\n for(GlueCallbackFrame *frame=(GlueCallbackFrame*)glue_callback_frames_get(); frame; frame=frame->previous) if(frame->token==token) return frame;\n return NULL;\n}\n");
}
/// One invoke trampoline per callback signature. It checks the token against the calling
/// thread's live frames, validates foreign arguments, converts discriminants to managed enums,
/// resolves the closure record from its root and calls its code pointer with or without the
/// environment, exactly as an indirect Beskid call does.
fn emit_callback_trampolines(manifest: &GlueManifest, c: &mut String) {
    let code_offset = crate::EVENT_HANDLER_CODE_OFFSET;
    let environment_offset = crate::EVENT_HANDLER_ENVIRONMENT_OFFSET;
    let mut seen = BTreeSet::new();
    for binding in manifest.bindings.iter().filter(|binding| binding.direction == "import") {
        for parameter in &binding.parameters {
            let Some(callback) = &parameter.callback else { continue };
            if !seen.insert(callback.callback_sha256.clone()) {
                continue;
            }
            let mut arguments = vec!["uint64_t context".to_owned()];
            for (index, member) in callback.parameters.iter().enumerate() {
                arguments.push(format!("{} arg_{index}", member.slots[0]));
            }
            let has_out = !callback.result.slots.is_empty();
            if has_out {
                arguments.push(format!("{} *out", callback.result.slots[0]));
            }
            c.push_str(&format!("int32_t {}({}) {{\n int32_t status=0;\n", callback.invoke, arguments.join(",")));
            if has_out {
                c.push_str(" if(!out) return 1; memset(out,0,sizeof(*out));\n");
            }
            c.push_str(" GlueCallbackFrame *frame=glue_callback_find(context); if(!frame) return 2;\n");
            for (index, member) in callback.parameters.iter().enumerate() {
                c.push_str(&scalar_domain_check(member, &format!("arg_{index}"), "return 1;"));
            }
            let enums = callback.parameters.iter().enumerate().filter(|(_, member)| member.enumeration.is_some()).collect::<Vec<_>>();
            for (index, _) in &enums {
                c.push_str(&format!(" void *member_{index}=NULL; size_t member_root_{index}=0;\n"));
            }
            c.push_str(" void *closure=NULL; void *code=NULL; void *environment=NULL;\n");
            for (index, member) in &enums {
                let enumeration = member.enumeration.as_ref().expect("enum member");
                c.push_str(&format!(" member_{index}={}(arg_{index}); if(!member_{index}) {{status=3;goto done;}} member_root_{index}=beskid_rt_v5_gc_try_root_handle(member_{index}); if(!member_root_{index}) {{status=3;goto done;}}\n", enumeration.constructor));
            }
            for (index, _) in &enums {
                c.push_str(&format!(" member_{index}=beskid_rt_v5_gc_resolve_handle(member_root_{index}); if(!member_{index}) {{status=2;goto done;}}\n"));
            }
            c.push_str(&format!(" closure=beskid_rt_v5_gc_resolve_handle(frame->root); if(!closure) {{status=2;goto done;}}\n code=*(void**)((uint8_t*)closure+{code_offset}); environment=*(void**)((uint8_t*)closure+{environment_offset}); if(!code) {{status=2;goto done;}}\n"));
            let body_parameters = callback.parameters.iter().map(|member| body_type(member).to_owned()).collect::<Vec<_>>();
            let values = callback
                .parameters
                .iter()
                .enumerate()
                .map(|(index, member)| if member.enumeration.is_some() { format!("member_{index}") } else { format!("arg_{index}") })
                .collect::<Vec<_>>();
            let result = body_type(&callback.result);
            let plain = if body_parameters.is_empty() { "void".to_owned() } else { body_parameters.join(",") };
            let bound = std::iter::once("void*".to_owned()).chain(body_parameters.iter().cloned()).collect::<Vec<_>>().join(",");
            let bound_values = std::iter::once("environment".to_owned()).chain(values.iter().cloned()).collect::<Vec<_>>().join(",");
            c.push_str(&format!(" {{\n typedef {result} (*plain_entry)({plain});\n typedef {result} (*bound_entry)({bound});\n"));
            if has_out {
                c.push_str(&format!(" {result} result=environment ? ((bound_entry)code)({bound_values}) : ((plain_entry)code)({});\n", values.join(",")));
                match &callback.result.enumeration {
                    Some(enumeration) => c.push_str(&format!(" status={}(result,out);\n", enumeration.reader)),
                    None => c.push_str(" *out=result;\n"),
                }
            } else {
                c.push_str(&format!(" if(environment) ((bound_entry)code)({bound_values}); else ((plain_entry)code)({});\n", values.join(",")));
            }
            c.push_str(" }\ndone:\n");
            for (index, _) in &enums {
                c.push_str(&format!(" if(member_root_{index}) gc_unroot_handle(member_root_{index});\n"));
            }
            if has_out {
                c.push_str(" if(status) memset(out,0,sizeof(*out));\n");
            }
            c.push_str(" return status;\n}\n");
        }
    }
}
/// Canonical `BeskidCheckedAllocationScope` geometry from the ABI-v5 manifest. A Glue checked
/// scope is that runtime scope; its failure reason word records the first foreign status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckedScopeGeometry {
    pub size: u64,
    pub failure_reason_offset: u64,
}
impl CheckedScopeGeometry {
    pub fn from_manifest(manifest: &beskid_abi::abi_v5::AbiManifestV5) -> Result<Self, GlueArtifactError> {
        let invalid = |message: &str| GlueArtifactError::Invalid(message.into());
        let layout = manifest
            .layouts
            .iter()
            .find(|layout| layout.name == "BeskidCheckedAllocationScope")
            .ok_or_else(|| invalid("checked scope geometry unavailable"))?;
        let reason = layout
            .fields
            .iter()
            .find(|field| field.name == "failure_reason" && field.ty == "usize")
            .ok_or_else(|| invalid("checked scope failure reason field unavailable"))?;
        if layout.size != 64 || layout.alignment != 8 || reason.offset % 8 != 0 || reason.offset + 8 > layout.size {
            return Err(invalid("checked scope geometry differs"));
        }
        Ok(Self { size: layout.size, failure_reason_offset: reason.offset })
    }
}
/// Glue checked scope services, emitted only when a binding is fallible. A fallible import adapter
/// records its first nonzero foreign status in the runtime checked scope that this thread's Glue
/// checked invocation entered; checked body counterparts observe it after the call and return, and
/// the invocation returns the recorded status. Any other scope, or no scope, keeps the trap.
fn emit_checked_scope_services(scope: &CheckedScopeGeometry, c: &mut String) {
    // The entered scope is per thread; the slot never uses CRT-implicit TLS on Windows.
    crate::native_tls::emit_thread_slot("glue_checked_scope", "void *", c);
    c.push_str(&format!(
        "extern void *beskid_rt_v5_checked_scope_current(void);\nextern uint8_t beskid_rt_v5_checked_scope_enter(void*,size_t);\nextern size_t beskid_rt_v5_checked_scope_failure_reason(void);\nextern void beskid_rt_v5_checked_scope_leave(void*);\n_Static_assert(sizeof(size_t)==8,\"Glue checked scope reason width\");\n#define GLUE_CHECKED_SCOPE_SIZE {}\n#define GLUE_CHECKED_FOREIGN_TAG UINT64_C(0x474c554500000000)\nstatic int glue_checked_absorb(int32_t status) {{\n void *scope=beskid_rt_v5_checked_scope_current();\n if(!status || !scope || scope!=glue_checked_scope_get()) return 0;\n size_t *reason=(size_t*)((uint8_t*)scope+{});\n if(!*reason) *reason=(size_t)(GLUE_CHECKED_FOREIGN_TAG|(uint64_t)(uint32_t)status);\n return 1;\n}}\nstatic int32_t glue_checked_status(size_t reason) {{\n if(((uint64_t)reason&UINT64_C(0xffffffff00000000))==GLUE_CHECKED_FOREIGN_TAG) return (int32_t)(uint32_t)reason;\n return 3;\n}}\n",
        scope.size, scope.failure_reason_offset
    ));
}
fn emit_checked_wrappers(manifest: &GlueManifest, c: &mut String) {
    c.push_str("extern void *beskid_rt_v5_gc_resolve_handle(size_t);\nextern int32_t beskid_glue_v1_owner_validate_binding(uint64_t,uint64_t);\n");
    c.push_str("extern int32_t beskid_glue_v1_owner_validate_opaque(uint64_t,const uint8_t*,uint64_t);\nextern size_t beskid_rt_v5_gc_try_root_handle(void*);\n");
    for b in manifest.bindings.iter().filter(|binding| binding.direction == "export") {
        let mut arguments = Vec::new();
        let mut body_arguments = Vec::new();
        let mut body_types = Vec::new();
        for (index, parameter) in b.parameters.iter().enumerate() {
            for (slot, ty) in parameter.slots.iter().enumerate() {
                arguments.push(format!("{ty} arg_{index}_{slot}"));
            }
            if !parameter.slots.is_empty() {
                body_types.push(body_type(parameter));
                body_arguments.push(if rooted(parameter) {
                    format!("managed_{index}")
                } else {
                    format!("arg_{index}_0")
                });
            }
        }
        let owned = managed(&b.result);
        let result_type = if owned {
            "GlueOwnedView"
        } else if b.result.opaque.is_some() {
            "uint64_t"
        } else if let Some(record) = &b.result.record {
            record.c_name.as_str()
        } else if b.result.enumeration.is_some() {
            "uint32_t"
        } else {
            body_type(&b.result)
        };
        let has_result = owned || !b.result.slots.is_empty();
        if has_result {
            arguments.push(format!("{result_type} *output"));
        }
        let callee = if b.fallible { checked_body_symbol(b) } else { b.body_symbol.clone() };
        c.push_str(&format!(
            "extern {} {}({});\nint32_t {}({}) {{\n",
            body_type(&b.result),
            callee,
            if body_types.is_empty() { "void".into() } else { body_types.join(",") },
            checked_invocation_symbol(b),
            if arguments.is_empty() { "void".into() } else { arguments.join(",") }
        ));
        if has_result {
            c.push_str(&format!(" if(!output || (uintptr_t)output % _Alignof({result_type})) return 1;\n memset(output,0,sizeof(*output));\n"));
        }
        c.push_str(&format!(" uint64_t library=0; int32_t status=bind_shapes(&library); if(status) return status;\n status=beskid_glue_v1_owner_validate_binding(library,UINT64_C({})); if(status) return status;\n",b.shape_id));
        for (index, parameter) in b.parameters.iter().enumerate() {
            c.push_str(&scalar_domain_check(parameter, &format!("arg_{index}_0"), "return 1;"));
            if let Some(record) = &parameter.record {
                c.push_str(&record_domain_checks(record, &format!("arg_{index}_0"), "return 1;"));
            }
        }
        for (index, parameter) in b.parameters.iter().enumerate().filter(|(_, parameter)| rooted(parameter)) {
            let constructed = match (&parameter.record, &parameter.enumeration) {
                (Some(record), _) => Some(format!("{}(&arg_{index}_0)", record.constructor)),
                (None, Some(enumeration)) => Some(format!("{}(arg_{index}_0)", enumeration.constructor)),
                (None, None) => None,
            };
            if let Some(constructed) = constructed {
                // Field values are copied into a new managed object; nothing aliases the input.
                c.push_str(&format!(" void *managed_{index}={constructed}; size_t root_{index}=0; if(!managed_{index}) status=3; else {{root_{index}=beskid_rt_v5_gc_try_root_handle(managed_{index}); if(!root_{index}) status=3;}} if(status){{"));
            } else if let Some(opaque)=&parameter.opaque {
                let brand=hex_c(&opaque.brand_sha256);
                c.push_str(&format!(" extern void *{}(uint64_t); static const uint8_t brand_{index}[32]={{{brand}}}; void *managed_{index}=NULL;size_t root_{index}=0;\n",opaque.constructor));
                if opaque.nullable {c.push_str(&format!(" if(arg_{index}_0)status=beskid_glue_v1_owner_validate_opaque(library,brand_{index},arg_{index}_0);\n"));}
                else {c.push_str(&format!(" status=beskid_glue_v1_owner_validate_opaque(library,brand_{index},arg_{index}_0);\n"));}
                c.push_str(&format!(" if(!status){{managed_{index}={}(arg_{index}_0);if(!managed_{index})status=3;else{{root_{index}=beskid_rt_v5_gc_try_root_handle(managed_{index});if(!root_{index})status=3;}}}} if(status){{",opaque.constructor));
            }else {
                c.push_str(&format!(" void *managed_{index}=NULL; size_t root_{index}=0;\n status=beskid_glue_v1_input_{}(arg_{index}_0,arg_{index}_1,&managed_{index},&root_{index}); if(status) {{",parameter.logical));
            }
            for (previous, parameter) in
                b.parameters[..index].iter().enumerate().filter(|(_, parameter)| rooted(parameter))
            {
                let _ = parameter;
                c.push_str(&format!("gc_unroot_handle(root_{previous});"));
            }
            c.push_str("return status;}\n");
        }
        if b.fallible {
            // The reserved failure cell is admitted before the managed arguments are resolved, so
            // its allocation cannot move them; the scope publishes no managed failure value.
            c.push_str(" size_t checked_root=0; uint8_t checked_entered=0; void *checked_previous=glue_checked_scope_get(); _Alignas(8) uint8_t checked_scope[GLUE_CHECKED_SCOPE_SIZE]; memset(checked_scope,0,sizeof checked_scope);\n");
            c.push_str(" { static const uint8_t checked_cell_bytes[1]={0}; void *checked_cell=NULL; if(beskid_glue_v1_input_bytes(checked_cell_bytes,0,&checked_cell,&checked_root)) status=3; else if(!beskid_rt_v5_checked_scope_enter(checked_scope,checked_root)) status=3; else if(!glue_checked_scope_set(checked_scope)) {beskid_rt_v5_checked_scope_leave(checked_scope); status=3;} else checked_entered=1; }\n");
        }
        for (index, _) in b.parameters.iter().enumerate().filter(|(_, parameter)| rooted(parameter)) {
            c.push_str(&format!(" managed_{index}=beskid_rt_v5_gc_resolve_handle(root_{index}); if(!managed_{index}) status=2;\n"));
        }
        c.push_str(" if(!status) {\n");
        let invocation = format!("{}({})", callee, body_arguments.join(","));
        let (call, publish) = if owned {
            (
                format!(" void *result={invocation};\n"),
                format!(
                    " status=beskid_glue_v1_result_{}(library,UINT64_C({}),result,output);\n",
                    b.result.logical, b.shape_id
                ),
            )
        } else if let Some(record) = &b.result.record {
            (format!(" void *result={invocation};\n"), format!(" status={}(result,output);\n", record.reader))
        } else if let Some(enumeration) = &b.result.enumeration {
            (
                format!(" void *result={invocation}; uint32_t discriminant=0;\n"),
                format!(" status={}(result,&discriminant); if(!status) *output=discriminant;\n", enumeration.reader),
            )
        } else if let Some(opaque)=&b.result.opaque {
            let brand=hex_c(&opaque.brand_sha256);
            let mut publish = format!(" status={}(result,&token);\n", opaque.reader);
            if opaque.nullable {publish.push_str(" if(!status&&token)status=beskid_glue_v1_owner_validate_opaque(library,result_brand,token);\n");}
            else {publish.push_str(" if(!status)status=beskid_glue_v1_owner_validate_opaque(library,result_brand,token);\n");}
            publish.push_str(" if(!status)*output=token;\n");
            (format!(" extern int32_t {}(void*,uint64_t*);static const uint8_t result_brand[32]={{{brand}}};void *result={invocation};uint64_t token=0;\n",opaque.reader), publish)
        } else if has_result {
            let publish = match b.result.logical.as_str() {
                "bool" => " if(result>1) status=1; else *output=result;\n",
                "char" => " if(result>UINT32_C(0x10ffff) || (result>=UINT32_C(0xd800) && result<=UINT32_C(0xdfff))) status=1; else *output=result;\n",
                _ => " *output=result;\n",
            };
            (format!(" {result_type} result={invocation};\n"), publish.to_owned())
        } else {
            (format!(" {invocation};\n"), String::new())
        };
        c.push_str(&call);
        if b.fallible {
            // Leaving restores the prior scope before any result is published; the managed result
            // stays valid because neither leaving nor unrooting the failure cell can collect.
            c.push_str(" size_t checked_reason=beskid_rt_v5_checked_scope_failure_reason(); beskid_rt_v5_checked_scope_leave(checked_scope); glue_checked_scope_set(checked_previous); checked_entered=0; gc_unroot_handle(checked_root); checked_root=0;\n if(checked_reason) status=glue_checked_status(checked_reason);\n if(!status) {\n");
            c.push_str(&publish);
            c.push_str(" }\n");
        } else {
            c.push_str(&publish);
        }
        c.push_str(" }\n");
        if b.fallible {
            c.push_str(" if(checked_entered) {beskid_rt_v5_checked_scope_leave(checked_scope); glue_checked_scope_set(checked_previous);}\n if(checked_root) gc_unroot_handle(checked_root);\n");
        }
        for (index, _) in b.parameters.iter().enumerate().filter(|(_, parameter)| rooted(parameter)) {
            c.push_str(&format!(" gc_unroot_handle(root_{index});\n"));
        }
        c.push_str(" return status;\n}\n");
    }
}
fn import_physical_signature(binding:&GlueBindingManifest)->Vec<String>{
    let mut types=binding.parameters.iter().flat_map(|ty|ty.slots.iter().cloned()).collect::<Vec<_>>();
    if !binding.result.slots.is_empty(){
        types.push(if managed(&binding.result){"GlueOwnedView*".into()}else{format!("{}*",binding.result.slots[0])});
    }
    types
}
fn emit_import_table(manifest:&GlueManifest,c:&mut String){
    let imports=manifest.bindings.iter().filter(|binding|binding.direction=="import").collect::<Vec<_>>();
    c.push_str("typedef struct {uint8_t identity[32];uint64_t address;uint64_t library;uint64_t generation;} GlueImportRowV1;\ntypedef struct {uint32_t version;uint32_t size;size_t count;const GlueImportRowV1 *rows;} GlueImportTableV1;\n_Static_assert(sizeof(GlueImportRowV1)==56,\"import row ABI\");\n_Static_assert(sizeof(GlueImportTableV1)==24,\"import table ABI\");\nstatic uint8_t imports_admitted=0;\n");
    for binding in &imports {
        let signature=import_physical_signature(binding);
        let args=if signature.is_empty(){"void".into()}else{signature.join(",")};
        c.push_str(&format!("typedef int32_t(*GlueImport{} )({args});static GlueImport{} import_{}=NULL;static uint64_t import_library_{}=0;\n",binding.identity_sha256,binding.identity_sha256,binding.identity_sha256,binding.identity_sha256));
    }
    c.push_str(&format!("int32_t beskid_glue_artifact_v1_bind_imports(const GlueImportTableV1 *table){{if(imports_admitted||!table||table->version!=1||table->size!=sizeof(*table)||table->count!={}||(table->count&&!table->rows))return 1;\n",imports.len()));
    for (index,binding) in imports.iter().enumerate(){
        let identity=hex_c(&binding.identity_sha256);
        c.push_str(&format!("static const uint8_t expected_{index}[32]={{{identity}}};if(memcmp(table->rows[{index}].identity,expected_{index},32)||!table->rows[{index}].address||!table->rows[{index}].library||!table->rows[{index}].generation)return 1;\n"));
    }
    for (index,binding) in imports.iter().enumerate(){
        c.push_str(&format!("memcpy(&import_{},&table->rows[{index}].address,sizeof(import_{}));import_library_{}=table->rows[{index}].library;\n",binding.identity_sha256,binding.identity_sha256,binding.identity_sha256));
    }
    c.push_str("imports_admitted=1;return 0;}\n");
}
fn emit_import_wrappers(manifest: &GlueManifest, c: &mut String) {
    emit_import_table(manifest,c);
    c.push_str("extern int32_t beskid_glue_v1_owner_release_token(uint64_t,uint64_t);\nextern size_t gc_root_handle(void*);\nextern _Noreturn void beskid_rt_v5_trap(uint8_t,void*,size_t);\nstatic _Noreturn void glue_import_failure(void) { static const char message[]=\"Rust Glue foreign invocation failed\"; beskid_rt_v5_trap(8,(void*)message,sizeof(message)-1); }\n");
    for binding in manifest.bindings.iter().filter(|binding| binding.direction == "import") {
        let mut source_args = Vec::new();
        let mut foreign_args = Vec::new();
        let mut foreign_types = Vec::new();
        let mut source_values = Vec::new();
        for (index, parameter) in binding.parameters.iter().enumerate() {
            if parameter.slots.is_empty() {
                continue;
            }
            source_args.push(format!("{} value_{index}", body_type(parameter)));
            source_values.push(format!("value_{index}"));
            foreign_types.extend(parameter.slots.iter().cloned());
            if managed(parameter) {
                foreign_args.push(format!("input_{index}.pointer"));
                foreign_args.push(format!("input_{index}.length"));
            } else if let Some(callback) = &parameter.callback {
                foreign_args.push(callback.invoke.clone());
                foreign_args.push(format!("callback_frame_{index}.token"));
            } else if parameter.record.is_some() {
                foreign_args.push(format!("record_{index}"));
            } else if parameter.enumeration.is_some() {
                foreign_args.push(format!("discriminant_{index}"));
            } else {
                foreign_args.push(if parameter.opaque.is_some(){format!("opaque_token_{index}")}else{format!("value_{index}")});
            }
        }
        let owned = managed(&binding.result);
        let value_result = value_transport(&binding.result);
        let result = body_type(&binding.result);
        let has_foreign_result=!binding.result.slots.is_empty();
        if has_foreign_result {
            foreign_types.push(if owned{"GlueOwnedView*".into()}else{format!("{}*",binding.result.slots[0])});
            foreign_args.push(if owned{"&foreign_output".into()}else if binding.result.opaque.is_some(){"&foreign_token".into()}else{"&foreign_result".into()});
        }
        let checked = format!("{}_checked", binding.body_symbol);
        let has_result = owned || !binding.result.slots.is_empty();
        let mut checked_args = source_args.clone();
        if has_result {
            checked_args.push(format!("{result} *output"));
        }
        c.push_str(&format!(
            "static int32_t {checked}({}) {{\n",
            if checked_args.is_empty() { "void".into() } else { checked_args.join(",") }
        ));
        if has_result {
            c.push_str(" if(!output) return 1; memset(output,0,sizeof(*output));\n");
        }
        c.push_str(&format!(" uint64_t library=0; int32_t status=bind_shapes(&library); if(status) return status; status=beskid_glue_v1_owner_validate_binding(library,UINT64_C({})); if(status) return status;\n",binding.shape_id));
        for (index, parameter) in binding.parameters.iter().enumerate().filter(|(_, parameter)| managed(parameter)) {
            c.push_str(&format!(" size_t root_{index}=0; GlueOwnedView input_{index}={{0}};\n"));
        }
        c.push_str(&format!(" if(!imports_admitted || !import_{}) return 2; uint64_t foreign_library=import_library_{};status=beskid_glue_v1_owner_validate_binding(foreign_library,UINT64_C({}));if(status)return status;\n",binding.identity_sha256,binding.identity_sha256,binding.shape_id));
        if has_foreign_result && !owned && binding.result.opaque.is_none(){c.push_str(&format!("{} foreign_result={{0}};\n",binding.result.slots[0]));}
        if owned {
            c.push_str(" GlueOwnedView foreign_output={0}; void *managed_output=NULL; size_t output_root=0;\n");
        } else if binding.result.opaque.is_some(){c.push_str("uint64_t foreign_token=0;void *managed_output=NULL;size_t output_root=0;\n");}
        for (index,parameter) in binding.parameters.iter().enumerate().filter(|(_,p)|p.opaque.is_some()){
            let opaque=parameter.opaque.as_ref().unwrap();let brand=hex_c(&opaque.brand_sha256);
            c.push_str(&format!("extern int32_t {}(void*,uint64_t*);static const uint8_t opaque_brand_{index}[32]={{{brand}}};uint64_t opaque_token_{index}=0;size_t opaque_root_{index}=0;\n",opaque.reader));
        }
        if value_result {
            c.push_str(" void *managed_output=NULL; size_t output_root=0;\n");
        }
        // Records and enums are copied out of their managed objects before anything can allocate;
        // callbacks are rooted and linked into this thread's frames for the foreign call only.
        for (index, parameter) in binding.parameters.iter().enumerate() {
            if let Some(record) = &parameter.record {
                c.push_str(&format!(" {} record_{index}; memset(&record_{index},0,sizeof(record_{index}));\n", record.c_name));
            } else if parameter.enumeration.is_some() {
                c.push_str(&format!(" uint32_t discriminant_{index}=0;\n"));
            } else if parameter.callback.is_some() {
                c.push_str(&format!(" GlueCallbackFrame callback_frame_{index}; memset(&callback_frame_{index},0,sizeof(callback_frame_{index})); size_t callback_root_{index}=0; uint8_t callback_linked_{index}=0;\n"));
            }
        }
        for (index, parameter) in binding.parameters.iter().enumerate() {
            if let Some(record) = &parameter.record {
                c.push_str(&format!(" status={}(value_{index},&record_{index}); if(status) goto cleanup;\n", record.reader));
            } else if let Some(enumeration) = &parameter.enumeration {
                c.push_str(&format!(" status={}(value_{index},&discriminant_{index}); if(status) goto cleanup;\n", enumeration.reader));
            }
        }
        for (index, parameter) in binding.parameters.iter().enumerate().filter(|(_, parameter)| parameter.callback.is_some()) {
            let _ = parameter;
            c.push_str(&format!(" callback_root_{index}=beskid_rt_v5_gc_try_root_handle(value_{index}); if(!callback_root_{index}) {{status=3;goto cleanup;}} if(!glue_callback_push(&callback_frame_{index},callback_root_{index})) {{status=3;goto cleanup;}} callback_linked_{index}=1;\n"));
        }
        for (index, parameter) in binding.parameters.iter().enumerate() {
            match parameter.logical.as_str() {
                "bool"=>c.push_str(&format!(" if(value_{index}>1) {{status=1;goto cleanup;}}\n")),
                "char"=>c.push_str(&format!(" if(value_{index}>UINT32_C(0x10ffff) || (value_{index}>=UINT32_C(0xd800) && value_{index}<=UINT32_C(0xdfff))) {{status=1;goto cleanup;}}\n")),
                _=>{},
            }
            if managed(parameter) {
                c.push_str(&format!(
                    " root_{index}=gc_root_handle(value_{index}); if(!root_{index}) {{status=5;goto cleanup;}}\n"
                ));
            }
        }
        for (index,parameter) in binding.parameters.iter().enumerate().filter(|(_,p)|p.opaque.is_some()){
            let opaque=parameter.opaque.as_ref().unwrap();
            c.push_str(&format!("opaque_root_{index}=beskid_rt_v5_gc_try_root_handle(value_{index});if(!opaque_root_{index}){{status=3;goto cleanup;}}value_{index}=beskid_rt_v5_gc_resolve_handle(opaque_root_{index});status={}(value_{index},&opaque_token_{index});if(status)goto cleanup;\n",opaque.reader));
            if opaque.nullable{c.push_str(&format!("if(opaque_token_{index})status=beskid_glue_v1_owner_validate_opaque(library,opaque_brand_{index},opaque_token_{index});if(status)goto cleanup;\n"));}
            else{c.push_str(&format!("status=beskid_glue_v1_owner_validate_opaque(library,opaque_brand_{index},opaque_token_{index});if(status)goto cleanup;\n"));}
        }
        for (index, parameter) in binding.parameters.iter().enumerate().filter(|(_, parameter)| managed(parameter)) {
            c.push_str(&format!(" value_{index}=beskid_rt_v5_gc_resolve_handle(root_{index}); if(!value_{index}) {{status=2;goto cleanup;}} status=beskid_glue_v1_result_{}(library,UINT64_C({}),value_{index},&input_{index}); if(status) goto cleanup;\n",parameter.logical,binding.shape_id));
        }
        let call = format!("import_{}({})", binding.identity_sha256, foreign_args.join(","));
        if owned {
            c.push_str(&format!(" status={call}; if(!status && !foreign_output.token) status=1; if(!status) status=beskid_glue_v1_input_{}(foreign_output.pointer,foreign_output.length,&managed_output,&output_root);\n",binding.result.logical));
        }else if let Some(record) = &binding.result.record {
            c.push_str(&format!(" status={call};if(status)goto cleanup;\n"));
            c.push_str(&record_domain_checks(record, "foreign_result", "status=1;goto cleanup;"));
            c.push_str(&format!(" managed_output={}(&foreign_result); if(!managed_output) {{status=3;goto cleanup;}} output_root=beskid_rt_v5_gc_try_root_handle(managed_output); if(!output_root) {{status=3;goto cleanup;}}\n", record.constructor));
        }else if let Some(enumeration) = &binding.result.enumeration {
            c.push_str(&format!(" status={call};if(status)goto cleanup;\n"));
            c.push_str(&scalar_domain_check(&binding.result, "foreign_result", "status=1;goto cleanup;"));
            c.push_str(&format!(" managed_output={}(foreign_result); if(!managed_output) {{status=3;goto cleanup;}} output_root=beskid_rt_v5_gc_try_root_handle(managed_output); if(!output_root) {{status=3;goto cleanup;}}\n", enumeration.constructor));
        }else if let Some(opaque)=&binding.result.opaque{
            let brand=hex_c(&opaque.brand_sha256);
            c.push_str(&format!("extern void *{}(uint64_t);static const uint8_t result_brand[32]={{{brand}}};status={call};if(status)goto cleanup;\n",opaque.constructor));
            if opaque.nullable{c.push_str("if(foreign_token)status=beskid_glue_v1_owner_validate_opaque(library,result_brand,foreign_token);if(status)goto cleanup;\n");}
            else{c.push_str("status=beskid_glue_v1_owner_validate_opaque(library,result_brand,foreign_token);if(status)goto cleanup;\n");}
            c.push_str(&format!("managed_output={}(foreign_token);if(!managed_output){{status=3;goto cleanup;}}output_root=beskid_rt_v5_gc_try_root_handle(managed_output);if(!output_root){{status=3;goto cleanup;}}\n",opaque.constructor));
        } else if has_result {
            c.push_str(&format!(" status={call};if(status)goto cleanup; {result} result=foreign_result;\n"));
            match binding.result.logical.as_str() {
                "bool"=>c.push_str(" if(result>1) status=1;\n"),
                "char"=>c.push_str(" if(result>UINT32_C(0x10ffff) || (result>=UINT32_C(0xd800) && result<=UINT32_C(0xdfff))) status=1;\n"),
                _=>{},
            }
            c.push_str(" if(!status) *output=result;\n");
        } else {
            c.push_str(&format!(" status={call};\n"));
        }
        c.push_str("cleanup:\n");
        for (index, _) in binding.parameters.iter().enumerate().rev().filter(|(_, parameter)| parameter.callback.is_some()) {
            c.push_str(&format!(" if(callback_linked_{index}) glue_callback_pop(&callback_frame_{index}); if(callback_root_{index}) gc_unroot_handle(callback_root_{index});\n"));
        }
        if owned {
            c.push_str(" if(foreign_output.token) {int32_t released=beskid_glue_v1_owner_release_token(foreign_library,foreign_output.token);if(released && !status)status=released;}\n");
        }
        for (index, _) in binding.parameters.iter().enumerate().filter(|(_, parameter)| managed(parameter)) {
            c.push_str(&format!(" if(input_{index}.token) {{int32_t released=beskid_glue_v1_owner_release_token(library,input_{index}.token);if(released && !status) status=released;}} if(root_{index}) gc_unroot_handle(root_{index});\n"));
        }
        for (index,_) in binding.parameters.iter().enumerate().filter(|(_,p)|p.opaque.is_some()){
            c.push_str(&format!("if(opaque_root_{index})gc_unroot_handle(opaque_root_{index});\n"));
        }
        if owned || binding.result.opaque.is_some() || value_result {
            c.push_str(" if(!status) {managed_output=beskid_rt_v5_gc_resolve_handle(output_root);if(!managed_output) status=2;else *output=managed_output;} if(output_root) gc_unroot_handle(output_root);\n");
        }
        if has_result {
            c.push_str(" if(status) memset(output,0,sizeof(*output));\n");
        }
        c.push_str(" return status;\n}\n");
        c.push_str(&format!(
            "{} {}({}) {{\n",
            result,
            binding.body_symbol,
            if source_args.is_empty() { "void".into() } else { source_args.join(",") }
        ));
        if has_result {
            c.push_str(&format!(" {result} output={{0}};\n"));
            source_values.push("&output".into());
        }
        // A fallible import inside its thread's Glue checked scope records the status and returns
        // the zeroed output; the checked body counterpart returns before the output is used. Every
        // other failure is terminal.
        let absorb = if binding.fallible {
            if has_result { "if(glue_checked_absorb(status)) return output; " } else { "if(glue_checked_absorb(status)) return; " }
        } else {
            ""
        };
        c.push_str(&format!(
            " int32_t status={checked}({}); if(status) {{ {absorb}static const char message[]=\"Rust Glue foreign invocation failed: {}\"; beskid_rt_v5_trap(8,(void*)message,sizeof(message)-1); }}\n",
            source_values.join(","),
            binding.symbol
        ));
        if has_result {
            c.push_str(" return output;\n");
        }
        c.push_str("}\n");
    }
}
pub(super) fn emit(
    manifest: &GlueManifest,
    scope: &CheckedScopeGeometry,
) -> Result<(Vec<u8>, Vec<u8>), GlueArtifactError> {
    let shapes = serde_json::to_vec(&manifest.bindings)?;
    let mut c = String::from(
        "/* Canonical fact-derived normalized wrappers. */\n#include <stdint.h>\n#include <stddef.h>\n#include <stdatomic.h>\n#include <string.h>\n#include \"../include/beskid_glue.h\"\nextern int32_t beskid_glue_v1_input_utf8(const uint8_t*,size_t,void**,size_t*);\nextern int32_t beskid_glue_v1_input_bytes(const uint8_t*,size_t,void**,size_t*);\nextern int32_t beskid_glue_v1_result_utf8(uint64_t,uint64_t,void*,GlueOwnedView*);\nextern int32_t beskid_glue_v1_result_bytes(uint64_t,uint64_t,void*,GlueOwnedView*);\nextern int32_t beskid_glue_v1_owner_copy(uint64_t,uint64_t,uint64_t,uint64_t,const uint8_t*,size_t,GlueOwnedView*);\nextern void gc_unroot_handle(size_t);\n",
    );
    let source_digest: Vec<u8> = (0..32)
        .map(|index| {
            manifest
                .compiled_source_sha256
                .get(index * 2..index * 2 + 2)
                .and_then(|value| u8::from_str_radix(value, 16).ok())
                .ok_or_else(|| GlueArtifactError::Invalid("invalid compiled source digest".into()))
        })
        .collect::<Result<_, _>>()?;
    // The admitted closure is every binding shape (one distinct nonzero shape per binding) plus
    // the shape of every opaque brand those bindings carry; the loader issues the same closure.
    let mut ids: BTreeSet<_> = manifest.bindings.iter().map(|binding| binding.shape_id).collect();
    if ids.is_empty() || ids.len() != manifest.bindings.len() || ids.contains(&0) {
        return Err(GlueArtifactError::Invalid("invalid compiled shape closure".into()));
    }
    for binding in &manifest.bindings {
        for physical in binding.parameters.iter().chain(std::iter::once(&binding.result)) {
            if let Some(opaque) = &physical.opaque {
                ids.insert(super::artifact::brand_shape(&opaque.brand_sha256)?);
            }
        }
    }
    if ids.len() > 4096 || ids.contains(&0) {
        return Err(GlueArtifactError::Invalid("compiled shape closure exceeds 4096 shapes".into()));
    }
    c.push_str(&format!(
        "static const uint8_t compiled_source[32]={{{}}};\nstatic const uint64_t compiled_ids[]={{{}}};\n",
        source_digest.iter().map(u8::to_string).collect::<Vec<_>>().join(","),
        ids.iter().map(|id| format!("UINT64_C({id})")).collect::<Vec<_>>().join(",")
    ));
    c.push_str("static _Atomic uint32_t admission_state=0;\nstatic _Atomic uint64_t admitted_library=0;\n_Static_assert(sizeof(GlueAdmissionTableV1)==24,\"Glue admission V1 native64 table\");\nint32_t beskid_glue_artifact_v1_initialize(const GlueAdmissionTableV1 *table) {\n if(!table || (uintptr_t)table % _Alignof(GlueAdmissionTableV1) || table->version!=1 || table->size!=sizeof(*table) || !table->context || !table->admit) return 1;\n uint32_t expected=0; if(!atomic_compare_exchange_strong_explicit(&admission_state,&expected,1,memory_order_acq_rel,memory_order_acquire)) return 4;\n uint64_t library=0; int32_t status=table->admit(table->context,compiled_source,compiled_ids,sizeof(compiled_ids)/sizeof(compiled_ids[0]),&library);\n if(status || !library) { atomic_store_explicit(&admitted_library,0,memory_order_relaxed); atomic_store_explicit(&admission_state,0,memory_order_release); return status ? status : 1; }\n atomic_store_explicit(&admitted_library,library,memory_order_relaxed); atomic_store_explicit(&admission_state,2,memory_order_release); return 0;\n}\nstatic int32_t bind_shapes(uint64_t *library) {\n *library=0; if(atomic_load_explicit(&admission_state,memory_order_acquire)!=2) return 2;\n *library=atomic_load_explicit(&admitted_library,memory_order_relaxed); return *library ? 0 : 2;\n}\n");
    for b in manifest.bindings.iter().filter(|b| b.direction == "export") {
        if requires_checked_primary(b) {
            continue;
        }
        let mut args = Vec::new();
        let mut body_args = Vec::new();
        let mut body_decl = Vec::new();
        for (i, p) in b.parameters.iter().enumerate() {
            for (slot, t) in p.slots.iter().enumerate() {
                args.push(format!("{t} arg_{i}_{slot}"));
            }
            if !p.slots.is_empty() {
                body_decl.push(body_type(p));
                body_args.push(if managed(p) { format!("managed_{i}") } else { format!("arg_{i}_0") });
            }
        }
        let returns_owned = managed(&b.result);
        if returns_owned {
            args.push("GlueOwnedView *output".into());
        }
        if returns_owned {
            let mut forwarded = b
                .parameters
                .iter()
                .enumerate()
                .flat_map(|(index, parameter)| {
                    (0..parameter.slots.len()).map(move |slot| format!("arg_{index}_{slot}"))
                })
                .collect::<Vec<_>>();
            forwarded.push("output".into());
            c.push_str(&format!(
                "int32_t {}({}) {{ return {}({}); }}\n",
                b.symbol,
                args.join(","),
                checked_invocation_symbol(b),
                forwarded.join(",")
            ));
            continue;
        }
        c.push_str(&format!(
            "extern {} {}({});\n{} {}({}) {{\n",
            body_type(&b.result),
            b.body_symbol,
            if body_decl.is_empty() { "void".into() } else { body_decl.join(",") },
            if returns_owned { "int32_t" } else { body_type(&b.result) },
            b.symbol,
            if args.is_empty() { "void".into() } else { args.join(",") }
        ));
        if returns_owned {
            c.push_str(" if (!output || ((uintptr_t)output % _Alignof(GlueOwnedView))) return 1;\n memset(output,0,sizeof(*output));\n uint64_t library=0; int32_t status=bind_shapes(&library); if(status) return status;\n");
        }
        // Checked managed parameters require a checked-result protocol so no
        // error can be disguised as a successful scalar result.
        if !returns_owned && b.parameters.iter().any(managed) {
            return Err(GlueArtifactError::Invalid(
                "managed input with scalar result requires checked scalar wrapper".into(),
            ));
        }
        for (i, p) in b.parameters.iter().enumerate().filter(|(_, p)| managed(p)) {
            c.push_str(&format!(" void *managed_{i}=NULL; size_t root_{i}=0;\n"));
            c.push_str(&format!(
                " status=beskid_glue_v1_input_{}(arg_{i}_0,arg_{i}_1,&managed_{i},&root_{i});\n if(status) {{",
                p.logical
            ));
            for (j, previous) in b.parameters[..i].iter().enumerate() {
                if managed(previous) {
                    c.push_str(&format!("gc_unroot_handle(root_{j});"));
                }
            }
            c.push_str("return status;}\n");
        }
        let invocation = format!("{}({})", b.body_symbol, body_args.join(","));
        if returns_owned {
            c.push_str(&format!(
                " void *result={invocation};\n status=beskid_glue_v1_result_{}(library,UINT64_C({}),result,output);\n",
                b.result.logical, b.shape_id
            ));
        } else if b.result.slots.is_empty() {
            c.push_str(&format!(" {invocation};\n"));
        } else {
            c.push_str(&format!(" return {invocation};\n"));
        }
        for (i, p) in b.parameters.iter().enumerate() {
            if managed(p) {
                c.push_str(&format!(" gc_unroot_handle(root_{i});\n"));
            }
        }
        if returns_owned {
            c.push_str(" return status;\n");
        }
        c.push_str("}\n");
    }
    if manifest.bindings.iter().any(|binding| binding.fallible) {
        emit_checked_scope_services(scope, &mut c);
    }
    emit_value_transport_prototypes(manifest, &mut c);
    emit_checked_wrappers(manifest, &mut c);
    if manifest.bindings.iter().any(|binding| binding.parameters.iter().any(|parameter| parameter.callback.is_some())) {
        emit_callback_services(&mut c);
        emit_callback_trampolines(manifest, &mut c);
    }
    emit_import_wrappers(manifest, &mut c);
    if !owned_release_symbols(manifest).is_empty() {
        c.push_str("extern int32_t beskid_glue_v1_owner_release_token(uint64_t,uint64_t);\n");
        let managed_releases=manifest.bindings.iter().filter(|binding|binding.direction=="export"&&managed(&binding.result)).map(|binding|owned_release_symbol(&binding.library)).collect::<BTreeSet<_>>();
        for symbol in managed_releases {
            c.push_str(&format!("int32_t {symbol}(uint64_t token) {{ uint64_t library=0; int32_t status=bind_shapes(&library); return status ? status : beskid_glue_v1_owner_release_token(library,token); }}\n"));
        }
    }
    let mut release_brands=BTreeSet::new();
    for binding in &manifest.bindings {
        for physical in binding.parameters.iter().chain(std::iter::once(&binding.result)) {
            let Some(opaque)=&physical.opaque else{continue};
            if !release_brands.insert(opaque.brand_sha256.clone()){continue;}
            let symbol=format!("beskid_glue_opaque_{}_release",opaque.brand_sha256);
            let brand=hex_c(&opaque.brand_sha256);
            c.push_str(&format!("extern int32_t beskid_glue_v1_owner_release_consumer_opaque(uint64_t,const uint8_t*,uint64_t);\nint32_t {symbol}(uint64_t token){{static const uint8_t brand[32]={{{brand}}};uint64_t library=0;int32_t status=bind_shapes(&library);return status?status:beskid_glue_v1_owner_release_consumer_opaque(library,brand,token);}}\n"));
        }
    }
    Ok((c.into_bytes(), shapes))
}

fn hex_c(hex:&str)->String {(0..32).map(|i|format!("0x{}",&hex[i*2..i*2+2])).collect::<Vec<_>>().join(",")}

#[cfg(test)]
mod tests {
    use super::*;

    const SCOPE: CheckedScopeGeometry = CheckedScopeGeometry { size: 64, failure_reason_offset: 24 };

    fn physical(logical: &str, slots: &[&str], opaque: Option<GlueOpaqueType>) -> GluePhysicalType {
        GluePhysicalType { opaque, ..GluePhysicalType::plain(logical, slots.iter().map(|slot| (*slot).to_owned()).collect(), false) }
    }

    fn manifest(bindings: Vec<GlueBindingManifest>) -> GlueManifest {
        GlueManifest {
            schema_version: 1,
            backend: "glue-rust".into(),
            target: "x86_64-unknown-linux-gnu".into(),
            runtime_abi: 5,
            runtime_linkage: "glue-shared-provider-v1".into(),
            owner_issuer_version: 1,
            native_width_bits: 64,
            compiled_source_sha256: "ab".repeat(32),
            bindings,
            required_tools: Vec::new(),
            required_native_adapters: Vec::new(),
            expected_outputs: Vec::new(),
        }
    }

    fn binding(shape: &str, parameters: Vec<GluePhysicalType>) -> GlueBindingManifest {
        GlueBindingManifest {
            identity_sha256: shape.into(),
            direction: "export".into(),
            library: "owner".into(),
            symbol: format!("Exported{}", &shape[..4]),
            body_symbol: format!("body_{}", &shape[..4]),
            shape_id: super::super::artifact::shape_id(shape).expect("shape"),
            parameters,
            result: physical("unit", &[], None),
            fallible: false,
            source: GlueSourceAuthority {
                logical_unit: "Main".into(),
                typed_unit_sha256: "cd".repeat(32),
                span_start: 0,
                span_end: 1,
            },
        }
    }

    /// The admitted closure of a binding that carries an opaque brand is its own shape plus the
    /// brand shape, exactly the closure the loader issues; it is not one shape per binding.
    #[test]
    fn opaque_brand_shapes_extend_the_compiled_closure() {
        // Not byte-symmetric: the big-endian hex prefix and the little-endian owner fold differ.
        let brand = "0102030405060708".to_owned() + &"ab".repeat(24);
        let opaque = GlueOpaqueType {
            brand_sha256: brand.clone(),
            library: "owner".into(),
            nullable: false,
            constructor: "make_handle".into(),
            reader: "read_handle".into(),
            descriptor: "handle_descriptor".into(),
        };
        let manifest = manifest(vec![binding(&"34".repeat(32), vec![physical("Handle", &["uint64_t"], Some(opaque))])]);
        let (source, _) = emit(&manifest, &SCOPE).expect("opaque binding closure is valid");
        let source = String::from_utf8(source).expect("C source");
        let brand_id = super::super::artifact::brand_shape(&brand).expect("brand shape");
        assert_eq!(brand_id, u64::from_le_bytes([1, 2, 3, 4, 5, 6, 7, 8]), "the brand folds little-endian");
        assert_ne!(brand_id, super::super::artifact::shape_id(&brand).expect("hex prefix"), "a binding shape is not a brand fold");
        let binding_id = manifest.bindings[0].shape_id;
        let mut expected = [binding_id, brand_id];
        expected.sort_unstable();
        assert!(
            source.contains(&format!(
                "static const uint64_t compiled_ids[]={{UINT64_C({}),UINT64_C({})}};",
                expected[0], expected[1]
            )),
            "{source}"
        );
    }

    /// A fallible import records its status only inside this thread's Glue checked scope, and a
    /// fallible export's checked invocation runs its checked body counterpart inside that scope.
    /// Without fallible bindings no scope service is emitted and every import failure traps.
    #[test]
    fn fallible_bindings_route_through_the_glue_checked_scope() {
        let mut import = binding(&"78".repeat(32), vec![physical("u64", &["uint64_t"], None)]);
        import.direction = "import".into();
        import.result = physical("u64", &["uint64_t"], None);
        import.fallible = true;
        let mut export = binding(&"9a".repeat(32), vec![physical("u64", &["uint64_t"], None)]);
        export.result = physical("u64", &["uint64_t"], None);
        export.fallible = true;
        let (source, _) = emit(&manifest(vec![export.clone(), import.clone()]), &SCOPE).expect("fallible closure");
        let source = String::from_utf8(source).expect("C source");
        assert!(source.contains("static void * glue_checked_scope_get(void)") && !source.contains("__declspec(thread)"), "{source}");
        assert!(source.contains("size_t *reason=(size_t*)((uint8_t*)scope+24);"), "{source}");
        assert!(source.contains(&format!("extern uint64_t {}(uint64_t);", checked_body_symbol(&export))), "{source}");
        assert!(source.contains("beskid_rt_v5_checked_scope_enter(checked_scope,checked_root)"), "{source}");
        assert!(source.contains("if(checked_reason) status=glue_checked_status(checked_reason);"), "{source}");
        assert!(
            source.contains("if(status) { if(glue_checked_absorb(status)) return output; static const char message[]"),
            "{source}"
        );
        import.fallible = false;
        export.fallible = false;
        let (source, _) = emit(&manifest(vec![export, import]), &SCOPE).expect("infallible closure");
        let source = String::from_utf8(source).expect("C source");
        assert!(!source.contains("glue_checked_absorb") && !source.contains("_checked_body"), "{source}");
        assert!(source.contains("if(status) { static const char message[]"), "{source}");
    }

    /// Two bindings never share one shape identity.
    #[test]
    fn duplicate_binding_shapes_fail_closed() {
        let shape = "56".repeat(32);
        let manifest = manifest(vec![
            binding(&shape, vec![physical("u64", &["uint64_t"], None)]),
            binding(&shape, vec![physical("u64", &["uint64_t"], None)]),
        ]);
        assert!(emit(&manifest, &SCOPE).is_err());
    }
}
