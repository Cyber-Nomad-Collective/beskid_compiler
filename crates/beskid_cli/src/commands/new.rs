//! `beskid new` — list, install, uninstall, and instantiate project templates.

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};

use beskid_template::{
    InstallTemplateRequest, InstantiateTemplateRequest, ListTemplatesRequest, TemplateSelector,
    UninstallTemplateRequest, count_selectors, install_template, instantiate_template,
    list_templates, parse_kind_filter, parse_symbol_flag, uninstall_template,
};
use beskid_tools::registry::{RegistryConnectConfig, parse_package_selector};

#[derive(Args, Debug)]
pub struct NewArgs {
    /// Open the interactive template picker TUI (installed + registry download).
    #[arg(long)]
    pub tui: bool,

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

    #[command(flatten)]
    pub registry: RegistryConnectArgs,
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
    #[arg(long, env = "BESKID_PCKG_URL", default_value = "https://pckg.beskid-lang.org")]
    pub registry_url: String,

    #[arg(long, env = "BESKID_PCKG_TOKEN")]
    pub bearer_token: Option<String>,

    #[arg(long, env = "BESKID_PCKG_API_KEY")]
    pub api_key: Option<String>,
}

impl RegistryConnectArgs {
    fn to_config(&self) -> RegistryConnectConfig {
        let mut config = RegistryConnectConfig::new(&self.registry_url);
        if let Some(token) = &self.bearer_token {
            config = config.with_bearer_token(token);
        } else if let Some(key) = &self.api_key {
            config = config.with_api_key(key);
        }
        config
    }
}

pub fn execute(args: NewArgs) -> Result<()> {
    if args.tui && args.command.is_none() && args.short_name.is_none() {
        return beskid_tools::tui::run_project_wizard().map_err(Into::into);
    }
    match args.command {
        Some(NewCommand::List(list)) => execute_list(list),
        Some(NewCommand::Install(install)) => execute_install(install),
        Some(NewCommand::Uninstall(uninstall)) => execute_uninstall(uninstall),
        None => execute_instantiate(args.short_name, args.instantiate),
    }
}

fn execute_list(args: ListArgs) -> Result<()> {
    let kind_filter = args.kind.as_deref().map(parse_kind_filter).transpose()?;

    let output = list_templates(ListTemplatesRequest {
        kind_filter,
        online: args.online,
        registry: args.registry.to_config(),
    })?;

    println!("Installed templates:");
    for row in &output.installed {
        let yanked = if row.yanked { " [yanked]" } else { "" };
        let version = row
            .version
            .as_deref()
            .map(|v| format!("@{v}"))
            .unwrap_or_default();
        let package = row.package_id.as_deref().unwrap_or("—");
        println!(
            "  {} — {} ({:?}){} [{:?}] {}{}",
            row.short_name, row.name, row.kind, yanked, row.source, package, version
        );
    }

    if !output.registry.is_empty() {
        println!("\nRegistry packages:");
        for row in &output.registry {
            println!("  {} — {}", row.package_id, row.description);
        }
    }

    Ok(())
}

fn execute_install(args: InstallArgs) -> Result<()> {
    let result = install_template(InstallTemplateRequest {
        package_or_short: args.package_or_short,
        path: args.path,
        git: args.git,
        git_ref: args.git_ref,
        git_subpath: args.git_subpath,
        registry: args.registry.to_config(),
    })?;
    println!(
        "Installed template `{}` at {}",
        result.install_dir.join("manifest.snapshot.json").display(),
        result.install_dir.display()
    );
    Ok(())
}

fn execute_uninstall(args: UninstallArgs) -> Result<()> {
    let result = uninstall_template(UninstallTemplateRequest {
        short_name: args.short_name.clone(),
    })?;
    if result.removed {
        println!("Uninstalled template `{}`.", args.short_name);
    } else {
        println!(
            "No installed template with short name `{}`.",
            args.short_name
        );
    }
    Ok(())
}

fn execute_instantiate(short_name: Option<String>, flags: InstantiateFlags) -> Result<()> {
    let selector = build_selector(short_name, &flags)?;
    if count_selectors(&selector) != 1 {
        anyhow::bail!(
            "exactly one template selector is required: shortName, --package, --path, or --git"
        );
    }

    let output = flags
        .output
        .clone()
        .ok_or_else(|| anyhow!("`-o` / `--output` is required"))?;

    let mut symbols = Vec::new();
    for flag in &flags.symbols {
        let (id, value) = parse_symbol_flag(flag).map_err(|e| anyhow!("{e}"))?;
        symbols.push((id, value));
    }

    let result = instantiate_template(InstantiateTemplateRequest {
        selector,
        output,
        name: flags.name,
        symbols,
        no_interactive: flags.no_interactive,
        force: flags.force,
        host_project: flags.project,
        allow_yanked: flags.allow_yanked,
        strict_post_actions: flags.strict_post_actions,
        allow_project_manifest: flags.allow_project_manifest,
        registry: flags.registry.to_config(),
        beskid_exe: Some(std::env::current_exe()?),
    })?;

    println!(
        "Created template output at {}",
        result.output_root.display()
    );
    Ok(())
}

fn build_selector(
    short_name: Option<String>,
    flags: &InstantiateFlags,
) -> Result<TemplateSelector> {
    if let Some(path) = &flags.path {
        return Ok(TemplateSelector::Path(path.clone()));
    }
    if let Some(url) = &flags.git {
        return Ok(TemplateSelector::Git {
            url: url.clone(),
            git_ref: flags.git_ref.clone(),
            subpath: flags.git_subpath.clone(),
        });
    }
    if let Some(package) = &flags.package {
        let (id, version) = parse_package_selector(package)?;
        return Ok(TemplateSelector::Package { id, version });
    }
    let short = short_name.ok_or_else(|| anyhow!("template short name required"))?;
    Ok(TemplateSelector::ShortName(short))
}
