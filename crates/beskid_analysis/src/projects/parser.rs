//! Lower Bsol manifest AST into typed project / workspace models.

use std::collections::HashMap;

use super::bsol::{
    BsolAssignment, BsolBlock, BsolBlockHeader, BsolBodyItem, BsolDocument, BsolListItem,
    BsolNestedBlockKind, BsolReservedBlockKind, BsolSpan, BsolValue,
    parse_bsol_document,
};
use super::error::ProjectError;
use super::model::{
    Dependency, DependencySource, ProjectKind, ProjectLinkSection, ProjectManifest,
    ProjectModSection, ProjectSection, ProjectTemplateSection, Target, TargetKind,
    WorkspaceManifest, WorkspaceMember, WorkspaceOverride, WorkspaceRegistry, WorkspaceSection,
};
use super::validator::{validate_manifest, validate_workspace_manifest};

#[derive(Debug)]
struct ParsedBlock {
    kind: String,
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
    fields: HashMap<String, Vec<String>>,
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
const LINK_FIELDS: &[&str] = &["libraries", "searchPaths", "extraArgs"];

pub fn parse_manifest(source: &str) -> Result<ProjectManifest, ProjectError> {
    let document = parse_bsol_document(source)?;
    let parsed = lower_project_document(document)?;
    let manifest = build_manifest(parsed)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn parse_workspace_manifest(source: &str) -> Result<WorkspaceManifest, ProjectError> {
    let document = parse_bsol_document(source)?;
    let parsed = lower_workspace_document(document)?;
    let manifest = build_workspace_manifest(parsed)?;
    validate_workspace_manifest(&manifest)?;
    Ok(manifest)
}

fn lower_project_document(document: BsolDocument) -> Result<ParsedBlocks, ProjectError> {
    let mut parsed = ParsedBlocks::default();
    for block in document.blocks {
        let BsolBlock { span, header, body } = block;
        match header {
            BsolBlockHeader::ProjectRoot { ident } => {
                if ident == "project" {
                    return Err(ProjectError::meta_contract(
                        "E1894",
                        "legacy `project { ... }` block is not supported; use a named root block matching `name` (for example `myapp { name = \"myapp\" ... }`)",
                    ));
                }
                if parsed.project.is_some() {
                    return Err(parse_at(
                        span,
                        "manifest must contain exactly one named project root block",
                    ));
                }
                parsed.project = Some(lower_project_root_block(ident, body)?);
            }
            BsolBlockHeader::Reserved { kind, label } => match kind {
                BsolReservedBlockKind::Target | BsolReservedBlockKind::Dependency => {
                    let mut flat = lower_flat_block(span, kind, body)?;
                    flat.label = label.map(|q| q.value);
                    match kind {
                        BsolReservedBlockKind::Target => parsed.targets.push(flat),
                        BsolReservedBlockKind::Dependency => parsed.dependencies.push(flat),
                        _ => unreachable!(),
                    }
                }
                BsolReservedBlockKind::Link => {
                    if label.is_some() {
                        return Err(parse_at(span, "`link` block cannot carry a label"));
                    }
                    if parsed.link.is_some() {
                        return Err(ProjectError::meta_contract(
                            "E1890",
                            "duplicate `link` block at top level",
                        ));
                    }
                    parsed.link = Some(lower_link_block(span, body)?);
                }
                BsolReservedBlockKind::Workspace
                | BsolReservedBlockKind::Member
                | BsolReservedBlockKind::Override
                | BsolReservedBlockKind::Registry => {
                    return Err(parse_at(
                        span,
                        format!("unexpected `{}` block in project manifest", kind.as_str()),
                    ));
                }
            },
        }
    }
    Ok(parsed)
}

fn lower_workspace_document(document: BsolDocument) -> Result<ParsedWorkspaceBlocks, ProjectError> {
    let mut parsed = ParsedWorkspaceBlocks::default();
    for block in document.blocks {
        let BsolBlock { span, header, body } = block;
        match header {
            BsolBlockHeader::ProjectRoot { .. } => {
                return Err(parse_at(
                    span,
                    "workspace manifest must not contain a project root block",
                ));
            }
            BsolBlockHeader::Reserved { kind, label } => {
                let mut flat = match kind {
                    BsolReservedBlockKind::Workspace | BsolReservedBlockKind::Member => {
                        lower_loose_block(span, kind, body)?
                    }
                    BsolReservedBlockKind::Override | BsolReservedBlockKind::Registry => {
                        lower_flat_block(span, kind, body)?
                    }
                    _ => {
                        return Err(parse_at(
                            span,
                            format!(
                                "unexpected `{}` block in workspace manifest",
                                kind.as_str()
                            ),
                        ));
                    }
                };
                flat.label = label.map(|q| q.value);
                match kind {
                    BsolReservedBlockKind::Workspace => parsed.workspace = Some(flat),
                    BsolReservedBlockKind::Member => parsed.members.push(flat),
                    BsolReservedBlockKind::Override => parsed.overrides.push(flat),
                    BsolReservedBlockKind::Registry => parsed.registries.push(flat),
                    _ => unreachable!(),
                }
            }
        }
    }
    Ok(parsed)
}

fn lower_project_root_block(
    ident: String,
    body: Vec<BsolBodyItem>,
) -> Result<ParsedProjectBlock, ProjectError> {
    let mut fields = HashMap::new();
    let mut mod_section = None;
    let mut template_section = None;
    for item in body {
        match item {
            BsolBodyItem::Assignment(assignment) => {
                reject_corelib_opt_out_assignment(&assignment)?;
                let span = assignment.span;
                let (key, value) = lower_strict_assignment(assignment)?;
                if fields.insert(key.clone(), value).is_some() {
                    return Err(parse_at(
                        span,
                        format!("duplicate `{ident}` field `{key}`"),
                    ));
                }
            }
            BsolBodyItem::NestedBlock(nested) => match nested.kind {
                BsolNestedBlockKind::Mod | BsolNestedBlockKind::Meta => {
                    if mod_section.is_some() {
                        return Err(parse_at(
                            nested.span,
                            format!("duplicate `mod` block inside `{ident}`"),
                        ));
                    }
                    mod_section = Some(lower_mod_fields(nested.assignments)?);
                }
                BsolNestedBlockKind::Template => {
                    if template_section.is_some() {
                        return Err(parse_at(
                            nested.span,
                            format!("duplicate `template` block inside `{ident}`"),
                        ));
                    }
                    template_section = Some(lower_template_fields(nested.assignments)?);
                }
            },
        }
    }
    reject_corelib_opt_out_keys(&fields)?;
    let (fields, extras) = split_known_fields(fields, PROJECT_ROOT_FIELDS);
    Ok(ParsedProjectBlock {
        block_kind: ident,
        fields,
        extras,
        mod_section,
        template_section,
    })
}

fn lower_flat_block(
    span: BsolSpan,
    kind: BsolReservedBlockKind,
    body: Vec<BsolBodyItem>,
) -> Result<ParsedBlock, ProjectError> {
    let mut fields = HashMap::new();
    for item in body {
        let BsolBodyItem::Assignment(assignment) = item else {
            return Err(parse_at(
                span,
                format!("nested blocks are not allowed inside `{}`", kind.as_str()),
            ));
        };
        let (key, value) = lower_strict_assignment(assignment)?;
        fields.insert(key, value);
    }
    Ok(ParsedBlock {
        kind: kind.as_str().to_string(),
        label: None,
        fields,
    })
}

fn lower_loose_block(
    span: BsolSpan,
    kind: BsolReservedBlockKind,
    body: Vec<BsolBodyItem>,
) -> Result<ParsedBlock, ProjectError> {
    let mut fields = HashMap::new();
    for item in body {
        let BsolBodyItem::Assignment(assignment) = item else {
            return Err(parse_at(
                span,
                format!("nested blocks are not allowed inside `{}`", kind.as_str()),
            ));
        };
        let (key, value) = lower_loose_assignment(assignment)?;
        fields.insert(key, value);
    }
    Ok(ParsedBlock {
        kind: kind.as_str().to_string(),
        label: None,
        fields,
    })
}

fn lower_link_block(span: BsolSpan, body: Vec<BsolBodyItem>) -> Result<ParsedLinkBlock, ProjectError> {
    let mut fields: HashMap<String, Vec<String>> = HashMap::new();
    for item in body {
        let BsolBodyItem::Assignment(assignment) = item else {
            return Err(parse_at(span, "nested blocks are not allowed inside `link`"));
        };
        let key = assignment.key.clone();
        if !LINK_FIELDS.contains(&key.as_str()) {
            return Err(ProjectError::meta_contract(
                "E1891",
                format!(
                    "unknown `link` field `{key}` (expected one of libraries, searchPaths, extraArgs)"
                ),
            ));
        }
        let values = lower_bracket_list_value(&assignment, &key)?;
        if fields.insert(key.clone(), values).is_some() {
            return Err(parse_at(
                assignment.span,
                format!("duplicate `link` field `{key}`"),
            ));
        }
    }
    Ok(ParsedLinkBlock { fields })
}

fn lower_mod_fields(assignments: Vec<BsolAssignment>) -> Result<HashMap<String, ModFieldValue>, ProjectError> {
    let mut fields = HashMap::new();
    for assignment in assignments {
        let key = assignment.key.clone();
        let value = lower_mod_field_value(&key, &assignment)?;
        if fields.insert(key.clone(), value).is_some() {
            return Err(parse_at(
                assignment.span,
                format!("duplicate `mod` field `{key}`"),
            ));
        }
    }
    Ok(fields)
}

fn lower_template_fields(
    assignments: Vec<BsolAssignment>,
) -> Result<HashMap<String, String>, ProjectError> {
    let mut fields = HashMap::new();
    for assignment in assignments {
        let key = assignment.key.clone();
        match key.as_str() {
            "shortName" | "identity" => {}
            other => {
                return Err(ProjectError::meta_contract(
                    "E1885",
                    format!("unknown `template` field `{other}`"),
                ));
            }
        }
        let value = lower_strict_string_value(&assignment)?;
        if fields.insert(key.clone(), value).is_some() {
            return Err(parse_at(
                assignment.span,
                format!("duplicate `template` field `{key}`"),
            ));
        }
    }
    Ok(fields)
}

fn lower_strict_assignment(assignment: BsolAssignment) -> Result<(String, String), ProjectError> {
    let key = assignment.key.clone();
    let value = if allows_enum_literal(&key) {
        lower_enum_or_string_value(&assignment)?
    } else {
        lower_strict_string_value(&assignment)?
    };
    Ok((assignment.key, value))
}

fn lower_loose_assignment(assignment: BsolAssignment) -> Result<(String, String), ProjectError> {
    let key = assignment.key;
    let value = match &assignment.value {
        BsolValue::QuotedString(q) => q.value.clone(),
        BsolValue::Ident(ident) => ident.clone(),
        BsolValue::BracketList(list) => format_loose_bracket_list(list),
    };
    Ok((key, value))
}

fn format_loose_bracket_list(list: &super::bsol::BsolBracketList) -> String {
    let items = list
        .items
        .iter()
        .map(|item| match item {
            BsolListItem::Default => "default".to_string(),
            BsolListItem::Ident(ident) => format!("\"{ident}\""),
            BsolListItem::QuotedString(q) => format!("\"{}\"", q.value),
        })
        .collect::<Vec<_>>();
    format!("[{}]", items.join(", "))
}

fn lower_mod_field_value(key: &str, assignment: &BsolAssignment) -> Result<ModFieldValue, ProjectError> {
    match key {
        "attachTo" | "entryModules" | "entryModule" => Ok(ModFieldValue::StringList(Vec::new())),
        "capabilities" => parse_string_or_list_value(&assignment.value, key)
            .map(ModFieldValue::StringList)
            .map_err(|message| parse_at(assignment.span, message)),
        "maxGeneratorRounds" | "maxMetaRounds" => parse_positive_u32_value(&assignment.value, key)
            .map(ModFieldValue::U32)
            .map_err(|message| parse_at(assignment.span, message)),
        "artifactPolicy" => {
            let token = match &assignment.value {
                BsolValue::QuotedString(q) => q.value.clone(),
                BsolValue::Ident(ident) => ident.clone(),
                BsolValue::BracketList(_) => {
                    return Err(parse_at(
                        assignment.span,
                        format!("`{key}` must be a single identifier or quoted string"),
                    ));
                }
            };
            Ok(ModFieldValue::String(token))
        }
        other => Err(parse_at(
            assignment.span,
            format!("unknown `mod` field `{other}`"),
        )),
    }
}

fn lower_enum_or_string_value(assignment: &BsolAssignment) -> Result<String, ProjectError> {
    match &assignment.value {
        BsolValue::QuotedString(q) => Ok(q.value.clone()),
        BsolValue::Ident(ident) => Ok(ident.clone()),
        BsolValue::BracketList(_) => Err(parse_at(
            assignment.span,
            format!(
                "expected quoted string (or unquoted enum for this field), found list"
            ),
        )),
    }
}

fn lower_strict_string_value(assignment: &BsolAssignment) -> Result<String, ProjectError> {
    match &assignment.value {
        BsolValue::QuotedString(q) => Ok(q.value.clone()),
        other => Err(parse_at(
            assignment.span,
            format!(
                "expected quoted string (or unquoted enum for this field), found `{}`",
                value_preview(other)
            ),
        )),
    }
}

fn lower_bracket_list_value(
    assignment: &BsolAssignment,
    field: &str,
) -> Result<Vec<String>, ProjectError> {
    let BsolValue::BracketList(list) = &assignment.value else {
        return Err(parse_at(
            assignment.span,
            format!("`link.{field}` expected `[...]` list"),
        ));
    };
    bracket_list_to_strings(list, field)
        .map_err(|message| parse_at(assignment.span, format!("`link.{field}` {message}")))
}

fn parse_string_or_list_value(value: &BsolValue, field: &str) -> Result<Vec<String>, String> {
    match value {
        BsolValue::BracketList(list) => bracket_list_to_strings(list, field),
        BsolValue::Ident(ident) if ident == "default" => Ok(vec!["default".to_string()]),
        BsolValue::QuotedString(q) => Ok(vec![q.value.clone()]),
        BsolValue::Ident(ident) => parse_ident_token(ident).map(|token| vec![token]),
    }
}

fn parse_positive_u32_value(value: &BsolValue, field: &str) -> Result<u32, String> {
    let BsolValue::Ident(text) = value else {
        return Err(format!(
            "`{field}` must be a positive decimal integer, found `{}`",
            value_preview(value)
        ));
    };
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "`{field}` must be a positive decimal integer, found `{text}`"
        ));
    }
    text.parse::<u32>()
        .map_err(|_| format!("`{field}` integer overflow or invalid: `{text}`"))
}

