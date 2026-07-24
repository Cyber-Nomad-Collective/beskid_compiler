//! High-level `beskid new` orchestration: list, install, uninstall, instantiate.

use std::path::PathBuf;

use anyhow::{Context, Result, anyhow};
use semver::Version;

use beskid_tools::prompt::{confirm_overwrite, confirm_yanked};
use beskid_tools::registry::{
    RegistryConnectConfig, build_pckg_client, is_network_error, latest_non_yanked, pckg_to_anyhow, pick_version,
    tokio_runtime,
};

use crate::{
    GitTemplateRef, InstallSnapshot, InstallSource, InstantiateOptions, InstantiateResult, RegistryIndexEntry,
    SymbolCollectOptions, TemplateManifest, TemplateOutputKind, clone_or_update, collect_symbol_values,
    extract_bpk_to_dir, find_installed_by_short_name, install_from_tree, list_installed,
    load_manifest_from_template_root, load_registry_index, resolve_package_id, save_registry_index,
    stdin_is_interactive, uninstall_by_short_name,
};

/// How a template is selected for install or instantiate.
#[derive(Debug, Clone)]
pub enum TemplateSelector {
    ShortName(String),
    Package { id: String, version: Option<String> },
    Path(PathBuf),
    Git { url: String, git_ref: Option<String>, subpath: Option<String> },
}

/// Request for `beskid new list`.
#[derive(Debug, Clone)]
pub struct ListTemplatesRequest {
    pub kind_filter: Option<TemplateOutputKind>,
    pub online: bool,
    pub registry: RegistryConnectConfig,
}

/// One installed template row for display.
#[derive(Debug, Clone)]
pub struct InstalledTemplateRow {
    pub short_name: String,
    pub name: String,
    pub identity: String,
    pub kind: TemplateOutputKind,
    pub package_id: Option<String>,
    pub version: Option<String>,
    pub source: InstallSource,
    pub yanked: bool,
}

/// Registry package row for display.
#[derive(Debug, Clone)]
pub struct RegistryTemplateRow {
    pub package_id: String,
    pub description: String,
}

/// Output of [`list_templates`].
#[derive(Debug, Clone, Default)]
pub struct ListTemplatesOutput {
    pub installed: Vec<InstalledTemplateRow>,
    pub registry: Vec<RegistryTemplateRow>,
}

/// Request for `beskid new install`.
#[derive(Debug, Clone)]
pub struct InstallTemplateRequest {
    pub package_or_short: String,
    pub path: Option<PathBuf>,
    pub git: Option<String>,
    pub git_ref: Option<String>,
    pub git_subpath: Option<String>,
    pub registry: RegistryConnectConfig,
}

/// Output of [`install_template`].
#[derive(Debug, Clone)]
pub struct InstallTemplateOutput {
    pub install_dir: PathBuf,
    pub short_name: String,
}

/// Request for `beskid new uninstall`.
#[derive(Debug, Clone)]
pub struct UninstallTemplateRequest {
    pub short_name: String,
}

/// Output of [`uninstall_template`].
#[derive(Debug, Clone)]
pub struct UninstallTemplateOutput {
    pub removed: bool,
}

/// Request for `beskid new <shortName>` / instantiate.
#[derive(Debug, Clone)]
pub struct InstantiateTemplateRequest {
    pub selector: TemplateSelector,
    pub output: PathBuf,
    pub name: Option<String>,
    pub symbols: Vec<(String, String)>,
    pub no_interactive: bool,
    pub force: bool,
    pub host_project: Option<PathBuf>,
    pub allow_yanked: bool,
    pub strict_post_actions: bool,
    pub allow_project_manifest: bool,
    pub registry: RegistryConnectConfig,
    pub beskid_exe: Option<PathBuf>,
}

