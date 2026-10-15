//! A Corelib checkout Mod is built on demand as its own root whenever a checkout project
//! activates it (the Corelib test matrix does). Its `Project.lock` must therefore be committed
//! and current: a missing lock is written into the checkout by that build, which mutates the
//! tree the test matrix proves unchanged and fails every target of a fresh CI checkout.

use beskid_analysis::projects::{
    ProjectKind, WorkspacePrepareOptions, build_compile_plan, load_manifest_from_path,
    prepare_project_workspace_with_options,
};

#[test]
fn every_checkout_corelib_mod_carries_a_current_committed_lock() {
    let corelib = beskid_abi::runtime_source::compiler_corelib_checkout_root().expect("compiler checkout Corelib");
    let mut mods = Vec::new();
    for entry in std::fs::read_dir(corelib.join("mods")).expect("Corelib mods directory") {
        let directory = entry.expect("Corelib mods entry").path();
        let Ok(entries) = std::fs::read_dir(&directory) else { continue };
        for manifest in entries.map(|entry| entry.expect("Mod project entry").path()) {
            if manifest.extension().and_then(|extension| extension.to_str()) != Some("bproj") {
                continue;
            }
            let parsed = load_manifest_from_path(&manifest).expect("Mod manifest");
            if parsed.project.kind == ProjectKind::Mod {
                mods.push(manifest);
            }
        }
    }
    assert!(!mods.is_empty(), "the Corelib checkout ships its Mods under mods/");
    for manifest in mods {
        let plan = build_compile_plan(&manifest, None).expect("Mod compile plan");
        // Frozen preparation never writes `Project.lock` and fails when it is missing or stale.
        let frozen = WorkspacePrepareOptions { offline: true, frozen: true, locked: true, refresh_lock: false };
        if let Err(error) = prepare_project_workspace_with_options(&plan, frozen, None) {
            panic!(
                "{} has no current committed Project.lock ({error}); regenerate it with `beskid update --all --offline --project {}`",
                manifest.display(),
                manifest.display()
            );
        }
    }
}
