//! `beskid new` — list, install, uninstall, and instantiate project templates.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use anyhow::{anyhow, Context, Result};
use clap::{Args, Subcommand};
use semver::Version;

use beskid_pckg::{PckgClient, PckgClientConfig, PckgError};
use beskid_template::{
    clone_or_update, collect_symbol_values, extract_bpk_to_dir, find_installed_by_short_name,
    install_from_tree, list_installed, load_manifest_from_template_root, load_registry_index,
    parse_symbol_flag, resolve_package_id, save_registry_index, stdin_is_interactive,
    uninstall_by_short_name, GitTemplateRef, InstallSnapshot, InstallSource, InstantiateOptions,
    RegistryIndexEntry, SymbolCollectOptions, TemplateManifest, TemplateOutputKind,
};

#[derive(Args, Debug)]
pub struct NewArgs {
    #[command(subcommand)]
    pub command: Option<NewCommand>,

    /// Template short name (e.g. `console`, `lib`) when not using a subcommand.
    #[arg(value_name = "SHORT_NAME")]
    pub short_name: Option<String>,

    #[command(flatten)]
    pub instantiate: InstantiateFlags,
}

#[derive(Subcommand, Debug)]
pub enum NewCommand {
    /// List installed (and optionally online) templates.
    List(ListArgs),
    /// Install a template package into the tooling cache.
    Install(InstallArgs),
    /// Remove a cached template by short name.
    Uninstall(UninstallArgs),
}

#[derive(Args, Debug, Clone)]
pub struct InstantiateFlags {
    /// Output directory or file path (item templates).
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Primary name symbol (default symbol id `name`).
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,

    /// Symbol binding (`id=value`), repeatable.
    #[arg(long = "symbol", value_name = "ID=VALUE")]
    pub symbols: Vec<String>,

    /// Do not prompt; fail when required symbols are missing.
    #[arg(long = "no-interactive")]
    pub no_interactive: bool,

    /// Allow writing into a non-empty output directory.
    #[arg(long)]
    pub force: bool,

    /// Load template from a local directory (contains `.beskid/template.json`).
    #[arg(long = "path")]
    pub path: Option<PathBuf>,

    /// Load template from a git remote.
    #[arg(long = "git")]
    pub git: Option<String>,

    #[arg(long = "git-ref")]
    pub git_ref: Option<String>,

    #[arg(long = "git-subpath")]
    pub git_subpath: Option<String>,

    /// Registry package id (`beskid.templates.*`) with optional `@version`.
    #[arg(long = "package")]
    pub package: Option<String>,

    /// Host `Project.proj` for item templates.
    #[arg(long = "project")]
    pub project: Option<PathBuf>,

    /// Continue after yanked-version warning.
    #[arg(long = "allow-yanked")]
    pub allow_yanked: bool,

    /// Fail on unknown post-action ids.
    #[arg(long = "strict-post-actions")]
    pub strict_post_actions: bool,

    /// Item template may emit `Project.proj`.
    #[arg(long = "allow-project-manifest")]
    pub allow_project_manifest: bool,

    /// pckg registry base URL (update checks / auto-install).
    #[arg(long, env = "BESKID_PCKG_URL", default_value = "http://127.0.0.1:8082")]
    pub registry_url: String,
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Query registry for `beskid.templates.*` packages.
    #[arg(long)]
    pub online: bool,

    /// Filter by `tags.type` (`project`, `workspace`, `item`).
    #[arg(long = "kind")]
    pub kind: Option<String>,

    #[command(flatten)]
    pub registry: RegistryConnectArgs,
}

#[derive(Args, Debug)]
pub struct InstallArgs {
    /// Package id (`beskid.templates.console`) or first-party short name.
    pub package_or_short: String,

    #[arg(long = "path")]
    pub path: Option<PathBuf>,

    #[arg(long = "git")]
    pub git: Option<String>,

    #[arg(long = "git-ref")]
    pub git_ref: Option<String>,

    #[arg(long = "git-subpath")]
    pub git_subpath: Option<String>,

    #[command(flatten)]
    pub registry: RegistryConnectArgs,
}

#[derive(Args, Debug)]
pub struct UninstallArgs {
    pub short_name: String,
}

#[derive(Args, Debug, Clone, Default)]
pub struct RegistryConnectArgs {
    #[arg(long, env = "BESKID_PCKG_URL", default_value = "http://127.0.0.1:8082")]
    pub registry_url: String,

    #[arg(long, env = "BESKID_PCKG_TOKEN")]
    pub bearer_token: Option<String>,

    #[arg(long, env = "BESKID_PCKG_API_KEY")]
    pub api_key: Option<String>,
}

