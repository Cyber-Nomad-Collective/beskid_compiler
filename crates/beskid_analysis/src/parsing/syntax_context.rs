//! A light token scan that tells a parse diagnostic what was being parsed.
//!
//! Pest reports where parsing stopped and which tokens could follow, but not which construct was
//! open there. This scan walks the tokens before the failure, tracks open delimiters and the
//! statement or item each one belongs to, and names the innermost one ("host `App`",
//! "`let` binding `x`", "call to `Print`"). It never decides whether the source is valid.

/// Words of the grammar's `Keyword` rule: never valid as a name.
pub const RESERVED_WORDS: &[&str] = &[
    "type",
    "enum",
    "contract",
    "attribute",
    "impl",
    "extend",
    "This",
    "where",
    "match",
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
    "const",
    "mut",
    "move",
    "bulk",
    "mod",
    "use",
    "using",
    "pub",
    "test",
    "skip",
    "spawn",
    "clif",
    "async",
    "await",
    "host",
    "registry",
    "scope",
    "startup",
    "init",
    "dispose",
    "with",
    "launch",
    "inject",
    "single",
    "transient",
    "global",
    "try",
    "catch",
];

/// One token of the scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tok {
    pub kind: TokKind,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokKind {
    Word(String),
    Number,
    Str,
    Char,
    Punct(&'static str),
    /// A character the scan does not know (for example `#`).
    Other(char),
}

impl Tok {
    pub fn word(&self) -> Option<&str> {
        match &self.kind {
            TokKind::Word(word) => Some(word.as_str()),
            _ => None,
        }
    }

    pub fn is_punct(&self, text: &str) -> bool {
        matches!(&self.kind, TokKind::Punct(p) if *p == text)
    }

    pub fn is_word(&self, text: &str) -> bool {
        self.word() == Some(text)
    }

    /// The token as a diagnostic names it.
    pub fn describe(&self, source: &str) -> String {
        match &self.kind {
            TokKind::Str => "a string".to_string(),
            TokKind::Char => "a character".to_string(),
            _ => format!("`{}`", &source[self.start..self.end]),
        }
    }
}

/// Punctuation and operators, longest first so a scan takes the longest match.
pub(crate) const PUNCTS: &[&str] = &[
    "===", "!==", "```", "::", "=>", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "++", "--",
    "->", "<<", ">>", "..", "${", "@{", "(", ")", "{", "}", "[", "]", ";", ",", ".", ":", "=", "<", ">", "+", "-", "*",
    "/", "%", "!", "?", "&", "|", "^", "~", "@", "$", "'",
];

/// Skip whitespace and comments from `pos`.
pub fn skip_trivia(source: &str, mut pos: usize) -> usize {
    let bytes = source.as_bytes();
    while pos < bytes.len() {
        match bytes[pos] {
            b' ' | b'\t' | b'\n' | b'\r' => pos += 1,
            b'/' if bytes.get(pos + 1) == Some(&b'/') => {
                while pos < bytes.len() && bytes[pos] != b'\n' {
                    pos += 1;
                }
            }
            b'/' if bytes.get(pos + 1) == Some(&b'*') => {
                pos += 2;
                while pos < bytes.len() && !(bytes[pos] == b'*' && bytes.get(pos + 1) == Some(&b'/')) {
                    pos += 1;
                }
                pos = (pos + 2).min(bytes.len());
            }
            _ => break,
        }
    }
    pos
}

/// The token that starts at `pos` (after trivia), or `None` at end of input.
pub fn token_at(source: &str, pos: usize) -> Option<Tok> {
    let start = skip_trivia(source, pos.min(source.len()));
    if start >= source.len() {
        return None;
    }
    Some(lex_one(source, start))
}

/// Every token of `source` that starts before `limit`.
pub fn tokens_before(source: &str, limit: usize) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    loop {
        pos = skip_trivia(source, pos);
        if pos >= source.len() || pos >= limit {
            break;
        }
        let tok = lex_one(source, pos);
        pos = tok.end.max(pos + 1);
        out.push(tok);
    }
    out
}

