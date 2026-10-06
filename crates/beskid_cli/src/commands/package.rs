#![allow(non_snake_case)]
use super::new::{InstallArgs, ListArgs, UninstallArgs};
use anyhow::Result;
use beskid_pckg::cli::{
    ConfigureArgs, DetailsArgs, PackArgs, PckgArgs, PckgCommand, PublishArgs, SearchArgs,
};
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct PackageArgs {
    #[arg(
        long,
        env = "BESKID_PCKG_URL",
        default_value = "https://pckg.beskid-lang.org"
    )]
    pub base_url: String,
    #[arg(long, env = "BESKID_PCKG_TOKEN", conflicts_with = "api_key")]
    pub bearer_token: Option<String>,
    #[arg(long, env = "BESKID_PCKG_API_KEY", conflicts_with = "bearer_token")]
    pub api_key: Option<String>,
    #[arg(long, default_value_t = 30)]
    pub timeout_secs: u64,
    #[arg(long, default_value = ".beskid/pckg/repositories.json")]
    pub config_file: PathBuf,
    #[arg(short, long)]
    pub verbose: bool,
    #[command(subcommand)]
    pub command: PackageCommand,
}
#[derive(Subcommand, Debug)]
pub enum PackageCommand {
    Search(SearchArgs),
    Info(DetailsArgs),
    Pack(PackArgs),
    Publish(PublishArgs),
    /// Persist an explicitly supplied publisher key for this repository.
    Login(LoginArgs),
    /// Remove saved authentication for the selected repository.
    Logout,
    Template(TemplateArgs),
}
#[derive(Args, Debug)]
pub struct LoginArgs {
    /// Explicit key to persist; environment authentication is not copied.
    #[arg(long = "key")]
    pub key: String,
}
#[derive(Args, Debug)]
pub struct TemplateArgs {
    #[command(subcommand)]
    pub command: TemplateCommand,
}
#[derive(Subcommand, Debug)]
pub enum TemplateCommand {
    List(ListArgs),
    Install(InstallArgs),
    Uninstall(UninstallArgs),
}

pub fn execute(args: PackageArgs) -> Result<()> {
    let command = match args.command {
        PackageCommand::Search(value) => PckgCommand::Search(value),
        PackageCommand::Info(value) => PckgCommand::Details(value),
        PackageCommand::Pack(value) => PckgCommand::Pack(value),
        PackageCommand::Publish(value) => PckgCommand::Upload(value),
        PackageCommand::Login(value) => PckgCommand::Configure(ConfigureArgs {
            repository_url: None,
            api_key: value.key,
        }),
        PackageCommand::Logout => return LogoutRepository(&args.config_file, &args.base_url),
        PackageCommand::Template(value) => {
            return match value.command {
                TemplateCommand::List(value) => super::new::execute_list(value),
                TemplateCommand::Install(value) => super::new::execute_install(value),
                TemplateCommand::Uninstall(value) => super::new::execute_uninstall(value),
            };
        }
    };
    super::super::cli::app::execute_pckg(PckgArgs {
        base_url: args.base_url,
        bearer_token: args.bearer_token,
        api_key: args.api_key,
        timeout_secs: args.timeout_secs,
        config_file: args.config_file,
        verbose: args.verbose,
        command,
    })
}

fn LogoutRepository(path: &std::path::Path, base_url: &str) -> Result<()> {
    if !path.exists() {
        println!("No saved repository authentication.");
        return Ok(());
    }
    let mut config: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
    let repository = config
        .get_mut("repositories")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| {
            anyhow::anyhow!("saved repository configuration is malformed; it was not changed")
        })?;
    let canonical = beskid_pckg::PckgClientConfig::new(base_url.trim())?
        .base_url
        .to_string();
    let key = canonical.trim_end_matches('/');
    repository.retain(|url, _| url.trim_end_matches('/') != key);
    let mut bytes = serde_json::to_vec_pretty(&config)?;
    bytes.push(b'\n');
    let pending = path.with_extension("logout.pending");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&pending)?;
    use std::io::Write;
    file.write_all(&bytes)?;
    file.sync_all()?;
    std::fs::rename(&pending, path)?;
    println!("Removed saved authentication for selected repository.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logout_preserves_other_repository_authentication() {
        let root = std::env::temp_dir().join(format!(
            "beskid-v06-logout-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("repositories.json");
        std::fs::write(&path, br#"{"repositories":{"https://one.invalid/":{"api_key":"fixture-one"},"https://two.invalid/":{"api_key":"fixture-two"}}}"#).unwrap();
        LogoutRepository(&path, "https://one.invalid").unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(saved["repositories"].get("https://one.invalid/").is_none());
        assert_eq!(
            saved["repositories"]["https://two.invalid/"]["api_key"],
            "fixture-two"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
