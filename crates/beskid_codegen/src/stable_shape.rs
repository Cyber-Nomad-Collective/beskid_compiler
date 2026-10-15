//! One canonical portable semantic graph signature, shared by serialization and Dynamic.
use crate::CodegenInput;
use beskid_queries::{AstNodeKey, DynamicPackingNode, DynamicPackingShape};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct CompiledStableShape {
    signature: Vec<u8>,
    digest: [u8; 32],
}
impl CompiledStableShape {
    pub fn signature(&self) -> &[u8] {
        &self.signature
    }
    pub fn sha256(&self) -> &[u8; 32] {
        &self.digest
    }
}
struct SignatureWriter {
    bytes: Vec<u8>,
    limit: usize,
}
impl SignatureWriter {
    fn token(&mut self, token: &str) -> Result<(), String> {
        let length = token.len().to_string();
        let size = self
            .bytes
            .len()
            .checked_add(length.len())
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_add(token.len()))
            .ok_or("shape signature length overflow")?;
        if size > self.limit {
            return Err("shape signature exceeds checked byte limit".into());
        }
        self.bytes.extend_from_slice(length.as_bytes());
        self.bytes.push(b':');
        self.bytes.extend_from_slice(token.as_bytes());
        Ok(())
    }
    fn number(&mut self, number: usize) -> Result<(), String> {
        self.token(&number.to_string())
    }
    fn flag(&mut self, flag: bool) -> Result<(), String> {
        self.token(if flag { "1" } else { "0" })
    }
}
impl CodegenInput<'_> {
    pub(crate) fn compile_stable_shape_graph(
        &self,
        graph: &DynamicPackingShape,
        schema_version: &str,
        max_signature_bytes: usize,
    ) -> Result<CompiledStableShape, String> {
        if graph.nodes().is_empty() || graph.nodes().len() > 1024 || schema_version.is_empty() {
            return Err("invalid bounded stable shape graph".into());
        }
        let program = self.typed_program();
        program.assembly.package_identities().validate().map_err(|e| e.to_string())?;
        let mut writer = SignatureWriter { bytes: Vec::new(), limit: max_signature_bytes };
        writer.token("beskid.shape/1")?;
        writer.token(schema_version)?;
        writer.number(0)?;
        writer.number(graph.nodes().len())?;
        let mut edges = 0usize;
        let reference = |writer: &mut SignatureWriter, index: u32| {
            if index as usize >= graph.nodes().len() {
                return Err("shape graph reference outside owned graph".into());
            }
            writer.number(index as usize)
        };
        for node in graph.nodes() {
            match node {
                DynamicPackingNode::Scalar { name, managed } => {
                    if !matches!(
                        *name,
                        "unit"
                            | "bool"
                            | "i8"
                            | "i16"
                            | "i32"
                            | "i64"
                            | "u8"
                            | "u16"
                            | "u32"
                            | "u64"
                            | "f32"
                            | "f64"
                            | "char"
                            | "word"
                            | "utf8"
                    ) {
                        return Err("unsupported stable primitive identity".into());
                    }
                    writer.token("scalar")?;
                    writer.token(name)?;
                    writer.flag(*managed)?;
                }
                DynamicPackingNode::Array { element } => {
                    writer.token("array")?;
                    reference(&mut writer, *element)?;
                    edges += 1;
                }
                DynamicPackingNode::Record { declaration, arguments, fields, container, .. } => {
                    writer.token(match container { None => "record", Some(beskid_queries::CanonicalContainerKind::List) => "record.list", Some(beskid_queries::CanonicalContainerKind::Map) => "record.map", _ => return Err("invalid canonical record container".into()) })?;
                    self.stable_shape_declaration(&mut writer, *declaration)?;
                    writer.number(arguments.len())?;
                    for argument in arguments {
                        reference(&mut writer, *argument)?;
                    }
                    writer.number(fields.len())?;
                    for field in fields {
                        writer.token(&field.name)?;
                        reference(&mut writer, field.ty)?;
                        writer.flag(field.managed)?;
                    }
                    edges = edges
                        .checked_add(arguments.len())
                        .and_then(|n| n.checked_add(fields.len()))
                        .ok_or("shape edge overflow")?;
                }
                DynamicPackingNode::Enum { declaration, arguments, variants, container, .. } => {
                    writer.token(match container { None => "enum", Some(beskid_queries::CanonicalContainerKind::Optional) => "enum.optional", _ => return Err("invalid canonical enum container".into()) })?;
                    self.stable_shape_declaration(&mut writer, *declaration)?;
                    writer.number(arguments.len())?;
                    for argument in arguments {
                        reference(&mut writer, *argument)?;
                    }
                    writer.number(variants.len())?;
                    edges = edges
                        .checked_add(arguments.len())
                        .and_then(|n| n.checked_add(variants.len()))
                        .ok_or("shape edge overflow")?;
                    for (ordinal, variant) in variants.iter().enumerate() {
                        if variant.ordinal as usize != ordinal {
                            return Err("shape variant ordinal differs from source order".into());
                        }
                        writer.token(&variant.name)?;
                        writer.number(ordinal)?;
                        writer.number(variant.fields.len())?;
                        for field in &variant.fields {
                            writer.token(&field.name)?;
                            reference(&mut writer, field.ty)?;
                            writer.flag(field.managed)?;
                        }
                        edges = edges.checked_add(variant.fields.len()).ok_or("shape edge overflow")?;
                    }
                }
            }
            if edges > 65536 {
                return Err("shape graph edge limit".into());
            }
        }
        let digest = Sha256::digest(&writer.bytes).into();
        Ok(CompiledStableShape { signature: writer.bytes, digest })
    }
    fn stable_shape_declaration(&self, writer: &mut SignatureWriter, key: AstNodeKey) -> Result<(), String> {
        use beskid_analysis::projects::VerifiedPackageSource;
        let identity = beskid_queries::portable_nominal_identity(self.database(), self.typed_program(), key)
            .map_err(|e| e.to_string())?;
        let package = identity.package();
        let (kind, registry, artifact) = match package.source() {
            VerifiedPackageSource::Local => ("local", "", ""),
            VerifiedPackageSource::Corelib => ("corelib", "", ""),
            VerifiedPackageSource::Registry { registry, artifact_digest } => {
                ("registry", registry.as_str(), artifact_digest.as_str())
            }
        };
        for token in [kind, package.package_name(), package.version(), package.source_digest(), registry, artifact] {
            writer.token(token)?;
        }
        writer.token(&identity.declaration().source_path)?;
        writer.number(identity.declaration().lexical_path.len())?;
        for component in &identity.declaration().lexical_path {
            writer.token(component)?;
        }
        Ok(())
    }
}
