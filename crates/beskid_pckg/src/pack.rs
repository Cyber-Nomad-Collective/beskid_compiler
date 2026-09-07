//! `.bpk` source collection and readme injection for registry artifacts.

use std::fs;
use std::io;
use std::path::Path;

use beskid_analysis::projects::{
    PACKAGE_README_ARTIFACT_NAME, ProjectKind, discover_project_manifest_in_dir, discover_readme_for_package_root,
    is_package_root_readme_entry, parse_manifest, resolve_readme_file_path,
};
use beskid_pckg_artifacts::{ArtifactDependency, canonicalize_project_dependencies};
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
/// a `.bproj`, which keeps `beskid pckg pack --package-kind tool` usable for CLI-only
/// tool packages that ship without a normative project manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PackProfileOverride {
    #[default]
    Auto,
    Tool,
}

/// Resolve pack profile from the canonical `.bproj` manifest when present; otherwise library.
pub fn detect_pack_profile(source_root: &Path) -> Result<PackProfile, PckgError> {
    detect_pack_profile_with_override(source_root, PackProfileOverride::Auto)
}

/// Resolve pack profile honoring an explicit CLI override.
///
/// * `Auto` matches [`detect_pack_profile`] — a `.bproj` selects template vs library, with
///   no manifest defaulting to library.
/// * `Tool` selects [`PackProfile::Tool`] unconditionally, but still rejects template projects
///   so we never silently drop a `.beskid/template.json` payload at pack time.
pub fn detect_pack_profile_with_override(
    source_root: &Path,
    override_kind: PackProfileOverride,
) -> Result<PackProfile, PckgError> {
    let manifest_path = discover_project_manifest_in_dir(source_root).map_err(|err| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("failed to discover `.bproj` manifest in {}: {err}", source_root.display()),
        body: None,
    })?;

    let manifest = if let Some(manifest_path) = manifest_path {
        let source = fs::read_to_string(&manifest_path).map_err(|err| PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!("failed to read {}: {err}", manifest_path.display()),
            body: None,
        })?;

        Some(parse_manifest(&source).map_err(|err| PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!("invalid {}: {err}", manifest_path.display()),
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
            message: format!("template project requires `{TEMPLATE_JSON_REL}` at {}", source_root.display()),
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
    let schema = root.get("schema").and_then(Value::as_str).unwrap_or_default();
    if schema != "beskid.template.v1" {
        return Err(PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!("`{TEMPLATE_JSON_REL}` schema must be `beskid.template.v1`, found `{schema}`"),
            body: None,
        });
    }

    Ok(TemplatePackageSummary {
        short_name: root.get("shortName").and_then(Value::as_str).map(str::to_string),
        identity: root.get("identity").and_then(Value::as_str).map(str::to_string),
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
    has_api_docs: bool,
    dependencies: &[ArtifactDependency],
) -> Result<String, PckgError> {
    use crate::api_doc::API_JSON_SCHEMA_VERSION;

    let dependency_json = dependencies
        .iter()
        .map(|dependency| {
            json!({
                "name": dependency.name,
                "version": dependency.version,
                "source": dependency.source,
            })
        })
        .collect::<Vec<_>>();
    let mut value = match profile {
        PackProfile::Library if has_api_docs => json!({
            "schema": "beskid.package.v1",
            "id": package_id,
            "version": version,
            "documentation": {
                "apiJson": ".beskid/docs/api.json",
                "schemaVersion": API_JSON_SCHEMA_VERSION,
            },
        }),
        PackProfile::Library => json!({
            "schema": "beskid.package.v1",
            "id": package_id,
            "version": version,
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
    if !dependency_json.is_empty() {
        value
            .as_object_mut()
            .expect("package manifest is always an object")
            .insert("dependencies".into(), Value::Array(dependency_json));
    }

    serde_json::to_string_pretty(&value).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("failed to serialize package.json: {source}"),
        body: None,
    })
}

/// Apply the optional staged `package.json` dependency plan to the artifact's
/// root project manifest. This mutates only collected artifact bytes.
pub fn prepare_artifact_dependencies(
    source_root: &Path,
    entries: &mut [(String, Vec<u8>)],
) -> Result<Vec<ArtifactDependency>, PckgError> {
    let planned = load_staged_dependency_plan(source_root)?;
    let manifests =
        entries.iter_mut().filter(|(name, _)| !name.contains('/') && name.ends_with(".bproj")).collect::<Vec<_>>();
    if manifests.is_empty() {
        if planned.is_empty() {
            return Ok(Vec::new());
        }
        return Err(pack_request_error("dependency plan requires a root .bproj manifest"));
    }
    if manifests.len() != 1 {
        return Err(pack_request_error("package must contain exactly one root .bproj manifest"));
    }
    let (_, bytes) = manifests.into_iter().next().expect("one manifest");
    let project = std::str::from_utf8(bytes).map_err(|_| pack_request_error("root .bproj manifest is not UTF-8"))?;
    let (rewritten, dependencies) =
        canonicalize_project_dependencies(project, &planned).map_err(|error| pack_request_error(error.to_string()))?;
    *bytes = rewritten.into_bytes();
    Ok(dependencies)
}

fn load_staged_dependency_plan(source_root: &Path) -> Result<Vec<ArtifactDependency>, PckgError> {
    let path = source_root.join("package.json");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_slice(&fs::read(&path)?)
        .map_err(|error| pack_request_error(format!("invalid staged {}: {error}", path.display())))?;
    let Some(raw_dependencies) = value.get("dependencies") else {
        return Ok(Vec::new());
    };
    let dependencies = raw_dependencies
        .as_array()
        .ok_or_else(|| pack_request_error("staged package.json dependencies must be an array"))?;
    dependencies
        .iter()
        .map(|dependency| {
            let name = dependency.get("name").and_then(Value::as_str).unwrap_or_default();
            let version = dependency.get("version").and_then(Value::as_str).unwrap_or_default();
            let source = dependency.get("source").and_then(Value::as_str).unwrap_or_default();
            Ok(ArtifactDependency { name: name.into(), version: version.into(), source: source.into() })
        })
        .collect()
}

fn pack_request_error(message: impl Into<String>) -> PckgError {
    PckgError::Api { status: reqwest::StatusCode::BAD_REQUEST, message: message.into(), body: None }
}

/// Move the authoring manifest to the registry artifact root and remove generated API docs.
pub fn prepare_template_pack_entries(entries: &mut Vec<(String, Vec<u8>)>) -> Result<(), PckgError> {
    if entries.iter().any(|(name, _)| name == "template.json") {
        return Err(PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: "template source must not contain both `template.json` and `.beskid/template.json`".to_string(),
            body: None,
        });
    }
    let manifest = entries.iter_mut().find(|(name, _)| name == TEMPLATE_JSON_REL).ok_or_else(|| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("template project requires `{TEMPLATE_JSON_REL}`"),
        body: None,
    })?;
    manifest.0 = "template.json".to_string();
    entries.retain(|(name, _)| {
        !name.starts_with(".beskid/docs/") && name != ".beskid/docs/api.json" && name != ".beskid/docs/index.md"
    });
    Ok(())
}

/// Remove generated API docs from tool artifacts (tool profile does not require api.json).
///
/// `api.json` is *optional* for tool packages per platform-spec, but `beskid pckg pack` strips
/// any prior generated docs to keep the artifact body lean unless the publisher explicitly opts in.
pub fn strip_tool_pack_excludes(entries: &mut Vec<(String, Vec<u8>)>) {
    entries.retain(|(name, _)| {
        !name.starts_with(".beskid/docs/") && name != ".beskid/docs/api.json" && name != ".beskid/docs/index.md"
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
pub fn apply_pack_readme(source_root: &Path, entries: &mut Vec<(String, Vec<u8>)>) -> Result<(), PckgError> {
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
            message: format!("readme file `{}` does not exist or is not a file", readme_path.display()),
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
        let json = build_package_json("beskid.templates.lib", "1.0.0", &PackProfile::Template(summary), false, &[])
            .expect("serialize");
        let root: Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(root["packageKind"], PACKAGE_KIND_TEMPLATE);
        assert_eq!(root["template"]["shortName"], "lib");
        assert!(root.get("documentation").is_none());
    }

    #[test]
    fn build_package_json_library_omits_dangling_api_doc_pointer() {
        let json =
            build_package_json("corelib_compiler_sdk", "0.1.1", &PackProfile::Library, false, &[]).expect("serialize");
        let root: Value = serde_json::from_str(&json).expect("parse");
        assert!(root.get("documentation").is_none());
    }

    #[test]
    fn build_package_json_library_advertises_present_api_docs() {
        let json =
            build_package_json("corelib_foundation", "0.1.1", &PackProfile::Library, true, &[]).expect("serialize");
        let root: Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(root["documentation"]["apiJson"], ".beskid/docs/api.json");
    }

    #[test]
    fn prepares_path_independent_dependencies_from_staged_package_plan() {
        let dir = std::env::temp_dir().join(format!("beskid_pckg_dependency_plan_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(
            dir.join("package.json"),
            r#"{"version":"0.4.1","dependencies":[{"name":"corelib_foundation","version":"0.4.2","source":"registry"}]}"#,
        )
        .expect("write plan");
        let project = r#"corelib_runtime {
  name = "corelib_runtime"
}
dependency "corelib_foundation" {
  source = path
  path = "../foundation"
}
"#;
        let mut entries = vec![("corelib_runtime.bproj".into(), project.as_bytes().to_vec())];

        let dependencies = prepare_artifact_dependencies(&dir, &mut entries).expect("prepare dependencies");

        assert_eq!(dependencies.len(), 1);
        assert_eq!(dependencies[0].name, "corelib_foundation");
        let packed_project = String::from_utf8(entries[0].1.clone()).unwrap();
        assert!(packed_project.contains("source = registry"));
        assert!(packed_project.contains("version = \"0.4.2\""));
        assert!(!packed_project.contains("path ="));
        assert!(fs::read_to_string(dir.join("package.json")).unwrap().contains("0.4.2"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prepare_template_pack_entries_moves_manifest_to_artifact_root() {
        let mut entries = vec![
            (".beskid/template.json".into(), br#"{"schema":"beskid.template.v1"}"#.to_vec()),
            (".beskid/docs/api.json".into(), vec![1]),
            ("src/Main.bd".into(), vec![]),
        ];
        prepare_template_pack_entries(&mut entries).expect("prepare template entries");
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|(n, _)| n == "template.json"));
        assert!(!entries.iter().any(|(n, _)| n == ".beskid/template.json"));
        assert!(entries.iter().any(|(n, _)| n == "src/Main.bd"));
    }

    #[test]
    fn load_template_package_summary_requires_v1_schema() {
        let dir = std::env::temp_dir().join(format!("beskid_pckg_template_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".beskid")).expect("mkdir");
        fs::write(dir.join(TEMPLATE_JSON_REL), r#"{"schema":"beskid.template.v0","shortName":"x"}"#).expect("write");
        let err = load_template_package_summary(&dir.join(TEMPLATE_JSON_REL)).expect_err("wrong schema");
        assert!(err.to_string().contains("beskid.template.v1"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pack_profile_helpers_track_variant() {
        let summary = TemplatePackageSummary { short_name: None, identity: None, tags: None };
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
        let json =
            build_package_json("beskid.cli.fmt-extra", "0.1.0", &PackProfile::Tool, false, &[]).expect("serialize");
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
        let dir = std::env::temp_dir().join(format!("beskid_pckg_tool_no_manifest_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");

        let profile = detect_pack_profile_with_override(&dir, PackProfileOverride::Tool)
            .expect("tool override succeeds without manifest");
        assert!(profile.is_tool());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_pack_profile_with_override_rejects_template_project() {
        let dir = std::env::temp_dir().join(format!("beskid_pckg_tool_vs_template_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".beskid")).expect("mkdir");
        fs::write(
            dir.join("Tpl.bproj"),
            r#"Tpl {
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
        .expect("write bproj");
        fs::write(dir.join(TEMPLATE_JSON_REL), r#"{"schema":"beskid.template.v1","shortName":"x"}"#)
            .expect("write template.json");

        let err = detect_pack_profile_with_override(&dir, PackProfileOverride::Tool)
            .expect_err("template + tool override must conflict");
        assert!(err.to_string().contains("--package-kind tool"), "error mentions the conflicting flag: {err}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_pack_profile_auto_matches_legacy_behavior() {
        let dir = std::env::temp_dir().join(format!("beskid_pckg_auto_no_manifest_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");

        let profile = detect_pack_profile_with_override(&dir, PackProfileOverride::Auto)
            .expect("library profile without manifest");
        assert!(matches!(profile, PackProfile::Library));

        let _ = fs::remove_dir_all(&dir);
    }
}
