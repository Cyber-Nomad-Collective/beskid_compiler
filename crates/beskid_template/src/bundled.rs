//! The application template is compiled into every installed CLI.
#![allow(non_snake_case)]
use anyhow::{Result, bail};
use std::path::Path;

pub(crate) fn ApplicationTemplate(name: Option<&str>) -> Result<tempfile::TempDir> {
    let name = name.ok_or_else(|| anyhow::anyhow!("bundled app requires a project name"))?;
    let mut chars = name.chars();
    if !chars.next().is_some_and(|value| value.is_ascii_alphabetic() || value == '_')
        || !chars.all(|value| value.is_ascii_alphanumeric() || value == '_')
    {
        bail!(
            "project name must start with a letter or underscore and contain letters, digits or underscores; for example `beskid new hello`"
        );
    }
    let root = tempfile::tempdir()?;
    Write(root.path(), ".beskid/template.json", include_bytes!("../bundled/app/.beskid/template.json"))?;
    Write(root.path(), "{{name}}.bproj", include_bytes!("../bundled/app/{{name}}.bproj"))?;
    Write(root.path(), "Src/Main.bd", include_bytes!("../bundled/app/Src/Main.bd"))?;
    Ok(root)
}
fn Write(root: &Path, relative: &str, bytes: &[u8]) -> Result<()> {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("bundled path has parent"))?;
    std::fs::write(path, bytes)?;
    Ok(())
}
