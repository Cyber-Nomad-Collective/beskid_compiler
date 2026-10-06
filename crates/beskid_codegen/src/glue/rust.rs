//! Emit Rust client declarations from canonical logical binding facts.
//! Beskid function bodies remain canonical AOT code; this emitter never translates source text.
use super::artifact::*;
use crate::{CodegenInput, backend::BackendError, module_emission::SyntaxModuleItem};
use beskid_analysis::syntax::PrimitiveType;
use beskid_queries::{GlueDirection, GlueLogicalType, glue_binding, node_span};
use std::collections::BTreeSet;
fn scalar(value: PrimitiveType) -> Result<(&'static str, &'static str, &'static str, bool), BackendError> {
    Ok(match value {
        PrimitiveType::I8 => ("i8", "i8", "int8_t", false),
        PrimitiveType::I16 => ("i16", "i16", "int16_t", false),
        PrimitiveType::I32 => ("i32", "i32", "int32_t", false),
        PrimitiveType::I64 => ("i64", "i64", "int64_t", false),
        PrimitiveType::U8 => ("u8", "u8", "uint8_t", false),
        PrimitiveType::U16 => ("u16", "u16", "uint16_t", false),
        PrimitiveType::U32 => ("u32", "u32", "uint32_t", false),
        PrimitiveType::U64 => ("u64", "u64", "uint64_t", false),
        PrimitiveType::F32 => ("f32", "f32", "float", false),
        PrimitiveType::F64 => ("f64", "f64", "double", false),
        PrimitiveType::Bool => ("bool", "u8", "uint8_t", true),
        PrimitiveType::Char => ("char", "u32", "uint32_t", true),
        PrimitiveType::Word => ("word", "usize", "uintptr_t", true),
        PrimitiveType::Unit => ("unit", "()", "void", false),
        PrimitiveType::String => {
            return Err(BackendError::UnsupportedRustGlueType("string is a managed view, not a scalar".into()));
        }
        PrimitiveType::Pointer | PrimitiveType::Never => {
            return Err(BackendError::UnsupportedRustGlueType("raw pointer/never".into()));
        }
    })
}
fn physical(ty: &GlueLogicalType) -> Result<GluePhysicalType, BackendError> {
    match ty {
        GlueLogicalType::Primitive(PrimitiveType::String) => Ok(GluePhysicalType {
            logical: "utf8".into(),
            slots: vec!["const uint8_t*".into(), "size_t".into()],
            checked: true,
            opaque:None,
        }),
        GlueLogicalType::Primitive(p) => {
            let (logical, _, c, checked) = scalar(*p)?;
            Ok(GluePhysicalType {
                logical: logical.into(),
                slots: if *p == PrimitiveType::Unit { vec![] } else { vec![c.into()] },
                checked,
                opaque:None,
            })
        }
        GlueLogicalType::Array(element) if element.as_ref() == &GlueLogicalType::Primitive(PrimitiveType::U8) => {
            Ok(GluePhysicalType {
                logical: "bytes".into(),
                slots: vec!["const uint8_t*".into(), "size_t".into()],
                checked: true,
            opaque:None,
            })
        }
        GlueLogicalType::Array(_) => {
            Err(BackendError::UnsupportedRustGlueType("only canonically proven u8 arrays are supported".into()))
        }
        GlueLogicalType::Generic { .. } => Err(BackendError::UnsupportedRustGlueType(
            "generic Glue signatures must be canonically specialized before emission".into(),
        )),
        GlueLogicalType::Handle(brand) => {
            let arguments = brand.arguments.iter().map(physical).collect::<Result<Vec<_>, _>>()?;
            Ok(GluePhysicalType {
                logical: format!(
                    "opaque:{}:{}:{}:{}",
                    brand.library,
                    brand.qualified_identity,
                    brand.nullable,
                    serde_json::to_string(&arguments).map_err(GlueArtifactError::from)?
                ),
                slots: vec!["uint64_t".into()],
                checked: true,
            opaque:None,
            })
        }
    }
}
fn managed(ty: &GlueLogicalType) -> bool {
    matches!(ty, GlueLogicalType::Primitive(PrimitiveType::String))
        || matches!(ty,GlueLogicalType::Array(element) if element.as_ref()==&GlueLogicalType::Primitive(PrimitiveType::U8))
}
fn identifier(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(),Some(c) if c=='_'||c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}
fn c_to_rust(c: &str) -> Result<&'static str, BackendError> {
    Ok(match c {
        "int8_t" => "i8",
        "int16_t" => "i16",
        "int32_t" => "i32",
        "int64_t" => "i64",
        "uint8_t" => "u8",
        "uint16_t" => "u16",
        "uint32_t" => "u32",
        "uint64_t" => "u64",
        "float" => "f32",
        "double" => "f64",
        "uintptr_t" | "size_t" => "usize",
        "const uint8_t*" => "*const u8",
        _ => return Err(BackendError::UnsupportedRustGlueType(c.into())),
    })
}
fn checked_rust_type(logical: &str) -> Option<&'static str> {
    Some(match logical {
        "i8" => "i8",
        "i16" => "i16",
        "i32" => "i32",
        "i64" => "i64",
        "u8" => "u8",
        "u16" => "u16",
        "u32" => "u32",
        "u64" => "u64",
        "f32" => "f32",
        "f64" => "f64",
        "bool" => "bool",
        "char" => "char",
        "word" => "usize",
        "unit" => "()",
        "utf8" => "String",
        "bytes" => "Vec<u8>",
        _ => return None,
    })
}
fn checked_rust_wrapper(index: usize, binding: &GlueBindingManifest) -> Result<String, BackendError> {
    let result = if let Some(opaque)=&binding.result.opaque {
        let name=format!("OpaqueHandle{}",opaque.brand_sha256);
        if opaque.nullable{format!("Option<{name}>")}else{name}
    }else{let Some(result)=checked_rust_type(&binding.result.logical)else{return Ok(String::new())};result.to_owned()};
    let mut parameters = Vec::new();
    let mut arguments = Vec::new();
    for (n, ty) in binding.parameters.iter().enumerate() {
        let name = format!("arg_{n}");
        let native = if let Some(opaque)=&ty.opaque {
            let name=format!("OpaqueHandle{}",opaque.brand_sha256);
            if opaque.nullable{format!("Option<&{name}>")}else{format!("&{name}")}
        }else{match ty.logical.as_str() {
            "utf8" => "&str",
            "bytes" => "&[u8]",
            logical => match checked_rust_type(logical) {
                Some(ty) => ty,
                None => return Ok(String::new()),
            },
        }.to_owned()};
        parameters.push(format!("{name}: {native}"));
        if let Some(opaque)=&ty.opaque {
            arguments.push(if opaque.nullable{format!("{name}.map_or(0,|value|value.token)")}else{format!("{name}.token")});
            continue;
        }
        match ty.logical.as_str() {
            "utf8" | "bytes" => {
                arguments.push(format!("{name}.as_ptr()"));
                arguments.push(format!("{name}.len()"));
            }
            "bool" => arguments.push(format!("u8::from({name})")),
            "char" => arguments.push(format!("{name} as u32")),
            "unit" => {}
            _ => arguments.push(name),
        }
    }
    let managed = matches!(binding.result.logical.as_str(), "utf8" | "bytes");
    let mut body = String::new();
    if managed {
        body.push_str("let mut output=GlueOwnedView{pointer:core::ptr::null(),length:0,token:0};\n");
    } else if binding.result.logical != "unit" {
        let physical = c_to_rust(
            binding
                .result
                .slots
                .first()
                .ok_or_else(|| BackendError::UnsupportedRustGlueType("missing checked result slot".into()))?,
        )?;
        body.push_str(&format!("let mut output:{physical}=Default::default();\n"));
    }
    if binding.result.logical != "unit" {
        arguments.push("&mut output".into());
    }
    body.push_str(&format!("let status=unsafe{{checked_binding_{index}({})}};\n", arguments.join(",")));
    if managed {
        let release = format!("release_owned_{}", digest(binding.library.as_bytes()));
        body.push_str(&format!("if status!=0{{if output.token!=0{{let _=unsafe{{{release}(output.token)}};return Err(GlueError::Protocol);}}if !output.pointer.is_null()||output.length!=0{{return Err(GlueError::Protocol);}}return Err(GlueError::Status(status));}}\nlet copied=unsafe{{copy_owned(output,{release})}}?;\n"));
        if binding.result.logical == "utf8" {
            body.push_str("String::from_utf8(copied).map_err(|_|GlueError::InvalidUtf8)\n");
        } else {
            body.push_str("Ok(copied)\n");
        }
    } else if let Some(opaque)=&binding.result.opaque {
        body.push_str("if status!=0{return Err(GlueError::Status(status));}\n");
        let name=format!("OpaqueHandle{}",opaque.brand_sha256);
        if opaque.nullable{body.push_str(&format!("if output==0{{Ok(None)}}else{{Ok(Some({name}{{token:output}}))}}\n"));}
        else{body.push_str(&format!("if output==0{{Err(GlueError::Protocol)}}else{{Ok({name}{{token:output}})}}\n"));}
    } else {
        body.push_str("if status!=0{return Err(GlueError::Status(status));}\n");
        body.push_str(match binding.result.logical.as_str() {
            "bool" => "match output{0=>Ok(false),1=>Ok(true),_=>Err(GlueError::InvalidBool)}\n",
            "char" => "char::from_u32(output).ok_or(GlueError::InvalidChar)\n",
            "unit" => "Ok(())\n",
            _ => "Ok(output)\n",
        });
    }
    Ok(format!("pub fn call_binding_{index}({})->Result<{result},GlueError>{{\n{body}}}\n", parameters.join(",")))
}

