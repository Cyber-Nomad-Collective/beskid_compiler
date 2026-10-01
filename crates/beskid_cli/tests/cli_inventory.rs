use std::process::Command;
use std::{fs, path::Path};

#[test]
fn new_rejects_removed_tui_picker_and_graph_keeps_its_tui_flag() {
    let binary = env!("CARGO_BIN_EXE_beskid_cli");
    let help = Command::new(binary).args(["new", "--help"]).output().expect("new help");
    assert!(help.status.success());
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(!help.contains("--tui"), "removed picker advertised: {help}");

    let rejected = Command::new(binary).args(["new", "--tui"]).output().expect("removed picker");
    assert!(!rejected.status.success());
    let diagnostic = String::from_utf8_lossy(&rejected.stderr);
    assert!(diagnostic.contains("unexpected argument '--tui'"), "{diagnostic}");
    assert!(!rejected.stdout.contains(&27) && !rejected.stderr.contains(&27));

    let graph = Command::new(binary).args(["graph", "--help"]).output().expect("graph help");
    assert!(graph.status.success());
    assert!(String::from_utf8_lossy(&graph.stdout).contains("--tui"));
}

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

#[test]
fn hi_shell_launcher_and_app_are_absent_from_production_tools() {
    fn rust_sources(root: &Path, sources: &mut Vec<(String, String)>) {
        for entry in fs::read_dir(root).expect("read tools source directory") {
            let path = entry.expect("read tools source entry").path();
            if path.is_dir() {
                rust_sources(&path, sources);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let source = fs::read_to_string(&path).expect("read Rust source");
                sources.push((path.display().to_string(), source));
            }
        }
    }

    let tools_src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../beskid_tools/src");
    let mut sources = Vec::new();
    rust_sources(&tools_src, &mut sources);

    for (path, source) in sources {
        assert!(!source.contains("run_hi_blocking"), "Hi blocking launcher remains in {path}");
        assert!(!source.contains("HiShellApp"), "Hi shell app remains in {path}");
        assert!(!source.contains("pub fn run_hi("), "Hi realm runner remains in {path}");
    }
}
