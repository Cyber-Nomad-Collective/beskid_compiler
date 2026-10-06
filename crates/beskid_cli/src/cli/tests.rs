use super::app::{Cli, Commands};
use clap::{CommandFactory, Parser};
use std::path::{Path, PathBuf};

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

#[test]
fn test_matrix_timeout_defaults_to_thirty_minutes() {
    let _guard = MATRIX_TIMEOUT_ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let inherited = std::env::var_os("BESKID_MATRIX_TIMEOUT_SECS");
    // SAFETY: serialized by MATRIX_TIMEOUT_ENV_LOCK; no other test mutates this key.
    unsafe { std::env::remove_var("BESKID_MATRIX_TIMEOUT_SECS") };
    let parsed = Cli::try_parse_from(["beskid", "test", "--all-targets", "Main.bd"]);
    if let Some(value) = inherited {
        unsafe { std::env::set_var("BESKID_MATRIX_TIMEOUT_SECS", value) };
    }
    let cli = parsed.expect("parse cli");
    let Commands::Test(args) = cli.command else {
        panic!("expected test command");
    };
    assert_eq!(args.execution_budgets().matrix, std::time::Duration::from_secs(1800));
}

#[test]
fn test_matrix_timeout_flag_sets_whole_matrix_budget() {
    let cli = Cli::try_parse_from(["beskid", "test", "--all-targets", "--matrix-timeout", "3600", "Main.bd"])
        .expect("parse cli");
    let Commands::Test(args) = cli.command else {
        panic!("expected test command");
    };
    assert_eq!(args.execution_budgets().matrix, std::time::Duration::from_secs(3600));
}

#[test]
fn test_help_documents_matrix_timeout_default_and_env() {
    let mut test = Cli::command().find_subcommand_mut("test").expect("test command").clone();
    let help = test.render_long_help().to_string();
    assert!(help.contains("--matrix-timeout"));
    assert!(help.contains("default: 1800"));
    assert!(help.contains("BESKID_MATRIX_TIMEOUT_SECS"));
}

static MATRIX_TIMEOUT_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_matrix_timeout_env_var_sets_budget_and_flag_wins() {
    let _guard = MATRIX_TIMEOUT_ENV_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let inherited = std::env::var_os("BESKID_MATRIX_TIMEOUT_SECS");
    // SAFETY: serialized by MATRIX_TIMEOUT_ENV_LOCK; no other test mutates this key.
    unsafe { std::env::set_var("BESKID_MATRIX_TIMEOUT_SECS", "2400") };
    let from_env = Cli::try_parse_from(["beskid", "test", "--all-targets", "Main.bd"]);
    let from_flag = Cli::try_parse_from(["beskid", "test", "--all-targets", "--matrix-timeout", "3600", "Main.bd"]);
    if let Some(value) = inherited {
        unsafe { std::env::set_var("BESKID_MATRIX_TIMEOUT_SECS", value) };
    } else {
        unsafe { std::env::remove_var("BESKID_MATRIX_TIMEOUT_SECS") };
    }
    let Commands::Test(env_args) = from_env.expect("parse env timeout").command else {
        panic!("expected test command");
    };
    let Commands::Test(flag_args) = from_flag.expect("parse flag timeout").command else {
        panic!("expected test command");
    };
    assert_eq!(env_args.execution_budgets().matrix, std::time::Duration::from_secs(2400));
    assert_eq!(flag_args.execution_budgets().matrix, std::time::Duration::from_secs(3600));
}

#[test]
fn v06_root_is_task_oriented_without_legacy_dispatch() {
    let root = Cli::command();
    let mut names =
        root.get_subcommands().map(|command| command.get_name()).filter(|name| *name != "help").collect::<Vec<_>>();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "add",
            "build",
            "check",
            "dev",
            "doc",
            "doctor",
            "fmt",
            "new",
            "package",
            "remove",
            "run",
            "test",
            "toolchain",
            "update"
        ]
    );
    for old in [
        "analyze",
        "format",
        "parse",
        "tree",
        "clif",
        "pckg",
        "up",
        "fetch",
        "lock",
        "graph",
        "lsp",
        "corelib",
        "runtime-kit",
        "repl",
        "hi",
    ] {
        assert!(Cli::try_parse_from(["beskid", old]).is_err(), "retired root {old} must not dispatch");
    }
}

