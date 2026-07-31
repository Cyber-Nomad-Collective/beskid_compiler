//! Shared expected-token heuristics used by sync-style parser recovery.

use super::{scan, syntax_primitives};
use crate::parser::Rule;
use pest::error::ErrorVariant;
use std::collections::HashSet;

pub(crate) const MAX_EXPECTED_TOKEN_REPAIRS: usize = 6;
pub(crate) const MAX_SIMPLE_REPLACEMENT_TEXT: usize = 12;
const DEFAULT_KEYWORD_CONFIDENCE: u8 = 80;

#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum ReplacementTokenClass {
    Delimiter,
    Operator,
    Identifier,
    Number,
    StringLike,
    Keyword,
    Other,
}

pub(crate) fn expected_token_candidates(
    parse_error: &pest::error::Error<Rule>,
) -> Vec<(&'static str, &'static str, u8)> {
    let ErrorVariant::ParsingError { positives, .. } = &parse_error.variant else {
        return Vec::new();
    };

    let mut seen: HashSet<&'static str> = HashSet::new();
    let mut scored: Vec<(&str, &'static str, u8)> = Vec::new();

    for rule in positives {
        let Some((insert_text, reason, confidence)) = expected_token_for_rule(*rule) else {
            continue;
        };
        if !seen.insert(insert_text) {
            continue;
        }
        scored.push((insert_text, reason, confidence));
    }

    scored.sort_by_key(|item| std::cmp::Reverse(item.2));
    scored.truncate(MAX_EXPECTED_TOKEN_REPAIRS);
    scored
}

pub(crate) fn expected_keyword_tokens(parse_error: &pest::error::Error<Rule>) -> Vec<&'static str> {
    expected_tokens_by_class(parse_error, ReplacementTokenClass::Keyword)
}

pub(crate) fn expected_tokens_by_class(
    parse_error: &pest::error::Error<Rule>,
    token_class: ReplacementTokenClass,
) -> Vec<&'static str> {
    let mut seen: HashSet<&'static str> = HashSet::new();
    let mut tokens = Vec::new();

    for (text, _, _) in expected_token_candidates(parse_error) {
        if replacement_token_class(text) != token_class {
            continue;
        }
        if token_class == ReplacementTokenClass::Keyword && !syntax_primitives::is_keyword_text(text) {
            continue;
        }
        if seen.insert(text) {
            tokens.push(text);
        }
    }

    tokens
}

pub(crate) fn replacement_token_class(text: &str) -> ReplacementTokenClass {
    if text.is_empty() {
        return ReplacementTokenClass::Other;
    }
    let bytes = text.as_bytes();
    let b0 = bytes[0];
    if b0.is_ascii_digit() || (b0 == b'-' && text.len() > 1 && text.as_bytes()[1].is_ascii_digit()) {
        return ReplacementTokenClass::Number;
    }
    if b0 == b'\'' || b0 == b'"' {
        return ReplacementTokenClass::StringLike;
    }
    if scan::is_delimiter_byte(b0) {
        return ReplacementTokenClass::Delimiter;
    }
    if scan::is_operator_byte(b0) {
        return ReplacementTokenClass::Operator;
    }
    if b0 == b'_' || b0.is_ascii_alphabetic() || syntax_primitives::is_keyword_text(text) {
        if syntax_primitives::is_keyword_text(text) {
            return ReplacementTokenClass::Keyword;
        }
        return ReplacementTokenClass::Identifier;
    }
    ReplacementTokenClass::Other
}

pub(crate) fn replacement_tokens_compatible(
    replaced: ReplacementTokenClass,
    replacement: ReplacementTokenClass,
) -> bool {
    match (replaced, replacement) {
        (ReplacementTokenClass::Delimiter, ReplacementTokenClass::Delimiter) => true,
        (ReplacementTokenClass::Operator, ReplacementTokenClass::Operator) => true,
        (ReplacementTokenClass::Identifier, ReplacementTokenClass::Identifier | ReplacementTokenClass::Keyword) => true,
        (ReplacementTokenClass::Identifier, ReplacementTokenClass::Number) => true,
        (ReplacementTokenClass::Number, ReplacementTokenClass::Number | ReplacementTokenClass::Identifier) => true,
        (ReplacementTokenClass::StringLike, ReplacementTokenClass::StringLike) => true,
        (ReplacementTokenClass::Keyword, ReplacementTokenClass::Keyword | ReplacementTokenClass::Identifier) => true,
        (ReplacementTokenClass::Keyword, ReplacementTokenClass::Operator | ReplacementTokenClass::Delimiter) => false,
        (ReplacementTokenClass::Delimiter, ReplacementTokenClass::Operator) => false,
        _ => replaced == replacement,
    }
}

