//! Machine-readable API documentation (`api.json`) shared by `beskid doc` and `beskid_pckg`.

use serde::{Deserialize, Serialize};

pub const API_JSON_SCHEMA_VERSION: u32 = 4;

/// Schema used when `api.json` is emitted without a resolver graph (symbol list only).
pub const API_JSON_SCHEMA_VERSION_BEFORE_GRAPH: u32 = 2;

/// Previous graph schema (v3); consumers may ignore v4-only fields when reading v3.
pub const API_JSON_SCHEMA_VERSION_GRAPH_V3: u32 = 3;

/// When present on [`ApiDocRoot`], consumers should build navigation only from `parentId` / `memberIds`, not from splitting `qualifiedName`.
pub const API_JSON_NAVIGATION_MODEL_GRAPH_V1: &str = "graph-v1";

/// Pointer embedded in `.bpk` `package.json` for registry ingestion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiDocumentationPointer {
    pub api_json: String,
    pub schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemDocArgument {
    pub name: String,
    pub markdown: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemDocStructured {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arguments: Vec<ItemDocArgument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_variants: Vec<ItemDocArgument>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub type_parameters: Vec<ItemDocArgument>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiTypeAnnotation {
    pub display: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_item_id: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiParameterDoc {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: ApiTypeAnnotation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_markdown: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiGenericParameterDoc {
    pub name: String,
}

/// Compiler-derived signature payload merged into [`ApiDocItem`] at emit time.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiItemSignature {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_type: Option<ApiTypeAnnotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<ApiTypeAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ApiParameterDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generic_parameters: Vec<ApiGenericParameterDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub module_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiLocation {
    pub file: String,
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiDocItem {
    pub id: Option<usize>,
    pub qualified_name: String,
    pub name: String,
    pub kind: String,
    pub visibility: Option<String>,
    pub location: ApiLocation,
    /// Parent row in `items` when this row is a member; `None` for roots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<usize>,
    /// Child row ids in emission order (same id space as `id`); redundant with `parentId` edges.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub member_ids: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub module_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_type: Option<ApiTypeAnnotation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_type: Option<ApiTypeAnnotation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<ApiParameterDoc>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub generic_parameters: Vec<ApiGenericParameterDoc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<ItemDocStructured>,
    #[serde(default)]
    pub controls: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiDocRoot {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub navigation_model: Option<String>,
    pub generator: String,
    pub source: String,
    pub items: Vec<ApiDocItem>,
}

impl ApiDocRoot {
    pub fn from_json_slice(bytes: &[u8]) -> serde_json::Result<Self> {
        serde_json::from_slice(bytes)
    }

    pub fn from_json_str(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }
}
