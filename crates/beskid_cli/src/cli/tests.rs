use clap::{CommandFactory, Parser};
use std::path::Path;

use super::app::{Cli, Commands};
use super::dev::{
    DevArgs, DevBuildArgs, DevBuildCommand, DevCommand, DevProjectArgs, DevProjectCommand, DevSyntaxArgs,
    DevSyntaxCommand,
};

#[test]
fn parses_developer_syntax_analyze() {
    let cli = Cli::try_parse_from(["beskid", "dev", "syntax", "analyze", "Main.bd"]).expect("parse developer syntax");
    assert!(matches!(
        cli.command,
        Commands::Dev(DevArgs {
            command: DevCommand::Syntax(DevSyntaxArgs { command: DevSyntaxCommand::Analyze(_) })
        })
    ));
}

#[test]
fn parses_developer_build_compile() {
    let cli = Cli::try_parse_from(["beskid", "dev", "build", "compile", "Main.bd"]).expect("parse developer build");
    assert!(matches!(
        cli.command,
        Commands::Dev(DevArgs { command: DevCommand::Build(DevBuildArgs { command: DevBuildCommand::Compile(_) }) })
    ));
}

#[test]
fn parses_developer_project_fetch() {
    let cli = Cli::try_parse_from(["beskid", "dev", "project", "fetch", "--project", "Project.proj"])
        .expect("parse developer project");
    assert!(matches!(
        cli.command,
        Commands::Dev(DevArgs {
            command: DevCommand::Project(DevProjectArgs { command: DevProjectCommand::Fetch(_) })
        })
    ));
}

#[test]
fn parses_developer_package_registry() {
    let cli = Cli::try_parse_from(["beskid", "dev", "package", "registry", "search", "beskid"])
        .expect("parse developer package registry");
    assert!(matches!(cli.command, Commands::Dev(DevArgs { command: DevCommand::Package(_) })));
}

#[test]
fn developer_help_lists_documented_groups() {
    let mut root = Cli::command();
    let root_help = root.render_long_help().to_string();
    assert!(root_help.contains("dev"), "root help must list the developer command");

    let mut dev = Cli::command().find_subcommand_mut("dev").expect("developer command must exist").clone();
    let dev_help = dev.render_long_help().to_string();
    for group in ["syntax", "build", "project", "package"] {
        assert!(dev_help.contains(group), "developer help must list the {group} group");
    }
}

#[test]
fn parses_up_list() {
    let parsed = Cli::try_parse_from(["beskid", "up", "list"]).unwrap();
    assert!(matches!(parsed.command, Commands::Up(_)));
}

#[test]
fn parses_mod_rebuild_with_clean_project_and_target() {
    let cli = Cli::try_parse_from([
        "beskid",
        "mod",
        "rebuild",
        "--clean",
        "--target-triple",
        "aarch64-apple-darwin",
        "mods/MyMod",
    ])
    .expect("parse cli");

    let Commands::Mod(args) = cli.command else {
        panic!("expected mod command");
    };
    let crate::commands::compiler_mod::ModCommand::Rebuild(args) = args.command else {
        panic!("expected rebuild command");
    };
    assert!(args.clean);
    assert_eq!(args.target_triple.as_deref(), Some("aarch64-apple-darwin"));
    assert_eq!(args.project.as_deref(), Some(Path::new("mods/MyMod")));
}

#[test]
fn parses_new_list_with_online() {
    let cli = Cli::try_parse_from(["beskid", "new", "list", "--online"]).expect("parse");
    let Commands::New(args) = cli.command else {
        panic!("expected new command");
    };
    let Some(crate::commands::new::NewCommand::List(list)) = args.command else {
        panic!("expected list subcommand");
    };
    assert!(list.online);
}

#[test]
fn parses_new_console_instantiate() {
    let cli = Cli::try_parse_from(["beskid", "new", "console", "-n", "MyApp", "-o", "./MyApp", "--no-interactive"])
        .expect("parse");
    let Commands::New(args) = cli.command else {
        panic!("expected new");
    };
    assert_eq!(args.short_name.as_deref(), Some("console"));
    assert_eq!(args.instantiate.name.as_deref(), Some("MyApp"));
}

