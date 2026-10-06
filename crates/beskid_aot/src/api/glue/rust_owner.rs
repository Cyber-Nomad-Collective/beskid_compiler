//! Source compilation, not JSON or a caller-supplied library, issues owning-image authority.
use super::{AotResult, bounded_file, hash, hash_file, invalid, loader_dependencies};
use crate::{
    api::NativeExecutionControl,
    linker::{LinkToolInvocation, LinkToolReceipt, run_link_tool},
    runtime::{RuntimeBuildRequest, RuntimeLinkage, prepare_runtime},
};
use beskid_codegen::{
    CodegenInput,
    glue::{GlueArtifact, artifact::GluePhysicalType},
    module_emission::SyntaxModuleItem,
};
use object::{Object, ObjectKind};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

pub struct RustOwnerSource {
    pub relative_path: String,
    pub bytes: Vec<u8>,
}
pub struct RustOwnerType {
    pub brand_sha256: String,
    pub rust_type_path: String,
}
/// Exact binding identity selects the compiler-issued signature, never an arbitrary C prototype.
pub struct RustOwnerCallable {
    pub binding_identity_sha256: String,
    pub rust_callable_path: String,
    pub fallible: bool,
}
pub struct RustOwnerBuildRequest<'a> {
    pub input: &'a CodegenInput<'a>,
    pub selected: &'a [SyntaxModuleItem],
    pub native_library: &'a str,
    pub expected: &'a GlueArtifact,
    pub sources: &'a [RustOwnerSource],
    pub types: &'a [RustOwnerType],
    pub callables: &'a [RustOwnerCallable],
    pub runtime: &'a RuntimeBuildRequest,
    pub cargo: &'a Path,
    pub rustc: &'a Path,
    pub linker: &'a Path,
    pub release: bool,
    pub output_path: &'a Path,
    pub control: &'a NativeExecutionControl,
}
/// Private, linear admission. No Clone, Deserialize, or public witness constructor.
pub struct PreparedRustOwner {
    pub(super) library: String,
    pub(super) generation: u64,
    pub(super) payload_path: PathBuf,
    pub(super) payload_sha256: String,
    pub(super) provider_path: PathBuf,
    pub(super) provider_sha256: String,
    pub(super) source_sha256: [u8; 32],
    pub(super) compiler: LinkToolReceipt,
    pub(super) destructors: Vec<([u8; 32], String)>,
    pub(super) checked_exports: Vec<(String, String, u64)>,
    rustc: LinkToolReceipt,
    linker: LinkToolReceipt,
    driver_path: PathBuf,
    driver_sha256: String,
    child_receipts: Vec<beskid_execution::CompilerDriverReceipt>,
    exports: Vec<String>,
    source_inventory: Vec<(String, String)>,
}
impl PreparedRustOwner {
    /// Native library identity of this owner.
    pub fn library(&self) -> &str {
        &self.library
    }
    /// SHA-256 of the produced owner image bytes; a consumer admission record pins it.
    pub fn native_sha256(&self) -> &str {
        &self.payload_sha256
    }
    pub(super) fn verify(&self) -> AotResult<()> {
        if hash_file(&self.payload_path)? != self.payload_sha256
            || hash_file(&self.provider_path)? != self.provider_sha256
            || hash_file(&self.compiler.executable)? != self.compiler.sha256
            || hash_file(&self.rustc.executable)? != self.rustc.sha256
            || hash_file(&self.linker.executable)? != self.linker.sha256
            || hash_file(&self.driver_path)? != self.driver_sha256
            || self.child_receipts.is_empty()
            || self.source_inventory.is_empty()
            || self.exports.is_empty()
        {
            return Err(invalid("Rust owner producer closure drifted"));
        }
        Ok(())
    }
}
fn digest_bytes(text: &str) -> AotResult<[u8; 32]> {
    if text.len() != 64 || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid("invalid opaque brand encoding"));
    }
    let mut result = [0; 32];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).map_err(|_| invalid("invalid opaque brand"))?;
    }
    Ok(result)
}
fn rust_path(text: &str) -> bool {
    !text.is_empty()
        && text.split("::").all(|part| {
            let mut c = part.chars();
            matches!(c.next(),Some(c) if c=='_' || c.is_ascii_alphabetic())
                && c.all(|c| c == '_' || c.is_ascii_alphanumeric())
        })
}
fn scalar(ty: &GluePhysicalType) -> AotResult<(&'static str, &'static str)> {
    if ty.opaque.is_some() {
        return Err(invalid("opaque inputs require an explicit borrowing contract, not scalar reinterpretation"));
    }
    let result = match ty.logical.as_str() {
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
        "unit" => ("()", "()"),
        _ => {
            return Err(invalid("Rust owner scalar signature has unsupported transport"));
        }
    };
    Ok(result)
}
fn bridge_source(
    req: &RustOwnerBuildRequest<'_>,
    current: &GlueArtifact,
) -> AotResult<(String, Vec<([u8; 32], String)>, Vec<String>)> {
    let mut source = String::from(
        "#![deny(unsafe_op_in_unsafe_fn)]\nmod implementation;\nuse std::{ffi::c_void,panic::{catch_unwind,AssertUnwindSafe},sync::atomic::{AtomicU8,AtomicU64,Ordering}};\nstatic INIT:AtomicU8=AtomicU8::new(0);static LIB:AtomicU64=AtomicU64::new(0);static GEN:AtomicU64=AtomicU64::new(0);\nunsafe extern \"C\" { fn beskid_glue_v1_owner_opaque_create(library:u64,brand:*const u8,pointer:*mut c_void,destructor:unsafe extern \"C\" fn(*mut c_void)->i32,out:*mut u64)->i32; }\n#[unsafe(no_mangle)] pub extern \"C\" fn beskid_glue_rust_owner_v1_initialize(library:u64,generation:u64)->i32 { if library==0||generation==0{return 1;} if INIT.compare_exchange(0,1,Ordering::AcqRel,Ordering::Acquire).is_err(){return 2;} LIB.store(library,Ordering::Relaxed);GEN.store(generation,Ordering::Relaxed);INIT.store(2,Ordering::Release);0 }\n",
    );
    source.push_str("unsafe extern \"C\" {fn beskid_glue_v1_owner_opaque_borrow_begin(library:u64,brand:*const u8,token:u64,payload:*mut *mut c_void,lease:*mut u64)->i32;fn beskid_glue_v1_owner_opaque_borrow_end(library:u64,brand:*const u8,lease:u64)->i32;fn beskid_glue_v1_owner_opaque_release(library:u64,brand:*const u8,token:u64)->i32;} struct BorrowGuard<'a>{library:u64,brand:[u8;32],lease:u64,cleanup:&'a std::cell::Cell<i32>}impl Drop for BorrowGuard<'_>{fn drop(&mut self){let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);loop{let status=unsafe{beskid_glue_v1_owner_opaque_borrow_end(self.library,self.brand.as_ptr(),self.lease)};if status==0{break;}if status!=4||std::time::Instant::now()>=deadline{self.cleanup.set(status);break;}std::thread::yield_now();}}}\n");
    source.push_str("#[repr(C)]struct OwnedView{pointer:*const u8,length:usize,token:u64}unsafe extern \"C\" {fn beskid_glue_v1_owner_copy(library:u64,shape:u64,generation:u64,kind:u64,bytes:*const u8,length:usize,out:*mut OwnedView)->i32;fn beskid_glue_v1_owner_release_token(library:u64,token:u64)->i32;}\n");
    let mut types = BTreeMap::new();
    let mut destructors = Vec::new();
    let mut exports = vec!["beskid_glue_rust_owner_v1_initialize".into()];
    for ty in req.types {
        if !rust_path(&ty.rust_type_path)
            || !ty.rust_type_path.starts_with("implementation::")
            || types.insert(ty.brand_sha256.clone(), ty).is_some()
        {
            return Err(invalid("Rust owner type path/brand invalid or duplicate"));
        }
        let admitted = current
            .manifest
            .bindings
            .iter()
            .flat_map(|b| b.parameters.iter().chain(std::iter::once(&b.result)))
            .filter_map(|p| p.opaque.as_ref())
            .any(|p| p.brand_sha256 == ty.brand_sha256 && p.library == req.native_library);
        if !admitted {
            return Err(invalid("Rust owner brand absent from current canonical signature"));
        }
        let brand = digest_bytes(&ty.brand_sha256)?;
        let symbol = format!("beskid_glue_owner_{}_destroy", ty.brand_sha256);
        source.push_str(&format!("#[unsafe(no_mangle)] pub unsafe extern \"C\" fn {symbol}(value:*mut c_void)->i32 {{if value.is_null(){{return 1;}} match catch_unwind(AssertUnwindSafe(||unsafe{{drop(Box::from_raw(value.cast::<{}>()));}})){{Ok(())=>0,Err(_)=>7}}}}\n",ty.rust_type_path));
        destructors.push((brand, symbol.clone()));
        exports.push(symbol);
    }
    let mut ids = BTreeSet::new();
    let mut used_types = BTreeSet::new();
    for callable in req.callables {
        if !rust_path(&callable.rust_callable_path)
            || !callable.rust_callable_path.starts_with("implementation::")
            || !ids.insert(&callable.binding_identity_sha256)
        {
            return Err(invalid("Rust owner callable duplicate/invalid"));
        }
        let mut matches = current
            .manifest
            .bindings
            .iter()
            .filter(|b| b.identity_sha256 == callable.binding_identity_sha256 && b.library == req.native_library);
        let binding = matches.next().ok_or_else(|| invalid("Rust owner callable has no source binding"))?;
        if matches.next().is_some() {
            return Err(invalid("Rust owner binding ambiguous"));
        }
        let symbol = format!("beskid_glue_rust_owner_v1_{}", binding.identity_sha256);
        digest_bytes(&binding.identity_sha256)?;
        let mut args = Vec::new();
        let mut values = Vec::new();
        let mut source_parameters = Vec::new();
        let mut checks = String::new();
        for (index, param) in binding.parameters.iter().enumerate() {
            if let Some(opaque) = &param.opaque {
                if opaque.library != req.native_library || opaque.nullable {
                    return Err(invalid("borrowed opaque parameter must belong to exact nonnullable owner"));
                }
                let ty = types.get(&opaque.brand_sha256).ok_or_else(|| invalid("borrowed opaque Rust type missing"))?;
                let brand = digest_bytes(&opaque.brand_sha256)?;
                args.push(format!("a{index}:u64"));
                checks.push_str(&format!("let brand{index}:[u8;32]={brand:?};let mut payload{index}:*mut c_void=std::ptr::null_mut();let mut lease{index}=0;let status=unsafe{{beskid_glue_v1_owner_opaque_borrow_begin(LIB.load(Ordering::Acquire),brand{index}.as_ptr(),a{index},&mut payload{index},&mut lease{index})}};if status!=0{{return status;}}if lease{index}==0{{return 1;}}let _borrow{index}=BorrowGuard{{library:LIB.load(Ordering::Acquire),brand:brand{index},lease:lease{index},cleanup:&cleanup}};if payload{index}.is_null(){{return 1;}}"));
                source_parameters.push(format!("&{}", ty.rust_type_path));
                values.push(format!("unsafe{{&*payload{index}.cast::<{}>()}}", ty.rust_type_path));
                continue;
            }
            if matches!(param.logical.as_str(), "utf8" | "bytes") {
                args.extend([format!("a{index}:*const u8"), format!("n{index}:usize")]);
                checks.push_str(&format!("if n{index}>16777216||n{index}>isize::MAX as usize||(a{index}.is_null()&&n{index}!=0){{return 1;}}let slice{index}=if n{index}==0{{&[][..]}}else{{unsafe{{std::slice::from_raw_parts(a{index},n{index})}}}};"));
                source_parameters.push(if param.logical == "utf8" { "&str" } else { "&[u8]" }.to_owned());
                if param.logical == "utf8" {
                    checks
                        .push_str(&format!("let Ok(text{index})=std::str::from_utf8(slice{index})else{{return 1;}};"));
                    values.push(format!("text{index}"));
                } else {
                    values.push(format!("slice{index}"));
                }
                continue;
            }
            let (abi, logical) = scalar(param)?;
            source_parameters.push(logical.to_owned());
            if logical == "()" {
                values.push("()".into());
                continue;
            }
            args.push(format!("a{index}:{abi}"));
            if logical == "bool" {
                checks.push_str(&format!("if a{index}>1{{return 1;}}"));
                values.push(format!("a{index}!=0"));
            } else if logical == "char" {
                checks.push_str(&format!("let Some(c{index})=char::from_u32(a{index}) else{{return 1;}};"));
                values.push(format!("c{index}"));
            } else {
                values.push(format!("a{index}"));
            }
        }
        let source_result = if let Some(opaque) = &binding.result.opaque {
            types.get(&opaque.brand_sha256).ok_or_else(|| invalid("factory type missing"))?.rust_type_path.clone()
        } else if binding.result.logical == "utf8" {
            "String".into()
        } else if binding.result.logical == "bytes" {
            "Vec<u8>".into()
        } else {
            scalar(&binding.result)?.1.to_owned()
        };
        let source_result = if callable.fallible { format!("Result<{source_result},i32>") } else { source_result };
        checks.push_str(&format!(
            "let implementation_call:fn({})->{source_result}={};",
            source_parameters.join(","),
            callable.rust_callable_path
        ));
        let raw_call = format!("implementation_call({})", values.join(","));
        let call = if callable.fallible {
            format!("match {raw_call} {{Ok(value)=>value,Err(status)=>{{if status==0{{return 1;}}return status;}}}}")
        } else {
            raw_call
        };
        let body = if let Some(opaque) = &binding.result.opaque {
            if opaque.library != req.native_library || opaque.nullable {
                return Err(invalid("owning factory requires nonnullable current full brand"));
            }
            let ty = types.get(&opaque.brand_sha256).ok_or_else(|| invalid("owning factory Rust type missing"))?;
            used_types.insert(opaque.brand_sha256.clone());
            let brand = digest_bytes(&opaque.brand_sha256)?;
            args.push("out:*mut u64".into());
            let destructor = format!("beskid_glue_owner_{}_destroy", opaque.brand_sha256);
            format!(
                "if out.is_null(){{return 1;}}unsafe{{*out=0;}}{checks}let value:{}={call};let raw=Box::into_raw(Box::new(value));let brand:[u8;32]={:?};let status=unsafe{{beskid_glue_v1_owner_opaque_create(LIB.load(Ordering::Acquire),brand.as_ptr(),raw.cast(),{destructor},out)}};if status!=0{{unsafe{{drop(Box::from_raw(raw));}}}}status",
                ty.rust_type_path, brand
            )
        } else if matches!(binding.result.logical.as_str(), "utf8" | "bytes") {
            args.extend([
                "out_pointer:*mut *const u8".into(),
                "out_length:*mut usize".into(),
                "out_token:*mut u64".into(),
            ]);
            let logical = if binding.result.logical == "utf8" { "String" } else { "Vec<u8>" };
            let kind = if binding.result.logical == "utf8" { 1 } else { 2 };
            format!(
                "if out_pointer.is_null()||out_length.is_null()||out_token.is_null(){{return 1;}}unsafe{{*out_pointer=std::ptr::null();*out_length=0;*out_token=0;}}{checks}let value:{logical}={call};if value.len()>16777216{{return 1;}}let mut owned=OwnedView{{pointer:std::ptr::null(),length:0,token:0}};let status=unsafe{{beskid_glue_v1_owner_copy(LIB.load(Ordering::Acquire),{},GEN.load(Ordering::Acquire),{kind},value.as_ptr(),value.len(),&mut owned)}};if status!=0{{return status;}}if owned.token==0||owned.length!=value.len()||(owned.pointer.is_null()&&owned.length!=0){{if owned.token!=0{{unsafe{{beskid_glue_v1_owner_release_token(LIB.load(Ordering::Acquire),owned.token);}}}}return 1;}}unsafe{{*out_pointer=owned.pointer;*out_length=owned.length;*out_token=owned.token;}}0",
                binding.shape_id
            )
        } else {
            let (abi, logical) = scalar(&binding.result)?;
            if logical == "()" {
                format!("{checks}let _:()={call};0")
            } else {
                args.push(format!("out:*mut {abi}"));
                let value = if logical == "bool" {
                    "value as u8"
                } else if logical == "char" {
                    "value as u32"
                } else {
                    "value"
                };
                format!(
                    "if out.is_null(){{return 1;}}unsafe{{out.write(Default::default());}}{checks}let value:{logical}={call};unsafe{{out.write({value});}}0"
                )
            }
        };
        let cleanup_failure = if let Some(opaque) = &binding.result.opaque {
            let brand = digest_bytes(&opaque.brand_sha256)?;
            format!(
                "if !out.is_null(){{let token=unsafe{{*out}};if token!=0{{let brand:[u8;32]={brand:?};unsafe{{beskid_glue_v1_owner_opaque_release(LIB.load(Ordering::Acquire),brand.as_ptr(),token);*out=0;}}}}}}"
            )
        } else if matches!(binding.result.logical.as_str(), "utf8" | "bytes") {
            "if !out_token.is_null(){let token=unsafe{*out_token};if token!=0{unsafe{beskid_glue_v1_owner_release_token(LIB.load(Ordering::Acquire),token);*out_token=0;*out_length=0;*out_pointer=std::ptr::null();}}}".to_owned()
        } else if binding.result.logical != "unit" {
            "if !out.is_null(){unsafe{out.write(Default::default());}}".to_owned()
        } else {
            String::new()
        };
        source.push_str(&format!("#[unsafe(no_mangle)] pub unsafe extern \"C\" fn {symbol}({})->i32{{if INIT.load(Ordering::Acquire)!=2{{return 2;}}let cleanup=std::cell::Cell::new(0);let result=catch_unwind(AssertUnwindSafe(||{{{body}}}));if cleanup.get()!=0{{{cleanup_failure}return cleanup.get();}}match result{{Ok(status)=>status,Err(_)=>7}}}}\n",args.join(",")));
        exports.push(symbol);
    }
    if used_types.len() != types.len() {
        return Err(invalid("Rust owner type has no exact owning factory"));
    }
    // The admission record module is generated after the source digest (Gate WEB-GLUE03).
    source.push_str("mod owner_admission;\n");
    exports.push(super::published::OWNER_RECORD_SYMBOL.to_owned());
    Ok((source, destructors, exports))
}

