//! `.bpk` source collection and readme injection for registry artifacts.

use std::fs;
use std::io;
use std::path::Path;

use beskid_analysis::projects::{
    PACKAGE_README_ARTIFACT_NAME, ProjectKind, discover_readme_for_package_root,
    is_package_root_readme_entry, parse_manifest, resolve_readme_file_path,
};
use serde_json::{Value, json};
use walkdir::WalkDir;
use zip::result::ZipError;

use crate::PckgError;

/// Relative path to the normative template manifest inside a template package tree.
pub const TEMPLATE_JSON_REL: &str = ".beskid/template.json";

/// `package.json` discriminator for scaffold packages (see platform-spec template packages).
pub const PACKAGE_KIND_TEMPLATE: &str = "template";

/// `package.json` discriminator for tool packages (see platform-spec package kinds, D-TOOL-PCKG-0004).
pub const PACKAGE_KIND_TOOL: &str = "tool";

/// `package.json` discriminator for library packages (the implicit default).
pub const PACKAGE_KIND_LIBRARY: &str = "library";

/// Summary copied from `.beskid/template.json` into packed `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatePackageSummary {
    pub short_name: Option<String>,
    pub identity: Option<String>,
    pub tags: Option<Value>,
}

/// Whether the pack uses library docs (`api.json`), the template profile, or the tool profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackProfile {
    Library,
    Template(TemplatePackageSummary),
    /// CLI / developer-tool package — no required `api.json`, no `.beskid/template.json`.
    Tool,
}

impl PackProfile {
    pub fn is_template(&self) -> bool {
        matches!(self, Self::Template(_))
    }

    pub fn is_tool(&self) -> bool {
        matches!(self, Self::Tool)
    }
}

/// CLI-supplied override for [`detect_pack_profile_with_override`].
///
/// `Auto` reproduces the manifest-driven detection used before the `tool` packageKind landed
/// (D-TOOL-PCKG-0004). `Tool` forces the tool profile even when the source tree omits
/// `Project.proj`, which keeps `beskid pckg pack --package-kind tool` usable for CLI-only
/// tool packages that ship without a normative project manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackProfileOverride {
    Auto,
    Tool,
}

impl Default for PackProfileOverride {
    fn default() -> Self {
        Self::Auto
    }
}

/// Resolve pack profile from `Project.proj` when present; otherwise library.
pub fn detect_pack_profile(source_root: &Path) -> Result<PackProfile, PckgError> {
    detect_pack_profile_with_override(source_root, PackProfileOverride::Auto)
}

/// Resolve pack profile honoring an explicit CLI override.
///
/// * `Auto` matches [`detect_pack_profile`] — `Project.proj` selects template vs library, with
///   no manifest defaulting to library.
/// * `Tool` selects [`PackProfile::Tool`] unconditionally, but still rejects template projects
///   so we never silently drop a `.beskid/template.json` payload at pack time.
pub fn detect_pack_profile_with_override(
    source_root: &Path,
    override_kind: PackProfileOverride,
) -> Result<PackProfile, PckgError> {
    let manifest_path = source_root.join("Project.proj");

    let manifest = if manifest_path.is_file() {
        let source = fs::read_to_string(&manifest_path).map_err(|err| PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!(
                "failed to read Project.proj at {}: {err}",
                manifest_path.display()
            ),
            body: None,
        })?;

        Some(parse_manifest(&source).map_err(|err| PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!("invalid Project.proj: {err}"),
            body: None,
        })?)
    } else {
        None
    };

    if override_kind == PackProfileOverride::Tool {
        if let Some(manifest) = manifest.as_ref()
            && manifest.project.kind == ProjectKind::Template
        {
            return Err(PckgError::Api {
                status: reqwest::StatusCode::BAD_REQUEST,
                message: format!(
                    "--package-kind tool conflicts with template project at {}: refuse to drop \
                     `{TEMPLATE_JSON_REL}` from a template artifact",
                    source_root.display()
                ),
                body: None,
            });
        }
        return Ok(PackProfile::Tool);
    }

    let Some(manifest) = manifest else {
        return Ok(PackProfile::Library);
    };

    if manifest.project.kind != ProjectKind::Template {
        return Ok(PackProfile::Library);
    }

    let template_path = source_root.join(TEMPLATE_JSON_REL);
    if !template_path.is_file() {
        return Err(PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!(
                "template project requires `{TEMPLATE_JSON_REL}` at {}",
                source_root.display()
            ),
            body: None,
        });
    }

    let summary = load_template_package_summary(&template_path)?;
    Ok(PackProfile::Template(summary))
}