#[test]
fn parses_lsp_install_with_release_tag() {
    let cli = Cli::try_parse_from(["beskid", "lsp", "install", "--release-tag", "lsp-v0.1.5"]).expect("parse cli");
    let Commands::Lsp(args) = cli.command else {
        panic!("expected lsp command");
    };
    let Some(crate::commands::lsp::LspCommand::Install(install)) = args.command else {
        panic!("expected lsp install");
    };
    assert_eq!(install.release_tag, "lsp-v0.1.5");
}

#[test]
fn parses_mod_clean_with_project() {
    let cli = Cli::try_parse_from(["beskid", "mod", "clean", "mods/MyMod"]).expect("parse cli");

    let Commands::Mod(args) = cli.command else {
        panic!("expected mod command");
    };
    let crate::commands::compiler_mod::ModCommand::Clean(args) = args.command else {
        panic!("expected clean command");
    };
    assert_eq!(args.project.as_deref(), Some(Path::new("mods/MyMod")));
}

#[test]
fn parses_runtime_kit_build_contract() {
    let cli = Cli::try_parse_from([
        "beskid",
        "runtime-kit",
        "build",
        "--prefix",
        "/opt/beskid",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--profile",
        "release",
        "--static-library",
        "out/libbeskid_runtime.a",
        "--shared-library",
        "out/libbeskid_runtime.so",
    ])
    .expect("parse runtime-kit build");

    let Commands::RuntimeKit(args) = cli.command else {
        panic!("expected runtime-kit command");
    };
    let crate::commands::runtime_kit::RuntimeKitCommand::Build(args) = args.command else {
        panic!("expected runtime-kit build command");
    };
    assert_eq!(args.prefix, Path::new("/opt/beskid"));
    assert_eq!(args.target, "x86_64-unknown-linux-gnu");
    assert_eq!(args.profile.as_str(), "release");
    assert_eq!(args.static_library, Path::new("out/libbeskid_runtime.a"));
    assert_eq!(args.shared_library, Path::new("out/libbeskid_runtime.so"));
    assert!(args.shared_import_library.is_none());
}

#[test]
fn parses_runtime_kit_native_host_contract() {
    let cli = Cli::try_parse_from([
        "beskid",
        "runtime-kit",
        "build-native-host",
        "--prefix",
        "/tmp/beskid-native-runtime",
        "--profile",
        "debug",
    ])
    .expect("parse native host runtime-kit build");

    let Commands::RuntimeKit(args) = cli.command else {
        panic!("expected runtime-kit command");
    };
    let crate::commands::runtime_kit::RuntimeKitCommand::BuildNativeHost(args) = args.command else {
        panic!("expected native host runtime-kit build");
    };
    assert_eq!(args.prefix, Path::new("/tmp/beskid-native-runtime"));
    assert_eq!(args.profile.as_str(), "debug");
}