/// Parse bounded Cargo protocol. Only an exact package/manifest/profile cdylib
/// inside this private target root can become a candidate; success is mandatory.
fn cargo_artifact(
    stdout: &[u8],
    manifest: &Path,
    target_root: &Path,
    release: bool,
    package_id: &str,
) -> AotResult<PathBuf> {
    if stdout.len() > 4 * 1024 * 1024 {
        return Err(invalid("Cargo output exceeds bound"));
    }
    let manifest = fs::canonicalize(manifest).map_err(|e| invalid(e.to_string()))?;
    let root = fs::canonicalize(target_root).map_err(|e| invalid(e.to_string()))?;
    let mut candidate = None;
    let mut finished = false;

    for line in stdout.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        if line.len() > 1024 * 1024 {
            return Err(invalid("Cargo message exceeds bound"));
        }
        if !line.starts_with(b"{") {
            continue;
        }
        let message: serde_json::Value =
            serde_json::from_slice(line).map_err(|e| invalid(format!("malformed Cargo JSON: {e}")))?;
        match message.get("reason").and_then(|v| v.as_str()) {
            Some("build-finished") => {
                if finished || message.get("success").and_then(|v| v.as_bool()) != Some(true) {
                    return Err(invalid("Cargo build did not finish successfully exactly once"));
                }
                finished = true;
            }
            Some("compiler-artifact") => {
                let Some(path) = message.get("manifest_path").and_then(|v| v.as_str()) else {
                    return Err(invalid("Cargo artifact manifest missing"));
                };
                let path = fs::canonicalize(path).map_err(|e| invalid(e.to_string()))?;
                if path != manifest {
                    continue;
                }
                let package = message
                    .get("package_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| invalid("Cargo package ID missing"))?;
                if package != package_id {
                    return Err(invalid("Cargo selected package identity mismatch"));
                }
                let target = message.get("target").ok_or_else(|| invalid("Cargo artifact target missing"))?;
                if target.get("name").and_then(|v| v.as_str()) != Some("beskid_rust_owner")
                    || target
                        .get("crate_types")
                        .and_then(|v| v.as_array())
                        .is_none_or(|types| types.len() != 1 || types[0].as_str() != Some("cdylib"))
                {
                    return Err(invalid("Cargo artifact selected wrong crate target"));
                }
                let profile = message.get("profile").ok_or_else(|| invalid("Cargo artifact profile missing"))?;
                let opt = profile
                    .get("opt_level")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| invalid("Cargo optimization profile missing"))?;
                if opt != if release { "3" } else { "0" }
                    || profile.get("test").and_then(|v| v.as_bool()) != Some(false)
                {
                    return Err(invalid("Cargo artifact profile mismatch"));
                }
                let files = message
                    .get("filenames")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| invalid("Cargo artifact filenames missing"))?;
                for value in files {
                    let file = value.as_str().ok_or_else(|| invalid("Cargo artifact filename invalid"))?;
                    let path = fs::canonicalize(file).map_err(|e| invalid(e.to_string()))?;
                    if !path.starts_with(&root) {
                        return Err(invalid("Cargo artifact escaped private target root"));
                    }
                    let extension = path.extension().and_then(|v| v.to_str()).unwrap_or("");
                    if !matches!(extension, "so" | "dylib" | "dll") {
                        continue;
                    }
                    if candidate.replace(path).is_some() {
                        return Err(invalid("Cargo cdylib output ambiguous"));
                    }
                }
            }
            _ => {}
        }
    }
    if !finished {
        return Err(invalid("Cargo successful build-finished absent"));
    }
    candidate.ok_or_else(|| invalid("Cargo produced no exact cdylib artifact"))
}

