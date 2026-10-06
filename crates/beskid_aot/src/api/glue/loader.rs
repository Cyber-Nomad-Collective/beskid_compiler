//! Private producer admission into one retained canonical runtime process.
use super::{AotResult, IssuedGlueImage, PreparedGlueArtifact, hash_file, invalid};
use beskid_artifacts::{native_host::NativeHostLease, native_image::PinnedNativeImages};
use std::{
    collections::BTreeSet,
    ffi::{CStr, c_void},
    panic::{AssertUnwindSafe, catch_unwind},
};

type OwnerOpen = unsafe extern "C" fn(u64, *mut u64) -> i32;
type OwnerBind = unsafe extern "C" fn(u64, u64, *const u64, usize) -> i32;
type OwnerClose = unsafe extern "C" fn(u64, u64) -> i32;
type Initialize = unsafe extern "C" fn(*const AdmissionTable) -> i32;
type Admit = unsafe extern "C" fn(*mut c_void, *const u8, *const u64, usize, *mut u64) -> i32;
type DescriptorGetter = unsafe extern "C" fn() -> *mut c_void;
type DynamicConstruct = unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void;
type DynamicRead = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
#[repr(C)]
struct ImageClosureRow {
    source: [u8; 32],
    signature: [u8; 32],
    address: u64,
    role: u64,
}
impl ImageClosureRow {
    fn bytes(&self) -> [u8; 80] {
        let mut bytes = [0; 80];
        bytes[..32].copy_from_slice(&self.source);
        bytes[32..64].copy_from_slice(&self.signature);
        bytes[64..72].copy_from_slice(&self.address.to_ne_bytes());
        bytes[72..80].copy_from_slice(&self.role.to_ne_bytes());
        bytes
    }
}
#[repr(C)]
struct ImportRowV1 {identity:[u8;32],address:u64,library:u64,generation:u64}
#[repr(C)]
struct ImportTableV1 {version:u32,size:u32,count:usize,rows:*const ImportRowV1}
const _:()={assert!(size_of::<ImportRowV1>()==56);assert!(size_of::<ImportTableV1>()==24);};
#[repr(C)]
struct DynamicRegistration {
    version: u32,
    size: u32,
    library: u64,
    generation: u64,
    source: [u8; 32],
    digest: [u8; 32],
    signature: *const u8,
    length: usize,
    payload: *mut c_void,
    cell: *mut c_void,
    construct: DynamicConstruct,
    read: DynamicRead,
}
const _: () = {
    assert!(size_of::<ImageClosureRow>() == 80);
    assert!(size_of::<DynamicRegistration>() == 136);
};

#[repr(C)]
struct AdmissionTable {
    version: u32,
    size: u32,
    context: *mut c_void,
    admit: Admit,
}

struct AdmissionContext {
    source: [u8; 32],
    shapes: Vec<u64>,
    generation: u64,
    open: OwnerOpen,
    bind: OwnerBind,
    close: OwnerClose,
    library: u64,
    called: bool,
}

unsafe extern "C" fn admit(
    context: *mut c_void,
    source: *const u8,
    shapes: *const u64,
    count: usize,
    output: *mut u64,
) -> i32 {
    if context.is_null() || output.is_null() || (output as usize) % align_of::<u64>() != 0 {
        return 1;
    }
    unsafe {
        *output = 0;
    }
    // Only our source-issued image receives this borrowed table. A native image
    // cannot retain this context; all pointers are consumed synchronously.
    let context = unsafe { &mut *context.cast::<AdmissionContext>() };
    let result = catch_unwind(AssertUnwindSafe(|| {
        if context.called {
            return 1;
        }
        context.called = true;
        if source.is_null()
            || shapes.is_null()
            || (shapes as usize) % align_of::<u64>() != 0
            || count != context.shapes.len()
            || count == 0
            || count > 4096
        {
            return 1;
        }
        if unsafe { std::slice::from_raw_parts(source, 32) } != context.source
            || unsafe { std::slice::from_raw_parts(shapes, count) } != context.shapes
        {
            return 2;
        }
        let status = unsafe { (context.open)(context.generation, &mut context.library) };
        if status != 0 || context.library == 0 {
            return if status != 0 { status } else { 1 };
        }
        let status = unsafe {
            (context.bind)(context.library, context.generation, context.shapes.as_ptr(), context.shapes.len())
        };
        if status != 0 {
            return status;
        }
        unsafe {
            *output = context.library;
        }
        0
    }));
    result.unwrap_or(1)
}

struct AdmittedImage {
    image: usize,
    library: u64,
    generation: u64,
    _producer: IssuedGlueImage,
    failures: Vec<super::reserved::ReservedFailureDomain>,
    dynamic_tags: Vec<([u8; 32], u64)>,
    opaque_domains_admitted:bool,
    imports_admitted:bool,
}

