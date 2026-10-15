//! On-disk Beskid module path resolution against effective source roots.

use std::fmt;
use std::path::{Path, PathBuf};

use super::roots::{EffectiveCompilationRoots, RootEntry};

pub fn module_path_to_relative_path(module_path: &str) -> PathBuf {
    let normalized = module_path.replace("::", ".");
    let segments: Vec<&str> = normalized.split('.').filter(|segment| !segment.is_empty()).collect();
    let mut relative = PathBuf::new();
    for segment in segments {
        relative.push(segment);
    }
    relative
}

fn module_file_candidates(relative: &std::path::Path, source_root: &Path) -> Vec<PathBuf> {
    let flat = relative.with_extension("bd");
    let homonymous = relative
        .file_name()
        .map(|name| relative.join(name).with_extension("bd"))
        .unwrap_or_else(|| relative.with_extension("bd"));
    let mod_file = relative.join("mod.bd");
    let mut out = vec![flat, homonymous, mod_file];
    if let Some(generated_root) = source_root.parent().map(|parent| parent.join(".generated")) {
        let generated = generated_root.join(relative).with_extension("g.bd");
        out.push(generated);
    }
    out
}

/// One source root of the resolved dependency closure that provides a logical module path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModulePathProvider {
    /// Dependency name of the providing package; `None` for the host project.
    pub package: Option<String>,
    /// Module file the package provides for the path.
    pub file: PathBuf,
}

impl fmt::Display for ModulePathProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.package {
            Some(package) => write!(f, "package `{package}` ({})", self.file.display()),
            None => write!(f, "the host project ({})", self.file.display()),
        }
    }
}

/// A logical module path that more than one package of the resolved dependency closure provides
/// (diagnostic E1104). Providers are listed in effective root order: host first, then
/// dependencies in plan order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbiguousModulePath {
    pub path: String,
    pub providers: Vec<ModulePathProvider>,
}

impl fmt::Display for AmbiguousModulePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "E1104: ambiguous module path `{}`: it is provided by ", self.path)?;
        for (index, provider) in self.providers.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(f, "{provider}")?;
        }
        f.write_str("; a module path must be provided by exactly one package of the dependency closure")
    }
}

impl std::error::Error for AmbiguousModulePath {}

/// Resolve `module_path` to the single module file that provides it in the resolved closure.
///
/// Every effective root (host, then dependencies) is searched. Inside one root the first existing
/// candidate form wins (flat, homonymous, `mod.bd`, generated). When roots of more than one package
/// provide the path, resolution fails closed with [`AmbiguousModulePath`]; roots that reach the
/// same canonical file are one provider, not an ambiguity.
pub fn resolve_module_file(
    module_path: &str,
    roots: &EffectiveCompilationRoots,
) -> Result<Option<PathBuf>, AmbiguousModulePath> {
    let relative = module_path_to_relative_path(module_path);
    if relative.as_os_str().is_empty() {
        return Ok(None);
    }
    let mut providers: Vec<ModulePathProvider> = Vec::new();
    let mut canonical_files: Vec<PathBuf> = Vec::new();
    for entry in root_entries(roots) {
        let root = &entry.source_root;
        let Some(file) = module_file_candidates(&relative, root)
            .into_iter()
            .map(|candidate| root.join(candidate))
            .find(|candidate| candidate.is_file())
        else {
            continue;
        };
        let canonical = file.canonicalize().unwrap_or_else(|_| file.clone());
        if canonical_files.contains(&canonical) {
            continue;
        }
        canonical_files.push(canonical);
        providers.push(ModulePathProvider { package: entry.dependency_name.clone(), file });
    }
    match providers.len() {
        0 => Ok(None),
        1 => Ok(providers.pop().map(|provider| provider.file)),
        _ => Err(AmbiguousModulePath { path: module_path.replace('.', "::"), providers }),
    }
}

pub fn module_path_exists_on_disk(module_path: &str, roots: &[PathBuf]) -> bool {
    if roots.is_empty() {
        return false;
    }
    let relative = module_path_to_relative_path(module_path);
    if relative.as_os_str().is_empty() {
        return false;
    }
    roots
        .iter()
        .any(|root| module_file_candidates(&relative, root).into_iter().any(|candidate| root.join(candidate).is_file()))
}

