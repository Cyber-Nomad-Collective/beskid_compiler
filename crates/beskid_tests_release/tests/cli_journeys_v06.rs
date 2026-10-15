//! CLI06-01..06: everyday CLI journeys against the installed candidate (`tooling--cli--command-surface`,
//! `tooling--project-scaffolding--beskid-new`, `tooling--cli--build-analyze-run-contract`).
use beskid_tests_release::installed::{Cli, Home, has_cursor_controls, transcript};
use beskid_tests_release::project::{self, snapshot, write_app, write_lib};
use beskid_tests_release::registry::Registry;
use std::path::{Path, PathBuf};
use std::process::Output;

const ROOT_FAMILIES: [&str; 14] =
    ["new", "check", "build", "run", "test", "fmt", "doc", "add", "remove", "update", "package", "toolchain", "doctor", "dev"];
const DESCRIBED: [&str; 8] = ["build", "run", "test", "doc", "remove", "update", "package", "toolchain"];
const DEV_ROUTES: [&str; 10] = ["syntax", "build", "project", "corelib", "runtime-kit", "mod", "import", "lsp", "bsol", "repl"];

fn run(cli: &Cli, home: &Home, args: &[&str]) -> Output {
    cli.run(cli.command(home).args(args))
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Subcommand names of a clap help text: the first token of indented lines under `Commands:`.
fn command_names(help: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_commands = false;
    for line in help.lines() {
        if line.starts_with("Commands:") {
            in_commands = true;
            continue;
        }
        if in_commands {
            if line.trim().is_empty() || !line.starts_with(' ') {
                in_commands = false;
                continue;
            }
            if let Some(name) = line.split_whitespace().next()
                && name.chars().all(|character| character.is_ascii_alphanumeric() || character == '-')
                && name != "help"
            {
                names.push(name.to_owned());
            }
        }
    }
    names
}

fn description_of(help: &str, command: &str) -> String {
    help.lines()
        .find(|line| line.starts_with("  ") && line.split_whitespace().next() == Some(command))
        .map(|line| line.trim_start().trim_start_matches(command).trim().to_owned())
        .unwrap_or_default()
}

fn local_template(root: &Path) -> PathBuf {
    let template = root.join("local-template");
    std::fs::create_dir_all(template.join(".beskid")).unwrap();
    std::fs::write(
        template.join(".beskid/template.json"),
        r#"{
        "schema":"beskid.template.v1", "identity":"test.offline", "name":"Offline",
        "shortName":"offline-local", "sources":[{"source":"./","target":"./","include":["README.md","Name.txt"]}]
    }"#,
    )
    .unwrap();
    std::fs::write(template.join("README.md"), "verified local bytes\n").unwrap();
    std::fs::write(template.join("Name.txt"), "{{name}}\n").unwrap();
    template
}

fn assert_success(output: &Output, what: &str) {
    assert!(output.status.success(), "{what} failed:\n{}", transcript(output));
}

// ---------------------------------------------------------------------------------------------
// CLI06-01 Discoverable help

#[test]
fn cli06_01_root_help_shows_add_example_and_new_add_run_sequence() {
    let cli = Cli::qualified();
    let home = Home::new();
    let help = run(cli, &home, &["--help"]);
    assert_success(&help, "root help");
    let text = stdout(&help);
    let new = text.find("beskid new").expect("root help shows a `beskid new` step");
    let add = text.find("beskid add").expect("root help shows a dependency add example");
    let run_step = text.find("beskid run").expect("root help shows a `beskid run` step");
    assert!(new < add && add < run_step, "root help orders new, add, run:\n{text}");
    assert!(text[add..].starts_with("beskid add <package>"), "the add example names a package placeholder:\n{text}");
    for family in DESCRIBED {
        let description = description_of(&text, family);
        assert!(!description.is_empty(), "{family} has a user-facing description:\n{text}");
        assert!(!description.contains("Args") && !description.contains("struct"), "{family} description is not argument-structure documentation: {description}");
    }
    assert!(!home.has_provisioning_writes(), "help must not provision");
}

