use std::collections::HashMap;

use super::super::model::{ModGeneratedOutput, ProjectGrammarSection, ProjectSchemasSection};

#[derive(Debug)]
pub(super) struct ParsedBlock {
    pub(super) label: Option<String>,
    pub(super) fields: HashMap<String, String>,
}

#[derive(Debug)]
pub(super) struct ParsedProjectBlock {
    pub(super) block_kind: String,
    pub(super) fields: HashMap<String, String>,
    pub(super) extras: HashMap<String, String>,
    pub(super) mod_section: Option<HashMap<String, ModFieldValue>>,
    pub(super) mod_generated_outputs: Vec<ModGeneratedOutput>,
    pub(super) grammar_section: Option<ProjectGrammarSection>,
    pub(super) template_section: Option<HashMap<String, String>>,
    pub(super) schemas_section: Option<ProjectSchemasSection>,
}

#[derive(Debug, Default)]
pub(super) struct ParsedBlocks {
    pub(super) project: Option<ParsedProjectBlock>,
    pub(super) targets: Vec<ParsedBlock>,
    pub(super) dependencies: Vec<ParsedBlock>,
    pub(super) link: Option<ParsedLinkBlock>,
}

#[derive(Debug)]
pub(super) struct ParsedLinkBlock {
    pub(super) libraries: Vec<String>,
    pub(super) search_paths: Vec<String>,
    pub(super) extra_args: Vec<String>,
}

#[derive(Debug, Default)]
pub(super) struct ParsedWorkspaceBlocks {
    pub(super) workspace: Option<ParsedBlock>,
    pub(super) members: Vec<ParsedBlock>,
    pub(super) overrides: Vec<ParsedBlock>,
    pub(super) registries: Vec<ParsedBlock>,
}

#[derive(Debug, Clone)]
pub(super) enum ModFieldValue {
    StringList(Vec<String>),
    U32(u32),
    String(String),
}

pub(super) const PROJECT_ROOT_FIELDS: &[&str] = &["name", "version", "root", "root_namespace", "type", "readme"];
pub(super) const WORKSPACE_ROOT_FIELDS: &[&str] = &["name", "resolver"];
pub(super) const MEMBER_FIELDS: &[&str] = &["path"];