pub fn load_template_package_summary(path: &Path) -> Result<TemplatePackageSummary, PckgError> {
    let bytes = fs::read(path).map_err(|err| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("failed to read {}: {err}", path.display()),
        body: None,
    })?;
    let root: Value = serde_json::from_slice(&bytes).map_err(|err| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("invalid `{TEMPLATE_JSON_REL}`: {err}"),
        body: None,
    })?;
    let schema = root
        .get("schema")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if schema != "beskid.template.v1" {
        return Err(PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!(
                "`{TEMPLATE_JSON_REL}` schema must be `beskid.template.v1`, found `{schema}`"
            ),
            body: None,
        });
    }

    Ok(TemplatePackageSummary {
        short_name: root
            .get("shortName")
            .and_then(Value::as_str)
            .map(str::to_string),
        identity: root
            .get("identity")
            .and_then(Value::as_str)
            .map(str::to_string),
        tags: root.get("tags").cloned(),
    })
}

pub fn template_summary_json(summary: &TemplatePackageSummary) -> Value {
    let mut obj = serde_json::Map::new();
    if let Some(short_name) = &summary.short_name {
        obj.insert("shortName".to_string(), json!(short_name));
    }
    if let Some(identity) = &summary.identity {
        obj.insert("identity".to_string(), json!(identity));
    }
    if let Some(tags) = &summary.tags {
        obj.insert("tags".to_string(), tags.clone());
    }
    Value::Object(obj)
}

pub fn build_package_json(
    package_id: &str,
    version: &str,
    profile: &PackProfile,
) -> Result<String, PckgError> {
    use crate::api_doc::API_JSON_SCHEMA_VERSION;

    let value = match profile {
        PackProfile::Library => json!({
            "schema": "beskid.package.v1",
            "id": package_id,
            "version": version,
            "documentation": {
                "apiJson": ".beskid/docs/api.json",
                "schemaVersion": API_JSON_SCHEMA_VERSION,
            },
        }),
        PackProfile::Template(summary) => json!({
            "schema": "beskid.package.v1",
            "id": package_id,
            "version": version,
            "packageKind": PACKAGE_KIND_TEMPLATE,
            "template": template_summary_json(summary),
        }),
        PackProfile::Tool => json!({
            "schema": "beskid.package.v1",
            "id": package_id,
            "version": version,
            "packageKind": PACKAGE_KIND_TOOL,
        }),
    };

    serde_json::to_string_pretty(&value).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("failed to serialize package.json: {source}"),
        body: None,
    })
}

/// Remove generated API docs from template artifacts (template profile skips doc generation).
pub fn strip_template_pack_excludes(entries: &mut Vec<(String, Vec<u8>)>) {
    entries.retain(|(name, _)| {
        !name.starts_with(".beskid/docs/")
            && name != ".beskid/docs/api.json"
            && name != ".beskid/docs/index.md"
    });
}

/// Remove generated API docs from tool artifacts (tool profile does not require api.json).
///
/// `api.json` is *optional* for tool packages per platform-spec, but `beskid pckg pack` strips
/// any prior generated docs to keep the artifact body lean unless the publisher explicitly opts in.
pub fn strip_tool_pack_excludes(entries: &mut Vec<(String, Vec<u8>)>) {
    entries.retain(|(name, _)| {
        !name.starts_with(".beskid/docs/")
            && name != ".beskid/docs/api.json"
            && name != ".beskid/docs/index.md"
    });
}

