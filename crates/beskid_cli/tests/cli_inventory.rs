use std::process::Command;

fn command_names(help: &str) -> impl Iterator<Item = &str> {
    help.lines().filter_map(|line| {
        let name = line.split_whitespace().next()?;
        (name.chars().all(|character| character.is_ascii_alphanumeric() || character == '-')).then_some(name)
    })
}

#[test]
fn hi_is_not_discoverable_or_dispatched_and_graph_remains_available() {
    let help = Command::new(env!("CARGO_BIN_EXE_beskid_cli")).arg("--help").output().expect("run top-level help");
    assert!(help.status.success(), "top-level help failed: {}", String::from_utf8_lossy(&help.stderr));
    let help_text = String::from_utf8_lossy(&help.stdout);
    let names: Vec<_> = command_names(&help_text).collect();
    assert!(!names.contains(&"hi"), "removed hi command remains in top-level help:\n{help_text}");
    assert!(names.contains(&"graph"), "graph command disappeared from top-level help:\n{help_text}");

    let invocation =
        Command::new(env!("CARGO_BIN_EXE_beskid_cli")).arg("hi").output().expect("invoke removed hi command");
    assert!(!invocation.status.success(), "removed hi command unexpectedly succeeded");
    let diagnostic =
        format!("{}{}", String::from_utf8_lossy(&invocation.stdout), String::from_utf8_lossy(&invocation.stderr));
    assert!(
        diagnostic.contains("unrecognized subcommand") || diagnostic.contains("unknown subcommand"),
        "hi should fail as an unknown subcommand, got:\n{diagnostic}"
    );
    assert!(
        !diagnostic.contains("\u{1b}[?1049h") && !diagnostic.contains("\u{1b}[?1049l"),
        "unknown hi invocation entered the alternate screen: {diagnostic:?}"
    );
}