/// Return false when a replacement candidate is implausible for the observed token class.
pub(crate) fn is_replacement_credible(replaced: &str, replacement: &str) -> bool {
    if replaced.is_empty() || replacement.is_empty() {
        return false;
    }
    if replaced == replacement {
        return true;
    }

    let replaced_class = replacement_token_class(replaced);
    let replacement_class = replacement_token_class(replacement);
    let distance = replacement_text_cost(replaced, replacement);

    let allowed = match (replaced_class, replacement_class) {
        (ReplacementTokenClass::Delimiter, ReplacementTokenClass::Delimiter) => distance <= 2,
        (ReplacementTokenClass::Operator, ReplacementTokenClass::Operator) => distance <= 2,
        (ReplacementTokenClass::Keyword, ReplacementTokenClass::Keyword) => {
            distance <= 2 && same_prefix_shape(replaced, replacement)
        }
        (ReplacementTokenClass::Identifier, ReplacementTokenClass::Identifier | ReplacementTokenClass::Keyword) => {
            distance <= 2 && same_prefix_shape(replaced, replacement)
        }
        (ReplacementTokenClass::Number, ReplacementTokenClass::Number) => distance <= 3,
        (ReplacementTokenClass::Number, ReplacementTokenClass::Identifier) => distance <= 1,
        (ReplacementTokenClass::StringLike, ReplacementTokenClass::StringLike) => {
            distance <= 1 + (replacement.len().max(replaced.len()) / 4) as u8
        }
        (_, ReplacementTokenClass::Delimiter) | (_, ReplacementTokenClass::Operator) => false,
        _ => distance <= 2,
    };

    allowed && replacement.len() <= MAX_SIMPLE_REPLACEMENT_TEXT
}

fn same_prefix_shape(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let first_eq = a[0] == b[0];
    let first_alpha_eq = a[0].is_ascii_alphabetic() == b[0].is_ascii_alphabetic();
    first_eq || first_alpha_eq
}

pub(crate) fn is_simple_replacement_text(text: &str) -> bool {
    if text.is_empty() || text.len() > MAX_SIMPLE_REPLACEMENT_TEXT {
        return false;
    }
    if text.chars().any(|c| c.is_ascii_whitespace()) {
        return false;
    }
    if text.contains("->") || text.starts_with("path") || text.starts_with("word") || text == "let" {
        return false;
    }
    if text.bytes().any(|byte| scan::is_open_delimiter_byte(byte) || scan::is_close_delimiter_byte(byte))
        && text != "=>"
    {
        return false;
    }
    true
}