struct AdmittedRustOwner {
    image:usize, library:u64, generation:u64,
    producer:IssuedRustOwner,
}

/// Loader-facing admission facts for one Rust owner image. Only actual owner production
/// (`build_rust_owner`) or a pinned published admission record constructs one.
pub(super) struct IssuedRustOwner {
    pub(super) library: String,
    pub(super) generation: u64,
    pub(super) payload_path: std::path::PathBuf,
    pub(super) payload_sha256: String,
    pub(super) provider_path: std::path::PathBuf,
    pub(super) provider_sha256: String,
    pub(super) source_sha256: [u8; 32],
    pub(super) destructors: Vec<([u8; 32], String)>,
    pub(super) checked_exports: Vec<(String, String, u64)>,
    /// The complete producer closure when this process built the owner; `None` when the
    /// consumer's pinned admission record issued it.
    pub(super) producer: Option<super::rust_owner::PreparedRustOwner>,
}
impl IssuedRustOwner {
    fn verify(&self) -> AotResult<()> {
        if hash_file(&self.payload_path)? != self.payload_sha256
            || hash_file(&self.provider_path)? != self.provider_sha256
        {
            return Err(invalid("Rust owner image or provider changed after admission"));
        }
        if let Some(producer) = &self.producer {
            producer.verify()?;
        }
        Ok(())
    }
}
impl From<super::rust_owner::PreparedRustOwner> for IssuedRustOwner {
    fn from(owner: super::rust_owner::PreparedRustOwner) -> Self {
        Self {
            library: owner.library.clone(),
            generation: owner.generation,
            payload_path: owner.payload_path.clone(),
            payload_sha256: owner.payload_sha256.clone(),
            provider_path: owner.provider_path.clone(),
            provider_sha256: owner.provider_sha256.clone(),
            source_sha256: owner.source_sha256,
            destructors: owner.destructors.clone(),
            checked_exports: owner.checked_exports.clone(),
            producer: Some(owner),
        }
    }
}
#[repr(C)]
struct OpaqueDomainRow {consumer_brand:[u8;32],owner_library:u64,owner_brand:[u8;32],owner_generation:u64}
const _:()={assert!(size_of::<OpaqueDomainRow>()==80);};

/// Holds one canonical process and all admitted descriptor images until heap
/// teardown. It inherits the opening-thread restriction of NativeHostLease.
pub struct GlueProcess {
    lease: Option<NativeHostLease>,
    close: OwnerClose,
    images: Vec<AdmittedImage>,
    rust_owners:Vec<AdmittedRustOwner>,
}

/// Borrowed callable issued by the actual producer and this live process.
/// Neither a caller symbol string nor an artifact JSON row can construct it.
pub struct CheckedGlueCallable<'a> {
    process: &'a GlueProcess,
    image: usize,
    entry: usize,
}
impl CheckedGlueCallable<'_> {
    pub fn source_symbol(&self) -> &str {
        &self.process.images[self.image]._producer.producer.checked_entries[self.entry].source_symbol
    }
    pub fn source_parameter_count(&self) -> usize {
        self.process.images[self.image]._producer.producer.checked_entries[self.entry].source_parameter_count
    }
    /// Invokes only the privately emitted checked source entry. The destination
    /// is registered before `invoke` evaluates or marshals any arguments, and
    /// remains rooted through `consume` and its allocations.
    ///
    /// # Safety
    /// T and argument marshaling must match this source-issued native signature:
    /// ordinary source parameters followed by failure handle and destination.
    /// `invoke` must call the supplied entry exactly once, and `consume` must not
    /// expose an unrooted managed address beyond this lease.
    pub unsafe fn invoke<T, R>(
        &self,
        invoke: impl FnOnce(&T, usize, *mut *mut c_void) -> AotResult<()>,
        consume: impl FnOnce(*mut c_void) -> AotResult<R>,
    ) -> AotResult<R> {
        let image = &self.process.images[self.image];
        if !image.imports_admitted || !image.opaque_domains_admitted {return Err(invalid("source call dependencies are not privately admitted"));}
        let witness = &image._producer.producer.checked_entries[self.entry];
        let reservation = image
            ._producer
            .producer
            .failure_admissions
            .iter()
            .position(|symbol| symbol == &witness.admission_symbol)
            .ok_or_else(|| invalid("checked source entry lost its issued reservation"))?;
        let domain =
            image.failures.get(reservation).ok_or_else(|| invalid("checked source reservation was not admitted"))?;
        let lease = self.process.lease.as_ref().ok_or_else(|| invalid("closed Glue process"))?;
        lease.images().verify_integrity().map_err(|error| invalid(error.to_string()))?;
        let mut destination = domain.destination()?;
        let symbol = std::ffi::CString::new(witness.symbol.as_str())
            .map_err(|_| invalid("invalid private checked source symbol"))?;
        let entry = unsafe { lease.images().image_symbol::<T>(image.image, &symbol) }
            .map_err(|error| invalid(error.to_string()))?;
        invoke(&entry, destination.failure_handle(), destination.address())?;
        lease.images().verify_integrity().map_err(|error| invalid(error.to_string()))?;
        destination.with_published(consume)
    }
}