pub fn execute(args: NewArgs) -> Result<()> {
    match args.command {
        Some(NewCommand::List(list)) => execute_list(list),
        Some(NewCommand::Install(install)) => execute_install(install),
        Some(NewCommand::Uninstall(uninstall)) => execute_uninstall(uninstall),
        None => execute_instantiate(args.short_name, args.instantiate),
    }
}

fn execute_list(args: ListArgs) -> Result<()> {
    let kind_filter = args.kind.as_deref().map(parse_kind_filter).transpose()?;

    println!("Installed templates:");
    for (snap, path) in list_installed()? {
        if let Some(kind) = kind_filter {
            let manifest = load_manifest_from_template_root(&path).ok();
            if manifest.as_ref().map(|m| m.output_kind()) != Some(kind) {
                continue;
            }
        }
        let yanked = if snap.yanked { " [yanked]" } else { "" };
        println!(
            "  {} — {} ({:?}){}",
            snap.short_name,
            snap.identity,
            snap.source,
            yanked
        );
    }

    if args.online {
        let client = build_pckg_client(&args.registry)?;
        let runtime = tokio_runtime()?;
        let packages = runtime.block_on(client.list_packages())?;
        println!("\nRegistry packages:");
        for pkg in packages {
            if !pkg.name.starts_with("beskid.templates.") {
                continue;
            }
            println!("  {} — {}", pkg.name, pkg.description);
        }
    }

    Ok(())
}

fn execute_install(args: InstallArgs) -> Result<()> {
    let (template_root, snapshot) = resolve_install_source(&args)?;
    let dest = install_from_tree(&template_root, snapshot)?;
    println!(
        "Installed template `{}` at {}",
        dest.join("manifest.snapshot.json").display(),
        dest.display()
    );
    Ok(())
}

fn execute_uninstall(args: UninstallArgs) -> Result<()> {
    if uninstall_by_short_name(&args.short_name)? {
        println!("Uninstalled template `{}`.", args.short_name);
    } else {
        println!("No installed template with short name `{}`.", args.short_name);
    }
    Ok(())
}

fn execute_instantiate(short_name: Option<String>, flags: InstantiateFlags) -> Result<()> {
    let selectors = count_selectors(&flags, short_name.is_some());
    if selectors != 1 {
        anyhow::bail!(
            "exactly one template selector is required: shortName, --package, --path, or --git"
        );
    }

    let output = flags
        .output
        .clone()
        .ok_or_else(|| anyhow!("`-o` / `--output` is required"))?;

    if stdin_is_interactive() && output.exists() && !flags.force
        && !confirm_overwrite(&output)? {
            anyhow::bail!("cancelled");
        }

    let (template_root, manifest, registry_meta) = resolve_template_for_instantiate(
        short_name.as_deref(),
        &flags,
    )?;

    if let Some((package_id, version, yanked)) = registry_meta {
        check_registry_version(&flags, &package_id, &version, yanked)?;
    }

    let mut bindings = std::collections::BTreeMap::new();
    for flag in &flags.symbols {
        let (id, value) = parse_symbol_flag(flag).map_err(|e| anyhow!("{e}"))?;
        bindings.insert(id, value);
    }

    let symbol_options = SymbolCollectOptions {
        interactive: stdin_is_interactive(),
        no_interactive: flags.no_interactive,
        primary_name: flags.name.clone(),
        bindings,
    };

    let _values = collect_symbol_values(&manifest, &symbol_options).map_err(|e| anyhow!("{e}"))?;

    let options = InstantiateOptions {
        template_root: template_root.clone(),
        output,
        host_project: flags.project.clone(),
        force: flags.force,
        allow_project_manifest: flags.allow_project_manifest,
        strict_post_actions: flags.strict_post_actions,
        symbol_options,
        skip_default_lock: false,
        beskid_exe: Some(std::env::current_exe()?),
    };

    let result = beskid_template::instantiate(&manifest, &options).map_err(|e| anyhow!("{e}"))?;
    println!(
        "Created template output at {}",
        result.output_root.display()
    );
    Ok(())
}

