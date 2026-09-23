pub(crate) fn is_beskid_string_representable(value: &str) -> bool {
    value.chars().all(|ch| ch == ' ' || ch.is_ascii_graphic())
}

pub(crate) fn escape_beskid_string(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            other => out.push(other),
        }
    }
    out
}
