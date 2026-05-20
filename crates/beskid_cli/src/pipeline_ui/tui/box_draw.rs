//! Fixed-width box drawing for stderr panels.

use std::io::{self, Write};

pub const BOX_INNER_WIDTH: usize = 40;

pub fn write_box_top(out: &mut dyn Write, title: &str) -> io::Result<()> {
    let dash_count = BOX_INNER_WIDTH.saturating_sub(title.len().saturating_add(3));
    writeln!(out, "╭─ {title} {dash}", dash = "─".repeat(dash_count))?;
    Ok(())
}

pub fn write_box_line(out: &mut dyn Write, text: &str) -> io::Result<()> {
    let clipped = clip_to_width(text, BOX_INNER_WIDTH);
    writeln!(out, "│{clipped:<BOX_INNER_WIDTH$}│")?;
    Ok(())
}

pub fn write_box_bottom(out: &mut dyn Write) -> io::Result<()> {
    writeln!(out, "╰{}╯", "─".repeat(BOX_INNER_WIDTH))?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_lines_share_fixed_width() {
        let mut buf = Vec::new();
        write_box_top(&mut buf, "Deps").unwrap();
        write_box_line(&mut buf, "● root").unwrap();
        write_box_line(&mut buf, "├─ child").unwrap();
        write_box_bottom(&mut buf).unwrap();
        let text = String::from_utf8(buf).unwrap();
        for line in text.lines().filter(|line| line.starts_with('│')) {
            assert_eq!(line.chars().count(), BOX_INNER_WIDTH + 2);
        }
    }
}