#[test]
fn cli06_01_add_help_explains_effects_project_targeting_and_recovery() {
    let cli = Cli::qualified();
    let home = Home::new();
    let help = run(cli, &home, &["add", "--help"]);
    assert_success(&help, "add help");
    let text = stdout(&help).to_ascii_lowercase();
    assert!(text.contains("--project"), "add help explains project targeting:\n{text}");
    assert!(text.contains("lock"), "add help explains the lock effect:\n{text}");
    assert!(text.contains("dependency"), "add help explains the dependency effect:\n{text}");
    assert!(text.contains("--offline") && text.contains("--locked") && text.contains("--frozen"), "add help names the resolution policies used for recovery:\n{text}");
    assert!(!home.has_provisioning_writes(), "help must not provision");
}

// ---------------------------------------------------------------------------------------------
// CLI06-02 Bundled default application creation

#[test]
fn cli06_02_new_offline_bundled_app_builds_checks_runs() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let created = run(cli, &home, &["new", "hello", "--offline", "--registry-url", registry.url()]);
    assert_success(&created, "new hello --offline");
    let manifest = home.work().join("hello/hello.bproj");
    assert!(manifest.is_file(), "new creates the project manifest");
    assert!(home.work().join("hello/Src/Main.bd").is_file(), "new creates the application source");
    assert_eq!(registry.count(), 0, "offline bundled creation performs no registry request");
    let manifest_text = manifest.to_str().unwrap();
    assert_success(&run(cli, &home, &["check", "--plain", "--project", manifest_text]), "check of the bundled app");
    let executable = home.work().join(beskid_tests_release::installed::executable_name("hello-out"));
    assert_success(&run(cli, &home, &["build", "--plain", "--project", manifest_text, "--output", executable.to_str().unwrap()]), "build of the bundled app");
    assert!(executable.is_file(), "build writes the requested executable");
    assert_success(&run(cli, &home, &["run", "--plain", "--project", manifest_text]), "run of the bundled app");
}

#[test]
fn cli06_02_new_template_override_binds_name_and_output() {
    let cli = Cli::qualified();
    let home = Home::new();
    let template = local_template(&home.work());
    assert_success(&run(cli, &home, &["package", "template", "install", "offline-local", "--path", template.to_str().unwrap()]), "template install");
    let output = home.work().join("custom-output");
    let created = run(cli, &home, &["new", "ignored-name", "--offline", "--no-interactive", "--template", "offline-local", "--name", "Custom", "--output", output.to_str().unwrap()]);
    assert_success(&created, "new with template override");
    assert_eq!(std::fs::read_to_string(output.join("Name.txt")).unwrap(), "Custom\n", "the template binds the explicit name");
    assert_eq!(std::fs::read_to_string(output.join("README.md")).unwrap(), "verified local bytes\n");
    assert!(!home.work().join("ignored-name").exists(), "explicit --output wins over the positional default");
}

#[test]
fn cli06_02_new_nonempty_output_rejected_byte_for_byte() {
    let cli = Cli::qualified();
    let home = Home::new();
    let existing = home.work().join("hello");
    std::fs::create_dir_all(existing.join("notes")).unwrap();
    std::fs::write(existing.join("keep.txt"), "existing bytes\n").unwrap();
    std::fs::write(existing.join("notes/todo.txt"), "more\n").unwrap();
    let before = snapshot(&existing);
    let rejected = run(cli, &home, &["new", "hello", "--offline", "--no-interactive"]);
    assert_eq!(rejected.status.code(), Some(1), "nonempty output without --force fails:\n{}", transcript(&rejected));
    assert_eq!(snapshot(&existing), before, "every existing byte is preserved");
    assert!(stderr(&rejected).to_ascii_lowercase().contains("force") || stderr(&rejected).to_ascii_lowercase().contains("empty"), "the conflict is reported:\n{}", transcript(&rejected));
}

