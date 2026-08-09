use std::collections::HashMap;

use bsol::{BsolSpan, ValidatedBlock};

use super::super::{
    error::ProjectError,
    model::{
        Dependency, DependencySource, GrammarOutputEntry, ModGeneratedOutput, ProjectGrammarSection, ProjectKind,
        ProjectLinkSection, ProjectManifest, ProjectModSection, ProjectSchemasSection, ProjectSection,
        ProjectTemplateSection, SchemaExport, Target, TargetKind,
    },
};
use super::{
    fields_errors::{parse_at, reject_corelib_opt_out_keys, required_field},
    intermediate::{ModFieldValue, ParsedBlock, ParsedBlocks, ParsedLinkBlock, ParsedProjectBlock},
};

pub(super) fn lower_grammar_block(block: &ValidatedBlock) -> Result<ProjectGrammarSection, ProjectError> {
    let roots = block.lists.get("roots").cloned().unwrap_or_default();
    let grammar_outputs = block
        .nested
        .iter()
        .filter(|nested| nested.rule_id == "grammarOutput")
        .map(|nested| {
            Ok(GrammarOutputEntry {
                pest: required_field(&nested.fields, "pest")?,
                module: required_field(&nested.fields, "module")?,
                package_id: required_field(&nested.fields, "packageId")?,
            })
        })
        .collect::<Result<Vec<_>, ProjectError>>()?;
    Ok(ProjectGrammarSection { roots, grammar_outputs })
}

pub(super) fn lower_mod_generated_outputs(block: &ValidatedBlock) -> Result<Vec<ModGeneratedOutput>, ProjectError> {
    block
        .nested
        .iter()
        .filter(|nested| nested.rule_id == "generatedOutput")
        .map(|nested| {
            Ok(ModGeneratedOutput {
                layout: required_field(&nested.fields, "layout")?,
                root: nested.fields.get("root").cloned().unwrap_or_default(),
            })
        })
        .collect()
}

pub(super) fn lower_schemas_block(block: &ValidatedBlock) -> Result<ProjectSchemasSection, ProjectError> {
    let default_profile = block.fields.get("defaultProfile").cloned();
    let mut exports = Vec::new();
    for nested in &block.nested {
        if nested.rule_id != "export" {
            continue;
        }
        exports.push(SchemaExport {
            name: nested
                .label
                .clone()
                .ok_or_else(|| ProjectError::Validation("`export` block requires a label".to_string()))?,
            profile: required_field(&nested.fields, "profile")?,
            path: required_field(&nested.fields, "path")?,
        });
    }
    Ok(ProjectSchemasSection { default_profile, exports })
}

pub(super) fn lower_flat_block(block: ValidatedBlock) -> ParsedBlock {
    let mut fields = block.fields;
    fields.extend(block.extras);
    ParsedBlock { label: block.label, fields }
}

pub(super) fn lower_link_block(block: ValidatedBlock) -> Result<ParsedLinkBlock, ProjectError> {
    Ok(ParsedLinkBlock {
        libraries: block.lists.get("libraries").cloned().unwrap_or_default(),
        search_paths: block.lists.get("searchPaths").cloned().unwrap_or_default(),
        extra_args: block.lists.get("extraArgs").cloned().unwrap_or_default(),
    })
}

pub(super) fn lower_mod_block(block: &ValidatedBlock) -> Result<HashMap<String, ModFieldValue>, ProjectError> {
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

pub(super) fn lower_template_block(block: &ValidatedBlock) -> Result<HashMap<String, String>, ProjectError> {
    for key in block.fields.keys().chain(block.extras.keys()) {
        if key != "shortName" && key != "identity" {
            return Err(ProjectError::meta_contract("E1885", format!("unknown `template` field `{key}`")));
        }
    }
    Ok(block.fields.clone())
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
    generated_outputs: &[ModGeneratedOutput],
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

    let generated_outputs = if generated_outputs.is_empty() { None } else { Some(generated_outputs.to_vec()) };

    Ok(ProjectModSection { max_generator_rounds, capabilities, artifact_policy, generated_outputs })
}

fn build_project_template_from_fields(template_fields: &HashMap<String, String>) -> ProjectTemplateSection {
    ProjectTemplateSection {
        short_name: template_fields.get("shortName").cloned(),
        identity: template_fields.get("identity").cloned(),
    }
}

fn assemble_project_section(project: &ParsedProjectBlock) -> Result<ProjectSection, ProjectError> {
    reject_corelib_opt_out_keys(&project.fields, &project.extras, BsolSpan { start: 0, end: 0, line: 1 })?;
    let kind = build_project_kind(project.fields.get("type").map(|s| s.as_str()))?;
    let mod_section = match (&kind, &project.mod_section) {
        (ProjectKind::Host | ProjectKind::Template | ProjectKind::Aggregate | ProjectKind::Bsol, Some(_)) => {
            return Err(ProjectError::meta_contract("E1874", "`mod` is only allowed when `type = Mod`"));
        }
        (ProjectKind::Mod, Some(mod_fields)) => {
            Some(build_project_mod_from_fields(mod_fields, &project.mod_generated_outputs)?)
        }
        _ => None,
    };
    let template_section = match (&kind, &project.template_section) {
        (ProjectKind::Host | ProjectKind::Mod | ProjectKind::Aggregate | ProjectKind::Bsol, Some(_)) => {
            return Err(ProjectError::meta_contract("E1879", "`template` is only allowed when `type = Template`"));
        }
        (ProjectKind::Template, Some(template_fields)) => Some(build_project_template_from_fields(template_fields)),
        _ => None,
    };
    let schemas_section = match (&kind, &project.schemas_section) {
        (ProjectKind::Host | ProjectKind::Mod | ProjectKind::Template | ProjectKind::Aggregate, Some(_)) => {
            return Err(ProjectError::meta_contract("E1900", "`schemas` is only allowed when `type = Bsol`"));
        }
        (ProjectKind::Bsol, section) => section.clone(),
        (_, None) => None,
    };

    Ok(ProjectSection {
        block_kind: project.block_kind.clone(),
        name: required_field(&project.fields, "name")?,
        version: required_field(&project.fields, "version")?,
        root: project.fields.get("root").cloned().unwrap_or_else(|| {
            if matches!(kind, ProjectKind::Aggregate | ProjectKind::Bsol) { String::new() } else { "Src".to_string() }
        }),
        root_namespace: project.fields.get("root_namespace").cloned(),
        kind,
        mod_section,
        grammar_section: project.grammar_section.clone(),
        template_section,
        schemas_section,
        readme: project.fields.get("readme").cloned(),
        extras: project.extras.clone(),
    })
}

pub(super) fn build_manifest(parsed: ParsedBlocks) -> Result<ProjectManifest, ProjectError> {
    let project = parsed
        .project
        .ok_or_else(|| ProjectError::Validation("missing required named project root block".to_string()))?;

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
            name: target
                .label
                .ok_or_else(|| ProjectError::Validation("target block must include a label".to_string()))?,
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
            name: dependency
                .label
                .ok_or_else(|| ProjectError::Validation("dependency block must include a label".to_string()))?,
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

    Ok(ProjectManifest { project: project_section, targets, dependencies, link })
}
