//! v0.3 link-time extern resolution (platform-spec). Ignored until link driver implements Standard path.

use anyhow::Result;

#[test]
#[ignore = "v0.3 FFI impl: link-time extern resolution"]
fn link_time_extern_getpid_matches_platform_spec() -> Result<()> {
    // Placeholder: compile artifact with Extern contract + link against libc via project.link metadata.
    Ok(())
}

#[test]
#[ignore = "v0.3 FFI impl: export symbol emission"]
fn export_plugin_init_visible_to_linker() -> Result<()> {
    Ok(())
}

#[test]
#[ignore = "v0.3 FFI impl: callback registration table"]
fn host_registers_callbacks_with_layout_band() -> Result<()> {
    Ok(())
}