impl GlueProcess {
    pub fn dynamic_shape_tags(&self, image: usize) -> AotResult<&[([u8; 32], u64)]> {
        self.images
            .iter()
            .find(|entry| entry.image == image)
            .map(|entry| entry.dynamic_tags.as_slice())
            .ok_or_else(|| invalid("Glue image has no issued initialization"))
    }
    pub fn checked_entries(&self, image: usize) -> AotResult<Vec<CheckedGlueCallable<'_>>> {
        let index = self
            .images
            .iter()
            .position(|entry| entry.image == image)
            .ok_or_else(|| invalid("Glue image has no issued initialization"))?;
        Ok((0..self.images[index]._producer.producer.checked_entries.len())
            .map(|entry| CheckedGlueCallable { process: self, image: index, entry })
            .collect())
    }
    /// Only a non-deserializable actual producer witness can start this process.
    pub fn open(artifact: PreparedGlueArtifact) -> AotResult<Self> {
        artifact.verify_native_closure()?;
        Self::open_issued(artifact.into_issued())
    }

    /// Starts the process from issued consumer facts: an actual producer witness or a
    /// pinned published admission record.
    pub(super) fn open_issued(artifact: IssuedGlueImage) -> AotResult<Self> {
        artifact.verify_native_closure()?;
        let images = unsafe {
            PinnedNativeImages::load(
                &artifact.provider_path,
                &artifact.provider_sha256,
                &artifact.payload_path,
                &artifact.payload_sha256,
            )
        }
        .map_err(|error| invalid(error.to_string()))?;
        let lease = unsafe { NativeHostLease::open(images) }.map_err(|error| invalid(error.to_string()))?;
        let close = *unsafe { lease.images().provider_symbol::<OwnerClose>(c"beskid_glue_v1_owner_close_library") }
            .map_err(|error| invalid(error.to_string()))?;
        let mut process = Self { lease: Some(lease), close, images: Vec::new(),rust_owners:Vec::new() };
        process.initialize(artifact, 0)?;
        Ok(process)
    }

    /// Attach a second independently produced artifact to the existing process;
    /// no second provider or runtime is opened and images are not unloaded early.
    pub fn attach(&mut self, artifact: PreparedGlueArtifact) -> AotResult<usize> {
        artifact.verify_native_closure()?;
        let artifact = artifact.into_issued();
        let image = unsafe {
            self.lease.as_mut().ok_or_else(|| invalid("closed Glue process"))?.attach_image(
                &artifact.payload_path,
                &artifact.payload_sha256,
                &artifact.provider_sha256,
            )
        }
        .map_err(|error| invalid(error.to_string()))?;
        self.initialize(artifact, image)?;
        Ok(image)
    }

    fn initialize(&mut self, artifact: IssuedGlueImage, image: usize) -> AotResult<()> {
        let lease = self.lease.as_ref().ok_or_else(|| invalid("closed Glue process"))?;
        lease.images().verify_integrity().map_err(|error| invalid(error.to_string()))?;
        let mut source = [0; 32];
        for (index, byte) in source.iter_mut().enumerate() {
            *byte = u8::from_str_radix(
                artifact
                    .packet
                    .manifest
                    .compiled_source_sha256
                    .get(index * 2..index * 2 + 2)
                    .ok_or_else(|| invalid("invalid issued Glue source digest"))?,
                16,
            )
            .map_err(|error| invalid(error.to_string()))?;
        }
        let mut shapes: BTreeSet<_> = artifact.packet.manifest.bindings.iter().map(|binding| binding.shape_id).collect();
        for transport in &artifact.producer.handle_transports {
            shapes.insert(beskid_codegen::glue::artifact::shape_id(&transport.brand)
                .map_err(|error|invalid(error.to_string()))?);
        }
        // Complete source-issued immutable reservation before any usable domain
        // token is published. These symbols are private actual-producer evidence.
        let mut failures = Vec::with_capacity(artifact.producer.failure_admissions.len());
        for symbol in &artifact.producer.failure_admissions {
            let symbol = std::ffi::CString::new(symbol.as_str())
                .map_err(|_| invalid("invalid issued failure admission symbol"))?;
            failures.push(unsafe { super::reserved::ReservedFailureDomain::admit(lease, image, &symbol) }?);
        }
        if shapes.is_empty()
            || shapes.contains(&0)
            || shapes.len() > 4096
        {
            return Err(invalid("invalid issued Glue shape closure"));
        }
        let open = *unsafe { lease.images().provider_symbol::<OwnerOpen>(c"beskid_glue_v1_owner_open_library") }
            .map_err(|error| invalid(error.to_string()))?;
        let bind = *unsafe { lease.images().provider_symbol::<OwnerBind>(c"beskid_glue_v1_owner_bind_shapes") }
            .map_err(|error| invalid(error.to_string()))?;
        let initialize =
            *unsafe { lease.images().image_symbol::<Initialize>(image, c"beskid_glue_artifact_v1_initialize") }
                .map_err(|error| invalid(error.to_string()))?;
        let mut context = AdmissionContext {
            source,
            shapes: shapes.into_iter().collect(),
            generation: artifact.producer.syntax_generation.0,
            open,
            bind,
            close: self.close,
            library: 0,
            called: false,
        };
        let table = AdmissionTable {
            version: 1,
            size: size_of::<AdmissionTable>() as u32,
            context: std::ptr::from_mut(&mut context).cast(),
            admit,
        };
        let status = unsafe { initialize(&table) };
        if status != 0 || !context.called || context.library == 0 {
            if context.library != 0 {
                let closed = unsafe { (context.close)(context.library, context.generation) };
                if closed != 0 {
                    return Err(invalid("Glue initialization rollback failed; image remains process-pinned"));
                }
            }
            return Err(invalid(format!("issued Glue initialization rejected ({status})")));
        }
        if let Err(error) = lease.images().verify_integrity() {
            let closed = unsafe { (context.close)(context.library, context.generation) };
            return Err(invalid(format!(
                "Glue post-initialization integrity failed: {error}; rollback status {closed}"
            )));
        }
        let dynamic_tags =
            match Self::admit_dynamic(lease, image, &artifact, source, context.library, context.generation) {
                Ok(tags) => tags,
                Err(error) => {
                    let closed = unsafe { (context.close)(context.library, context.generation) };
                    if closed != 0 {
                        return Err(invalid(format!("Dynamic admission failed: {error}; rollback status {closed}")));
                    }
                    return Err(error);
                }
            };
        self.images.push(AdmittedImage {
            image,
            library: context.library,
            generation: context.generation,
            _producer: artifact,
            failures,
            dynamic_tags,
            opaque_domains_admitted:false,
            imports_admitted:false,
        });
        self.bind_opaque_domains()?;self.bind_imports()?;
        Ok(())
    }

    /// Only actual source compilation can supply the owning image witness.
    pub fn attach_rust_owner(&mut self,owner:super::rust_owner::PreparedRustOwner)->AotResult<usize>{
        owner.verify()?;
        self.attach_issued_owner(IssuedRustOwner::from(owner))
    }

    /// Attaches issued owner facts: an actual producer witness or a pinned published record.
    pub(super) fn attach_issued_owner(&mut self,owner:IssuedRustOwner)->AotResult<usize>{
        owner.verify()?;
        if self.rust_owners.iter().any(|existing|existing.producer.library==owner.library){
            return Err(invalid("opaque owner library already admitted in this process"));
        }
        let image=unsafe {self.lease.as_mut().ok_or_else(||invalid("closed Glue process"))?
            .attach_image(&owner.payload_path,&owner.payload_sha256,&owner.provider_sha256)}
            .map_err(|error|invalid(error.to_string()))?;
        let lease=self.lease.as_ref().ok_or_else(||invalid("closed Glue process"))?;
        let open=*unsafe {lease.images().provider_symbol::<OwnerOpen>(c"beskid_glue_v1_owner_open_library")}
            .map_err(|error|invalid(error.to_string()))?;
        let bind=*unsafe {lease.images().provider_symbol::<OwnerBind>(c"beskid_glue_v1_owner_bind_shapes")}
            .map_err(|error|invalid(error.to_string()))?;
        let mut library=0;let status=unsafe{open(owner.generation,&mut library)};
        if status!=0||library==0{return Err(invalid("Rust owner library issuance failed"));}
        let admission=(||->AotResult<()>{
            let mut ids=BTreeSet::new();let mut rows=Vec::new();
            for(brand,symbol)in &owner.destructors {
                let id=u64::from_le_bytes(brand[..8].try_into().map_err(|_|invalid("opaque brand width"))?);
                if id==0||!ids.insert(id){return Err(invalid("Rust owner folded brand collision"));}
                type Destructor=unsafe extern "C" fn(*mut c_void)->i32;
                let name=std::ffi::CString::new(symbol.as_str()).map_err(|_|invalid("Rust owner destructor name"))?;
                let callback=*unsafe{lease.images().image_symbol::<Destructor>(image,&name)}
                    .map_err(|error|invalid(error.to_string()))?;
                rows.push(ImageClosureRow{source:owner.source_sha256,signature:*brand,address:callback as usize as u64,role:6});
            }
            for (identity,symbol,shape) in &owner.checked_exports {
                if identity.len()!=64 || !identity.bytes().all(|byte|byte.is_ascii_hexdigit())
                    || symbol!=&format!("beskid_glue_rust_owner_v1_{identity}") || *shape==0 || !ids.insert(*shape) {
                    return Err(invalid("Rust owner checked export closure differs or collides"));
                }
                type CheckedAddress=unsafe extern "C" fn();
                let name=std::ffi::CString::new(symbol.as_str()).map_err(|_|invalid("Rust owner checked export name"))?;
                let _actual=unsafe{lease.images().image_symbol::<CheckedAddress>(image,&name)}
                    .map_err(|error|invalid(error.to_string()))?;
            }
            let ids=ids.into_iter().collect::<Vec<_>>();
            if ids.is_empty()||ids.len()>1024{return Err(invalid("Rust owner brand closure bounds"));}
            if unsafe{bind(library,owner.generation,ids.as_ptr(),ids.len())}!=0{return Err(invalid("Rust owner shape binding failed"));}
            rows.sort_by_key(ImageClosureRow::bytes);
            type BindImage=unsafe extern "C" fn(u64,u64,*const ImageClosureRow,usize)->i32;
            let bind_image=*unsafe{lease.images().provider_symbol::<BindImage>(c"beskid_glue_v1_owner_bind_image_closure")}
                .map_err(|error|invalid(error.to_string()))?;
            if unsafe{bind_image(library,owner.generation,rows.as_ptr(),rows.len())}!=0{return Err(invalid("Rust owner actual image closure rejected"));}
            type OwnerInitialize=unsafe extern "C" fn(u64,u64)->i32;
            let initialize=*unsafe{lease.images().image_symbol::<OwnerInitialize>(image,c"beskid_glue_rust_owner_v1_initialize")}
                .map_err(|error|invalid(error.to_string()))?;
            if unsafe{initialize(library,owner.generation)}!=0{return Err(invalid("Rust owner context publication rejected"));}
            lease.images().verify_integrity().map_err(|error|invalid(error.to_string()))?;Ok(())
        })();
        if let Err(error)=admission {
            let status=unsafe{(self.close)(library,owner.generation)};
            return Err(invalid(format!("Rust owner admission failed: {error}; rollback status {status}")));
        }
        self.rust_owners.push(AdmittedRustOwner{image,library,generation:owner.generation,producer:owner});
        self.bind_opaque_domains()?;self.bind_imports()?;Ok(image)
    }
    /// # Safety
    /// T must be the exact status/out-parameter signature produced for this
    /// current binding identity; the owning process lease must cover invocation.
    pub unsafe fn rust_owner_export<'a,T:'a>(&'a self,image:usize,identity:&str)
        ->AotResult<impl std::ops::Deref<Target=T>+'a> {
        let owner=self.rust_owners.iter().find(|owner|owner.image==image)
            .ok_or_else(||invalid("Rust owner image was not privately admitted"))?;
        owner.producer.verify()?;
        let (_,symbol,_)=owner.producer.checked_exports.iter().find(|(issued,_,_)|issued==identity)
            .ok_or_else(||invalid("Rust checked callable is outside the issued source closure"))?;
        let name=std::ffi::CString::new(symbol.as_str()).map_err(|_|invalid("Rust checked callable name"))?;
        let lease=self.lease.as_ref().ok_or_else(||invalid("closed Glue process"))?;
        lease.images().verify_integrity().map_err(|error|invalid(error.to_string()))?;
        unsafe{lease.images().image_symbol::<T>(image,&name)}.map_err(|error|invalid(error.to_string()))
    }
    fn bind_imports(&mut self)->AotResult<()> {
        let lease=self.lease.as_ref().ok_or_else(||invalid("closed Glue process"))?;
        for image in &mut self.images {
            if image.imports_admitted {continue;}
            let bindings=image._producer.packet.manifest.bindings.iter().filter(|binding|binding.direction=="import").collect::<Vec<_>>();
            if bindings.is_empty(){image.imports_admitted=true;continue;}
            if bindings.len()>4096{return Err(invalid("source import closure exceeds4096 rows"));}
            let mut rows=Vec::with_capacity(bindings.len());let mut ready=true;
            for binding in bindings {
                let matches=self.rust_owners.iter().filter_map(|owner| {
                    if owner.producer.library!=binding.library{return None;}
                    owner.producer.checked_exports.iter().find(|(identity,_,_)|identity==&binding.identity_sha256).map(|entry|(owner,entry))
                }).collect::<Vec<_>>();
                if matches.is_empty(){ready=false;break;}
                if matches.len()!=1{return Err(invalid("source import has ambiguous actual owning images"));}
                let(owner,(_,symbol,shape))=matches[0];owner.producer.verify()?;
                if *shape!=binding.shape_id{return Err(invalid("actual Rust checked signature differs from source import"));}
                let mut identity=[0;32];
                for(index,byte)in identity.iter_mut().enumerate(){*byte=u8::from_str_radix(binding.identity_sha256.get(index*2..index*2+2).ok_or_else(||invalid("import identity width"))?,16).map_err(|_|invalid("import identity encoding"))?;}
                type Address=unsafe extern "C" fn();
                let name=std::ffi::CString::new(symbol.as_str()).map_err(|_|invalid("source import symbol"))?;
                let address=*unsafe{lease.images().image_symbol::<Address>(owner.image,&name)}.map_err(|error|invalid(error.to_string()))?;
                rows.push(ImportRowV1{identity,address:address as usize as u64,library:owner.library,generation:owner.generation});
            }
            if !ready{continue;}
            type BindImports=unsafe extern "C" fn(*const ImportTableV1)->i32;
            let bind=*unsafe{lease.images().image_symbol::<BindImports>(image.image,c"beskid_glue_artifact_v1_bind_imports")}.map_err(|error|invalid(error.to_string()))?;
            let table=ImportTableV1{version:1,size:size_of::<ImportTableV1>()as u32,count:rows.len(),rows:rows.as_ptr()};
            if unsafe{bind(&table)}!=0{return Err(invalid("actual source import table admission rejected"));}
            lease.images().verify_integrity().map_err(|error|invalid(error.to_string()))?;
            image.imports_admitted=true;
        }
        Ok(())
    }

    fn bind_opaque_domains(&mut self)->AotResult<()>{
        let lease=self.lease.as_ref().ok_or_else(||invalid("closed Glue process"))?;
        type BindDomains=unsafe extern "C" fn(u64,u64,*const OpaqueDomainRow,usize)->i32;
        let bind=*unsafe{lease.images().provider_symbol::<BindDomains>(c"beskid_glue_v1_owner_bind_opaque_domains")}
            .map_err(|error|invalid(error.to_string()))?;
        for image in &mut self.images {
            if image.opaque_domains_admitted{continue;}
            let mut brands=std::collections::BTreeMap::new();
            for binding in &image._producer.packet.manifest.bindings {
                for physical in binding.parameters.iter().chain(std::iter::once(&binding.result)){
                    if let Some(opaque)=&physical.opaque {
                        if let Some(previous)=brands.insert(opaque.brand_sha256.clone(),opaque.library.clone()){
                            if previous!=opaque.library{return Err(invalid("source opaque brand has conflicting owner libraries"));}
                        }
                    }
                }
            }
            if brands.is_empty(){image.opaque_domains_admitted=true;continue;}
            let mut rows=Vec::new();let mut ready=true;
            for(brand,owner_name)in brands {
                let mut bytes=[0;32];for(index,byte)in bytes.iter_mut().enumerate(){
                    *byte=u8::from_str_radix(brand.get(index*2..index*2+2).ok_or_else(||invalid("opaque brand length"))?,16)
                        .map_err(|_|invalid("opaque brand encoding"))?;
                }
                let candidates=self.rust_owners.iter().filter(|owner|owner.producer.library==owner_name
                    &&owner.producer.destructors.iter().any(|(owned,_)|owned==&bytes)).collect::<Vec<_>>();
                if candidates.is_empty(){ready=false;break;}
                if candidates.len()!=1{return Err(invalid("opaque brand has ambiguous actual owning images"));}
                let owner=candidates[0];owner.producer.verify()?;
                rows.push(OpaqueDomainRow{consumer_brand:bytes,owner_library:owner.library,owner_brand:bytes,owner_generation:owner.generation});
            }
            if !ready{continue;}
            rows.sort_by_key(|row|row.consumer_brand);
            if unsafe{bind(image.library,image.generation,rows.as_ptr(),rows.len())}!=0{return Err(invalid("source/actual-owner opaque domain binding rejected"));}
            image.opaque_domains_admitted=true;
        }Ok(())
    }

    fn admit_dynamic(
        lease: &NativeHostLease,
        image: usize,
        artifact: &IssuedGlueImage,
        source: [u8; 32],
        library: u64,
        generation: u64,
    ) -> AotResult<Vec<([u8; 32], u64)>> {
        if artifact.producer.dynamic_shapes.is_empty() && artifact.producer.handle_transports.is_empty() {
            return Ok(Vec::new());
        }
        if artifact.producer.dynamic_shapes.len() > 4096 / 6 {
            return Err(invalid("Dynamic image closure exceeds4096 rows"));
        }
        let cell_getter =
            *unsafe { lease.images().provider_symbol::<DescriptorGetter>(c"beskid_dynamic_v1_erased_cell_descriptor") }
                .map_err(|error| invalid(error.to_string()))?;
        let cell = unsafe { cell_getter() };
        lease.images().verify_provider_address(cell).map_err(|error| invalid(error.to_string()))?;
        let construct =
            *unsafe { lease.images().provider_symbol::<DynamicConstruct>(c"beskid_dynamic_v1_erased_construct") }
                .map_err(|error| invalid(error.to_string()))?;
        let read = *unsafe { lease.images().provider_symbol::<DynamicRead>(c"beskid_dynamic_v1_erased_read") }
            .map_err(|error| invalid(error.to_string()))?;
        let checked_construct = *unsafe { lease.images().provider_symbol::<DynamicConstruct>(c"beskid_dynamic_v1_checked_erased_construct") }
            .map_err(|error| invalid(error.to_string()))?;
        let checked_read = *unsafe { lease.images().provider_symbol::<DynamicRead>(c"beskid_dynamic_v1_checked_erased_read") }
            .map_err(|error| invalid(error.to_string()))?;
        let mut resolved = Vec::with_capacity(artifact.producer.dynamic_shapes.len());
        let mut rows = Vec::with_capacity(artifact.producer.dynamic_shapes.len() * 6);
        for shape in &artifact.producer.dynamic_shapes {
            let symbol = std::ffi::CString::new(shape.payload_descriptor_getter.as_str())
                .map_err(|_| invalid("invalid issued Dynamic descriptor getter"))?;
            let getter = *unsafe { lease.images().image_symbol::<DescriptorGetter>(image, &symbol) }
                .map_err(|error| invalid(error.to_string()))?;
            let payload = unsafe { getter() };
            lease.images().verify_image_address(image, payload).map_err(|error| invalid(error.to_string()))?;
            for (address, role) in
                [(payload as usize, 1), (cell as usize, 2), (construct as usize, 3), (read as usize, 4),
                 (checked_construct as usize, 13), (checked_read as usize, 14)]
            {
                rows.push(ImageClosureRow { source, signature: shape.signature_sha256, address: address as u64, role });
            }
            resolved.push((shape, payload));
        }
        // Opaque nominal descriptors come from this same actual object producer.
        // Full brands remain distinct from the folded lookup ID.
        let mut admitted_brands=BTreeSet::new();
        for transport in &artifact.producer.handle_transports {
            if !admitted_brands.insert(&transport.brand) {continue;}
            let mut brand=[0;32];
            for (index,byte) in brand.iter_mut().enumerate() {
                *byte=u8::from_str_radix(transport.brand.get(index*2..index*2+2)
                    .ok_or_else(||invalid("invalid source opaque brand"))?,16)
                    .map_err(|error|invalid(error.to_string()))?;
            }
            let symbol=std::ffi::CString::new(transport.descriptor.as_str())
                .map_err(|_|invalid("invalid source opaque descriptor getter"))?;
            let getter=*unsafe {lease.images().image_symbol::<DescriptorGetter>(image,&symbol)}
                .map_err(|error|invalid(error.to_string()))?;
            let descriptor=unsafe {getter()};
            lease.images().verify_image_address(image,descriptor).map_err(|error|invalid(error.to_string()))?;
            rows.push(ImageClosureRow{source,signature:brand,address:descriptor as usize as u64,role:1});
        }
        if rows.len()>4096 {return Err(invalid("combined source image closure exceeds4096 rows"));}
        rows.sort_by_key(ImageClosureRow::bytes);
        if rows.windows(2).any(|pair| pair[0].bytes() == pair[1].bytes()) {
            return Err(invalid("duplicate issued Dynamic closure row"));
        }
        type Bind = unsafe extern "C" fn(u64, u64, *const ImageClosureRow, usize) -> i32;
        let bind = *unsafe { lease.images().provider_symbol::<Bind>(c"beskid_glue_v1_owner_bind_image_closure") }
            .map_err(|error| invalid(error.to_string()))?;
        let status = unsafe { bind(library, generation, rows.as_ptr(), rows.len()) };
        if status != 0 {
            return Err(invalid(format!("issued Dynamic image closure rejected ({status})")));
        }
        type Register = unsafe extern "C" fn(*const DynamicRegistration, *mut u64) -> i32;
        let register = *unsafe { lease.images().provider_symbol::<Register>(c"beskid_dynamic_v1_register_shape") }
            .map_err(|error| invalid(error.to_string()))?;
        let mut tags = Vec::with_capacity(resolved.len());
        for (shape, payload) in resolved {
            let dto = DynamicRegistration {
                version: 1,
                size: size_of::<DynamicRegistration>() as u32,
                library,
                generation,
                source,
                digest: shape.signature_sha256,
                signature: shape.signature.as_ptr(),
                length: shape.signature.len(),
                payload,
                cell,
                construct,
                read,
            };
            let mut tag = 0;
            let status = unsafe { register(&dto, &mut tag) };
            if status != 0 || tag == 0 {
                return Err(invalid(format!("issued Dynamic shape registration rejected ({status})")));
            }
            tags.push((shape.signature_sha256, tag));
        }
        lease.images().verify_integrity().map_err(|error| invalid(error.to_string()))?;
        Ok(tags)
    }

    /// # Safety
    /// T must be the exact normalized function signature issued for this export.
    pub unsafe fn export<'a, T: 'a>(
        &'a self,
        image: usize,
        name: &CStr,
    ) -> AotResult<impl std::ops::Deref<Target = T> + 'a> {
        let admitted = self
            .images
            .iter()
            .find(|entry| entry.image == image)
            .ok_or_else(|| invalid("Glue image has no issued initialization"))?;
        if !admitted._producer.packet.manifest.bindings.iter().any(|binding| {
            binding.direction == "export"
                && (beskid_codegen::glue::artifact::export_transport_symbol(binding).as_bytes() == name.to_bytes()
                    || beskid_codegen::glue::artifact::checked_invocation_symbol(binding).as_bytes() == name.to_bytes())
        }) && !beskid_codegen::glue::artifact::owned_release_symbols(&admitted._producer.packet.manifest)
            .iter()
            .any(|symbol| symbol.as_bytes() == name.to_bytes())
        {
            return Err(invalid("symbol is outside the issued Glue export closure"));
        }
        if !admitted.imports_admitted || !admitted.opaque_domains_admitted {return Err(invalid("source call dependencies are not privately admitted"));}
        let lease = self.lease.as_ref().ok_or_else(|| invalid("closed Glue process"))?;
        lease.images().verify_integrity().map_err(|error| invalid(error.to_string()))?;
        unsafe { lease.images().image_symbol(image, name) }.map_err(|error| invalid(error.to_string()))
    }
}

