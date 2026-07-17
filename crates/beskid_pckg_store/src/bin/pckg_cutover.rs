//! Explicit one-shot pckg cutover runner.
//!
//! It is intentionally not an HTTP endpoint and never runs at service start.
//! Run it only against a restored legacy database and artifact-store clone.

use std::{
    env, fs,
    path::{Component, Path, PathBuf},
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

use beskid_pckg_store::{
    LegacyIdentityCutoverError, LegacyIdentityCutoverRequest, LegacyIdentitySubjectMapping,
    SqlxPackageRepository,
};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};

#[derive(Debug)]
struct Arguments {
    database_url: String,
    mapping_file: PathBuf,
    artifact_root: PathBuf,
    requested_by: String,
    run_id: String,
    apply: bool,
}

#[derive(Debug)]
struct LegacyArtifact {
    storage_key: String,
    checksum_sha256: String,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pckg cutover failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let arguments = parse_arguments(env::args().skip(1))?;
    let mappings = read_mappings(&arguments.mapping_file)?;
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&arguments.database_url)
        .await
        .map_err(|error| format!("cannot connect to restored database: {error}"))?;
    let (legacy_packages, legacy_versions, artifacts) = preflight(&pool, &mappings).await?;
    verify_artifacts(&arguments.artifact_root, &artifacts)?;
    println!(
        "preflight passed: {legacy_packages} packages, {legacy_versions} versions, {} reviewed mappings, {} verified artifacts",
        mappings.len(),
        artifacts.len()
    );
    if !arguments.apply {
        println!("dry-run complete; rerun with --apply only after two-person mapping review.");
        return Ok(());
    }
    ensure_apply_acknowledged(true, env::var("PCKG_CUTOVER_REHEARSAL").ok().as_deref())?;

    let repository = SqlxPackageRepository::new(pool);
    repository
        .migrate()
        .await
        .map_err(|error| format!("Rust migration failed: {error:?}"))?;
    let report = repository
        .import_legacy_identity_cutover(LegacyIdentityCutoverRequest {
            run_id: arguments.run_id,
            requested_by: arguments.requested_by,
            mappings,
            now_unix_seconds: now_unix_seconds()?,
        })
        .await
        .map_err(render_cutover_error)?;
    if report.imported_package_count != legacy_packages
        || report.imported_version_count != legacy_versions
    {
        return Err(format!(
            "import count mismatch: expected {legacy_packages} packages/{legacy_versions} versions, imported {}/{}",
            report.imported_package_count, report.imported_version_count
        ));
    }
    println!(
        "apply completed: run={}, packages={}, versions={}",
        report.run_id, report.imported_package_count, report.imported_version_count
    );
    Ok(())
}

async fn preflight(
    pool: &PgPool,
    mappings: &[LegacyIdentitySubjectMapping],
) -> Result<(u64, u64, Vec<LegacyArtifact>), String> {
    let legacy_packages: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"Packages\"")
        .fetch_one(pool)
        .await
        .map_err(|error| format!("legacy Packages table is unavailable: {error}"))?;
    let legacy_versions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"PackageVersions\"")
        .fetch_one(pool)
        .await
        .map_err(|error| format!("legacy PackageVersions table is unavailable: {error}"))?;
    let known = mappings
        .iter()
        .map(|mapping| mapping.legacy_identity_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let owners =
        sqlx::query("SELECT DISTINCT \"OwnerUserId\" FROM \"Packages\" ORDER BY \"OwnerUserId\"")
            .fetch_all(pool)
            .await
            .map_err(|error| format!("cannot read legacy package owners: {error}"))?;
    let missing = owners
        .iter()
        .map(|row| row.try_get::<String, _>("OwnerUserId"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot decode a legacy package owner: {error}"))?
        .into_iter()
        .filter(|owner| !known.contains(owner.as_str()))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "mapping file is missing legacy owners: {}",
            missing.join(", ")
        ));
    }
    let artifacts = sqlx::query("SELECT \"StorageKey\", \"ChecksumSha256\" FROM \"PackageVersions\" ORDER BY \"StorageKey\"")
        .fetch_all(pool)
        .await
        .map_err(|error| format!("cannot read legacy version artifacts: {error}"))?
        .into_iter()
        .map(|row| Ok(LegacyArtifact {
            storage_key: row.try_get("StorageKey")?,
            checksum_sha256: row.try_get("ChecksumSha256")?,
        }))
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(|error| format!("cannot decode a legacy artifact: {error}"))?;
    Ok((
        u64::try_from(legacy_packages).map_err(|_| "negative package count".to_owned())?,
        u64::try_from(legacy_versions).map_err(|_| "negative version count".to_owned())?,
        artifacts,
    ))
}

