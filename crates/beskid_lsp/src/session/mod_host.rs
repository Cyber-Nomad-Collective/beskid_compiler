//! Native Mod host selection from LSP initialization options.
//!
//! Preparing a project runs its native Mods (Corelib depends on the Serialization Mod). The
//! server never builds Mods; it runs qualified artifacts in a Mod host executable, by default the
//! installed `beskid` CLI next to this server. Like rust-analyzer's `procMacro.server`, an editor
//! or test harness can select the Mod host explicitly:
//!
//! ```json
//! { "modHost": { "path": "/opt/beskid/bin/beskid" } }
//! ```
//!
//! The path must be absolute and name an executable file. The selection is validated at
//! `initialize`; an invalid selection fails initialization instead of falling back to the
//! installed toolchain. Artifact qualification applies to the selected host exactly as to the
//! installed one. The runtime kit is the one the server process resolves (`BESKID_RUNTIME_PREFIX`,
//! else its installation prefix).

use std::path::Path;

use anyhow::{Result, bail};
use beskid_tools::native_mods::ModHostSelection;
use serde_json::Value;

/// Initialization-option key that selects the native Mod host.
pub const MOD_HOST_OPTION: &str = "modHost";

/// Parse and validate the `modHost` initialization option. `Ok(None)` when the option is absent.
pub fn mod_host_from_initialization_options(options: &Value) -> Result<Option<ModHostSelection>> {
    let Some(value) = options.get(MOD_HOST_OPTION) else {
        return Ok(None);
    };
    let Some(object) = value.as_object() else {
        bail!("`{MOD_HOST_OPTION}` must be an object with a `path` string");
    };
    if let Some(unknown) = object.keys().find(|key| key.as_str() != "path") {
        bail!("`{MOD_HOST_OPTION}.{unknown}` is not a Mod host setting; expected `path`");
    }
    match object.get("path") {
        Some(Value::String(path)) if !path.trim().is_empty() => Ok(Some(ModHostSelection::explicit(Path::new(path))?)),
        _ => bail!("`{MOD_HOST_OPTION}.path` must be a non-empty path string"),
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
    }

    #[test]
    fn invalid_selection_fails_closed() {
        let missing = std::env::temp_dir().join("beskid-lsp-missing-mod-host-executable");
        for options in [
            json!({"modHost": "beskid"}),
            json!({"modHost": {}}),
            json!({"modHost": {"path": 7}}),
            json!({"modHost": {"path": ""}}),
            json!({"modHost": {"path": "relative/beskid"}}),
            json!({"modHost": {"path": missing}}),
            json!({"modHost": {"server": "/usr/bin/beskid"}}),
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
