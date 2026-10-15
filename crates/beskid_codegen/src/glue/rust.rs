//! Emit Rust client declarations from canonical logical binding facts.
//! Beskid function bodies remain canonical AOT code; this emitter never translates source text.
use super::artifact::*;
use crate::{CodegenInput, backend::BackendError, module_emission::SyntaxModuleItem};
use beskid_analysis::syntax::PrimitiveType;
use beskid_queries::{
    AstNodeKey, CallLowering, GlueDirection, GlueLogicalType, call_lowering, child_nodes, glue_binding, node_span,
};
use std::collections::{BTreeSet, HashSet};

/// Selected imports whose Rust owner callable is declared `[RustOwner(Fallible:true)]`.
fn fallible_imports(input: &CodegenInput<'_>, items: &[SyntaxModuleItem]) -> Result<HashSet<AstNodeKey>, BackendError> {
    let mut keys = HashSet::new();
    for item in items {
        let fact = glue_binding(input.database(), item.key)
            .map_err(|e| BackendError::RustGlueFact { key: item.key, message: e.to_string() })?;
        if let Some(fact) = fact
            && fact.direction == GlueDirection::Import
            && fact.rust_owner.as_ref().is_some_and(|owner| owner.fallible)
        {
            keys.insert(item.key);
        }
    }
    Ok(keys)
}