/// End offset of the last token that ends at or before `pos` (where a missing `;` belongs).
pub fn previous_token_end(source: &str, pos: usize) -> Option<usize> {
    tokens_before(source, pos).iter().rev().map(|tok| tok.end).find(|end| *end <= pos)
}

fn lex_one(source: &str, start: usize) -> Tok {
    let bytes = source.as_bytes();
    let first = bytes[start];
    if first.is_ascii_alphabetic() || first == b'_' {
        let mut end = start + 1;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
            end += 1;
        }
        return Tok { kind: TokKind::Word(source[start..end].to_string()), start, end };
    }
    if first.is_ascii_digit() {
        let mut end = start + 1;
        while end < bytes.len()
            && (bytes[end].is_ascii_alphanumeric()
                || bytes[end] == b'_'
                || (bytes[end] == b'.' && bytes.get(end + 1).is_some_and(u8::is_ascii_digit)))
        {
            end += 1;
        }
        return Tok { kind: TokKind::Number, start, end };
    }
    if first == b'"' {
        return Tok { kind: TokKind::Str, start, end: skip_string(bytes, start) };
    }
    if first == b'\'' {
        // A one-character literal `'x'`, an escape `'\''` or a Unicode escape `'\u{41}'`;
        // anything longer is a stray quote.
        let end = match (bytes.get(start + 1), bytes.get(start + 2), bytes.get(start + 3)) {
            (Some(b'\\'), Some(b'u'), Some(b'{')) => {
                source[start + 4..].find('}').map_or(start + 3, |close| start + 4 + close + 1)
            }
            (Some(b'\\'), _, _) => start + 3,
            _ => start + 1 + source[start + 1..].chars().next().map_or(1, char::len_utf8),
        };
        if bytes.get(end) == Some(&b'\'') {
            return Tok { kind: TokKind::Char, start, end: end + 1 };
        }
    }
    if source[start..].starts_with("```") {
        let end = source[start + 3..].find("```").map(|rel| start + 3 + rel + 3).unwrap_or(source.len());
        return Tok { kind: TokKind::Str, start, end };
    }
    for punct in PUNCTS {
        if source[start..].starts_with(punct) {
            return Tok { kind: TokKind::Punct(punct), start, end: start + punct.len() };
        }
    }
    let ch = source[start..].chars().next().unwrap_or('?');
    Tok { kind: TokKind::Other(ch), start, end: start + ch.len_utf8() }
}

/// End of the string literal opening at `start` (past its closing quote, or end of input).
fn skip_string(bytes: &[u8], start: usize) -> usize {
    let mut pos = start + 1;
    while pos < bytes.len() {
        match bytes[pos] {
            b'\\' => pos += 2,
            b'"' => return pos + 1,
            b'$' if bytes.get(pos + 1) == Some(&b'{') => {
                pos += 2;
                let mut depth = 1usize;
                while pos < bytes.len() && depth > 0 {
                    match bytes[pos] {
                        b'"' => {
                            pos = skip_string(bytes, pos);
                            continue;
                        }
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    pos += 1;
                }
            }
            _ => pos += 1,
        }
    }
    bytes.len()
}

/// What a delimited region holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// The file or an inline module body: items.
    Items,
    /// A block of statements (function, test, `if`, loop, lambda or arm body).
    Code,
    /// The arms of a `match`.
    Match,
    /// Fields and methods of a `type`.
    TypeBody,
    /// Variants of an `enum`.
    EnumBody,
    /// Methods of an `impl`, `extend type` or `contract`.
    Methods,
    /// Body of a `host` or `scope`.
    HostBody,
    /// Fields of a struct value, attribute parameters, `registry` entries and other braces.
    Braces,
    /// Parentheses.
    Paren,
    /// Square brackets.
    Bracket,
}