#[test]
fn cli06_02_new_offline_local_template_resolves_without_network() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let template = local_template(&home.work());
    let created = run(cli, &home, &["new", "local", "--offline", "--no-interactive", "--path", template.to_str().unwrap(), "--registry-url", registry.url()]);
    assert_success(&created, "new from a local template");
    assert_eq!(std::fs::read_to_string(home.work().join("local/README.md")).unwrap(), "verified local bytes\n");
    assert_eq!(registry.count(), 0);
}

#[test]
fn cli06_02_new_offline_installed_template_checks_bytes_before_output() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    let template = local_template(&home.work());
    assert_success(&run(cli, &home, &["package", "template", "install", "offline-local", "--path", template.to_str().unwrap(), "--registry-url", registry.url()]), "template install");
    let instantiate = |name: &str| run(cli, &home, &["new", name, "--offline", "--no-interactive", "--template", "offline-local", "--registry-url", registry.url()]);
    assert_success(&instantiate("warm"), "verified installed template");
    assert_eq!(std::fs::read_to_string(home.work().join("warm/README.md")).unwrap(), "verified local bytes\n");
    std::fs::write(home.config().join("templates/installed/test.offline/README.md"), "tampered\n").unwrap();
    let tampered = instantiate("tampered");
    assert_eq!(tampered.status.code(), Some(1), "tampered installed template fails:\n{}", transcript(&tampered));
    let diagnostic = stderr(&tampered).to_ascii_lowercase();
    assert!(diagnostic.contains("checksum") || diagnostic.contains("integrity"), "{diagnostic}");
    assert!(!home.work().join("tampered").exists(), "no output before verification");
    assert_eq!(registry.count(), 0);
}

// ---------------------------------------------------------------------------------------------
// CLI06-03 Canonical grammar, program streams and status

#[test]
fn cli06_03_inventory_matches_canonical_grammar() {
    let cli = Cli::qualified();
    let home = Home::new();
    let root = run(cli, &home, &["--help"]);
    assert_success(&root, "root help");
    let mut names = command_names(&stdout(&root));
    names.sort();
    let mut expected = ROOT_FAMILIES.map(str::to_owned).to_vec();
    expected.sort();
    assert_eq!(names, expected, "root families are exactly the canonical set");
    let dev = run(cli, &home, &["dev", "--help"]);
    assert_success(&dev, "dev help");
    let dev_names = command_names(&stdout(&dev));
    for route in DEV_ROUTES {
        assert!(dev_names.iter().any(|name| name == route), "dev lists {route}: {dev_names:?}");
    }
    assert!(dev_names.iter().any(|name| name == "capabilities"), "dev lists capabilities");
    for retired in ["test", "fmt", "doc", "update", "native-mod-worker"] {
        assert!(!dev_names.iter().any(|name| name == retired), "dev must not expose {retired}: {dev_names:?}");
    }
    let package = run(cli, &home, &["package", "--help"]);
    assert_success(&package, "package help");
    let package_names = command_names(&stdout(&package));
    for route in ["search", "info", "pack", "publish", "login", "logout", "template"] {
        assert!(package_names.iter().any(|name| name == route), "package lists {route}: {package_names:?}");
    }
    let template = run(cli, &home, &["package", "template", "--help"]);
    assert_success(&template, "package template help");
    let template_names = command_names(&stdout(&template));
    for route in ["list", "install", "uninstall"] {
        assert!(template_names.iter().any(|name| name == route), "package template lists {route}: {template_names:?}");
    }
    let toolchain = run(cli, &home, &["toolchain", "--help"]);
    assert_success(&toolchain, "toolchain help");
    let toolchain_names = command_names(&stdout(&toolchain));
    assert_eq!(toolchain_names, ["status", "update"], "toolchain routes");
    let bsol = run(cli, &home, &["dev", "bsol", "--help"]);
    assert_success(&bsol, "dev bsol help");
    assert_eq!(command_names(&stdout(&bsol)), ["validate", "migrate"]);
    let hi = run(cli, &home, &["hi"]);
    assert_eq!(hi.status.code(), Some(2), "hi is an unknown subcommand:\n{}", transcript(&hi));
    assert!(!hi.stdout.contains(&0x1b) && !hi.stderr.contains(&0x1b), "no alternate screen for an unknown command");
}

