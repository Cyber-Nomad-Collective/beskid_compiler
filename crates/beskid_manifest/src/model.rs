use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ManifestRoot {
    pub manifest: ManifestMeta,
    pub kernel: Vec<KernelEntry>,
    #[serde(default)]
    pub dispatch: DispatchTables,
    #[serde(default)]
    pub intrinsic: Vec<IntrinsicEntry>,
    #[serde(default)]
    pub profiles: ManifestProfiles,
}

#[derive(Debug, Default, Deserialize)]
pub struct ManifestProfiles {
    #[serde(default)]
    pub minimal: ProfileEntry,
    #[serde(default)]
    pub std: ProfileEntry,
}

#[derive(Debug, Default, Deserialize)]
pub struct ProfileEntry {
    #[serde(default)]
    pub owners: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DispatchTables {
    #[serde(default)]
    pub usize: Vec<DispatchEntry>,
    #[serde(default)]
    pub ptr: Vec<DispatchEntry>,
    #[serde(default)]
    pub unit: Vec<DispatchEntry>,
    #[serde(default)]
    pub i64: Vec<DispatchEntry>,
}

#[derive(Debug, Deserialize)]
pub struct ManifestMeta {
    pub abi_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KernelEntry {
    pub symbol: String,
    pub name: String,
    #[serde(default)]
    pub params: Vec<String>,
    pub returns: String,
    #[serde(default)]
    pub injected: bool,
    #[serde(default)]
    pub beskid_path: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DispatchEntry {
    /// Stable routing key for codegen and dispatch lookup (e.g. `str_len`).
    #[serde(alias = "legacy_symbol")]
    pub dispatch_key: String,
    pub name: String,
    pub tag: u32,
    #[serde(default)]
    pub params: Vec<String>,
    pub returns: String,
    #[serde(default = "default_true")]
    pub injected: bool,
    #[serde(default)]
    pub beskid_path: Vec<String>,
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub language_handler: bool,
}

impl DispatchEntry {
    pub fn is_host(&self) -> bool {
        self.owner == "host"
    }

    pub fn is_language_handler(&self) -> bool {
        self.language_handler
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntrinsicEntry {
    pub symbol: String,
    pub path: Vec<String>,
    #[serde(default)]
    pub params: Vec<String>,
    pub returns: String,
    #[serde(default)]
    pub injected: bool,
}
