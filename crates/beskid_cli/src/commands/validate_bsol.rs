//! Validate BSOL documents against schema profiles.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use beskid_analysis::projects::{ProjectKind, ProjectManifest, is_project_manifest_path, load_manifest_from_path};
use bsol::{
    AnalysisOptions, AnalysisSession, BsolError, CompositeSchemaSource, ImportSchemaSpec, ImportSource, SchemaProfile,
    SchemaSource, resolve_active_profile, validate_profile_document,
};
#[cfg(test)]
use bsol::{load_profile, parse_bsol_document, validate};
use bsol_beskid_bridge::{LockedImport, LockedSchemaHost};
use clap::Parser;

/// Validate a BSOL document against a schema profile.
#[derive(Debug, Parser)]
pub struct ValidateBsolArgs {
    /// Schema profile name (for example `project.v1`, `workspace.v1`, `board.v2`). Defaults to
    /// `project.v1`. A `type = Bsol` project manifest is validated against its declared schema
    /// exports instead and rejects this option.
    #[arg(long)]
    pub profile: Option<String>,

    /// Apply profile migration rewrites before validation.
    #[arg(long)]
    pub migrate: bool,

    /// Controlled root containing already materialized schema imports.
    #[arg(long, requires = "schema_import_lock")]
    pub schema_import_root: Option<PathBuf>,

    /// JSON array of exact request, canonical identity, materialized path and SHA-256 locks.
    #[arg(long, requires = "schema_import_root")]
    pub schema_import_lock: Option<PathBuf>,

    /// Path to the BSOL document (defaults to stdin when omitted).
    pub path: Option<PathBuf>,
}

const DEFAULT_PROFILE: &str = "project.v1";

pub fn execute(args: ValidateBsolArgs) -> Result<()> {
    // `--migrate` rewrites a (possibly legacy) document before validation, so it keeps the
    // document-level route; every other project manifest is classified by its declared kind.
    if let Some(path) = &args.path
        && is_project_manifest_path(path)
        && !args.migrate
    {
        let manifest = load_manifest_from_path(path).map_err(|err| anyhow::Error::msg(err.to_string()))?;
        if manifest.project.kind == ProjectKind::Bsol {
            anyhow::ensure!(
                args.profile.is_none(),
                "`{}` is a `type = Bsol` project: it is validated against its declared schema exports; omit --profile",
                path.display()
            );
            let locked = schema_session(&args)?;
            let validated = validate_bsol_project(path, &manifest, locked)?;
            for profile in validated {
                eprintln!("ok: validated exported schema profile `{profile}`");
            }
            return Ok(());
        }
    }

    let source = match &args.path {
        Some(path) => std::fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?,
        None => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf).context("failed to read stdin")?;
            buf
        }
    };

    let base_dir =
        args.path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| PathBuf::from("."));

    let profile = args.profile.as_deref().unwrap_or(DEFAULT_PROFILE);
    let mut session = AnalysisSession::new();
    if let Some(locked) = schema_session(&args)? {
        session.add_schema_source(locked);
    }
    let options = AnalysisOptions::for_profile(profile).with_base_dir(base_dir).with_migrate(args.migrate);
    session.analyze_source(&source, &options).map_err(|err| anyhow::Error::msg(err.to_string()))?;

    eprintln!("ok: validated against profile `{profile}`");
    Ok(())
}

/// Optional locked import host shared by document and Bsol project validation.
fn schema_session(args: &ValidateBsolArgs) -> Result<Option<Box<dyn SchemaSource>>> {
    match (&args.schema_import_root, &args.schema_import_lock) {
        (Some(root), Some(lock_path)) => {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(lock_path)
                .context("failed to open schema import lock")?
                .take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            anyhow::ensure!(bytes.len() <= 1024 * 1024, "schema import lock exceeds 1 MiB");
            let locks: Vec<LockedImport> = serde_json::from_slice(&bytes).context("invalid schema import lock")?;
            anyhow::ensure!(locks.len() <= 4096, "schema import lock exceeds 4096 entries");
            let host: Box<dyn SchemaSource> = Box::new(LockedSchemaHost::open(root, locks, 16 * 1024 * 1024)?);
            Ok(Some(host))
        }
        (None, None) => Ok(None),
        _ => anyhow::bail!("schema import root and lock must be supplied together"),
    }
}