#[test]
fn cli06_03_run_reserves_stdout_for_program_output_and_propagates_exit_status() {
    let cli = Cli::qualified();
    let home = Home::new();
    let manifest = write_app(
        &home.work(),
        "Streams",
        "use Core.Args;\nuse Core.Error;\nuse Core.Output;\n\ni32 Main() {\n    Output.WriteLine(\"stdout-marker\");\n    Error.WriteLine(\"stderr-marker\");\n    string[] all = Args.All();\n    mut i64 index = 1;\n    while index < Args.Count() {\n        Output.WriteLine(\"arg:\" + all[index]);\n        index = index + 1;\n    }\n    return 7;\n}\n",
    );
    let plain = run(cli, &home, &["run", "--plain", "--project", manifest.to_str().unwrap()]);
    assert_eq!(plain.status.code(), Some(7), "run propagates the numeric child status:\n{}", transcript(&plain));
    assert_eq!(stdout(&plain), "stdout-marker\n", "stdout carries only program output:\n{}", transcript(&plain));
    assert!(stderr(&plain).contains("stderr-marker"), "stderr carries the program's stderr:\n{}", transcript(&plain));
    assert!(!has_cursor_controls(&plain.stderr), "redirected progress has no cursor controls:\n{}", transcript(&plain));
    let with_arguments = run(cli, &home, &["run", "--plain", "--project", manifest.to_str().unwrap(), "--", "", "two words", "--dash", "caf\u{e9}"]);
    assert_eq!(with_arguments.status.code(), Some(7), "trailing -- arguments are accepted:\n{}", transcript(&with_arguments));
    let text = stdout(&with_arguments);
    let lines: Vec<&str> = text.lines().skip(1).collect();
    assert_eq!(lines, ["arg:", "arg:two words", "arg:--dash", "arg:caf\u{e9}"], "argument boundaries, empty values, spaces, Unicode and leading dashes arrive unchanged");
}

#[test]
fn cli06_03_removed_root_spellings_exit_2_and_name_canonical_replacement() {
    let cli = Cli::qualified();
    let home = Home::new();
    for (old, replacement) in [("analyze", "check"), ("pckg", "package"), ("up", "toolchain"), ("fetch", "dev project fetch"), ("lsp", "dev lsp")] {
        let output = run(cli, &home, &[old]);
        assert_eq!(output.status.code(), Some(2), "`beskid {old}` is a usage error:\n{}", transcript(&output));
        let diagnostic = format!("{}{}", stdout(&output), stderr(&output));
        assert!(diagnostic.contains(&format!("beskid {replacement}")), "`beskid {old}` names `beskid {replacement}`:\n{diagnostic}");
        assert!(!diagnostic.contains("\u{1b}[?1049h"), "no alternate screen");
    }
    assert!(!home.has_provisioning_writes());
}

// ---------------------------------------------------------------------------------------------
// CLI06-04 Scoped provisioning, machine output, redirected completion

#[test]
fn cli06_04_help_new_list_doctor_without_corelib_or_network_make_no_provisioning_writes() {
    let cli = Cli::qualified();
    let home = Home::new();
    let registry = Registry::new();
    for args in [vec!["--help"], vec!["new", "--list", "--offline", "--registry-url", registry.url()], vec!["doctor"]] {
        let output = run(cli, &home, &args);
        assert_success(&output, &format!("{args:?}"));
        assert!(!output.stdout.is_empty(), "{args:?} returns useful output");
    }
    assert_eq!(registry.count(), 0, "no network request");
    assert!(!home.has_provisioning_writes(), "help, new --list and doctor provision nothing and touch no credentials");
}

