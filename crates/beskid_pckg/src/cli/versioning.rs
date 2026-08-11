use super::{BTreeMap, PackArgs, Path, PathBuf, PckgError, Version, fs};

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct PackVersionState {
    versions: BTreeMap<String, String>,
}

pub(super) fn resolve_pack_version(source: &Path, args: &PackArgs) -> Result<String, PckgError> {
    let source_manifest_version = read_source_manifest_version(source)?;
    let stored_version = read_stored_pack_version(source, args)?;

    let baseline = max_version(source_manifest_version.as_ref(), stored_version.as_ref());
    let auto_version = bump_patch(baseline)?;

    match args.version.as_deref() {
        Some(explicit) => {
            let explicit_version = parse_version(explicit)?;
            if explicit_version <= auto_version {
                return Err(PckgError::Api {
                    status: reqwest::StatusCode::BAD_REQUEST,
                    message: format!(
                        "explicit version '{}' must be higher than auto-resolved '{}'",
                        explicit_version, auto_version
                    ),
                    body: None,
                });
            }

            Ok(explicit_version.to_string())
        }
        None => Ok(auto_version.to_string()),
    }
}

fn bump_patch(base: Option<Version>) -> Result<Version, PckgError> {
    let mut version = base.unwrap_or_else(|| Version::new(0, 1, 0));
    version.patch = version.patch.checked_add(1).ok_or_else(|| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: "cannot bump patch version beyond supported range".to_string(),
        body: None,
    })?;
    version.pre = semver::Prerelease::EMPTY;
    version.build = semver::BuildMetadata::EMPTY;
    Ok(version)
}

fn max_version(a: Option<&Version>, b: Option<&Version>) -> Option<Version> {
    match (a, b) {
        (Some(left), Some(right)) => Some(if left >= right { left.clone() } else { right.clone() }),
        (Some(left), None) => Some(left.clone()),
        (None, Some(right)) => Some(right.clone()),
        (None, None) => None,
    }
}

fn parse_version(raw: &str) -> Result<Version, PckgError> {
    Version::parse(raw.trim()).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("invalid semantic version '{}': {source}", raw.trim()),
        body: None,
    })
}

fn read_source_manifest_version(source: &Path) -> Result<Option<Version>, PckgError> {
    let manifest_path = source.join("package.json");
    if !manifest_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&manifest_path)?;
    let value: serde_json::Value = serde_json::from_str(&content).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("failed to parse package.json: {source}"),
        body: None,
    })?;

    let Some(version_str) = value.get("version").and_then(serde_json::Value::as_str) else {
        return Ok(None);
    };

    Ok(Some(parse_version(version_str)?))
}

fn version_state_path(source: &Path, args: &PackArgs) -> PathBuf {
    if args.version_state_file.is_absolute() {
        args.version_state_file.clone()
    } else {
        source.join(&args.version_state_file)
    }
}

fn read_stored_pack_version(source: &Path, args: &PackArgs) -> Result<Option<Version>, PckgError> {
    let path = version_state_path(source, args);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)?;
    let state: PackVersionState = serde_json::from_str(&content).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::BAD_REQUEST,
        message: format!("failed to parse version state file: {source}"),
        body: None,
    })?;

    let Some(version) = state.versions.get(&args.package) else {
        return Ok(None);
    };

    Ok(Some(parse_version(version)?))
}

pub(super) fn persist_pack_version_state(source: &Path, args: &PackArgs, version: &str) -> Result<(), PckgError> {
    let path = version_state_path(source, args);
    let mut state = if path.exists() {
        let content = fs::read_to_string(&path)?;
        serde_json::from_str::<PackVersionState>(&content).map_err(|source| PckgError::Api {
            status: reqwest::StatusCode::BAD_REQUEST,
            message: format!("failed to parse version state file: {source}"),
            body: None,
        })?
    } else {
        PackVersionState::default()
    };

    state.versions.insert(args.package.clone(), version.to_string());

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let output = serde_json::to_string_pretty(&state).map_err(|source| PckgError::Api {
        status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        message: format!("failed to serialize version state: {source}"),
        body: None,
    })?;

    fs::write(path, output + "\n")?;
    Ok(())
}
