//! Native Mod host selection from LSP initialization options.
//!
//! Preparing a project runs its native Mods (Corelib depends on the Serialization Mod). The
//! server never builds Mods; it runs qualified artifacts in a Mod host executable, by default the
//! installed `beskid` CLI next to this server. Like rust-analyzer's `procMacro.server`, an editor
//! or test harness can select the Mod host explicitly:
//!
//! ```json
//! { "modHost": { "path": "/opt/beskid/bin/beskid", "runtimePrefix": "/opt/beskid", "runtimeProfile": "release" } }
//! ```
//!
//! Every key is optional, but the object must contain at least one:
//!
//! - `path` selects the Mod host executable: an absolute path to an executable file.
//! - `runtimePrefix` selects the runtime kit that Mod artifacts are qualified and loaded against:
//!   an absolute installation prefix that holds an exact runtime kit for the native host target.
//! - `runtimeProfile` (`"debug"` or `"release"`) selects the kit profile; it requires
//!   `runtimePrefix` and defaults to `"debug"`.
//!
//! The selection is validated at `initialize`; an invalid selection fails initialization instead
//! of falling back to the installed toolchain. Artifact qualification applies to the selected host
//! and kit exactly as to the installed ones. Without `runtimePrefix` the kit is the one the server
//! process resolves (`BESKID_RUNTIME_PREFIX`, else its installation prefix), read only when a Mod
//! must run.

use std::path::Path;

use anyhow::{Context, Result, bail};
use beskid_analysis::mod_host::{RuntimeKitProfile as BuildProfile, RuntimeKitSelection};
use beskid_tools::native_mods::ModHostSelection;
use serde_json::Value;

/// Initialization-option key that selects the native Mod host.
pub const MOD_HOST_OPTION: &str = "modHost";

const KEYS: [&str; 3] = ["path", "runtimePrefix", "runtimeProfile"];

/// Parse and validate the `modHost` initialization option. `Ok(None)` when the option is absent.
pub fn mod_host_from_initialization_options(options: &Value) -> Result<Option<ModHostSelection>> {
    let Some(value) = options.get(MOD_HOST_OPTION) else {
        return Ok(None);
    };
    let Some(object) = value.as_object() else {
        bail!("`{MOD_HOST_OPTION}` must be an object with `path`, `runtimePrefix` or `runtimeProfile`");
    };
    if let Some(unknown) = object.keys().find(|key| !KEYS.contains(&key.as_str())) {
        bail!("`{MOD_HOST_OPTION}.{unknown}` is not a Mod host setting; expected `path`, `runtimePrefix` or `runtimeProfile`");
    }
    if object.is_empty() {
        bail!("`{MOD_HOST_OPTION}` must set `path`, `runtimePrefix` or both");
    }
    let mut selection = match object.get("path") {
        None => ModHostSelection::installed(),
        Some(value) => ModHostSelection::explicit(Path::new(non_empty_string(value, "path")?))
            .with_context(|| format!("`{MOD_HOST_OPTION}.path`"))?,
    };
    let profile = match object.get("runtimeProfile") {
        None => None,
        Some(value) => Some(match non_empty_string(value, "runtimeProfile")? {
            "debug" => BuildProfile::Debug,
            "release" => BuildProfile::Release,
            other => bail!("`{MOD_HOST_OPTION}.runtimeProfile` must be `debug` or `release`, got `{other}`"),
        }),
    };
    match (object.get("runtimePrefix"), profile) {
        (Some(value), profile) => {
            let prefix = Path::new(non_empty_string(value, "runtimePrefix")?);
            let runtime = RuntimeKitSelection::explicit(prefix, profile.unwrap_or(BuildProfile::Debug))
                .with_context(|| format!("`{MOD_HOST_OPTION}.runtimePrefix`"))?;
            selection = selection.with_runtime_kit(runtime);
        }
        (None, Some(_)) => bail!("`{MOD_HOST_OPTION}.runtimeProfile` requires `{MOD_HOST_OPTION}.runtimePrefix`"),
        (None, None) => {}
    }
    Ok(Some(selection))
}

fn non_empty_string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Ok(text),
        _ => bail!("`{MOD_HOST_OPTION}.{key}` must be a non-empty string"),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::mod_host_from_initialization_options;

    #[test]
    fn absent_option_keeps_the_installed_mod_host() {
        assert_eq!(mod_host_from_initialization_options(&json!({"logLevel": "info"})).expect("parse"), None);
    }

    #[test]
    fn existing_executable_is_selected() {
        let executable = std::env::current_exe().expect("test executable");
        let selection = mod_host_from_initialization_options(&json!({"modHost": {"path": executable}}))
            .expect("valid selection")
            .expect("selection present");
        assert_eq!(selection.executable(), Some(executable.canonicalize().expect("canonical").as_path()));
        assert_eq!(selection.runtime_kit(), None, "without `runtimePrefix` the installed kit is resolved later");
    }

    #[test]
    fn kitless_runtime_prefix_names_the_option_and_the_missing_kit() {
        let kitless = tempfile::tempdir().expect("prefix without a runtime kit");
        let error = mod_host_from_initialization_options(&json!({"modHost": {"runtimePrefix": kitless.path()}}))
            .expect_err("a prefix without an exact kit fails closed");
        let message = format!("{error:#}");
        assert!(message.contains("modHost.runtimePrefix") && message.contains("holds no exact"), "{message}");
    }

    #[test]
    fn invalid_selection_fails_closed() {
        let missing = std::env::temp_dir().join("beskid-lsp-missing-mod-host-executable");
        let kitless = tempfile::tempdir().expect("prefix without a runtime kit");
        for options in [
            json!({"modHost": "beskid"}),
            json!({"modHost": {}}),
            json!({"modHost": {"path": 7}}),
            json!({"modHost": {"path": ""}}),
            json!({"modHost": {"path": "relative/beskid"}}),
            json!({"modHost": {"path": missing}}),
            json!({"modHost": {"server": "/usr/bin/beskid"}}),
            json!({"modHost": {"runtimePrefix": ""}}),
            json!({"modHost": {"runtimePrefix": 3}}),
            json!({"modHost": {"runtimePrefix": "relative/prefix"}}),
            json!({"modHost": {"runtimePrefix": missing}}),
            json!({"modHost": {"runtimePrefix": kitless.path()}}),
            json!({"modHost": {"runtimeProfile": "debug"}}),
            json!({"modHost": {"runtimePrefix": kitless.path(), "runtimeProfile": "fast"}}),
        ] {
            assert!(mod_host_from_initialization_options(&options).is_err(), "must reject {options}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_executable_file_is_rejected() {
        let directory = tempfile::tempdir().expect("directory");
        let file = directory.path().join("beskid");
        std::fs::write(&file, "not a program").expect("file");
        let options = json!({"modHost": {"path": file}});
        let error = mod_host_from_initialization_options(&options).expect_err("non-executable file");
        assert!(format!("{error:#}").contains("not executable"), "{error:#}");
    }
}
