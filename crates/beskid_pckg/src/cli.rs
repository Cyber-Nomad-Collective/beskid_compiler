//! Clap surface and command execution for `beskid pckg` (pack, upload, registry queries).

use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{fs, io};

use crate::progress::UploadProgress;
use clap::{Args, Subcommand};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::models::PackageVersionSummaryResponse;
use crate::pack::{
    PackProfile, PackProfileOverride, build_package_json, collect_pack_entries, detect_pack_profile_with_override,
    strip_template_pack_excludes, strip_tool_pack_excludes, zip_to_pckg_error,
};
use crate::{PckgClient, PckgClientConfig, PckgError};

mod arguments;
mod commands;
mod output;
mod pack;
mod repository;
mod versioning;

#[cfg(test)]
mod tests;

pub use self::arguments::{
    ConfigureArgs, DetailsArgs, DownloadArgs, PackArgs, PackArgsPackageKind, PckgArgs, PckgCommand, PublishArgs,
    SearchArgs, VersionActionArgs, VersionsArgs,
};
pub use self::commands::execute;
