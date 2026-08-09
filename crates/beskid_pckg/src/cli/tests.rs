use super::{PackArgs, PackArgsPackageKind, PackProfileOverride, PckgCommand};
use clap::Parser;

#[derive(clap::Parser, Debug)]
#[command(name = "test-cli")]
struct TestRoot {
    #[command(subcommand)]
    cmd: PckgCommand,
}

fn parse(args: &[&str]) -> PackArgs {
    let parsed = TestRoot::try_parse_from(args).expect("clap parse");
    match parsed.cmd {
        PckgCommand::Pack(pack) => pack,
        other => panic!("expected pack, got {other:?}"),
    }
}

#[test]
fn pack_args_default_package_kind_is_auto() {
    let args = parse(&["test-cli", "pack", "--package", "demo", "--output", "/tmp/demo.bpk"]);
    assert!(matches!(args.package_kind, PackArgsPackageKind::Auto));
    assert!(matches!(args.package_kind_override(), PackProfileOverride::Auto));
}

#[test]
fn pack_args_package_kind_tool_flag_parses() {
    let args = parse(&["test-cli", "pack", "--package", "demo", "--output", "/tmp/demo.bpk", "--package-kind", "tool"]);
    assert!(matches!(args.package_kind, PackArgsPackageKind::Tool));
    assert!(matches!(args.package_kind_override(), PackProfileOverride::Tool));
}

#[test]
fn pack_args_package_kind_rejects_unknown_value() {
    let result = TestRoot::try_parse_from([
        "test-cli",
        "pack",
        "--package",
        "demo",
        "--output",
        "/tmp/demo.bpk",
        "--package-kind",
        "ghost",
    ]);
    assert!(result.is_err());
}
