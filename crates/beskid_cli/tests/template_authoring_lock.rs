//! Template-authoring packages have dependencies and portable locks, but no compile target.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
};

use beskid_analysis::projects::{ProjectLockSource, ProjectLockfileV2, load_project_lock_dependencies};

struct TemplateCase {
    root: PathBuf,
    project: PathBuf,
}

impl TemplateCase {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).expect("clock").as_nanos();
        let root = std::env::temp_dir().join(format!("beskid_template_author_lock_{}_{nonce}", std::process::id()));
        fs::create_dir_all(&root).expect("fixture root");
        let project = root.join("generated");
        Self { root, project }
    }

    fn cli(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_beskid_cli"));
        command.current_dir(&self.root).env("BESKID_CORELIB_ROOT", self.root.join("installed-corelib"));
        command
    }

    fn invoke(&self, operation: &str, flags: &[&str]) -> Output {
        self.cli()
            .arg(operation)
            .arg("--project")
            .arg(&self.project)
            .arg("--plain")
            .args(flags)
            .output()
            .expect("invoke CLI")
    }

    fn generate(&self) -> Output {
        // Same authoring manifest/content contract as first-party `project`, with a
        // literal output filename to isolate locking from the separate path-substitution fix.
        let template = self.root.join("template");
        fs::create_dir_all(template.join(".beskid")).expect("template metadata");
        fs::create_dir_all(template.join("content/content")).expect("template payload");
        fs::write(
            template.join(".beskid/template.json"),
            r#"{
              "schema": "beskid.template.v1", "identity": "test.project::1.0.0",
              "name": "Authoring package", "shortName": "template",
              "tags": { "type": "project" },
              "symbols": {
                "name": { "type": "string", "isRequired": true },
                "shortName": { "type": "string", "isRequired": true }
              },
              "sources": [{ "source": "./content/", "target": "./" }]
            }"#,
        )
        .expect("template manifest");
        fs::write(
            template.join("content/AuthorPackage.bproj"),
            "{{name}} {\n  name = \"{{name}}\"\n  version = \"0.1.0\"\n  type = Template\n  template {\n    shortName \
             = \"{{shortName}}\"\n    identity = \"{{name}}\"\n  }\n}\n",
        )
        .expect("authoring manifest");
        fs::write(template.join("content/content/README.md"), "Template payload\n").expect("payload");
        self.cli()
            .args(["new", "--path"])
            .arg(template)
            .args(["--name", "AuthorPackage", "--symbol", "shortName=author-package", "--no-interactive", "-o"])
            .arg(&self.project)
            .output()
            .expect("generate authoring project")
    }

    fn lock(&self) -> String {
        fs::read_to_string(self.project.join("Project.lock")).expect("generated lock")
    }
}

impl Drop for TemplateCase {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_success(output: Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn new_template_authoring_project_locks_corelib_and_replays_after_relocation() {
    let mut case = TemplateCase::new();
    assert_success(case.generate(), "new authoring package post-action");
    let original = case.lock();
    assert!(original.starts_with("# Project.lock v2\nroot_manifest=AuthorPackage.bproj\nproject_name=AuthorPackage\n"));
    ProjectLockfileV2::parse_v2(&original).expect("portable v2 lock");
    let entries = load_project_lock_dependencies(&case.project).expect("Corelib lock entries");
    assert!(!entries.is_empty(), "authoring lock omitted Corelib closure");
    assert!(entries.iter().all(|entry| entry.source() == ProjectLockSource::Corelib));
    let mut names = entries.iter().map(|entry| entry.name()).collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "Std",
            "corelib_compiler_sdk",
            "corelib_concurrency",
            "corelib_console",
            "corelib_foundation",
            "corelib_glue",
            "corelib_http",
            "corelib_interop",
            "corelib_network",
            "corelib_runtime",
        ],
        "authoring lock must contain the complete installed Corelib closure"
    );
    assert!(!original.contains(&case.root.display().to_string()), "lock contains machine paths");

    assert_success(case.invoke("update", &[]), "update authoring package");
    assert_eq!(case.lock(), original, "update changed portable identity");
    let relocated = case.root.join("relocated");
    fs::rename(&case.project, &relocated).expect("move generated checkout");
    case.project = relocated;
    assert_success(case.invoke("fetch", &["--locked"]), "relocated locked replay");
    assert_success(case.invoke("fetch", &["--frozen"]), "relocated frozen replay");
    assert_eq!(case.lock(), original, "replay rewrote lock");
    assert!(!case.project.join("obj/beskid/root").exists(), "authoring package acquired a compile source tree");

    let build = case.invoke("build", &[]);
    assert!(!build.status.success(), "Template root must not compile");
    let diagnostic = String::from_utf8_lossy(&build.stderr);
    assert!(
        diagnostic.contains("`Template` projects are template-authoring roots"),
        "wrong Template build rejection: {diagnostic}"
    );
    assert!(!case.project.join("bin").exists(), "Template root produced a build artifact");

    let foreign = original.replace("project_name=AuthorPackage\n", "project_name=OtherPackage\n");
    fs::write(case.project.join("Project.lock"), &foreign).expect("copied foreign identity");
    let replay = case.invoke("fetch", &["--locked"]);
    assert!(!replay.status.success(), "authoring replay accepted a copied foreign lock");
    assert!(
        String::from_utf8_lossy(&replay.stderr).contains("lockfile belongs to a different project"),
        "wrong foreign-lock rejection: {}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert_eq!(case.lock(), foreign, "rejected replay rewrote foreign lock");
}

#[test]
fn template_authoring_lock_rejects_an_explicit_compile_target_without_outputs() {
    let case = TemplateCase::new();
    fs::create_dir_all(case.project.join("content")).expect("authoring payload directory");
    fs::write(
        case.project.join("AuthorPackage.bproj"),
        "AuthorPackage {\n name = \"AuthorPackage\"\n version = \"0.1.0\"\n type = Template\n}\n",
    )
    .expect("authoring manifest");
    let output = case.invoke("lock", &["--target", "App"]);
    assert!(!output.status.success(), "authoring lock accepted a compile target");
    assert!(String::from_utf8_lossy(&output.stderr).contains("`Template` projects are template-authoring roots"));
    assert!(!case.project.join("obj").exists(), "target rejection materialized outputs");
    assert!(!case.project.join("Project.lock").exists(), "target rejection wrote a lock");
}