/// A construct being parsed: its words and where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Construct {
    pub description: String,
    pub start: usize,
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub kind: FrameKind,
    /// The opening delimiter (`{`, `(`, `[`) and its offset; `None` for the file.
    pub opener: Option<(char, usize)>,
    /// What owns the region ("function `Main`").
    pub owner: Option<Construct>,
    /// Tokens of the statement, item or element being read in this frame.
    pub segment: Vec<Tok>,
    /// The segment's block has closed; only `else` or `catch` continue it.
    pub segment_closed: bool,
    /// This block is the value of a match arm (`pattern => { ... }`).
    pub arm_block: bool,
}

impl Frame {
    fn new(kind: FrameKind, opener: Option<(char, usize)>, owner: Option<Construct>) -> Self {
        Self { kind, opener, owner, segment: Vec::new(), segment_closed: false, arm_block: false }
    }

    /// The closing delimiter this frame waits for.
    pub fn closer(&self) -> Option<char> {
        self.opener.map(|(open, _)| match open {
            '(' => ')',
            '[' => ']',
            _ => '}',
        })
    }
}

/// Open frames at a position, outermost first.
#[derive(Debug, Clone)]
pub struct ScanContext {
    pub frames: Vec<Frame>,
}

impl ScanContext {
    /// Scan `source` up to `limit` (the failure position).
    pub fn at(source: &str, limit: usize) -> Self {
        let mut frames = vec![Frame::new(FrameKind::Items, None, None)];
        for tok in tokens_before(source, limit) {
            feed(&mut frames, tok);
        }
        Self { frames }
    }

    pub fn innermost(&self) -> &Frame {
        self.frames.last().expect("scan keeps the file frame")
    }

    /// The construct being parsed at the scan position.
    pub fn construct(&self) -> Option<Construct> {
        let frame = self.innermost();
        if matches!(frame.kind, FrameKind::Paren | FrameKind::Bracket) {
            return frame.owner.clone();
        }
        if !frame.segment.is_empty() && !frame.segment_closed {
            return describe_segment(frame.kind, &frame.segment);
        }
        frame.owner.clone()
    }

    /// The segment the failure is in (tokens of the current statement or item).
    pub fn segment(&self) -> &[Tok] {
        let frame = self.innermost();
        if frame.segment_closed { &[] } else { &frame.segment }
    }
}

fn feed(frames: &mut Vec<Frame>, tok: Tok) {
    let frame = frames.last_mut().expect("file frame");
    if frame.segment_closed {
        if tok.is_word("else") || tok.is_word("catch") {
            frame.segment_closed = false;
        } else {
            frame.segment.clear();
            frame.segment_closed = false;
        }
    }
    match &tok.kind {
        TokKind::Punct("{") | TokKind::Punct("${") | TokKind::Punct("@{") => {
            let parent_kind = frame.kind;
            let (kind, owner, arm_block) = brace_frame(parent_kind, &frame.segment, &tok);
            frame.segment.push(tok.clone());
            let mut child = Frame::new(kind, Some(('{', tok.start)), owner);
            child.arm_block = arm_block;
            frames.push(child);
        }
        TokKind::Punct("(") => {
            let owner = paren_owner(frame.kind, &frame.segment, &tok);
            frame.segment.push(tok.clone());
            frames.push(Frame::new(FrameKind::Paren, Some(('(', tok.start)), owner));
        }
        TokKind::Punct("[") => {
            let owner = bracket_owner(frame.kind, &frame.segment, &tok);
            frame.segment.push(tok.clone());
            frames.push(Frame::new(FrameKind::Bracket, Some(('[', tok.start)), owner));
        }
        TokKind::Punct(close @ ("}" | ")" | "]")) => {
            let expected = match *close {
                "}" => '}',
                ")" => ')',
                _ => ']',
            };
            if frames.len() > 1 && frames.last().and_then(Frame::closer) == Some(expected) {
                let closed = frames.pop().expect("open frame");
                let parent = frames.last_mut().expect("file frame");
                parent.segment.push(tok);
                if expected == '}' && closes_segment(parent.kind, &parent.segment, closed.kind) {
                    parent.segment_closed = true;
                }
            } else {
                frames.last_mut().expect("file frame").segment.push(tok);
            }
        }
        TokKind::Punct(";") => {
            if !matches!(frame.kind, FrameKind::Paren | FrameKind::Bracket) {
                frame.segment.clear();
            } else {
                frame.segment.push(tok);
            }
        }
        TokKind::Punct(",") => {
            if matches!(
                frame.kind,
                FrameKind::Match
                    | FrameKind::TypeBody
                    | FrameKind::EnumBody
                    | FrameKind::Braces
                    | FrameKind::Paren
                    | FrameKind::Bracket
            ) {
                frame.segment.clear();
            } else {
                frame.segment.push(tok);
            }
        }
        _ => frame.segment.push(tok),
    }
}

