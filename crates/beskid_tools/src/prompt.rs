//! Stdin prompts shared by template and registry commands.

use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;

/// Ask whether to overwrite an existing output path.
pub fn confirm_overwrite(path: &Path) -> Result<bool> {
    print!("Output `{}` exists. Overwrite? [y/N] ", path.display());
    io::stdout().flush()?;
    read_yes_no()
}

/// Ask whether to continue after a yanked package warning.
pub fn confirm_yanked(package_id: &str, version: &str) -> Result<bool> {
    print!("Package `{package_id}@{version}` is yanked. Continue? [y/N] ");
    io::stdout().flush()?;
    read_yes_no()
}

fn read_yes_no() -> Result<bool> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}