fn bracket_list_to_strings(
    list: &super::bsol::BsolBracketList,
    field: &str,
) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for item in &list.items {
        let token = match item {
            BsolListItem::Default => "default".to_string(),
            BsolListItem::QuotedString(q) => q.value.clone(),
            BsolListItem::Ident(ident) => parse_ident_token(ident)
                .map_err(|e| format!("{field}: {e}"))?,
        };
        out.push(token);
    }
    Ok(out)
}

fn parse_ident_token(raw: &str) -> Result<String, String> {
    let t = raw.trim();
    if t.is_empty() {
        return Err("expected identifier".to_string());
    }
    let mut chars = t.chars();
    let Some(first) = chars.next() else {
        return Err("expected identifier".to_string());
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return Err(format!("invalid identifier start in `{t}`"));
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("invalid identifier `{t}`"));
    }
    Ok(t.to_string())
}

fn allows_enum_literal(field: &str) -> bool {
    matches!(field, "kind" | "source" | "resolver" | "type")
}

fn reject_corelib_opt_out_assignment(assignment: &BsolAssignment) -> Result<(), ProjectError> {
    if assignment.key == "noCorelib" {
        return Err(ProjectError::meta_contract(
            "E1876",
            "manifest must not declare `noCorelib`; host projects always resolve corelib through toolchain defaults",
        ));
    }
    if assignment.key == "useCorelib" {
        let disables = match &assignment.value {
            BsolValue::Ident(ident) => ident.eq_ignore_ascii_case("false"),
            BsolValue::QuotedString(q) => q.value.eq_ignore_ascii_case("false"),
            _ => false,
        };
        if disables {
            return Err(ProjectError::meta_contract(
                "E1876",
                "manifest must not set `useCorelib = false`; host projects always resolve corelib through toolchain defaults",
            ));
        }
    }
    Ok(())
}