/// List installed templates and optionally query the registry.
pub fn list_templates(request: ListTemplatesRequest) -> Result<ListTemplatesOutput> {
    let mut output = ListTemplatesOutput::default();

    for (snap, path) in list_installed()? {
        let manifest = load_manifest_from_template_root(&path).ok();
        let kind = manifest.as_ref().map(|m| m.output_kind()).unwrap_or(TemplateOutputKind::Project);
        if request.kind_filter.is_some_and(|filter| filter != kind) {
            continue;
        }
        output.installed.push(InstalledTemplateRow {
            short_name: snap.short_name.clone(),
            name: manifest.as_ref().map(|m| m.name.clone()).unwrap_or_else(|| snap.short_name.clone()),
            identity: snap.identity.clone(),
            kind,
            package_id: snap.package_id.clone(),
            version: snap.resolved_version.clone(),
            source: snap.source,
            yanked: snap.yanked,
        });
    }

    if request.online {
        let client = build_pckg_client(&request.registry)?;
        let runtime = tokio_runtime()?;
        let packages = runtime.block_on(client.list_packages())?;
        for pkg in packages {
            if !pkg.name.starts_with("beskid.templates.") {
                continue;
            }
            output.registry.push(RegistryTemplateRow { package_id: pkg.name, description: pkg.description });
        }
    }

    Ok(output)
}

/// Install a template into the tooling cache.
pub fn install_template(request: InstallTemplateRequest) -> Result<InstallTemplateOutput> {
    let (template_root, snapshot) = resolve_install_source(&request)?;
    let dest = install_from_tree(&template_root, snapshot)?;
    Ok(InstallTemplateOutput {
        install_dir: dest,
        short_name: load_manifest_from_template_root(&template_root)
            .map(|m| m.short_name)
            .unwrap_or_else(|_| request.package_or_short.clone()),
    })
}

/// Remove a cached template by short name.
pub fn uninstall_template(request: UninstallTemplateRequest) -> Result<UninstallTemplateOutput> {
    let removed = uninstall_by_short_name(&request.short_name)?;
    Ok(UninstallTemplateOutput { removed })
}

/// Instantiate a template into an output directory.
pub fn instantiate_template(request: InstantiateTemplateRequest) -> Result<InstantiateResult> {
    if stdin_is_interactive() && request.output.exists() && !request.force && !confirm_overwrite(&request.output)? {
        anyhow::bail!("cancelled");
    }

    let (template_root, manifest, registry_meta) =
        resolve_template_for_instantiate(&request.selector, &request.registry)?;

    if let Some((package_id, version, yanked)) = registry_meta {
        check_registry_version(&request, &package_id, &version, yanked)?;
    }

    let mut bindings = std::collections::BTreeMap::new();
    for (id, value) in &request.symbols {
        bindings.insert(id.clone(), value.clone());
    }

    let symbol_options = SymbolCollectOptions {
        interactive: stdin_is_interactive(),
        no_interactive: request.no_interactive,
        primary_name: request.name.clone(),
        bindings,
    };

    let _values = collect_symbol_values(&manifest, &symbol_options).map_err(|e| anyhow!("{e}"))?;

    let options = InstantiateOptions {
        template_root: template_root.clone(),
        output: request.output,
        host_project: request.host_project,
        force: request.force,
        allow_project_manifest: request.allow_project_manifest,
        strict_post_actions: request.strict_post_actions,
        symbol_options,
        skip_default_lock: false,
        beskid_exe: request.beskid_exe,
    };

    crate::instantiate(&manifest, &options).map_err(|e| anyhow!("{e}"))
}

