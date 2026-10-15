//! Package authority shared by native contract, factory, and callback selection.
use crate::CodegenInput;
use anyhow::{Result, bail};
use beskid_queries::AstNodeKey;

pub(super) fn require_sdk_package(input: &CodegenInput<'_>, key: AstNodeKey) -> Result<()> {
    if key.generation != input.typed_program().generation {
        bail!("native SDK declaration belongs to a stale generation");
    }
    let unit = input
        .typed_program()
        .assembly
        .units
        .iter()
        .find(|unit| beskid_queries::SourceUnitId::new(input.database(), unit.path.clone()) == key.unit)
        .ok_or_else(|| anyhow::anyhow!("native SDK declaration is outside the current assembly"))?;
    let package = input
        .typed_program()
        .assembly
        .package_identities()
        .validate_source(&unit.path, &unit.source)?
        .ok_or_else(|| anyhow::anyhow!("native SDK requires preparation-issued package authority"))?;
    if package.package_name() != "corelib_compiler_sdk"
        || !matches!(package.source(), beskid_analysis::projects::VerifiedPackageSource::Corelib)
    {
        bail!("copied local SDK declarations cannot authorize native Mod adapters");
    }
    Ok(())
}