/// Validate every schema export of a `type = Bsol` project: each export path must name a
/// package-local schema document that validates as `schema.v1`, declares exactly the exported
/// profile name, and resolves all of its imports. Returns the validated profile names.
fn validate_bsol_project(
    manifest_path: &Path,
    manifest: &ProjectManifest,
    locked: Option<Box<dyn SchemaSource>>,
) -> Result<Vec<String>> {
    let schemas = manifest
        .project
        .schemas_section
        .as_ref()
        .filter(|schemas| !schemas.exports.is_empty())
        .with_context(|| format!("`{}` declares no schema exports to validate", manifest_path.display()))?;
    let project_root = manifest_path
        .parent()
        .map(|parent| if parent.as_os_str().is_empty() { Path::new(".") } else { parent })
        .context("project manifest has no parent directory")?
        .canonicalize()
        .with_context(|| format!("failed to resolve project root of `{}`", manifest_path.display()))?;

    let package: Box<dyn SchemaSource> = Box::new(PackageFileSchemaSource { root: project_root.clone() });
    let mut sources = CompositeSchemaSource::new(vec![package]);
    if let Some(locked) = locked {
        sources.add_source(locked);
    }

    let mut validated = Vec::with_capacity(schemas.exports.len());
    for export in &schemas.exports {
        let path = package_file(&project_root, &project_root, &export.path)
            .with_context(|| format!("schema export `{}`", export.name))?;
        let profile = load_package_profile(&path, &export.profile)
            .with_context(|| format!("schema export `{}`", export.name))?;
        let base_dir = path.parent().unwrap_or(&project_root).to_path_buf();
        resolve_active_profile(profile, &base_dir, &sources)
            .map_err(|err| anyhow::anyhow!("schema export `{}`: {err}", export.name))?;
        validated.push(export.profile.clone());
    }
    if let Some(default_profile) = &schemas.default_profile {
        anyhow::ensure!(
            validated.contains(default_profile),
            "default schema profile `{default_profile}` is not one of the exported profiles"
        );
    }
    Ok(validated)
}

/// Resolve `relative` against `base_dir`, requiring an existing file inside `root`.
fn package_file(root: &Path, base_dir: &Path, relative: &str) -> Result<PathBuf> {
    let candidate = Path::new(relative);
    anyhow::ensure!(candidate.is_relative(), "schema path `{relative}` must be relative to the package");
    let resolved = base_dir
        .join(candidate)
        .canonicalize()
        .with_context(|| format!("schema path `{relative}` does not name a readable file"))?;
    anyhow::ensure!(resolved.starts_with(root), "schema path `{relative}` escapes the package root");
    anyhow::ensure!(resolved.is_file(), "schema path `{relative}` is not a file");
    Ok(resolved)
}

fn load_package_profile(path: &Path, expected: &str) -> Result<SchemaProfile> {
    let source = std::fs::read_to_string(path).with_context(|| format!("failed to read `{}`", path.display()))?;
    let profile = validate_profile_document(&source)
        .map_err(|err| anyhow::anyhow!("`{}` is not a valid schema profile: {err}", path.display()))?;
    anyhow::ensure!(
        profile.name == expected,
        "`{}` declares profile `{}` but the export names `{expected}`",
        path.display(),
        profile.name
    );
    Ok(profile)
}

/// Package-local `from = file` schema imports, confined to the Bsol project root.
struct PackageFileSchemaSource {
    root: PathBuf,
}