fn resolve_install_source(request: &InstallTemplateRequest) -> Result<(PathBuf, InstallSnapshot)> {
    let installed_at = chrono_lite_now();

    if let Some(path) = &request.path {
        let path = std::fs::canonicalize(path).with_context(|| format!("path {}", path.display()))?;
        let manifest = load_manifest_from_template_root(&path).map_err(|e| anyhow!("{e}"))?;
        let snapshot = InstallSnapshot {
            identity: manifest.identity.clone(),
            short_name: manifest.short_name.clone(),
            package_id: None,
            resolved_version: None,
            checksum: Some(crate::checksum_dir(&path).map_err(|e| anyhow!("{e}"))?),
            installed_at: installed_at.clone(),
            source: InstallSource::Path,
            yanked: false,
        };
        return Ok((path, snapshot));
    }

    if let Some(url) = &request.git {
        let root = clone_or_update(
            &GitTemplateRef {
                url: url.clone(),
                git_ref: request.git_ref.clone(),
                subpath: request.git_subpath.clone(),
            },
            true,
        )
        .map_err(|e| anyhow!("{e}"))?;
        let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
        let snapshot = InstallSnapshot {
            identity: manifest.identity.clone(),
            short_name: manifest.short_name.clone(),
            package_id: None,
            resolved_version: None,
            checksum: Some(crate::checksum_dir(&root).map_err(|e| anyhow!("{e}"))?),
            installed_at,
            source: InstallSource::Git,
            yanked: false,
        };
        return Ok((root, snapshot));
    }

    let package_id = resolve_install_package_id(&request.package_or_short)?;
    let (root, version, yanked) = fetch_registry_template(&request.registry, &package_id, None)?;
    let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
    let snapshot = InstallSnapshot {
        identity: manifest.identity.clone(),
        short_name: manifest.short_name.clone(),
        package_id: Some(package_id),
        resolved_version: Some(version),
        checksum: Some(crate::checksum_dir(&root).map_err(|e| anyhow!("{e}"))?),
        installed_at,
        source: InstallSource::Registry,
        yanked,
    };
    Ok((root, snapshot))
}

type ResolvedTemplateForInstantiate = (PathBuf, TemplateManifest, Option<(String, String, bool)>);

fn resolve_template_for_instantiate(
    selector: &TemplateSelector,
    registry: &RegistryConnectConfig,
) -> Result<ResolvedTemplateForInstantiate> {
    match selector {
        TemplateSelector::Path(path) => {
            let path = std::fs::canonicalize(path).with_context(|| format!("path {}", path.display()))?;
            let manifest = load_manifest_from_template_root(&path).map_err(|e| anyhow!("{e}"))?;
            Ok((path, manifest, None))
        }
        TemplateSelector::Git { url, git_ref, subpath } => {
            let root = clone_or_update(
                &GitTemplateRef { url: url.clone(), git_ref: git_ref.clone(), subpath: subpath.clone() },
                false,
            )
            .map_err(|e| anyhow!("{e}"))?;
            let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
            Ok((root, manifest, None))
        }
        TemplateSelector::Package { id, version } => {
            let (root, resolved, yanked) = fetch_registry_template(registry, id, version.as_deref())?;
            let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
            Ok((root, manifest, Some((id.clone(), resolved, yanked))))
        }
        TemplateSelector::ShortName(short) => {
            if let Some((snap, path)) = find_installed_by_short_name(short).map_err(|e| anyhow!("{e}"))? {
                let manifest = load_manifest_from_template_root(&path).map_err(|e| anyhow!("{e}"))?;
                let meta =
                    snap.package_id.clone().zip(snap.resolved_version.clone()).map(|(id, ver)| (id, ver, snap.yanked));
                return Ok((path, manifest, meta));
            }

            let package_id =
                resolve_package_id(short).map(str::to_string).ok_or_else(|| anyhow!("unknown short name `{short}`"))?;
            match fetch_registry_template(registry, &package_id, None) {
                Ok((root, version, yanked)) => {
                    let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
                    let snapshot = InstallSnapshot {
                        identity: manifest.identity.clone(),
                        short_name: manifest.short_name.clone(),
                        package_id: Some(package_id.clone()),
                        resolved_version: Some(version.clone()),
                        checksum: Some(crate::checksum_dir(&root).map_err(|e| anyhow!("{e}"))?),
                        installed_at: chrono_lite_now(),
                        source: InstallSource::Registry,
                        yanked,
                    };
                    let installed = install_from_tree(&root, snapshot).unwrap_or(root);
                    let manifest = load_manifest_from_template_root(&installed).map_err(|e| anyhow!("{e}"))?;
                    Ok((installed, manifest, Some((package_id, version, yanked))))
                }
                Err(e) if is_network_error(&e) => {
                    Err(anyhow!("template `{short}` is not installed and registry is unreachable: {e}"))
                }
                Err(e) => Err(e),
            }
        }
    }
}

