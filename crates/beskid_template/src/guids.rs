//! Regenerate GUIDs listed in the template manifest, preserving format per occurrence.

use std::collections::HashMap;

use regex::Regex;
use uuid::Uuid;

use crate::error::{TemplateError, TemplateResult};

pub fn replace_guids_in_text(
    text: &str,
    manifest_guids: &[String],
    mapping: &mut HashMap<String, String>,
) -> TemplateResult<String> {
    let mut out = text.to_string();
    for guid in manifest_guids {
        let replacement = mapping.entry(guid.clone()).or_insert_with(|| new_guid_matching_format(guid));
        out = replace_all_literal(&out, guid, replacement);
    }
    Ok(out)
}

pub fn verify_guids_replaced(text: &str, manifest_guids: &[String]) -> TemplateResult<()> {
    for guid in manifest_guids {
        if text.contains(guid) {
            return Err(TemplateError::GuidReplacement { guid: guid.clone() });
        }
    }
    Ok(())
}

fn replace_all_literal(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    haystack.replace(needle, replacement)
}

fn new_guid_matching_format(sample: &str) -> String {
    let uuid = Uuid::new_v4();
    let sample = sample.trim();
    if sample.starts_with('{') && sample.ends_with('}') {
        return format!("{{{}}}", uuid.hyphenated());
    }
    if sample.starts_with('(') && sample.ends_with(')') {
        return format!("({})", uuid.hyphenated());
    }
    if sample.contains('-') && sample.len() == 36 {
        return uuid.hyphenated().to_string();
    }
    if sample.len() == 32 && !sample.contains('-') {
        return uuid.simple().to_string().to_uppercase();
    }
    uuid.hyphenated().to_string()
}

/// Scan text for any manifest GUID still present after replacement (broader check).
pub fn scan_leftover_guid_patterns(text: &str, manifest_guids: &[String]) -> TemplateResult<()> {
    verify_guids_replaced(text, manifest_guids)?;
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").expect("guid regex")
    });
    for guid in manifest_guids {
        let normalized = guid.trim_matches(|c| c == '{' || c == '}' || c == '(' || c == ')');
        if let Some(mat) = re.find(text)
            && text.contains(normalized)
            && text.contains(guid)
        {
            return Err(TemplateError::GuidReplacement { guid: mat.as_str().to_string() });
        }
    }
    Ok(())
}
