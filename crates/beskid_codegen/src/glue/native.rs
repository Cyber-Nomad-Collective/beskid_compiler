//! Normalized C wrappers. This source is compiled and linked with canonical AOT
//! bodies; it never interprets or reparses Beskid source.
use super::artifact::*;
use std::collections::BTreeSet;

fn managed(ty: &GluePhysicalType) -> bool {
    matches!(ty.logical.as_str(), "utf8" | "bytes")
}
fn rooted(ty:&GluePhysicalType)->bool {managed(ty)||ty.opaque.is_some()}
fn body_type(ty: &GluePhysicalType) -> &str {
    if rooted(ty) { "void*" } else { ty.slots.first().map(String::as_str).unwrap_or("void") }
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
        let result_type = if owned { "GlueOwnedView" } else if b.result.opaque.is_some(){"uint64_t"} else { body_type(&b.result) };
        let has_result = owned || !b.result.slots.is_empty();
        if has_result {
            arguments.push(format!("{result_type} *output"));
        }
        c.push_str(&format!(
            "extern {} {}({});\nint32_t {}({}) {{\n",
            body_type(&b.result),
            b.body_symbol,
            if body_types.is_empty() { "void".into() } else { body_types.join(",") },
            checked_invocation_symbol(b),
            if arguments.is_empty() { "void".into() } else { arguments.join(",") }
        ));
        if has_result {
            c.push_str(&format!(" if(!output || (uintptr_t)output % _Alignof({result_type})) return 1;\n memset(output,0,sizeof(*output));\n"));
        }
        c.push_str(&format!(" uint64_t library=0; int32_t status=bind_shapes(&library); if(status) return status;\n status=beskid_glue_v1_owner_validate_binding(library,UINT64_C({})); if(status) return status;\n",b.shape_id));
        for (index, parameter) in b.parameters.iter().enumerate() {
            match parameter.logical.as_str() {
                "bool" => c.push_str(&format!(" if(arg_{index}_0>1) return 1;\n")),
                "char" => c.push_str(&format!(" if(arg_{index}_0>UINT32_C(0x10ffff) || (arg_{index}_0>=UINT32_C(0xd800) && arg_{index}_0<=UINT32_C(0xdfff))) return 1;\n")),
                _ => {},
            }
        }
        for (index, parameter) in b.parameters.iter().enumerate().filter(|(_, parameter)| rooted(parameter)) {
            if let Some(opaque)=&parameter.opaque {
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
        for (index, _) in b.parameters.iter().enumerate().filter(|(_, parameter)| rooted(parameter)) {
            c.push_str(&format!(" managed_{index}=beskid_rt_v5_gc_resolve_handle(root_{index}); if(!managed_{index}) status=2;\n"));
        }
        c.push_str(" if(!status) {\n");
        let invocation = format!("{}({})", b.body_symbol, body_arguments.join(","));
        if owned {
            c.push_str(&format!(
                " void *result={invocation};\n status=beskid_glue_v1_result_{}(library,UINT64_C({}),result,output);\n",
                b.result.logical, b.shape_id
            ));
        } else if let Some(opaque)=&b.result.opaque {
            let brand=hex_c(&opaque.brand_sha256);
            c.push_str(&format!(" extern int32_t {}(void*,uint64_t*);static const uint8_t result_brand[32]={{{brand}}};void *result={invocation};uint64_t token=0;status={}(result,&token);\n",opaque.reader,opaque.reader));
            if opaque.nullable {c.push_str(" if(!status&&token)status=beskid_glue_v1_owner_validate_opaque(library,result_brand,token);\n");}
            else {c.push_str(" if(!status)status=beskid_glue_v1_owner_validate_opaque(library,result_brand,token);\n");}
            c.push_str(" if(!status)*output=token;\n");
        } else if has_result {
            c.push_str(&format!(" {result_type} result={invocation};\n"));
            match b.result.logical.as_str() {
                "bool" => c.push_str(" if(result>1) status=1; else *output=result;\n"),
                "char" => c.push_str(" if(result>UINT32_C(0x10ffff) || (result>=UINT32_C(0xd800) && result<=UINT32_C(0xdfff))) status=1; else *output=result;\n"),
                _ => c.push_str(" *output=result;\n"),
            }
        } else {
            c.push_str(&format!(" {invocation};\n"));
        }
        c.push_str(" }\n");
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
            } else {
                foreign_args.push(if parameter.opaque.is_some(){format!("opaque_token_{index}")}else{format!("value_{index}")});
            }
        }
        let owned = managed(&binding.result);
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
        if owned {
            c.push_str(" if(foreign_output.token) {int32_t released=beskid_glue_v1_owner_release_token(foreign_library,foreign_output.token);if(released && !status)status=released;}\n");
        }
        for (index, _) in binding.parameters.iter().enumerate().filter(|(_, parameter)| managed(parameter)) {
            c.push_str(&format!(" if(input_{index}.token) {{int32_t released=beskid_glue_v1_owner_release_token(library,input_{index}.token);if(released && !status) status=released;}} if(root_{index}) gc_unroot_handle(root_{index});\n"));
        }
        for (index,_) in binding.parameters.iter().enumerate().filter(|(_,p)|p.opaque.is_some()){
            c.push_str(&format!("if(opaque_root_{index})gc_unroot_handle(opaque_root_{index});\n"));
        }
        if owned || binding.result.opaque.is_some() {
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
        c.push_str(&format!(
            " int32_t status={checked}({}); if(status) glue_import_failure();\n",
            source_values.join(",")
        ));
        if has_result {
            c.push_str(" return output;\n");
        }
        c.push_str("}\n");
    }
}
pub(super) fn emit(manifest: &GlueManifest) -> Result<(Vec<u8>, Vec<u8>), GlueArtifactError> {
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
    let mut ids: BTreeSet<_> = manifest.bindings.iter().map(|binding| binding.shape_id).collect();
    for binding in &manifest.bindings {
        for physical in binding.parameters.iter().chain(std::iter::once(&binding.result)) {
            if let Some(opaque) = &physical.opaque {
                ids.insert(super::artifact::shape_id(&opaque.brand_sha256)?);
            }
        }
    }
    if ids.len()>4096 {return Err(GlueArtifactError::Invalid("opaque shape closure exceeds4096".into()));}

    if ids.is_empty() || ids.len() != manifest.bindings.len() || ids.len() > 4096 || ids.contains(&0) {
        return Err(GlueArtifactError::Invalid("invalid compiled shape closure".into()));
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
    emit_checked_wrappers(manifest, &mut c);
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
