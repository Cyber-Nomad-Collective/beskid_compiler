//! Live test-run table for `beskid test` (status, duration, name).

use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use indicatif::MultiProgress;

use super::hyperlink::{FileLineLink, maybe_link_label};
use super::timer::format_duration;

const TABLE_INNER_WIDTH: usize = 68;
const STATUS_COL: usize = 10;
const TIME_COL: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestRowState {
    Pending,
    Running,
    Passed,
    Failed,
    Skipped,
    FilteredOut,
}

#[derive(Debug, Clone)]
struct TestRow {
    qualified_name: String,
    link: Option<FileLineLink>,
    state: TestRowState,
    duration: Option<Duration>,
}

/// Interactive or plain presenter for a test run.
pub struct TestRunUi {
    plain: bool,
    rows: Vec<TestRow>,
    lines_on_stderr: usize,
    multi: Option<MultiProgress>,
}

impl TestRunUi {
    pub fn new(plain: bool, use_tty: bool) -> Self {
        let interactive = use_tty && !plain && stderr().is_terminal();
        Self {
            plain: !interactive,
            rows: Vec::new(),
            lines_on_stderr: 0,
            multi: if interactive {
                Some(MultiProgress::new())
            } else {
                None
            },
        }
    }

    pub fn push_row(
        &mut self,
        qualified_name: impl Into<String>,
        state: TestRowState,
        link: Option<FileLineLink>,
    ) {
        self.rows.push(TestRow {
            qualified_name: qualified_name.into(),
            link,
            state,
            duration: None,
        });
    }

    pub fn is_plain(&self) -> bool {
        self.plain
    }

    pub fn draw_initial(&mut self) -> io::Result<()> {
        if self.plain {
            return Ok(());
        }
        self.redraw_table()
    }

    pub fn start_running(&mut self, index: usize) -> io::Result<()> {
        if index >= self.rows.len() {
            return Ok(());
        }
        if self.plain {
            eprintln!("→ {}", self.rows[index].qualified_name);
            return Ok(());
        }
        self.rows[index].state = TestRowState::Running;
        self.rows[index].duration = None;
        self.redraw_table()
    }

    pub fn finish_row(
        &mut self,
        index: usize,
        state: TestRowState,
        duration: Duration,
        detail: Option<&str>,
    ) -> io::Result<()> {
        if index >= self.rows.len() {
            return Ok(());
        }
        self.rows[index].state = state;
        self.rows[index].duration = Some(duration);
        if self.plain {
            let name = &self.rows[index].qualified_name;
            match state {
                TestRowState::Passed => eprintln!("PASS {name}"),
                TestRowState::Failed => eprintln!("FAIL {name}"),
                TestRowState::Skipped => {
                    if let Some(reason) = detail {
                        eprintln!("SKIP {name}: {reason}");
                    } else {
                        eprintln!("SKIP {name}");
                    }
                }
                TestRowState::FilteredOut => eprintln!("FILT {name}"),
                TestRowState::Pending | TestRowState::Running => eprintln!("???? {name}"),
            }
            return Ok(());
        }
        self.redraw_table()?;
        if state == TestRowState::Failed {
            writeln!(stderr())?;
        }
        Ok(())
    }

    pub fn print_summary(
        &mut self,
        passed: usize,
        failed: usize,
        skipped: usize,
        filtered_out: usize,
    ) -> io::Result<()> {
        if self.plain {
            println!(
                "Result: passed={passed}, failed={failed}, skipped={skipped}, filtered_out={filtered_out}"
            );
            return Ok(());
        }
        let summary = format!(
            "Result: passed={passed}, failed={failed}, skipped={skipped}, filtered_out={filtered_out}"
        );
        if let Some(multi) = &self.multi {
            let _ = multi.println(summary);
        } else {
            writeln!(stderr(), "{summary}")?;
        }
        Ok(())
    }

    fn redraw_table(&mut self) -> io::Result<()> {
        if self.plain {
            return Ok(());
        }
        let rows = self.rows.clone();
        let previous = self.lines_on_stderr;
        let plain = self.plain;
        self.lines_on_stderr = if let Some(multi) = &self.multi {
            multi.suspend(|| redraw_table_inner(&rows, previous, plain))?
        } else {
            redraw_table_inner(&rows, previous, plain)?
        };
        Ok(())
    }
}