/// Tokens of an item segment after leading attributes and `pub`.
pub(crate) fn item_head(segment: &[Tok]) -> &[Tok] {
    let mut index = 0usize;
    while index < segment.len() {
        if segment[index].is_punct("[") {
            // The bracket's contents went to the child frame; its `]` sits in the segment.
            match segment[index..].iter().position(|tok| tok.is_punct("]")) {
                Some(close) => index += close + 1,
                None => return &segment[segment.len()..],
            }
        } else if segment[index].is_word("pub") {
            index += 1;
        } else {
            break;
        }
    }
    &segment[index..]
}

fn word_after<'a>(segment: &'a [Tok], keyword: &str) -> Option<&'a str> {
    let position = segment.iter().position(|tok| tok.is_word(keyword))?;
    segment.get(position + 1).and_then(Tok::word)
}

fn named(kind: &str, name: Option<&str>) -> String {
    match name {
        Some(name) => format!("{kind} `{name}`"),
        None => kind.to_string(),
    }
}

/// The name in `Type Name(` or `Type Name<T>(` forms: the word just before the first `(` or `<`.
fn callable_name(segment: &[Tok]) -> Option<&str> {
    let paren = segment.iter().position(|tok| tok.is_punct("(") || tok.is_punct("<"))?;
    let index = paren.checked_sub(1)?;
    segment[index].word()
}

fn describe_item(segment: &[Tok]) -> Option<Construct> {
    let head = item_head(segment);
    let first = head.first()?;
    let start = segment.first().map_or(first.start, |tok| tok.start);
    let keyword = first.word().unwrap_or("");
    let name = head.get(1).and_then(Tok::word);
    let description = match keyword {
        "host" => named("host", name),
        "type" => named("type", name),
        "enum" => named("enum", name),
        "contract" => named("contract", name),
        "attribute" => named("attribute", name),
        "test" => named("test", name),
        "macro" => named("macro", name),
        "const" => named("constant", name),
        "impl" => "`impl` block".to_string(),
        "extend" => "`extend type` block".to_string(),
        "mod" => "module declaration".to_string(),
        "use" => "`use` declaration".to_string(),
        "registry" => "`registry` block".to_string(),
        "scope" => named("scope", name),
        "init" | "dispose" | "startup" => format!("`{keyword}` hook"),
        _ if head.iter().any(|tok| tok.is_punct("(")) => named("function", callable_name(head)),
        _ => "item".to_string(),
    };
    Some(Construct { description, start })
}

fn describe_member(segment: &[Tok]) -> Option<Construct> {
    let head = item_head(segment);
    let first = head.first()?;
    let start = segment.first().map_or(first.start, |tok| tok.start);
    if first.is_word("type") {
        return Some(Construct { description: named("associated type", head.get(1).and_then(Tok::word)), start });
    }
    if head.iter().any(|tok| tok.is_punct("(")) {
        return Some(Construct { description: named("method", callable_name(head)), start });
    }
    let name = head.iter().skip(1).rev().find_map(Tok::word);
    Some(Construct { description: named("field", name), start })
}

/// `Type name` at the start of a statement: the binding name, if the tokens read as one.
fn typed_binding_name(segment: &[Tok]) -> Option<&str> {
    let mut previous_is_type_end = false;
    for (index, tok) in segment.iter().enumerate() {
        if let Some(word) = tok.word() {
            if previous_is_type_end && index > 0 {
                let next = segment.get(index + 1);
                if next.is_none_or(|next| next.is_punct("=") || next.is_punct(";")) {
                    return Some(word);
                }
                return None;
            }
            previous_is_type_end = word != "mut";
        } else if tok.is_punct(">") || tok.is_punct("]") {
            previous_is_type_end = true;
        } else if tok.is_punct(".") || tok.is_punct("<") || tok.is_punct("[") || tok.is_punct(",") {
            previous_is_type_end = false;
        } else {
            return None;
        }
    }
    None
}