/// Whether `export` reaches one of the `fallible` imports through its direct-call closure. Such an
/// export is fallible: its checked invocation runs a checked counterpart of the body and returns a
/// recorded foreign status. A path the direct-call closure does not contain never enters a Glue
/// checked scope, so a failure on it still traps.
fn reaches_fallible_import(
    input: &CodegenInput<'_>,
    export: AstNodeKey,
    fallible: &HashSet<AstNodeKey>,
) -> Result<bool, BackendError> {
    if fallible.is_empty() {
        return Ok(false);
    }
    let db = input.database();
    let fact_error = |message: String| BackendError::RustGlueFact { key: export, message };
    let root = input
        .roots()
        .iter()
        .copied()
        .find(|root| root.unit == export.unit)
        .ok_or_else(|| fact_error("Glue export has no source root".into()))?;
    let closure = beskid_queries::reachable_items(db, root, export)
        .map_err(|error| fact_error(error.to_string()))?
        .ok_or_else(|| fact_error("incomplete direct-call facts for a Glue export".into()))?;
    let mut pending = std::iter::once(export).chain(closure.iter().copied()).collect::<Vec<_>>();
    let mut seen = HashSet::new();
    while let Some(node) = pending.pop() {
        if !seen.insert(node) {
            continue;
        }
        if let Ok(Some(CallLowering::Direct(declaration))) = call_lowering(db, node)
            && fallible.contains(&declaration)
        {
            return Ok(true);
        }
        if let Some(children) = child_nodes(db, node).map_err(|error| fact_error(error.to_string()))? {
            pending.extend(children.iter().copied());
        }
    }
    Ok(false)
}
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
pub(super) fn physical(ty: &GlueLogicalType) -> Result<GluePhysicalType, BackendError> {
    match ty {
        GlueLogicalType::Primitive(PrimitiveType::String) => Ok(GluePhysicalType::plain(
            "utf8",
            vec!["const uint8_t*".into(), "size_t".into()],
            true,
        )),
        GlueLogicalType::Primitive(p) => {
            let (logical, _, c, checked) = scalar(*p)?;
            Ok(GluePhysicalType::plain(logical, if *p == PrimitiveType::Unit { vec![] } else { vec![c.into()] }, checked))
        }
        GlueLogicalType::Array(element) if element.as_ref() == &GlueLogicalType::Primitive(PrimitiveType::U8) => {
            Ok(GluePhysicalType::plain("bytes", vec!["const uint8_t*".into(), "size_t".into()], true))
        }
        GlueLogicalType::Array(_) => {
            Err(BackendError::UnsupportedRustGlueType("only canonically proven u8 arrays are supported".into()))
        }
        GlueLogicalType::Generic { .. } => Err(BackendError::UnsupportedRustGlueType(
            "generic Glue signatures must be canonically specialized before emission".into(),
        )),
        GlueLogicalType::Handle(brand) => {
            let arguments = brand.arguments.iter().map(physical).collect::<Result<Vec<_>, _>>()?;
            Ok(GluePhysicalType::plain(
                format!(
                    "opaque:{}:{}:{}:{}",
                    brand.library,
                    brand.qualified_identity,
                    brand.nullable,
                    serde_json::to_string(&arguments).map_err(GlueArtifactError::from)?
                ),
                vec!["uint64_t".into()],
                true,
            ))
        }
        GlueLogicalType::Record(record) => {
            // The C ABI profile binding of the one lowered shape is the only layout authority.
            let beskid_abi::interop::TypeShape::Record(shape) = beskid_queries::glue_type_shape(ty)
                .map_err(|error| BackendError::UnsupportedRustGlueType(error.to_string()))?
            else {
                return Err(BackendError::UnsupportedRustGlueType("GlueRecord did not lower to a record shape".into()));
            };
            let fields = record
                .fields
                .iter()
                .zip(&shape.fields)
                .map(|(field, lowered)| {
                    let (logical, _, c, _) = scalar(field.ty)?;
                    if lowered.name != field.name {
                        return Err(BackendError::UnsupportedRustGlueType("GlueRecord field order differs".into()));
                    }
                    Ok(GlueRecordField {
                        name: field.name.clone(),
                        logical: logical.into(),
                        c_type: c.into(),
                        offset: lowered.offset,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            if fields.len() != shape.fields.len() {
                return Err(BackendError::UnsupportedRustGlueType("GlueRecord field count differs".into()));
            }
            let record_sha256 = digest(
                &serde_json::to_vec(&(&*record.qualified_identity, &fields, shape.size, shape.alignment))
                    .map_err(GlueArtifactError::from)?,
            );
            let c_name = format!("GlueRecord_{record_sha256}");
            Ok(GluePhysicalType {
                logical: format!("record:{}", record.qualified_identity),
                slots: vec![c_name.clone()],
                checked: true,
                opaque: None,
                record: Some(GlueRecordType {
                    constructor: record_constructor_symbol(&record_sha256),
                    reader: record_reader_symbol(&record_sha256),
                    record_sha256,
                    identity: record.qualified_identity.to_string(),
                    c_name,
                    fields,
                    size: shape.size,
                    alignment: shape.alignment,
                    rust_owner: record.rust_owner.as_deref().map(str::to_owned),
                }),
                enumeration: None,
                callback: None,
            })
        }
        GlueLogicalType::Enum(enumeration) => {
            beskid_queries::glue_type_shape(ty).map_err(|error| BackendError::UnsupportedRustGlueType(error.to_string()))?;
            let enum_sha256 = digest(
                &serde_json::to_vec(&(&*enumeration.qualified_identity, &*enumeration.variants))
                    .map_err(GlueArtifactError::from)?,
            );
            Ok(GluePhysicalType {
                logical: format!("enum:{}", enumeration.qualified_identity),
                slots: vec!["uint32_t".into()],
                checked: true,
                opaque: None,
                record: None,
                enumeration: Some(GlueEnumType {
                    constructor: enum_constructor_symbol(&enum_sha256),
                    reader: enum_reader_symbol(&enum_sha256),
                    enum_sha256,
                    identity: enumeration.qualified_identity.to_string(),
                    variants: enumeration.variants.to_vec(),
                    rust_owner: enumeration.rust_owner.as_deref().map(str::to_owned),
                }),
                callback: None,
            })
        }
        GlueLogicalType::Callback(callback) => {
            beskid_queries::glue_type_shape(ty).map_err(|error| BackendError::UnsupportedRustGlueType(error.to_string()))?;
            let parameters = callback.parameters.iter().map(physical).collect::<Result<Vec<_>, _>>()?;
            let result = physical(&callback.result)?;
            let callback_sha256 =
                digest(&serde_json::to_vec(&(&parameters, &result)).map_err(GlueArtifactError::from)?);
            let c_name = format!("GlueCallback_{callback_sha256}");
            Ok(GluePhysicalType {
                logical: format!("callback:{callback_sha256}"),
                slots: vec![c_name.clone(), "uint64_t".into()],
                checked: true,
                opaque: None,
                record: None,
                enumeration: None,
                callback: Some(Box::new(GlueCallbackType {
                    invoke: format!("beskid_glue_callback_{callback_sha256}_invoke"),
                    callback_sha256,
                    c_name,
                    parameters,
                    result,
                })),
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
/// A Rust identifier for a Beskid field or variant name: a Rust keyword becomes a raw identifier;
/// the four names that cannot be raw identifiers are rejected.
pub fn rust_identifier(name: &str) -> Result<String, BackendError> {
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern", "false", "fn", "for",
        "gen", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "static",
        "struct", "trait", "true", "try", "type", "unsafe", "use", "where", "while", "abstract", "become", "box", "do",
        "final", "macro", "override", "priv", "typeof", "unsized", "virtual", "yield",
    ];
    if !identifier(name) || matches!(name, "self" | "Self" | "super" | "crate" | "_") {
        return Err(BackendError::UnsupportedRustGlueType(format!("`{name}` is not a Rust identifier")));
    }
    Ok(if KEYWORDS.contains(&name) { format!("r#{name}") } else { name.to_owned() })
}
fn c_to_rust(c: &str) -> Result<String, BackendError> {
    if (c.starts_with("GlueRecord_") || c.starts_with("GlueCallback_")) && identifier(c) {
        return Ok(c.to_owned());
    }
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
    }
    .to_owned())
}
/// Rust client name of a record or enum physical type.
fn nominal_rust_type(ty: &GluePhysicalType) -> Option<String> {
    ty.record
        .as_ref()
        .map(|record| record.c_name.clone())
        .or_else(|| ty.enumeration.as_ref().map(|enumeration| format!("GlueEnum_{}", enumeration.enum_sha256)))
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
    }else if let Some(nominal) = nominal_rust_type(&binding.result) {
        nominal
    }else{let Some(result)=checked_rust_type(&binding.result.logical)else{return Ok(String::new())};result.to_owned()};
    let mut parameters = Vec::new();
    let mut arguments = Vec::new();
    for (n, ty) in binding.parameters.iter().enumerate() {
        let name = format!("arg_{n}");
        let native = if let Some(opaque)=&ty.opaque {
            let name=format!("OpaqueHandle{}",opaque.brand_sha256);
            if opaque.nullable{format!("Option<&{name}>")}else{format!("&{name}")}
        }else if let Some(nominal) = nominal_rust_type(ty) {
            nominal
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
        if ty.enumeration.is_some() {
            arguments.push(format!("{name}.into_raw()"));
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
    } else if let Some(record) = &binding.result.record {
        // Every record field type has an all-zero value (Bool false, char NUL).
        body.push_str(&format!("let mut output:{}=unsafe{{core::mem::zeroed()}};\n", record.c_name));
    } else if binding.result.enumeration.is_some() {
        body.push_str("let mut output:u32=0;\n");
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
    } else if binding.result.record.is_some() {
        body.push_str("if status!=0{return Err(GlueError::Status(status));}\nOk(output)\n");
    } else if let Some(nominal) = binding.result.enumeration.as_ref().and(nominal_rust_type(&binding.result)) {
        body.push_str(&format!(
            "if status!=0{{return Err(GlueError::Status(status));}}\n{nominal}::from_raw(output).ok_or(GlueError::InvalidDiscriminant)\n"
        ));
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
        // Project-relative unit labels: absolute paths (temporary directories, checkouts) never
        // enter the packet, so equivalent projects at different locations issue one packet.
        .map(|unit| (input.portable_unit_label(&unit.path, &unit.logical_name), unit.program.clone()))
        .collect::<Vec<_>>();
    source_units.sort_by(|a, b| a.0.cmp(&b.0));
    let compiled_source_sha256 = digest(&serde_json::to_vec(&source_units).map_err(GlueArtifactError::from)?);
    let mut bindings = Vec::new();
    let mut seen = BTreeSet::new();
    let mut adapters = BTreeSet::new();
    let fallible_imports = fallible_imports(input, items)?;
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
        let values = parameters.iter().chain(std::iter::once(&result));
        for (label, present) in [
            ("record_value_v1", values.clone().any(|p| p.record.is_some())),
            ("enum_discriminant_v1", values.clone().any(|p| p.enumeration.is_some())),
            ("call_scoped_callback_v1", values.clone().any(|p| p.callback.is_some())),
        ] {
            if present {
                adapters.insert(format!("{}:{}:{label}", library, fact.symbol));
            }
        }
        for plan in super::handles::source_handle_box_plans(input,item.key)? {
            // Record and enum transports carry their metadata in the physical type already.
            if plan.value.is_some() {
                continue;
            }
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
            logical_unit: input.portable_unit_label(&unit.path, &unit.logical_name),
            typed_unit_sha256: digest(&serde_json::to_vec(&unit.program).map_err(GlueArtifactError::from)?),
            span_start: span.start,
            span_end: span.end,
        };
        let fallible = match fact.direction {
            GlueDirection::Import => fallible_imports.contains(&item.key),
            GlueDirection::Export => reaches_fallible_import(input, item.key, &fallible_imports)?,
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
            fallible,
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
pub enum GlueError { Status(i32), Protocol, InvalidBool, InvalidChar, InvalidUtf8, Release(i32), InvalidDiscriminant, ForeignThread }
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
    emit_nominal_declarations(&bindings, &mut header, &mut rust)?;
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
                Some(("GlueOwnedView", "GlueOwnedView".to_owned()))
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
        rust.push_str(&format!("#[link(name = {:?})]\nunsafe extern \"C\" {{\n    #[link_name = {:?}]\n    pub fn binding_{index}({}) -> {};\n}}\n",binding.library,binding.symbol,rargs.join(", "),if result=="void"{"()".to_owned()}else{c_to_rust(result)?}));
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
    let scope = super::native::CheckedScopeGeometry::from_manifest(input.abi_manifest())?;
    let (native, shapes) = super::native::emit(&manifest, &scope)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(GlueArtifactError::from)?;
    let mut files = vec![GeneratedFile::new("Cargo.toml",b"[package]\nname = \"beskid_glue_client\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[lib]\npath = \"src/lib.rs\"\ncrate-type = [\"rlib\"]\n[dependencies]\nbeskid_glue = { path = \"vendor/beskid_glue\" }\nbeskid_serialization = { path = \"vendor/beskid_serialization\" }\n".to_vec()),GeneratedFile::new("src/lib.rs",format!("#![allow(non_camel_case_types, improper_ctypes)]\npub mod peer;\n{rust}").into_bytes()),GeneratedFile::new("include/beskid_glue.h",header.into_bytes()),GeneratedFile::new("glue-manifest.json",manifest_bytes),GeneratedFile::new("native/adapters.c",native),GeneratedFile::new("native/compiled-shapes.json",shapes)];
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

/// C typedefs (with layout assertions) and Rust client declarations of every record, enum and
/// callback the packet carries. Record layout is the C ABI profile binding recorded in the packet.
fn emit_nominal_declarations(
    bindings: &[GlueBindingManifest],
    header: &mut String,
    rust: &mut String,
) -> Result<(), BackendError> {
    let mut seen = BTreeSet::new();
    for binding in bindings {
        for ty in nominal_values(binding) {
            if let Some(record) = &ty.record
                && seen.insert(record.record_sha256.clone())
            {
                let name = &record.c_name;
                let mut c_fields = String::new();
                let mut rust_fields = Vec::new();
                let mut asserts = format!(
                    "_Static_assert(sizeof({name})=={}, \"Glue record size\");\n_Static_assert(_Alignof({name})=={}, \"Glue record alignment\");\n",
                    record.size, record.alignment
                );
                let mut rust_asserts = format!(
                    "assert!(core::mem::size_of::<{name}>()=={});assert!(core::mem::align_of::<{name}>()=={});",
                    record.size, record.alignment
                );
                for field in &record.fields {
                    c_fields.push_str(&format!(" {} field_{};", field.c_type, field.name));
                    asserts.push_str(&format!(
                        "_Static_assert(offsetof({name},field_{})=={}, \"Glue record offset\");\n",
                        field.name, field.offset
                    ));
                    let rust_name = rust_identifier(&field.name)?;
                    let rust_type = checked_rust_type(&field.logical)
                        .ok_or_else(|| BackendError::UnsupportedRustGlueType("record field type".into()))?;
                    rust_fields.push(format!("pub {rust_name}:{rust_type}"));
                    rust_asserts.push_str(&format!("assert!(core::mem::offset_of!({name},{rust_name})=={});", field.offset));
                }
                header.push_str(&format!("typedef struct {{{c_fields} }} {name};\n{asserts}"));
                rust.push_str(&format!(
                    "/// `{}` by value.\n#[repr(C)]\n#[derive(Debug,Clone,Copy,PartialEq)]\npub struct {name}{{{}}}\nconst _:()={{{rust_asserts}}};\n",
                    record.identity,
                    rust_fields.join(",")
                ));
            }
            if let Some(enumeration) = &ty.enumeration
                && seen.insert(enumeration.enum_sha256.clone())
            {
                let name = format!("GlueEnum_{}", enumeration.enum_sha256);
                let variants =
                    enumeration.variants.iter().map(|variant| rust_identifier(variant)).collect::<Result<Vec<_>, _>>()?;
                let from_raw = variants
                    .iter()
                    .enumerate()
                    .map(|(index, variant)| format!("{index}=>Some(Self::{variant})"))
                    .collect::<Vec<_>>()
                    .join(",");
                let into_raw = variants
                    .iter()
                    .enumerate()
                    .map(|(index, variant)| format!("Self::{variant}=>{index}"))
                    .collect::<Vec<_>>()
                    .join(",");
                rust.push_str(&format!(
                    "/// `{}` as its source-order discriminant.\n#[derive(Debug,Clone,Copy,PartialEq,Eq)]\npub enum {name}{{{}}}\nimpl {name}{{pub fn from_raw(value:u32)->Option<Self>{{match value{{{from_raw},_=>None}}}}pub fn into_raw(self)->u32{{match self{{{into_raw}}}}}}}\n",
                    enumeration.identity,
                    variants.join(",")
                ));
            }
            if let Some(callback) = &ty.callback
                && seen.insert(callback.callback_sha256.clone())
            {
                let mut c_parameters = vec!["uint64_t".to_owned()];
                let mut rust_parameters = vec!["u64".to_owned()];
                for member in &callback.parameters {
                    c_parameters.push(member.slots[0].clone());
                    rust_parameters.push(c_to_rust(&member.slots[0])?);
                }
                if let Some(result) = callback.result.slots.first() {
                    c_parameters.push(format!("{result}*"));
                    rust_parameters.push(format!("*mut {}", c_to_rust(result)?));
                }
                header.push_str(&format!("typedef int32_t (*{})({});\n", callback.c_name, c_parameters.join(",")));
                rust.push_str(&format!(
                    "pub type {} = unsafe extern \"C\" fn({})->i32;\n",
                    callback.c_name,
                    rust_parameters.join(",")
                ));
            }
        }
    }
    Ok(())
}