pub fn collect_pack_entries(source_root: &Path) -> Result<Vec<(String, Vec<u8>)>, PckgError> {
    let mut entries = collect_pack_entries_from_tree(source_root)?;
    apply_pack_readme(source_root, &mut entries)?;
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries)
}

fn collect_pack_entries_from_tree(source_root: &Path) -> Result<Vec<(String, Vec<u8>)>, PckgError> {
    let mut entries = Vec::new();

    for entry in WalkDir::new(source_root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let rel_path = path.strip_prefix(source_root).map_err(io::Error::other)?;
        let rel = normalize_rel_path(rel_path);

        if rel == "checksums.sha256" || rel == "package.json" {
            continue;
        }

        let bytes = fs::read(path)?;
        entries.push((rel, bytes));
    }

    Ok(entries)
}

/// Ensure the packed artifact exposes a root `README.md` entry for pckg when a readme is configured.
pub fn apply_pack_readme(
    source_root: &Path,
    entries: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), PckgError> {
    let manifest = discover_readme_for_package_root(source_root).map_err(|err| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("failed to read project manifest for readme: {err}"),
        body: None,
    })?;

    let Some(relative) = manifest else {
        return Ok(());
    };

    let readme_path = resolve_readme_file_path(source_root, &relative);
    if !readme_path.is_file() {
        return Err(PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!(
                "readme file `{}` does not exist or is not a file",
                readme_path.display()
            ),
            body: None,
        });
    }

    let bytes = fs::read(&readme_path)?;
    let normalized = normalize_rel_path(Path::new(&relative));

    if !is_package_root_readme_entry(&normalized) {
        entries.retain(|(name, _)| !name.eq_ignore_ascii_case(PACKAGE_README_ARTIFACT_NAME));
        entries.push((PACKAGE_README_ARTIFACT_NAME.to_string(), bytes.clone()));
    }

    Ok(())
}

