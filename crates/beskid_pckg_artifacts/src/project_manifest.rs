use bsol::{BsolBlock, BsolItem, BsolValue, parse_bsol_document};

use crate::errors::ArtifactError;
use crate::manifest::{dependency_map, validate_exact_version};
use crate::model::ArtifactDependency;

#[derive(Debug, Clone)]
pub(crate) struct ProjectDependencyBlock {
    pub(crate) dependency: ArtifactDependency,
    pub(crate) span_start: usize,
    pub(crate) span_end: usize,
}

/// Rewrites source-only dependency declarations into path-independent registry
/// declarations for the artifact. The source manifest passed by the caller is
/// never changed on disk.
pub fn canonicalize_project_dependencies(
    project: &str,
    planned: &[ArtifactDependency],
) -> Result<(String, Vec<ArtifactDependency>), ArtifactError> {
    let planned = dependency_map(planned, "dependency plan")?;
    let blocks = parse_project_dependencies(project)?;
    let mut resolved = Vec::with_capacity(blocks.len());
    for block in &blocks {
        let declared = &block.dependency;
        let dependency = if declared.source == "registry" {
            validate_exact_version(&declared.version, &declared.name)?;
            if let Some(planned) = planned.get(&declared.name)
                && planned.version != declared.version
            {
                return Err(ArtifactError::InvalidManifest(format!(
                    "dependency plan version for '{}' disagrees with the project manifest",
                    declared.name
                )));
            }
            ArtifactDependency::registry(&declared.name, &declared.version)
        } else {
            planned.get(&declared.name).cloned().ok_or_else(|| {
                ArtifactError::InvalidManifest(format!(
                    "dependency '{}' uses source '{}' but has no exact registry version in package.json",
                    declared.name, declared.source
                ))
            })?
        };
        resolved.push(dependency);
    }
    let resolved_map = dependency_map(&resolved, "project dependencies")?;
    if planned.keys().any(|name| !resolved_map.contains_key(name)) {
        return Err(ArtifactError::InvalidManifest(
            "dependency plan contains a package not declared by the project manifest".into(),
        ));
    }

    let mut rewritten = project.to_owned();
    for (block, dependency) in blocks.iter().zip(&resolved).rev() {
        let replacement = format!(
            "dependency \"{}\" {{\n  source = registry\n  version = \"{}\"\n}}",
            dependency.name, dependency.version
        );
        rewritten.replace_range(block.span_start..block.span_end, &replacement);
    }
    resolved.sort_by(|left, right| left.name.cmp(&right.name));
    Ok((rewritten, resolved))
}

pub(crate) fn parse_project_dependencies(project: &str) -> Result<Vec<ProjectDependencyBlock>, ArtifactError> {
    let document = parse_bsol_document(project)
        .map_err(|error| ArtifactError::InvalidManifest(format!("project manifest is invalid: {error}")))?;
    document
        .blocks
        .iter()
        .filter(|block| block.kind == "dependency")
        .map(|block| {
            let name =
                block.label.as_ref().map(|label| label.value.clone()).filter(|name| !name.is_empty()).ok_or_else(
                    || ArtifactError::InvalidManifest("dependency block must have a package name".into()),
                )?;
            let source = block_string_field(block, "source").unwrap_or_default();
            let version = block_string_field(block, "version").unwrap_or_default();
            Ok(ProjectDependencyBlock {
                dependency: ArtifactDependency { name, version, source },
                span_start: block.span.start,
                span_end: block.span.end,
            })
        })
        .collect()
}

fn block_string_field(block: &BsolBlock, field: &str) -> Option<String> {
    block.items.iter().find_map(|item| match item {
        BsolItem::Assignment(assignment) if assignment.key == field => match &assignment.value {
            BsolValue::QuotedString(value) => Some(value.value.clone()),
            BsolValue::Ident(value) => Some(value.clone()),
            _ => None,
        },
        _ => None,
    })
}

pub(crate) fn validate_published_project_dependencies(
    project: &str,
    manifest_dependencies: &[ArtifactDependency],
) -> Result<(), ArtifactError> {
    let blocks = parse_project_dependencies(project)?;
    let declared = blocks.into_iter().map(|block| block.dependency).collect::<Vec<_>>();
    for dependency in &declared {
        if dependency.source != "registry" {
            return Err(ArtifactError::InvalidManifest(format!(
                "published project dependency '{}' must use registry source",
                dependency.name
            )));
        }
        validate_exact_version(&dependency.version, &dependency.name)?;
    }
    if dependency_map(&declared, "project dependencies")?
        != dependency_map(manifest_dependencies, "package.json dependencies")?
    {
        return Err(ArtifactError::InvalidManifest(
            "project dependencies disagree with package.json dependencies".into(),
        ));
    }
    Ok(())
}

pub(crate) fn project_field<'a>(content: &'a str, key: &str) -> Option<&'a str> {
    content.lines().map(str::trim).filter(|line| !line.starts_with('#')).find_map(|line| {
        let (current, value) = line.split_once('=')?;
        if current.trim() != key {
            return None;
        }
        Some(value.trim().trim_matches('"'))
    })
}

pub(crate) fn project_root_block_identifier(content: &str) -> Option<&str> {
    let line = content.lines().map(str::trim).find(|line| !line.is_empty() && !line.starts_with('#'))?;
    let identifier = line.strip_suffix('{')?.trim();
    let mut characters = identifier.chars();
    let first = characters.next()?;
    if !(first.is_ascii_alphabetic() || first == '_')
        || !characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    Some(identifier)
}
