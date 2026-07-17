use beskid_up::{UpArgs, UpCommand};
use clap::Parser;

#[derive(Parser)]
struct TestCli {
    #[command(flatten)]
    up: UpArgs,
}

#[test]
fn parses_a_version_selection_command() {
    let parsed = TestCli::try_parse_from(["beskid", "use", "1.2.3"]).unwrap();

    assert!(matches!(parsed.up.command, UpCommand::Use { .. }));
}

#[test]
fn rejects_latest_as_a_mutable_version_selection() {
    assert!(TestCli::try_parse_from(["beskid", "use", "latest"]).is_err());
}
