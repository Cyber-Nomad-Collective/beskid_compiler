//! Tree-style prefixes for hierarchical pipeline progress lines.

/// Branch and indent glyphs for nested pipeline progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeGlyphs {
    pub branch: &'static str,
    pub indent: &'static str,
    pub continuation: &'static str,
}

impl TreeGlyphs {
    pub fn for_plain(plain: bool) -> Self {
        if plain {
            Self {
                branch: "|- ",
                indent: "|  ",
                continuation: "|  ",
            }
        } else {
            Self {
                branch: "├─ ",
                indent: "│  ",
                continuation: "│  ",
            }
        }
    }
}

/// Prefix for a phase or work-unit line at the given nesting depth.
pub fn tree_line_prefix(depth: usize, plain: bool) -> String {
    if depth == 0 {
        return String::new();
    }
    let glyphs = TreeGlyphs::for_plain(plain);
    let mut out = String::new();
    for _ in 0..depth.saturating_sub(1) {
        out.push_str(glyphs.indent);
    }
    out.push_str(glyphs.branch);
    out
}

fn tree_continuation_prefix(depth: usize, plain: bool) -> String {
    if depth == 0 {
        return String::new();
    }
    let glyphs = TreeGlyphs::for_plain(plain);
    let mut out = String::new();
    for _ in 0..depth.saturating_sub(1) {
        out.push_str(glyphs.indent);
    }
    out.push_str(glyphs.continuation);
    out
}

pub fn format_phase_start(depth: usize, plain: bool, label: &str) -> String {
    if depth == 0 {
        label.to_owned()
    } else {
        format!("{}{label}", tree_line_prefix(depth, plain))
    }
}

pub fn format_phase_end(depth: usize, plain: bool, label: &str, duration: &str) -> String {
    let text = format!("{label} ({duration})");
    if depth == 0 {
        text
    } else {
        format!("{}{text}", tree_continuation_prefix(depth, plain))
    }
}

pub fn format_work_unit(depth: usize, plain: bool, done: u64, total: u64, label: &str) -> String {
    format!("{}[{done}/{total}] {label}", tree_line_prefix(depth, plain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_tree_uses_ascii_branch() {
        let line = format_phase_start(1, true, "Copy sources");
        assert_eq!(line, "|- Copy sources");
    }

    #[test]
    fn unicode_tree_uses_box_branch() {
        let line = format_phase_start(1, false, "Copy sources");
        assert_eq!(line, "├─ Copy sources");
    }

    #[test]
    fn nested_depth_indents_further() {
        let line = format_phase_start(2, false, "Type check");
        assert_eq!(line, "│  ├─ Type check");
    }

    #[test]
    fn phase_end_uses_continuation_not_tick() {
        let line = format_phase_end(1, false, "Type check", "12ms");
        assert_eq!(line, "│  Type check (12ms)");
    }

    #[test]
    fn top_level_has_no_branch_prefix() {
        let start = format_phase_start(0, false, "Semantic analysis");
        assert_eq!(start, "Semantic analysis");
        let end = format_phase_end(0, false, "Semantic analysis", "1s");
        assert_eq!(end, "Semantic analysis (1s)");
    }

    #[test]
    fn work_unit_line_includes_progress() {
        let line = format_work_unit(2, false, 3, 8, "Resolve (pass 1)");
        assert_eq!(line, "│  ├─ [3/8] Resolve (pass 1)");
    }
}
