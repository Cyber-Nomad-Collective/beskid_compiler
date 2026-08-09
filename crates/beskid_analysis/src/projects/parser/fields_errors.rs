use std::collections::HashMap;

use bsol::BsolSpan;

use super::super::error::ProjectError;

pub(super) fn reject_corelib_opt_out_keys(
    fields: &HashMap<String, String>,
    extras: &HashMap<String, String>,
    _span: BsolSpan,
) -> Result<(), ProjectError> {
    if fields.contains_key("noCorelib") || extras.contains_key("noCorelib") {
        return Err(ProjectError::meta_contract(
            "E1876",
            "manifest must not declare `noCorelib`; host projects always resolve corelib through toolchain defaults",
        ));
    }
    let disables = fields
        .get("useCorelib")
        .or_else(|| extras.get("useCorelib"))
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("false"));
    if disables {
        return Err(ProjectError::meta_contract(
            "E1876",
            "manifest must not set `useCorelib = false`; host projects always resolve corelib through toolchain defaults",
        ));
    }
    Ok(())
}

pub(super) fn split_known_fields(
    fields: HashMap<String, String>,
    known: &[&str],
) -> (HashMap<String, String>, HashMap<String, String>) {
    let mut known_out = HashMap::new();
    let mut extras = HashMap::new();
    for (key, value) in fields {
        if known.contains(&key.as_str()) {
            known_out.insert(key, value);
        } else {
            extras.insert(key, value);
        }
    }
    (known_out, extras)
}

pub(super) fn required_field(fields: &HashMap<String, String>, key: &str) -> Result<String, ProjectError> {
    fields.get(key).cloned().ok_or_else(|| ProjectError::Validation(format!("missing required field `{key}`")))
}

pub(super) fn parse_at(span: BsolSpan, message: impl Into<String>) -> ProjectError {
    ProjectError::ParseAt { line: span.line, message: message.into(), start: Some(span.start), end: Some(span.end) }
}