pub fn normalize_rel_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub fn zip_to_pckg_error(source: ZipError) -> PckgError {
    PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("zip packaging error: {source}"),
        body: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn template_summary_json_copies_expected_keys() {
        let summary = TemplatePackageSummary {
            short_name: Some("console".into()),
            identity: Some("beskid.templates.console".into()),
            tags: Some(json!({ "type": "project" })),
        };
        let value = template_summary_json(&summary);
        assert_eq!(value["shortName"], "console");
        assert_eq!(value["identity"], "beskid.templates.console");
        assert_eq!(value["tags"]["type"], "project");
    }

    #[test]
    fn build_package_json_template_profile_omits_api_doc_pointer() {
        let summary = TemplatePackageSummary {
            short_name: Some("lib".into()),
            identity: Some("beskid.templates.lib".into()),
            tags: None,
        };
        let json = build_package_json(
            "beskid.templates.lib",
            "1.0.0",
            &PackProfile::Template(summary),
        )
        .expect("serialize");
        let root: Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(root["packageKind"], PACKAGE_KIND_TEMPLATE);
        assert_eq!(root["template"]["shortName"], "lib");
        assert!(root.get("documentation").is_none());
    }

    #[test]
    fn strip_template_pack_excludes_removes_beskid_docs_tree() {
        let mut entries = vec![
            (".beskid/template.json".into(), vec![]),
            (".beskid/docs/api.json".into(), vec![1]),
            ("src/Main.bd".into(), vec![]),
        ];
        strip_template_pack_excludes(&mut entries);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|(n, _)| n == ".beskid/template.json"));
        assert!(entries.iter().any(|(n, _)| n == "src/Main.bd"));
    }

    #[test]
    fn load_template_package_summary_requires_v1_schema() {
        let dir = std::env::temp_dir().join(format!(
            "beskid_pckg_template_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".beskid")).expect("mkdir");
        fs::write(
            dir.join(TEMPLATE_JSON_REL),
            r#"{"schema":"beskid.template.v0","shortName":"x"}"#,
        )
        .expect("write");
        let err = load_template_package_summary(&dir.join(TEMPLATE_JSON_REL))
            .expect_err("wrong schema");
        assert!(err.to_string().contains("beskid.template.v1"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_profile_helpers_track_variant() {
        let summary = TemplatePackageSummary {
            short_name: None,
            identity: None,
            tags: None,
        };
        let library = PackProfile::Library;
        let template = PackProfile::Template(summary);
        let tool = PackProfile::Tool;

        assert!(!library.is_template());
        assert!(!library.is_tool());
        assert!(template.is_template());
        assert!(!template.is_tool());
        assert!(!tool.is_template());
        assert!(tool.is_tool());
    }

    #[test]
    fn build_package_json_tool_profile_omits_api_doc_pointer() {
        let json = build_package_json("beskid.cli.fmt-extra", "0.1.0", &PackProfile::Tool)
            .expect("serialize");
        let root: Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(root["schema"], "beskid.package.v1");
        assert_eq!(root["packageKind"], PACKAGE_KIND_TOOL);
        assert!(root.get("documentation").is_none());
        assert!(root.get("template").is_none());
    }

    #[test]
    fn strip_tool_pack_excludes_removes_generated_docs() {
        let mut entries: Vec<(String, Vec<u8>)> = vec![
            (".beskid/docs/api.json".into(), vec![0xAB]),
            (".beskid/docs/index.md".into(), vec![0xCD]),
            (".beskid/docs/types/Foo.md".into(), vec![]),
            ("bin/beskid-fmt-extra".into(), vec![0xEF]),
            ("README.md".into(), vec![]),
        ];
        strip_tool_pack_excludes(&mut entries);
        let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        assert!(!names.iter().any(|n| n.starts_with(".beskid/docs/")));
        assert!(names.contains(&"bin/beskid-fmt-extra"));
        assert!(names.contains(&"README.md"));
    }

    #[test]
    fn detect_pack_profile_with_override_forces_tool_when_no_manifest() {
        let dir = std::env::temp_dir().join(format!(
            "beskid_pckg_tool_no_manifest_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");

        let profile = detect_pack_profile_with_override(&dir, PackProfileOverride::Tool)
            .expect("tool override succeeds without manifest");
        assert!(profile.is_tool());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_pack_profile_with_override_rejects_template_project() {
        let dir = std::env::temp_dir().join(format!(
            "beskid_pckg_tool_vs_template_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".beskid")).expect("mkdir");
        fs::write(
            dir.join("Project.proj"),
            r#"project {
  name = "Tpl"
  version = "0.1.0"
  type = Template
  template {
    shortName = "tpl"
    identity  = "beskid.test.tpl"
  }
}
"#,
        )
        .expect("write proj");
        fs::write(
            dir.join(TEMPLATE_JSON_REL),
            r#"{"schema":"beskid.template.v1","shortName":"x"}"#,
        )
        .expect("write template.json");

        let err = detect_pack_profile_with_override(&dir, PackProfileOverride::Tool)
            .expect_err("template + tool override must conflict");
        assert!(
            err.to_string().contains("--package-kind tool"),
            "error mentions the conflicting flag: {err}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_pack_profile_auto_matches_legacy_behavior() {
        let dir = std::env::temp_dir().join(format!(
            "beskid_pckg_auto_no_manifest_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");

        let profile = detect_pack_profile_with_override(&dir, PackProfileOverride::Auto)
            .expect("library profile without manifest");
        assert!(matches!(profile, PackProfile::Library));

        let _ = fs::remove_dir_all(&dir);
    }
}
