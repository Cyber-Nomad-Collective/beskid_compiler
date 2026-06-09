//! Adapter from markdown headings into the shared outline model.

use crate::widgets::document_viewer::DocumentOutlineItem;

/// Extracts markdown headings as shared outline entries.
pub fn markdown_outline_from_content(content: &str) -> Vec<DocumentOutlineItem> {
    content
        .lines()
        .enumerate()
        .filter_map(|(index, line)| markdown_heading(line, index + 1))
        .collect()
}

/// Converts one markdown heading line into an outline item.
fn markdown_heading(line: &str, line_number: usize) -> Option<DocumentOutlineItem> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if !(1..=6).contains(&level) || !trimmed.chars().nth(level).is_some_and(char::is_whitespace) {
        return None;
    }
    let title = trimmed[level..].trim().trim_matches('#').trim();
    if title.is_empty() {
        return None;
    }
    Some(DocumentOutlineItem::new(
        title,
        line_number,
        level - 1,
        "heading",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_markdown_headings() {
        let outline = markdown_outline_from_content("# Top\ntext\n## Child");
        assert_eq!(outline.len(), 2);
        assert_eq!(outline[1].level, 1);
        assert_eq!(outline[1].line, 3);
    }
}
