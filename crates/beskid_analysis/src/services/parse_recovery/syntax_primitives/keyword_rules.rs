/// Control-flow starters used by statement-level heuristics.
pub(crate) const CONTROL_FLOW_KEYWORDS: &[&str] = &["if", "else", "while", "for", "with", "match"];
pub(crate) const CONTROL_EXPRESSION_KEYWORDS: &[&str] = &["if", "while", "for", "match"];
pub(crate) const PRIMITIVE_TYPE_KEYWORDS: &[&str] =
    &["bool", "i32", "i64", "u8", "pointer", "word", "f64", "char", "string", "unit", "never"];
pub(crate) const TERMINATOR_KEYWORDS: &[&str] = &["let", "const", "return", "break", "continue", "launch"];

pub(crate) const KEYWORDS: &[&str] = &[
    "attribute",
    "const",
    "contract",
    "enum",
    "meta",
    "event",
    "host",
    "impl",
    "extend",
    "macro",
    "range",
    "global",
    "type",
    "in",
    "use",
    "mod",
    "test",
    "match",
    "skip",
    "clif",
    "try",
    "catch",
    "startup",
    "init",
    "dispose",
    "scope",
    "registry",
    "when",
    "if",
    "else",
    "while",
    "for",
    "return",
    "break",
    "continue",
    "let",
    "mut",
    "with",
    "launch",
    "spawn",
    "async",
    "await",
    "code",
    "inject",
    "single",
    "transient",
    "parent",
    "pub",
    "as",
];

pub(crate) const RULE_KEYWORDS: &[(&str, &str)] = &[
    ("TypeDeclarationKeyword", "type"),
    ("EnumDeclarationKeyword", "enum"),
    ("ContractDeclarationKeyword", "contract"),
    ("AttributeDeclarationKeyword", "attribute"),
    ("ImplKeyword", "impl"),
    ("ExtendKeyword", "extend"),
    ("MatchKeyword", "match"),
    ("EventKeyword", "event"),
    ("WhenKeyword", "when"),
    ("IfKeyword", "if"),
    ("ElseKeyword", "else"),
    ("WhileKeyword", "while"),
    ("ForKeyword", "for"),
    ("InKeyword", "in"),
    ("ReturnKeyword", "return"),
    ("BreakKeyword", "break"),
    ("ContinueKeyword", "continue"),
    ("LetKeyword", "let"),
    ("ConstKeyword", "const"),
    ("MutKeyword", "mut"),
    ("ModKeyword", "mod"),
    ("UseKeyword", "use"),
    ("PubKeyword", "pub"),
    ("TestKeyword", "test"),
    ("SkipKeyword", "skip"),
    ("ClifKeyword", "clif"),
    ("TryKeyword", "try"),
    ("CatchKeyword", "catch"),
    ("SpawnKeyword", "spawn"),
    ("AsyncKeyword", "async"),
    ("AwaitKeyword", "await"),
    ("CodeKeyword", "code"),
    ("HostKeyword", "host"),
    ("RegistryKeyword", "registry"),
    ("ScopeKeyword", "scope"),
    ("StartupKeyword", "startup"),
    ("InitKeyword", "init"),
    ("DisposeKeyword", "dispose"),
    ("WithKeyword", "with"),
    ("LaunchKeyword", "launch"),
    ("InjectKeyword", "inject"),
    ("SingleKeyword", "single"),
    ("TransientKeyword", "transient"),
    ("GlobalKeyword", "global"),
    ("ParentKeyword", "parent"),
];

pub(crate) fn keyword_rule_token(rule_name: &str) -> Option<&'static str> {
    RULE_KEYWORDS.iter().find_map(|(rule, token)| if *rule == rule_name { Some(*token) } else { None })
}

pub(crate) fn strip_keyword_suffix(rule_name: &str) -> &str {
    rule_name
        .strip_suffix("Keyword")
        .or_else(|| rule_name.strip_suffix("keyword"))
        .or_else(|| rule_name.strip_suffix("_Keyword"))
        .or_else(|| rule_name.strip_suffix("_keyword"))
        .unwrap_or(rule_name)
}