fn describe_statement(segment: &[Tok]) -> Option<Construct> {
    let first = segment.first()?;
    let start = first.start;
    let description = match first.word().unwrap_or("") {
        "let" => {
            let name = segment.iter().skip(1).find_map(|tok| tok.word().filter(|word| *word != "mut"));
            named("`let` binding", name)
        }
        "return" => "`return` statement".to_string(),
        "if" => "`if` statement".to_string(),
        "else" => "`else` branch".to_string(),
        "while" => "`while` loop".to_string(),
        "for" => "`for` loop".to_string(),
        "match" => "`match` expression".to_string(),
        "break" => "`break` statement".to_string(),
        "continue" => "`continue` statement".to_string(),
        "use" => "scoped `use`".to_string(),
        "with" => "`with` statement".to_string(),
        "launch" => "`launch` statement".to_string(),
        "try" => "`try` expression".to_string(),
        _ => match typed_binding_name(segment) {
            Some(name) => format!("binding `{name}`"),
            None => "statement".to_string(),
        },
    };
    Some(Construct { description, start })
}

fn describe_segment(kind: FrameKind, segment: &[Tok]) -> Option<Construct> {
    match kind {
        FrameKind::Items | FrameKind::HostBody => describe_item(segment),
        FrameKind::TypeBody | FrameKind::Methods => describe_member(segment),
        FrameKind::Code => describe_statement(segment),
        FrameKind::Match => {
            segment.first().map(|tok| Construct { description: "match arm".to_string(), start: tok.start })
        }
        FrameKind::EnumBody => segment.first().map(|tok| Construct {
            description: named("variant", item_head(segment).first().and_then(Tok::word)),
            start: tok.start,
        }),
        FrameKind::Braces | FrameKind::Paren | FrameKind::Bracket => None,
    }
}

fn brace_frame(parent: FrameKind, segment: &[Tok], open: &Tok) -> (FrameKind, Option<Construct>, bool) {
    if open.is_punct("${") {
        return (
            FrameKind::Braces,
            Some(Construct { description: "string interpolation".into(), start: open.start }),
            false,
        );
    }
    if open.is_punct("@{") {
        return (FrameKind::Braces, Some(Construct { description: "code hole".into(), start: open.start }), false);
    }
    if segment.last().is_some_and(|tok| tok.is_punct("=>")) {
        let arm = parent == FrameKind::Match;
        let description = if arm { "match arm" } else { "lambda body" };
        let start = segment.first().map_or(open.start, |tok| tok.start);
        return (FrameKind::Code, Some(Construct { description: description.into(), start }), arm);
    }
    match parent {
        FrameKind::Items | FrameKind::HostBody => {
            let owner = describe_item(segment);
            let head = item_head(segment);
            let keyword = head.first().and_then(Tok::word).unwrap_or("");
            let kind = match keyword {
                "host" | "scope" => FrameKind::HostBody,
                "type" => FrameKind::TypeBody,
                "enum" => FrameKind::EnumBody,
                "contract" | "impl" | "extend" => FrameKind::Methods,
                "mod" => FrameKind::Items,
                "attribute" | "registry" => FrameKind::Braces,
                _ => FrameKind::Code,
            };
            (kind, owner, false)
        }
        FrameKind::TypeBody | FrameKind::Methods => (FrameKind::Code, describe_member(segment), false),
        FrameKind::Code
        | FrameKind::Match
        | FrameKind::Paren
        | FrameKind::Bracket
        | FrameKind::Braces
        | FrameKind::EnumBody => {
            let head = segment.first().and_then(Tok::word).unwrap_or("");
            if matches!(head, "match") && parent == FrameKind::Code
                || segment.iter().rev().take_while(|tok| !tok.is_punct("=")).any(|tok| tok.is_word("match"))
            {
                let start = segment.iter().rev().find(|tok| tok.is_word("match")).map_or(open.start, |tok| tok.start);
                return (FrameKind::Match, Some(Construct { description: "`match` expression".into(), start }), false);
            }
            let block_statement = parent == FrameKind::Code
                && matches!(head, "if" | "else" | "while" | "for" | "with" | "use" | "try" | "catch");
            let after_block_keyword = segment.last().is_some_and(|tok| {
                tok.is_word("else")
                    || tok.is_word("try")
                    || tok.is_word("clif")
                    || tok.is_punct(")")
                    || tok.is_punct("(")
            });
            if block_statement
                || after_block_keyword
                || segment.is_empty()
                || segment.last().is_some_and(|tok| tok.is_punct("="))
            {
                let owner = if parent == FrameKind::Code { describe_statement(segment) } else { None };
                let owner = owner.or_else(|| Some(Construct { description: "block".into(), start: open.start }));
                return (FrameKind::Code, owner, false);
            }
            let name = segment.last().and_then(Tok::word);
            let start = segment.last().map_or(open.start, |tok| tok.start);
            (FrameKind::Braces, Some(Construct { description: named("struct value", name), start }), false)
        }
    }
}

