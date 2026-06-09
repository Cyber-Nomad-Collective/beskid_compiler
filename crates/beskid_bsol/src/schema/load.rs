//! Bootstrap loader: parse declarative schema profile documents into [`SchemaProfile`].

use std::collections::HashMap;

use crate::ast::{BsolBlock, BsolDocument, BsolItem, BsolListItem, BsolValue};
use crate::build::parse_bsol_document;
use crate::error::BsolError;
use crate::schema::{
    BlockRule, Cardinality, FieldRule, KindMatch, LabelRequirement, RuleScope, SchemaProfile,
    ValueType,
};

const EMBEDDED_PROFILES: &[(&str, &str)] = &[
    ("project.v1", include_str!("../../schemas/project.v1.bsol")),
    ("workspace.v1", include_str!("../../schemas/workspace.v1.bsol")),
    ("runtime.v1", include_str!("../../schemas/runtime.v1.bsol")),
    ("board.v1", include_str!("../../schemas/board.v1.bsol")),
];

/// Load an embedded schema profile by name (for example `project.v1`).
pub fn load_profile(name: &str) -> Result<SchemaProfile, BsolError> {
    let source = EMBEDDED_PROFILES
        .iter()
        .find(|(id, _)| *id == name)
        .map(|(_, src)| *src)
        .ok_or_else(|| BsolError::UnknownProfile(name.to_string()))?;
    let document = parse_bsol_document(source)?;
    parse_profile_document(&document)
}

fn parse_profile_document(document: &BsolDocument) -> Result<SchemaProfile, BsolError> {
    let profile_block = document
        .blocks
        .iter()
        .find(|b| b.kind == "profile")
        .ok_or_else(|| BsolError::Schema("schema profile document must contain a `profile` block".into()))?;
    let name = profile_block
        .label
        .as_ref()
        .map(|q| q.value.clone())
        .ok_or_else(|| {
            BsolError::schema_at(
                profile_block.span,
                "`profile` block must carry a quoted label (profile name)",
            )
        })?;

    let mut rules = HashMap::new();
    let mut top_level_order = Vec::new();
    for item in &profile_block.items {
        let BsolItem::Block(rule_block) = item else {
            continue;
        };
        if rule_block.kind != "rule" {
            return Err(BsolError::schema_at(
                rule_block.span,
                format!("unexpected item `{}` inside profile", rule_block.kind),
            ));
        }
        let rule_id = rule_block
            .label
            .as_ref()
            .map(|q| q.value.clone())
            .or_else(|| assignment_string(rule_block, "id").ok().flatten())
            .ok_or_else(|| {
                BsolError::schema_at(rule_block.span, "rule block requires a quoted label or `id`")
            })?;
        let rule = parse_block_rule(rule_block, RuleScope::TopLevel)?;
        if rule.scope == RuleScope::TopLevel {
            top_level_order.push(rule_id.clone());
        }
        rules.insert(rule_id, rule);
    }

    Ok(SchemaProfile {
        name,
        rules,
        top_level_order,
    })
}

fn parse_block_rule(block: &BsolBlock, default_scope: RuleScope) -> Result<BlockRule, BsolError> {
    let id = block
        .label
        .as_ref()
        .map(|q| q.value.clone())
        .or_else(|| assignment_string(block, "id").ok().flatten())
        .unwrap_or_else(|| block.kind.clone());
    let scope = parse_scope(block)?.unwrap_or(default_scope);
    let kind_match = parse_kind_match(block)?;
    let label = parse_label(block)?;
    let cardinality = parse_cardinality(block)?;
    let allow_extra_fields = parse_bool(block, "extras").unwrap_or(false);
    let allow_extra_nested = parse_bool(block, "nested_extras").unwrap_or(false);
    let schemaless = parse_bool(block, "schemaless").unwrap_or(false);

    let mut fields = HashMap::new();
    let mut nested = HashMap::new();
    let mut nested_order = Vec::new();
    for item in &block.items {
        match item {
            BsolItem::Block(nested_block) if nested_block.kind == "field" => {
                let field_name = nested_block
                    .label
                    .as_ref()
                    .map(|q| q.value.clone())
                    .or_else(|| assignment_string(nested_block, "name").ok().flatten())
                    .ok_or_else(|| {
                        BsolError::schema_at(
                            nested_block.span,
                            "`field` block requires a quoted label or `name`",
                        )
                    })?;
                let field_rule = parse_field_rule(nested_block)?;
                fields.insert(field_name, field_rule);
            }
            BsolItem::Block(nested_block) if nested_block.kind == "nested" => {
                let nested_id = nested_block
                    .label
                    .as_ref()
                    .map(|q| q.value.clone())
                    .or_else(|| assignment_string(nested_block, "id").ok().flatten())
                    .ok_or_else(|| {
                        BsolError::schema_at(
                            nested_block.span,
                            "`nested` block requires a quoted label or `id`",
                        )
                    })?;
                let nested_rule = parse_block_rule(nested_block, RuleScope::Nested)?;
                nested_order.push(nested_id.clone());
                nested.insert(nested_id, nested_rule);
            }
            BsolItem::Assignment(_) => {}
            BsolItem::Block(other) => {
                return Err(BsolError::schema_at(
                    other.span,
                    format!("unexpected `{}` inside rule", other.kind),
                ));
            }
        }
    }

    Ok(BlockRule {
        id,
        scope,
        kind_match,
        label,
        cardinality,
        fields,
        nested,
        nested_order,
        allow_extra_fields,
        allow_extra_nested,
        schemaless,
    })
}

