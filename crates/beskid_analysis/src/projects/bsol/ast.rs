//! Normative Bsol abstract syntax tree.
//!
//! These types are the reference AST for Beskid Structured Object Language manifest files.
//! Platform spec: `site/website/src/content/docs/platform-spec/tooling/manifests-and-lockfiles/bsol/`.

/// A parsed `.bproj` or `.bws` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolDocument {
    pub blocks: Vec<BsolBlock>,
}

/// One top-level block in a manifest file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolBlock {
    pub span: BsolSpan,
    pub header: BsolBlockHeader,
    pub body: Vec<BsolBodyItem>,
}

/// Block header: either a named project root or a reserved manifest block kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BsolBlockHeader {
    /// `{ myapp { name = "myapp" ... } }` — block kind must match `name`.
    ProjectRoot { ident: String },
    /// Reserved blocks such as `target "main" { ... }`.
    Reserved {
        kind: BsolReservedBlockKind,
        label: Option<BsolQuotedString>,
    },
}

/// Reserved top-level block keywords defined by the manifest contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsolReservedBlockKind {
    Target,
    Dependency,
    Link,
    Workspace,
    Member,
    Override,
    Registry,
}

/// Body item inside a block: flat assignment or nested section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BsolBodyItem {
    Assignment(BsolAssignment),
    NestedBlock(BsolNestedBlock),
}

/// Nested block allowed only inside a project root block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolNestedBlock {
    pub span: BsolSpan,
    pub kind: BsolNestedBlockKind,
    pub assignments: Vec<BsolAssignment>,
}

/// Nested section kinds under a project root block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BsolNestedBlockKind {
    Mod,
    /// Legacy alias accepted during transition; lowered identically to [`Mod`].
    Meta,
    Template,
}

/// `key = value` assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolAssignment {
    pub span: BsolSpan,
    pub key: String,
    pub value: BsolValue,
}

/// Right-hand side of an assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BsolValue {
    QuotedString(BsolQuotedString),
    Ident(String),
    BracketList(BsolBracketList),
}

/// Double-quoted string literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolQuotedString {
    pub span: BsolSpan,
    pub value: String,
}

/// Bracket list `[a, b, "c"]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BsolBracketList {
    pub span: BsolSpan,
    pub items: Vec<BsolListItem>,
}

/// One element of a bracket list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BsolListItem {
    Default,
    QuotedString(BsolQuotedString),
    Ident(String),
}

/// UTF-8 source span with 1-based line index for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BsolSpan {
    pub start: usize,
    pub end: usize,
    /// 1-based line number.
    pub line: usize,
}

impl BsolSpan {
    pub fn from_pest(span: pest::Span<'_>, source: &str) -> Self {
        let start = span.start();
        let end = span.end();
        let line = source[..start].lines().count().max(1);
        Self { start, end, line }
    }

    pub fn merge(a: Self, b: Self) -> Self {
        Self {
            start: a.start.min(b.start),
            end: a.end.max(b.end),
            line: a.line,
        }
    }
}

impl BsolQuotedString {
    pub fn new(span: BsolSpan, raw: &str) -> Self {
        let value = raw
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(raw)
            .to_string();
        Self { span, value }
    }
}

impl BsolReservedBlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::Dependency => "dependency",
            Self::Link => "link",
            Self::Workspace => "workspace",
            Self::Member => "member",
            Self::Override => "override",
            Self::Registry => "registry",
        }
    }
}

impl BsolNestedBlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mod => "mod",
            Self::Meta => "meta",
            Self::Template => "template",
        }
    }
}