fn reject_corelib_opt_out_keys(fields: &HashMap<String, String>) -> Result<(), ProjectError> {
    if fields.contains_key("noCorelib") {
        return Err(ProjectError::meta_contract(
            "E1876",
            "manifest must not declare `noCorelib`; host projects always resolve corelib through toolchain defaults",
        ));
    }
    if fields
        .get("useCorelib")
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("false"))
    {
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
        Some(other) => Err(ProjectError::meta_contract(
            "E1807",
            format!(
                "unsupported project.type `{other}` (omit the field for ordinary host projects, or use `Mod`, `Template`, or `Aggregate`)"
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
    reject_corelib_opt_out_keys(&project.fields)?;
    let kind = build_project_kind(project.fields.get("type").map(|s| s.as_str()))?;
    let mod_section = match (&kind, &project.mod_section) {
        (ProjectKind::Host | ProjectKind::Template | ProjectKind::Aggregate, Some(_)) => {
            return Err(ProjectError::meta_contract(
                "E1874",
                "`mod` is only allowed when `type = Mod`",
            ));
        }
        (ProjectKind::Mod, Some(mod_fields)) => Some(build_project_mod_from_fields(mod_fields)?),
        _ => None,
    };
    let template_section = match (&kind, &project.template_section) {
        (ProjectKind::Host | ProjectKind::Mod | ProjectKind::Aggregate, Some(_)) => {
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

    Ok(ProjectSection {
        block_kind: project.block_kind.clone(),
        name: required_field(&project.fields, "name")?,
        version: required_field(&project.fields, "version")?,
        root: project
            .fields
            .get("root")
            .cloned()
            .unwrap_or_else(|| {
                if kind == ProjectKind::Aggregate {
                    String::new()
                } else {
                    "Src".to_string()
                }
            }),
        root_namespace: project.fields.get("root_namespace").cloned(),
        kind,
        mod_section,
        template_section,
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

    let link = parsed.link.as_ref().map(build_link_section);

    Ok(ProjectManifest {
        project: project_section,
        targets,
        dependencies,
        link,
    })
}

fn build_link_section(parsed: &ParsedLinkBlock) -> ProjectLinkSection {
    ProjectLinkSection {
        libraries: parsed.fields.get("libraries").cloned().unwrap_or_default(),
        search_paths: parsed
            .fields
            .get("searchPaths")
            .cloned()
            .unwrap_or_default(),
        extra_args: parsed.fields.get("extraArgs").cloned().unwrap_or_default(),
    }
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

fn value_preview(value: &BsolValue) -> String {
    match value {
        BsolValue::QuotedString(q) => format!("\"{}\"", q.value),
        BsolValue::Ident(ident) => ident.clone(),
        BsolValue::BracketList(_) => "[...]".to_string(),
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
    fn invalid_kind_reports_parse_at() {
        let src = minimal_project("Blob", "path");
        let err = parse_manifest(&src).expect_err("bad kind");
        match err {
            ProjectError::Validation(msg) => assert!(msg.contains("Blob")),
            other => panic!("expected Validation, got {other:?}"),
        }
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
        match err {
            ProjectError::MetaContractViolation { code, .. } => assert_eq!(code, "E1891"),
            other => panic!("expected MetaContractViolation E1891, got {other:?}"),
        }
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
}