fn verify_artifacts(root: &Path, artifacts: &[LegacyArtifact]) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("cannot open artifact root {}: {error}", root.display()))?;
    for artifact in artifacts {
        let path = safe_artifact_path(&root, &artifact.storage_key)?;
        let bytes = fs::read(&path)
            .map_err(|error| format!("cannot read artifact {}: {error}", path.display()))?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if !actual.eq_ignore_ascii_case(&artifact.checksum_sha256) {
            return Err(format!(
                "checksum mismatch for artifact {}",
                artifact.storage_key
            ));
        }
    }
    Ok(())
}

fn safe_artifact_path(root: &Path, storage_key: &str) -> Result<PathBuf, String> {
    let relative = Path::new(storage_key);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("unsafe legacy storage key: {storage_key}"));
    }
    Ok(root.join(relative))
}

fn read_mappings(path: &Path) -> Result<Vec<LegacyIdentitySubjectMapping>, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("cannot read mapping file {}: {error}", path.display()))?;
    let mut mappings = Vec::new();
    let mut seen_legacy_identities = std::collections::BTreeSet::new();
    for (line_number, line) in contents.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 4 {
            return Err(format!(
                "mapping line {} must have four tab-separated fields",
                line_number + 1
            ));
        }
        let approved_at_unix_seconds = fields[3].parse::<i64>().map_err(|_| {
            format!(
                "mapping line {} has an invalid approval timestamp",
                line_number + 1
            )
        })?;
        if fields
            .iter()
            .take(3)
            .any(|field| field.trim().is_empty() || *field != field.trim())
            || !canonical_github_subject(fields[1])
        {
            return Err(format!(
                "mapping line {} must use trimmed values and github:<numeric-id>",
                line_number + 1
            ));
        }
        if !seen_legacy_identities.insert(fields[0]) {
            return Err(format!(
                "mapping line {} repeats legacy identity `{}`",
                line_number + 1,
                fields[0]
            ));
        }
        mappings.push(LegacyIdentitySubjectMapping {
            legacy_identity_id: fields[0].to_owned(),
            github_subject: fields[1].to_owned(),
            approved_by: fields[2].to_owned(),
            approved_at_unix_seconds,
        });
    }
    if mappings.is_empty() {
        return Err("mapping file must contain at least one reviewed mapping".to_owned());
    }
    Ok(mappings)
}

fn canonical_github_subject(value: &str) -> bool {
    value.starts_with("github:") && value["github:".len()..].parse::<u64>().is_ok()
}

fn ensure_apply_acknowledged(apply: bool, acknowledgement: Option<&str>) -> Result<(), String> {
    if apply && acknowledgement != Some("restored-clone") {
        return Err(
            "refusing --apply without PCKG_CUTOVER_REHEARSAL=restored-clone; this runner is only for a restored clone"
                .to_owned(),
        );
    }
    Ok(())
}

