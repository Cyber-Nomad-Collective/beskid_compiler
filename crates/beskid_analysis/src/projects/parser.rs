//! Lower schema-validated Bsol documents into typed project / workspace models.

use std::collections::HashMap;

use bsol::{
    BsolSpan, ValidatedBlock, ValidatedDocument, load_profile, parse_bsol_document, validate,
};

use super::error::ProjectError;
use super::model::{
    Dependency, DependencySource, ProjectKind, ProjectLinkSection, ProjectManifest,
    ProjectModSection, ProjectSchemasSection, ProjectSection, ProjectTemplateSection, SchemaExport,
    Target, TargetKind,
    WorkspaceManifest, WorkspaceMember, WorkspaceOverride, WorkspaceRegistry, WorkspaceSection,
};
use super::validator::{validate_manifest, validate_workspace_manifest};

#[derive(Debug)]
struct ParsedBlock {
    label: Option<String>,
    fields: HashMap<String, String>,
}

#[derive(Debug)]
struct ParsedProjectBlock {
    block_kind: String,
    fields: HashMap<String, String>,
    extras: HashMap<String, String>,
    mod_section: Option<HashMap<String, ModFieldValue>>,
    template_section: Option<HashMap<String, String>>,
    schemas_section: Option<ProjectSchemasSection>,
}

#[derive(Debug, Default)]
struct ParsedBlocks {
    project: Option<ParsedProjectBlock>,
    targets: Vec<ParsedBlock>,
    dependencies: Vec<ParsedBlock>,
    link: Option<ParsedLinkBlock>,
}

#[derive(Debug)]
struct ParsedLinkBlock {
    libraries: Vec<String>,
    search_paths: Vec<String>,
    extra_args: Vec<String>,
}

#[derive(Debug, Default)]
struct ParsedWorkspaceBlocks {
    workspace: Option<ParsedBlock>,
    members: Vec<ParsedBlock>,
    overrides: Vec<ParsedBlock>,
    registries: Vec<ParsedBlock>,
}

#[derive(Debug, Clone)]
enum ModFieldValue {
    StringList(Vec<String>),
    U32(u32),
    String(String),
}

const PROJECT_ROOT_FIELDS: &[&str] = &["name", "version", "root", "root_namespace", "type", "readme"];
const WORKSPACE_ROOT_FIELDS: &[&str] = &["name", "resolver"];
const MEMBER_FIELDS: &[&str] = &["path"];

fn parse_project_document(source: &str) -> Result<ValidatedDocument, ProjectError> {
    let document = parse_bsol_document(source)
        .map_err(|e| ProjectError::from_bsol(bsol::BsolError::from(e)))?;
    let profile = load_profile("project.v1").map_err(ProjectError::from_bsol)?;
    validate(&document, &profile).map_err(ProjectError::from_bsol)
}

fn parse_workspace_document(source: &str) -> Result<ValidatedDocument, ProjectError> {
    let document = parse_bsol_document(source)
        .map_err(|e| ProjectError::from_bsol(bsol::BsolError::from(e)))?;
    let profile = load_profile("workspace.v1").map_err(ProjectError::from_bsol)?;
    validate(&document, &profile).map_err(ProjectError::from_bsol)
}