/// Derive the keyword surface form from a grammar rule name when no explicit mapping exists.
///
/// This primarily catches any future keyword-style rules without requiring a new manual map entry.
pub(crate) fn derive_keyword_rule_token(rule_name: &str) -> Option<String> {
    let base = rule_name
        .strip_suffix("Keyword")
        .or_else(|| rule_name.strip_suffix("keyword"))
        .or_else(|| rule_name.strip_suffix("_Keyword"))
        .or_else(|| rule_name.strip_suffix("_keyword"))?;
    if base.is_empty() {
        return None;
    }

    let mut out = String::with_capacity(base.len());
    for (index, c) in base.chars().enumerate() {
        if c == '_' {
            break;
        }
        if index > 0 && c.is_ascii_uppercase() && out.chars().last().is_some_and(|prev| prev.is_ascii_lowercase()) {
            break;
        }
        if c == '_' {
            break;
        }
        out.push(c.to_ascii_lowercase());
    }

    if out.is_empty() || !out.chars().next().unwrap_or(' ').is_ascii_alphabetic() { None } else { Some(out) }
}

pub(crate) fn keyword_rule_token_or_derived(rule_name: &str) -> Option<&'static str> {
    let base = strip_keyword_suffix(rule_name);
    keyword_rule_token(base).or_else(|| {
        derive_keyword_rule_token(rule_name).map(|derived| {
            // New derived keyword entries are expected to be infrequent and only used as
            // fallback coverage for uncommon grammar additions.
            Box::leak(derived.into_boxed_str())
        } as &'static str)
    })
}

/// Full syntax-start keyword surface, used for sync boundaries.
pub(crate) const SYNC_KEYWORDS: &[&str] = KEYWORDS;

/// Keywords where a sync boundary should avoid semicolon insertion.
pub(super) const NON_SEMI_SYNC_KEYWORDS: &[&str] = &["else", "in", "match", "when", "as"];

/// Top-level item starter surface shared by item-focused heuristics.
pub(crate) const ITEM_START_KEYWORDS: &[&str] = &[
    "host",
    "macro",
    "impl",
    "extend",
    "type",
    "enum",
    "contract",
    "test",
    "attribute",
    "mod",
    "use",
    "scope",
    "registry",
    "meta",
    "skip",
    "single",
    "transient",
];

/// Keywords that introduce a brace-delimited item body and should prefer body-close recovery.
pub(crate) const ITEM_BODY_OPEN_KEYWORDS: &[&str] = &[
    "type",
    "enum",
    "contract",
    "impl",
    "extend",
    "attribute",
    "macro",
    "test",
    "host",
    "mod",
    "scope",
    "registry",
    "meta",
    "skip",
    "single",
    "transient",
];

pub(crate) const RULE_FAMILY_FALLBACKS: &[(&str, &str, &str, u8)] = &[
    ("Expression", "true", "inserted parser-expected expression", 45),
    ("Statement", "true;", "inserted parser-expected statement", 44),
    ("Body", "{ }", "inserted parser-expected block body", 44),
    ("Definition", "word() { }", "inserted parser-expected definition", 44),
    ("Declaration", "let value = 0;", "inserted parser-expected declaration", 44),
    ("Clause", "=>", "inserted parser-expected clause", 42),
    ("Operator", "=", "inserted parser-expected operator", 42),
    ("Separator", ";", "inserted parser-expected separator", 40),
    ("List", "value", "inserted parser-expected list entry", 42),
    ("Field", "field: word", "inserted parser-expected structure field", 46),
    ("Path", "path", "inserted parser-expected path", 46),
    ("Type", "word", "inserted parser-expected type", 46),
    ("Parameter", "word", "inserted parser-expected parameter", 44),
    ("Argument", "word", "inserted parser-expected parameter", 44),
    ("Item", "let value = 0", "inserted parser-expected item", 44),
    ("Literal", "0", "inserted parser-expected literal", 44),
    ("Number", "0", "inserted parser-expected literal", 44),
    ("Comment", "///", "inserted parser-expected comment", 40),
    ("Identifier", "value", "inserted parser-expected identifier", 40),
    ("LanguageTag", "txt", "inserted parser-expected code language tag", 40),
    ("StringText", "text", "inserted parser-expected string text", 40),
    ("StringEscape", "\\", "inserted parser-expected string escape", 38),
    ("Pattern", "_", "inserted parser-expected pattern", 45),
    ("Variant", "Value", "inserted parser-expected variant", 40),
    ("Constructor", "path()", "inserted parser-expected constructor", 40),
    ("Qualifier", "global", "inserted parser-expected qualifier", 40),
];

