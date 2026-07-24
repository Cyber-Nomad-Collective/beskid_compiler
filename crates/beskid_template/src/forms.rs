//! Built-in symbol value transforms (`identity`, `lowerCase`, `upperCase`, `safeName`, `namespace`).

use crate::error::{TemplateError, TemplateResult};

pub fn apply_form(form_id: &str, input: &str) -> TemplateResult<String> {
    match form_id {
        "identity" => Ok(input.to_string()),
        "lowerCase" => Ok(input.to_ascii_lowercase()),
        "upperCase" => Ok(input.to_ascii_uppercase()),
        "safeName" => Ok(safe_name(input)),
        "namespace" => Ok(namespace_from_name(input)),
        other => Err(TemplateError::InvalidManifest(format!("unknown form id `{other}`"))),
    }
}

fn safe_name(input: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = true;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
            prev_sep = false;
        } else if !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "Project".to_string()
    } else if trimmed.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("_{trimmed}")
    } else {
        trimmed.to_string()
    }
}

fn namespace_from_name(input: &str) -> String {
    safe_name(input).replace('_', ".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_name_strips_invalid() {
        assert_eq!(apply_form("safeName", "My App!").unwrap(), "My_App");
    }
}