#[test]
fn cli06_04_update_dry_run_json_stdout_has_only_schema_version_1_records() {
    let cli = Cli::qualified();
    let home = Home::new();
    write_lib(&home.work(), "Local", "pub i32 Answer() { return 42; }\n");
    let manifest = write_app(&home.work(), "App", "i32 Main() { return 0; }\n");
    assert_success(&run(cli, &home, &["add", "Local", "--path", "../Local", "--offline", "--project", manifest.to_str().unwrap()]), "add path dependency");
    let output = run(cli, &home, &["update", "Local", "--dry-run", "--json", "--offline", "--project", manifest.to_str().unwrap()]);
    assert_success(&output, "update --dry-run --json");
    let text = stdout(&output);
    assert!(!text.trim().is_empty(), "machine output is present");
    assert!(!has_cursor_controls(&output.stdout), "JSON stdout has no ANSI or progress");
    let mut saw_result = false;
    for line in text.lines() {
        let record: serde_json::Value = serde_json::from_str(line).unwrap_or_else(|error| panic!("stdout line is not a JSON record ({error}): {line}"));
        assert_eq!(record["schemaVersion"], 1, "every record carries schemaVersion=1: {line}");
        assert_eq!(record["command"], "update", "every record names the command: {line}");
        assert!(record["event"].is_string() && record["status"].is_string(), "every record carries event and status: {line}");
        saw_result |= record["event"] == "result";
    }
    assert!(saw_result, "a result record is emitted:\n{text}");
}

#[test]
fn cli06_04_run_json_exits_2_before_executing() {
    let cli = Cli::qualified();
    let home = Home::new();
    let manifest = write_app(&home.work(), "App", "use Core.Output;\ni32 Main() { Output.WriteLine(\"program-ran\"); return 0; }\n");
    let output = run(cli, &home, &["run", "--json", "--plain", "--project", manifest.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2), "run --json is a usage error:\n{}", transcript(&output));
    assert!(!stdout(&output).contains("program-ran"), "the program never executes");
    let diagnostic = stderr(&output);
    assert!(["check", "build", "test", "add", "remove", "update", "doctor"].iter().filter(|command| diagnostic.contains(*command)).count() >= 2, "usage guidance names the commands that support --json:\n{diagnostic}");
    assert!(!home.work().join("App/obj").exists(), "no executable preparation before the usage error");
}

#[test]
fn cli06_04_doctor_reports_missing_kit_without_installing_or_touching_credentials() {
    let cli = Cli::qualified();
    let home = Home::new();
    let incomplete_root = home.work().join("incomplete-prefix");
    let incomplete = cli.copied_into_incomplete_prefix(&incomplete_root);
    let before = snapshot(&incomplete_root);
    let output = incomplete.run(incomplete.command(&home).arg("doctor"));
    assert_eq!(output.status.code(), Some(1), "doctor on an incomplete installation fails:\n{}", transcript(&output));
    let diagnostic = stderr(&output).to_ascii_lowercase();
    assert!(diagnostic.contains("runtime kit") || diagnostic.contains("prefix") || diagnostic.contains("corelib"), "doctor names the missing component:\n{diagnostic}");
    assert!(diagnostic.contains("toolchain update") || diagnostic.contains("install"), "doctor gives an actionable next step:\n{diagnostic}");
    assert_eq!(snapshot(&incomplete_root), before, "doctor installs nothing into the prefix");
    assert!(!home.has_provisioning_writes(), "doctor touches no per-user state or credentials");
}

