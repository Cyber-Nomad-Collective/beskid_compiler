//! Shared parsing for `workspace/executeCommand` argument payloads.

use serde_json::{Map, Value};
use tower_lsp_server::jsonrpc::{Error, Result};

/// LSP invalid-params error when required execute-command arguments are absent.
pub fn missing_args() -> Error {
    Error::invalid_params("missing command arguments")
}

/// First argument object, when the client sends a single JSON object in `arguments`.
pub fn first_arg_object(arguments: &Option<Vec<Value>>) -> Option<&Map<String, Value>> {
    let args = arguments.as_ref()?;
    args.first()?.as_object()
}

/// Read a non-empty URI from either a bare string argument or `{ key: "file://..." }`.
pub fn required_uri_arg(arguments: &Option<Vec<Value>>, key: &str) -> Result<String> {
    let args = arguments.as_ref().ok_or_else(missing_args)?;
    if args.is_empty() {
        return Err(missing_args());
    }
    if let Some(uri) = args[0].as_str() {
        return Ok(uri.to_string());
    }
    if let Some(obj) = args[0].as_object()
        && let Some(uri) = obj.get(key).and_then(Value::as_str) {
            return Ok(uri.to_string());
        }
    Err(missing_args())
}

/// Trimmed non-empty string field from a command argument object.
pub fn non_empty_str_arg<'a>(args: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
const FORBIDDEN_RESPONSE_KEYS: &[&str] = &[
    "apiKey",
    "api_key",
    "token",
    "secret",
    "password",
    "authorization",
    "bearer",
];

/// Keys that must never appear in LSP execute-command JSON responses (recursive).
#[cfg(test)]
pub fn find_forbidden_secret_keys(value: &Value) -> Vec<String> {
    let mut found = Vec::new();
    collect_forbidden_keys(value, &mut found);
    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
fn collect_forbidden_keys(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if FORBIDDEN_RESPONSE_KEYS
                    .iter()
                    .any(|f| key.eq_ignore_ascii_case(f))
                {
                    found.push(key.clone());
                }
                collect_forbidden_keys(child, found);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_forbidden_keys(item, found);
            }
        }
        _ => {}
    }
}
