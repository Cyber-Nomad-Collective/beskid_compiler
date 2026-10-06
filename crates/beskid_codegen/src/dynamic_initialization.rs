//! Compiler-issued native initialization, sealed to the exact lowered artifact.
use crate::{CodegenArtifact, CodegenInput};
use beskid_queries::GenericSpecializationInstance;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone)]
struct ArtifactSeal {
    functions: Vec<crate::artifact::LoweredFunction>,
    types: HashMap<beskid_analysis::types::TypeId, crate::artifact::TypeDescriptorData>,
    literals: HashMap<String, Vec<u8>>,
    closures: Vec<crate::closure_static::ClosureStaticPlan>,
    event_wrapper: bool,
    aggregates: Vec<crate::AggregateStaticPlan>,
    arrays: Vec<crate::array_static::ArrayStaticPlan>,
    imports: Vec<crate::ExternImport>,
    trusted_imports: Vec<crate::ExternImport>,
    exports: Vec<crate::ExportEntry>,
}
impl ArtifactSeal {
    fn capture(a: &CodegenArtifact) -> Self {
        Self {
            functions: a.functions.clone(),
            types: a.type_descriptors.clone(),
            literals: a.string_literals.clone(),
            closures: a.closure_static_plans.clone(),
            event_wrapper: a.event_handler_wrapper_required,
            aggregates: a.aggregate_static_plans.clone(),
            arrays: a.array_static_plans.clone(),
            imports: a.extern_imports.clone(),
            trusted_imports: a.trusted_extern_imports.clone(),
            exports: a.exports.clone(),
        }
    }
    fn matches(&self, a: &CodegenArtifact) -> bool {
        self.functions == a.functions
            && self.types == a.type_descriptors
            && self.literals == a.string_literals
            && self.closures == a.closure_static_plans
            && self.event_wrapper == a.event_handler_wrapper_required
            && self.aggregates == a.aggregate_static_plans
            && self.arrays == a.array_static_plans
            && self.imports == a.extern_imports
            && self.trusted_imports == a.trusted_extern_imports
            && self.exports == a.exports
    }
}
#[derive(Debug, Clone)]
pub struct DynamicSignatureData {
    bytes: Vec<u8>,
    digest: [u8; 32],
    payload_getter: String,
}
impl DynamicSignatureData {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub fn signature_getter(&self) -> String {
        format!("beskid_dynamic_signature_v1_{}", hex(&self.digest))
    }
    pub fn shape_tag_getter(&self) -> String {
        format!("beskid_dynamic_shape_tag_v1_{}", hex(&self.digest))
    }
}
/// No public constructor/deserializer. Cloning this cannot grant authority to
/// changed functions, imports, data, exports or descriptor/request plans.
#[derive(Debug, Clone)]
pub struct DynamicInitializationPlan {
    generation: beskid_queries::SyntaxGenerationId,
    target: beskid_abi::abi_v5::TargetMetadata,
    source: [u8; 32],
    symbol: String,
    shapes: Vec<DynamicSignatureData>,
    seal: ArtifactSeal,
}
impl DynamicInitializationPlan {
    pub fn target(&self)->&beskid_abi::abi_v5::TargetMetadata { &self.target }
    pub fn generation(&self) -> beskid_queries::SyntaxGenerationId {
        self.generation
    }
    pub fn initializer_symbol(&self) -> &str {
        &self.symbol
    }
    pub fn signatures(&self) -> &[DynamicSignatureData] {
        &self.shapes
    }
    pub fn requires_shared_provider_v1(&self) -> bool {
        true
    }
    pub fn validate_artifact(&self, artifact: &CodegenArtifact) -> Result<(), String> {
        if !self.seal.matches(artifact) {
            return Err("Dynamic initializer retained with changed lowered artifact".into());
        }
        let Some(current) = &artifact.dynamic_initialization else {
            return Err("Dynamic initializer detached from artifact".into());
        };
        if current.symbol != self.symbol
            || current.source != self.source
            || current.generation != self.generation
            || current.target != self.target
            || current.shapes.len() != self.shapes.len()
            || current
                .shapes
                .iter()
                .zip(&self.shapes)
                .any(|(a, b)| a.bytes != b.bytes || a.digest != b.digest || a.payload_getter != b.payload_getter)
        {
            return Err("Dynamic initializer plan changed".into());
        }
        Ok(())
    }
    /// Native source is production input, not JSON authority. The privileged
    /// producer must validate the structural seal before compiling/linking it.
    pub fn native_source(&self) -> Vec<u8> {
        let mut c = String::from(
            "#include <stdint.h>\n#include <stddef.h>\n#include <string.h>\n#include <stdlib.h>\n#include \"owner_identity_v1.h\"\nextern void *beskid_dynamic_v1_erased_cell_descriptor(void);\nextern void *beskid_dynamic_v1_erased_construct(void*,void*);\nextern void *beskid_dynamic_v1_erased_read(void*);\nextern void *beskid_dynamic_v1_checked_erased_construct(void*,void*);\nextern void *beskid_dynamic_v1_checked_erased_read(void*);\nstatic uint64_t library=0;\nstatic int row_order(const void*a,const void*b){return memcmp(a,b,sizeof(BeskidGlueImageClosureRowV1));}\n",
        );
        for (i, s) in self.shapes.iter().enumerate() {
            c.push_str(&format!("extern void *{}(void);\nstatic const uint8_t signature_{i}[]={{{}}};\nconst uint8_t *{}(void){{return signature_{i};}}\nstatic uint64_t tag_{i}=0;\nuint64_t {}(void){{return tag_{i};}}\n",s.payload_getter,numbers(&s.bytes),s.signature_getter(),s.shape_tag_getter()));
        }
        c.push_str(&format!("int32_t {}(uint64_t generation){{\nif(!generation || library) return 1;\nuint64_t admitted=0;int32_t status=beskid_glue_v1_owner_open_library(generation,&admitted);if(status || !admitted)return status?status:1;\nBeskidGlueImageClosureRowV1 rows[{}]={{0}};\n",self.symbol,self.shapes.len()*6));
        for (i, s) in self.shapes.iter().enumerate() {
            c.push_str(&format!(
                "static const uint8_t source_{i}[32]={{{}}},digest_{i}[32]={{{}}};\n",
                numbers(&self.source),
                numbers(&s.digest)
            ));
            for (row_index, (role, address)) in [
                (1, format!("{}()", s.payload_getter)),
                (2, "beskid_dynamic_v1_erased_cell_descriptor()".into()),
                (3, "beskid_dynamic_v1_erased_construct".into()),
                (4, "beskid_dynamic_v1_erased_read".into()),
                (13, "beskid_dynamic_v1_checked_erased_construct".into()),
                (14, "beskid_dynamic_v1_checked_erased_read".into()),
            ].into_iter().enumerate() {
                let row = i * 6 + row_index;
                c.push_str(&format!("memcpy(rows[{row}].source_sha256,source_{i},32);memcpy(rows[{row}].signature_sha256,digest_{i},32);rows[{row}].address=(uint64_t)(uintptr_t)({address});rows[{row}].role={role};\n"));
            }
        }
        c.push_str(&format!("qsort(rows,{},sizeof *rows,row_order);status=beskid_glue_v1_owner_bind_image_closure(admitted,generation,rows,{});if(status)goto failed;\n",self.shapes.len()*6,self.shapes.len()*6));
        for (i, s) in self.shapes.iter().enumerate() {
            c.push_str(&format!("BeskidDynamicShapeRegistrationV1 shape_{i}={{0}};shape_{i}.version=1;shape_{i}.size=sizeof shape_{i};shape_{i}.library=admitted;shape_{i}.generation=generation;memcpy(shape_{i}.source_sha256,source_{i},32);memcpy(shape_{i}.signature_sha256,digest_{i},32);shape_{i}.signature={}();shape_{i}.signature_length={};shape_{i}.payload_descriptor={}();shape_{i}.cell_descriptor=beskid_dynamic_v1_erased_cell_descriptor();shape_{i}.construct=beskid_dynamic_v1_erased_construct;shape_{i}.read=beskid_dynamic_v1_erased_read;status=beskid_dynamic_v1_register_shape(&shape_{i},&tag_{i});if(status || !tag_{i}){{if(!status)status=1;goto failed;}}\n",s.signature_getter(),s.bytes.len(),s.payload_getter));
        }
        c.push_str("library=admitted;return 0;\nfailed:\n");
        for i in 0..self.shapes.len() {
            c.push_str(&format!("tag_{i}=0;\n"));
        }
        c.push_str("if(beskid_glue_v1_owner_close_library(admitted,generation))return 2;return status;\n}\n");
        c.into_bytes()
    }
}
fn numbers(bytes: &[u8]) -> String {
    bytes.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn issue_dynamic_initialization(
    input: &CodegenInput<'_>,
    instances: &[GenericSpecializationInstance],
    artifact: &CodegenArtifact,
) -> Result<Option<DynamicInitializationPlan>, String> {
    let mut shapes = BTreeMap::new();
    let mut total = 0usize;
    for requested in instances {
        let binding=beskid_queries::dynamic_shape_binding(input.database(),requested).map_err(|e|e.to_string())?;
        let unpack=beskid_queries::dynamic_unpacking_bridge(input.database(),requested).map_err(|e|e.to_string())?;
        let instance=binding.as_ref().map(|binding|binding.packing())
            .or_else(||unpack.as_ref().map(|bridge|bridge.packing())).unwrap_or(requested);
        let Some(shape) = input.compiled_dynamic_packing_shape(instance)? else { continue };
        let bridge = beskid_queries::dynamic_packing_bridge(input.database(), instance)
            .map_err(|e| e.to_string())?
            .ok_or("Dynamic initializer lacks current Pack bridge")?;
        let plan = input
            .aggregate_static_plan_for_specialization(bridge.box_literal(), Some(instance))
            .ok_or("Dynamic initializer lacks source allocation")?;
        if !artifact.aggregate_static_plans.iter().any(|existing| existing == &plan) {
            return Err("Dynamic initializer descriptor was not emitted".into());
        }
        if shapes.contains_key(shape.sha256()) {
            continue;
        }
        total = total.checked_add(shape.bytes().len()).ok_or("Dynamic signature total overflow")?;
        if total > 16 * 1024 * 1024 || shapes.len() >= 4096 / 6 {
            return Err("Dynamic initializer closure exceeds profile bound".into());
        }
        shapes.insert(
            *shape.sha256(),
            DynamicSignatureData {
                bytes: shape.bytes().to_vec(),
                digest: *shape.sha256(),
                payload_getter: plan.descriptor_getter.ok_or("Dynamic source descriptor getter unavailable")?,
            },
        );
    }
    if shapes.is_empty() {
        return Ok(None);
    }
    if input.typed_program().generation.0 == 0 {
        return Err("Dynamic requires a nonzero actual source generation".into());
    }
    let mut units =
        input.typed_program().assembly.units.iter().map(|u| (&u.logical_name, &u.program)).collect::<Vec<_>>();
    units.sort_by(|a, b| a.0.cmp(b.0));
    let source: [u8; 32] = Sha256::digest(serde_json::to_vec(&units).map_err(|e| e.to_string())?).into();
    let mut identity = Sha256::new();
    identity.update(b"beskid.Dynamic.Initialization.V1\0");
    identity.update(source);
    for digest in shapes.keys() {
        identity.update(digest);
    }
    Ok(Some(DynamicInitializationPlan {
        generation: input.typed_program().generation,
        target: input.target().clone(),
        source,
        symbol: format!("beskid_dynamic_initialize_v1_{:x}", identity.finalize()),
        shapes: shapes.into_values().collect(),
        seal: ArtifactSeal::capture(artifact),
    }))
}

#[cfg(test)]
mod seal_tests {
    mod support {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/codegen_input/support.rs"));
    }
    use super::*;
    use std::sync::Arc;
    use crate::module_emission::{SyntaxModuleItem,lower_syntax_program};
    use beskid_abi::abi_v5::AbiManifestV5;

    #[test]
    fn structural_seal_rejects_changed_registered_body_data_and_imports() {
        let (db,typed,root,target)=support::input_fixture_with_source("pub string Entry(){return \"held\";}");
        let key=support::find_node(&db,root,beskid_queries::IndexedNodeKind::FunctionDefinition).unwrap();
        let input=CodegenInput::new(&db,typed,Arc::from([root]),target.clone(),AbiManifestV5::canonical_runtime(target)).unwrap();
        let isa=cranelift_codegen::isa::lookup_by_name("x86_64").unwrap()
            .finish(cranelift_codegen::settings::Flags::new(cranelift_codegen::settings::builder())).unwrap();
        let artifact=lower_syntax_program(&input,isa.as_ref(),&[SyntaxModuleItem{key,symbol:"Entry".into()}]).unwrap();
        assert!(!artifact.functions.is_empty());assert!(!artifact.string_literals.is_empty());
        let seal=ArtifactSeal::capture(&artifact);assert!(seal.matches(&artifact));
        let mut changed=artifact.clone();changed.functions[0].function.signature.returns.clear();
        assert!(!seal.matches(&changed),"actual registered function ABI mutation must reject");
        let mut changed=artifact.clone();changed.string_literals.values_mut().next().unwrap()[0]^=1;
        assert!(!seal.matches(&changed),"actual source literal substitution must reject");
        let mut changed=artifact.clone();changed.extern_imports.push(crate::ExternImport{symbol:"foreign".into(),abi:Some("C".into()),library:None});
        assert!(!seal.matches(&changed),"additional native import must reject");
        let mut changed=artifact.clone();changed.event_handler_wrapper_required=!changed.event_handler_wrapper_required;
        assert!(!seal.matches(&changed),"descriptor emission mode mutation must reject");
    }
}