#[test]
fn v06_default_new_accepts_name_and_explicit_template() {
    assert!(Cli::try_parse_from(["beskid", "new", "hello"]).is_ok());
    assert!(Cli::try_parse_from(["beskid", "new", "hello", "--template", "lib"]).is_ok());
    assert!(Cli::try_parse_from(["beskid", "new", "--list"]).is_ok());
}

#[test]
fn v06_canonical_dev_routes_remove_ordinary_duplicates() {
    for args in [
        vec!["beskid", "dev", "syntax", "parse", "Main.bd"],
        vec!["beskid", "dev", "lsp"],
        vec!["beskid", "dev", "repl"],
        vec!["beskid", "dev", "project", "graph", "--kind", "project"],
    ] {
        assert!(Cli::try_parse_from(&args).is_ok(), "canonical route {args:?}");
    }
    for args in [
        vec!["beskid", "dev", "syntax", "analyze"],
        vec!["beskid", "dev", "syntax", "format"],
        vec!["beskid", "dev", "syntax", "doc"],
        vec!["beskid", "dev", "build", "test", "--all-targets"],
        vec!["beskid", "dev", "project", "update"],
    ] {
        assert!(Cli::try_parse_from(&args).is_err(), "duplicate route {args:?}");
    }
}

#[test]
fn v06_root_help_teaches_dependency_add() {
    let help = Cli::command().render_long_help().to_string();
    assert!(help.contains("beskid new hello"));
    assert!(help.contains("beskid add"));
    assert!(help.contains("beskid run"));
}

#[test]
fn v06_everyday_help_describes_user_actions() {
    let mut command = Cli::command();
    for (name, action) in [
        ("build", "Compile"),
        ("run", "Build"),
        ("test", "Build"),
        ("doc", "Generate"),
        ("remove", "Remove"),
        ("update", "Update"),
        ("package", "Find"),
        ("toolchain", "Inspect"),
    ] {
        let subcommand = command.find_subcommand_mut(name).expect("canonical command");
        let about = subcommand.get_about().expect("everyday commands need an action description").to_string();
        assert!(about.starts_with(action), "{name} must describe its user action: {about}");
        let help = subcommand.render_long_help().to_string();
        for internal in ["Full set of flags", "Independent lock and network policy", "resolution commands"] {
            assert!(
                !about.contains(internal) && !help.contains(internal),
                "{name} exposes implementation documentation: {help}"
            );
        }
    }
}

#[test]
fn v06_package_and_toolchain_have_canonical_grammar() {
    for args in [
        vec!["beskid", "package", "search", "math"],
        vec!["beskid", "package", "info", "math"],
        vec!["beskid", "package", "template", "list"],
        vec!["beskid", "toolchain", "status"],
        vec!["beskid", "toolchain", "update"],
        vec!["beskid", "doctor"],
    ] {
        assert!(Cli::try_parse_from(&args).is_ok(), "canonical route {args:?}");
    }
}

#[test]
fn v06_mod_runtime_and_lsp_contracts_survive_route_migration() {
    use super::dev::{DevArgs, DevCommand};
    let cli = Cli::try_parse_from([
        "beskid",
        "dev",
        "mod",
        "rebuild",
        "--clean",
        "--target-triple",
        "aarch64-apple-darwin",
        "mods/MyMod",
    ])
    .unwrap();
    let Commands::Dev(DevArgs { command: DevCommand::Mod(args), .. }) = cli.command else {
        panic!("expected dev mod");
    };
    let crate::commands::compiler_mod::ModCommand::Rebuild(args) = args.command else {
        panic!("expected rebuild");
    };
    assert!(args.clean);
    assert_eq!(args.target_triple.as_deref(), Some("aarch64-apple-darwin"));
    assert_eq!(args.project.as_deref(), Some(std::path::Path::new("mods/MyMod")));
    let cli = Cli::try_parse_from(["beskid", "dev", "lsp", "install", "--release-tag", "lsp-v0.6.0"]).unwrap();
    let Commands::Dev(DevArgs { command: DevCommand::Lsp(args), .. }) = cli.command else {
        panic!("expected dev lsp");
    };
    let Some(crate::commands::lsp::LspCommand::Install(args)) = args.command else {
        panic!("expected install");
    };
    assert_eq!(args.release_tag, "lsp-v0.6.0");
    let cli = Cli::try_parse_from([
        "beskid",
        "dev",
        "runtime-kit",
        "build",
        "--prefix",
        "/opt/beskid",
        "--target",
        "x86_64-unknown-linux-gnu",
        "--profile",
        "release",
        "--static-library",
        "out/runtime.a",
        "--shared-library",
        "out/runtime.so",
    ])
    .unwrap();
    let Commands::Dev(DevArgs { command: DevCommand::RuntimeKit(args), .. }) = cli.command else {
        panic!("expected runtime kit");
    };
    let crate::commands::runtime_kit::RuntimeKitCommand::Build(args) = args.command else {
        panic!("expected build");
    };
    assert_eq!(args.prefix, std::path::Path::new("/opt/beskid"));
    assert_eq!(args.target, "x86_64-unknown-linux-gnu");
    assert_eq!(args.profile.as_str(), "release");
    assert!(
        Cli::try_parse_from([
            "beskid",
            "dev",
            "runtime-kit",
            "build",
            "--prefix",
            "/opt/beskid",
            "--target",
            "x86_64-unknown-linux-gnu",
            "--profile",
            "release",
            "--static-library",
            "out/runtime.a"
        ])
        .is_err()
    );
}

