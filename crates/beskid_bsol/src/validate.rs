//! Validate a parsed document against a schema profile.

use std::collections::HashMap;

use crate::ast::{BsolAssignment, BsolBlock, BsolDocument, BsolItem, BsolListItem, BsolValue};
use crate::error::BsolError;
use crate::schema::{
    BlockRule, Cardinality, FieldRule, LabelRequirement, SchemaProfile, ValueType,
};

/// Document validated against a schema profile; blocks carry matched rule ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedDocument {
    pub profile: String,
    pub blocks: Vec<ValidatedBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedBlock {
    pub span: crate::ast::BsolSpan,
    pub rule_id: String,
    pub kind: String,
    pub label: Option<String>,
    pub fields: HashMap<String, String>,
    pub field_spans: HashMap<String, crate::ast::BsolSpan>,
    pub extras: HashMap<String, String>,
    pub nested: Vec<ValidatedBlock>,
    pub lists: HashMap<String, Vec<String>>,
    pub raw_body: Option<String>,
}

/// Validate `document` against `profile`.
pub fn validate(document: &BsolDocument, profile: &SchemaProfile) -> Result<ValidatedDocument, BsolError> {
    let mut validated_top = Vec::new();
    let mut rule_counts: HashMap<String, usize> = HashMap::new();

    for block in &document.blocks {
        let rule = match_top_level_rule(profile, &block.kind)
            .ok_or_else(|| BsolError::schema_at(block.span, format!("unknown top-level block `{kind}`", kind = block.kind)))?;
        let validated = validate_block(block, rule)?;
        *rule_counts.entry(rule.id.clone()).or_default() += 1;
        validated_top.push(validated);
    }

    for rule in profile.top_level_rules() {
        let count = rule_counts.get(&rule.id).copied().unwrap_or(0);
        match rule.cardinality {
            Cardinality::One if count != 1 => {
                return Err(BsolError::Schema(format!(
                    "profile `{}` requires exactly one `{}` block, found {count}",
                    profile.name, rule.id
                )));
            }
            Cardinality::ZeroOrOne if count > 1 => {
                return Err(BsolError::Schema(format!(
                    "profile `{}` allows at most one `{}` block, found {count}",
                    profile.name, rule.id
                )));
            }
            _ => {}
        }
    }

    Ok(ValidatedDocument {
        profile: profile.name.clone(),
        blocks: validated_top,
    })
}

fn match_top_level_rule<'a>(profile: &'a SchemaProfile, kind: &str) -> Option<&'a BlockRule> {
    profile.top_level_rules().find(|rule| rule.matches_kind(kind))
}