pub fn emit(
    input: &CodegenInput<'_>,
    items: &[SyntaxModuleItem],
    native_library: &str,
) -> Result<GlueArtifact, BackendError> {
    if native_library.is_empty()
        || native_library.len() > 256
        || !native_library.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
    {
        return Err(BackendError::InvalidRustGlueLibrary(native_library.into()));
    }
    if items.len() > 4096 {
        return Err(BackendError::UnsupportedRustGlueType("binding bound exceeds4096".into()));
    }
    let mut source_units = input
        .typed_program()
        .assembly
        .units
        .iter()
        .map(|unit| (unit.logical_name.clone(), unit.program.clone()))
        .collect::<Vec<_>>();
    source_units.sort_by(|a, b| a.0.cmp(&b.0));
    let compiled_source_sha256 = digest(&serde_json::to_vec(&source_units).map_err(GlueArtifactError::from)?);
    let mut bindings = Vec::new();
    let mut seen = BTreeSet::new();
    let mut adapters = BTreeSet::new();
    for item in items {
        if item.key.generation != input.typed_program().generation
            || !input
                .typed_program()
                .assembly
                .units
                .iter()
                .any(|u| beskid_queries::SourceUnitId::new(input.database(), u.path.clone()) == item.key.unit)
        {
            return Err(BackendError::StaleRustGlueItem { key: item.key, symbol: item.symbol.clone() });
        }
        let fact = glue_binding(input.database(), item.key)
            .map_err(|e| BackendError::RustGlueFact { key: item.key, message: e.to_string() })?
            .ok_or_else(|| BackendError::MissingRustGlueBinding(item.key))?;
        if fact.declaration != item.key {
            return Err(BackendError::StaleRustGlueItem { key: item.key, symbol: item.symbol.clone() });
        }
        if !identifier(&fact.symbol) {
            return Err(BackendError::InvalidRustGlueSymbol(fact.symbol));
        }
        if !fact.generics.is_empty() {
            return Err(BackendError::UnsupportedRustGlueType("unspecialized generic declaration".into()));
        }
        let library = match fact.direction {
            GlueDirection::Export => native_library.to_owned(),
            GlueDirection::Import => fact
                .library
                .clone()
                .ok_or_else(|| BackendError::InvalidRustGlueLibrary("missing import library".into()))?,
        };
        if !seen.insert((library.clone(), fact.symbol.clone())) {
            return Err(BackendError::DuplicateRustGlueSymbol(fact.symbol));
        }
        let mut parameters = fact.parameters.iter().map(|p| physical(&p.ty)).collect::<Result<Vec<_>, _>>()?;
        let mut result = physical(&fact.result)?;
        if managed(&fact.result) {
            result.slots = vec!["int32_t".into(), "GlueOwnedView*".into()];
            adapters.insert(format!("{}:{}:managed_return_v1", library, fact.symbol));
        }
        if fact.parameters.iter().any(|p| managed(&p.ty)) {
            adapters.insert(format!("{}:{}:managed_input_v1", library, fact.symbol));
        }
        if fact.parameters.iter().any(|p| matches!(p.ty, GlueLogicalType::Handle(_)))
            || matches!(fact.result, GlueLogicalType::Handle(_))
        {
            adapters.insert(format!("{}:{}:checked_owner_v1", library, fact.symbol));
        }
        if parameters.iter().any(|p| p.checked) || result.checked {
            adapters.insert(format!("{}:{}:checked_values_v1", library, fact.symbol));
        }
        for plan in super::handles::source_handle_box_plans(input,item.key)? {
            let metadata=GlueOpaqueType{
                brand_sha256:plan.brand_sha256.clone(),library:plan.library,nullable:plan.nullable,
                constructor:format!("beskid_glue_handle_{}_construct",plan.brand_sha256),
                reader:format!("beskid_glue_handle_{}_read",plan.brand_sha256),
                descriptor:plan.plan.descriptor_getter.ok_or_else(||BackendError::UnsupportedRustGlueType("opaque descriptor getter absent".into()))?,
            };
            match plan.position {
                Some(index)=>parameters.get_mut(index).ok_or_else(||BackendError::UnsupportedRustGlueType("opaque position outside signature".into()))?.opaque=Some(metadata),
                None=>result.opaque=Some(metadata),
            }
        }
        let unit = input
            .typed_program()
            .assembly
            .units
            .iter()
            .find(|u| beskid_queries::SourceUnitId::new(input.database(), u.path.clone()) == item.key.unit)
            .ok_or_else(|| BackendError::MissingRustGlueBinding(item.key))?;
        let span = node_span(input.database(), item.key)
            .map_err(|e| BackendError::RustGlueFact { key: item.key, message: e.to_string() })?
            .ok_or_else(|| BackendError::MissingRustGlueBinding(item.key))?;
        let source = GlueSourceAuthority {
            logical_unit: unit.logical_name.clone(),
            typed_unit_sha256: digest(&serde_json::to_vec(&unit.program).map_err(GlueArtifactError::from)?),
            span_start: span.start,
            span_end: span.end,
        };
        let mut binding = GlueBindingManifest {
            identity_sha256: String::new(),
            direction: match fact.direction {
                GlueDirection::Import => "import",
                GlueDirection::Export => "export",
            }
            .into(),
            library,
            symbol: fact.symbol,
            body_symbol: String::new(),
            shape_id: 0,
            parameters,
            result,
            source,
        };
        let shape = binding_shape_digest(&compiled_source_sha256, &binding)?;
        binding.shape_id = shape_id(&shape)?;
        binding.body_symbol = format!("__beskid_glue_body_{shape}");
        binding.identity_sha256 = digest(&serde_json::to_vec(&binding).map_err(GlueArtifactError::from)?);
        bindings.push(binding);
    }
    if bindings.is_empty() {
        return Err(BackendError::UnsupportedRustGlueType("no declared Glue bindings".into()));
    }
    bindings
        .sort_by(|a, b| (&a.library, &a.symbol, &a.identity_sha256).cmp(&(&b.library, &b.symbol, &b.identity_sha256)));
    let mut rust = String::from(
        "// Generated from registered canonical Glue facts; Beskid bodies remain AOT.\n#[repr(C)]\npub struct GlueOwnedView { pub pointer: *const u8, pub length: usize, pub token: u64 }\nconst _: () = { assert!(core::mem::size_of::<GlueOwnedView>() == 24); assert!(core::mem::align_of::<GlueOwnedView>() == 8); assert!(core::mem::offset_of!(GlueOwnedView, token) == 16); };\n",
    );
    rust.push_str(
        r#"
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum GlueError { Status(i32), Protocol, InvalidBool, InvalidChar, InvalidUtf8, Release(i32) }
unsafe fn copy_owned(view:GlueOwnedView,release:unsafe extern "C" fn(u64)->i32)->Result<Vec<u8>,GlueError> {
    if view.token==0 {return Err(GlueError::Protocol);}
    let valid=view.length<=16*1024*1024 && !view.pointer.is_null();
    let copied=if valid {Some(unsafe{core::slice::from_raw_parts(view.pointer,view.length)}.to_vec())}else{None};
    let status=unsafe{release(view.token)};
    if status!=0{return Err(GlueError::Release(status));}
    copied.ok_or(GlueError::Protocol)
}
"#,
    );
    let mut emitted_brands=std::collections::BTreeSet::new();
    for binding in &bindings {
        for physical in binding.parameters.iter().chain(std::iter::once(&binding.result)) {
            let Some(opaque)=&physical.opaque else{continue};
            if !emitted_brands.insert(opaque.brand_sha256.clone()){continue;}
            let name=format!("OpaqueHandle{}",opaque.brand_sha256);
            let release=format!("beskid_glue_opaque_{}_release",opaque.brand_sha256);
            rust.push_str(&format!("#[derive(Debug)]\npub struct {name}{{token:u64}}\nimpl {name}{{pub fn close(&mut self)->Result<(),GlueError>{{if self.token==0{{return Ok(());}}let status=unsafe{{{release}(self.token)}};if status==0||status==2||status==6{{self.token=0;}}if status==0{{Ok(())}}else{{Err(GlueError::Release(status))}}}}}}\nimpl Drop for {name}{{fn drop(&mut self){{let token=core::mem::replace(&mut self.token,0);if token!=0{{let _=unsafe{{{release}(token)}};}}}}}}\n#[link(name={native_library:?})]\nunsafe extern \"C\"{{fn {release}(token:u64)->i32;}}\n"));
        }
    }
    let mut header = String::from(
        "/* Generated normalized local C ABI; not the stdio wire protocol. */\n#ifndef BESKID_GENERATED_GLUE_H\n#define BESKID_GENERATED_GLUE_H\n#include <stdint.h>\n#include <stddef.h>\ntypedef struct { const uint8_t *pointer; size_t length; uint64_t token; } GlueOwnedView;\ntypedef int32_t (*GlueAdmitV1)(void*,const uint8_t*,const uint64_t*,size_t,uint64_t*);\ntypedef struct { uint32_t version; uint32_t size; void *context; GlueAdmitV1 admit; } GlueAdmissionTableV1;\nint32_t beskid_glue_artifact_v1_initialize(const GlueAdmissionTableV1*);\n",
    );
    for (index, binding) in bindings.iter().enumerate() {
        let mut cargs = Vec::new();
        let mut rargs = Vec::new();
        for (param, ty) in binding.parameters.iter().enumerate() {
            for (slot, c) in ty.slots.iter().enumerate() {
                let name = format!("arg_{param}_{slot}");
                cargs.push(format!("{c} {name}"));
                rargs.push(format!("{name}: {}", c_to_rust(c)?));
            }
        }
        if binding.direction == "export" {
            let mut checked_cargs = cargs.clone();
            let mut checked_rargs = rargs.clone();
            let output_type = if matches!(binding.result.logical.as_str(), "utf8" | "bytes") {
                Some(("GlueOwnedView", "GlueOwnedView"))
            } else if let Some(result) = binding.result.slots.first() {
                Some((result.as_str(), c_to_rust(result)?))
            } else {
                None
            };
            if let Some((c, rust_ty)) = output_type {
                checked_cargs.push(format!("{c} *output"));
                checked_rargs.push(format!("output: *mut {rust_ty}"));
            }
            let checked = checked_invocation_symbol(binding);
            header.push_str(&format!(
                "int32_t {checked}({});\n",
                if checked_cargs.is_empty() { "void".into() } else { checked_cargs.join(", ") }
            ));
            rust.push_str(&format!("#[link(name = {:?})]\nunsafe extern \"C\" {{ #[link_name = {checked:?}] pub fn checked_binding_{index}({}) -> i32; }}\n", binding.library, checked_rargs.join(", ")));
            rust.push_str(&checked_rust_wrapper(index, binding)?);
            if requires_checked_primary(binding) {
                continue;
            }
        }
        let result = if binding.result.slots.len() == 2 && binding.result.slots[1] == "GlueOwnedView*" {
            cargs.push("GlueOwnedView* output".into());
            rargs.push("output: *mut GlueOwnedView".into());
            "int32_t"
        } else {
            binding.result.slots.first().map(String::as_str).unwrap_or("void")
        };
        rust.push_str(&format!("#[link(name = {:?})]\nunsafe extern \"C\" {{\n    #[link_name = {:?}]\n    pub fn binding_{index}({}) -> {};\n}}\n",binding.library,binding.symbol,rargs.join(", "),if result=="void"{"()"}else{c_to_rust(result)?}));
        header.push_str(&format!(
            "{result} {}({});\n",
            binding.symbol,
            if cargs.is_empty() { "void".into() } else { cargs.join(", ") }
        ));
    }
    for library in bindings
        .iter()
        .filter(|b| matches!(b.result.logical.as_str(), "utf8" | "bytes"))
        .map(|b| &b.library)
        .collect::<BTreeSet<_>>()
    {
        let symbol = owned_release_symbol(library);
        rust.push_str(&format!("#[link(name = {library:?})]\nunsafe extern \"C\" {{ #[link_name = {symbol:?}] pub fn release_owned_{}(token: u64) -> i32; }}\n",digest(library.as_bytes())));
        header.push_str(&format!("int32_t {symbol}(uint64_t token);\n"));
    }
    header.push_str("#endif\n");
    let mut tools =
        vec!["rustc".into(), if input.target().object_format.as_str() == "coff" { "cl".into() } else { "cc".into() }];
    tools.sort();
    let manifest = GlueManifest {
        schema_version: 1,
        backend: "rust".into(),
        target: input.target().triple.as_str().into(),
        runtime_abi: 5,
        runtime_linkage: "canonical_shared_provider".into(),
        owner_issuer_version: 1,
        native_width_bits: 64,
        compiled_source_sha256,
        bindings,
        required_tools: tools,
        required_native_adapters: adapters.into_iter().collect(),
        expected_outputs: vec!["libbeskid_glue_client.rlib".into()],
    };
    let (native, shapes) = super::native::emit(&manifest)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(GlueArtifactError::from)?;
    let mut files = vec![GeneratedFile::new("Cargo.toml",b"[package]\nname = \"beskid_glue_client\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[lib]\npath = \"src/lib.rs\"\ncrate-type = [\"rlib\"]\n[dependencies]\nbeskid_glue = { path = \"vendor/beskid_glue\" }\nbeskid_serialization = { path = \"vendor/beskid_serialization\" }\n".to_vec()),GeneratedFile::new("src/lib.rs",format!("pub mod peer;\n{rust}").into_bytes()),GeneratedFile::new("include/beskid_glue.h",header.into_bytes()),GeneratedFile::new("glue-manifest.json",manifest_bytes),GeneratedFile::new("native/adapters.c",native),GeneratedFile::new("native/compiled-shapes.json",shapes)];
    files.push(GeneratedFile::new("src/peer.rs", generated_peer(&manifest)?));
    files.extend(shared_transport_sources());
    GlueArtifact::new(manifest, files).map_err(BackendError::from)
}