#[test]
fn v06_read_only_routes_do_not_provision_corelib() {
    for args in [
        vec!["beskid", "new", "--list"],
        vec!["beskid", "package", "search", "math"],
        vec!["beskid", "doctor"],
        vec!["beskid", "toolchain", "status"],
        vec!["beskid", "dev", "capabilities", "--json"],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        assert!(!super::app::NeedsCorelib(&cli.command));
    }
    let cli = Cli::try_parse_from(["beskid", "check", "Main.bd"]).unwrap();
    assert!(super::app::NeedsCorelib(&cli.command));
}

#[test]
fn v06_retired_route_diagnostic_names_replacement() {
    for (old, new) in [("analyze", "check"), ("pckg", "package"), ("lsp", "dev lsp"), ("fetch", "dev project fetch")] {
        let error = match super::app::ParseInvocation(["beskid", old]) {
            Ok(_) => panic!("retired command parsed"),
            Err(error) => error,
        };
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains(&format!("beskid {new}")));
    }
}

#[test]
fn v06_fmt_defaults_to_current_directory() {
    assert!(Cli::try_parse_from(["beskid", "fmt", "--check"]).is_ok());
}

#[test]
fn v06_fmt_rejects_obsolete_write_flags() {
    for flag in ["--write", "-w"] {
        let error = match Cli::try_parse_from(["beskid", "fmt", flag, "Main.bd"]) {
            Ok(_) => panic!("obsolete formatter flag {flag} must not parse"),
            Err(error) => error,
        };
        assert_eq!(error.exit_code(), 2);
        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }
    assert!(Cli::try_parse_from(["beskid", "fmt", "Main.bd"]).is_ok());
    assert!(Cli::try_parse_from(["beskid", "fmt", "--check", "Main.bd"]).is_ok());
    assert!(Cli::try_parse_from(["beskid", "fmt", "--output", "Formatted.bd", "Main.bd"]).is_ok());
}

#[test]
fn v06_ordinary_resolution_routes_accept_independent_offline_policy() {
    for route in ["check", "build", "run", "test", "doc"] {
        for flags in [vec!["--offline"], vec!["--offline", "--locked"], vec!["--frozen"]] {
            let mut argv = vec!["beskid", route, "--project", "App.bproj"];
            argv.extend(flags);
            assert!(Cli::try_parse_from(&argv).is_ok(), "resolution policy must parse on {argv:?}");
        }
    }
    assert!(Cli::try_parse_from(["beskid", "new", "hello", "--offline"]).is_ok());
    assert!(Cli::try_parse_from(["beskid", "new", "--list", "--offline"]).is_ok());
}

#[test]
fn v06_ordinary_resolution_preserves_independent_policy_bits() {
    for route in ["check", "build", "run", "test", "doc"] {
        for (flags, locked, offline, frozen) in [
            (vec!["--offline"], false, true, false),
            (vec!["--locked"], true, false, false),
            (vec!["--offline", "--locked"], true, true, false),
            (vec!["--frozen"], false, false, true),
        ] {
            let mut argv = vec!["beskid", route];
            argv.extend(flags);
            let cli = Cli::try_parse_from(&argv).unwrap();
            let policy = match cli.command {
                Commands::Check(args) => args.lockfile.WorkspaceOptions(),
                Commands::Build(args) => args.lockfile.WorkspaceOptions(),
                Commands::Run(args) => args.lockfile.WorkspaceOptions(),
                Commands::Test(args) => args.lockfile.WorkspaceOptions(),
                Commands::Doc(args) => args.lockfile.WorkspaceOptions(),
                _ => unreachable!(),
            };
            assert_eq!((policy.locked, policy.offline, policy.frozen), (locked, offline, frozen), "{argv:?}");
            assert!(!policy.refresh_lock, "ordinary commands must retain eligible pins");
        }
    }
}