fn validate_block(block: &BsolBlock, rule: &BlockRule) -> Result<ValidatedBlock, BsolError> {
    match (&block.schemaless_body, rule.schemaless) {
        (Some(_), false) => {
            return Err(BsolError::schema_at(
                block.span,
                format!(
                    "block `{}` uses `@schemaless` but profile rule `{}` is structured",
                    block.kind, rule.id
                ),
            ));
        }
        (None, true) => {
            return Err(BsolError::schema_at(
                block.span,
                format!(
                    "block `{}` requires `@schemaless` for profile rule `{}`",
                    block.kind, rule.id
                ),
            ));
        }
        (Some(raw), true) => {
            return Ok(ValidatedBlock {
                span: block.span,
                rule_id: rule.id.clone(),
                kind: block.kind.clone(),
                label: block.label.as_ref().map(|q| q.value.clone()),
                fields: HashMap::new(),
                field_spans: HashMap::new(),
                extras: HashMap::new(),
                nested: Vec::new(),
                lists: HashMap::new(),
                raw_body: Some(raw.clone()),
            });
        }
        (None, false) => {}
    }

    match rule.label {
        LabelRequirement::Required if block.label.is_none() => {
            return Err(BsolError::schema_at(
                block.span,
                format!("block `{}` requires a label", block.kind),
            ));
        }
        LabelRequirement::Forbidden if block.label.is_some() => {
            return Err(BsolError::schema_at(
                block.span,
                format!("block `{}` cannot carry a label", block.kind),
            ));
        }
        _ => {}
    }

    let mut fields = HashMap::new();
    let mut field_spans = HashMap::new();
    let mut extras = HashMap::new();
    let mut lists = HashMap::new();
    let mut nested = Vec::new();
    let mut nested_counts: HashMap<String, usize> = HashMap::new();

    for item in &block.items {
        match item {
            BsolItem::Assignment(assignment) => {
                let key = assignment.key.clone();
                if let Some(field_rule) = rule.fields.get(&key) {
                    let (value, list) = validate_field_value(assignment, field_rule)?;
                    field_spans.insert(key.clone(), assignment.span);
                    if let Some(list) = list {
                        lists.insert(key.clone(), list);
                    }
                    fields.insert(key, value);
                } else if rule.allow_extra_fields {
                    let value = extra_field_value(assignment)?;
                    extras.insert(key, value);
                } else {
                    return Err(BsolError::schema_at(
                        assignment.span,
                        format!("unknown field `{key}` on block `{}`", block.kind),
                    ));
                }
            }
            BsolItem::Block(nested_block) => {
                let nested_rule = rule
                    .nested_rule_for_kind(&nested_block.kind)
                    .ok_or_else(|| {
                        BsolError::schema_at(
                            nested_block.span,
                            format!(
                                "nested block `{}` not allowed inside `{}`",
                                nested_block.kind, block.kind
                            ),
                        )
                    })?;
                let validated = validate_block(nested_block, nested_rule)?;
                *nested_counts.entry(nested_rule.id.clone()).or_default() += 1;
                nested.push(validated);
            }
        }
    }

    for (field_name, field_rule) in &rule.fields {
        if field_rule.required && !fields.contains_key(field_name) && !lists.contains_key(field_name)
        {
            return Err(BsolError::schema_at(
                block.span,
                format!("missing required field `{field_name}`"),
            ));
        }
    }

    for nested_rule in rule.nested_order.iter().filter_map(|id| rule.nested.get(id)) {
        let count = nested_counts.get(&nested_rule.id).copied().unwrap_or(0);
        match nested_rule.cardinality {
            Cardinality::One if count != 1 => {
                return Err(BsolError::schema_at(
                    block.span,
                    format!(
                        "expected exactly one nested `{}` block, found {count}",
                        nested_rule.id
                    ),
                ));
            }
            Cardinality::ZeroOrOne if count > 1 => {
                return Err(BsolError::schema_at(
                    block.span,
                    format!(
                        "expected at most one nested `{}` block, found {count}",
                        nested_rule.id
                    ),
                ));
            }
            _ => {}
        }
    }

    Ok(ValidatedBlock {
        span: block.span,
        rule_id: rule.id.clone(),
        kind: block.kind.clone(),
        label: block.label.as_ref().map(|q| q.value.clone()),
        fields,
        field_spans,
        extras,
        nested,
        lists,
        raw_body: None,
    })
}

fn validate_field_value(
    assignment: &BsolAssignment,
    rule: &FieldRule,
) -> Result<(String, Option<Vec<String>>), BsolError> {
    match &rule.value_type {
        ValueType::Quoted => Ok((require_quoted(assignment)?, None)),
        ValueType::Ident => Ok((require_ident(assignment)?, None)),
        ValueType::U32 => Ok((require_u32(assignment)?, None)),
        ValueType::Loose => Ok((loose_string(assignment)?, None)),
        ValueType::List => {
            let list = require_list(assignment)?;
            let joined = list.join(",");
            Ok((joined, Some(list)))
        }
        ValueType::EnumOrQuoted(values) => {
            let value = enum_or_quoted(assignment, values)?;
            Ok((value, None))
        }
    }
}