#[test]
fn cli06_04_redirected_build_and_test_complete_without_cursor_controls() {
    let cli = Cli::qualified();
    let home = Home::new();
    let project = home.work().join("Native");
    std::fs::create_dir_all(project.join("Src")).unwrap();
    std::fs::write(project.join("Src/Main.bd"), "i32 Main() { return 0; }\n").unwrap();
    std::fs::write(project.join("Src/Smoke.bd"), "use Testing.Assert;\ntest smoke_passes { Assert.True(true, \"redirected test\"); }\n").unwrap();
    let manifest = project.join("Native.bproj");
    std::fs::write(&manifest, "Native { name = \"Native\" version = \"0.1.0\" }\ntarget \"App\" { kind = \"App\" entry = \"Main.bd\" }\ntarget \"Smoke\" { kind = \"Test\" entry = \"Smoke.bd\" }\n").unwrap();
    let build = run(cli, &home, &["build", "--project", manifest.to_str().unwrap(), "--target", "App"]);
    assert_success(&build, "redirected build");
    assert!(!has_cursor_controls(&build.stdout) && !has_cursor_controls(&build.stderr), "redirected build streams carry no cursor controls:\n{}", transcript(&build));
    let test = run(cli, &home, &["test", "--project", manifest.to_str().unwrap(), "--target", "Smoke"]);
    assert_success(&test, "redirected test");
    assert!(!has_cursor_controls(&test.stdout) && !has_cursor_controls(&test.stderr), "redirected test streams carry no cursor controls:\n{}", transcript(&test));
    // stdout carries the readable result summary; per-test rows are progress on stderr.
    assert!(stdout(&test).contains("passed=1") && stdout(&test).contains("failed=0"), "the test summary is readable:\n{}", transcript(&test));
    assert!(stderr(&test).contains("smoke_passes"), "the passing test row names the test:\n{}", transcript(&test));
}

// ---------------------------------------------------------------------------------------------
// CLI06-05 Capability description

fn capabilities(cli: &Cli, home: &Home) -> serde_json::Value {
    let output = run(cli, home, &["dev", "capabilities", "--json"]);
    assert_success(&output, "dev capabilities --json");
    serde_json::from_str(stdout(&output).trim()).unwrap_or_else(|error| panic!("capabilities is one JSON document ({error}):\n{}", transcript(&output)))
}

#[test]
fn cli06_05_dev_capabilities_json_lists_version_canonical_commands_and_schemas() {
    let cli = Cli::qualified();
    let home = Home::new();
    let document = capabilities(cli, &home);
    assert_eq!(document["schemaVersion"], 1);
    let version = run(cli, &home, &["--version"]);
    assert_success(&version, "--version");
    let reported = stdout(&version).trim().strip_prefix("beskid ").expect("`beskid <version>`").to_owned();
    assert_eq!(document["cliVersion"], reported, "the capability version is the running CLI version");
    let mut commands: Vec<String> = document["commands"].as_array().expect("commands array").iter().map(|value| value.as_str().unwrap().to_owned()).collect();
    commands.sort();
    let mut expected = ROOT_FAMILIES.map(str::to_owned).to_vec();
    expected.sort();
    assert_eq!(commands, expected, "canonical commands");
    let schemas = document["outputSchemas"].as_array().expect("outputSchemas array");
    for command in ["check", "build", "test", "add", "remove", "update", "doctor"] {
        assert!(
            schemas.iter().any(|schema| schema["command"] == command && schema["schemaVersion"] == 1),
            "{command} --json output schema is described: {schemas:?}"
        );
    }
}

#[test]
fn cli06_05_help_inventory_matches_capability_inventory() {
    let cli = Cli::qualified();
    let home = Home::new();
    let document = capabilities(cli, &home);
    let mut capability_commands: Vec<String> = document["commands"].as_array().expect("commands").iter().map(|value| value.as_str().unwrap().to_owned()).collect();
    capability_commands.sort();
    let help = run(cli, &home, &["--help"]);
    assert_success(&help, "root help");
    let mut help_commands = command_names(&stdout(&help));
    help_commands.sort();
    assert_eq!(help_commands, capability_commands, "help and capability inventories agree");
    for command in &capability_commands {
        let output = run(cli, &home, &[command.as_str(), "--help"]);
        assert_success(&output, &format!("{command} --help"));
    }
}

