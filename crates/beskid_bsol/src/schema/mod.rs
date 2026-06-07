//! In-memory schema profile representation.

mod load;

use std::collections::HashMap;

pub use load::load_profile;

/// Loaded schema profile (e.g. `project.v1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaProfile {
    pub name: String,
    pub rules: HashMap<String, BlockRule>,
    pub top_level_order: Vec<String>,
}

/// Rule for matching and validating a block kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRule {
    pub id: String,
    pub scope: RuleScope,
    pub kind_match: KindMatch,
    pub label: LabelRequirement,
    pub cardinality: Cardinality,
    pub fields: HashMap<String, FieldRule>,
    pub nested: HashMap<String, BlockRule>,
    pub nested_order: Vec<String>,
    pub allow_extra_fields: bool,
    pub allow_extra_nested: bool,
    pub schemaless: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleScope {
    TopLevel,
    Nested,
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KindMatch {
    /// Exact keyword match (`target`, `workspace`, …).
    Keyword(String),
    /// Any identifier except listed keywords (`root` block).
    FreeIdent { except: Vec<String> },
    /// Match any of several keywords (e.g. `mod` / `meta`).
    Keywords(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelRequirement {
    #[default]
    Optional,
    Required,
    Forbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cardinality {
    #[default]
    Many,
    One,
    ZeroOrOne,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldRule {
    pub value_type: ValueType,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueType {
    Quoted,
    Ident,
    U32,
    List,
    EnumOrQuoted(Vec<String>),
    Loose,
}

impl SchemaProfile {
    pub fn rule(&self, id: &str) -> Option<&BlockRule> {
        self.rules.get(id)
    }

    pub fn top_level_rules(&self) -> impl Iterator<Item = &BlockRule> {
        self.top_level_order
            .iter()
            .filter_map(|id| self.rules.get(id))
    }
}

impl BlockRule {
    pub fn matches_kind(&self, kind: &str) -> bool {
        match &self.kind_match {
            KindMatch::Keyword(k) => kind == k,
            KindMatch::Keywords(keys) => keys.iter().any(|k| k == kind),
            KindMatch::FreeIdent { except } => !except.iter().any(|k| k == kind),
        }
    }

    pub fn nested_rule_for_kind(&self, kind: &str) -> Option<&BlockRule> {
        self.nested
            .values()
            .find(|rule| rule.matches_kind(kind))
            .or_else(|| {
                self.nested
                    .get(kind)
                    .filter(|rule| rule.matches_kind(kind))
            })
    }
}