fn parse_field_rule(block: &BsolBlock) -> Result<FieldRule, BsolError> {
    let value_type = parse_value_type(block)?;
    let required = parse_bool(block, "required").unwrap_or(false);
    Ok(FieldRule {
        value_type,
        required,
    })
}

fn parse_value_type(block: &BsolBlock) -> Result<ValueType, BsolError> {
    let ty = assignment_string(block, "type")?.ok_or_else(|| {
        BsolError::schema_at(block.span, "`field` block requires `type`")
    })?;
    match ty.as_str() {
        "quoted" => Ok(ValueType::Quoted),
        "ident" => Ok(ValueType::Ident),
        "u32" => Ok(ValueType::U32),
        "list" => Ok(ValueType::List),
        "loose" => Ok(ValueType::Loose),
        "enum_or_quoted" => {
            let values = assignment_list(block, "values")?;
            Ok(ValueType::EnumOrQuoted(values))
        }
        other => Err(BsolError::schema_at(
            block.span,
            format!("unknown field type `{other}`"),
        )),
    }
}

fn parse_kind_match(block: &BsolBlock) -> Result<KindMatch, BsolError> {
    let match_kind = assignment_string(block, "match")?.unwrap_or_else(|| "keyword".to_string());
    match match_kind.as_str() {
        "keyword" => {
            let keyword = assignment_string(block, "keyword")?.ok_or_else(|| {
                BsolError::schema_at(block.span, "`match = keyword` requires `keyword`")
            })?;
            Ok(KindMatch::Keyword(keyword))
        }
        "keywords" => {
            let keywords = assignment_list(block, "keywords")?;
            Ok(KindMatch::Keywords(keywords))
        }
        "free_ident" => {
            let except = assignment_list(block, "except").unwrap_or_default();
            Ok(KindMatch::FreeIdent { except })
        }
        other => Err(BsolError::schema_at(
            block.span,
            format!("unknown kind match `{other}`"),
        )),
    }
}

fn parse_scope(block: &BsolBlock) -> Result<Option<RuleScope>, BsolError> {
    let Some(scope) = assignment_string(block, "scope")? else {
        return Ok(None);
    };
    Ok(Some(match scope.as_str() {
        "top" => RuleScope::TopLevel,
        "nested" => RuleScope::Nested,
        "any" => RuleScope::Any,
        other => {
            return Err(BsolError::schema_at(
                block.span,
                format!("unknown scope `{other}`"),
            ));
        }
    }))
}

fn parse_label(block: &BsolBlock) -> Result<LabelRequirement, BsolError> {
    let Some(label) = assignment_string(block, "label")? else {
        return Ok(LabelRequirement::Optional);
    };
    Ok(match label.as_str() {
        "required" => LabelRequirement::Required,
        "forbidden" => LabelRequirement::Forbidden,
        "optional" => LabelRequirement::Optional,
        other => {
            return Err(BsolError::schema_at(
                block.span,
                format!("unknown label requirement `{other}`"),
            ));
        }
    })
}

fn parse_cardinality(block: &BsolBlock) -> Result<Cardinality, BsolError> {
    let Some(card) = assignment_string(block, "cardinality")? else {
        return Ok(Cardinality::Many);
    };
    Ok(match card.as_str() {
        "one" => Cardinality::One,
        "many" => Cardinality::Many,
        "zero_or_one" => Cardinality::ZeroOrOne,
        other => {
            return Err(BsolError::schema_at(
                block.span,
                format!("unknown cardinality `{other}`"),
            ));
        }
    })
}

fn parse_bool(block: &BsolBlock, key: &str) -> Option<bool> {
    assignment_string(block, key)
        .ok()
        .flatten()
        .map(|v| v == "true")
}

fn assignment_string(block: &BsolBlock, key: &str) -> Result<Option<String>, BsolError> {
    for item in &block.items {
        let BsolItem::Assignment(a) = item else {
            continue;
        };
        if a.key != key {
            continue;
        }
        return Ok(Some(value_as_string(&a.value)?));
    }
    Ok(None)
}

fn assignment_list(block: &BsolBlock, key: &str) -> Result<Vec<String>, BsolError> {
    for item in &block.items {
        let BsolItem::Assignment(a) = item else {
            continue;
        };
        if a.key != key {
            continue;
        }
        return value_as_list(&a.value);
    }
    Ok(Vec::new())
}

fn value_as_string(value: &BsolValue) -> Result<String, BsolError> {
    match value {
        BsolValue::QuotedString(q) => Ok(q.value.clone()),
        BsolValue::Ident(i) => Ok(i.clone()),
        BsolValue::BracketList(_) => Err(BsolError::Schema("expected string, found list".into())),
    }
}

fn value_as_list(value: &BsolValue) -> Result<Vec<String>, BsolError> {
    let BsolValue::BracketList(list) = value else {
        return Err(BsolError::Schema("expected list".into()));
    };
    let mut out = Vec::new();
    for item in &list.items {
        match item {
            BsolListItem::QuotedString(q) => out.push(q.value.clone()),
            BsolListItem::Ident(i) => out.push(i.clone()),
            BsolListItem::Default => out.push("default".to_string()),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_project_profile() {
        let profile = load_profile("project.v1").expect("load project profile");
        assert_eq!(profile.name, "project.v1");
        assert!(profile.rule("root").is_some());
        assert!(profile.rule("target").is_some());
    }
}
