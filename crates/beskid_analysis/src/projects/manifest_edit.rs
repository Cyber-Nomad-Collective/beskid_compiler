//! Source-range helpers used exclusively by the dependency intent editor.
use super::{DependencyIntent, DependencyIntentSource};
use crate::projects::ProjectError;
use bsol::{BsolBlock, BsolDocument, BsolItem, BsolQuotedString, BsolSpan, BsolValue, write_bsol_value};
use std::collections::BTreeMap;

pub(super) struct Edit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

pub(super) fn nonempty(value: &str, field: &str) -> Result<(), ProjectError> {
    if value.trim().is_empty() {
        return Err(ProjectError::Validation(format!(
            "{field} must not be empty; resolve registry intent before editing"
        )));
    }
    Ok(())
}
pub(super) fn validate_intent(intent: &DependencyIntent) -> Result<(), ProjectError> {
    nonempty(&intent.name, "dependency name")?;
    match &intent.source {
        DependencyIntentSource::Registry { registry, version } => {
            nonempty(version, "resolved dependency version")?;
            if let Some(registry) = registry {
                nonempty(registry, "registry name")?;
            }
        }
        DependencyIntentSource::Path(path) => {
            let value = path
                .to_str()
                .ok_or_else(|| ProjectError::Validation("dependency path must be valid Unicode".into()))?;
            nonempty(value, "dependency path")?;
            if path.is_absolute() {
                return Err(ProjectError::Validation(
                    "dependency path intent must remain relative to the selected project".into(),
                ));
            }
        }
    }
    Ok(())
}
pub(super) fn quoted(text: &str) -> Result<String, ProjectError> {
    write_bsol_value(&BsolValue::QuotedString(BsolQuotedString { span: BsolSpan::default(), value: text.to_owned() }))
        .map_err(|error| ProjectError::Parse(error.to_string()))
}
pub(super) fn newline(original: &str) -> &'static str {
    match original.find('\n') {
        Some(position) if position > 0 && original.as_bytes()[position - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}
pub(super) fn render_intent(intent: &DependencyIntent, newline: &str) -> Result<String, ProjectError> {
    let mut text = format!("dependency {} {{{newline}", quoted(&intent.name)?);
    match &intent.source {
        DependencyIntentSource::Registry { registry, version } => {
            text.push_str(&format!("  source = \"registry\"{newline}  version = {}{newline}", quoted(version)?));
            if let Some(registry) = registry {
                text.push_str(&format!("  registry = {}{newline}", quoted(registry)?));
            }
        }
        DependencyIntentSource::Path(path) => {
            let path = path
                .to_str()
                .ok_or_else(|| ProjectError::Validation("dependency path must be valid Unicode".into()))?;
            text.push_str(&format!("  source = \"path\"{newline}  path = {}{newline}", quoted(path)?));
        }
    }
    text.push_str(&format!("}}{newline}"));
    Ok(text)
}
pub(super) fn dependency_blocks(document: &BsolDocument) -> Result<BTreeMap<&str, &BsolBlock>, ProjectError> {
    let mut blocks = BTreeMap::new();
    for block in document.blocks.iter().filter(|block| block.kind == "dependency") {
        let name = block
            .label
            .as_ref()
            .ok_or_else(|| ProjectError::Validation("dependency block requires a label".into()))?
            .value
            .as_str();
        if blocks.insert(name, block).is_some() {
            return Err(ProjectError::Validation(format!("duplicate dependency `{name}`; refusing an ambiguous edit")));
        }
    }
    Ok(blocks)
}
pub(super) fn version_span(block: &BsolBlock) -> Result<BsolSpan, ProjectError> {
    let mut values = block.items.iter().filter_map(|item| match item {
        BsolItem::Assignment(a) if a.key == "version" => Some(a),
        _ => None,
    });
    let assignment = values
        .next()
        .ok_or_else(|| ProjectError::Validation("registry dependency requires an explicit version".into()))?;
    if values.next().is_some() {
        return Err(ProjectError::Validation("duplicate dependency version fields".into()));
    }
    match &assignment.value {
        BsolValue::QuotedString(value) => Ok(value.span),
        _ => Err(ProjectError::Validation("dependency version must be explicitly quoted to edit safely".into())),
    }
}
pub(super) fn apply(original: &str, mut edits: Vec<Edit>) -> Result<String, ProjectError> {
    edits.sort_by_key(|edit| edit.start);
    let mut previous = 0;
    for edit in &edits {
        if edit.start < previous
            || edit.end < edit.start
            || edit.end > original.len()
            || !original.is_char_boundary(edit.start)
            || !original.is_char_boundary(edit.end)
        {
            return Err(ProjectError::Validation("invalid or overlapping dependency source spans".into()));
        }
        previous = edit.end;
    }
    let mut output = original.to_owned();
    for edit in edits.into_iter().rev() {
        output.replace_range(edit.start..edit.end, &edit.replacement);
    }
    Ok(output)
}
