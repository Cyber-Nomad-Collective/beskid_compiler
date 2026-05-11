//! Machine-readable API documentation (`api.json` v2) shared by `beskid doc` and `beskid_pckg`.

use serde::{Deserialize, Serialize};

pub const API_JSON_SCHEMA_VERSION: u32 = 2;

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