fn paren_owner(parent: FrameKind, segment: &[Tok], open: &Tok) -> Option<Construct> {
    let previous = segment.last();
    let item_level =
        matches!(parent, FrameKind::Items | FrameKind::HostBody | FrameKind::TypeBody | FrameKind::Methods);
    match previous.and_then(Tok::word) {
        Some(word) if item_level => {
            let head = item_head(segment);
            let owner = describe_item(segment).map(|item| item.description).unwrap_or_else(|| format!("`{word}`"));
            let description = if head.first().is_some_and(|tok| tok.is_word("attribute") || tok.is_word("host")) {
                format!("parameters of {owner}")
            } else if head.len() >= 2
                || head
                    .first()
                    .is_some_and(|tok| tok.is_word("init") || tok.is_word("dispose") || tok.is_word("startup"))
            {
                format!("parameters of `{word}`")
            } else {
                format!("attribute `{word}`")
            };
            Some(Construct { description, start: previous.map_or(open.start, |tok| tok.start) })
        }
        Some("if" | "while" | "match" | "return" | "for" | "in") => {
            Some(Construct { description: "parenthesized expression".into(), start: open.start })
        }
        Some("range") => Some(Construct {
            description: "`range(...)` expression".into(),
            start: previous.map_or(open.start, |tok| tok.start),
        }),
        Some("catch") => Some(Construct { description: "`catch` binding".into(), start: open.start }),
        Some(word) => Some(Construct {
            description: format!("call to `{word}`"),
            start: previous.map_or(open.start, |tok| tok.start),
        }),
        None if previous.is_some_and(|tok| tok.is_punct(">") || tok.is_punct(")")) => {
            Some(Construct { description: "call arguments".into(), start: open.start })
        }
        None => Some(Construct { description: "parenthesized expression".into(), start: open.start }),
    }
}

fn bracket_owner(parent: FrameKind, segment: &[Tok], open: &Tok) -> Option<Construct> {
    let item_level =
        matches!(parent, FrameKind::Items | FrameKind::HostBody | FrameKind::TypeBody | FrameKind::Methods);
    let previous = segment.last();
    let description = if (item_level || parent == FrameKind::Code) && item_head(segment).is_empty() {
        "attribute"
    } else if previous.is_some_and(|tok| tok.word().is_some() || tok.is_punct(")") || tok.is_punct("]")) {
        "index"
    } else {
        "array"
    };
    Some(Construct { description: description.into(), start: open.start })
}

