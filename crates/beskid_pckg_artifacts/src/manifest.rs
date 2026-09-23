use std::collections::BTreeMap;

use semver::Version;
use serde_json::Value;

use crate::errors::ArtifactError;
use crate::model::{ArtifactDependency, PackageKind, PackageManifestMetadata};

pub(crate) fn validate_manifest(
    manifest: &str,
    package: &str,
    version: &str,
    entries: &BTreeMap<String, usize>,
) -> Result<PackageManifestMetadata, ArtifactError> {
    let metadata = parse_package_manifest_metadata(manifest, package, version)?;
    if metadata.package_kind == PackageKind::Template && !entries.contains_key("template.json") {
        return Err(ArtifactError::InvalidManifest("template package requires template.json".into()));
    }
    if metadata.package_kind != PackageKind::Template && entries.contains_key("template.json") {
        return Err(ArtifactError::InvalidManifest("only templates may include template.json".into()));
    }
    Ok(metadata)
}

/// Parses the immutable manifest metadata persisted beside a package version.
/// Artifact validation and catalog projection share this one interpretation.
pub fn parse_package_manifest_metadata(
    manifest: &str,
    package: &str,
    version: &str,
) -> Result<PackageManifestMetadata, ArtifactError> {
    let value: Value = serde_json::from_str(manifest)
        .map_err(|_| ArtifactError::InvalidManifest("package.json is not valid JSON".into()))?;
    let object = value
        .as_object()
        .ok_or_else(|| ArtifactError::InvalidManifest("package.json root must be an object".into()))?;
    let field = |name: &str| object.get(name).and_then(Value::as_str);
    if field("schema") != Some("beskid.package.v1") {
        return Err(ArtifactError::InvalidManifest("schema must be beskid.package.v1".into()));
    }
    if !field("id").is_some_and(|id| id.eq_ignore_ascii_case(package)) {
        return Err(ArtifactError::InvalidManifest("id does not match requested package".into()));
    }
    if field("version") != Some(version) {
        return Err(ArtifactError::InvalidManifest("version does not match requested version".into()));
    }
    let kind = PackageKind::parse(
        field("packageKind").ok_or_else(|| ArtifactError::InvalidManifest("packageKind is required".into()))?,
    )?;
    let dependencies = match object.get("dependencies") {
        None => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|dependency| {
                let name = dependency.get("name").and_then(Value::as_str).unwrap_or_default();
                let version = dependency.get("version").and_then(Value::as_str).unwrap_or_default();
                let source = dependency.get("source").and_then(Value::as_str).unwrap_or_default();
                if name.is_empty() {
                    return Err(ArtifactError::InvalidManifest("published dependency must have a name".into()));
                }
                if source != "registry" {
                    return Err(ArtifactError::InvalidManifest(
                        "published dependency must use canonical registry source".into(),
                    ));
                }
                validate_exact_version(version, name)?;
                Ok(ArtifactDependency::registry(name, version))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => {
            return Err(ArtifactError::InvalidManifest("dependencies must be an array".into()));
        }
    };
    dependency_map(&dependencies, "package.json dependencies")?;
    let template = object.get("template").cloned();
    if kind == PackageKind::Template && !template.as_ref().is_some_and(Value::is_object) {
        return Err(ArtifactError::InvalidManifest("template package requires a template summary object".into()));
    }
    Ok(PackageManifestMetadata { package_kind: kind, dependencies, template })
}

pub(crate) fn dependency_map(
    dependencies: &[ArtifactDependency],
    context: &str,
) -> Result<BTreeMap<String, ArtifactDependency>, ArtifactError> {
    let mut map = BTreeMap::new();
    for dependency in dependencies {
        if dependency.name.is_empty() || dependency.source != "registry" {
            return Err(ArtifactError::InvalidManifest(format!("{context} must contain named registry dependencies")));
        }
        validate_exact_version(&dependency.version, &dependency.name)?;
        if map.insert(dependency.name.clone(), dependency.clone()).is_some() {
            return Err(ArtifactError::InvalidManifest(format!(
                "{context} contains duplicate dependency '{}'",
                dependency.name
            )));
        }
    }
    Ok(map)
}

pub(crate) fn validate_exact_version(version: &str, dependency: &str) -> Result<(), ArtifactError> {
    Version::parse(version).map_err(|_| {
        ArtifactError::InvalidManifest(format!(
            "published dependency '{dependency}' must have an exact semantic version"
        ))
    })?;
    Ok(())
}

pub(crate) fn validate_template_contract(
    template_json: &str,
    package: &str,
    package_summary: Option<&Value>,
) -> Result<(), ArtifactError> {
    let template: Value = serde_json::from_str(template_json)
        .map_err(|_| ArtifactError::InvalidManifest("template.json is not valid JSON".into()))?;
    let object = template
        .as_object()
        .ok_or_else(|| ArtifactError::InvalidManifest("template.json root must be an object".into()))?;
    if object.get("schema").and_then(Value::as_str) != Some("beskid.template.v1") {
        return Err(ArtifactError::InvalidManifest("template.json schema must be beskid.template.v1".into()));
    }
    let identity = object.get("identity").and_then(Value::as_str).unwrap_or_default();
    if identity.split_once("::").map_or(identity, |(name, _)| name) != package {
        return Err(ArtifactError::InvalidManifest("template.json identity does not match package identity".into()));
    }
    let mut expected_summary = serde_json::Map::new();
    for key in ["shortName", "identity", "tags"] {
        if let Some(value) = object.get(key) {
            expected_summary.insert(key.into(), value.clone());
        }
    }
    if package_summary != Some(&Value::Object(expected_summary)) {
        return Err(ArtifactError::InvalidManifest(
            "package.json template summary disagrees with template.json".into(),
        ));
    }
    Ok(())
}