fn verify_dependency_inventory(payload: &Path, stage: &Path, inventory: &[(String, String)]) -> AotResult<()> {
    let depfile = payload.with_extension("d");
    let bytes = fs::read(&depfile).map_err(|_| invalid("Cargo compiler dependency evidence missing"))?;
    if bytes.len() > 1024 * 1024 {
        return Err(invalid("Rust compiler dependency evidence exceeds bound"));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| invalid("Rust compiler dependency evidence is not UTF8"))?;
    let logical = text.replace("\\\n", "");
    let line =
        logical.lines().find(|line| line.contains(": ")).ok_or_else(|| invalid("Rust dependency target missing"))?;
    let (_, deps) = line.split_once(": ").ok_or_else(|| invalid("Rust dependency target malformed"))?;
    let allowed = inventory
        .iter()
        .map(|(path, _)| fs::canonicalize(stage.join(path)).map_err(|e| invalid(e.to_string())))
        .collect::<AotResult<BTreeSet<_>>>()?;
    let mut names = Vec::new();
    let mut word = String::new();
    let mut escaped = false;
    for c in deps.chars() {
        if escaped {
            word.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c.is_whitespace() {
            if !word.is_empty() {
                names.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }
    if escaped {
        return Err(invalid("Rust dependency trailing escape"));
    }
    if !word.is_empty() {
        names.push(word);
    }
    if names.is_empty() {
        return Err(invalid("Rust compiler source dependency evidence empty"));
    }
    for name in names {
        let path = Path::new(&name);
        let path = if path.is_absolute() { path.to_owned() } else { stage.join(path) };
        let canonical = fs::canonicalize(path).map_err(|e| invalid(e.to_string()))?;
        if !allowed.contains(&canonical) {
            return Err(invalid("Rust compilation read source outside exact inventory"));
        }
    }
    Ok(())
}

fn observed_child_receipts(
    directory: &Path,
    nonce: &str,
    rustc: &Path,
    rustc_sha: &str,
    linker: &Path,
    linker_sha: &str,
    target: &str,
) -> AotResult<Vec<beskid_execution::CompilerDriverReceipt>> {
    let mut receipts = Vec::new();
    let mut bytes = 0usize;
    let mut emitting = 0;
    let mut links = 0;
    for entry in fs::read_dir(directory).map_err(|e| invalid(e.to_string()))? {
        if receipts.len() >= 1024 {
            return Err(invalid("compiler child receipt count bound"));
        }
        let entry = entry.map_err(|e| invalid(e.to_string()))?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|e| invalid(e.to_string()))?;
        if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
            return Err(invalid("compiler child receipt regular-file bound"));
        }
        let data = fs::read(entry.path()).map_err(|e| invalid(e.to_string()))?;
        bytes = bytes.checked_add(data.len()).ok_or_else(|| invalid("compiler child receipt size overflow"))?;
        if bytes > 8 * 1024 * 1024 {
            return Err(invalid("compiler child receipt aggregate bytes bound"));
        }
        let receipt: beskid_execution::CompilerDriverReceipt =
            serde_json::from_slice(&data).map_err(|e| invalid(e.to_string()))?;
        if receipt.version != 1 || receipt.nonce != nonce || !receipt.success || receipt.exit_code != Some(0) {
            return Err(invalid("compiler child receipt belongs to failed or foreign invocation"));
        }
        match receipt.phase.as_str() {
            "rustc" => {
                if receipt.executable != rustc || receipt.sha256 != rustc_sha {
                    return Err(invalid("observed Rustc executable differs"));
                }
                if receipt.arguments.windows(2).any(|a| a[0] == "--crate-name" && a[1] == "beskid_rust_owner") {
                    if !receipt.arguments.windows(2).any(|a| a[0] == "--target" && a[1] == target)
                        || !receipt.arguments.windows(2).any(|a| a[0] == "--crate-type" && a[1] == "cdylib")
                    {
                        return Err(invalid("observed Rustc target/artifact differs"));
                    }
                    emitting += 1;
                }
            }
            "linker" => {
                if receipt.executable != linker || receipt.sha256 != linker_sha {
                    return Err(invalid("observed native linker differs"));
                }
                links += 1;
            }
            _ => return Err(invalid("unknown compiler child receipt phase")),
        }
        receipts.push(receipt);
    }
    if emitting != 1 || links != 1 {
        return Err(invalid("Rust owner requires one actual emitting Rustc and one native linker invocation"));
    }
    Ok(receipts)
}

/// Variables the owner build may inherit. Everything else is removed from the child, so no
/// `CARGO_*`, `RUSTFLAGS`, `RUSTDOCFLAGS`, `RUSTC*` or `RUSTUP_TOOLCHAIN` value can reach Cargo.
/// - `PATH`: the explicit native linker (usually `cc`) locates its own `ld`/`ar` helpers through it.
/// - `HOME`, `RUSTUP_HOME`: a rustup proxy `rustc` cannot resolve its default toolchain without them;
///   `RUSTUP_TOOLCHAIN` stays removed and `rust-toolchain*` files are rejected, so they cannot select one.
/// - `TMPDIR`, `TEMP`, `TMP`: compiler and linker scratch files.
/// - `SYSTEMROOT`: required by the Windows loader and MSVC tools.
/// - `DEVELOPER_DIR`, `SDKROOT`: Apple linker SDK discovery.
const INHERITED_ALLOWLIST: &[&str] =
    &["PATH", "HOME", "RUSTUP_HOME", "TMPDIR", "TEMP", "TMP", "SYSTEMROOT", "DEVELOPER_DIR", "SDKROOT"];

fn inherited_allowed(name: &OsStr) -> bool {
    let name = name.to_string_lossy();
    INHERITED_ALLOWLIST.iter().any(|allowed| {
        if cfg!(windows) { name.eq_ignore_ascii_case(allowed) } else { name.as_ref() == *allowed }
    })
}

type EnvironmentEntry = (OsString, Option<OsString>);

/// Removal entries for every inherited variable outside the allowlist, then an empty-directory
/// `CARGO_HOME`. The linker service only supports overrides, so removal of each inherited name
/// reproduces `env_clear` plus the allowlist.
fn hermetic_environment(inherited_names: impl IntoIterator<Item = OsString>, cargo_home: &Path) -> Vec<EnvironmentEntry> {
    let mut environment: Vec<EnvironmentEntry> =
        inherited_names.into_iter().filter(|name| !inherited_allowed(name)).map(|name| (name, None)).collect();
    environment.push(("CARGO_HOME".into(), Some(cargo_home.as_os_str().to_owned())));
    environment
}

/// Cargo merges ancestor `.cargo/config*` and honors `rust-toolchain*`; neither can be disabled
/// by flags, so any such file above (or in) the staging directory fails the build before Cargo runs.
fn reject_ambient_cargo_inputs(stage: &Path) -> AotResult<()> {
    for directory in stage.ancestors() {
        for relative in [".cargo/config", ".cargo/config.toml", "rust-toolchain", "rust-toolchain.toml"] {
            let candidate = directory.join(relative);
            if fs::symlink_metadata(&candidate).is_ok() {
                return Err(invalid(format!(
                    "Rust owner build rejected: ambient Cargo input {} would affect the owner build",
                    candidate.display()
                )));
            }
        }
    }
    Ok(())
}

/// Private staging directory in the system temporary area, never inside the output directory or project.
fn private_stage(output_parent: &Path) -> AotResult<(tempfile::TempDir, PathBuf)> {
    let stage = tempfile::Builder::new()
        .prefix("beskid-rust-owner-")
        .tempdir_in(std::env::temp_dir())
        .map_err(|e| invalid(e.to_string()))?;
    let canonical = fs::canonicalize(stage.path()).map_err(|e| invalid(e.to_string()))?;
    let output_parent = fs::canonicalize(output_parent).map_err(|e| invalid(e.to_string()))?;
    if canonical.starts_with(&output_parent) {
        return Err(invalid("Rust owner staging directory must not be inside the output directory"));
    }
    reject_ambient_cargo_inputs(&canonical)?;
    Ok((stage, canonical))
}

pub fn build_rust_owner(req: &RustOwnerBuildRequest<'_>) -> AotResult<PreparedRustOwner> {
    let current =
        beskid_codegen::glue::emit(req.input, req.selected, req.native_library).map_err(|e| invalid(e.to_string()))?;
    if current != *req.expected || req.runtime.linkage != RuntimeLinkage::GlueSharedProviderV1 {
        return Err(invalid("Rust owner requires current source packet and shared provider"));
    }
    if req.sources.is_empty()
        || req.sources.len() > 1024
        || req.types.is_empty()
        || req.types.len() > 1024
        || req.callables.is_empty()
        || req.callables.len() > 1024
    {
        return Err(invalid("Rust owner source/type/callable count bound"));
    }
    let runtime = prepare_runtime(req.runtime)?;
    let provider = fs::canonicalize(
        runtime.shared_library_path.as_ref().ok_or_else(|| invalid("Rust owner shared provider missing"))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let provider_digest = hash_file(&provider)?;
    let parent = req.output_path.parent().ok_or_else(|| invalid("Rust owner output parent missing"))?;
    fs::create_dir_all(parent).map_err(|e| invalid(e.to_string()))?;
    if req.output_path.exists() {
        return Err(invalid("Rust owner output exists"));
    }
    let (stage, stage_root) = private_stage(parent)?;
    let cargo_home = stage_root.join("cargo-home");
    fs::create_dir(&cargo_home).map_err(|e| invalid(e.to_string()))?;
    let mut names = BTreeSet::new();
    let mut inventory = Vec::new();
    let mut total = 0usize;
    for source in req.sources {
        let path = Path::new(&source.relative_path);
        if source.relative_path.contains('\\')
            || path.is_absolute()
            || !source.relative_path.ends_with(".rs")
            || path.components().any(|c| !matches!(c, std::path::Component::Normal(_)))
            || !names.insert(source.relative_path.clone())
            || source.relative_path == "owner_bridge.rs"
            || source.relative_path == "owner_admission.rs"
        {
            return Err(invalid("Rust owner source inventory path invalid/duplicate"));
        }
        total = total.checked_add(source.bytes.len()).ok_or_else(|| invalid("Rust owner source size overflow"))?;
        if source.bytes.len() > 8 * 1024 * 1024
            || total > 32 * 1024 * 1024
            || std::str::from_utf8(&source.bytes).is_err()
        {
            return Err(invalid("Rust owner source bytes invalid/out of bounds"));
        }
        let output = stage.path().join(path);
        fs::create_dir_all(output.parent().unwrap()).map_err(|e| invalid(e.to_string()))?;
        fs::write(output, &source.bytes).map_err(|e| invalid(e.to_string()))?;
        inventory.push((source.relative_path.clone(), hash(&source.bytes)));
    }
    if !names.contains("implementation.rs") {
        return Err(invalid("Rust owner implementation.rs entry missing"));
    }
    let (source, destructors, exports) = bridge_source(req, &current)?;
    let manifest_bytes=b"[package]\nname = \"beskid_rust_owner\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[lib]\npath = \"owner_bridge.rs\"\ncrate-type = [\"cdylib\"]\n[workspace]\n[profile.dev]\nopt-level = 0\n[profile.release]\nopt-level = 3\n";
    let lock_bytes = b"version = 4\n[[package]]\nname = \"beskid_rust_owner\"\nversion = \"0.0.0\"\n";
    fs::write(stage.path().join("Cargo.toml"), manifest_bytes).map_err(|e| invalid(e.to_string()))?;
    fs::write(stage.path().join("Cargo.lock"), lock_bytes).map_err(|e| invalid(e.to_string()))?;
    fs::write(stage.path().join("owner_bridge.rs"), source.as_bytes()).map_err(|e| invalid(e.to_string()))?;
    inventory.extend([
        ("Cargo.toml".into(), hash(manifest_bytes)),
        ("Cargo.lock".into(), hash(lock_bytes)),
        ("owner_bridge.rs".into(), hash(source.as_bytes())),
    ]);
    inventory.sort();
    let mut framed = b"beskid.rust-owner-source/2".to_vec();
    for (path, digest) in &inventory {
        framed.extend_from_slice(&(path.len() as u64).to_le_bytes());
        framed.extend_from_slice(path.as_bytes());
        framed.extend_from_slice(&(digest.len() as u64).to_le_bytes());
        framed.extend_from_slice(digest.as_bytes());
    }
    let source_digest = digest_bytes(&hash(&framed))?;
    let rustc = fs::canonicalize(req.rustc).map_err(|e| invalid(e.to_string()))?;
    let rustc_digest = hash_file(&rustc)?;
    let linker = fs::canonicalize(req.linker).map_err(|e| invalid(e.to_string()))?;
    let linker_digest = hash_file(&linker)?;
    let driver = beskid_abi::compiler_driver::installed_compiler_driver().map_err(invalid)?;
    let cargo = fs::canonicalize(req.cargo).map_err(|e| invalid(e.to_string()))?;
    let cargo_digest = hash_file(&cargo)?;
    let checked_exports = req
        .callables
        .iter()
        .map(|callable| -> AotResult<(String, String, u64)> {
            let binding = current
                .manifest
                .bindings
                .iter()
                .find(|binding| binding.identity_sha256 == callable.binding_identity_sha256)
                .ok_or_else(|| invalid("Rust owner callable has no source binding"))?;
            Ok((
                binding.identity_sha256.clone(),
                format!("beskid_glue_rust_owner_v1_{}", binding.identity_sha256),
                binding.shape_id,
            ))
        })
        .collect::<AotResult<Vec<_>>>()?;
    let provider_identity = super::provider_identity(req.runtime.kit.target.triple.as_str(), &provider)?;
    // Immutable admission record, compiled into the owner image (Gate WEB-GLUE03). It covers
    // every build input except itself; the consumer record pins the resulting image bytes.
    let record = super::published::OwnerRecord {
        format: super::published::OwnerRecord::current_format(),
        compiler: super::published::RECORD_COMPILER.to_owned(),
        library: req.native_library.to_owned(),
        generation: req.input.typed_program().generation.0,
        provider_sha256: provider_digest.clone(),
        provider_identity: provider_identity.clone(),
        source_sha256: hash(&framed),
        source_inventory: inventory.clone(),
        cargo_sha256: cargo_digest.clone(),
        rustc_sha256: rustc_digest.clone(),
        linker_sha256: linker_digest.clone(),
        driver_sha256: driver.sha256().to_owned(),
        destructors: destructors.iter().map(|(brand, symbol)| (super::published::hex(brand), symbol.clone())).collect(),
        checked_exports: checked_exports.clone(),
        exports: exports.clone(),
    };
    let record_source = super::published::record_rust_source(
        super::published::OWNER_RECORD_SYMBOL,
        &super::published::frame_owner_record(&record)?,
    );
    fs::write(stage.path().join("owner_admission.rs"), record_source.as_bytes()).map_err(|e| invalid(e.to_string()))?;
    inventory.push(("owner_admission.rs".into(), hash(record_source.as_bytes())));
    inventory.sort();
    let receipts_path = stage.path().join("compiler-receipts");
    fs::create_dir(&receipts_path).map_err(|e| invalid(e.to_string()))?;
    let receipts_path = fs::canonicalize(receipts_path).map_err(|e| invalid(e.to_string()))?;
    let nonce =
        hash(format!("beskid.compiler-driver/1:{}:{}", stage.path().display(), hash(&source_digest)).as_bytes());
    let remaining = req.control.remaining_budget().as_millis().min(600_000) as u64;
    if remaining == 0 {
        return Err(invalid("Rust owner build deadline expired before driver admission"));
    }
    let configuration = beskid_execution::CompilerDriverConfiguration {
        version: 1,
        nonce: nonce.clone(),
        rustc: beskid_execution::CompilerDriverTool { executable: rustc.clone(), sha256: rustc_digest.clone() },
        linker: beskid_execution::CompilerDriverTool { executable: linker.clone(), sha256: linker_digest.clone() },
        receipts: receipts_path.clone(),
        remaining_millis: remaining,
        output_limit: 4 * 1024 * 1024,
    };
    let configuration_path = stage.path().join("compiler-driver.json");
    let configuration_bytes = serde_json::to_vec(&configuration).map_err(|e| invalid(e.to_string()))?;
    fs::write(&configuration_path, &configuration_bytes).map_err(|e| invalid(e.to_string()))?;
    let target = req.runtime.kit.target.triple.as_str();
    let target_root = stage.path().join("target");
    let mut invocation = LinkToolInvocation::new(req.cargo.as_os_str());
    invocation.current_dir = Some(stage.path().to_owned());
    invocation.args = vec![
        "build".into(),
        "--offline".into(),
        "--locked".into(),
        "--message-format=json".into(),
        "--manifest-path".into(),
        stage.path().join("Cargo.toml").into_os_string(),
        "--target".into(),
        target.into(),
        "--target-dir".into(),
        target_root.as_os_str().to_owned(),
        "--lib".into(),
    ];
    if req.release {
        invocation.args.push("--release".into());
    }
    let mut environment = hermetic_environment(std::env::vars_os().map(|(name, _)| name), &cargo_home);
    environment.push(("RUSTC".into(), Some(rustc.as_os_str().to_owned())));
    environment.extend([
        ("RUSTC_WRAPPER".into(), Some(driver.path().as_os_str().to_owned())),
        ("BESKID_NATIVE_COMPILER_DRIVER_CONFIG".into(), Some(configuration_path.as_os_str().to_owned())),
    ]);
    // Encoded flags preserve paths containing spaces without shell parsing.
    let mut flags = vec![
        "-C".to_owned(),
        "panic=unwind".to_owned(),
        "-C".to_owned(),
        format!("linker={}", driver.path().display()),
        "-C".to_owned(),
        format!("link-arg={}", runtime.link_path.display()),
    ];
    if target.contains("linux") {
        flags.extend(["-C".into(), "link-arg=-Wl,--no-as-needed".into()]);
    }
    if !target.contains("windows") {
        flags.extend([
            "-C".into(),
            format!(
                "link-arg=-Wl,-rpath,{}",
                provider.parent().ok_or_else(|| invalid("provider parent missing"))?.display()
            ),
        ]);
    }
    if flags.iter().any(|flag| flag.contains('\x1f')) {
        return Err(invalid("tool path has forbidden encoded flag separator"));
    }
    environment.push(("CARGO_ENCODED_RUSTFLAGS".into(), Some(flags.join("\x1f").into())));
    // The version probe and the build share one working directory and one environment.
    invocation.environment = environment.clone();
    let mut metadata_invocation = LinkToolInvocation::new(req.cargo.as_os_str());
    metadata_invocation.current_dir = Some(stage.path().to_owned());
    metadata_invocation.environment = environment;
    metadata_invocation.args = vec![
        "metadata".into(),
        "--offline".into(),
        "--locked".into(),
        "--no-deps".into(),
        "--format-version=1".into(),
        "--manifest-path".into(),
        stage.path().join("Cargo.toml").into_os_string(),
    ];
    let metadata_control = req.control.clone().with_output_limit(1024 * 1024)?;
    let (metadata_output, metadata_receipt) =
        run_link_tool(&metadata_invocation, stage.path(), Some(&metadata_control))?;
    if !metadata_output.status.success() {
        return Err(invalid("Cargo metadata failed"));
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&metadata_output.stdout).map_err(|e| invalid(e.to_string()))?;
    if metadata.get("version").and_then(|v| v.as_u64()) != Some(1) {
        return Err(invalid("Cargo metadata version mismatch"));
    }
    let canonical_manifest = fs::canonicalize(stage.path().join("Cargo.toml")).map_err(|e| invalid(e.to_string()))?;
    let packages = metadata
        .get("packages")
        .and_then(|v| v.as_array())
        .ok_or_else(|| invalid("Cargo metadata packages missing"))?;
    let mut matches = packages.iter().filter(|package| {
        package.get("manifest_path").and_then(|v| v.as_str()).and_then(|path| fs::canonicalize(path).ok()).as_ref()
            == Some(&canonical_manifest)
    });
    let package = matches.next().ok_or_else(|| invalid("Cargo metadata exact package missing"))?;
    if matches.next().is_some()
        || package.get("name").and_then(|v| v.as_str()) != Some("beskid_rust_owner")
        || package.get("version").and_then(|v| v.as_str()) != Some("0.0.0")
    {
        return Err(invalid("Cargo metadata package identity ambiguous/wrong"));
    }
    let package_id = package.get("id").and_then(|v| v.as_str()).ok_or_else(|| invalid("Cargo package ID missing"))?;
    let control = req.control.clone().with_output_limit(4 * 1024 * 1024)?;
    let (output, receipt) = run_link_tool(&invocation, stage.path(), Some(&control))?;
    if metadata_receipt.executable != receipt.executable
        || metadata_receipt.sha256 != receipt.sha256
        || receipt.sha256 != cargo_digest
    {
        return Err(invalid("Cargo executable changed between admission record, metadata and build"));
    }
    if !output.status.success() {
        return Err(invalid(format!("Rust owner Cargo build failed: {}", String::from_utf8_lossy(&output.stderr))));
    }
    if hash_file(&rustc)? != rustc_digest
        || hash_file(&provider)? != provider_digest
        || hash_file(&linker)? != linker_digest
    {
        return Err(invalid("Rust compiler/provider mutated during build"));
    }
    for (path, digest) in &inventory {
        if hash_file(&stage.path().join(path))? != *digest {
            return Err(invalid("Rust owner source mutated during compilation"));
        }
    }
    driver.verify().map_err(invalid)?;
    if fs::read(&configuration_path).map_err(|e| invalid(e.to_string()))? != configuration_bytes {
        return Err(invalid("Rust owner compiler driver configuration changed"));
    }
    let child_receipts =
        observed_child_receipts(&receipts_path, &nonce, &rustc, &rustc_digest, &linker, &linker_digest, target)?;
    let staged_payload = cargo_artifact(
        &output.stdout,
        &stage.path().join("Cargo.toml"),
        &target_root.join(target).join(if req.release { "release" } else { "debug" }),
        req.release,
        package_id,
    )?;
    verify_dependency_inventory(&staged_payload, stage.path(), &inventory)?;
    let bytes = bounded_file(&staged_payload)?;
    let image = object::File::parse(bytes.as_slice()).map_err(|e| invalid(e.to_string()))?;
    let architecture = if target.starts_with("aarch64-") {
        object::Architecture::Aarch64
    } else if target.starts_with("x86_64-") {
        object::Architecture::X86_64
    } else {
        return Err(invalid("Rust owner target unsupported"));
    };
    let format = if target.contains("apple") {
        object::BinaryFormat::MachO
    } else if target.contains("windows") {
        object::BinaryFormat::Pe
    } else {
        object::BinaryFormat::Elf
    };
    if image.format() != format {
        return Err(invalid("Rust owner object format/target mismatch"));
    }
    if image.kind() != ObjectKind::Dynamic
        || !image.is_64()
        || !image.is_little_endian()
        || image.architecture() != architecture
    {
        return Err(invalid("Rust owner native image representation/target mismatch"));
    }
    let native_exports = image.exports().map_err(|e| invalid(e.to_string()))?;
    for symbol in &exports {
        let name = if target.contains("apple") { format!("_{symbol}") } else { symbol.clone() };
        if !native_exports.iter().any(|e| e.name() == name.as_bytes()) {
            return Err(invalid(format!("Rust owner exact export absent: {symbol}")));
        }
    }
    let dependencies = loader_dependencies(&image)?;
    if dependencies.iter().filter(|d| **d == provider_identity).count() != 1
        || dependencies.iter().any(|d| d != &provider_identity && d.contains("beskid_runtime"))
    {
        return Err(invalid("Rust owner lacks exact sole canonical provider dependency"));
    }
    let embedded: super::published::OwnerRecord =
        super::published::read_record(&image, super::published::OWNER_RECORD_SYMBOL)?;
    if embedded != record {
        return Err(invalid("Rust owner image admission record differs from the issued owner record"));
    }
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| invalid(e.to_string()))?;
    std::io::Write::write_all(&mut temporary, &bytes).map_err(|e| invalid(e.to_string()))?;
    temporary.as_file().sync_all().map_err(|e| invalid(e.to_string()))?;
    temporary.persist_noclobber(req.output_path).map_err(|e| invalid(e.to_string()))?;
    let result = PreparedRustOwner {
        library: req.native_library.into(),
        generation: req.input.typed_program().generation.0,
        payload_path: fs::canonicalize(req.output_path).map_err(|e| invalid(e.to_string()))?,
        payload_sha256: hash(&bytes),
        provider_path: provider,
        provider_sha256: provider_digest,
        source_sha256: source_digest,
        compiler: receipt,
        driver_path: driver.path().to_owned(),
        driver_sha256: driver.sha256().to_owned(),
        child_receipts,
        rustc: LinkToolReceipt { executable: rustc, sha256: rustc_digest },
        linker: LinkToolReceipt { executable: linker, sha256: linker_digest },
        destructors,
        checked_exports,
        exports,
        source_inventory: inventory,
    };
    result.verify()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("Cargo.toml");
        fs::write(&manifest, b"[package]").unwrap();
        let root = temp.path().join("target/x86_64-unknown-linux-gnu/debug");
        fs::create_dir_all(&root).unwrap();
        let payload = root.join("libbeskid_rust_owner.so");
        fs::write(&payload, b"fixture only selector input").unwrap();
        (temp, manifest, root, payload)
    }
    fn artifact(manifest: &Path, payload: &Path) -> serde_json::Value {
        serde_json::json!({"reason":"compiler-artifact","package_id":"exact-package","manifest_path":manifest,"target":{"name":"beskid_rust_owner","crate_types":["cdylib"]},"profile":{"opt_level":"0","test":false},"filenames":[payload]})
    }
    fn stream(messages: &[serde_json::Value]) -> Vec<u8> {
        messages.iter().map(|message| format!("{message}\n")).collect::<String>().into_bytes()
    }
    #[test]
    fn rust_owner_cargo_selection_requires_exact_successful_unique_artifact() {
        let (_temp, manifest, root, payload) = setup();
        let message = artifact(&manifest, &payload);
        let success = serde_json::json!({"reason":"build-finished","success":true});
        assert_eq!(
            cargo_artifact(&stream(&[message.clone(), success.clone()]), &manifest, &root, false, "exact-package")
                .unwrap(),
            payload.canonicalize().unwrap()
        );
        assert!(cargo_artifact(&stream(&[message.clone()]), &manifest, &root, false, "exact-package").is_err());
        assert!(
            cargo_artifact(&stream(&[message.clone(), success.clone()]), &manifest, &root, false, "wrong-package")
                .is_err()
        );
        assert!(
            cargo_artifact(
                &stream(&[message.clone(), message.clone(), success]),
                &manifest,
                &root,
                false,
                "exact-package"
            )
            .is_err()
        );
        assert!(
            cargo_artifact(
                &stream(&[message, serde_json::json!({"reason":"build-finished","success":false})]),
                &manifest,
                &root,
                false,
                "exact-package"
            )
            .is_err()
        );
    }
    #[test]
    fn rust_owner_dependency_inventory_rejects_external_source() {
        let (temp, _manifest, _root, payload) = setup();
        let source = temp.path().join("implementation.rs");
        fs::write(&source, b"pub struct T;").unwrap();
        let outside = temp.path().join("foreign.rs");
        fs::write(&outside, b"pub struct Foreign;").unwrap();
        fs::write(payload.with_extension("d"), format!("{}: {}\n", payload.display(), source.display())).unwrap();
        let inventory = vec![("implementation.rs".into(), hash(b"pub struct T;"))];
        assert!(verify_dependency_inventory(&payload, temp.path(), &inventory).is_ok());
        fs::write(
            payload.with_extension("d"),
            format!("{}: {} {}\n", payload.display(), source.display(), outside.display()),
        )
        .unwrap();
        assert!(verify_dependency_inventory(&payload, temp.path(), &inventory).is_err());
    }
    fn inherited_names() -> Vec<OsString> {
        [
            "PATH",
            "HOME",
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
            "RUSTC",
            "RUSTC_WRAPPER",
            "RUSTUP_TOOLCHAIN",
            "CARGO_TARGET_DIR",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_HOME",
            "CARGO_ENCODED_RUSTFLAGS",
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }
    #[test]
    fn rust_owner_environment_removes_inherited_cargo_and_rustc_settings() {
        let home = Path::new("/stage/cargo-home");
        let environment = hermetic_environment(inherited_names(), home);
        for name in
            ["RUSTFLAGS", "RUSTDOCFLAGS", "RUSTC", "RUSTC_WRAPPER", "RUSTUP_TOOLCHAIN", "CARGO_TARGET_DIR", "CARGO_BUILD_RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"]
        {
            assert!(environment.iter().any(|(key, value)| key == name && value.is_none()), "{name} not removed");
        }
        for name in ["PATH", "HOME"] {
            assert!(!environment.iter().any(|(key, _)| key == name), "{name} must stay inherited");
        }
        let cargo_home: Vec<_> = environment.iter().filter(|(key, _)| key == "CARGO_HOME").collect();
        assert_eq!(cargo_home.last().unwrap().1.as_deref(), Some(home.as_os_str()));
    }
    #[cfg(unix)]
    #[test]
    fn rust_owner_environment_does_not_reach_child_process() {
        let temp = tempfile::tempdir().unwrap();
        let cargo_home = temp.path().join("cargo-home");
        let mut command = std::process::Command::new("/usr/bin/env");
        command.env("RUSTFLAGS", "-Cinherited").env("CARGO_TARGET_DIR", "/elsewhere").env("RUSTUP_TOOLCHAIN", "nightly");
        let names = inherited_names().into_iter().chain(std::env::vars_os().map(|(name, _)| name));
        for (key, value) in hermetic_environment(names, &cargo_home) {
            match value {
                Some(value) => command.env(key, value),
                None => command.env_remove(key),
            };
        }
        let output = command.output().unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        let variables: Vec<&str> = text.lines().filter_map(|line| line.split_once('=').map(|(k, _)| k)).collect();
        for name in &variables {
            assert!(
                inherited_allowed(OsStr::new(name)) || *name == "CARGO_HOME",
                "unexpected variable reached child: {name}"
            );
        }
        assert!(text.lines().any(|line| line == format!("CARGO_HOME={}", cargo_home.display())));
    }
    #[test]
    fn rust_owner_rejects_ancestor_cargo_configuration() {
        for file in [".cargo/config.toml", ".cargo/config", "rust-toolchain.toml", "rust-toolchain"] {
            let temp = tempfile::tempdir().unwrap();
            let stage = temp.path().join("a/b/stage");
            fs::create_dir_all(&stage).unwrap();
            assert!(reject_ambient_cargo_inputs(&stage).is_ok());
            let marker = temp.path().join("a").join(file);
            fs::create_dir_all(marker.parent().unwrap()).unwrap();
            fs::write(&marker, b"").unwrap();
            let error = reject_ambient_cargo_inputs(&stage).unwrap_err().to_string();
            assert!(error.contains(&marker.display().to_string()), "{error}");
        }
    }
    #[test]
    fn rust_owner_stage_is_private_and_outside_output_directory() {
        let output = tempfile::tempdir().unwrap();
        let (stage, canonical) = private_stage(output.path()).unwrap();
        assert!(canonical.starts_with(fs::canonicalize(std::env::temp_dir()).unwrap()));
        assert!(!canonical.starts_with(fs::canonicalize(output.path()).unwrap()));
        drop(stage);
    }
    #[test]
    fn rust_owner_brands_do_not_accept_unbounded_or_nonhex_input() {
        assert!(digest_bytes(&"z".repeat(64)).is_err());
        assert!(digest_bytes(&"é".repeat(32)).is_err());
        assert!(digest_bytes(&"a".repeat(64)).is_ok());
    }
}