/// Freeze the complete format-neutral model and reciprocal protocol source into
/// the emitted artifact. Generated projects never depend on the compiler/AOT.
fn shared_transport_sources() -> Vec<GeneratedFile> {
    let mut files = vec![
        GeneratedFile::new("vendor/beskid_serialization/Cargo.toml", b"[package]\nname = \"beskid_serialization\"\nversion = \"0.1.0\"\nedition = \"2024\"\n".to_vec()),
        GeneratedFile::new("vendor/beskid_glue/Cargo.toml", b"[package]\nname = \"beskid_glue\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nbeskid_serialization = { path = \"../beskid_serialization\" }\n".to_vec()),
        GeneratedFile::new("vendor/beskid_serialization/src/lib.rs", include_bytes!("../../../beskid_serialization/src/lib.rs").to_vec()),
    ];
    macro_rules! source {
        ($name:literal) => {
            files.push(GeneratedFile::new(
                concat!("vendor/beskid_glue/src/", $name),
                include_bytes!(concat!("../../../beskid_glue/src/", $name)).to_vec(),
            ));
        };
    }
    source!("lib.rs");
    source!("wire.rs");
    source!("codec.rs");
    source!("session.rs");
    source!("pump.rs");
    source!("peer.rs");
    files
}
fn peer_type(ty: &GluePhysicalType) -> Option<&'static str> {
    checked_rust_type(&ty.logical)
}
fn peer_decode(logical: &str, name: &str) -> Option<String> {
    Some(match logical {
        "unit" => format!("match {name} {{ Value::Unit => (), _ => return Err(invalid()) }}"),
        "bool" => format!("match {name} {{ Value::Boolean(v) => v, _ => return Err(invalid()) }}"),
        "char" => format!("match {name} {{ Value::Scalar(v) => v, _ => return Err(invalid()) }}"),
        "utf8" => format!("match {name} {{ Value::String(v) => v, _ => return Err(invalid()) }}"),
        "bytes" => format!("match {name} {{ Value::Bytes(v) => v, _ => return Err(invalid()) }}"),
        "f32" => format!(
            "match {name} {{ Value::Float{{bits,width:32}} if bits <= u32::MAX as u64 => f32::from_bits(bits as u32), _ => return Err(invalid()) }}"
        ),
        "f64" => format!(
            "match {name} {{ Value::Float{{bits,width:64}} => f64::from_bits(bits), _ => return Err(invalid()) }}"
        ),
        "word" => format!(
            "match {name} {{ Value::Unsigned{{value,width:64}} => usize::try_from(value).map_err(|_|invalid())?, _ => return Err(invalid()) }}"
        ),
        value if value.starts_with('i') => {
            let width = &value[1..];
            format!(
                "match {name} {{ Value::Signed{{value,width:{width}}} => {value}::try_from(value).map_err(|_|invalid())?, _ => return Err(invalid()) }}"
            )
        }
        value if value.starts_with('u') => {
            let width = &value[1..];
            format!(
                "match {name} {{ Value::Unsigned{{value,width:{width}}} => {value}::try_from(value).map_err(|_|invalid())?, _ => return Err(invalid()) }}"
            )
        }
        _ => return None,
    })
}
fn peer_encode(logical: &str) -> Option<String> {
    Some(match logical {
        "unit" => "Value::Unit".into(),
        "bool" => "Value::Boolean(result)".into(),
        "char" => "Value::Scalar(result)".into(),
        "utf8" => "Value::String(result)".into(),
        "bytes" => "Value::Bytes(result)".into(),
        "f32" => "Value::Float{bits:result.to_bits() as u64,width:32}".into(),
        "f64" => "Value::Float{bits:result.to_bits(),width:64}".into(),
        "word" => "Value::Unsigned{value:result as u64,width:64}".into(),
        v if v.starts_with('i') => format!("Value::Signed{{value:result as i64,width:{}}}", &v[1..]),
        v if v.starts_with('u') => format!("Value::Unsigned{{value:result as u64,width:{}}}", &v[1..]),
        _ => return None,
    })
}
fn generated_peer(manifest: &GlueManifest) -> Result<Vec<u8>, BackendError> {
    let bindings = &manifest.bindings;
    let mut out = String::from(
        "use beskid_serialization::{DataValue as Value,SequenceKind};\nfn invalid()->Value{Value::String(\"invalid_argument\".into())}\npub trait Service {\n",
    );
    for (i, b) in bindings.iter().enumerate() {
        if let Some(result) = peer_type(&b.result) {
            let args = b
                .parameters
                .iter()
                .enumerate()
                .map(|(n, t)| peer_type(t).map(|t| format!("arg_{n}:{t}")))
                .collect::<Option<Vec<_>>>();
            if let Some(args) = args {
                out.push_str(&format!("fn binding_{i}(&mut self,{})->Result<{result},Value>;\n", args.join(",")));
            }
        }
    }
    out.push_str("}\npub fn dispatch<S:Service>(service:&mut S,index:usize,value:Value)->Result<Value,Value>{\nlet Value::Sequence{kind:SequenceKind::Array,values}=value else{return Err(invalid())};\nmatch index{\n");
    for (i, b) in bindings.iter().enumerate() {
        let Some(encoded) = peer_encode(&b.result.logical) else { continue };
        let decodes = b
            .parameters
            .iter()
            .enumerate()
            .map(|(n, t)| {
                peer_decode(&t.logical, &format!("values.next().ok_or_else(invalid)?"))
                    .map(|d| format!("let arg_{n}={d};\n"))
            })
            .collect::<Option<Vec<_>>>();
        let Some(decodes) = decodes else { continue };
        out.push_str(&format!(
            "{i}=>{{if values.len()!={}{{return Err(invalid())}}let mut values=values.into_iter();\n{}",
            b.parameters.len(),
            decodes.join("")
        ));
        let args = (0..b.parameters.len()).map(|n| format!("arg_{n}")).collect::<Vec<_>>().join(",");
        out.push_str(&format!("let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||service.binding_{i}({args}))).map_err(|_|Value::String(\"foreign_panic\".into()))??;Ok({encoded})}},\n"));
    }
    out.push_str("_=>Err(Value::String(\"unadmitted_binding\".into()))}}\n");
    out.push_str("pub fn serve<S:Service>(service:&mut S,library:&str,generation:u64)->Result<(),beskid_glue::peer::PeerError>{\nlet (specs,indices,compatibility)=match library{\n");
    for library in bindings.iter().map(|b| &b.library).collect::<BTreeSet<_>>() {
        let mut specs = Vec::new();
        let mut indices = Vec::new();
        for (i, b) in bindings.iter().enumerate().filter(|(_, b)| &b.library == library) {
            let identity = hex_bytes(&b.identity_sha256)?;
            let shape = hex_bytes(&binding_shape_digest(&manifest.compiled_source_sha256, b)?)?;
            specs.push(format!("beskid_glue::peer::BindingSpec{{identity:{identity:?},shapes:vec!{:?}}}", shape));
            indices.push(i);
        }
        out.push_str(&format!(
            "{library:?}=>(vec![{}],vec!{indices:?},Value::String({:?}.into())),\n",
            specs.join(","),
            digest(&serde_json::to_vec(manifest).map_err(GlueArtifactError::from)?)
        ));
    }
    out.push_str("_=>return Err(beskid_glue::peer::PeerError::Binding)};\nlet mut peer=beskid_glue::peer::Peer::new(library,generation,compatibility,&specs)?;\nbeskid_glue::peer::serve_stdio(&mut peer,&mut |index,value|dispatch(service,indices[index],value))}\n");
    Ok(out.into_bytes())
}

fn hex_bytes(value: &str) -> Result<Vec<u8>, BackendError> {
    if value.len() != 64 {
        return Err(BackendError::UnsupportedRustGlueType("invalid compiled digest".into()));
    }
    (0..32)
        .map(|i| {
            u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
                .map_err(|_| BackendError::UnsupportedRustGlueType("invalid compiled digest".into()))
        })
        .collect()
}
