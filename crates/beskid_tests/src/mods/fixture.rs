//! Filesystem helpers for the `sample_mod` fixture used by end-to-end mod-host tests.
//!
//! Each test gets an isolated workspace that copies (or synthesizes) the fixture
//! tree under a unique temp dir so descriptors and registrations can be tweaked
//! without leaking across tests.

use std::fs;
use std::path::{Path, PathBuf};

use beskid_analysis::projects::{
    CompilePlan, ResolvedDependencyProject, Target, TargetKind,
};

use crate::test_harness::temp_case_dir;

const HOST_PROJECT_MANIFEST: &str = "Host.bproj";
const SAMPLE_MOD_PROJECT_MANIFEST: &str = "SampleMod.bproj";
const SAMPLE_MOD_PROJECT: &str = include_str!("../../fixtures/mods/sample_mod/SampleMod.bproj");
const SAMPLE_MOD_SOURCE: &str = include_str!("../../fixtures/mods/sample_mod/Src/Mod.bd");

/// One per-test workspace materialized under `temp_case_dir(prefix)`.
pub(crate) struct ModFixtureWorkspace {
    pub(crate) root: PathBuf,
    pub(crate) host_dir: PathBuf,
    pub(crate) mod_dir: PathBuf,
}

impl ModFixtureWorkspace {
    pub(crate) fn new(prefix: &str) -> Self {
        let root = temp_case_dir(prefix);
        let host_dir = root.join("Host");
        let mod_dir = root.join("SampleMod");
        fs::create_dir_all(host_dir.join("Src")).expect("host source root");
        fs::create_dir_all(mod_dir.join("Src")).expect("mod source root");
        fs::write(
            host_dir.join("Src").join("Main.bd"),
            "unit main() { return; }\n",
        )
        .expect("host source");
        fs::write(host_dir.join(HOST_PROJECT_MANIFEST), HOST_MANIFEST).expect("host manifest");
        fs::write(
            mod_dir.join(SAMPLE_MOD_PROJECT_MANIFEST),
            SAMPLE_MOD_PROJECT,
        )
        .expect("mod manifest");
        fs::write(mod_dir.join("Src").join("Mod.bd"), SAMPLE_MOD_SOURCE).expect("mod source");
        Self {
            root,
            host_dir,
            mod_dir,
        }
    }

    pub(crate) fn write_descriptor(&self, registrations_json: &str) -> PathBuf {
        let descriptor_dir = self
            .host_dir
            .join(".beskid")
            .join("obj")
            .join("mods")
            .join("SampleMod")
            .join("cache-key")
            .join("test-triple");
        fs::create_dir_all(&descriptor_dir).expect("descriptor dir");
        let descriptor_path = descriptor_dir.join("mod.descriptor.json");
        let descriptor = format!(
            r#"{{
  "schemaVersion": 1,
  "packageId": "SampleMod",
  "modSourceHash": "fixture-source",
  "lockHash": "fixture-lock",
  "targetTriple": "test-triple",
  "compilerVersion": "test",
  "objectFile": "mod.o",
  "registrations": {registrations_json}
}}"#
        );
        fs::write(&descriptor_path, descriptor).expect("write descriptor");
        descriptor_path
    }

    /// Default registration set covering all four contract kinds plus the
    /// AttributeGenerator surface used by the reference fixture.
    pub(crate) fn default_registrations_json() -> &'static str {
        r#"[
    { "contractId": "Beskid.Compiler.Collect.Collector",          "typeId": "SampleMod.SampleCollect",   "entrySymbol": "samplemod_collect" },
    { "contractId": "Beskid.Compiler.Collect.Generator",          "typeId": "SampleMod.SampleGenerate",  "entrySymbol": "samplemod_generate" },
    { "contractId": "Beskid.Compiler.Collect.AttributeGenerator", "typeId": "SampleMod.SampleAttribute", "entrySymbol": "samplemod_attribute" },
    { "contractId": "Beskid.Compiler.Collect.Analyzer",           "typeId": "SampleMod.SampleAnalyze",   "entrySymbol": "samplemod_analyze" },
    { "contractId": "Beskid.Compiler.Collect.Rewriter",           "typeId": "SampleMod.SampleRewrite",   "entrySymbol": "samplemod_rewrite" }
  ]"#
    }

    pub(crate) fn compile_plan(&self) -> CompilePlan {
        CompilePlan {
            project_root: self.host_dir.clone(),
            manifest_path: self.host_dir.join(HOST_PROJECT_MANIFEST),
            project_name: "Host".to_string(),
            source_root: self.host_dir.join("Src"),
            target: Target {
                name: "main".to_string(),
                kind: TargetKind::App,
                entry: Some("Main.bd".to_string()),
            },
            dependency_projects: vec![ResolvedDependencyProject {
                dependency_name: "SampleMod".to_string(),
                manifest_path: self.mod_dir.join(SAMPLE_MOD_PROJECT_MANIFEST),
                project_root: self.mod_dir.clone(),
                project_name: "SampleMod".to_string(),
                source_root: self.mod_dir.join("Src"),
            }],
            unresolved_dependencies: Vec::new(),
            has_std_dependency: false,
        }
    }

    pub(crate) fn host_source(&self) -> &'static str {
        "unit main() { return; }\n"
    }
}

impl Drop for ModFixtureWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const HOST_MANIFEST: &str = r#"
Host {
  name = "Host"
  version = "0.1.0"
}

target "main" {
  kind = App
  entry = "Main.bd"
}

dependency "SampleMod" {
  source = path
  path = "../SampleMod"
}
"#;

/// Walk a host workspace and return the descriptor JSON path if present.
#[allow(dead_code)]
pub(crate) fn descriptor_path_in(workspace: &Path) -> PathBuf {
    workspace
        .join(".beskid")
        .join("obj")
        .join("mods")
        .join("SampleMod")
        .join("cache-key")
        .join("test-triple")
        .join("mod.descriptor.json")
}