fn resolve_install_source(args: &InstallArgs) -> Result<(PathBuf, InstallSnapshot)> {
    let installed_at = chrono_lite_now();

    if let Some(path) = &args.path {
        let path = std::fs::canonicalize(path).with_context(|| format!("path {}", path.display()))?;
        let manifest = load_manifest_from_template_root(&path).map_err(|e| anyhow!("{e}"))?;
        let snapshot = InstallSnapshot {
            identity: manifest.identity.clone(),
            short_name: manifest.short_name.clone(),
            package_id: None,
            resolved_version: None,
            checksum: Some(beskid_template::checksum_dir(&path).map_err(|e| anyhow!("{e}"))?),
            installed_at: installed_at.clone(),
            source: InstallSource::Path,
            yanked: false,
        };
        return Ok((path, snapshot));
    }

    if let Some(url) = &args.git {
        let root = clone_or_update(
            &GitTemplateRef {
                url: url.clone(),
                git_ref: args.git_ref.clone(),
                subpath: args.git_subpath.clone(),
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
            checksum: Some(beskid_template::checksum_dir(&root).map_err(|e| anyhow!("{e}"))?),
            installed_at,
            source: InstallSource::Git,
            yanked: false,
        };
        return Ok((root, snapshot));
    }

    let package_id = resolve_install_package_id(&args.package_or_short)?;
    let (root, version, yanked) = fetch_registry_template(&args.registry, &package_id, None)?;
    let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
    let snapshot = InstallSnapshot {
        identity: manifest.identity.clone(),
        short_name: manifest.short_name.clone(),
        package_id: Some(package_id),
        resolved_version: Some(version),
        checksum: Some(beskid_template::checksum_dir(&root).map_err(|e| anyhow!("{e}"))?),
        installed_at,
        source: InstallSource::Registry,
        yanked,
    };
    Ok((root, snapshot))
}

fn resolve_template_for_instantiate(
    short_name: Option<&str>,
    flags: &InstantiateFlags,
) -> Result<(PathBuf, TemplateManifest, Option<(String, String, bool)>)> {
    if let Some(path) = &flags.path {
        let path = std::fs::canonicalize(path).with_context(|| format!("path {}", path.display()))?;
        let manifest = load_manifest_from_template_root(&path).map_err(|e| anyhow!("{e}"))?;
        return Ok((path, manifest, None));
    }

    if let Some(url) = &flags.git {
        let root = clone_or_update(
            &GitTemplateRef {
                url: url.clone(),
                git_ref: flags.git_ref.clone(),
                subpath: flags.git_subpath.clone(),
            },
            false,
        )
        .map_err(|e| anyhow!("{e}"))?;
        let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
        return Ok((root, manifest, None));
    }

    if let Some(package) = &flags.package {
        let (id, version) = parse_package_selector(package)?;
        let registry = RegistryConnectArgs {
            registry_url: flags.registry_url.clone(),
            bearer_token: None,
            api_key: None,
        };
        let (root, resolved, yanked) = fetch_registry_template(&registry, &id, version.as_deref())?;
        let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
        return Ok((root, manifest, Some((id, resolved, yanked))));
    }

    let short = short_name.ok_or_else(|| anyhow!("template short name required"))?;
    if let Some((snap, path)) = find_installed_by_short_name(short).map_err(|e| anyhow!("{e}"))? {
        let root = path;
        let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
        let meta = snap
            .package_id
            .clone()
            .zip(snap.resolved_version.clone())
            .map(|(id, ver)| (id, ver, snap.yanked));
        return Ok((root, manifest, meta));
    }

    let package_id = resolve_package_id(short)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("unknown short name `{short}`"))?;
    let registry = RegistryConnectArgs {
        registry_url: flags.registry_url.clone(),
        bearer_token: None,
        api_key: None,
    };
    match fetch_registry_template(&registry, &package_id, None) {
        Ok((root, version, yanked)) => {
            let manifest = load_manifest_from_template_root(&root).map_err(|e| anyhow!("{e}"))?;
            let snapshot = InstallSnapshot {
                identity: manifest.identity.clone(),
                short_name: manifest.short_name.clone(),
                package_id: Some(package_id.clone()),
                resolved_version: Some(version.clone()),
                checksum: Some(beskid_template::checksum_dir(&root).map_err(|e| anyhow!("{e}"))?),
                installed_at: chrono_lite_now(),
                source: InstallSource::Registry,
                yanked,
            };
            let installed = install_from_tree(&root, snapshot).unwrap_or(root);
            let manifest = load_manifest_from_template_root(&installed).map_err(|e| anyhow!("{e}"))?;
            Ok((installed, manifest, Some((package_id, version, yanked))))
        }
        Err(e) if is_network_error(&e) => Err(anyhow!(
            "template `{short}` is not installed and registry is unreachable: {e}"
        )),
        Err(e) => Err(e),
    }
}

fn fetch_registry_template(
    registry: &RegistryConnectArgs,
    package_id: &str,
    version: Option<&str>,
) -> Result<(PathBuf, String, bool)> {
    let client = build_pckg_client(registry)?;
    let runtime = tokio_runtime()?;
    let versions = runtime
        .block_on(client.list_package_versions(package_id))
        .map_err(pckg_to_anyhow)?;

    let chosen = pick_version(&versions, version)?;
    let yanked = chosen.is_yanked;
    let bytes = runtime
        .block_on(client.download_package_version(package_id, &chosen.version))
        .map_err(pckg_to_anyhow)?;

    let extract_dir = std::env::temp_dir().join(format!(
        "beskid-template-{}-{}",
        package_id.replace('.', "_"),
        chosen.version
    ));
    extract_bpk_to_dir(&bytes, &extract_dir).map_err(|e| anyhow!("{e}"))?;
    update_registry_index(package_id, &chosen.version)?;
    Ok((extract_dir, chosen.version.clone(), yanked))
}

