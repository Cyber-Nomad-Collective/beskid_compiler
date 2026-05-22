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

/// Summary copied from `.beskid/template.json` into packed `package.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplatePackageSummary {
    pub short_name: Option<String>,
    pub identity: Option<String>,
    pub tags: Option<Value>,
}

/// Whether the pack uses library docs (`api.json`) or the template profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackProfile {
    Library,
    Template(TemplatePackageSummary),
}

impl PackProfile {
    pub fn is_template(&self) -> bool {
        matches!(self, Self::Template(_))
    }
}

/// Resolve pack profile from `Project.proj` when present; otherwise library.
pub fn detect_pack_profile(source_root: &Path) -> Result<PackProfile, PckgError> {
    let manifest_path = source_root.join("Project.proj");
    if !manifest_path.is_file() {
        return Ok(PackProfile::Library);
    }

    let source = fs::read_to_string(&manifest_path).map_err(|err| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!(
            "failed to read Project.proj at {}: {err}",
            manifest_path.display()
        ),
        body: None,
    })?;

    let manifest = parse_manifest(&source).map_err(|err| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("invalid Project.proj: {err}"),
        body: None,
    })?;

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
}
