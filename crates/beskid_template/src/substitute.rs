//! `{{symbolId}}` and `sourceName` placeholder substitution.

use std::collections::BTreeMap;

use regex::Regex;

use crate::error::{TemplateError, TemplateResult};
use crate::forms::apply_form;
use crate::manifest::TemplateManifest;

pub fn build_substitution_map(
    manifest: &TemplateManifest,
    raw_values: &BTreeMap<String, String>,
) -> TemplateResult<BTreeMap<String, String>> {
    let mut resolved = BTreeMap::new();
    for (id, value) in raw_values {
        resolved.insert(id.clone(), value.clone());
    }

    for (form_name, form) in &manifest.forms {
        if let Some(input_id) = form_name.strip_suffix("::input").or(Some(form_name.as_str()))
            && let Some(input) = raw_values.get(input_id) {
                let out = apply_form(&form.form_id, input)?;
                resolved.insert(form_name.clone(), out);
            }
    }

    Ok(resolved)
}

pub fn substitute_text(text: &str, values: &BTreeMap<String, String>) -> String {
    let mut out = text.to_string();
    for (id, value) in values {
        let needle = format!("{{{{{id}}}}}");
        out = out.replace(&needle, value);
    }
    out
}

pub fn apply_source_name(text: &str, source_name: &str, primary_value: &str) -> String {
    text.replace(source_name, primary_value)
}

pub fn ensure_no_placeholders_remain(text: &str) -> TemplateResult<()> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\{\{[^}]+\}\}").expect("placeholder regex"));
    if let Some(cap) = re.find(text) {
        return Err(TemplateError::InvalidManifest(format!(
            "unresolved placeholder `{}`",
            cap.as_str()
        )));
    }
    Ok(())
}

pub fn substitute_path_component(path: &str, values: &BTreeMap<String, String>) -> String {
    substitute_text(path, values)
}
