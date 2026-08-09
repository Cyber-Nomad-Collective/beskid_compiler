use super::{Args, PackProfileOverride, PathBuf, Subcommand};

const DEFAULT_PCKG_CONFIG_PATH: &str = ".beskid/pckg/repositories.json";
/// Connection, auth, timeout, and subcommand selection for `beskid pckg`.
#[derive(Args, Debug, Clone)]
pub struct PckgArgs {
    /// pckg server base URL.
    #[arg(long, env = "BESKID_PCKG_URL", default_value = "https://pckg.beskid-lang.org")]
    pub base_url: String,

    /// Bearer token for authenticated endpoints.
    #[arg(long, env = "BESKID_PCKG_TOKEN", conflicts_with = "api_key")]
    pub bearer_token: Option<String>,

    /// Publisher API key for authenticated endpoints.
    #[arg(long, env = "BESKID_PCKG_API_KEY", conflicts_with = "bearer_token")]
    pub api_key: Option<String>,

    /// Request timeout in seconds.
    #[arg(long, default_value_t = 30)]
    pub timeout_secs: u64,

    /// Repository-local pckg config file path.
    #[arg(long, default_value = DEFAULT_PCKG_CONFIG_PATH)]
    pub config_file: PathBuf,

    /// Extra diagnostics (base URL, auth presence, timings).
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: PckgCommand,
}
/// Individual registry operations (pack locally or call HTTP APIs).
#[derive(Subcommand, Debug, Clone)]
pub enum PckgCommand {
    /// Build a publishable .bpk artifact from a package directory.
    Pack(PackArgs),

    /// Upload and publish a package artifact version.
    Upload(PublishArgs),

    /// Save repository-local API key config used by upload commands.
    Configure(ConfigureArgs),

    /// List visible packages.
    List,

    /// Search packages by free-text query.
    Search(SearchArgs),

    /// Show package details by id or name.
    Details(DetailsArgs),

    /// List package versions by package name.
    Versions(VersionsArgs),

    /// Download an artifact version to file.
    Download(DownloadArgs),

    /// Yank a package version.
    Yank(VersionActionArgs),

    /// Restore a previously yanked package version.
    Unyank(VersionActionArgs),

    /// Print current authenticated user profile.
    Whoami,
}

#[derive(Args, Debug, Clone)]
pub struct ConfigureArgs {
    /// Repository URL to associate with API key.
    ///
    /// Defaults to --base-url when omitted.
    #[arg(long)]
    pub repository_url: Option<String>,

    /// API key to persist in repository config.
    #[arg(long)]
    pub api_key: String,
}

#[derive(Args, Debug, Clone)]
pub struct PublishArgs {
    pub package: String,
    #[arg(long)]
    pub artifact: PathBuf,
    #[arg(long)]
    pub checksum_sha256: Option<String>,
    #[arg(long)]
    pub manifest_json: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct SearchArgs {
    pub query: String,
}

#[derive(Args, Debug, Clone)]
pub struct DetailsArgs {
    pub id_or_name: String,
}

#[derive(Args, Debug, Clone)]
pub struct VersionsArgs {
    pub package: String,
}

#[derive(Args, Debug, Clone)]
pub struct DownloadArgs {
    pub package: String,
    #[arg(long)]
    pub version: String,
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Args, Debug, Clone)]
pub struct VersionActionArgs {
    pub package: String,
    #[arg(long)]
    pub version: String,
}

/// `beskid pckg pack --package-kind` override (platform-spec packageKind tool, D-TOOL-PCKG-0004).
#[derive(clap::ValueEnum, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PackArgsPackageKind {
    /// Detect from `Project.proj` (library or template; the historical default).
    #[default]
    Auto,
    /// Pack a tool package: omits `documentation.apiJson`, strips `.beskid/docs/**`.
    Tool,
}

impl PackArgsPackageKind {
    fn as_override(self) -> PackProfileOverride {
        match self {
            Self::Auto => PackProfileOverride::Auto,
            Self::Tool => PackProfileOverride::Tool,
        }
    }
}

#[derive(Args, Debug, Clone)]
pub struct PackArgs {
    #[arg(long)]
    pub package: String,
    #[arg(long)]
    pub version: Option<String>,
    #[arg(long, default_value = ".")]
    pub source: PathBuf,
    #[arg(long)]
    pub output: PathBuf,
    #[arg(long, default_value = ".beskid/pckg-version-state.json")]
    pub version_state_file: PathBuf,
    /// Force a packageKind profile (`auto` honors `Project.proj`; `tool` packs a tool package
    /// per platform-spec D-TOOL-PCKG-0004 without requiring `Project.proj`).
    #[arg(long, value_enum, default_value_t = PackArgsPackageKind::Auto)]
    pub package_kind: PackArgsPackageKind,
}

impl PackArgs {
    pub fn package_kind_override(&self) -> PackProfileOverride {
        self.package_kind.as_override()
    }
}
