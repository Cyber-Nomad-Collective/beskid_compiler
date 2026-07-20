use std::io::{self, BufRead, IsTerminal, Write};

use crate::eval::EvalOutcome;
use crate::session::ReplSession;

/// Input source for the REPL (stdin in production, buffers in tests).
pub trait ReplInput {
    fn read_line(&mut self, prompt: &str) -> io::Result<Option<String>>;
    fn is_tty(&self) -> bool;
}

/// Buffered stdin reader with an optional prompt when attached to a TTY.
pub struct StdioInput {
    reader: io::StdinLock<'static>,
    tty: bool,
}

impl StdioInput {
    pub fn new() -> Self {
        Self {
            reader: io::stdin().lock(),
            tty: io::stdin().is_terminal(),
        }
    }
}

impl Default for StdioInput {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplInput for StdioInput {
    fn read_line(&mut self, prompt: &str) -> io::Result<Option<String>> {
        if self.tty {
            print!("{prompt}");
            io::stdout().flush()?;
        }
        let mut line = String::new();
        let bytes = self.reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        Ok(Some(line))
    }

    fn is_tty(&self) -> bool {
        self.tty
    }
}

/// In-memory line iterator for unit tests.
pub struct BufferInput<'a> {
    lines: std::slice::Iter<'a, String>,
}

impl<'a> BufferInput<'a> {
    pub fn new(lines: &'a [String]) -> Self {
        Self {
            lines: lines.iter(),
        }
    }
}

impl ReplInput for BufferInput<'_> {
    fn read_line(&mut self, _prompt: &str) -> io::Result<Option<String>> {
        Ok(self.lines.next().cloned())
    }

    fn is_tty(&self) -> bool {
        false
    }
}

pub fn run_loop(session: &mut ReplSession, input: &mut dyn ReplInput) -> io::Result<()> {
    if input.is_tty() {
        writeln!(
            io::stdout(),
            "Beskid REPL (v1 snippets). Commands: :quit, :reset, :type <expr>"
        )?;
    }

    while let Some(line) = input.read_line("beskid> ")? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match handle_line(session, trimmed) {
            LineAction::Quit => break,
            LineAction::Print(message) => {
                writeln!(io::stdout(), "{message}")?;
            }
            LineAction::PrintError(message) => {
                writeln!(io::stderr(), "error: {message}")?;
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
enum LineAction {
    Quit,
    Print(String),
    PrintError(String),
}

fn handle_line(session: &mut ReplSession, line: &str) -> LineAction {
    if let Some(command) = line.strip_prefix(':') {
        return handle_command(session, command.trim());
    }

    match session.eval(line) {
        EvalOutcome::Value(value) => LineAction::Print(value),
        EvalOutcome::Unit => LineAction::Print("ok".to_string()),
        EvalOutcome::Type(_) => {
            LineAction::PrintError(":type is a command, not an expression".into())
        }
        EvalOutcome::Error(error) => LineAction::PrintError(error),
    }
}

fn handle_command(session: &mut ReplSession, command: &str) -> LineAction {
    let (name, rest) = command
        .split_once(char::is_whitespace)
        .map(|(name, rest)| (name, rest.trim()))
        .unwrap_or((command, ""));

    match name {
        "quit" | "q" | "exit" => LineAction::Quit,
        "reset" => {
            session.reset();
            LineAction::Print("session reset".to_string())
        }
        "type" | "t" => {
            if rest.is_empty() {
                return LineAction::PrintError("usage: :type <snippet>".into());
            }
            match session.type_of(rest) {
                EvalOutcome::Type(display) => LineAction::Print(display),
                EvalOutcome::Error(error) => LineAction::PrintError(error),
                other => LineAction::PrintError(format!("unexpected outcome: {other:?}")),
            }
        }
        "help" | "h" | "?" => {
            LineAction::Print("commands: :quit, :reset, :type <snippet>".to_string())
        }
        other => LineAction::PrintError(format!("unknown command `{other}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalOutcome;
    use beskid_abi::runtime_kit::BuildProfile;
    use beskid_engine::{Engine, host_runtime_target};
    use beskid_tools::toolchain::runtime_kit::{RuntimeKitProfile, build_native_host};

    fn shared_exact_kit_prefix() -> &'static std::path::Path {
        use std::sync::OnceLock;
        static PREFIX: OnceLock<std::path::PathBuf> = OnceLock::new();
        PREFIX.get_or_init(|| {
            let prefix = tempfile::tempdir().expect("exact kit prefix").keep();
            build_native_host(prefix.clone(), RuntimeKitProfile::Debug)
                .expect("publish exact native kit");
            prefix
        })
    }

    fn exact_kit_session() -> ReplSession {
        let target = host_runtime_target().expect("host target");
        let engine = Engine::with_runtime_kit(shared_exact_kit_prefix(), target, BuildProfile::Debug)
            .expect("load exact kit");
        ReplSession::from_engine(engine)
    }

    #[test]
    fn quit_command_stops_loop() {
        let mut session = exact_kit_session();
        let lines = vec![":quit".to_string()];
        let mut input = BufferInput::new(&lines);
        run_loop(&mut session, &mut input).expect("loop");
    }

    #[test]
    fn eval_from_buffer_input() {
        let mut session = exact_kit_session();
        let lines = vec!["2 + 3".to_string(), ":quit".to_string()];
        let mut input = BufferInput::new(&lines);
        run_loop(&mut session, &mut input).expect("loop");
    }

    #[test]
    fn reset_command_clears_session() {
        let mut session = exact_kit_session();
        assert_eq!(session.eval("1 + 1"), EvalOutcome::Value("2".to_string()));
        assert!(matches!(
            handle_line(&mut session, ":reset"),
            LineAction::Print(_)
        ));
        assert_eq!(session.eval("3 + 4"), EvalOutcome::Value("7".to_string()));
    }

    #[test]
    fn type_command_prints_inferred_type() {
        let mut session = exact_kit_session();
        match handle_line(&mut session, ":type 1 + 1") {
            LineAction::Print(value) => assert_eq!(value, "i64"),
            other => panic!("expected print, got {other:?}"),
        }
    }
}
