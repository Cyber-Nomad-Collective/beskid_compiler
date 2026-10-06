//! Immutable generator correspondence; physical layouts are issued separately by codegen.
use anyhow::{Result, bail};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const CANONICAL_SDK_SCHEMA: &[u8] =
    include_bytes!("../../../../corelib/packages/compiler-sdk/src/Beskid/Syntax/syntax.schema.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Schema {
    schema_version: u32,
    module: String,
    types: BTreeMap<String, SchemaType>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SchemaType {
    rust_source: Option<String>,
    body: Body,
}
#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Body {
    #[serde(rename = "record")]
    Record { fields: Vec<SchemaField> },
    #[serde(rename = "enum")]
    Enum { variants: Vec<SchemaVariant> },
    #[serde(rename = "list")]
    List { element: String, storage: String, field: String },
    #[serde(rename = "optional")]
    Optional { element: String },
    #[serde(rename = "spanned")]
    Spanned {
        element: String,
        #[serde(rename = "spanType")]
        span_type: String,
        #[serde(rename = "nodeIdType")]
        node_id_type: String,
    },
    #[serde(rename = "source-span")]
    SourceSpan {
        #[serde(rename = "offsetType")]
        offset_type: String,
        #[serde(rename = "positionType")]
        position_type: String,
        #[serde(rename = "positionBase")]
        position_base: u32,
    },
    #[serde(rename = "issued-node-reference")]
    IssuedNodeReference {
        #[serde(rename = "sourceUnitType")]
        source_unit_type: String,
        #[serde(rename = "issuerType")]
        issuer_type: String,
        #[serde(rename = "generationType")]
        generation_type: String,
        #[serde(rename = "nodeIdType")]
        node_id_type: String,
    },
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaField {
    ordinal: usize,
    beskid_name: String,
    rust_name: String,
    #[serde(rename = "type")]
    type_name: String,
}
impl SchemaField {
    pub fn ordinal(&self) -> usize {
        self.ordinal
    }
    pub fn beskid_name(&self) -> &str {
        &self.beskid_name
    }
    pub fn rust_name(&self) -> &str {
        &self.rust_name
    }
    pub fn type_name(&self) -> &str {
        &self.type_name
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaVariant {
    ordinal: usize,
    beskid_name: String,
    rust_name: String,
    kind: String,
    fields: Vec<SchemaField>,
}
impl SchemaVariant {
    pub fn ordinal(&self) -> usize {
        self.ordinal
    }
    pub fn beskid_name(&self) -> &str {
        &self.beskid_name
    }
    pub fn rust_name(&self) -> &str {
        &self.rust_name
    }
    pub fn fields(&self) -> &[SchemaField] {
        &self.fields
    }
}
/// No caller-created correspondence can acquire compiler layout authority.
pub struct CanonicalSdkSchema {
    schema: Schema,
}
impl CanonicalSdkSchema {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 8 * 1024 * 1024 || bytes != CANONICAL_SDK_SCHEMA {
            bail!("SDK correspondence differs from this compiler's generated schema; rebuild SDK and Mod together");
        }
        let schema: Schema = serde_json::from_slice(bytes)?;
        if schema.schema_version != 2 || schema.module != "Beskid.Syntax.Nodes" {
            bail!("invalid canonical SDK schema identity");
        }
        let result = Self { schema };
        for (name, ty) in &result.schema.types {
            if name.is_empty() {
                bail!("empty SDK type");
            }
            if ty.rust_source.as_ref().is_some_and(|path| path.is_empty() || path.contains("..")) {
                bail!("invalid SDK Rust source");
            }
            match &ty.body {
                Body::Record { fields } => result.validate_fields(fields)?,
                Body::Enum { variants } => {
                    for (ordinal, variant) in variants.iter().enumerate() {
                        if variant.ordinal != ordinal
                            || variant.rust_name.is_empty()
                            || variant.beskid_name.is_empty()
                            || !matches!(variant.kind.as_str(), "unit" | "tuple" | "record")
                            || (variant.kind == "unit" && !variant.fields.is_empty())
                        {
                            bail!("invalid SDK variant correspondence");
                        }
                        result.validate_fields(&variant.fields)?;
                    }
                }
                Body::List { element, storage, field } => {
                    if storage != "managed-array-record" || field != "items" {
                        bail!("invalid SDK list storage correspondence");
                    }
                    result.validate_reference(element)?;
                }
                Body::Optional { element } => result.validate_reference(element)?,
                Body::Spanned { element, span_type, node_id_type } => {
                    result.validate_reference(element)?;
                    if span_type != "NodeSpan" || node_id_type != "u32" {
                        bail!("invalid SDK spanned wrapper");
                    }
                }
                Body::SourceSpan { offset_type, position_type, position_base } => {
                    if offset_type != "u64" || position_type != "u64" || *position_base != 1 {
                        bail!("invalid SDK source span widths");
                    }
                }
                Body::IssuedNodeReference { source_unit_type, issuer_type, generation_type, node_id_type } => {
                    if source_unit_type != "string"
                        || issuer_type != "u64"
                        || generation_type != "u64"
                        || node_id_type != "u32"
                    {
                        bail!(
                            "SDK node reference must retain source-unit, invocation issuer and exact generation/node widths"
                        );
                    }
                }
            }
        }
        Ok(result)
    }
    fn validate_fields(&self, fields: &[SchemaField]) -> Result<()> {
        for (ordinal, field) in fields.iter().enumerate() {
            if field.ordinal != ordinal || field.beskid_name.is_empty() || field.rust_name.is_empty() {
                bail!("invalid SDK field correspondence");
            }
            self.validate_reference(&field.type_name)?;
        }
        Ok(())
    }
    fn validate_reference(&self, name: &str) -> Result<()> {
        if matches!(
            name,
            "bool" | "string" | "u64" | "u32" | "u16" | "u8" | "i64" | "i32" | "i16" | "i8" | "f64" | "f32"
        ) {
            return Ok(());
        }
        let name = name.strip_prefix("Beskid.Syntax.Nodes.").unwrap_or(name);
        if !self.schema.types.contains_key(name) {
            bail!("SDK schema references missing type {name}");
        }
        Ok(())
    }
    /// Exact source factory inventory derived from the same generated mirror.
    /// Arity is checked against current registered source declarations; layouts
    /// and nominal signatures remain separate compiler-issued facts.
    pub(super) fn constructor_inventory(&self) -> BTreeMap<String, usize> {
        let mut constructors = BTreeMap::new();
        for (name, ty) in &self.schema.types {
            match &ty.body {
                Body::Record { fields } => {
                    constructors.insert(format!("{name}Value"), fields.len());
                }
                Body::Enum { variants } => {
                    for variant in variants {
                        constructors.insert(format!("{name}{}Value", variant.beskid_name), variant.fields.len());
                    }
                }
                Body::List { .. } => {
                    constructors.insert(format!("{name}Value"), 1);
                }
                Body::Optional { .. } => {
                    constructors.insert(format!("{name}NoneValue"), 0);
                    constructors.insert(format!("{name}SomeValue"), 1);
                }
                Body::Spanned { .. } => {
                    constructors.insert(format!("{name}Value"), 3);
                }
                Body::SourceSpan { .. } => {
                    constructors.insert(format!("{name}Value"), 6);
                }
                Body::IssuedNodeReference { .. } => {
                    constructors.insert(format!("{name}Value"), 4);
                }
            }
        }
        constructors
    }
    pub fn bytes(&self) -> &'static [u8] {
        CANONICAL_SDK_SCHEMA
    }
    pub fn type_count(&self) -> usize {
        self.schema.types.len()
    }
    pub fn field(&self, name: &str, rust_name: &str) -> Option<&SchemaField> {
        let Body::Record { fields } = &self.schema.types.get(name)?.body else {
            return None;
        };
        fields.iter().find(|field| field.rust_name == rust_name)
    }
    pub fn variant(&self, name: &str, rust_name: &str) -> Option<&SchemaVariant> {
        let Body::Enum { variants } = &self.schema.types.get(name)?.body else {
            return None;
        };
        variants.iter().find(|variant| variant.rust_name == rust_name)
    }
}