#[test]
fn runtime_kit_build_requires_both_primary_artifacts() {
    let error = match Cli::try_parse_from([
        "beskid",
        "runtime-kit",
        "build",
        "--prefix",
        "/opt/beskid",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--profile",
        "debug",
        "--static-library",
        "out/libbeskid_runtime.a",
    ]) {
        Ok(_) => panic!("missing shared library must fail parsing"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("--shared-library"));
}

#[test]
fn parses_runtime_kit_matrix_contract() {
    let cli = Cli::try_parse_from([
        "beskid",
        "runtime-kit",
        "build-matrix",
        "--prefix",
        "/opt/beskid",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--debug-static-library",
        "out/debug/libbeskid_runtime.a",
        "--debug-shared-library",
        "out/debug/libbeskid_runtime.so",
        "--release-static-library",
        "out/release/libbeskid_runtime.a",
        "--release-shared-library",
        "out/release/libbeskid_runtime.so",
        "--debug-static-provenance-symbol-list",
        "out/debug/static.symbols",
        "--debug-shared-provenance-symbol-list",
        "out/debug/shared.symbols",
        "--release-static-provenance-symbol-list",
        "out/release/static.symbols",
        "--release-shared-provenance-symbol-list",
        "out/release/shared.symbols",
    ])
    .expect("parse runtime-kit matrix");
    let Commands::RuntimeKit(args) = cli.command else {
        panic!("expected runtime-kit command");
    };
    let crate::commands::runtime_kit::RuntimeKitCommand::BuildMatrix(args) = args.command else {
        panic!("expected runtime-kit matrix command");
    };
    assert_eq!(args.target, "x86_64-unknown-linux-gnu");
    assert_eq!(args.debug_static_library, Path::new("out/debug/libbeskid_runtime.a"));
    assert_eq!(args.release_shared_library, Path::new("out/release/libbeskid_runtime.so"));
    assert_eq!(args.debug_static_provenance_symbol_list, Path::new("out/debug/static.symbols"));
    assert_eq!(args.debug_shared_provenance_symbol_list, Path::new("out/debug/shared.symbols"));
    assert_eq!(args.release_static_provenance_symbol_list, Path::new("out/release/static.symbols"));
    assert_eq!(args.release_shared_provenance_symbol_list, Path::new("out/release/shared.symbols"));
}

#[test]
fn test_target_timeout_defaults_to_none() {
    // The default only holds when `BESKID_TARGET_TIMEOUT_SECS` is unset. Hold the env lock so the
    // env-var tests below cannot set it mid-parse, and clear any value inherited from the caller's
    // environment (restored afterwards) so the test does not depend on the shell it runs in.
    let _guard = TARGET_TIMEOUT_ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let inherited = std::env::var_os("BESKID_TARGET_TIMEOUT_SECS");
    // SAFETY: serialized by TARGET_TIMEOUT_ENV_LOCK; no other test reads/writes this key.
    unsafe { std::env::remove_var("BESKID_TARGET_TIMEOUT_SECS") };
    let result = Cli::try_parse_from(["beskid", "test", "Main.bd"]);
    if let Some(value) = inherited {
        unsafe { std::env::set_var("BESKID_TARGET_TIMEOUT_SECS", value) };
    }
    let cli = result.expect("parse cli");
    let Commands::Test(args) = cli.command else {
        panic!("expected test command");
    };
    assert_eq!(args.target_timeout, None);
    assert_eq!(args.execution_budgets().target, std::time::Duration::from_secs(120));
}

#[test]
fn test_target_timeout_flag_sets_budget() {
    let cli = Cli::try_parse_from(["beskid", "test", "--target-timeout", "5", "Main.bd"]).expect("parse cli");
    let Commands::Test(args) = cli.command else {
        panic!("expected test command");
    };
    assert_eq!(args.target_timeout, Some(5));
    assert_eq!(args.execution_budgets().target, std::time::Duration::from_secs(5));
}

// Guards the two env-var tests below, which mutate a process-wide env var, against
// cargo's default multi-threaded test runner racing on the same key.
static TARGET_TIMEOUT_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_target_timeout_env_var_sets_budget() {
    let _guard = TARGET_TIMEOUT_ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // SAFETY: serialized by TARGET_TIMEOUT_ENV_LOCK; no other test reads/writes this key.
    unsafe { std::env::set_var("BESKID_TARGET_TIMEOUT_SECS", "42") };
    let result = Cli::try_parse_from(["beskid", "test", "Main.bd"]);
    unsafe { std::env::remove_var("BESKID_TARGET_TIMEOUT_SECS") };
    let cli = result.expect("parse cli");
    let Commands::Test(args) = cli.command else {
        panic!("expected test command");
    };
    assert_eq!(args.target_timeout, Some(42));
}

#[test]
fn test_target_timeout_flag_wins_over_env_var() {
    let _guard = TARGET_TIMEOUT_ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // SAFETY: serialized by TARGET_TIMEOUT_ENV_LOCK; no other test reads/writes this key.
    unsafe { std::env::set_var("BESKID_TARGET_TIMEOUT_SECS", "42") };
    let result = Cli::try_parse_from(["beskid", "test", "--target-timeout", "7", "Main.bd"]);
    unsafe { std::env::remove_var("BESKID_TARGET_TIMEOUT_SECS") };
    let cli = result.expect("parse cli");
    let Commands::Test(args) = cli.command else {
        panic!("expected test command");
    };
    assert_eq!(args.target_timeout, Some(7));
}