impl Drop for GlueProcess {
    fn drop(&mut self) {
        for image in &self.images {
            if unsafe { (self.close)(image.library, image.generation) } != 0 {
                // A failed revocation leaves uncertain managed ownership. Keep
                // runtime and descriptor owners instead of unloading them.
                if let Some(lease) = self.lease.take() {
                    std::mem::forget(lease);
                }
                return;
            }
        }
        for owner in &self.rust_owners {
            if unsafe{(self.close)(owner.library,owner.generation)}!=0 {
                if let Some(lease)=self.lease.take(){std::mem::forget(lease);}return;
            }
        }
        // Handle owners must die before their provider's heap is destroyed.
        for image in &mut self.images {
            image.failures.clear();
        }
        // NativeHostLease destroys the heap before releasing any image handle.
        drop(self.lease.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe extern "C" fn record_open(_: u64, output: *mut u64) -> i32 {
        unsafe {
            *output = u64::MAX;
        }
        9
    }
    unsafe extern "C" fn reject_bind(_: u64, _: u64, _: *const u64, _: usize) -> i32 {
        9
    }
    unsafe extern "C" fn close(_: u64, _: u64) -> i32 {
        0
    }
    fn context() -> AdmissionContext {
        AdmissionContext {
            source: [7; 32],
            shapes: vec![11, u64::MAX],
            generation: 17,
            open: record_open,
            bind: reject_bind,
            close,
            library: 0,
            called: false,
        }
    }

    // These test the host callback boundary, not native artifact qualification.
    #[test]
    fn substituted_source_or_shapes_fail_before_owner_issuance() {
        for substitute_source in [true, false] {
            let mut context = context();
            let source = if substitute_source { [8; 32] } else { [7; 32] };
            let shapes = if substitute_source { [11, u64::MAX] } else { [11, 12] };
            let mut output = u64::MAX;
            let status = unsafe {
                admit(
                    std::ptr::from_mut(&mut context).cast(),
                    source.as_ptr(),
                    shapes.as_ptr(),
                    shapes.len(),
                    &mut output,
                )
            };
            assert_eq!(status, 2);
            assert_eq!(output, 0);
            assert_eq!(context.library, 0, "owner open must not run for a substituted closure");
        }
    }

    #[test]
    fn borrowed_admission_callback_is_once_only_even_after_failure() {
        let mut context = context();
        let source = context.source;
        let shapes = context.shapes.clone();
        let mut output = u64::MAX;
        let first = unsafe {
            admit(std::ptr::from_mut(&mut context).cast(), source.as_ptr(), shapes.as_ptr(), shapes.len(), &mut output)
        };
        assert_eq!(first, 9);
        assert_eq!(output, 0);
        assert_eq!(context.library, u64::MAX, "loader must roll back a provisional issued token");
        context.library = 0;
        output = u64::MAX;
        let second = unsafe {
            admit(std::ptr::from_mut(&mut context).cast(), source.as_ptr(), shapes.as_ptr(), shapes.len(), &mut output)
        };
        assert_eq!(second, 1);
        assert_eq!(output, 0);
        assert_eq!(context.library, 0);
    }
}