pub fn parse_manifest(source: &str) -> Result<ProjectManifest, ProjectError> {
    let validated = parse_project_document(source)?;
    let parsed = lower_project_document(validated)?;
    let manifest = build_manifest(parsed)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn parse_workspace_manifest(source: &str) -> Result<WorkspaceManifest, ProjectError> {
    let validated = parse_workspace_document(source)?;
    let parsed = lower_workspace_document(validated)?;
    let manifest = build_workspace_manifest(parsed)?;
    validate_workspace_manifest(&manifest)?;
    Ok(manifest)
}

fn lower_project_document(validated: ValidatedDocument) -> Result<ParsedBlocks, ProjectError> {
    let mut parsed = ParsedBlocks::default();
    for block in validated.blocks {
        match block.rule_id.as_str() {
            "root" => {
                if block.kind == "project" {
                    return Err(ProjectError::meta_contract(
                        "E1894",
                        "legacy `project { ... }` block is not supported; use a named root block matching `name` (for example `myapp { name = \"myapp\" ... }`)",
                    ));
                }
                if parsed.project.is_some() {
                    return Err(parse_at(
                        block.span,
                        "manifest must contain exactly one named project root block",
                    ));
                }
                parsed.project = Some(lower_project_root_block(block)?);
            }
            "target" => parsed.targets.push(lower_flat_block(block)),
            "dependency" => parsed.dependencies.push(lower_flat_block(block)),
            "link" => {
                if parsed.link.is_some() {
                    return Err(ProjectError::meta_contract(
                        "E1890",
                        "duplicate `link` block at top level",
                    ));
                }
                parsed.link = Some(lower_link_block(block)?);
            }
            other => {
                return Err(parse_at(
                    block.span,
                    format!("unexpected `{other}` block in project manifest"),
                ));
            }
        }
    }
    Ok(parsed)
}

fn lower_workspace_document(
    validated: ValidatedDocument,
) -> Result<ParsedWorkspaceBlocks, ProjectError> {
    let mut parsed = ParsedWorkspaceBlocks::default();
    for block in validated.blocks {
        match block.rule_id.as_str() {
            "workspace" => parsed.workspace = Some(lower_flat_block(block)),
            "member" => parsed.members.push(lower_flat_block(block)),
            "override" => parsed.overrides.push(lower_flat_block(block)),
            "registry" => parsed.registries.push(lower_flat_block(block)),
            other => {
                return Err(parse_at(
                    block.span,
                    format!("unexpected `{other}` block in workspace manifest"),
                ));
            }
        }
    }
    Ok(parsed)
}

fn lower_project_root_block(block: ValidatedBlock) -> Result<ParsedProjectBlock, ProjectError> {
    reject_corelib_opt_out_keys(&block.fields, &block.extras, block.span)?;
    let (fields, extras) = split_known_fields(block.fields, PROJECT_ROOT_FIELDS);
    let mod_section = block
        .nested
        .iter()
        .find(|n| n.rule_id == "mod")
        .map(lower_mod_block)
        .transpose()?;
    let template_section = block
        .nested
        .iter()
        .find(|n| n.rule_id == "template")
        .map(lower_template_block)
        .transpose()?;
    let schemas_section = block
        .nested
        .iter()
        .find(|n| n.rule_id == "schemas")
        .map(lower_schemas_block)
        .transpose()?;
    Ok(ParsedProjectBlock {
        block_kind: block.kind,
        fields,
        extras,
        mod_section,
        template_section,
        schemas_section,
    })
}

fn lower_schemas_block(block: &ValidatedBlock) -> Result<ProjectSchemasSection, ProjectError> {
    let default_profile = block.fields.get("defaultProfile").cloned();
    let mut exports = Vec::new();
    for nested in &block.nested {
        if nested.rule_id != "export" {
            continue;
        }
        exports.push(SchemaExport {
            name: nested.label.clone().ok_or_else(|| {
                ProjectError::Validation("`export` block requires a label".to_string())
            })?,
            profile: required_field(&nested.fields, "profile")?,
            path: required_field(&nested.fields, "path")?,
        });
    }
    Ok(ProjectSchemasSection {
        default_profile,
        exports,
    })
}

fn lower_flat_block(block: ValidatedBlock) -> ParsedBlock {
    let mut fields = block.fields;
    fields.extend(block.extras);
    ParsedBlock {
        label: block.label,
        fields,
    }
}

fn lower_link_block(block: ValidatedBlock) -> Result<ParsedLinkBlock, ProjectError> {
    Ok(ParsedLinkBlock {
        libraries: block.lists.get("libraries").cloned().unwrap_or_default(),
        search_paths: block.lists.get("searchPaths").cloned().unwrap_or_default(),
        extra_args: block.lists.get("extraArgs").cloned().unwrap_or_default(),
    })
}

fn lower_mod_block(block: &ValidatedBlock) -> Result<HashMap<String, ModFieldValue>, ProjectError> {
    let mut fields = HashMap::new();
    for key in [
        "attachTo",
        "entryModules",
        "entryModule",
        "capabilities",
        "maxGeneratorRounds",
        "maxMetaRounds",
        "artifactPolicy",
    ] {
        if key == "attachTo" || key == "entryModules" || key == "entryModule" {
            if block.fields.contains_key(key) || block.lists.contains_key(key) {
                fields.insert(key.to_string(), ModFieldValue::StringList(Vec::new()));
            }
            continue;
        }
        if let Some(list) = block.lists.get(key) {
            fields.insert(key.to_string(), ModFieldValue::StringList(list.clone()));
        } else if let Some(value) = block.fields.get(key) {
            let parsed = match key {
                "maxGeneratorRounds" | "maxMetaRounds" => ModFieldValue::U32(
                    value
                        .parse::<u32>()
                        .map_err(|_| parse_at(block.span, format!("`mod.{key}` must be a positive integer")))?,
                ),
                "artifactPolicy" => ModFieldValue::String(value.clone()),
                "capabilities" => ModFieldValue::StringList(vec![value.clone()]),
                _ => ModFieldValue::String(value.clone()),
            };
            fields.insert(key.to_string(), parsed);
        }
    }
    Ok(fields)
}

fn lower_template_block(block: &ValidatedBlock) -> Result<HashMap<String, String>, ProjectError> {
    for key in block.fields.keys().chain(block.extras.keys()) {
        if key != "shortName" && key != "identity" {
            return Err(ProjectError::meta_contract(
                "E1885",
                format!("unknown `template` field `{key}`"),
            ));
        }
    }
    Ok(block.fields.clone())
}

fn reject_corelib_opt_out_keys(
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

fn split_known_fields(
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

fn build_project_kind(type_field: Option<&str>) -> Result<ProjectKind, ProjectError> {
    match type_field.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(ProjectKind::Host),
        Some("Mod") | Some("Meta") => Ok(ProjectKind::Mod),
        Some("Template") => Ok(ProjectKind::Template),
        Some("Aggregate") => Ok(ProjectKind::Aggregate),
        Some("Bsol") => Ok(ProjectKind::Bsol),
        Some(other) => Err(ProjectError::meta_contract(
            "E1807",
            format!(
                "unsupported project.type `{other}` (omit the field for ordinary host projects, or use `Mod`, `Template`, `Aggregate`, or `Bsol`)"
            ),
        )),
    }
}

fn build_project_mod_from_fields(
    mod_fields: &HashMap<String, ModFieldValue>,
) -> Result<ProjectModSection, ProjectError> {
    let max_generator_rounds = match mod_fields.get("maxGeneratorRounds") {
        None => match mod_fields.get("maxMetaRounds") {
            None => None,
            Some(ModFieldValue::U32(u)) => Some(*u),
            Some(ModFieldValue::StringList(_)) | Some(ModFieldValue::String(_)) => {
                return Err(ProjectError::meta_contract(
                    "E1872",
                    "`project.mod.maxMetaRounds` must be a positive integer",
                ));
            }
        },
        Some(ModFieldValue::U32(u)) => Some(*u),
        Some(ModFieldValue::StringList(_)) | Some(ModFieldValue::String(_)) => {
            return Err(ProjectError::meta_contract(
                "E1872",
                "`project.mod.maxGeneratorRounds` must be a positive integer",
            ));
        }
    };

    let capabilities = match mod_fields.get("capabilities") {
        None => None,
        Some(ModFieldValue::StringList(v)) => Some(v.clone()),
        Some(ModFieldValue::U32(_)) | Some(ModFieldValue::String(_)) => {
            return Err(ProjectError::meta_contract(
                "E1873",
                "`project.mod.capabilities` must be a list of capability names",
            ));
        }
    };

    let artifact_policy = match mod_fields.get("artifactPolicy") {
        None => None,
        Some(ModFieldValue::String(v)) => Some(v.clone()),
        Some(ModFieldValue::StringList(_)) | Some(ModFieldValue::U32(_)) => {
            return Err(ProjectError::meta_contract(
                "E1875",
                "`project.mod.artifactPolicy` must be a single identifier or quoted string",
            ));
        }
    };

    Ok(ProjectModSection {
        max_generator_rounds,
        capabilities,
        artifact_policy,
    })
}

fn build_project_template_from_fields(
    template_fields: &HashMap<String, String>,
) -> ProjectTemplateSection {
    ProjectTemplateSection {
        short_name: template_fields.get("shortName").cloned(),
        identity: template_fields.get("identity").cloned(),
    }
}

fn assemble_project_section(project: &ParsedProjectBlock) -> Result<ProjectSection, ProjectError> {
    reject_corelib_opt_out_keys(&project.fields, &project.extras, BsolSpan {
        start: 0,
        end: 0,
        line: 1,
    })?;
    let kind = build_project_kind(project.fields.get("type").map(|s| s.as_str()))?;
    let mod_section = match (&kind, &project.mod_section) {
        (ProjectKind::Host | ProjectKind::Template | ProjectKind::Aggregate | ProjectKind::Bsol, Some(_)) => {
            return Err(ProjectError::meta_contract(
                "E1874",
                "`mod` is only allowed when `type = Mod`",
            ));
        }
        (ProjectKind::Mod, Some(mod_fields)) => Some(build_project_mod_from_fields(mod_fields)?),
        _ => None,
    };
    let template_section = match (&kind, &project.template_section) {
        (ProjectKind::Host | ProjectKind::Mod | ProjectKind::Aggregate | ProjectKind::Bsol, Some(_)) => {
            return Err(ProjectError::meta_contract(
                "E1879",
                "`template` is only allowed when `type = Template`",
            ));
        }
        (ProjectKind::Template, Some(template_fields)) => {
            Some(build_project_template_from_fields(template_fields))
        }
        _ => None,
    };
    let schemas_section = match (&kind, &project.schemas_section) {
        (ProjectKind::Host | ProjectKind::Mod | ProjectKind::Template | ProjectKind::Aggregate, Some(_)) => {
            return Err(ProjectError::meta_contract(
                "E1900",
                "`schemas` is only allowed when `type = Bsol`",
            ));
        }
        (ProjectKind::Bsol, section) => section.clone(),
        (_, None) => None,
    };

    Ok(ProjectSection {
        block_kind: project.block_kind.clone(),
        name: required_field(&project.fields, "name")?,
        version: required_field(&project.fields, "version")?,
        root: project
            .fields
            .get("root")
            .cloned()
            .unwrap_or_else(|| {
                if matches!(kind, ProjectKind::Aggregate | ProjectKind::Bsol) {
                    String::new()
                } else {
                    "Src".to_string()
                }
            }),
        root_namespace: project.fields.get("root_namespace").cloned(),
        kind,
        mod_section,
        template_section,
        schemas_section,
        readme: project.fields.get("readme").cloned(),
        extras: project.extras.clone(),
    })
}

fn build_manifest(parsed: ParsedBlocks) -> Result<ProjectManifest, ProjectError> {
    let project = parsed.project.ok_or_else(|| {
        ProjectError::Validation("missing required named project root block".to_string())
    })?;

    let project_section = assemble_project_section(&project)?;
    if project_section.block_kind != project_section.name {
        return Err(ProjectError::meta_contract(
            "E1896",
            format!(
                "project root block kind `{}` must match `name = \"{}\"`",
                project_section.block_kind, project_section.name
            ),
        ));
    }

    let mut targets = Vec::with_capacity(parsed.targets.len());
    for target in parsed.targets {
        let kind = match required_field(&target.fields, "kind")?.as_str() {
            "App" => TargetKind::App,
            "Lib" => TargetKind::Lib,
            "Test" => TargetKind::Test,
            other => {
                return Err(ProjectError::Validation(format!(
                    "target `{}` has unsupported kind `{other}` (expected App, Lib, or Test, e.g. `kind = Lib`)",
                    target.label.as_deref().unwrap_or("<unnamed>")
                )));
            }
        };

        let entry = target.fields.get("entry").cloned();
        if !matches!(kind, TargetKind::Lib) && entry.as_deref().unwrap_or("").trim().is_empty() {
            return Err(ProjectError::Validation(format!(
                "target `{}` requires `entry` when kind is App or Test",
                target.label.as_deref().unwrap_or("<unnamed>")
            )));
        }

        targets.push(Target {
            name: target.label.ok_or_else(|| {
                ProjectError::Validation("target block must include a label".to_string())
            })?,
            kind,
            entry,
        });
    }

    let mut dependencies = Vec::with_capacity(parsed.dependencies.len());
    for dependency in parsed.dependencies {
        let source = match required_field(&dependency.fields, "source")?.as_str() {
            "path" => DependencySource::Path,
            "git" => DependencySource::Git,
            "registry" => DependencySource::Registry,
            other => {
                return Err(ProjectError::Validation(format!(
                    "dependency `{}` has unsupported source `{other}` (expected path, git, or registry, e.g. `source = path`)",
                    dependency.label.as_deref().unwrap_or("<unnamed>")
                )));
            }
        };

        dependencies.push(Dependency {
            name: dependency.label.ok_or_else(|| {
                ProjectError::Validation("dependency block must include a label".to_string())
            })?,
            source,
            path: dependency.fields.get("path").cloned(),
            url: dependency.fields.get("url").cloned(),
            rev: dependency.fields.get("rev").cloned(),
            version: dependency.fields.get("version").cloned(),
            registry: dependency.fields.get("registry").cloned(),
        });
    }

    let link = parsed.link.map(|l| ProjectLinkSection {
        libraries: l.libraries,
        search_paths: l.search_paths,
        extra_args: l.extra_args,
    });

    Ok(ProjectManifest {
        project: project_section,
        targets,
        dependencies,
        link,
    })
}

fn build_workspace_manifest(
    parsed: ParsedWorkspaceBlocks,
) -> Result<WorkspaceManifest, ProjectError> {
    let workspace = parsed.workspace.ok_or_else(|| {
        ProjectError::Validation("missing required `workspace` block".to_string())
    })?;

    let (workspace_fields, workspace_extras) =
        split_known_fields(workspace.fields, WORKSPACE_ROOT_FIELDS);
    let workspace_section = WorkspaceSection {
        name: required_field(&workspace_fields, "name")?,
        resolver: workspace_fields
            .get("resolver")
            .cloned()
            .unwrap_or_else(|| "v1".to_string()),
        extras: workspace_extras,
    };

    let mut members = Vec::with_capacity(parsed.members.len());
    for member in parsed.members {
        let (member_fields, member_extras) = split_known_fields(member.fields, MEMBER_FIELDS);
        members.push(WorkspaceMember {
            name: member.label.ok_or_else(|| {
                ProjectError::Validation("member block must include a label".to_string())
            })?,
            path: required_field(&member_fields, "path")?,
            extras: member_extras,
        });
    }

    let mut overrides = Vec::with_capacity(parsed.overrides.len());
    for dependency_override in parsed.overrides {
        overrides.push(WorkspaceOverride {
            dependency: dependency_override.label.ok_or_else(|| {
                ProjectError::Validation("override block must include a label".to_string())
            })?,
            version: required_field(&dependency_override.fields, "version")?,
        });
    }

    let mut registries = Vec::with_capacity(parsed.registries.len());
    for registry in parsed.registries {
        registries.push(WorkspaceRegistry {
            name: registry.label.ok_or_else(|| {
                ProjectError::Validation("registry block must include a label".to_string())
            })?,
            url: required_field(&registry.fields, "url")?,
        });
    }

    Ok(WorkspaceManifest {
        workspace: workspace_section,
        members,
        overrides,
        registries,
    })
}

fn required_field(fields: &HashMap<String, String>, key: &str) -> Result<String, ProjectError> {
    fields
        .get(key)
        .cloned()
        .ok_or_else(|| ProjectError::Validation(format!("missing required field `{key}`")))
}

fn parse_at(span: BsolSpan, message: impl Into<String>) -> ProjectError {
    ProjectError::ParseAt {
        line: span.line,
        message: message.into(),
        start: Some(span.start),
        end: Some(span.end),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::model::{DependencySource, TargetKind};

    fn minimal_project(kind: &str, source_field: &str) -> String {
        format!(
            r#"p {{
  name = "p"
  version = "0.1.0"
}}
target "t" {{
  kind = {kind}
  entry = "Main.bd"
}}
dependency "d" {{
  source = {source_field}
  path = "../x"
}}
"#
        )
    }

    #[test]
    fn parse_kind_lib_unquoted() {
        let src = minimal_project("Lib", "path");
        let m = parse_manifest(&src).expect("parse");
        assert_eq!(m.targets[0].kind, TargetKind::Lib);
        assert_eq!(m.dependencies[0].source, DependencySource::Path);
    }

    #[test]
    fn parse_kind_and_source_quoted_legacy() {
        let src = minimal_project("\"Lib\"", "\"path\"");
        let m = parse_manifest(&src).expect("parse");
        assert_eq!(m.targets[0].kind, TargetKind::Lib);
        assert_eq!(m.dependencies[0].source, DependencySource::Path);
    }

    #[test]
    fn name_must_stay_quoted() {
        let src = r#"MyApp {
  name = MyApp
  version = "0.1.0"
}
target "t" { kind = Lib entry = "e.bd" }
"#;
        let err = parse_manifest(src).expect_err("name unquoted");
        assert!(matches!(err, ProjectError::ParseAt { .. }));
    }

    #[test]
    fn invalid_kind_reports_validation() {
        let src = minimal_project("Blob", "path");
        let err = parse_manifest(&src).expect_err("bad kind");
        assert!(matches!(err, ProjectError::ParseAt { .. }));
    }

    #[test]
    fn parse_link_block_libraries_and_paths() {
        let src = r#"p {
  name = "p"
  version = "0.1.0"
}

target "t" {
  kind = App
  entry = "Main.bd"
}

link {
  libraries = [libc, pthread]
  searchPaths = ["/usr/lib", "/opt/local/lib"]
  extraArgs = ["-lm"]
}
"#;
        let m = parse_manifest(src).expect("parse link block");
        let link = m.link.expect("link section present");
        assert_eq!(link.libraries, vec!["libc", "pthread"]);
        assert_eq!(link.search_paths, vec!["/usr/lib", "/opt/local/lib"]);
        assert_eq!(link.extra_args, vec!["-lm"]);
    }

    #[test]
    fn parse_link_block_unknown_key_rejected() {
        let src = r#"p {
  name = "p"
  version = "0.1.0"
}
target "t" {
  kind = App
  entry = "Main.bd"
}
link {
  bogus = [libc]
}
"#;
        let err = parse_manifest(src).expect_err("unknown link key must error");
        assert!(matches!(err, ProjectError::ParseAt { .. }));
    }

    #[test]
    fn parse_link_block_duplicate_library_rejected() {
        let src = r#"p {
  name = "p"
  version = "0.1.0"
}
target "t" {
  kind = App
  entry = "Main.bd"
}
link {
  libraries = [libc, libc]
}
"#;
        let err = parse_manifest(src).expect_err("duplicate library must error");
        match err {
            ProjectError::MetaContractViolation { code, .. } => assert_eq!(code, "E1893"),
            other => panic!("expected MetaContractViolation E1893, got {other:?}"),
        }
    }

    #[test]
    fn parse_link_block_absent_yields_none() {
        let src = r#"p {
  name = "p"
  version = "0.1.0"
}
target "t" {
  kind = App
  entry = "Main.bd"
}
"#;
        let m = parse_manifest(src).expect("parse without link");
        assert!(m.link.is_none());
    }

    #[test]
    fn workspace_resolver_unquoted() {
        let src = r#"workspace {
  name = "w"
  resolver = v1
}
member "m" {
  path = "pkg"
}
"#;
        let w = parse_workspace_manifest(src).expect("parse workspace");
        assert_eq!(w.workspace.resolver, "v1");
        assert_eq!(w.workspace.name, "w");
        assert_eq!(w.members[0].path, "pkg");
    }

    #[test]
    fn workspace_default_test_member_lands_in_extras() {
        let src = r#"workspace {
  name = "corelib"
  resolver = v1
  defaultTestMember = "corelib_tests"
}
member "corelib_tests" {
  path = "tests/corelib_tests"
}
"#;
        let w = parse_workspace_manifest(src).expect("parse workspace");
        assert_eq!(
            w.workspace.extras.get("defaultTestMember").map(String::as_str),
            Some("corelib_tests")
        );
    }
}