fn fetch_registry_template(
    registry: &RegistryConnectConfig,
    package_id: &str,
    version: Option<&str>,
) -> Result<(PathBuf, String, bool)> {
    let client = build_pckg_client(registry)?;
    let runtime = tokio_runtime()?;
    let versions = runtime.block_on(client.list_package_versions(package_id)).map_err(pckg_to_anyhow)?;

    let chosen = pick_version(&versions, version)?;
    let yanked = chosen.is_yanked;
    let bytes =
        runtime.block_on(client.download_package_version(package_id, &chosen.version)).map_err(pckg_to_anyhow)?;

    let extract_dir =
        std::env::temp_dir().join(format!("beskid-template-{}-{}", package_id.replace('.', "_"), chosen.version));
    extract_bpk_to_dir(&bytes, &extract_dir).map_err(|e| anyhow!("{e}"))?;
    update_registry_index(package_id, &chosen.version)?;
    Ok((extract_dir, chosen.version.clone(), yanked))
}

fn check_registry_version(
    request: &InstantiateTemplateRequest,
    package_id: &str,
    current_version: &str,
    yanked: bool,
) -> Result<()> {
    if yanked {
        eprintln!("warning: template package `{package_id}@{current_version}` is yanked");
        if !request.allow_yanked {
            if stdin_is_interactive() {
                if !confirm_yanked(package_id, current_version)? {
                    anyhow::bail!("cancelled");
                }
            } else {
                eprintln!("use --allow-yanked to proceed without prompting");
            }
        }
    }

    let client = build_pckg_client(&request.registry).ok();
    let Some(client) = client else {
        return Ok(());
    };
    let runtime = tokio_runtime().ok();
    let Some(runtime) = runtime else {
        return Ok(());
    };

    let versions = match runtime.block_on(client.list_package_versions(package_id)) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("warning: update check skipped (registry unreachable)");
            return Ok(());
        }
    };

    let latest = latest_non_yanked(&versions);
    if let Some(latest_ver) = latest {
        let current = Version::parse(current_version).ok();
        let latest_parsed = Version::parse(&latest_ver.version).ok();
        if let (Some(c), Some(l)) = (current, latest_parsed)
            && l > c
        {
            println!(
                "A newer template version is available: {package_id}@{}. Run `beskid new install {package_id}` to update.",
                latest_ver.version
            );
        }
    }
    Ok(())
}

fn resolve_install_package_id(input: &str) -> Result<String> {
    if input.contains('.') {
        Ok(input.to_string())
    } else {
        resolve_package_id(input).map(str::to_string).ok_or_else(|| anyhow!("unknown short name `{input}`"))
    }
}

fn update_registry_index(package_id: &str, version: &str) -> Result<()> {
    let mut index = load_registry_index();
    index.packages.insert(
        package_id.to_string(),
        RegistryIndexEntry { latest_version: version.to_string(), checked_at: chrono_lite_now() },
    );
    save_registry_index(&index).map_err(|e| anyhow!("{e}"))?;
    Ok(())
}

fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    secs.to_string()
}

/// Parse `tags.type` filter string for list commands.
pub fn parse_kind_filter(kind: &str) -> Result<TemplateOutputKind> {
    match kind {
        "project" => Ok(TemplateOutputKind::Project),
        "workspace" => Ok(TemplateOutputKind::Workspace),
        "item" => Ok(TemplateOutputKind::Item),
        other => Err(anyhow!("unknown kind `{other}`")),
    }
}

/// Count how many template selectors are active.
pub fn count_selectors(selector: &TemplateSelector) -> usize {
    match selector {
        TemplateSelector::ShortName(_) => 1,
        TemplateSelector::Package { .. } => 1,
        TemplateSelector::Path(_) => 1,
        TemplateSelector::Git { .. } => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beskid_tools::registry::parse_package_selector;

    #[test]
    fn selector_count_is_one_per_variant() {
        assert_eq!(count_selectors(&TemplateSelector::ShortName("x".into())), 1);
    }

    #[test]
    fn parses_package_selector() {
        let (id, ver) = parse_package_selector("beskid.templates.console@1.0.0").unwrap();
        assert_eq!(id, "beskid.templates.console");
        assert_eq!(ver.as_deref(), Some("1.0.0"));
    }
}
