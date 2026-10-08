//! Preparation-issued package provenance. Empty synthetic proofs grant no stable identity.
use super::{ProjectError, ProjectLockDependencyEntry, ProjectLockSource, ProjectWorkspacePlan};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum VerifiedPackageSource {
    Local,
    Corelib,
    Registry { registry: String, artifact_digest: String },
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct VerifiedPackageIdentity {
    package_name: String,
    version: String,
    source: VerifiedPackageSource,
    source_digest: String,
}
impl VerifiedPackageIdentity {
    pub fn package_name(&self) -> &str {
        &self.package_name
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn source(&self) -> &VerifiedPackageSource {
        &self.source
    }
    pub fn source_digest(&self) -> &str {
        &self.source_digest
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedPackageRoot {
    identity: VerifiedPackageIdentity,
    roots: Vec<PathBuf>,
    files: BTreeMap<PathBuf, Vec<u8>>,
    manifest: PathBuf,
    manifest_bytes: Vec<u8>,
}
impl VerifiedPackageRoot {
    /// The package-relative path of a canonical `path` under the most specific containing root.
    ///
    /// A materialized copy can live inside the original root (`<project>/obj/beskid/root/...`), so
    /// the deepest root decides. When that root's proof does not list the file, the file is outside
    /// this package; a shallower root never takes over.
    fn owned_relative<'p>(&self, path: &'p Path) -> Option<&'p Path> {
        let relative = self.containing_relative(path)?;
        self.files.contains_key(relative).then_some(relative)
    }
    fn containing_relative<'p>(&self, path: &'p Path) -> Option<&'p Path> {
        self.roots
            .iter()
            .filter_map(|root| path.strip_prefix(root).ok())
            .min_by_key(|relative| relative.components().count())
    }
    pub fn relative_source_path(&self, path: &Path) -> Option<String> {
        let path = path.canonicalize().ok()?;
        let relative = self.owned_relative(&path)?;
        Some(relative.to_str()?.replace('\\', "/"))
    }
    /// Exact project-relative source evidence path, including a declared custom source root.
    pub fn project_relative_source_path(&self, path: &Path) -> Option<String> {
        let path = path.canonicalize().ok()?;
        let project = self.manifest.parent()?.canonicalize().ok()?;
        self.owned_relative(&path)?;
        let relative = path.strip_prefix(project).ok()?;
        if relative.components().any(|component| !matches!(component, std::path::Component::Normal(_))) { return None; }
        Some(relative.to_str()?.replace('\\', "/"))
    }
    pub fn identity(&self) -> &VerifiedPackageIdentity {
        &self.identity
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifiedPackageIdentities {
    entries: Vec<VerifiedPackageRoot>,
    lock: Option<(PathBuf, Vec<u8>)>,
}
fn error(message: impl Into<String>) -> ProjectError {
    ProjectError::Validation(message.into())
}
fn read(path: &Path) -> Result<Vec<u8>, ProjectError> {
    std::fs::read(path).map_err(|e| error(format!("package proof {}: {e}", path.display())))
}
/// Build and VCS directories never belong to a package.
pub(crate) fn is_build_or_vcs_directory(name: Option<&str>) -> bool {
    matches!(name, Some("obj" | ".beskid" | ".git" | "node_modules"))
}
/// A subdirectory with its own `.bproj` is a nested project with its own identity, so it is not part
/// of the enclosing package (the Cargo nested-package and Go nested-module rule).
fn is_nested_project_directory(directory: &Path) -> std::io::Result<bool> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().and_then(|e| e.to_str()) == Some("bproj") {
            return Ok(true);
        }
    }
    Ok(false)
}
/// The one rule deciding whether a directory below a package root is outside the package. Source
/// proof and materialization both use it, so a materialized copy and its original root hold the
/// same file set by construction.
pub(crate) fn is_outside_package(directory: &Path) -> std::io::Result<bool> {
    Ok(is_build_or_vcs_directory(directory.file_name().and_then(|name| name.to_str()))
        || is_nested_project_directory(directory)?)
}
fn files(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, ProjectError> {
    fn walk(root: &Path, at: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<(), ProjectError> {
        if at != root && is_build_or_vcs_directory(at.file_name().and_then(|name| name.to_str())) {
            return Ok(());
        }
        let metadata = std::fs::symlink_metadata(at).map_err(|e| error(e.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(error("package source proof rejects symlinks"));
        }
        if metadata.is_dir() {
            if at != root && is_outside_package(at).map_err(|e| error(e.to_string()))? {
                return Ok(());
            }
            for entry in std::fs::read_dir(at).map_err(|e| error(e.to_string()))? {
                walk(root, &entry.map_err(|e| error(e.to_string()))?.path(), result)?;
            }
        } else if metadata.is_file() {
            if at.extension().and_then(|extension| extension.to_str()) != Some("bd") {
                return Ok(());
            }
            let relative = at.strip_prefix(root).map_err(|e| error(e.to_string()))?.to_owned();
            if relative.to_str().is_none() {
                return Err(error("package source identity requires UTF-8 paths"));
            }
            result.insert(relative, read(at)?);
        } else {
            return Err(error("package source proof rejects nonregular entries"));
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result)?;
    Ok(result)
}
/// Whether `source_root` is physically the source root of a compiler-checkout Corelib package.
///
/// A host project checked directly from `corelib/packages/<package>` is that Corelib package, so
/// it carries the same identity it has as a dependency. The decision is the resolved physical
/// location of the compiler's own checkout, never a package name or a path spelling; a copy of
/// the package anywhere else stays a local package.
fn is_compiler_corelib_source_root(source_root: &Path) -> bool {
    let Ok(physical) = source_root.canonicalize() else {
        return false;
    };
    let Some(package) = physical.parent().and_then(Path::file_name).and_then(|name| name.to_str()) else {
        return false;
    };
    beskid_abi::runtime_source::compiler_corelib_package_source_root(package).is_some_and(|root| root == physical)
}
impl VerifiedPackageIdentities {
    /// The one package that owns `path`. The most specific containing root across all entries
    /// decides; two different packages claiming the same deepest root are ambiguous and own nothing.
    pub fn for_source(&self, path: &Path) -> Option<&VerifiedPackageRoot> {
        let path = path.canonicalize().ok()?;
        let mut deepest: Option<(usize, &VerifiedPackageRoot)> = None;
        let mut ambiguous = false;
        for entry in &self.entries {
            let Some(depth) = entry.containing_relative(&path).map(|relative| relative.components().count()) else {
                continue;
            };
            match deepest {
                Some((best, _)) if depth > best => {}
                Some((best, _)) if depth == best => ambiguous = true,
                _ => {
                    deepest = Some((depth, entry));
                    ambiguous = false;
                }
            }
        }
        let (_, entry) = deepest?;
        if ambiguous {
            return None;
        }
        entry.owned_relative(&path).map(|_| entry)
    }
    pub fn validate(&self) -> Result<(), ProjectError> {
        if let Some((path, bytes)) = &self.lock {
            if read(path)? != *bytes {
                return Err(error("package proof lock revision changed"));
            }
        }
        for entry in &self.entries {
            if read(&entry.manifest)? != entry.manifest_bytes {
                return Err(error("package proof manifest revision changed"));
            }
            for root in &entry.roots {
                if files(root)? != entry.files {
                    return Err(error("package proof source revision changed"));
                }
            }
        }
        Ok(())
    }
    pub fn validate_source(&self, path: &Path, source: &str) -> Result<Option<&VerifiedPackageIdentity>, ProjectError> {
        let Some(entry) = self.for_source(path) else {
            return Ok(None);
        };
        let path = path.canonicalize().map_err(|e| error(e.to_string()))?;
        let relative = entry.owned_relative(&path).ok_or_else(|| error("source outside package proof"))?;
        if entry.files.get(relative).map(Vec::as_slice) != Some(source.as_bytes()) {
            return Err(error(format!(
                "package proof differs from registered source snapshot for {} (bytes differ)",
                relative.display()
            )));
        }
        Ok(Some(&entry.identity))
    }
    pub(crate) fn prepare(
        plan: &ProjectWorkspacePlan,
        materialized_source: &Path,
        dependencies: &[super::MaterializedDependencyProject],
        locks: &[ProjectLockDependencyEntry],
        lock_path: &Path,
    ) -> Result<Self, ProjectError> {
        let mut result = Self { entries: Vec::new(), lock: Some((lock_path.to_owned(), read(lock_path)?)) };
        if let Some(source) = &plan.source_root {
            let identity_source = if is_compiler_corelib_source_root(source) {
                VerifiedPackageSource::Corelib
            } else {
                VerifiedPackageSource::Local
            };
            let roots = vec![source.clone(), materialized_source.to_owned()];
            result.issue(&plan.manifest_path, roots, identity_source, None)?;
        }
        for dependency in dependencies {
            let pin = locks
                .iter()
                .find(|pin| pin.name() == dependency.dependency_name)
                .ok_or_else(|| error("materialized package lacks lock provenance"))?;
            let (source, version) = match pin.source() {
                ProjectLockSource::Registry => (
                    VerifiedPackageSource::Registry {
                        registry: pin.registry().ok_or_else(|| error("registry identity missing"))?.to_owned(),
                        artifact_digest: pin
                            .artifact_digest()
                            .ok_or_else(|| error("artifact proof missing"))?
                            .to_owned(),
                    },
                    Some(pin.resolved_version().ok_or_else(|| error("exact registry version missing"))?),
                ),
                ProjectLockSource::Corelib => (VerifiedPackageSource::Corelib, None),
                ProjectLockSource::Path => (VerifiedPackageSource::Local, None),
            };
            let mut roots = vec![dependency.materialized_source_root.clone()];
            if let Some(original) =
                plan.dependency_projects.iter().find(|original| original.dependency_name == dependency.dependency_name)
            {
                roots.push(original.source_root.clone());
            }
            result.issue(&dependency.manifest_path, roots, source, version)?;
        }
        result.validate()?;
        Ok(result)
    }
    fn issue(
        &mut self,
        manifest: &Path,
        roots: Vec<PathBuf>,
        source: VerifiedPackageSource,
        locked_version: Option<&str>,
    ) -> Result<(), ProjectError> {
        let manifest_bytes = read(manifest)?;
        let parsed = super::load_manifest_from_path(manifest)?;
        let version = semver::Version::parse(&parsed.project.version)
            .map_err(|_| error("package identity requires exact declared SemVer"))?
            .to_string();
        if locked_version.is_some_and(|locked| locked != version) {
            return Err(error("package manifest version differs from verified registry lock"));
        }
        let mut canonical_roots = Vec::new();
        for root in roots {
            let root = root.canonicalize().map_err(|e| error(e.to_string()))?;
            if !canonical_roots.contains(&root) {
                canonical_roots.push(root);
            }
        }
        let source_files = files(&canonical_roots[0])?;
        let mut digest = Sha256::new();
        digest.update(b"beskid.package.sources.v1\0");
        for (path, bytes) in &source_files {
            let path = path.to_str().ok_or_else(|| error("non-UTF8 source path"))?.replace('\\', "/");
            digest.update((path.len() as u64).to_be_bytes());
            digest.update(path.as_bytes());
            digest.update((bytes.len() as u64).to_be_bytes());
            digest.update(bytes);
        }
        self.entries.push(VerifiedPackageRoot {
            identity: VerifiedPackageIdentity {
                package_name: parsed.project.name,
                version,
                source,
                source_digest: format!("{:x}", digest.finalize()),
            },
            roots: canonical_roots,
            files: source_files,
            manifest: manifest.to_owned(),
            manifest_bytes,
        });
        Ok(())
    }
}

#[cfg(test)]
mod corelib_host_identity_tests {
    use super::is_compiler_corelib_source_root;

    #[test]
    fn only_the_compiler_checkout_corelib_package_root_is_a_corelib_host() {
        let foundation = beskid_abi::runtime_source::compiler_corelib_package_source_root("foundation")
            .expect("compiler checkout Foundation source root");
        assert!(is_compiler_corelib_source_root(&foundation), "the checkout package checked directly is Corelib");

        let copy = std::env::temp_dir()
            .join(format!("beskid_corelib_host_identity_{}", std::process::id()))
            .join("packages/foundation/src");
        std::fs::create_dir_all(&copy).expect("copied package source root");
        assert!(!is_compiler_corelib_source_root(&copy), "a package named like Foundation elsewhere stays local");
        let _ = std::fs::remove_dir_all(copy.ancestors().nth(2).expect("copy base"));
        assert!(beskid_abi::runtime_source::compiler_corelib_package_source_root("../foundation").is_none());
    }
}

#[cfg(test)]
mod source_ownership_tests {
    use super::{VerifiedPackageIdentities, VerifiedPackageSource};

    fn package(at: &std::path::Path, name: &str) -> std::path::PathBuf {
        std::fs::create_dir_all(at).unwrap();
        let manifest = at.join(format!("{name}.bproj"));
        std::fs::write(&manifest, format!(
                "{name} {{ name = \"{name}\" version = \"1.0.0\" root = \".\" }}\ntarget \"{name}Lib\" {{ kind = Lib }}\n"
            )).unwrap();
        manifest
    }

    #[test]
    fn most_specific_root_owns_a_source_and_equal_roots_of_different_packages_are_ambiguous() {
        let base = std::env::temp_dir().join(format!("beskid_source_ownership_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let host = base.join("host");
        let host_manifest = package(&host, "Host");
        std::fs::write(host.join("Main.bd"), "pub type Main { pub i32 Value, }\n").unwrap();
        let materialized = host.join("obj/beskid/root/Host");
        std::fs::create_dir_all(&materialized).unwrap();
        std::fs::write(materialized.join("Main.bd"), "pub type Main { pub i32 Value, }\n").unwrap();
        let dependency = host.join("obj/beskid/root/Dep");
        let dependency_manifest = package(&base.join("dep"), "Dep");
        std::fs::create_dir_all(&dependency).unwrap();
        std::fs::write(dependency.join("Lib.bd"), "pub type Lib { pub i32 Value, }\n").unwrap();

        let mut identities = VerifiedPackageIdentities::default();
        identities.issue(&host_manifest, vec![host.clone(), materialized.clone()], VerifiedPackageSource::Local, None).unwrap();
        identities.issue(&dependency_manifest, vec![dependency.clone()], VerifiedPackageSource::Local, None).unwrap();
        let owner = identities.for_source(&materialized.join("Main.bd")).unwrap();
        assert_eq!(owner.identity().package_name(), "Host");
        assert_eq!(owner.relative_source_path(&materialized.join("Main.bd")).as_deref(), Some("Main.bd"));
        assert_eq!(identities.for_source(&dependency.join("Lib.bd")).unwrap().identity().package_name(), "Dep");

        // A second package proving the very same root is true ambiguity: nobody owns the source.
        let rival_manifest = package(&base.join("rival"), "Rival");
        identities.issue(&rival_manifest, vec![dependency.clone()], VerifiedPackageSource::Local, None).unwrap();
        assert!(identities.for_source(&dependency.join("Lib.bd")).is_none());
        assert_eq!(identities.for_source(&materialized.join("Main.bd")).unwrap().identity().package_name(), "Host");
        let _ = std::fs::remove_dir_all(&base);
    }
}