fn expected_token_for_rule(rule: Rule) -> Option<(&'static str, &'static str, u8)> {
    match rule {
        Rule::Block => Some(("{ }", "inserted parser-expected block", 95)),
        Rule::Statement => Some((";", "inserted parser-expected statement terminator", 95)),
        Rule::CodeExpression => Some(("path()", "inserted parser-expected code expression", 72)),
        Rule::LaunchStatement => Some(("launch path();", "inserted parser-expected launch statement", 82)),
        Rule::WithStatement => Some(("with path() {}", "inserted parser-expected with statement", 82)),
        Rule::ExpressionList => Some(("true", "inserted parser-expected expression list entry", 82)),
        Rule::Expression => Some(("true", "inserted parser-expected expression head", 82)),
        Rule::ExpressionStatement => Some(("true;", "inserted parser-expected expression statement", 80)),
        Rule::BeskidType => Some(("word", "inserted parser-expected type", 70)),
        Rule::TypeName => Some(("word", "inserted parser-expected type name", 70)),
        Rule::TypeFieldList => Some(("field: word", "inserted parser-expected field list entry", 66)),
        Rule::FieldWithDocs => Some(("field: word", "inserted parser-expected field declaration", 66)),
        Rule::ValueField => Some(("field: word", "inserted parser-expected value field", 66)),
        Rule::Field => Some(("field: word", "inserted parser-expected field", 66)),
        Rule::FieldValue => Some(("field: word", "inserted parser-expected struct field value", 66)),
        Rule::FieldValueList => Some(("field: word", "inserted parser-expected struct field list", 66)),
        Rule::EventField => Some(("event ok()", "inserted parser-expected event field", 64)),
        Rule::InjectField => Some(("inject word name", "inserted parser-expected inject field", 64)),
        Rule::EnumVariantList => Some(("Value", "inserted parser-expected enum variant list entry", 66)),
        Rule::EnumVariant => Some(("Value", "inserted parser-expected enum variant", 66)),
        Rule::Pattern => Some(("_", "inserted parser-expected pattern", 70)),
        Rule::PatternList => Some(("_", "inserted parser-expected pattern list entry", 68)),
        Rule::Identifier => Some(("value", "inserted parser-expected identifier", 70)),
        Rule::Path => Some(("path", "inserted parser-expected path", 68)),
        Rule::PathSegment => Some(("name", "inserted parser-expected path segment", 68)),
        Rule::PrimitiveType => Some(("word", "inserted parser-expected primitive type", 66)),
        Rule::BeskidTypeList => Some(("word", "inserted parser-expected type list", 52)),
        Rule::ParameterList => Some(("( )", "inserted parser-expected parameter list", 65)),
        Rule::ArgumentList => Some(("( )", "inserted parser-expected argument list", 65)),
        Rule::MatchKeyword => Some(("match", "inserted parser-expected match keyword", 90)),
        Rule::EventKeyword => Some(("event", "inserted parser-expected event keyword", 85)),
        Rule::IfKeyword => Some(("if", "inserted parser-expected if keyword", 90)),
        Rule::WhileKeyword => Some(("while", "inserted parser-expected while keyword", 90)),
        Rule::ForKeyword => Some(("for", "inserted parser-expected for keyword", 90)),
        Rule::SpawnKeyword => Some(("spawn", "inserted parser-expected spawn keyword", 83)),
        Rule::LetKeyword => Some(("let", "inserted parser-expected let keyword", 88)),
        Rule::ReturnKeyword => Some(("return", "inserted parser-expected return keyword", 88)),
        Rule::BreakKeyword => Some(("break", "inserted parser-expected break keyword", 88)),
        Rule::ContinueKeyword => Some(("continue", "inserted parser-expected continue keyword", 88)),
        Rule::TypeDeclarationKeyword => Some(("type", "inserted parser-expected type keyword", 84)),
        Rule::EnumDeclarationKeyword => Some(("enum", "inserted parser-expected enum keyword", 84)),
        Rule::ContractDeclarationKeyword => Some(("contract", "inserted parser-expected contract keyword", 84)),
        Rule::AttributeDeclarationKeyword => Some(("attribute", "inserted parser-expected attribute keyword", 84)),
        Rule::ImplKeyword => Some(("impl", "inserted parser-expected impl keyword", 84)),
        Rule::ExtendKeyword => Some(("extend", "inserted parser-expected extend keyword", 84)),
        Rule::ConstKeyword => Some(("const", "inserted parser-expected const keyword", 86)),
        Rule::ModKeyword => Some(("mod", "inserted parser-expected mod keyword", 84)),
        Rule::UseKeyword => Some(("use", "inserted parser-expected use keyword", 84)),
        Rule::HostKeyword => Some(("host", "inserted parser-expected host keyword", 84)),
        Rule::RegistryKeyword => Some(("registry", "inserted parser-expected registry keyword", 84)),
        Rule::ScopeKeyword => Some(("scope", "inserted parser-expected scope keyword", 84)),
        Rule::TestKeyword => Some(("test", "inserted parser-expected test keyword", 83)),
        Rule::SkipKeyword => Some(("skip", "inserted parser-expected skip keyword", 83)),
        Rule::AsyncKeyword => Some(("async", "inserted parser-expected async keyword", 83)),
        Rule::AwaitKeyword => Some(("await", "inserted parser-expected await keyword", 83)),
        Rule::CodeKeyword => Some(("code", "inserted parser-expected code keyword", 79)),
        Rule::WithKeyword => Some(("with", "inserted parser-expected with keyword", 79)),
        Rule::LaunchKeyword => Some(("launch", "inserted parser-expected launch keyword", 79)),
        Rule::ScopeHookName => Some(("init", "inserted parser-expected scope hook keyword", 72)),
        Rule::ScopeBodyItem => Some(("registry {}", "inserted parser-expected scope body item", 60)),
        Rule::ScopeDefinition => Some(("scope item() {}", "inserted parser-expected scope definition", 64)),
        Rule::RegistryBlock => Some(("registry {}", "inserted parser-expected registry block", 70)),
        Rule::HostBodyItem => Some(("registry {}", "inserted parser-expected host body item", 60)),
        Rule::HostDefinition => Some(("host Host() {}", "inserted parser-expected host definition", 72)),
        Rule::TestDefinition => Some(("test t {}", "inserted parser-expected test definition", 68)),
        Rule::TestBody => Some(("{ }", "inserted parser-expected test body", 68)),
        Rule::TestBodyItemWithDocs => Some(("return 0;", "inserted parser-expected test body item", 60)),
        Rule::InjectKeyword => Some(("inject", "inserted parser-expected inject keyword", 79)),
        Rule::MutKeyword => Some(("mut", "inserted parser-expected mut keyword", 79)),
        Rule::InKeyword => Some(("in", "inserted parser-expected in keyword", 79)),
        Rule::ElseKeyword => Some(("else", "inserted parser-expected else keyword", 79)),
        Rule::WhenKeyword => Some(("when", "inserted parser-expected when keyword", 79)),
        Rule::StartupKeyword => Some(("startup", "inserted parser-expected startup keyword", 73)),
        Rule::InitKeyword => Some(("init", "inserted parser-expected init keyword", 73)),
        Rule::DisposeKeyword => Some(("dispose", "inserted parser-expected dispose keyword", 73)),
        Rule::ParentKeyword => Some(("parent", "inserted parser-expected parent qualifier", 70)),
        Rule::GlobalKeyword => Some(("global", "inserted parser-expected global qualifier", 70)),
        Rule::SingleKeyword => Some(("single", "inserted parser-expected single registration", 70)),
        Rule::TransientKeyword => Some(("transient", "inserted parser-expected transient registration", 70)),
        Rule::PubKeyword => Some(("pub", "inserted parser-expected pub keyword", 80)),
        Rule::Literal => Some(("true", "inserted parser-expected literal", 74)),
        Rule::CodePlainBody => Some(("\"\"", "inserted parser-expected code body", 73)),
        Rule::CodePlainText => Some(("\"\"", "inserted parser-expected code text", 71)),
        Rule::CodeFence => Some(("```txt\n```", "inserted parser-expected code fence", 73)),
        Rule::CodeFenceClose => Some(("```", "inserted parser-expected code fence close", 73)),
        Rule::CodeFenceOpen => Some(("```txt\n", "inserted parser-expected code fence open", 73)),
        Rule::CodeHole => Some(("${value}", "inserted parser-expected code hole", 68)),
        Rule::CodeLanguageTag => Some(("txt", "inserted parser-expected code language tag", 62)),
        Rule::IntegerLiteral => Some(("0", "inserted parser-expected integer literal", 74)),
        Rule::FloatLiteral => Some(("0.0", "inserted parser-expected float literal", 74)),
        Rule::StringLiteral => Some(("\"\"", "inserted parser-expected string literal", 74)),
        Rule::CharLiteral => Some(("'x'", "inserted parser-expected char literal", 74)),
        Rule::GroupedExpression => Some(("(true)", "inserted parser-expected grouped expression", 72)),
        Rule::LambdaExpression => Some(("|x| x", "inserted parser-expected lambda expression", 72)),
        Rule::MatchExpression => Some(("match true { _ => 0 }", "inserted parser-expected match expression", 72)),
        Rule::MatchArm => Some(("_ => 0", "inserted parser-expected match arm", 68)),
        Rule::MatchGuard => Some(("when true", "inserted parser-expected match guard", 68)),
        Rule::CallExpression => Some(("path()", "inserted parser-expected call expression", 68)),
        Rule::PostfixExpression => Some(("path", "inserted parser-expected postfix expression", 66)),
        Rule::CallOperator => Some(("( )", "inserted parser-expected call arguments", 66)),
        Rule::MemberAccess => Some((".field", "inserted parser-expected member access", 66)),
        Rule::SubscriptOperator => Some(("[ ]", "inserted parser-expected index expression", 66)),
        Rule::TryOperator => Some(("?", "inserted parser-expected try operator", 66)),
        Rule::MacroInvocation => Some(("value!()", "inserted parser-expected macro invocation", 65)),
        Rule::MacroInvocationBlock => Some(("{ }", "inserted parser-expected macro block", 60)),
        Rule::StructLiteralExpression => Some(("path { }", "inserted parser-expected struct literal", 67)),
        Rule::ArrayLiteralExpression => Some(("[ ]", "inserted parser-expected array literal", 67)),
        Rule::EnumConstructorExpression => Some(("path()", "inserted parser-expected enum constructor", 65)),
        Rule::FunctionDefinition => Some(("word value() { }", "inserted parser-expected function definition", 46)),
        Rule::ConstantDefinition => Some(("const VALUE = 0", "inserted parser-expected const definition", 64)),
        Rule::ImplBlock => Some(("impl Word { }", "inserted parser-expected impl block", 52)),
        Rule::ExtendTypeDefinition => Some(("extend type Word { }", "inserted parser-expected extend definition", 52)),
        Rule::MacroDefinition => Some(("macro m() { }", "inserted parser-expected macro definition", 52)),
        _ => expected_token_for_rule_fallback(rule),
    }
}