pub(crate) fn rule_family_fallback(rule_name: &str) -> Option<(&'static str, &'static str, u8)> {
    if let Some(fallback) = RULE_FAMILY_FALLBACKS.iter().find_map(|(name, token, reason, confidence)| {
        rule_name.contains(name).then_some((*token, *reason, *confidence))
    }) {
        return Some(fallback);
    }

    infer_rule_family_from_name(rule_name).or_else(|| infer_rule_name_shape(rule_name))
}

fn infer_rule_family_from_name(rule_name: &str) -> Option<(&'static str, &'static str, u8)> {
    if rule_name.ends_with("Statement") || rule_name.ends_with("statement") {
        return Some(("true;", "inserted parser-expected statement (shape)", 44));
    }
    if rule_name.ends_with("Expression") || rule_name.ends_with("expression") {
        return Some(("true", "inserted parser-expected expression (shape)", 44));
    }
    if rule_name.ends_with("Definition")
        || rule_name.ends_with("definition")
        || rule_name.ends_with("Decl")
        || rule_name.ends_with("decl")
        || rule_name.ends_with("Declaration")
        || rule_name.ends_with("declaration")
    {
        return Some(("let value = 0", "inserted parser-expected definition (shape)", 44));
    }
    if rule_name.ends_with("List") || rule_name.ends_with("list") {
        return Some(("value", "inserted parser-expected list entry (shape)", 42));
    }
    if rule_name.ends_with("ItemWithDocs")
        || rule_name.ends_with("itemwithdocs")
        || rule_name == "InnerItem"
        || rule_name == "inneritem"
        || rule_name == "ItemList"
        || rule_name == "itemlist"
        || rule_name.ends_with("Item")
        || rule_name.ends_with("item")
    {
        return Some(("let value = 0", "inserted parser-expected item (shape)", 42));
    }
    if rule_name.ends_with("Constructor")
        || rule_name.ends_with("constructor")
        || rule_name.ends_with("enum_constructor_nullary")
        || rule_name.ends_with("enum_constructor_with_args")
    {
        return Some(("path()", "inserted parser-expected constructor (shape)", 40));
    }
    if rule_name == "CodeExpression" || rule_name == "codeexpression" {
        return Some(("path()", "inserted parser-expected code expression (shape)", 40));
    }
    None
}

fn infer_rule_name_shape(rule_name: &str) -> Option<(&'static str, &'static str, u8)> {
    let name = rule_name.to_ascii_lowercase();
    if name.contains("list") {
        return Some(("value", "inserted parser-expected list entry (shape fragment)", 40));
    }
    if name.contains("pattern") {
        return Some(("_", "inserted parser-expected pattern (shape)", 42));
    }
    if name.contains("qual") {
        return Some(("global", "inserted parser-expected qualifier (shape)", 40));
    }
    if name.contains("body") {
        return Some(("{ }", "inserted parser-expected block body (shape)", 40));
    }
    if name.contains("keyword") {
        return keyword_rule_token_or_derived(strip_keyword_suffix(rule_name))
            .map(|keyword| (keyword, "inserted parser-expected rule shape keyword", 36));
    }
    None
}