fn redraw_table_inner(rows: &[TestRow], previous_lines: usize, plain: bool) -> io::Result<usize> {
    let mut err = stderr();
    clear_previous_block(&mut err, previous_lines)?;

    let count = rows.len();
    let mut lines = 0usize;
    let title = format!("Tests ({count})");
    write_box_top(&mut err, &title)?;
    lines += 1;
    for row in rows {
        write_table_line(&mut err, &format_row(row, plain))?;
        lines += 1;
    }
    write_box_bottom(&mut err)?;
    lines += 1;
    err.flush()?;
    Ok(lines)
}

/// Move up and erase a prior draw so redraws do not stack duplicate box headers.
fn clear_previous_block(out: &mut impl Write, line_count: usize) -> io::Result<()> {
    if line_count == 0 {
        return Ok(());
    }
    write!(out, "\x1b[{line_count}A")?;
    for i in 0..line_count {
        write!(out, "\x1b[2K")?;
        if i + 1 < line_count {
            write!(out, "\x1b[1B")?;
        }
    }
    write!(out, "\x1b[{line_count}A")?;
    Ok(())
}

fn format_row(row: &TestRow, plain: bool) -> String {
    let status = match row.state {
        TestRowState::Pending => "· pending".to_string(),
        TestRowState::Running => "⠋ running".to_string(),
        TestRowState::Passed => "✓ pass".to_string(),
        TestRowState::Failed => "✗ fail".to_string(),
        TestRowState::Skipped => "− skip".to_string(),
        TestRowState::FilteredOut => "○ filt".to_string(),
    };
    let time = row
        .duration
        .map(format_duration)
        .unwrap_or_else(|| "—".to_owned());
    let name_width = TABLE_INNER_WIDTH.saturating_sub(STATUS_COL + TIME_COL + 4);
    let name_plain = clip_to_width(&row.qualified_name, name_width);
    let name = maybe_link_label(row.link.as_ref(), &name_plain, plain);
    format!("{status:<STATUS_COL$} {time:>TIME_COL$}  {name}")
}

fn write_box_top(out: &mut impl Write, title: &str) -> io::Result<()> {
    let dash_count = TABLE_INNER_WIDTH.saturating_sub(title.len().saturating_add(3));
    writeln!(out, "╭─ {title} {dash}", dash = "─".repeat(dash_count))
}

fn write_table_line(out: &mut impl Write, text: &str) -> io::Result<()> {
    let visible = strip_ansi(text);
    let visible = if visible.chars().count() > TABLE_INNER_WIDTH {
        clip_to_width(&visible, TABLE_INNER_WIDTH)
    } else {
        visible
    };
    let pad = TABLE_INNER_WIDTH.saturating_sub(visible.chars().count());
    writeln!(out, "│{text}{}│", " ".repeat(pad))
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.next() == Some(']') {
                while let Some(c) = chars.next() {
                    if c == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            } else if chars.peek() == Some(&'[') {
                chars.next();
                while matches!(chars.peek(), Some(c) if *c != 'm' && *c != 'M') {
                    chars.next();
                }
                if matches!(chars.peek(), Some('m' | 'M')) {
                    chars.next();
                }
            }
            continue;
        }
        out.push(ch);
    }
    out
}

fn write_box_bottom(out: &mut impl Write) -> io::Result<()> {
    writeln!(out, "╰{}╯", "─".repeat(TABLE_INNER_WIDTH))
}

fn clip_to_width(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out = String::new();
    for ch in text.chars().take(max_chars.saturating_sub(1)) {
        out.push(ch);
    }
    out.push('…');
    out
}

fn stderr() -> io::Stderr {
    io::stderr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_row_clips_long_names() {
        let row = TestRow {
            qualified_name: "a".repeat(80),
            link: None,
            state: TestRowState::Passed,
            duration: Some(Duration::from_millis(4)),
        };
        let line = format_row(&row, true);
        assert!(line.contains("✓ pass"));
        assert!(line.contains('…'));
    }

    #[test]
    fn line_count_is_top_plus_rows_plus_bottom() {
        let rows = vec![
            TestRow {
                qualified_name: "a".to_string(),
                link: None,
                state: TestRowState::Pending,
                duration: None,
            },
            TestRow {
                qualified_name: "b".to_string(),
                link: None,
                state: TestRowState::Passed,
                duration: Some(Duration::from_millis(1)),
            },
        ];
        assert_eq!(redraw_table_inner(&rows, 0, true).unwrap(), rows.len() + 2);
    }
}
