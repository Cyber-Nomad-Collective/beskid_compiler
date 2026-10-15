//! Rust owner shell support for by-value records, fieldless enums, call-scoped callbacks and the
//! generated `beskid` consumer module of the consumer's exports (v0.6.1 Glue extensions).
//!
//! Records and enums use the owner type that `RustOwner(Path:..)` names, checked at compile time
//! against the C ABI profile layout (records) or by exhaustive matches over the Beskid variant
//! names (enums). A record or enum that only an export uses and that names no owner type gets a
//! `repr(C)` mirror type in the `beskid` module.
use super::{AotResult, invalid};
use beskid_codegen::glue::{
    GlueArtifact,
    artifact::{GlueBindingManifest, GlueEnumType, GluePhysicalType, GlueRecordType, owned_release_symbol},
    rust_identifier,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Owner-shell prelude shared by every generated callable: the callback failure payload and the
/// per-thread import depth that admits calls into Beskid exports.
pub(super) const OWNER_PRELUDE: &str = "struct GlueCallbackFailure(i32);\nthread_local!{static IMPORT_DEPTH:std::cell::Cell<u32>=const{std::cell::Cell::new(0)};}\nstruct ImportDepth;impl ImportDepth{fn enter()->Self{IMPORT_DEPTH.with(|depth|depth.set(depth.get()+1));ImportDepth}}impl Drop for ImportDepth{fn drop(&mut self){IMPORT_DEPTH.with(|depth|depth.set(depth.get().saturating_sub(1)));}}\n#[allow(dead_code)] fn inside_import()->bool{IMPORT_DEPTH.with(|depth|depth.get()>0)}\nfn glue_failure_status(payload:Box<dyn std::any::Any+Send>)->i32{payload.downcast_ref::<GlueCallbackFailure>().map_or(7,|failure|if failure.0==0{1}else{failure.0})}\n";

/// Rust types of every record and enum of the packet, with their generated converters.
pub(super) struct OwnerValueTypes {
    /// Identity digest to the Rust type path used in the owner crate.
    types: BTreeMap<String, String>,
    /// Identity digest to whether the type is an owner type (`RustOwner`) or a mirror.
    owned: BTreeMap<String, bool>,
    /// Crate-root items: layout assertions and enum converters.
    pub(super) root: String,
    /// `beskid` module items: mirror types.
    pub(super) mirrors: String,
}

fn rust_scalar(logical: &str) -> Option<(&'static str, &'static str)> {
    Some(match logical {
        "i8" => ("i8", "i8"),
        "i16" => ("i16", "i16"),
        "i32" => ("i32", "i32"),
        "i64" => ("i64", "i64"),
        "u8" => ("u8", "u8"),
        "u16" => ("u16", "u16"),
        "u32" => ("u32", "u32"),
        "u64" => ("u64", "u64"),
        "f32" => ("f32", "f32"),
        "f64" => ("f64", "f64"),
        "bool" => ("u8", "bool"),
        "char" => ("u32", "char"),
        "word" => ("usize", "usize"),
        _ => return None,
    })
}

fn owner_path(path: &str) -> AotResult<String> {
    if !path.starts_with("implementation::")
        || !path.split("::").all(|segment| {
            let mut chars = segment.chars();
            matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
                && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
        })
    {
        return Err(invalid(format!("Rust owner value type path `{path}` is invalid")));
    }
    Ok(format!("crate::{path}"))
}

impl OwnerValueTypes {
    pub(super) fn new(current: &GlueArtifact) -> AotResult<Self> {
        let mut values =
            Self { types: BTreeMap::new(), owned: BTreeMap::new(), root: String::new(), mirrors: String::new() };
        for binding in &current.manifest.bindings {
            for physical in beskid_codegen::glue::artifact::nominal_values(binding) {
                if let Some(record) = &physical.record {
                    values.record(record)?;
                }
                if let Some(enumeration) = &physical.enumeration {
                    values.enumeration(enumeration)?;
                }
            }
        }
        Ok(values)
    }

    fn record(&mut self, record: &GlueRecordType) -> AotResult<()> {
        if self.types.contains_key(&record.record_sha256) {
            return Ok(());
        }
        let (ty, owned) = match &record.rust_owner {
            Some(path) => (owner_path(path)?, true),
            None => (format!("crate::beskid::{}", record.c_name), false),
        };
        let mut fields = Vec::new();
        let mut asserts = format!(
            "assert!(core::mem::size_of::<{ty}>()=={});assert!(core::mem::align_of::<{ty}>()=={});",
            record.size, record.alignment
        );
        let mut types = String::new();
        for field in &record.fields {
            let name = rust_identifier(&field.name).map_err(|error| invalid(error.to_string()))?;
            let (_, rust) = rust_scalar(&field.logical).ok_or_else(|| invalid("record field transport"))?;
            fields.push(format!("pub {name}:{rust}"));
            asserts.push_str(&format!("assert!(core::mem::offset_of!({ty},{name})=={});", field.offset));
            types.push_str(&format!("let _:{rust}=value.{name};"));
        }
        if !owned {
            self.mirrors.push_str(&format!(
                "/// `{}` by value.\n#[repr(C)]\n#[derive(Debug,Clone,Copy,PartialEq)]\npub struct {}{{{}}}\n",
                record.identity,
                record.c_name,
                fields.join(",")
            ));
        }
        // The owner type must have the profile layout, the same field names and the same field
        // types; any difference fails the owner build.
        self.root.push_str(&format!(
            "const _:()={{{asserts}}};\n#[allow(dead_code)] fn glue_record_{}_fields(value:&{ty}){{{types}}}\n",
            record.record_sha256
        ));
        self.types.insert(record.record_sha256.clone(), ty);
        self.owned.insert(record.record_sha256.clone(), owned);
        Ok(())
    }

    fn enumeration(&mut self, enumeration: &GlueEnumType) -> AotResult<()> {
        if self.types.contains_key(&enumeration.enum_sha256) {
            return Ok(());
        }
        let (ty, owned) = match &enumeration.rust_owner {
            Some(path) => (owner_path(path)?, true),
            None => (format!("crate::beskid::GlueEnum_{}", enumeration.enum_sha256), false),
        };
        let variants = enumeration
            .variants
            .iter()
            .map(|variant| rust_identifier(variant).map_err(|error| invalid(error.to_string())))
            .collect::<AotResult<Vec<_>>>()?;
        if !owned {
            self.mirrors.push_str(&format!(
                "/// `{}` as its source-order discriminant.\n#[derive(Debug,Clone,Copy,PartialEq,Eq)]\npub enum GlueEnum_{}{{{}}}\n",
                enumeration.identity,
                enumeration.enum_sha256,
                variants.join(",")
            ));
        }
        let from_raw = variants
            .iter()
            .enumerate()
            .map(|(index, variant)| format!("{index}=>Some({ty}::{variant})"))
            .collect::<Vec<_>>()
            .join(",");
        // Exhaustive: an owner enum with other variant names fails to compile.
        let into_raw = variants
            .iter()
            .enumerate()
            .map(|(index, variant)| format!("{ty}::{variant}=>{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let sha = &enumeration.enum_sha256;
        self.root.push_str(&format!(
            "#[allow(dead_code)] fn glue_enum_{sha}_from_raw(value:u32)->Option<{ty}>{{match value{{{from_raw},_=>None}}}}\n#[allow(dead_code)] fn glue_enum_{sha}_into_raw(value:{ty})->u32{{match value{{{into_raw}}}}}\n"
        ));
        self.types.insert(sha.clone(), ty);
        self.owned.insert(sha.clone(), owned);
        Ok(())
    }

    /// Rust type of a record or enum physical type. An import requires an owner type.
    pub(super) fn type_of(&self, physical: &GluePhysicalType, import: bool) -> AotResult<Option<(String, String)>> {
        let (sha, marker) = match (&physical.record, &physical.enumeration) {
            (Some(record), _) => (&record.record_sha256, "GlueRecord"),
            (None, Some(enumeration)) => (&enumeration.enum_sha256, "GlueEnum"),
            (None, None) => return Ok(None),
        };
        let ty = self.types.get(sha).ok_or_else(|| invalid("record/enum type absent from the owner packet"))?;
        if import && !self.owned.get(sha).copied().unwrap_or(false) {
            return Err(invalid(format!(
                "{marker} `{}` used by a Rust owner import requires RustOwner(Path:..)",
                physical.logical
            )));
        }
        Ok(Some((sha.clone(), ty.clone())))
    }
}

/// One owner-side callback parameter: the raw C slots, the closure the implementation receives
/// and its `&mut dyn FnMut` type.
pub(super) struct OwnerCallback {
    pub(super) arguments: Vec<String>,
    pub(super) setup: String,
    pub(super) parameter_type: String,
    pub(super) value: String,
}

pub(super) fn owner_callback(
    values: &OwnerValueTypes,
    index: usize,
    physical: &GluePhysicalType,
) -> AotResult<Option<OwnerCallback>> {
    let Some(callback) = &physical.callback else { return Ok(None) };
    let mut raw = vec!["u64".to_owned()];
    let mut closure_parameters = Vec::new();
    let mut closure_types = Vec::new();
    let mut converted = vec![format!("c{index}")];
    for (position, member) in callback.parameters.iter().enumerate() {
        let name = format!("m{position}");
        if let Some((sha, ty)) = values.type_of(member, true)? {
            raw.push("u32".into());
            closure_parameters.push(format!("{name}:{ty}"));
            closure_types.push(ty);
            converted.push(format!("glue_enum_{sha}_into_raw({name})"));
            continue;
        }
        let (abi, logical) = rust_scalar(&member.logical).ok_or_else(|| invalid("callback member transport"))?;
        raw.push(abi.into());
        closure_parameters.push(format!("{name}:{logical}"));
        closure_types.push(logical.into());
        converted.push(match member.logical.as_str() {
            "bool" => format!("u8::from({name})"),
            "char" => format!("{name} as u32"),
            _ => name,
        });
    }
    let fail = "std::panic::resume_unwind(Box::new(GlueCallbackFailure(1)))";
    let (result_type, out, finish) = if callback.result.logical == "unit" {
        ("()".to_owned(), None, "()".to_owned())
    } else if let Some((sha, ty)) = values.type_of(&callback.result, true)? {
        (ty, Some("u32".to_owned()), format!("glue_enum_{sha}_from_raw(out).unwrap_or_else(||{fail})"))
    } else {
        let (abi, logical) = rust_scalar(&callback.result.logical).ok_or_else(|| invalid("callback result transport"))?;
        let finish = match callback.result.logical.as_str() {
            "bool" => format!("match out{{0=>false,1=>true,_=>{fail}}}"),
            "char" => format!("char::from_u32(out).unwrap_or_else(||{fail})"),
            _ => "out".to_owned(),
        };
        (logical.to_owned(), Some(abi.to_owned()), finish)
    };
    if let Some(out) = &out {
        raw.push(format!("*mut {out}"));
        converted.push("&mut out".into());
    }
    let declare_out = out.as_ref().map(|out| format!("let mut out:{out}=0 as {out};")).unwrap_or_default();
    let setup = format!(
        "let mut cb{index}=|{}|->{result_type}{{{declare_out}let status=unsafe{{a{index}({})}};if status!=0{{std::panic::resume_unwind(Box::new(GlueCallbackFailure(status)));}}{finish}}};",
        closure_parameters.join(","),
        converted.join(",")
    );
    Ok(Some(OwnerCallback {
        arguments: vec![format!("a{index}:unsafe extern \"C\" fn({})->i32", raw.join(",")), format!("c{index}:u64")],
        setup,
        parameter_type: format!("&mut dyn FnMut({})->{result_type}", closure_types.join(",")),
        value: format!("&mut cb{index}"),
    }))
}

/// Identity of a consumer owned-view release row in the export table.
pub(super) fn release_row_identity(library: &str) -> [u8; 32] {
    Sha256::digest(owned_release_symbol(library).as_bytes()).into()
}

fn identity_bytes(text: &str) -> AotResult<[u8; 32]> {
    let mut bytes = [0u8; 32];
    if text.len() != 64 {
        return Err(invalid("export identity width"));
    }
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).map_err(|_| invalid("export identity"))?;
    }
    Ok(bytes)
}

/// The rows the loader binds into the owner image: every export's checked invocation, then one
/// owned-view release per library whose exports return views. Both sides derive the same rows
/// from the same packet.
pub(super) fn export_rows(manifest: &beskid_codegen::glue::GlueManifest) -> AotResult<Vec<([u8; 32], String)>> {
    let exports = manifest.bindings.iter().filter(|binding| binding.direction == "export").collect::<Vec<_>>();
    let mut rows = Vec::new();
    for binding in &exports {
        rows.push((
            identity_bytes(&binding.identity_sha256)?,
            beskid_codegen::glue::artifact::checked_invocation_symbol(binding),
        ));
    }
    let mut libraries = exports
        .iter()
        .filter(|binding| matches!(binding.result.logical.as_str(), "utf8" | "bytes"))
        .map(|binding| binding.library.clone())
        .collect::<Vec<_>>();
    libraries.sort();
    libraries.dedup();
    for library in libraries {
        rows.push((release_row_identity(&library), owned_release_symbol(&library)));
    }
    Ok(rows)
}

fn export_function(values: &OwnerValueTypes, index: usize, binding: &GlueBindingManifest, rows: &[([u8; 32], String)]) -> AotResult<String> {
    let name = rust_identifier(&binding.symbol).map_err(|error| invalid(error.to_string()))?;
    let mut parameters = Vec::new();
    let mut raw = Vec::new();
    let mut arguments = Vec::new();
    for (position, parameter) in binding.parameters.iter().enumerate() {
        let value = format!("a{position}");
        if parameter.opaque.is_some() {
            parameters.push(format!("{value}:u64"));
            raw.push("u64".to_owned());
            arguments.push(value);
        } else if let Some((sha, ty)) = values.type_of(parameter, false)? {
            parameters.push(format!("{value}:{ty}"));
            if parameter.enumeration.is_some() {
                raw.push("u32".into());
                arguments.push(format!("crate::glue_enum_{sha}_into_raw({value})"));
            } else {
                raw.push(ty);
                arguments.push(value);
            }
        } else if matches!(parameter.logical.as_str(), "utf8" | "bytes") {
            parameters.push(format!("{value}:{}", if parameter.logical == "utf8" { "&str" } else { "&[u8]" }));
            raw.extend(["*const u8".to_owned(), "usize".to_owned()]);
            arguments.extend([format!("{value}.as_ptr()"), format!("{value}.len()")]);
        } else {
            let (abi, logical) =
                rust_scalar(&parameter.logical).ok_or_else(|| invalid("export parameter transport"))?;
            parameters.push(format!("{value}:{logical}"));
            raw.push(abi.into());
            arguments.push(match parameter.logical.as_str() {
                "bool" => format!("u8::from({value})"),
                "char" => format!("{value} as u32"),
                _ => value,
            });
        }
    }
    let result = &binding.result;
    let (result_type, out_type, out_init, finish) = if result.logical == "unit" {
        ("()".to_owned(), None, String::new(), "Ok(())".to_owned())
    } else if result.opaque.is_some() {
        ("u64".to_owned(), Some("u64".to_owned()), "0".to_owned(), "Ok(out)".to_owned())
    } else if let Some((sha, ty)) = values.type_of(result, false)? {
        if result.enumeration.is_some() {
            (
                ty,
                Some("u32".to_owned()),
                "0".to_owned(),
                format!("crate::glue_enum_{sha}_from_raw(out).ok_or(GlueError::InvalidDiscriminant)"),
            )
        } else {
            (ty.clone(), Some(ty), "unsafe{core::mem::zeroed()}".to_owned(), "Ok(out)".to_owned())
        }
    } else if matches!(result.logical.as_str(), "utf8" | "bytes") {
        let release = rows
            .iter()
            .position(|(identity, _)| *identity == release_row_identity(&binding.library))
            .ok_or_else(|| invalid("export view result has no release row"))?;
        let convert = if result.logical == "utf8" {
            "String::from_utf8(copied).map_err(|_|GlueError::InvalidUtf8)"
        } else {
            "Ok(copied)"
        };
        (
            if result.logical == "utf8" { "String" } else { "Vec<u8>" }.to_owned(),
            Some("OwnedView".to_owned()),
            "OwnedView{pointer:core::ptr::null(),length:0,token:0}".to_owned(),
            format!(
                "let release:unsafe extern \"C\" fn(u64)->i32=unsafe{{core::mem::transmute::<usize,unsafe extern \"C\" fn(u64)->i32>(address({release})? as usize)}};let copied=if out.token==0||out.length>16777216||(out.pointer.is_null()&&out.length!=0){{None}}else if out.length==0{{Some(Vec::new())}}else{{Some(unsafe{{core::slice::from_raw_parts(out.pointer,out.length)}}.to_vec())}};let released=if out.token==0{{0}}else{{unsafe{{release(out.token)}}}};if released!=0{{return Err(GlueError::Release(released));}}let copied=copied.ok_or(GlueError::Protocol)?;{convert}"
            ),
        )
    } else {
        let (abi, logical) = rust_scalar(&result.logical).ok_or_else(|| invalid("export result transport"))?;
        let finish = match result.logical.as_str() {
            "bool" => "match out{0=>Ok(false),1=>Ok(true),_=>Err(GlueError::InvalidBool)}".to_owned(),
            "char" => "char::from_u32(out).ok_or(GlueError::InvalidChar)".to_owned(),
            _ => "Ok(out)".to_owned(),
        };
        (logical.to_owned(), Some(abi.to_owned()), format!("0 as {abi}"), finish)
    };
    if let Some(out) = &out_type {
        raw.push(format!("*mut {out}"));
        arguments.push("&mut out".into());
    }
    let declare_out = out_type.as_ref().map(|out| format!("let mut out:{out}={out_init};")).unwrap_or_default();
    Ok(format!(
        "/// Calls the Beskid export `{}` of the admitted consumer image.\npub fn {name}({})->Result<{result_type},GlueError>{{let function:unsafe extern \"C\" fn({})->i32=unsafe{{core::mem::transmute::<usize,unsafe extern \"C\" fn({})->i32>(address({index})? as usize)}};{declare_out}let status=unsafe{{function({})}};if status!=0{{return Err(GlueError::Status(status));}}{finish}}}\n",
        binding.symbol,
        parameters.join(","),
        raw.join(","),
        raw.join(","),
        arguments.join(",")
    ))
}

/// The generated `beskid` module of the owner crate and its export-table binding entry.
pub(super) fn beskid_module(values: &OwnerValueTypes, current: &GlueArtifact) -> AotResult<String> {
    let rows = export_rows(&current.manifest)?;
    let count = rows.len();
    let identities = rows.iter().map(|(identity, _)| format!("{identity:?}")).collect::<Vec<_>>().join(",");
    let mut module = format!(
        "/// Generated consumer of the Beskid exports of this owner's consumer image. A call is admitted\n/// only on a thread inside a Beskid-to-owner import call.\n#[allow(dead_code, non_camel_case_types, improper_ctypes_definitions)]\npub mod beskid {{\nuse std::sync::atomic::{{AtomicU8,AtomicU64,Ordering}};\n#[derive(Debug,Clone,Copy,PartialEq,Eq)]\npub enum GlueError {{ Status(i32), Protocol, InvalidBool, InvalidChar, InvalidUtf8, Release(i32), InvalidDiscriminant, ForeignThread }}\n#[repr(C)] pub struct ExportRowV1 {{ identity:[u8;32], address:u64 }}\n#[repr(C)] pub struct ExportTableV1 {{ version:u32, size:u32, count:usize, rows:*const ExportRowV1 }}\n#[repr(C)] struct OwnedView {{ pointer:*const u8, length:usize, token:u64 }}\nconst ROWS:usize={count};\nconst IDENTITIES:[[u8;32];ROWS]=[{identities}];\nstatic ADDRESSES:[AtomicU64;ROWS]=[const{{AtomicU64::new(0)}};ROWS];\nstatic BOUND:AtomicU8=AtomicU8::new(0);\nfn address(index:usize)->Result<u64,GlueError>{{if !crate::inside_import(){{return Err(GlueError::ForeignThread);}}if BOUND.load(Ordering::Acquire)!=1{{return Err(GlueError::Status(2));}}let address=ADDRESSES[index].load(Ordering::Acquire);if address==0{{Err(GlueError::Status(2))}}else{{Ok(address)}}}}\npub(crate) fn bind(table:*const ExportTableV1)->i32{{if table.is_null(){{return 1;}}let table=unsafe{{&*table}};if table.version!=1||table.size as usize!=core::mem::size_of::<ExportTableV1>()||table.count!=ROWS||(ROWS!=0&&table.rows.is_null()){{return 1;}}if BOUND.compare_exchange(0,2,Ordering::AcqRel,Ordering::Acquire).is_err(){{return 4;}}let rows=if ROWS==0{{&[][..]}}else{{unsafe{{core::slice::from_raw_parts(table.rows,ROWS)}}}};if rows.iter().zip(IDENTITIES.iter()).any(|(row,identity)|row.identity!=*identity||row.address==0){{BOUND.store(0,Ordering::Release);return 1;}}for (slot,row) in ADDRESSES.iter().zip(rows){{slot.store(row.address,Ordering::Release);}}BOUND.store(1,Ordering::Release);0}}\n",
    );
    module.push_str(&values.mirrors);
    for (index, binding) in current.manifest.bindings.iter().filter(|binding| binding.direction == "export").enumerate() {
        module.push_str(&export_function(values, index, binding, &rows)?);
    }
    module.push_str("}\n#[unsafe(no_mangle)] pub unsafe extern \"C\" fn beskid_glue_rust_owner_v1_bind_exports(table:*const beskid::ExportTableV1)->i32 { match catch_unwind(AssertUnwindSafe(||beskid::bind(table))) { Ok(status)=>status, Err(_)=>7 } }\n");
    Ok(module)
}

/// Exported owner symbol that receives the consumer export table.
pub(super) const OWNER_BIND_EXPORTS_SYMBOL: &str = "beskid_glue_rust_owner_v1_bind_exports";