fn expected_token_for_rule_fallback(rule: Rule) -> Option<(&'static str, &'static str, u8)> {
    let rule_name = format!("{rule:?}");
    let lower = syntax_primitives::keyword_rule_token_or_derived(&rule_name);
    if let Some(keyword) = lower {
        return Some((keyword, "inserted parser-expected syntax keyword", DEFAULT_KEYWORD_CONFIDENCE));
    }

    match rule_name.as_str() {
        "WHITESPACE" => Some((" ", "inserted parser-expected whitespace separator", 20)),
        "NEWLINE" => Some(("\n", "inserted parser-expected newline", 20)),
        "ASCII_ALPHA_UNDERSCORE" => Some(("v", "inserted parser-expected identifier start", 35)),
        "DecimalIntegerBody" => Some(("0", "inserted parser-expected integer body", 44)),
        "HexIntegerBody" => Some(("0x1", "inserted parser-expected hex body", 44)),
        "Keyword" => Some(("let", "inserted parser-expected keyword", 32)),
        "BlockComment" => Some(("/* */", "inserted parser-expected comment", 32)),
        "FourSlashLineComment" => Some(("////", "inserted parser-expected comment", 32)),
        "OrdinaryLineComment" => Some(("//", "inserted parser-expected comment", 32)),
        "DocGap" => Some((" ", "inserted parser-expected doc gap", 30)),
        "DocLineContent" => Some(("///", "inserted parser-expected doc comment", 32)),
        "DocRun" => Some(("///", "inserted parser-expected doc comment", 32)),
        "CodeFenceBodyContents" => Some(("\"\"", "inserted parser-expected code body contents", 34)),
        "CodeFenceBody" => Some(("\"\"", "inserted parser-expected code body", 34)),
        "CodeFenceChar" => Some(("x", "inserted parser-expected code fence body chunk", 28)),
        "CodeHole" => Some(("${}", "inserted parser-expected code hole", 28)),
        "CodeText" | "StringText" => Some(("text", "inserted parser-expected string text", 28)),
        "StringContent" => Some(("\"\"", "inserted parser-expected string content", 28)),
        "StringInterpolation" => Some(("${}", "inserted parser-expected interpolation", 28)),
        "StringLiteralValueBody" => Some(("\"\"", "inserted parser-expected string literal value", 30)),
        "StringLiteralValue" => Some(("\"\"", "inserted parser-expected string literal value", 30)),
        "Program" => None,
        "PathList" => Some(("path", "inserted parser-expected path list entry", 48)),
        "TypeBody" => Some(("field: word", "inserted parser-expected type body entry", 45)),
        "TypeConformanceList" => Some((": Path", "inserted parser-expected conformance target", 44)),
        "TypeAnnotation" => Some((": word", "inserted parser-expected type annotation", 52)),
        "FieldList" => Some(("field: word", "inserted parser-expected field list entry", 58)),
        "AttributeDeclaration" => Some(("attribute [a] {}", "inserted parser-expected attribute declaration", 54)),
        "AttributeList" => Some(("[a]", "inserted parser-expected attribute list", 54)),
        "Attribute" => Some(("[a]", "inserted parser-expected attribute", 54)),
        "AttributeArgumentList" => Some(("a: 1", "inserted parser-expected attribute argument list", 52)),
        "AttributeArgument" => Some(("name: value", "inserted parser-expected attribute argument", 52)),
        "AttributeParameterList" => Some(("name: value", "inserted parser-expected attribute parameter list", 52)),
        "AttributeParameter" => Some(("name: value", "inserted parser-expected attribute parameter", 52)),
        "AttributeTargetList" => Some(("Target", "inserted parser-expected attribute target list", 50)),
        "AttributeTarget" => Some(("Target", "inserted parser-expected attribute target", 50)),
        "Visibility" => Some(("pub", "inserted parser-expected visibility", 52)),
        "GenericArguments" => Some(("<word>", "inserted parser-expected generic argument list", 50)),
        "GenericParameters" => Some(("<word>", "inserted parser-expected generic parameter list", 50)),
        "FunctionType" => Some(("word(word)", "inserted parser-expected function type", 44)),
        "ArrowFunctionType" => Some(("(word) => word", "inserted parser-expected arrow function type", 44)),
        "EnumPath" => Some(("Item", "inserted parser-expected enum path", 50)),
        "ArrayType" => Some(("word[]", "inserted parser-expected array type", 46)),
        "enum_constructor_with_args" => Some(("path()", "inserted parser-expected enum constructor", 48)),
        "IfStatement" => Some(("if true { }", "inserted parser-expected if statement", 70)),
        "ElsePart" => Some(("else { }", "inserted parser-expected else branch", 66)),
        "WhileStatement" => Some(("while true { }", "inserted parser-expected while statement", 70)),
        "ForStatement" => Some(("for i in values { }", "inserted parser-expected for statement", 70)),
        "RangeExpression" => Some(("range(0, 1)", "inserted parser-expected range expression", 60)),
        "RangeOperator" => Some(("range", "inserted parser-expected range operator", 45)),
        "AssignmentExpression" => Some(("true", "inserted parser-expected assignment expression", 60)),
        "AssignmentOperator" => Some(("=", "inserted parser-expected assignment operator", 45)),
        "LogicalOrExpression" => Some(("true", "inserted parser-expected logical expression", 52)),
        "LogicalAndExpression" => Some(("true", "inserted parser-expected logical expression", 52)),
        "BitwiseOrExpression" => Some(("true", "inserted parser-expected bitwise expression", 52)),
        "BitwiseAndExpression" => Some(("true", "inserted parser-expected bitwise expression", 52)),
        "EqualityExpression" => Some(("true", "inserted parser-expected equality expression", 52)),
        "ComparisonExpression" => Some(("true", "inserted parser-expected comparison expression", 52)),
        "ShiftExpression" => Some(("true", "inserted parser-expected shift expression", 52)),
        "AdditionExpression" => Some(("true", "inserted parser-expected addition expression", 52)),
        "MultiplicationExpression" => Some(("true", "inserted parser-expected multiplication expression", 52)),
        "UnaryExpression" => Some(("true", "inserted parser-expected unary expression", 52)),
        "PrefixUnary" => Some(("!true", "inserted parser-expected prefix unary", 48)),
        "SpawnUnary" => Some(("spawn path()", "inserted parser-expected spawn unary", 46)),
        "PostfixOperator" => Some((".field", "inserted parser-expected postfix operator", 48)),
        "CallOperator" => Some(("( )", "inserted parser-expected call operator", 48)),
        "TryOperator" => Some(("?", "inserted parser-expected try operator", 46)),
        "MacroArgumentList" => Some(("value", "inserted parser-expected macro argument", 44)),
        "MacroFragmentKind" => Some(("statement", "inserted parser-expected macro fragment kind", 44)),
        "MacroMetavariable" => Some(("$x", "inserted parser-expected macro metavariable", 44)),
        "MacroParameterList" => Some(("word x", "inserted parser-expected macro parameter list", 44)),
        "MacroParameter" => Some(("word x", "inserted parser-expected macro parameter", 44)),
        "MatchArm" => Some(("_ => 0", "inserted parser-expected match arm", 60)),
        "MatchGuard" => Some(("when true", "inserted parser-expected match guard", 60)),
        "MemberAccess" => Some((".field", "inserted parser-expected member access", 62)),
        "SubscriptOperator" => Some(("[0]", "inserted parser-expected subscript operator", 62)),
        "PrefixExpression" => Some(("-value", "inserted parser-expected unary expression", 42)),
        "LambdaParameters" => Some(("x", "inserted parser-expected lambda parameters", 54)),
        "LambdaParameterList" => Some(("x, y", "inserted parser-expected lambda parameters", 54)),
        "LambdaParameter" => Some(("x", "inserted parser-expected lambda parameter", 54)),
        "LambdaBody" => Some(("{ }", "inserted parser-expected lambda body", 50)),
        "MethodBody" => Some(("=> true", "inserted parser-expected method body", 54)),
        "PrimaryExpression" => Some(("true", "inserted parser-expected primary expression", 54)),
        "ModuleDeclaration" => Some(("mod m;", "inserted parser-expected module declaration", 55)),
        "InlineModule" => Some(("mod m { }", "inserted parser-expected inline module", 55)),
        "InlineModuleBody" => Some(("{ }", "inserted parser-expected inline module body", 52)),
        "UseDeclaration" => Some(("use path;", "inserted parser-expected use declaration", 55)),
        "TestBodyItem" => Some(("return 0;", "inserted parser-expected test body item", 48)),
        "TestMetaSection" => Some(("meta {}", "inserted parser-expected test meta section", 46)),
        "TestMetadataEntry" => Some(("name = true;", "inserted parser-expected test metadata", 46)),
        "TestSkipSection" => Some(("skip {}", "inserted parser-expected test skip section", 46)),
        "TestSkipEntry" => Some(("name = true;", "inserted parser-expected test skip entry", 46)),
        "Parameter" => Some(("word x", "inserted parser-expected parameter", 54)),
        "ParameterList" => Some(("word x", "inserted parser-expected parameter list", 54)),
        "ParameterWithDocs" => Some(("x: word", "inserted parser-expected parameter", 54)),
        "HostBodyItem" => Some(("scope item()", "inserted parser-expected host body item", 60)),
        "ScopeHook" => Some(("init()", "inserted parser-expected scope hook", 56)),
        "ScopeBodyItem" => Some(("scope item()", "inserted parser-expected scope body item", 56)),
        "RegistryEntry" => Some(("transient Item", "inserted parser-expected registry entry", 56)),
        "RegistryBlock" => Some(("registry {}", "inserted parser-expected registry block", 70)),
        "ScopeHookName" => Some(("init", "inserted parser-expected scope hook keyword", 70)),
        "RegistrationLifetime" => Some(("transient", "inserted parser-expected registry lifetime", 50)),
        "RegistrationTarget" => Some(("for Path", "inserted parser-expected registry target", 50)),
        "ReceiverType" => Some(("Word", "inserted parser-expected receiver type", 50)),
        "BlockCommentish" => Some(("/* */", "inserted parser-expected comment", 25)),
        "Intercept" => Some(("path", "inserted parser-expected interception", 28)),
        "InterpolationExpression" => Some(("true", "inserted parser-expected interpolation expression", 30)),
        "PathSegment" => Some(("path", "inserted parser-expected path segment", 62)),
        "InferredLetStatement" => Some(("let value = true;", "inserted parser-expected inferred let statement", 52)),
        "TypedLetStatement" => Some(("i32 value = 0;", "inserted parser-expected typed let statement", 52)),
        "ContractEmbedding" => Some(("Trait", "inserted parser-expected contract embedding", 52)),
        "ContractItem" => Some(("fn value() { }", "inserted parser-expected contract item", 52)),
        "ContractMethodSignature" => {
            Some(("i32 value() { }", "inserted parser-expected contract method signature", 52))
        }
        "ContractItemWithDocs" => Some(("fn value() { }", "inserted parser-expected documented contract item", 52)),
        "CodeFenceOpen" => Some(("```txt\n", "inserted parser-expected code fence open", 40)),
        "CodeFenceClose" => Some(("```", "inserted parser-expected code fence close", 40)),
        "CodePlainBody" => Some(("\"\"", "inserted parser-expected code body", 40)),
        "CodePlainBodyContents" => Some(("\"\"", "inserted parser-expected code body contents", 40)),
        "TypeFieldList" => Some(("field: word", "inserted parser-expected type field list", 46)),
        "EventCapacity" => Some(("{ 0 }", "inserted parser-expected event capacity", 46)),
        "TestBodyItemWithDocs" => Some(("return 0;", "inserted parser-expected test body item", 46)),
        "MethodDefinition" => Some(("name() { }", "inserted parser-expected method definition", 48)),
        "ImplMethodDefinition" => Some(("i32 value() { }", "inserted parser-expected impl method definition", 48)),
        "ImplMethodWithDocs" => Some(("i32 value() { }", "inserted parser-expected impl method with docs", 48)),
        "ExpressionBody" => Some(("=> true", "inserted parser-expected expression body", 46)),
        "MacroInvocationBlock" => Some(("{ }", "inserted parser-expected macro body", 46)),
        _ => token_from_rule_family(&rule_name),
    }
}

