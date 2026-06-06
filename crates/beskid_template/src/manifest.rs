//! Parse and validate **`.beskid/template.json`** (`beskid.template.v1`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::{TemplateError, TemplateResult};

pub const TEMPLATE_SCHEMA: &str = "beskid.template.v1";
pub const TEMPLATE_MANIFEST_REL: &str = ".beskid/template.json";

/// First-party `shortName` → registry package id.
pub const SHORT_NAME_PACKAGES: &[(&str, &str)] = &[
    ("console", "beskid.templates.console"),
    ("lib", "beskid.templates.lib"),
    ("template", "beskid.templates.project"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TemplateOutputKind {
    Project,
    Workspace,
    Item,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateManifest {
    pub schema: String,
    pub identity: String,
    pub name: String,
    pub short_name: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub classifications: Option<Vec<String>>,
    pub tags: TemplateTags,
    pub source_name: Option<String>,
    pub name_symbol: Option<String>,
    pub symbols: BTreeMap<String, TemplateSymbol>,
    pub sources: Vec<TemplateSource>,
    pub guids: Vec<String>,
    pub forms: BTreeMap<String, TemplateForm>,
    pub post_actions: Vec<TemplatePostAction>,
    pub prefer_interactive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateTags {
    pub template_type: Option<TemplateOutputKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateSymbol {
    pub symbol_type: SymbolType,
    pub description: Option<String>,
    pub default_value: Option<String>,
    pub choices: Option<Vec<String>>,
    pub is_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolType {
    String,
    Choice,
    Bool,
    Integer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateForm {
    pub form_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateSource {
    pub source: String,
    pub target: String,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub copy_only: Vec<String>,
    pub rename: BTreeMap<String, String>,
    pub condition: bool,
    pub modifiers: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatePostAction {
    pub action_id: String,
    pub args: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct RawManifest {
    schema: String,
    identity: String,
    name: String,
    #[serde(rename = "shortName")]
    short_name: String,
    author: Option<String>,
    description: Option<String>,
    classifications: Option<Vec<String>>,
    tags: Option<RawTags>,
    #[serde(rename = "sourceName")]
    source_name: Option<String>,
    #[serde(rename = "nameSymbol")]
    name_symbol: Option<String>,
    #[serde(default)]
    symbols: BTreeMap<String, RawSymbol>,
    #[serde(default)]
    sources: Vec<RawSource>,
    #[serde(default)]
    guids: Vec<String>,
    #[serde(default)]
    forms: BTreeMap<String, RawForm>,
    #[serde(default, rename = "postActions")]
    post_actions: Vec<RawPostAction>,
    #[serde(default, rename = "preferInteractive")]
    prefer_interactive: bool,
}

#[derive(Debug, Deserialize)]
struct RawTags {
    #[serde(rename = "type")]
    template_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSymbol {
    #[serde(rename = "type")]
    symbol_type: String,
    description: Option<String>,
    #[serde(rename = "defaultValue")]
    default_value: Option<serde_json::Value>,
    choices: Option<Vec<String>>,
    #[serde(default, rename = "isRequired")]
    is_required: bool,
}

#[derive(Debug, Deserialize)]
struct RawForm {
    #[serde(rename = "formId", alias = "id")]
    form_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawSource {
    #[serde(default = "default_source")]
    source: String,
    #[serde(default = "default_target")]
    target: String,
    #[serde(default = "default_include")]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default, rename = "copyOnly")]
    copy_only: Vec<String>,
    #[serde(default)]
    rename: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    condition: bool,
    #[serde(default)]
    modifiers: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawPostAction {
    #[serde(rename = "actionId")]
    action_id: String,
    #[serde(default)]
    args: serde_json::Value,
}

fn default_source() -> String {
    "./".to_string()
}

fn default_target() -> String {
    "./".to_string()
}

fn default_include() -> Vec<String> {
    vec!["**/*".to_string()]
}

fn default_true() -> bool {
    true
}

pub fn resolve_package_id(short_name: &str) -> Option<&'static str> {
    SHORT_NAME_PACKAGES
        .iter()
        .find(|(sn, _)| *sn == short_name)
        .map(|(_, id)| *id)
}

pub fn load_manifest_from_path(path: &Path) -> TemplateResult<TemplateManifest> {
    let bytes = std::fs::read(path)?;
    parse_manifest_bytes(&bytes)
}

pub fn load_manifest_from_template_root(root: &Path) -> TemplateResult<TemplateManifest> {
    load_manifest_from_path(&root.join(TEMPLATE_MANIFEST_REL))
}

pub fn parse_manifest_bytes(bytes: &[u8]) -> TemplateResult<TemplateManifest> {
    let raw: RawManifest = serde_json::from_slice(bytes)?;
    validate_and_convert(raw)
}

fn validate_and_convert(raw: RawManifest) -> TemplateResult<TemplateManifest> {
    if raw.schema != TEMPLATE_SCHEMA {
        return Err(TemplateError::InvalidManifest(format!(
            "expected schema `{TEMPLATE_SCHEMA}`, got `{}`",
            raw.schema
        )));
    }

    if raw.identity.trim().is_empty() || raw.short_name.trim().is_empty() {
        return Err(TemplateError::InvalidManifest(
            "`identity` and `shortName` are required".to_string(),
        ));
    }

    let tags = TemplateTags {
        template_type: raw
            .tags
            .and_then(|t| t.template_type)
            .as_deref()
            .map(parse_template_type)
            .transpose()?,
    };

    let symbols = raw
        .symbols
        .into_iter()
        .map(|(id, sym)| parse_symbol(&id, sym).map(|parsed| (id, parsed)))
        .collect::<TemplateResult<BTreeMap<_, _>>>()?;

    let sources = raw.sources.into_iter().map(convert_source).collect();

    let forms = raw
        .forms
        .into_iter()
        .map(|(name, form)| {
            let form_id = form.form_id.unwrap_or_else(|| name.clone());
            Ok((name, TemplateForm { form_id }))
        })
        .collect::<TemplateResult<BTreeMap<_, _>>>()?;

    let post_actions = raw
        .post_actions
        .into_iter()
        .map(|a| TemplatePostAction {
            action_id: a.action_id,
            args: a.args,
        })
        .collect();

    Ok(TemplateManifest {
        schema: raw.schema,
        identity: raw.identity,
        name: raw.name,
        short_name: raw.short_name,
        author: raw.author,
        description: raw.description,
        classifications: raw.classifications,
        tags,
        source_name: raw.source_name,
        name_symbol: raw.name_symbol,
        symbols,
        sources,
        guids: raw.guids,
        forms,
        post_actions,
        prefer_interactive: raw.prefer_interactive,
    })
}

fn parse_template_type(value: &str) -> TemplateResult<TemplateOutputKind> {
    match value {
        "project" => Ok(TemplateOutputKind::Project),
        "workspace" => Ok(TemplateOutputKind::Workspace),
        "item" => Ok(TemplateOutputKind::Item),
        other => Err(TemplateError::InvalidManifest(format!(
            "unsupported tags.type `{other}`"
        ))),
    }
}

fn parse_symbol(id: &str, raw: RawSymbol) -> TemplateResult<TemplateSymbol> {
    let symbol_type = match raw.symbol_type.as_str() {
        "string" => SymbolType::String,
        "choice" => SymbolType::Choice,
        "bool" => SymbolType::Bool,
        "integer" => SymbolType::Integer,
        other => {
            return Err(TemplateError::InvalidManifest(format!(
                "symbol `{id}` has unsupported type `{other}`"
            )));
        }
    };

    if matches!(symbol_type, SymbolType::Choice)
        && raw.choices.as_ref().is_none_or(|c| c.is_empty())
    {
        return Err(TemplateError::InvalidManifest(format!(
            "symbol `{id}` of type choice requires `choices`"
        )));
    }

    let default_value = raw.default_value.map(value_to_string);

    Ok(TemplateSymbol {
        symbol_type,
        description: raw.description,
        default_value,
        choices: raw.choices,
        is_required: raw.is_required,
    })
}

fn value_to_string(value: serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

fn convert_source(raw: RawSource) -> TemplateSource {
    let mut exclude = raw.exclude;
    if exclude.is_empty() {
        exclude = default_exclude_patterns();
    }
    TemplateSource {
        source: raw.source,
        target: raw.target,
        include: if raw.include.is_empty() {
            default_include()
        } else {
            raw.include
        },
        exclude,
        copy_only: raw.copy_only,
        rename: raw.rename,
        condition: raw.condition,
        modifiers: raw.modifiers,
    }
}

pub fn default_exclude_patterns() -> Vec<String> {
    vec![
        "**/.beskid/template.json".to_string(),
        "**/target/**".to_string(),
        "**/obj/**".to_string(),
        "**/.git/**".to_string(),
    ]
}

impl TemplateManifest {
    pub fn output_kind(&self) -> TemplateOutputKind {
        self.tags
            .template_type
            .unwrap_or(TemplateOutputKind::Project)
    }

    pub fn primary_name_symbol_id(&self) -> &str {
        self.name_symbol.as_deref().unwrap_or("name")
    }
}
