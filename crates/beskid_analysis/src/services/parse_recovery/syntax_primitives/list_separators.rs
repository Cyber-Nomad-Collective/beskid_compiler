use super::super::scan;

/// Return the byte position of a trailing `separator` just before the next
/// top-level close delimiter for the tracked list.
pub(crate) fn trailing_separator_before_list_close(
    source: &str,
    open_pos: usize,
    through: usize,
    open: u8,
    close: u8,
    separator: u8,
) -> Option<usize> {
    let bytes = source.as_bytes();
    if open_pos >= source.len() || open == close {
        return None;
    }

    let mut i = open_pos + 1;
    let limit = through.min(source.len());
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;
    let mut angle = 0i32;
    let mut last_top_level: Option<(usize, u8)> = None;

    while i < limit {
        match bytes[i] {
            b'"' | b'\'' => {
                i = scan::skip_string_or_char(source, i);
                continue;
            }
            b'/' if i + 1 < limit && bytes[i + 1] == b'/' => {
                i += 2;
                while i < limit && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if i + 1 < limit && bytes[i + 1] == b'*' => {
                i += 2;
                while i + 1 < limit && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    i += 1;
                }
                i = (i + 2).min(limit);
                continue;
            }
            b'(' => {
                paren += 1;
            }
            b')' => {
                if paren > 0 {
                    paren -= 1;
                } else if close == b')' && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                    return last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos);
                } else if paren == 0 && close != b')' && bracket == 0 && brace == 0 && angle == 0 {
                    return None;
                }
            }
            b'[' => {
                bracket += 1;
            }
            b']' => {
                if bracket > 0 {
                    bracket -= 1;
                } else if close == b']' && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                    return last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos);
                } else if bracket == 0 && close != b']' && paren == 0 && brace == 0 && angle == 0 {
                    return None;
                }
            }
            b'{' => {
                brace += 1;
            }
            b'}' => {
                if brace > 0 {
                    brace -= 1;
                } else if close == b'}' && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                    return last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos);
                } else if brace == 0 && close != b'}' && paren == 0 && bracket == 0 && angle == 0 {
                    return None;
                }
            }
            b'<' => {
                angle += 1;
            }
            b'>' => {
                if angle > 0 {
                    angle -= 1;
                } else if close == b'>' && paren == 0 && bracket == 0 && brace == 0 && angle == 0 {
                    return last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos);
                } else if angle == 0 && close != b'>' && paren == 0 && bracket == 0 && brace == 0 {
                    return None;
                }
            }
            _ => {}
        }

        if paren == 0 && bracket == 0 && brace == 0 && angle == 0 && !bytes[i].is_ascii_whitespace() {
            last_top_level = Some((i, bytes[i]));
        }

        i += 1;
    }

    last_top_level.filter(|(_, b)| *b == separator).map(|(pos, _)| pos)
}
