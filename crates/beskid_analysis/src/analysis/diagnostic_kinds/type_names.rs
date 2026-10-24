//! Type names as a Beskid programmer writes them: module paths use `.`, and a qualified name is
//! shortened to its last segment unless that makes two different types of one message read alike.

/// `name` with `.` module paths and every qualified path shortened to its last segment:
/// `Core::Results::Result<i32, string>` -> `Result<i32, string>`.
pub fn display_type(name: &str) -> String {
    shorten_paths(&dotted(name))
}

/// The two types of a mismatch, shortened unless shortening makes them read the same; then both
/// keep their full `.` paths.
pub fn display_type_pair(expected: &str, actual: &str) -> (String, String) {
    let (short_expected, short_actual) = (display_type(expected), display_type(actual));
    if short_expected == short_actual && dotted(expected) != dotted(actual) {
        (dotted(expected), dotted(actual))
    } else {
        (short_expected, short_actual)
    }
}

fn dotted(name: &str) -> String {
    name.replace("::", ".")
}

/// Drop the module qualifier of every path in a type expression: each run of identifier
/// characters and dots keeps only its last segment.
fn shorten_paths(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut path = String::new();
    let flush = |path: &mut String, out: &mut String| {
        out.push_str(path.rsplit('.').next().unwrap_or(path));
        path.clear();
    };
    for character in name.chars() {
        if character.is_alphanumeric() || character == '_' || character == '.' {
            path.push(character);
        } else {
            flush(&mut path, &mut out);
            out.push(character);
        }
    }
    flush(&mut path, &mut out);
    out
}

/// The outer generic name of a type expression (`Result` for `Result<i32, string>`).
pub(super) fn outer_name(name: &str) -> &str {
    name.split('<').next().unwrap_or(name).trim()
}

/// Whether `name` is a primitive numeric type, the only types a cast can convert between.
pub(super) fn is_numeric_primitive(name: &str) -> bool {
    matches!(
        name,
        "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "f32" | "f64" | "word" | "uword" | "char"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_paths_use_dots_and_are_shortened() {
        assert_eq!(display_type("Core::Results::Result<i32, string>"), "Result<i32, string>");
        assert_eq!(display_type("Core.Collections.List<App.Model.Item>[]"), "List<Item>[]");
        assert_eq!(display_type("i32"), "i32");
    }

    #[test]
    fn a_pair_keeps_full_paths_when_short_names_would_read_alike() {
        assert_eq!(
            display_type_pair("App::A::Item", "App::B::Item"),
            ("App.A.Item".to_string(), "App.B.Item".to_string())
        );
        assert_eq!(
            display_type_pair("i32", "Core::Results::Result<i32, string>"),
            ("i32".to_string(), "Result<i32, string>".to_string())
        );
    }

    #[test]
    fn outer_names_and_numeric_primitives() {
        assert_eq!(outer_name("Result<i32, string>"), "Result");
        assert!(is_numeric_primitive("i64") && !is_numeric_primitive("string"));
    }
}
