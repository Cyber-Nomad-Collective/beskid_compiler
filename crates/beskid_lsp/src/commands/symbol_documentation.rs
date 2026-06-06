//! LSP `workspace/executeCommand` handler for symbol documentation URLs.

use std::path::Path;

use std::str::FromStr;

use serde_json::{Value, json};
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{LSPAny, Uri};

use crate::protocol::execute_args::{first_arg_object, missing_args};
use crate::session::store::Document;

pub const CMD_GET_DOCUMENTATION_URI: &str = "beskid.symbol.getDocumentationUri";

const DEFAULT_PCKG_BASE: &str = "https://pckg.beskid-lang.org";
const DEFAULT_BOOK_BASE: &str = "https://beskid-lang.org";

pub fn handle_symbol_documentation_command(
    command: &str,
    arguments: Option<Vec<Value>>,
    doc: Option<&Document>,
    uri: &Uri,
) -> Result<Option<LSPAny>> {
    let _ = uri;
    if command != CMD_GET_DOCUMENTATION_URI {
        return Ok(None);
    }
    let Some(document) = doc else {
        return Ok(Some(json!({})));
    };
    let args = first_arg_object(&arguments).ok_or_else(missing_args)?;
    let offset = args
        .get("offset")
        .and_then(Value::as_u64)
        .ok_or_else(missing_args)? as usize;
    let url = documentation_uri_for_offset(document, offset);
    Ok(Some(json!({ "url": url })))
}

pub fn documentation_uri_for_document(document: &Document, offset: usize) -> Option<String> {
    documentation_uri_for_offset(document, offset)
}

fn documentation_uri_for_offset(document: &Document, offset: usize) -> Option<String> {
    let analysis = document.analysis.as_ref()?;
    let hover = beskid_analysis::services::hover_at_offset(analysis, offset)?;
    let symbol_name = extract_symbol_name(&hover.markdown);
    let source_path = hover.location.path.as_path();

    if let Some((package, version)) = package_from_materialized_path(source_path) {
        let base =
            std::env::var("BESKID_PCKG_BASE_URL").unwrap_or_else(|_| DEFAULT_PCKG_BASE.to_string());
        let base = base.trim_end_matches('/');
        let fragment = symbol_name
            .as_deref()
            .map(|s| format!("#{}", urlencoding_encode(s)))
            .unwrap_or_default();
        return Some(format!("{base}/docs/{package}@{version}{fragment}"));
    }

    let book_base =
        std::env::var("BESKID_BOOK_BASE_URL").unwrap_or_else(|_| DEFAULT_BOOK_BASE.to_string());
    let book_base = book_base.trim_end_matches('/');
    if let Some(name) = symbol_name {
        return Some(format!("{book_base}/book/?q={}", urlencoding_encode(&name)));
    }
    Some(format!("{book_base}/book/"))
}

fn extract_symbol_name(markdown: &str) -> Option<String> {
    for line in markdown.lines() {
        if let Some(rest) = line.strip_prefix("**")
            && let Some((name, _)) = rest.split_once("**")
        {
            let trimmed = name.trim();
            if !trimmed.is_empty() && trimmed != "local" {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

/// Extract package id and version from materialized dependency paths like
/// `.../obj/beskid/deps/src/corelib_console-abc123/src/...`
fn package_from_materialized_path(path: &Path) -> Option<(String, String)> {
    let path_str = path.to_string_lossy();
    let marker = "/deps/src/";
    let idx = path_str.find(marker)?;
    let after = &path_str[idx + marker.len()..];
    let segment = after.split('/').next()?;
    let (name_part, _hash) = segment.split_once('-')?;
    let package = name_part.to_string();
    Some((package, "latest".to_string()))
}

fn urlencoding_encode(input: &str) -> String {
    input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                c.to_string()
            } else {
                format!("%{:02X}", c as u8)
            }
        })
        .collect()
}

pub fn uri_from_command_args(arguments: &Option<Vec<Value>>) -> Result<Uri> {
    let args = first_arg_object(arguments).ok_or_else(missing_args)?;
    let uri_str = args
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(missing_args)?;
    Uri::from_str(uri_str).map_err(|_| missing_args())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_from_deps_path() {
        let path = Path::new(
            "/proj/obj/beskid/deps/src/corelib_console-bd3c8b5fe48c6cc8/src/Console/Style.bd",
        );
        let (pkg, ver) = package_from_materialized_path(path).expect("parsed");
        assert_eq!(pkg, "corelib_console");
        assert_eq!(ver, "latest");
    }
}