/// Whether a closed brace region ends the parent's statement or item.
fn closes_segment(parent: FrameKind, segment: &[Tok], closed: FrameKind) -> bool {
    match parent {
        FrameKind::Items | FrameKind::HostBody | FrameKind::TypeBody | FrameKind::Methods => true,
        FrameKind::Code => {
            closed == FrameKind::Code
                && segment.first().and_then(Tok::word).is_some_and(|word| {
                    matches!(word, "if" | "else" | "while" | "for" | "with" | "use" | "try" | "catch")
                })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn construct(source: &str, marker: &str) -> Option<String> {
        let pos = source.find(marker).expect("marker");
        ScanContext::at(source, pos).construct().map(|construct| construct.description)
    }

    #[test]
    fn names_the_open_construct() {
        assert_eq!(construct("host App {\n}", "{").as_deref(), Some("host `App`"));
        assert_eq!(construct("i32 Main() {\n    let x: i32 = 5;\n}", ":").as_deref(), Some("`let` binding `x`"));
        assert_eq!(
            construct("i32 Main() {\n    i32 x = 5\n    return x;\n}", "return").as_deref(),
            Some("binding `x`")
        );
        assert_eq!(construct("i32 Main() {\n    Foo(1, 2;\n}", ";").as_deref(), Some("call to `Foo`"));
        assert_eq!(construct("type Point {\n    i32 X;\n}", ";").as_deref(), Some("field `X`"));
        assert_eq!(construct("[Attr] pub i32 Run(i32 a; ) { }", ";").as_deref(), Some("parameters of `Run`"));
        assert_eq!(construct("unit F() {\n    if x { } else { }\n    y\n}", "y").as_deref(), Some("function `F`"));
        assert_eq!(construct("unit F() {\n    if x { } else { }\n    y = 1\n}", "1").as_deref(), Some("statement"));
    }

    #[test]
    fn marks_match_arm_blocks() {
        let source = "i32 F(i32 x) {\n    return match x { 1 => { 5 }, _ => 0 };\n}";
        let pos = source.find(" }").unwrap() + 1;
        let context = ScanContext::at(source, pos);
        assert!(context.innermost().arm_block);
    }

    #[test]
    fn lexes_strings_with_interpolation() {
        let source = "x = \"a ${ f(\"}\") } b\"; y";
        let tokens = tokens_before(source, source.len());
        assert_eq!(tokens.iter().filter(|tok| tok.kind == TokKind::Str).count(), 1);
        assert!(tokens.last().unwrap().is_word("y"));
    }

    #[test]
    fn lexes_character_literals_with_unicode_escapes() {
        for literal in ["'a'", "'\\''", "'\\u{41}'", "'\\u{1F600}'", "'é'"] {
            let source = format!("x = {literal}; y");
            let tokens = tokens_before(&source, source.len());
            let chars: Vec<&Tok> = tokens.iter().filter(|tok| tok.kind == TokKind::Char).collect();
            assert_eq!(chars.len(), 1, "{literal}: {tokens:?}");
            assert_eq!(&source[chars[0].start..chars[0].end], literal);
            assert!(tokens.last().unwrap().is_word("y"), "{literal}");
        }
    }

    /// `RESERVED_WORDS` is the grammar's `Keyword` rule, word for word and in order.
    #[test]
    fn reserved_words_match_the_grammar_keyword_rule() {
        let grammar = include_str!("../beskid.pest");
        let rule_start = grammar.find("\nKeyword = {").expect("Keyword rule");
        let rule = &grammar[rule_start..];
        let rule = &rule[..rule.find("\n}").expect("end of Keyword rule")];
        let words: Vec<String> = rule
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|name| name.ends_with("Keyword") && *name != "Keyword")
            .map(|name| {
                let definition = format!("\n{name} = @{{ \"");
                let at = grammar.find(&definition).unwrap_or_else(|| panic!("definition of {name}"));
                let text = &grammar[at + definition.len()..];
                text[..text.find('"').expect("closing quote")].to_string()
            })
            .collect();
        assert_eq!(words, RESERVED_WORDS, "update RESERVED_WORDS to the grammar's Keyword rule");
    }

    #[test]
    fn previous_token_end_skips_comments() {
        let source = "i32 x = 5 // five\n    return x;";
        let pos = source.find("return").unwrap();
        assert_eq!(previous_token_end(source, pos), Some(source.find('5').unwrap() + 1));
    }
}
