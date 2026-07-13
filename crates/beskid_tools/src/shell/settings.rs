//! Tool settings registry and BSOL config persistence.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use bsol::{ValidatedDocument, load_profile, parse_bsol_document, validate};

use super::scope::ShellScope;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingKind {
    Bool,
    U32,
    Quoted,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolSettingDescriptor {
    pub key: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub kind: SettingKind,
    pub default: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct ToolSettingsPage {
    pub tool_id: &'static str,
    pub title: &'static str,
    pub settings: &'static [ToolSettingDescriptor],
}

const SHELL_SETTINGS: &[ToolSettingDescriptor] = &[
    ToolSettingDescriptor {
        key: "autosave_layout",
        label: "Autosave layout",
        description: "Save layout edits after debounce",
        kind: SettingKind::Bool,
        default: "true",
    },
    ToolSettingDescriptor {
        key: "layout_edit_debounce_ms",
        label: "Layout debounce (ms)",
        description: "Milliseconds before autosaving layout edits",
        kind: SettingKind::U32,
        default: "500",
    },
];

const PCKG_SETTINGS: &[ToolSettingDescriptor] = &[
    ToolSettingDescriptor {
        key: "base_url",
        label: "Registry base URL",
        description: "Default pckg registry endpoint",
        kind: SettingKind::Quoted,
        default: "https://pckg.beskid-lang.org",
    },
    ToolSettingDescriptor {
        key: "cache_enabled",
        label: "Cache packages",
        description: "Keep downloaded packages on disk",
        kind: SettingKind::Bool,
        default: "true",
    },
];

const TEMPLATES_SETTINGS: &[ToolSettingDescriptor] = &[
    ToolSettingDescriptor {
        key: "registry_url",
        label: "Template registry URL",
        description: "Source for project templates",
        kind: SettingKind::Quoted,
        default: "https://pckg.beskid-lang.org",
    },
    ToolSettingDescriptor {
        key: "confirm_overwrite",
        label: "Confirm overwrite",
        description: "Prompt before overwriting existing directories",
        kind: SettingKind::Bool,
        default: "true",
    },
];

/// Tool page with no scalar settings; shortcut rebinding is handled in the settings widget.
const SHORTCUTS_SETTINGS: &[ToolSettingDescriptor] = &[];

pub const BUILTIN_SETTINGS: &[ToolSettingsPage] = &[
    ToolSettingsPage {
        tool_id: "shell",
        title: "Shell",
        settings: SHELL_SETTINGS,
    },
    ToolSettingsPage {
        tool_id: "pckg",
        title: "Packages",
        settings: PCKG_SETTINGS,
    },
    ToolSettingsPage {
        tool_id: "templates",
        title: "Templates",
        settings: TEMPLATES_SETTINGS,
    },
    ToolSettingsPage {
        tool_id: "shortcuts",
        title: "Shortcuts",
        settings: SHORTCUTS_SETTINGS,
    },
];

pub type ToolSettingsRegistrar = fn(&mut ToolSettingsRegistry);

#[derive(Debug, Clone, Default)]
pub struct ToolSettingsRegistry {
    pages: Vec<ToolSettingsPage>,
}

impl ToolSettingsRegistry {
    pub fn with_builtins() -> Self {
        let mut registry = Self::default();
        registry.register_pages(BUILTIN_SETTINGS);
        registry
    }

    pub fn register_pages(&mut self, pages: &[ToolSettingsPage]) {
        for page in pages {
            if !self.pages.iter().any(|p| p.tool_id == page.tool_id) {
                self.pages.push(*page);
            }
        }
    }

    pub fn pages(&self) -> &[ToolSettingsPage] {
        &self.pages
    }

    pub fn page(&self, tool_id: &str) -> Option<&ToolSettingsPage> {
        self.pages.iter().find(|p| p.tool_id == tool_id)
    }

    pub fn descriptor(&self, tool_id: &str, key: &str) -> Option<&ToolSettingDescriptor> {
        self.page(tool_id)?.settings.iter().find(|s| s.key == key)
    }

    pub fn default_value(&self, tool_id: &str, key: &str) -> Option<&str> {
        self.descriptor(tool_id, key).map(|d| d.default)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolsConfig {
    pub version: u32,
    values: HashMap<(String, String), String>,
}

impl ToolsConfig {
    pub fn merge(&mut self, other: &ToolsConfig) {
        if other.version != 0 {
            self.version = other.version;
        }
        for (k, v) in &other.values {
            self.values.insert(k.clone(), v.clone());
        }
    }
}

pub fn user_config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".beskid")
        .join("config")
        .join("tools.bsol")
}

pub fn scope_config_path(scope: &ShellScope) -> Option<PathBuf> {
    scope
        .root_dir()
        .map(|root| root.join(".beskid").join("tools.bsol"))
}

pub fn save_path_for_scope(scope: &ShellScope) -> PathBuf {
    scope_config_path(scope).unwrap_or_else(user_config_path)
}

pub fn load_config(scope: &ShellScope, registry: &ToolSettingsRegistry) -> ToolsConfig {
    let mut config = ToolsConfig {
        version: 1,
        ..ToolsConfig::default()
    };

    if user_config_path().is_file()
        && let Ok(parsed) = load_from_path(&user_config_path())
    {
        config.merge(&parsed);
    }

    if let Some(path) = scope_config_path(scope)
        && path.is_file()
        && let Ok(parsed) = load_from_path(&path)
    {
        config.merge(&parsed);
    }

    apply_defaults(&mut config, registry);
    config
}

pub fn save_config(scope: &ShellScope, config: &ToolsConfig) -> Result<(), String> {
    let path = save_path_for_scope(scope);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, emit_config(config)).map_err(|e| e.to_string())
}

pub fn get_value(
    config: &ToolsConfig,
    registry: &ToolSettingsRegistry,
    tool_id: &str,
    key: &str,
) -> String {
    config
        .values
        .get(&(tool_id.to_string(), key.to_string()))
        .cloned()
        .or_else(|| registry.default_value(tool_id, key).map(str::to_string))
        .unwrap_or_default()
}

pub fn set_value(config: &mut ToolsConfig, tool_id: &str, key: &str, value: String) {
    config
        .values
        .insert((tool_id.to_string(), key.to_string()), value);
}

fn apply_defaults(config: &mut ToolsConfig, registry: &ToolSettingsRegistry) {
    for page in registry.pages() {
        for desc in page.settings {
            let key = (page.tool_id.to_string(), desc.key.to_string());
            config
                .values
                .entry(key)
                .or_insert_with(|| desc.default.to_string());
        }
    }
}

fn load_from_path(path: &Path) -> Result<ToolsConfig, String> {
    let source = fs::read_to_string(path).map_err(|e| e.to_string())?;
    parse_config(&source)
}

pub fn parse_config(source: &str) -> Result<ToolsConfig, String> {
    let document = parse_bsol_document(source).map_err(|e| e.to_string())?;
    let profile = load_profile("tools.config.v1").map_err(|e| e.to_string())?;
    let validated = validate(&document, &profile).map_err(|e| e.to_string())?;
    lower_config(validated)
}

fn lower_config(doc: ValidatedDocument) -> Result<ToolsConfig, String> {
    let mut config = ToolsConfig::default();
    for block in &doc.blocks {
        match block.rule_id.as_str() {
            "config" => {
                if let Some(v) = block.fields.get("version") {
                    config.version = v.parse().unwrap_or(1);
                }
            }
            "setting" => {
                let tool_id = block
                    .fields
                    .get("tool_id")
                    .cloned()
                    .ok_or("setting missing tool_id")?;
                let key = block
                    .fields
                    .get("key")
                    .cloned()
                    .ok_or("setting missing key")?;
                let value = block
                    .fields
                    .get("value")
                    .cloned()
                    .ok_or("setting missing value")?;
                config.values.insert((tool_id, key), value);
            }
            other => return Err(format!("unexpected tools.config.v1 block `{other}`")),
        }
    }
    if config.version == 0 {
        config.version = 1;
    }
    Ok(config)
}

pub fn emit_config(config: &ToolsConfig) -> String {
    let mut out = String::from("config {\n");
    out.push_str(&format!("  version = {}\n", config.version.max(1)));
    out.push_str("}\n");

    let mut keys: Vec<_> = config.values.keys().collect();
    keys.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));

    for (tool_id, key) in keys {
        if let Some(value) = config.values.get(&(tool_id.clone(), key.clone())) {
            out.push_str("setting {\n");
            out.push_str(&format!("  tool_id = \"{}\"\n", escape(tool_id)));
            out.push_str(&format!("  key = \"{}\"\n", escape(key)));
            out.push_str(&format!("  value = \"{}\"\n", escape(value)));
            out.push_str("}\n");
        }
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_emit_roundtrip() {
        let registry = ToolSettingsRegistry::with_builtins();
        let source = r#"config {
  version = 1
}
setting {
  tool_id = "shell"
  key = "autosave_layout"
  value = "false"
}
"#;
        let parsed = parse_config(source).expect("parse");
        assert_eq!(
            get_value(&parsed, &registry, "shell", "autosave_layout"),
            "false"
        );
        let again = parse_config(&emit_config(&parsed)).expect("re-parse");
        assert_eq!(parsed, again);
    }

    #[test]
    fn defaults_fill_missing_keys() {
        let registry = ToolSettingsRegistry::with_builtins();
        let config = load_config(&ShellScope::User, &registry);
        assert_eq!(
            get_value(&config, &registry, "pckg", "base_url"),
            "https://pckg.beskid-lang.org"
        );
    }

    #[test]
    fn tools_config_profile_loads() {
        load_profile("tools.config.v1").expect("profile");
    }
}