fn check_registry_version(
    flags: &InstantiateFlags,
    package_id: &str,
    current_version: &str,
    yanked: bool,
) -> Result<()> {
    if yanked {
        eprintln!(
            "warning: template package `{package_id}@{current_version}` is yanked"
        );
        if !flags.allow_yanked {
            if stdin_is_interactive() {
                if !confirm_yanked(package_id, current_version)? {
                    anyhow::bail!("cancelled");
                }
            } else {
                eprintln!("use --allow-yanked to proceed without prompting");
            }
        }
    }

    let registry = RegistryConnectArgs {
        registry_url: flags.registry_url.clone(),
        bearer_token: None,
        api_key: None,
    };
    let client = build_pckg_client(&registry).ok();
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

fn pick_version(
    versions: &[beskid_pckg::models::PackageVersionSummaryResponse],
    explicit: Option<&str>,
) -> Result<beskid_pckg::models::PackageVersionSummaryResponse> {
    if let Some(v) = explicit {
        return versions
            .iter()
            .find(|entry| entry.version == v)
            .cloned()
            .ok_or_else(|| anyhow!("version `{v}` not found"));
    }
    latest_non_yanked(versions).ok_or_else(|| anyhow!("no published versions found"))
}

fn latest_non_yanked(
    versions: &[beskid_pckg::models::PackageVersionSummaryResponse],
) -> Option<beskid_pckg::models::PackageVersionSummaryResponse> {
    versions
        .iter()
        .filter(|v| !v.is_yanked)
        .filter_map(|v| Version::parse(&v.version).ok().map(|parsed| (parsed, v)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, v)| (*v).clone())
}

fn build_pckg_client(args: &RegistryConnectArgs) -> Result<PckgClient> {
    let mut config = PckgClientConfig::new(&args.registry_url)?;
    config = match (&args.bearer_token, &args.api_key) {
        (Some(token), _) => config.with_bearer_token(token),
        (None, Some(key)) => config.with_publisher_api_key(key),
        (None, None) => config,
    };
    PckgClient::new(config).map_err(pckg_to_anyhow)
}

fn tokio_runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("tokio runtime")
}

fn count_selectors(flags: &InstantiateFlags, has_short: bool) -> usize {
    let mut n = 0usize;
    if has_short {
        n += 1;
    }
    if flags.path.is_some() {
        n += 1;
    }
    if flags.git.is_some() {
        n += 1;
    }
    if flags.package.is_some() {
        n += 1;
    }
    n
}

fn parse_package_selector(selector: &str) -> Result<(String, Option<String>)> {
    if let Some((id, ver)) = selector.split_once('@') {
        Ok((id.to_string(), Some(ver.to_string())))
    } else {
        Ok((selector.to_string(), None))
    }
}

fn resolve_install_package_id(input: &str) -> Result<String> {
    if input.contains('.') {
        Ok(input.to_string())
    } else {
        resolve_package_id(input)
            .map(str::to_string)
            .ok_or_else(|| anyhow!("unknown short name `{input}`"))
    }
}

fn parse_kind_filter(kind: &str) -> Result<TemplateOutputKind> {
    match kind {
        "project" => Ok(TemplateOutputKind::Project),
        "workspace" => Ok(TemplateOutputKind::Workspace),
        "item" => Ok(TemplateOutputKind::Item),
        other => Err(anyhow!("unknown kind `{other}`")),
    }
}

fn confirm_overwrite(path: &Path) -> Result<bool> {
    print!(
        "Output `{}` exists. Overwrite? [y/N] ",
        path.display()
    );
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn confirm_yanked(package_id: &str, version: &str) -> Result<bool> {
    print!(
        "Package `{package_id}@{version}` is yanked. Continue? [y/N] "
    );
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn update_registry_index(package_id: &str, version: &str) -> Result<()> {
    let mut index = load_registry_index();
    index.packages.insert(
        package_id.to_string(),
        RegistryIndexEntry {
            latest_version: version.to_string(),
            checked_at: chrono_lite_now(),
        },
    );
    save_registry_index(&index).map_err(|e| anyhow!("{e}"))?;
    Ok(())
}

fn chrono_lite_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}

fn pckg_to_anyhow(err: PckgError) -> anyhow::Error {
    anyhow!("{err}")
}

fn is_network_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("registry") || err.to_string().contains("connect")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_package_selector() {
        let (id, ver) = parse_package_selector("beskid.templates.console@1.0.0").unwrap();
        assert_eq!(id, "beskid.templates.console");
        assert_eq!(ver.as_deref(), Some("1.0.0"));
    }
}
