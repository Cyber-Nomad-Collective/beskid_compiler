//! LSP `workspace/executeCommand` handlers for package registry connection state.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use beskid_analysis::projects::parse_workspace_manifest;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use serde_json::{Value, json};
use tokio::sync::RwLock;
use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::LSPAny;

use crate::protocol::execute_args::{first_arg_object, missing_args, non_empty_str_arg};
use crate::workspace_scan::{discover_workspace_manifest_paths, path_from_uri_string};

pub const CMD_GET_CONNECTION_STATUS: &str = "beskid.pckg.getConnectionStatus";
pub const CMD_SET_REGISTRY: &str = "beskid.pckg.setRegistry";
pub const CMD_VALIDATE_CONNECTION: &str = "beskid.pckg.validateConnection";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ValidationStatus {
    #[default]
    Unknown,
    Ok,
    Error,
}

#[derive(Debug, Clone, Default)]
pub struct PckgRegistryState {
    pub registry_base_url: Option<String>,
    pub registry_name: Option<String>,
    pub validation_status: ValidationStatus,
    pub validation_message: Option<String>,
}

pub async fn handle_pckg_registry_command(
    command: &str,
    arguments: Option<Vec<Value>>,
    workspace_roots: &[PathBuf],
    state: &Arc<RwLock<PckgRegistryState>>,
) -> Result<Option<LSPAny>> {
    match command {
        CMD_GET_CONNECTION_STATUS => {
            Ok(Some(get_connection_status(arguments, workspace_roots, state).await?))
        }
        CMD_SET_REGISTRY => {
            set_registry(arguments, state).await?;
            Ok(None)
        }
        CMD_VALIDATE_CONNECTION => {
            Ok(Some(validate_connection(arguments, workspace_roots, state).await?))
        }
        _ => Ok(None),
    }
}