fn require_quoted(assignment: &BsolAssignment) -> Result<String, BsolError> {
    match &assignment.value {
        BsolValue::QuotedString(q) => Ok(q.value.clone()),
        other => Err(BsolError::schema_at(
            assignment.span,
            format!("expected quoted string, found `{}`", value_preview(other)),
        )),
    }
}

fn require_ident(assignment: &BsolAssignment) -> Result<String, BsolError> {
    match &assignment.value {
        BsolValue::Ident(i) => Ok(i.clone()),
        BsolValue::QuotedString(q) => Ok(q.value.clone()),
        other => Err(BsolError::schema_at(
            assignment.span,
            format!("expected identifier, found `{}`", value_preview(other)),
        )),
    }
}

fn require_u32(assignment: &BsolAssignment) -> Result<String, BsolError> {
    let BsolValue::Ident(text) = &assignment.value else {
        return Err(BsolError::schema_at(
            assignment.span,
            "expected positive integer",
        ));
    };
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return Err(BsolError::schema_at(
            assignment.span,
            format!("expected positive integer, found `{text}`"),
        ));
    }
    Ok(text.clone())
}

fn loose_string(assignment: &BsolAssignment) -> Result<String, BsolError> {
    match &assignment.value {
        BsolValue::QuotedString(q) => Ok(q.value.clone()),
        BsolValue::Ident(i) => Ok(i.clone()),
        BsolValue::BracketList(_) => Err(BsolError::schema_at(
            assignment.span,
            "expected string or identifier, found list",
        )),
    }
}

/// Serialize schema `extras` values for downstream `HashMap<String, String>` lowering.
fn extra_field_value(assignment: &BsolAssignment) -> Result<String, BsolError> {
    match &assignment.value {
        BsolValue::QuotedString(q) => Ok(q.value.clone()),
        BsolValue::Ident(i) => Ok(i.clone()),
        BsolValue::BracketList(list) => Ok(format_bracket_list_literal(list)),
    }
}

fn format_bracket_list_literal(list: &crate::ast::BsolBracketList) -> String {
    let mut out = String::from("[");
    for (index, item) in list.items.iter().enumerate() {
        if index > 0 {
            out.push_str(", ");
        }
        match item {
            BsolListItem::Default => out.push_str("default"),
            BsolListItem::QuotedString(q) => {
                out.push('"');
                out.push_str(&q.value);
                out.push('"');
            }
            BsolListItem::Ident(i) => out.push_str(i),
        }
    }
    out.push(']');
    out
}

fn require_list(assignment: &BsolAssignment) -> Result<Vec<String>, BsolError> {
    let BsolValue::BracketList(list) = &assignment.value else {
        return Err(BsolError::schema_at(
            assignment.span,
            "expected bracket list",
        ));
    };
    let mut out = Vec::new();
    for item in &list.items {
        let token = match item {
            BsolListItem::Default => "default".to_string(),
            BsolListItem::QuotedString(q) => q.value.clone(),
            BsolListItem::Ident(i) => i.clone(),
        };
        out.push(token);
    }
    Ok(out)
}

fn enum_or_quoted(assignment: &BsolAssignment, allowed: &[String]) -> Result<String, BsolError> {
    let value = match &assignment.value {
        BsolValue::QuotedString(q) => q.value.clone(),
        BsolValue::Ident(i) => i.clone(),
        other => {
            return Err(BsolError::schema_at(
                assignment.span,
                format!(
                    "expected quoted string or enum literal, found `{}`",
                    value_preview(other)
                ),
            ));
        }
    };
    if allowed.is_empty() || allowed.iter().any(|v| v == &value) {
        Ok(value)
    } else {
        Err(BsolError::schema_at(
            assignment.span,
            format!("unsupported enum value `{value}`"),
        ))
    }
}

fn value_preview(value: &BsolValue) -> String {
    match value {
        BsolValue::QuotedString(q) => format!("\"{}\"", q.value),
        BsolValue::Ident(i) => i.clone(),
        BsolValue::BracketList(_) => "[...]".to_string(),
    }
}