#[test]
fn v06_dependency_mutations_accept_shared_lock_and_network_flags() {
    for route in ["add", "remove", "update"] {
        for flag in ["--locked", "--offline", "--frozen"] {
            assert!(
                Cli::try_parse_from(["beskid", route, "Numbers", flag]).is_ok(),
                "{route} {flag} must be parsed as policy"
            );
        }
    }
}

#[test]
fn parses_runtime_kit_native_host_contract() {
    let cli = Cli::try_parse_from([
        "beskid",
        "dev",
        "runtime-kit",
        "build-native-host",
        "--prefix",
        "/tmp/beskid-native-runtime",
        "--profile",
        "debug",
    ])
    .expect("parse native host runtime-kit build");

    let Commands::Dev(super::dev::DevArgs { command: super::dev::DevCommand::RuntimeKit(args), .. }) = cli.command
    else {
        panic!("expected runtime-kit command");
    };
    let crate::commands::runtime_kit::RuntimeKitCommand::BuildNativeHost(args) = args.command else {
        panic!("expected native host runtime-kit build");
    };
    assert_eq!(args.prefix, Path::new("/tmp/beskid-native-runtime"));
    assert_eq!(args.profile.as_str(), "debug");
}

#[test]
fn parses_runtime_kit_matrix_contract() {
    let cli = Cli::try_parse_from([
        "beskid",
        "dev",
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
    let Commands::Dev(super::dev::DevArgs { command: super::dev::DevCommand::RuntimeKit(args), .. }) = cli.command
    else {
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
fn v06_parses_dev_import_lib_defaults_and_explicit_options() {
    for (argv, provider, dry_run, project) in [
        (vec!["beskid", "dev", "import", "lib", "libc"], "c-posix", false, None),
        (
            vec![
                "beskid",
                "dev",
                "import",
                "lib",
                "libc",
                "--provider",
                "posix",
                "--dry-run",
                "--project",
                "/tmp/example",
            ],
            "posix",
            true,
            Some(PathBuf::from("/tmp/example")),
        ),
    ] {
        let cli = Cli::try_parse_from(argv).expect("canonical import grammar");
        let Commands::Dev(super::dev::DevArgs { command: super::dev::DevCommand::Import(args), .. }) = cli.command
        else {
            panic!("expected dev import");
        };
        let crate::commands::import::ImportCommand::Lib(args) = args.command;
        assert_eq!(args.logical, "libc");
        assert_eq!(args.provider, provider);
        assert_eq!(args.dry_run, dry_run);
        assert_eq!(args.project, project);
    }
    assert!(Cli::try_parse_from(["beskid", "import", "lib", "libc"]).is_err());
}

#[test]
fn v06_entire_canonical_command_tree_has_valid_clap_constraints() {
    use clap::CommandFactory;
    Cli::command().debug_assert();
}

#[test]
fn v06_template_install_selectors_are_real_mutually_exclusive_arguments() {
    for selector in ["--path", "--git"] {
        assert!(Cli::try_parse_from(["beskid", "package", "template", "install", "local", selector, "source"]).is_ok());
    }
    assert!(
        Cli::try_parse_from([
            "beskid", "package", "template", "install", "local", "--path", "source", "--git", "remote"
        ])
        .is_err()
    );
}

#[test]
fn v06_owner_producer_is_explicit_closed_and_has_no_prefix_override() {
    assert!(
        Cli::try_parse_from([
            "beskid",
            "dev",
            "toolchain-owner",
            "--owner",
            "manual",
            "--version",
            "0.6.0",
            "--target",
            "aarch64-apple-darwin"
        ])
        .is_ok()
    );
    assert!(
        Cli::try_parse_from([
            "beskid",
            "dev",
            "toolchain-owner",
            "--owner",
            "unknown",
            "--version",
            "0.6.0",
            "--target",
            "aarch64-apple-darwin"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "beskid",
            "dev",
            "toolchain-owner",
            "--owner",
            "manual",
            "--version",
            "0.6.0",
            "--target",
            "aarch64-apple-darwin",
            "--prefix",
            "elsewhere"
        ])
        .is_err()
    );
}
