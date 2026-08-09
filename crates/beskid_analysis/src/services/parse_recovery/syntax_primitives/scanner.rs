use super::super::scan;

/// Find the most recent unmatched delimiter opener for a delimiter pair before `through`.
pub(crate) fn find_unclosed_delimiter_before(source: &str, through: usize, open: u8, close: u8) -> Option<usize> {
    let through = through.min(source.len());
    if through == 0 || open == close {
        return None;
    }

    let mut stack: Vec<usize> = Vec::new();
    let bytes = source.as_bytes();
    let mut pos = 0usize;

    while pos < through {
        match bytes[pos] {
            b'"' | b'\'' => {
                pos = scan::skip_string_or_char(source, pos);
                continue;
            }
            b'/' if pos + 1 < through && bytes[pos + 1] == b'/' => {
                pos += 2;
                while pos < through && bytes[pos] != b'\n' {
                    pos += 1;
                }
                continue;
            }
            b'/' if pos + 1 < through && bytes[pos + 1] == b'*' => {
                pos += 2;
                while pos + 1 < through && !(bytes[pos] == b'*' && bytes[pos + 1] == b'/') {
                    pos += 1;
                }
                pos = (pos + 2).min(through);
                continue;
            }
            _ => {}
        }

        if bytes[pos] == open {
            stack.push(pos);
            pos += 1;
            continue;
        }

        if bytes[pos] == close {
            let _ = stack.pop();
            pos += 1;
            continue;
        }

        pos += 1;
    }

    stack.pop()
}

/// Find a matching close delimiter for `open_pos` by scanning forward with balanced nesting.
pub(crate) fn matching_delimiter_close(source: &str, open_pos: usize, open: u8, close: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    if open_pos >= source.len() || open == close || bytes.get(open_pos) != Some(&open) {
        return None;
    }

    let mut i = open_pos + 1;
    let limit = source.len();
    let mut depth = 0i32;

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
            _ => {}
        }

        if bytes[i] == open {
            depth += 1;
            i += 1;
            continue;
        }

        if bytes[i] == close {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
        }

        i += 1;
    }

    None
}