async fn get_connection_status(
    arguments: Option<Vec<Value>>,
    workspace_roots: &[PathBuf],
    state: &Arc<RwLock<PckgRegistryState>>,
) -> Result<Value> {
    let args = first_arg_object(&arguments);
    let workspace_uri = args
        .and_then(|o| o.get("workspaceUri"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let auth_configured = args
        .and_then(|o| o.get("authConfigured"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let (workspace_default_url, workspace_default_name) =
        resolve_workspace_default_registry(workspace_uri.as_deref(), workspace_roots);

    let guard = state.read().await;
    let base_url = guard
        .registry_base_url
        .clone()
        .or(workspace_default_url.clone())
        .map(|u| normalize_base_url(&u))
        .unwrap_or_default();
    let registry_name = guard
        .registry_name
        .clone()
        .or(workspace_default_name.clone());

    let validation = json!({
        "status": validation_status_str(&guard.validation_status),
        "message": guard.validation_message,
    });
    let connected = guard.validation_status == ValidationStatus::Ok
        && (!requires_auth_hint(&guard.validation_message) || auth_configured);

    Ok(json!({
        "baseUrl": base_url,
        "registryName": registry_name,
        "workspaceDefaultRegistryUrl": workspace_default_url.map(|u| normalize_base_url(&u)),
        "workspaceDefaultRegistryName": workspace_default_name,
        "authConfigured": auth_configured,
        "validation": validation,
        "connected": connected,
    }))
}

async fn set_registry(arguments: Option<Vec<Value>>, state: &Arc<RwLock<PckgRegistryState>>) -> Result<()> {
    let args = first_arg_object(&arguments).ok_or_else(missing_args)?;
    let base_url = non_empty_str_arg(args, "baseUrl").ok_or_else(missing_args)?;
    let registry_name = non_empty_str_arg(args, "registryName").map(str::to_string);

    let mut guard = state.write().await;
    guard.registry_base_url = Some(normalize_base_url(base_url));
    guard.registry_name = registry_name;
    guard.validation_status = ValidationStatus::Unknown;
    guard.validation_message = None;
    Ok(())
}

async fn validate_connection(
    arguments: Option<Vec<Value>>,
    workspace_roots: &[PathBuf],
    state: &Arc<RwLock<PckgRegistryState>>,
) -> Result<Value> {
    let args = first_arg_object(&arguments);
    let api_key = args
        .and_then(|o| o.get("apiKey"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let workspace_uri = args
        .and_then(|o| o.get("workspaceUri"))
        .and_then(Value::as_str);

    let (workspace_default_url, _) =
        resolve_workspace_default_registry(workspace_uri, workspace_roots);

    let base_url = {
        let guard = state.read().await;
        args.and_then(|o| non_empty_str_arg(o, "baseUrl").map(str::to_string))
            .or_else(|| guard.registry_base_url.clone())
            .or(workspace_default_url)
            .map(|u| normalize_base_url(&u))
            .filter(|u| !u.is_empty())
    };

    let Some(base_url) = base_url else {
        let mut guard = state.write().await;
        guard.validation_status = ValidationStatus::Error;
        guard.validation_message = Some("No registry base URL configured.".to_string());
        return Ok(validation_result(false, &guard.validation_message));
    };

    let result = tokio::task::spawn_blocking(move || probe_registry(&base_url, api_key.as_deref()))
        .await
        .map_err(|_| missing_args())?;

    let mut guard = state.write().await;
    match result {
        Ok(()) => {
            guard.validation_status = ValidationStatus::Ok;
            guard.validation_message = None;
            Ok(validation_result(true, &None))
        }
        Err(message) => {
            guard.validation_status = ValidationStatus::Error;
            guard.validation_message = Some(message.clone());
            Ok(validation_result(false, &Some(message)))
        }
    }
}

fn validation_result(ok: bool, message: &Option<String>) -> Value {
    json!({
        "ok": ok,
        "error": message,
        "validation": {
            "status": if ok { "ok" } else { "error" },
            "message": message,
        },
    })
}

fn probe_registry(base_url: &str, api_key: Option<&str>) -> std::result::Result<(), String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let url = format!(
        "{}/api/search",
        base_url.trim_end_matches('/')
    );
    let mut request = client
        .get(&url)
        .query(&[("limit", "1")])
        .header("Accept", "application/json");

    if let Some(key) = api_key {
        let value = format!("Bearer {key}");
        let header = HeaderValue::from_str(&value)
            .map_err(|_| "Invalid API key format.".to_string())?;
        request = request.header(AUTHORIZATION, header);
    }

    let response = request
        .send()
        .map_err(|e| format!("Could not reach registry: {e}"))?;

    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err("Registry rejected credentials (HTTP 401/403).".to_string());
    }
    Err(format!("Registry returned HTTP {}.", status.as_u16()))
}

fn resolve_workspace_default_registry(
    workspace_uri: Option<&str>,
    workspace_roots: &[PathBuf],
) -> (Option<String>, Option<String>) {
    if let Some(uri) = workspace_uri
        && let Some(path) = path_from_uri_string(uri)
            && let Some(pair) = default_from_workspace_manifest(&path) {
                return pair;
            }
    for manifest in discover_workspace_manifest_paths(workspace_roots) {
        if let Some(pair) = default_from_workspace_manifest(&manifest)
            && pair.0.is_some() {
                return pair;
            }
    }
    (None, None)
}

fn default_from_workspace_manifest(manifest_path: &Path) -> Option<(Option<String>, Option<String>)> {
    let text = std::fs::read_to_string(manifest_path).ok()?;
    let manifest = parse_workspace_manifest(&text).ok()?;
    let pick = manifest
        .registries
        .iter()
        .find(|r| r.name == "default")
        .or_else(|| manifest.registries.first());
    let registry = pick?;
    Some((Some(registry.url.clone()), Some(registry.name.clone())))
}

fn normalize_base_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

fn validation_status_str(status: &ValidationStatus) -> &'static str {
    match status {
        ValidationStatus::Unknown => "unknown",
        ValidationStatus::Ok => "ok",
        ValidationStatus::Error => "error",
    }
}

fn requires_auth_hint(message: &Option<String>) -> bool {
    message
        .as_deref()
        .map(|m| m.contains("401") || m.contains("403") || m.contains("credentials"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::execute_args::find_forbidden_secret_keys;

    #[test]
    fn validation_result_never_includes_api_key_field() {
        let value = validation_result(false, &Some("HTTP 401".to_string()));
        assert!(value.get("apiKey").is_none());
        assert!(find_forbidden_secret_keys(&value).is_empty());
    }

    #[tokio::test]
    async fn get_connection_status_response_has_no_secrets() {
        let state = Arc::new(RwLock::new(PckgRegistryState::default()));
        let value = get_connection_status(
            Some(vec![json!({ "authConfigured": true })]),
            &[],
            &state,
        )
        .await
        .expect("status");
        assert!(value.get("apiKey").is_none());
        assert!(find_forbidden_secret_keys(&value).is_empty());
        assert!(value.get("baseUrl").is_some());
        assert!(value.get("validation").is_some());
    }

    #[tokio::test]
    async fn validate_connection_response_has_no_secrets() {
        let state = Arc::new(RwLock::new(PckgRegistryState::default()));
        let value = validate_connection(
            Some(vec![json!({ "apiKey": "super-secret", "baseUrl": "http://127.0.0.1:1" })]),
            &[],
            &state,
        )
        .await
        .expect("validate");
        assert!(value.get("apiKey").is_none());
        assert!(find_forbidden_secret_keys(&value).is_empty());
    }

    #[test]
    fn normalize_base_url_strips_trailing_slash() {
        assert_eq!(normalize_base_url("https://pckg.test/"), "https://pckg.test");
    }

    #[test]
    fn default_from_workspace_manifest_prefers_default_registry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("Workspace.proj");
        std::fs::write(
            &path,
            r#"
workspace {
  name = "W"
  resolver = "v1"
}

registry "mirror" {
  url = "https://mirror.test"
}

registry "default" {
  url = "https://pckg.test"
}
"#,
        )
        .expect("write");
        let (url, name) = default_from_workspace_manifest(&path).expect("parsed");
        assert_eq!(url.as_deref(), Some("https://pckg.test"));
        assert_eq!(name.as_deref(), Some("default"));
    }
}
