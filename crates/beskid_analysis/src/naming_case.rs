//! Case-profile checks and normalization per platform-spec code-style-and-naming.

/// Identifier case profile from language-meta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingProfile {
    PascalCase,
    LowerCamelCase,
    SnakeCase,
}

/// Reserved spellings that force a leading `_` escape (see `beskid.pest` Keyword overlap).
const RESERVED_IDENTIFIERS: &[&str] = &[
    "type",
    "enum",
    "contract",
    "attribute",
    "impl",
    "match",
    "event",
    "when",
    "if",
    "else",
    "while",
    "for",
    "in",
    "return",
    "break",
    "continue",
    "let",
    "mut",
    "mod",
    "use",
    "pub",
    "ref",
    "out",
    "test",
    "meta",
    "skip",
];

/// Strip a single leading `_` used for keyword escape.
pub fn core_ident(name: &str) -> &str {
    name.strip_prefix('_').unwrap_or(name)
}

/// True when `name` uses the keyword-escape `_` prefix.
pub fn is_keyword_escape(name: &str) -> bool {
    let Some(core) = name.strip_prefix('_') else {
        return false;
    };
    reserved_keyword_prefix_conflict(&core.to_ascii_lowercase())
}

fn reserved_keyword_prefix_conflict(lower: &str) -> bool {
    RESERVED_IDENTIFIERS.iter().any(|kw| lower == *kw || lower.starts_with(&format!("{kw}_")))
}

fn is_ascii_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Split on `_` or camel/Pascal boundaries into lowercase word tokens.
pub fn split_words(name: &str) -> Vec<String> {
    let core = core_ident(name);
    if core.is_empty() {
        return vec![String::new()];
    }
    if core.contains('_') {
        return core.split('_').filter(|p| !p.is_empty()).map(|p| p.to_ascii_lowercase()).collect();
    }
    let mut words = Vec::new();
    let mut buf = String::new();
    for (i, c) in core.char_indices() {
        if i > 0 && c.is_ascii_uppercase() && !buf.is_empty() {
            words.push(buf.to_ascii_lowercase());
            buf.clear();
        }
        buf.push(c);
    }
    if !buf.is_empty() {
        words.push(buf.to_ascii_lowercase());
    }
    words
}

fn capitalize(word: &str) -> String {
    let mut ch = word.chars();
    match ch.next() {
        None => String::new(),
        Some(c) => {
            let mut out = c.to_uppercase().collect::<String>();
            out.push_str(&ch.as_str().to_ascii_lowercase());
            out
        }
    }
}

/// Escape leading `_` when the spelling conflicts with reserved keywords.
pub fn escape_reserved(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if reserved_keyword_prefix_conflict(&lower) { format!("_{raw}") } else { raw.to_string() }
}

/// Normalize `name` toward `profile` without changing word semantics.
pub fn normalize_to_profile(name: &str, profile: NamingProfile) -> String {
    let words = split_words(name);
    if words.is_empty() || words.iter().all(|w| w.is_empty()) {
        return escape_reserved("_");
    }
    let raw = match profile {
        NamingProfile::PascalCase => words.iter().map(|w| capitalize(w)).collect::<String>(),
        NamingProfile::LowerCamelCase => {
            let mut out = words[0].clone();
            for w in words.iter().skip(1) {
                out.push_str(&capitalize(w));
            }
            out
        }
        NamingProfile::SnakeCase => words.join("_"),
    };
    escape_reserved(&raw)
}

fn profile_body_matches(name: &str, profile: NamingProfile) -> bool {
    let core = core_ident(name);
    if core.is_empty() {
        return false;
    }
    if !core.chars().all(is_ascii_ident_char) {
        return false;
    }
    match profile {
        NamingProfile::PascalCase => {
            let mut ch = core.chars();
            let Some(first) = ch.next() else {
                return false;
            };
            if !first.is_ascii_uppercase() {
                return false;
            }
            ch.all(|c| c.is_ascii_alphanumeric())
        }
        NamingProfile::LowerCamelCase => {
            let mut ch = core.chars();
            let Some(first) = ch.next() else {
                return false;
            };
            if !first.is_ascii_lowercase() {
                return false;
            }
            ch.all(|c| c.is_ascii_alphanumeric())
        }
        NamingProfile::SnakeCase => {
            let mut ch = core.chars();
            let Some(first) = ch.next() else {
                return false;
            };
            if !first.is_ascii_lowercase() {
                return false;
            }
            ch.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') && !core.contains("__")
        }
    }
}

/// Returns true when `name` already matches `profile` (keyword-escape prefix allowed).
pub fn matches_profile(name: &str, profile: NamingProfile) -> bool {
    if name == "self" && profile == NamingProfile::LowerCamelCase {
        return true;
    }
    profile_body_matches(name, profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_mixed_case() {
        assert_eq!(split_words("isTty"), vec!["is", "tty"]);
        assert_eq!(split_words("HubReceive"), vec!["hub", "receive"]);
        assert_eq!(split_words("hub_register"), vec!["hub", "register"]);
    }

    #[test]
    fn normalize_profiles() {
        assert_eq!(normalize_to_profile("hub_register", NamingProfile::PascalCase), "HubRegister");
        assert_eq!(normalize_to_profile("HubRegister", NamingProfile::LowerCamelCase), "hubRegister");
        assert_eq!(normalize_to_profile("hubRegister", NamingProfile::SnakeCase), "hub_register");
    }

    #[test]
    fn matches_corelib_examples() {
        assert!(matches_profile("IsOk", NamingProfile::PascalCase));
        assert!(matches_profile("isTty", NamingProfile::LowerCamelCase));
        assert!(matches_profile("hub_register_accepts_channel", NamingProfile::SnakeCase));
        assert!(!matches_profile("is_tty", NamingProfile::LowerCamelCase));
        assert!(!matches_profile("isok", NamingProfile::PascalCase));
    }

    #[test]
    fn self_is_exempt() {
        assert!(matches_profile("self", NamingProfile::LowerCamelCase));
    }
}