impl SchemaSource for PackageFileSchemaSource {
    fn resolve(&self, spec: &ImportSchemaSpec, base_dir: &Path) -> Result<SchemaProfile, BsolError> {
        let ImportSource::File { path } = &spec.from else {
            return Err(BsolError::Import(format!(
                "schema import `{}` is not package-local; supply --schema-import-root and --schema-import-lock",
                spec.name
            )));
        };
        package_file(&self.root, base_dir, path)
            .and_then(|path| load_package_profile(&path, &spec.name))
            .map_err(|err| BsolError::Import(format!("schema import `{}`: {err:#}", spec.name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_minimal_manifest_fixture() {
        let src = r#"demo {
  name = "demo"
  version = "0.1.0"
  root = "."
}
"#;
        parse_bsol_document(src).expect("parse");
        let profile = load_profile("project.v1").expect("profile");
        validate(&parse_bsol_document(src).unwrap(), &profile).expect("validate");
    }

    const BASE_SCHEMA: &str = r#"profile "base.v1" {
  rule "option" {
    scope = top
    match = keyword
    keyword = option
    label = forbidden
    cardinality = many
    field "key" { type = quoted required = true }
  }
}
"#;

    const DERIVED_SCHEMA: &str = r#"profile "derived.v1" {
  import_schema "base.v1" {
    from = file
    path = "base.v1.bsol"
  }
  rule "derived" {
    scope = top
    match = keyword
    keyword = derived
    label = forbidden
    cardinality = one
    field "name" { type = quoted required = true }
  }
}
"#;

    fn bsol_project(exports: &[(&str, &str, &str)], files: &[(&str, &str)]) -> (tempfile::TempDir, PathBuf) {
        // The package lives one level down so `../` fixtures stay inside the temporary directory.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("pkg");
        std::fs::create_dir_all(&root).unwrap();
        for (path, text) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let exports = exports
            .iter()
            .map(|(name, profile, path)| {
                format!("    export \"{name}\" {{\n      profile = \"{profile}\"\n      path = \"{path}\"\n    }}\n")
            })
            .collect::<String>();
        let manifest = root.join("demo_schema.bproj");
        std::fs::write(
            &manifest,
            format!(
                "demo_schema {{\n  name = \"demo_schema\"\n  version = \"0.1.0\"\n  type = Bsol\n  schemas {{\n{exports}  }}\n}}\n"
            ),
        )
        .unwrap();
        (dir, manifest)
    }

    fn args(path: &Path) -> ValidateBsolArgs {
        ValidateBsolArgs {
            profile: None,
            migrate: false,
            schema_import_root: None,
            schema_import_lock: None,
            path: Some(path.to_path_buf()),
        }
    }

    #[test]
    fn bsol_project_validates_each_declared_export_and_its_file_imports() {
        let (_dir, manifest) = bsol_project(
            &[("base", "base.v1", "schemas/base.v1.bsol"), ("derived", "derived.v1", "schemas/derived.v1.bsol")],
            &[("schemas/base.v1.bsol", BASE_SCHEMA), ("schemas/derived.v1.bsol", DERIVED_SCHEMA)],
        );
        let loaded = load_manifest_from_path(&manifest).unwrap();
        let validated = validate_bsol_project(&manifest, &loaded, None).expect("declared exports validate");
        assert_eq!(validated, ["base.v1", "derived.v1"]);
        execute(args(&manifest)).expect("dev bsol validate accepts a Bsol project without --profile");
    }

    #[test]
    fn corelib_pest_gen_schema_package_validates_against_its_declared_exports() {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corelib/packages/pest-gen-schema/pest_gen_schema.bproj");
        let loaded = load_manifest_from_path(&manifest).unwrap();
        assert_eq!(loaded.project.kind, ProjectKind::Bsol);
        let validated = validate_bsol_project(&manifest, &loaded, None).expect("shipped schema package validates");
        assert_eq!(validated, ["configuration.v1", "pestGenConfig.v1"]);
    }

    #[test]
    fn bsol_project_rejects_an_explicit_profile() {
        let (_dir, manifest) =
            bsol_project(&[("base", "base.v1", "schemas/base.v1.bsol")], &[("schemas/base.v1.bsol", BASE_SCHEMA)]);
        let mut explicit = args(&manifest);
        explicit.profile = Some("project.v1".into());
        let error = execute(explicit).expect_err("Bsol projects validate their own exports");
        assert!(error.to_string().contains("omit --profile"), "{error}");
    }

    #[test]
    fn bsol_project_rejects_an_export_whose_document_declares_another_profile() {
        let (_dir, manifest) =
            bsol_project(&[("base", "other.v1", "schemas/base.v1.bsol")], &[("schemas/base.v1.bsol", BASE_SCHEMA)]);
        let loaded = load_manifest_from_path(&manifest).unwrap();
        let error = validate_bsol_project(&manifest, &loaded, None).expect_err("profile identity mismatch");
        assert!(format!("{error:#}").contains("declares profile `base.v1` but the export names `other.v1`"), "{error:#}");
    }

    #[test]
    fn bsol_project_rejects_missing_escaping_and_unresolved_schemas() {
        let (_dir, manifest) = bsol_project(&[("base", "base.v1", "schemas/missing.bsol")], &[]);
        assert!(execute(args(&manifest)).is_err(), "missing export file");

        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("base.v1.bsol"), BASE_SCHEMA).unwrap();
        let escaping = format!("{}/base.v1.bsol", outside.path().display());
        let (_dir, manifest) = bsol_project(&[("base", "base.v1", escaping.as_str())], &[]);
        assert!(execute(args(&manifest)).is_err(), "absolute export path");

        let (_dir, manifest) = bsol_project(
            &[("base", "base.v1", "../outside/base.v1.bsol")],
            &[("../outside/base.v1.bsol", BASE_SCHEMA)],
        );
        assert!(execute(args(&manifest)).is_err(), "export path escaping the package root");

        let (_dir, manifest) = bsol_project(
            &[("derived", "derived.v1", "schemas/derived.v1.bsol")],
            &[("schemas/derived.v1.bsol", DERIVED_SCHEMA)],
        );
        let loaded = load_manifest_from_path(&manifest).unwrap();
        let error = validate_bsol_project(&manifest, &loaded, None).expect_err("unresolved file import");
        assert!(format!("{error:#}").contains("base.v1"), "{error:#}");
    }

    #[test]
    fn non_bsol_project_manifest_keeps_the_project_profile_default() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("demo.bproj");
        std::fs::write(
            &manifest,
            "demo {\n  name = \"demo\"\n  version = \"0.1.0\"\n  root = \"src\"\n}\n\ntarget \"App\" {\n  kind = App\n  entry = \"Main.bd\"\n}\n",
        )
        .unwrap();
        execute(args(&manifest)).expect("ordinary project manifest validates against project.v1");
    }
}