fn parse_arguments(arguments: impl IntoIterator<Item = String>) -> Result<Arguments, String> {
    let mut database_url = None;
    let mut mapping_file = None;
    let mut artifact_root = None;
    let mut requested_by = None;
    let mut run_id = None;
    let mut apply = false;
    let mut values = arguments.into_iter();
    while let Some(argument) = values.next() {
        match argument.as_str() {
            "--database-url" => database_url = values.next(),
            "--mapping-file" => mapping_file = values.next().map(PathBuf::from),
            "--artifact-root" => artifact_root = values.next().map(PathBuf::from),
            "--requested-by" => requested_by = values.next(),
            "--run-id" => run_id = values.next(),
            "--apply" => apply = true,
            "--help" => return Err("usage: pckg_cutover --database-url URL --mapping-file FILE --artifact-root DIR --requested-by OPERATOR --run-id UUID [--apply]".to_owned()),
            _ => return Err(format!("unknown or incomplete argument: {argument}")),
        }
    }
    Ok(Arguments {
        database_url: database_url.ok_or("--database-url is required")?,
        mapping_file: mapping_file.ok_or("--mapping-file is required")?,
        artifact_root: artifact_root.ok_or("--artifact-root is required")?,
        requested_by: requested_by.ok_or("--requested-by is required")?,
        run_id: run_id.ok_or("--run-id is required")?,
        apply,
    })
}

fn now_unix_seconds() -> Result<i64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())
        .and_then(|duration| {
            i64::try_from(duration.as_secs()).map_err(|_| "current time exceeds i64".to_owned())
        })
}

fn render_cutover_error(error: LegacyIdentityCutoverError) -> String {
    match error {
        LegacyIdentityCutoverError::RejectedUnmappedIdentities(report) => format!(
            "import rejected: {} unmapped owners",
            report.unmapped_identities.len()
        ),
        other => format!("import failed: {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_github_subject, ensure_apply_acknowledged, parse_arguments, read_mappings,
        safe_artifact_path,
    };
    use std::{fs, path::Path};

    #[test]
    fn requires_explicit_apply_and_complete_arguments() {
        let arguments = parse_arguments(
            [
                "--database-url",
                "postgres://clone",
                "--mapping-file",
                "mapping.tsv",
                "--artifact-root",
                "artifacts",
                "--requested-by",
                "reviewer",
                "--run-id",
                "c48d3968-7b0f-4a70-89cd-102607f6a6b9",
            ]
            .into_iter()
            .map(str::to_owned),
        )
        .unwrap();
        assert!(!arguments.apply);
        assert!(parse_arguments(["--apply"].into_iter().map(str::to_owned)).is_err());
    }

    #[test]
    fn accepts_only_canonical_github_subjects_and_safe_artifact_keys() {
        assert!(canonical_github_subject("github:42"));
        assert!(!canonical_github_subject("legacy-42"));
        assert!(safe_artifact_path(Path::new("/artifacts"), "packages/demo.bpk").is_ok());
        assert!(safe_artifact_path(Path::new("/artifacts"), "../legacy-secret").is_err());
    }

    #[test]
    fn apply_requires_restored_clone_acknowledgement() {
        assert!(ensure_apply_acknowledged(false, None).is_ok());
        assert!(ensure_apply_acknowledged(true, None).is_err());
        assert!(ensure_apply_acknowledged(true, Some("production")).is_err());
        assert!(ensure_apply_acknowledged(true, Some("restored-clone")).is_ok());
    }

    #[test]
    fn rejects_duplicate_legacy_identity_mapping_during_dry_run() {
        let path = std::env::temp_dir().join(format!(
            "pckg-cutover-duplicate-mapping-{}.tsv",
            std::process::id()
        ));
        fs::write(
            &path,
            "legacy-1\tgithub:1\treviewer\t1760000000\nlegacy-1\tgithub:1\treviewer\t1760000000\n",
        )
        .unwrap();
        assert!(read_mappings(&path).is_err());
        fs::remove_file(path).unwrap();
    }
}