fn token_from_rule_family(rule_name: &str) -> Option<(&'static str, &'static str, u8)> {
    syntax_primitives::rule_family_fallback(rule_name)
}

pub(crate) fn replacement_text_cost(a: &str, b: &str) -> u8 {
    let distance = replacement_levenshtein_distance(a, b);
    distance as u8
}

fn replacement_levenshtein_distance(left: &str, right: &str) -> u32 {
    if left.is_empty() {
        return right.len() as u32;
    }
    if right.is_empty() {
        return left.len() as u32;
    }

    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let mut prev: Vec<u32> = (0..=right_bytes.len() as u32).collect();
    let mut curr = vec![0u32; right_bytes.len() + 1];

    for (i, &lb) in left_bytes.iter().enumerate() {
        curr[0] = (i as u32) + 1;
        for (j, &rb) in right_bytes.iter().enumerate() {
            let insertion = curr[j] + 1;
            let deletion = prev[j + 1] + 1;
            let substitution = prev[j] + u32::from(lb != rb);
            curr[j + 1] = insertion.min(deletion).min(substitution);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[right_bytes.len()]
}

#[cfg(test)]
mod tests {
    use super::{MAX_SIMPLE_REPLACEMENT_TEXT, expected_token_for_rule_fallback};
    use crate::parser::Rule;

    #[test]
    fn maps_keyword_rules_for_fallback_surface() {
        assert!(super::syntax_primitives::keyword_rule_token("HostKeyword").is_some());
        assert!(super::syntax_primitives::keyword_rule_token("RegistryKeyword").is_some());
        assert_eq!(super::syntax_primitives::keyword_rule_token_or_derived("CustomKeyword"), Some("custom"));
        assert_eq!(
            expected_token_for_rule_fallback(Rule::ASCII_ALPHA_UNDERSCORE),
            Some(("v", "inserted parser-expected identifier start", 35)),
        );
    }

    #[test]
    fn marks_long_or_keyword_replacements_simple() {
        assert!(!super::is_simple_replacement_text("path"));
        assert!(!super::is_simple_replacement_text("word"));
        assert!(!super::is_simple_replacement_text("this is two"));
        assert!(!super::is_simple_replacement_text(""));
        assert!(super::is_simple_replacement_text("=>"));
        assert!(super::is_simple_replacement_text("true"));
        assert_eq!(MAX_SIMPLE_REPLACEMENT_TEXT, 12);
    }

    #[test]
    fn ranks_replacement_distance_by_edit_cost() {
        assert_eq!(super::replacement_text_cost("true", "true"), 0);
        assert_eq!(super::replacement_text_cost("true", "trut"), 1);
        assert_eq!(super::replacement_text_cost("while", "while "), 1);
    }

    #[test]
    fn maps_additional_grammar_rules_with_primitive_fallbacks() {
        assert!(expected_token_for_rule_fallback(Rule::CodeLanguageTag).is_some());
        assert!(expected_token_for_rule_fallback(Rule::EnumVariantWithDocs).is_some());
        assert!(expected_token_for_rule_fallback(Rule::enum_constructor_nullary).is_some());
        assert!(expected_token_for_rule_fallback(Rule::EnumPattern).is_some());
        assert!(expected_token_for_rule_fallback(Rule::InjectQualifier).is_some());
        assert!(expected_token_for_rule_fallback(Rule::StringEscape).is_some());
        assert!(expected_token_for_rule_fallback(Rule::StringText).is_some());
        assert!(expected_token_for_rule_fallback(Rule::BreakStatement).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ContinueStatement).is_some());
        assert!(expected_token_for_rule_fallback(Rule::LetStatement).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ReturnStatement).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ContractDefinition).is_some());
        assert!(expected_token_for_rule_fallback(Rule::EnumDefinition).is_some());
        assert!(expected_token_for_rule_fallback(Rule::TypeDefinition).is_some());
        assert!(expected_token_for_rule_fallback(Rule::BlockExpression).is_some());
        assert!(expected_token_for_rule_fallback(Rule::CodeExpression).is_some());
        assert!(expected_token_for_rule_fallback(Rule::PubKeyword).is_some());
        assert!(expected_token_for_rule_fallback(Rule::InnerItem).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ItemList).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ItemWithDocs).is_some());
        assert!(expected_token_for_rule_fallback(Rule::TypeName).is_some());
        assert!(expected_token_for_rule_fallback(Rule::BeskidTypeList).is_some());
        assert!(expected_token_for_rule_fallback(Rule::GenericParameters).is_some());
        assert!(expected_token_for_rule_fallback(Rule::GenericArguments).is_some());
        assert!(expected_token_for_rule_fallback(Rule::MatchExpression).is_some());
        assert!(expected_token_for_rule_fallback(Rule::MatchArm).is_some());
        assert!(expected_token_for_rule_fallback(Rule::MatchGuard).is_some());
        assert!(expected_token_for_rule_fallback(Rule::TestMetaSection).is_some());
        assert!(expected_token_for_rule_fallback(Rule::TestMetadataEntry).is_some());
        assert!(expected_token_for_rule_fallback(Rule::TestSkipSection).is_some());
        assert!(expected_token_for_rule_fallback(Rule::TestSkipEntry).is_some());
        assert!(expected_token_for_rule_fallback(Rule::TestBodyItem).is_some());
        assert!(expected_token_for_rule_fallback(Rule::TestBodyItemWithDocs).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ScopeDefinition).is_some());
        assert!(expected_token_for_rule_fallback(Rule::HostDefinition).is_some());
        assert!(expected_token_for_rule_fallback(Rule::RegistryBlock).is_some());
        assert!(expected_token_for_rule_fallback(Rule::HostBodyItem).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ScopeBodyItem).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ScopeHookName).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ScopeHook).is_some());
        assert!(expected_token_for_rule_fallback(Rule::MethodBody).is_some());
        assert!(expected_token_for_rule_fallback(Rule::ExpressionBody).is_some());
    }

    #[test]
    fn filters_unrelated_replacement_candidates_with_credibility_gate() {
        assert!(super::is_replacement_credible("let", "let"));
        assert!(super::is_replacement_credible("let", "mut"));
        assert!(!super::is_replacement_credible("let", "while"));
        assert!(!super::is_replacement_credible("value", "collection"));
        assert!(super::is_replacement_credible("->", "=>"));
        assert!(!super::is_replacement_credible("->", "let"));
        assert!(super::is_replacement_credible("123", "1"));
        assert!(!super::is_replacement_credible("123", "while"));
    }
}