fn root_entries(roots: &EffectiveCompilationRoots) -> impl Iterator<Item = &RootEntry> {
    std::iter::once(&roots.host).chain(roots.dependencies.iter())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write(root: &Path, relative: &str) -> PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
        fs::write(&path, "pub i32 Value() { return 0; }\n").expect("write module");
        path
    }

    fn entry(name: Option<&str>, source_root: PathBuf) -> RootEntry {
        RootEntry { dependency_name: name.map(str::to_string), source_root }
    }

    #[test]
    fn single_provider_resolves_to_its_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let host = temp.path().join("app/src");
        let alpha = temp.path().join("alpha/src");
        fs::create_dir_all(&host).expect("host root");
        let file = write(&alpha, "Alpha/Codec.bd");
        let roots =
            EffectiveCompilationRoots { host: entry(None, host), dependencies: vec![entry(Some("alpha"), alpha)] };

        assert_eq!(resolve_module_file("Alpha::Codec", &roots), Ok(Some(file)));
        assert_eq!(resolve_module_file("Alpha::Missing", &roots), Ok(None));
        assert_eq!(resolve_module_file("", &roots), Ok(None));
    }

    #[test]
    fn module_path_provided_by_two_packages_is_ambiguous() {
        let temp = tempfile::tempdir().expect("tempdir");
        let host = temp.path().join("app/src");
        let alpha = temp.path().join("alpha/src");
        let beta = temp.path().join("beta/src");
        fs::create_dir_all(&host).expect("host root");
        let alpha_file = write(&alpha, "Shared/Codec.bd");
        let beta_file = write(&beta, "Shared/Codec/mod.bd");
        let roots = EffectiveCompilationRoots {
            host: entry(None, host),
            dependencies: vec![entry(Some("alpha"), alpha), entry(Some("beta"), beta)],
        };

        let error = resolve_module_file("Shared.Codec", &roots).expect_err("two packages provide the path");
        assert_eq!(error.path, "Shared::Codec");
        assert_eq!(
            error.providers,
            vec![
                ModulePathProvider { package: Some("alpha".into()), file: alpha_file.clone() },
                ModulePathProvider { package: Some("beta".into()), file: beta_file.clone() },
            ]
        );
        let message = error.to_string();
        assert!(message.starts_with("E1104: ambiguous module path `Shared::Codec`"), "{message}");
        assert!(message.contains("package `alpha`") && message.contains(&alpha_file.display().to_string()));
        assert!(message.contains("package `beta`") && message.contains(&beta_file.display().to_string()));
    }

    #[test]
    fn host_and_dependency_providing_the_same_path_is_ambiguous_in_root_order() {
        let temp = tempfile::tempdir().expect("tempdir");
        let host = temp.path().join("app/src");
        let alpha = temp.path().join("alpha/src");
        let host_file = write(&host, "Util.bd");
        let alpha_file = write(&alpha, "Util/Util.bd");
        let roots =
            EffectiveCompilationRoots { host: entry(None, host), dependencies: vec![entry(Some("alpha"), alpha)] };

        let error = resolve_module_file("Util", &roots).expect_err("host and dependency provide the path");
        assert_eq!(
            error.providers,
            vec![
                ModulePathProvider { package: None, file: host_file },
                ModulePathProvider { package: Some("alpha".into()), file: alpha_file },
            ]
        );
        assert!(error.to_string().contains("the host project"));
    }

    #[test]
    fn roots_reaching_the_same_file_are_one_provider() {
        let temp = tempfile::tempdir().expect("tempdir");
        let host = temp.path().join("app/src");
        let file = write(&host, "Util.bd");
        let roots = EffectiveCompilationRoots {
            host: entry(None, host.clone()),
            dependencies: vec![entry(Some("self_alias"), temp.path().join("app/./src"))],
        };

        assert_eq!(resolve_module_file("Util", &roots), Ok(Some(file)));
    }

    #[test]
    fn hub_module_and_its_subdirectory_in_one_package_are_not_ambiguous() {
        let temp = tempfile::tempdir().expect("tempdir");
        let host = temp.path().join("app/src");
        let alpha = temp.path().join("alpha/src");
        let beta = temp.path().join("beta/src");
        fs::create_dir_all(&host).expect("host root");
        let hub = write(&alpha, "Net/Http.bd");
        write(&alpha, "Net/Http/Http.bd");
        let child = write(&beta, "Net/Http/Client.bd");
        let roots = EffectiveCompilationRoots {
            host: entry(None, host),
            dependencies: vec![entry(Some("alpha"), alpha), entry(Some("beta"), beta)],
        };

        assert_eq!(resolve_module_file("Net::Http", &roots), Ok(Some(hub)));
        assert_eq!(resolve_module_file("Net::Http::Client", &roots), Ok(Some(child)));
    }
}