// ---------------------------------------------------------------------------------------------
// CLI06-06 Scripted installed beginner journeys (the uncoached observation is release-user-observation.mjs)

fn assert_outside_checkout(path: &Path) {
    let text = path.to_string_lossy();
    assert!(!text.contains("/compiler/crates/") && !text.contains("\\compiler\\crates\\"), "journey runs outside the source checkout: {text}");
}

#[test]
fn cli06_06_create_add_use_check_build_run_outside_checkout() {
    let cli = Cli::qualified();
    let home = Home::new();
    assert_outside_checkout(&home.work());
    assert_success(&run(cli, &home, &["new", "hello", "--offline"]), "create");
    write_lib(&home.work(), "Greeter", "pub i32 Answer() { return 42; }\n");
    let manifest = home.work().join("hello/hello.bproj");
    let manifest_text = manifest.to_str().unwrap();
    let added = run(cli, &home, &["add", "Greeter", "--path", "../Greeter", "--offline", "--project", manifest_text]);
    assert_success(&added, "add");
    assert!(std::fs::read_to_string(&manifest).unwrap().contains("dependency \"Greeter\""), "add declares the dependency");
    assert!(manifest.with_file_name("Project.lock").is_file(), "add writes the lock");
    std::fs::write(home.work().join("hello/Src/Main.bd"), "i32 Main() { return Greeter.Answer() - 42; }\n").unwrap();
    assert_success(&run(cli, &home, &["check", "--plain", "--offline", "--project", manifest_text]), "check");
    // `work/hello` is the project directory; the executable goes beside it.
    let executable = home.work().join(beskid_tests_release::installed::executable_name("hello-app"));
    assert_success(&run(cli, &home, &["build", "--plain", "--offline", "--project", manifest_text, "--output", executable.to_str().unwrap()]), "build");
    assert!(executable.is_file());
    let ran = run(cli, &home, &["run", "--plain", "--offline", "--project", manifest_text]);
    assert_eq!(ran.status.code(), Some(0), "run uses the dependency:\n{}", transcript(&ran));
}

#[test]
fn cli06_06_path_dependency_workflow_outside_checkout() {
    let cli = Cli::qualified();
    let home = Home::new();
    assert_outside_checkout(&home.work());
    write_lib(&home.work(), "Alpha", "pub i32 Value() { return 1; }\n");
    write_lib(&home.work(), "Beta", "pub i32 Value() { return 2; }\n");
    let manifest = write_app(&home.work(), "App", "i32 Main() { return Alpha.Value() + Beta.Value() - 3; }\n");
    let manifest_text = manifest.to_str().unwrap();
    for name in ["Alpha", "Beta"] {
        assert_success(&run(cli, &home, &["add", name, "--path", &format!("../{name}"), "--offline", "--project", manifest_text]), &format!("add {name}"));
    }
    let lock = std::fs::read_to_string(manifest.with_file_name("Project.lock")).unwrap();
    assert!(project::lock_pin(&lock, "Alpha").is_some() && project::lock_pin(&lock, "Beta").is_some(), "both path dependencies are locked:\n{lock}");
    assert_success(&run(cli, &home, &["run", "--plain", "--offline", "--project", manifest_text]), "run with two path dependencies");
    std::fs::write(home.work().join("App/Src/Main.bd"), "i32 Main() { return Alpha.Value() - 1; }\n").unwrap();
    assert_success(&run(cli, &home, &["remove", "Beta", "--offline", "--project", manifest_text]), "remove Beta");
    let lock = std::fs::read_to_string(manifest.with_file_name("Project.lock")).unwrap();
    assert!(project::lock_pin(&lock, "Beta").is_none() && project::lock_pin(&lock, "Alpha").is_some(), "remove drops only the removed intent:\n{lock}");
    assert!(!std::fs::read_to_string(&manifest).unwrap().contains("\"Beta\""), "remove deletes the direct intent");
    assert_success(&run(cli, &home, &["build", "--plain", "--offline", "--project", manifest_text]), "build after remove");
}
