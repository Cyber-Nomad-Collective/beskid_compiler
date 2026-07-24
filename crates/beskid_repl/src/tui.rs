//! Ratatui REPL with [`tui_term`] output pane.

use std::io::{self, stdout};

use crossterm::ExecutableCommand;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use tui_term::vt100::Parser;
use tui_term::widget::PseudoTerminal;

use crate::eval::EvalOutcome;
use crate::session::ReplSession;

const PROMPT: &str = "beskid> ";

pub fn run_tui(session: &mut ReplSession) -> io::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut parser = Parser::new(24, 80, 0);
    let mut input = String::new();
    write_banner(&mut parser);

    loop {
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(3), Constraint::Length(3)])
                .split(frame.area());

            let pseudo = PseudoTerminal::new(parser.screen())
                .block(Block::default().title(" Beskid REPL ").borders(Borders::ALL))
                .style(Style::default().fg(Color::White).bg(Color::Black).add_modifier(Modifier::BOLD));
            frame.render_widget(pseudo, chunks[0]);

            let input_line = Paragraph::new(format!("{PROMPT}{input}"))
                .block(Block::default().title(" Input ").borders(Borders::ALL));
            frame.render_widget(input_line, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Enter => {
                        let line = input.trim().to_string();
                        input.clear();
                        if line.is_empty() {
                            continue;
                        }
                        writeln_to_parser(&mut parser, &format!("{PROMPT}{line}"));
                        handle_line(session, &mut parser, &line);
                    }
                    KeyCode::Char(ch) => input.push(ch),
                    KeyCode::Backspace => {
                        input.pop();
                    }
                    _ => {}
                },
                Event::Resize(w, h) => {
                    parser.screen_mut().set_size(h, w);
                }
                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}

fn write_banner(parser: &mut Parser) {
    let banner = "Beskid REPL — :quit :reset :type <expr> (Ctrl+C to exit)\r\n";
    parser.process(banner.as_bytes());
}

fn writeln_to_parser(parser: &mut Parser, line: &str) {
    parser.process(line.as_bytes());
    parser.process(b"\r\n");
}

fn write_output(parser: &mut Parser, text: &str) {
    for line in text.lines() {
        parser.process(line.as_bytes());
        parser.process(b"\r\n");
    }
}

fn handle_line(session: &mut ReplSession, parser: &mut Parser, line: &str) {
    if let Some(command) = line.strip_prefix(':') {
        match handle_command(session, command.trim()) {
            CommandOutcome::Quit => {}
            CommandOutcome::Print(message) => write_output(parser, &message),
            CommandOutcome::PrintError(message) => write_output(parser, &format!("error: {message}")),
        }
        return;
    }

    match session.eval(line) {
        EvalOutcome::Value(value) => write_output(parser, &value),
        EvalOutcome::Unit => write_output(parser, "ok"),
        EvalOutcome::Type(_) => {
            write_output(parser, "error: :type is a command, not an expression");
        }
        EvalOutcome::Error(error) => write_output(parser, &format!("error: {error}")),
    }
}

enum CommandOutcome {
    Quit,
    Print(String),
    PrintError(String),
}

fn handle_command(session: &mut ReplSession, command: &str) -> CommandOutcome {
    let (name, rest) =
        command.split_once(char::is_whitespace).map(|(name, rest)| (name, rest.trim())).unwrap_or((command, ""));

    match name {
        "quit" | "q" | "exit" => CommandOutcome::Quit,
        "reset" => {
            session.reset();
            CommandOutcome::Print("session reset".into())
        }
        "type" | "t" => {
            if rest.is_empty() {
                return CommandOutcome::PrintError("usage: :type <snippet>".into());
            }
            match session.type_of(rest) {
                EvalOutcome::Type(display) => CommandOutcome::Print(display),
                EvalOutcome::Error(error) => CommandOutcome::PrintError(error),
                other => CommandOutcome::PrintError(format!("unexpected outcome: {other:?}")),
            }
        }
        "help" | "h" | "?" => CommandOutcome::Print("commands: :quit, :reset, :type <snippet>".into()),
        other => CommandOutcome::PrintError(format!("unknown command `{other}`")),
    }
}
