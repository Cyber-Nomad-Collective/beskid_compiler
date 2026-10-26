//! Optional parameters: default values and caller information.
//!
//! A parameter written `T name = <constant>` is optional. The model is C# optional parameters: the
//! default is a compile-time constant, optional parameters come after every required one, and a
//! call may omit any number of trailing optional arguments. Once type checking knows the callee
//! of a call (`beskid_queries` `semantic_contract::optional_arguments`), the compiler fills each
//! omitted argument at the call site with [`fill_omitted_arguments`], so lowering sees ordinary
//! explicit arguments.
//!
//! A compile-time constant is one of:
//! - a literal: `true`, `false`, an integer, a float, a character or a string without
//!   interpolation;
//! - a negated integer or float literal (`-1`, `-0.5`);
//! - the name of a `const` declared at the top level of the same file (its integer value).
//!
//! Enum variants, `()`, constructors and every other expression are not constants. A parameter
//! marked `[CallerFilePath]`, `[CallerLineNumber]` or `[CallerColumnNumber]` is an optional
//! parameter whose omitted argument is the call site's file, line or column instead of its
//! default.

use std::collections::HashMap;

use crate::macros::map_statement;
use crate::syntax::{
    Expression, HostBodyItem, Literal, LiteralExpression, MethodDefinition, Node, Parameter, PrimitiveType, Program,
    SpanInfo, Spanned, Statement, Type, UnaryExpression, UnaryOp, integer_literal_fits_primitive,
    integer_literal_primitive_type,
};

/// The value of a default after constant evaluation, in source spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefaultConstant {
    Bool(bool),
    /// Integer literal text, `-` first when negative, suffix kept (`-1`, `5_u8`).
    Integer(String),
    /// Float literal text, `-` first when negative, suffix kept.
    Float(String),
    /// Character literal text with its quotes.
    Char(String),
    /// String literal text with its quotes.
    String(String),
}

impl DefaultConstant {
    /// Source spelling, as diagnostics and signatures print it.
    pub fn text(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Integer(text) | Self::Float(text) | Self::Char(text) | Self::String(text) => text.clone(),
        }
    }

    /// The constant as an expression whose every node carries `span` (the call site).
    pub fn to_expression(&self, span: SpanInfo) -> Spanned<Expression> {
        let literal = |literal: Literal| {
            Spanned::new(
                Expression::Literal(Spanned::new(LiteralExpression { literal: Spanned::new(literal, span) }, span)),
                span,
            )
        };
        let negated = |inner: Spanned<Expression>| {
            Spanned::new(
                Expression::Unary(Spanned::new(
                    UnaryExpression { op: Spanned::new(UnaryOp::Neg, span), expr: Box::new(inner) },
                    span,
                )),
                span,
            )
        };
        match self {
            Self::Bool(value) => literal(Literal::Bool(*value)),
            Self::Integer(text) => match text.strip_prefix('-') {
                Some(magnitude) => negated(literal(Literal::Integer(magnitude.to_string()))),
                None => literal(Literal::Integer(text.clone())),
            },
            Self::Float(text) => match text.strip_prefix('-') {
                Some(magnitude) => negated(literal(Literal::Float(magnitude.to_string()))),
                None => literal(Literal::Float(text.clone())),
            },
            Self::Char(text) => literal(Literal::Char(text.clone())),
            Self::String(text) => literal(Literal::String(text.clone())),
        }
    }
}

/// `value` converted to the parameter type `ty` at compile time, as C# converts a constant
/// default: an integer without a suffix for an `f32`/`f64` parameter becomes the equal float
/// literal (`1` -> `1.0`), so the call site passes a value of the parameter type. Every other
/// value is returned unchanged (`default_fits` has already accepted it).
pub fn converted_to(value: DefaultConstant, ty: &Type) -> DefaultConstant {
    let float =
        matches!(ty, Type::Primitive(primitive) if matches!(primitive.node, PrimitiveType::F32 | PrimitiveType::F64));
    match value {
        DefaultConstant::Integer(text) if float && !has_integer_suffix(&text) => {
            let clean = text.replace('_', "");
            let (sign, digits) = match clean.strip_prefix('-') {
                Some(digits) => ("-", digits.to_string()),
                None => ("", clean),
            };
            let magnitude = match digits.strip_prefix("0x") {
                Some(hex) => u64::from_str_radix(hex, 16).map(|value| value.to_string()).unwrap_or(digits.clone()),
                None => digits,
            };
            DefaultConstant::Float(format!("{sign}{magnitude}.0"))
        }
        other => other,
    }
}

/// The compile-time constant `default` stands for, or `None` when it is not a constant.
/// `constant` looks up a top-level `const` of the declaring file by name.
pub fn default_constant(
    default: &Spanned<Expression>,
    constant: &dyn Fn(&str) -> Option<DefaultConstant>,
) -> Option<DefaultConstant> {
    match &default.node {
        Expression::Grouped(grouped) => default_constant(&grouped.node.expr, constant),
        Expression::Literal(literal) => literal_constant(&literal.node.literal.node),
        Expression::Unary(unary) if unary.node.op.node == UnaryOp::Neg => {
            match default_constant(&unary.node.expr, constant)? {
                DefaultConstant::Integer(text) => Some(DefaultConstant::Integer(negate(&text))),
                DefaultConstant::Float(text) => Some(DefaultConstant::Float(negate(&text))),
                _ => None,
            }
        }
        Expression::Path(path) => match path.node.path.node.segments.as_slice() {
            [segment] if segment.node.type_args.is_empty() => constant(&segment.node.name.node.name),
            _ => None,
        },
        _ => None,
    }
}

fn literal_constant(literal: &Literal) -> Option<DefaultConstant> {
    match literal {
        Literal::Bool(value) => Some(DefaultConstant::Bool(*value)),
        Literal::Integer(text) => Some(DefaultConstant::Integer(text.clone())),
        Literal::Float(text) => Some(DefaultConstant::Float(text.clone())),
        Literal::Char(text) => Some(DefaultConstant::Char(text.clone())),
        Literal::String(text) if !is_interpolated(text) => Some(DefaultConstant::String(text.clone())),
        Literal::String(_) | Literal::Unit => None,
    }
}

fn negate(text: &str) -> String {
    match text.strip_prefix('-') {
        Some(positive) => positive.to_string(),
        None => format!("-{text}"),
    }
}

/// Whether a string literal holds an interpolation `${...}` (an escaped `\${` is text).
fn is_interpolated(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + 1 < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'$' if bytes[index + 1] == b'{' => return true,
            _ => index += 1,
        }
    }
    false
}

/// Whether `value` is a constant of type `ty`. Contextual literal typing applies: an integer
/// without a suffix takes any integer type it fits and converts to `f32`/`f64`; a float without a
/// suffix takes `f32` or `f64`; a suffix must name `ty` exactly.
pub fn default_fits(value: &DefaultConstant, ty: &Type) -> bool {
    let Type::Primitive(primitive) = ty else { return false };
    let primitive = primitive.node;
    match value {
        DefaultConstant::Bool(_) => primitive == PrimitiveType::Bool,
        DefaultConstant::Char(_) => primitive == PrimitiveType::Char,
        DefaultConstant::String(_) => primitive == PrimitiveType::String,
        DefaultConstant::Integer(text) => {
            if has_integer_suffix(text) {
                return integer_literal_primitive_type(text) == primitive
                    && integer_literal_fits_primitive(text, primitive);
            }
            match primitive {
                PrimitiveType::F32 | PrimitiveType::F64 => true,
                PrimitiveType::I8
                | PrimitiveType::I16
                | PrimitiveType::I32
                | PrimitiveType::I64
                | PrimitiveType::U8
                | PrimitiveType::U16
                | PrimitiveType::U32
                | PrimitiveType::U64
                | PrimitiveType::Word => integer_literal_fits_primitive(text, primitive),
                _ => false,
            }
        }
        DefaultConstant::Float(text) => match primitive {
            PrimitiveType::F32 => !text.ends_with("_f64") && float_is_finite(text, true),
            PrimitiveType::F64 => !text.ends_with("_f32") && float_is_finite(text, false),
            _ => false,
        },
    }
}

fn has_integer_suffix(text: &str) -> bool {
    ["_i8", "_i16", "_i32", "_i64", "_u8", "_u16", "_u32", "_u64"].iter().any(|suffix| text.ends_with(suffix))
}

fn float_is_finite(text: &str, single: bool) -> bool {
    let magnitude = crate::syntax::float_literal_magnitude(text).replace('_', "");
    if single {
        magnitude.parse::<f32>().is_ok_and(f32::is_finite)
    } else {
        magnitude.parse::<f64>().is_ok_and(f64::is_finite)
    }
}

/// The caller-information attribute of a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallerKind {
    File,
    Line,
    Column,
}

impl CallerKind {
    pub fn attribute(self) -> &'static str {
        match self {
            Self::File => "CallerFilePath",
            Self::Line => "CallerLineNumber",
            Self::Column => "CallerColumnNumber",
        }
    }

    /// The parameter type the attribute requires: `string` for the file, `i64` for line and column.
    pub fn required_type(self) -> PrimitiveType {
        match self {
            Self::File => PrimitiveType::String,
            Self::Line | Self::Column => PrimitiveType::I64,
        }
    }
}

pub fn caller_kind(parameter: &Parameter) -> Option<CallerKind> {
    parameter.attributes.iter().find_map(|attribute| match attribute.node.name.node.name.as_str() {
        "CallerFilePath" => Some(CallerKind::File),
        "CallerLineNumber" => Some(CallerKind::Line),
        "CallerColumnNumber" => Some(CallerKind::Column),
        _ => None,
    })
}

/// The value of the top-level `const` of `program` named `name`: any literal a default may be
/// (boolean, integer, float, character, or a string without interpolation).
pub fn top_level_constant(program: &Program, name: &str) -> Option<DefaultConstant> {
    program.items.iter().find_map(|item| match &item.node {
        Node::ConstantDefinition(constant) if constant.node.name.node.name == name => {
            literal_constant(&constant.node.value.node)
        }
        _ => None,
    })
}

/// What the compiler passes for one omitted optional parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmittedArgument {
    /// The parameter's default value, converted to the parameter type.
    Default(DefaultConstant),
    /// The call site's file, line or column.
    Caller(CallerKind),
}

/// The arguments the compiler passes for the trailing optional parameters of `parameters`, in
/// order (one per parameter from the first optional one on), or `None` when there is no optional
/// parameter or the declaration is invalid (a required parameter after an optional one, a default
/// that is not a constant, a caller-information parameter without a default, a `bulk` default).
/// Invalid declarations are reported by the declaration rules; their calls must pass every
/// argument. `program` is the declaring file (for `const` defaults).
pub fn omitted_arguments(parameters: &[Spanned<Parameter>], program: &Program) -> Option<Vec<OmittedArgument>> {
    let first = parameters.iter().position(|parameter| parameter.node.default.is_some())?;
    parameters[first..]
        .iter()
        .map(|parameter| {
            let default = parameter.node.default.as_ref()?;
            if parameter.node.bulk {
                return None;
            }
            let value = default_constant(default, &|name| top_level_constant(program, name))?;
            Some(match caller_kind(&parameter.node) {
                Some(kind) => OmittedArgument::Caller(kind),
                None => OmittedArgument::Default(converted_to(value, &parameter.node.ty.node)),
            })
        })
        .collect()
}

/// `program` with the omitted arguments appended to each call named in `fills` by the span
/// (`start`, `end`) of its call expression. Appended nodes carry a zero-width span just before
/// the call's closing parenthesis; caller information is `file` (the unit's requested path) and the 1-based line and
/// column where the callee expression starts.
pub fn fill_omitted_arguments(
    program: &Spanned<Program>,
    file: &str,
    fills: &HashMap<(usize, usize), Vec<OmittedArgument>>,
) -> Spanned<Program> {
    let mut program = program.clone();
    let mut map = |expression: Spanned<Expression>| -> Spanned<Expression> {
        let Expression::Call(call) = &expression.node else { return expression };
        let Some(arguments) = fills
            .get(&(call.span.start, call.span.end))
            .or_else(|| fills.get(&(expression.span.start, expression.span.end)))
        else {
            return expression;
        };
        // Caller information names where the callee expression starts. The appended nodes have
        // no source text of their own: they carry a zero-width span just before the closing
        // parenthesis, which editors skip (no inlay hint, hover or definition lands on them).
        let (line, column) = call.node.callee.span.line_col_start;
        let span = synthetic_argument_span(expression.span);
        let mut call = call.clone();
        for argument in arguments {
            call.node.args.push(match argument {
                OmittedArgument::Default(value) => value.to_expression(span),
                OmittedArgument::Caller(kind) => {
                    let literal = match kind {
                        CallerKind::File => Literal::String(format!("\"{}\"", escape(file))),
                        CallerKind::Line => Literal::Integer(format!("{line}_i64")),
                        CallerKind::Column => Literal::Integer(format!("{column}_i64")),
                    };
                    let literal = Spanned::new(literal, span);
                    Spanned::new(Expression::Literal(Spanned::new(LiteralExpression { literal }, span)), span)
                }
            });
        }
        Spanned::new(Expression::Call(call), expression.span)
    };
    for item in &mut program.node.items {
        rewrite_item(item, &mut map);
    }
    program
}

/// A zero-width span just before the closing parenthesis of the call spanned by `call`.
fn synthetic_argument_span(call: SpanInfo) -> SpanInfo {
    let at = call.end.saturating_sub(1).max(call.start);
    let (line, column) = call.line_col_end;
    let position = (line, column.saturating_sub(1).max(1));
    SpanInfo { start: at, end: at, line_col_start: position, line_col_end: position }
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

type Map<'a> = dyn FnMut(Spanned<Expression>) -> Spanned<Expression> + 'a;

fn rewrite_block(statements: &mut Vec<Spanned<Statement>>, map: &mut Map<'_>) {
    *statements =
        std::mem::take(statements).into_iter().map(|statement| map_statement(statement, &mut |e| map(e))).collect();
}

fn rewrite_methods(methods: &mut [Spanned<MethodDefinition>], map: &mut Map<'_>) {
    for method in methods {
        rewrite_block(&mut method.node.body.node.statements, map);
    }
}

fn rewrite_host_body(body: &mut [Spanned<HostBodyItem>], map: &mut Map<'_>) {
    for item in body {
        match &mut item.node {
            HostBodyItem::Hook(hook) => rewrite_block(&mut hook.node.body.node.statements, map),
            HostBodyItem::Scope(scope) => rewrite_host_body(&mut scope.node.body, map),
            HostBodyItem::Registry(_) | HostBodyItem::Registration(_) => {}
        }
    }
}

fn rewrite_item(item: &mut Spanned<Node>, map: &mut Map<'_>) {
    match &mut item.node {
        Node::Function(function) => rewrite_block(&mut function.node.body.node.statements, map),
        Node::Method(method) => rewrite_block(&mut method.node.body.node.statements, map),
        Node::ImplBlock(implementation) => rewrite_methods(&mut implementation.node.methods, map),
        Node::ExtendTypeDefinition(extension) => rewrite_methods(&mut extension.node.methods, map),
        Node::TypeDefinition(definition) => rewrite_methods(&mut definition.node.methods, map),
        Node::HostDefinition(host) => rewrite_host_body(&mut host.node.body, map),
        Node::TestDefinition(test) => rewrite_block(&mut test.node.statements, map),
        Node::InlineModule(module) => {
            for inner in &mut module.node.items {
                rewrite_item(inner, map);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::parse_program;

    fn first_parameters(source: &str) -> (Program, Vec<Spanned<Parameter>>) {
        let program = parse_program(source).expect("parse").node;
        let parameters = program
            .items
            .iter()
            .find_map(|item| match &item.node {
                Node::Function(function) => Some(function.node.parameters.clone()),
                _ => None,
            })
            .expect("function");
        (program, parameters)
    }

    fn constant_of(source: &str) -> Option<DefaultConstant> {
        let (program, parameters) = first_parameters(source);
        let default = parameters.last().and_then(|parameter| parameter.node.default.clone()).expect("default");
        default_constant(&default, &|name| top_level_constant(&program, name))
    }

    #[test]
    fn literals_negated_numbers_and_constants_are_constants() {
        assert_eq!(constant_of("unit F(string b = \"\") { return; }"), Some(DefaultConstant::String("\"\"".into())));
        assert_eq!(constant_of("unit F(bool b = true) { return; }"), Some(DefaultConstant::Bool(true)));
        assert_eq!(constant_of("unit F(i64 b = -1) { return; }"), Some(DefaultConstant::Integer("-1".into())));
        assert_eq!(constant_of("unit F(f64 b = -0.5) { return; }"), Some(DefaultConstant::Float("-0.5".into())));
        // A `const` of any literal type may be a default, not only an integer one.
        assert_eq!(
            constant_of("const Name = \"ada\";\nunit F(string b = Name) { return; }"),
            Some(DefaultConstant::String("\"ada\"".into()))
        );
        assert_eq!(constant_of("const On = true;\nunit F(bool b = On) { return; }"), Some(DefaultConstant::Bool(true)));
        assert_eq!(
            constant_of("const Ratio = 0.5;\nunit F(f64 b = -Ratio) { return; }"),
            Some(DefaultConstant::Float("-0.5".into()))
        );
        assert_eq!(constant_of("unit F(char c = 'x') { return; }"), Some(DefaultConstant::Char("'x'".into())));
        assert_eq!(
            constant_of("const Limit = 10;\nunit F(i64 b = Limit) { return; }"),
            Some(DefaultConstant::Integer("10".into()))
        );
    }

    #[test]
    fn calls_interpolations_and_unknown_names_are_not_constants() {
        assert_eq!(constant_of("unit F(i64 b = G()) { return; }"), None);
        assert_eq!(constant_of("unit F(string b = \"${x}\") { return; }"), None);
        assert_eq!(constant_of("unit F(i64 b = Other) { return; }"), None);
        assert_eq!(constant_of("unit F(bool b = !true) { return; }"), None);
        assert!(constant_of("unit F(string b = \"\\${x}\") { return; }").is_some());
    }

    #[test]
    fn contextual_literal_typing_decides_fit() {
        let fits = |value: DefaultConstant, ty: PrimitiveType| {
            default_fits(&value, &Type::Primitive(Spanned::new(ty, SpanInfo::default())))
        };
        assert!(fits(DefaultConstant::Integer("0".into()), PrimitiveType::I64));
        assert!(fits(DefaultConstant::Integer("255".into()), PrimitiveType::U8));
        assert!(!fits(DefaultConstant::Integer("300".into()), PrimitiveType::U8));
        assert!(!fits(DefaultConstant::Integer("-1".into()), PrimitiveType::U32));
        assert!(fits(DefaultConstant::Integer("1".into()), PrimitiveType::F64));
        assert!(!fits(DefaultConstant::Integer("1_i32".into()), PrimitiveType::I64));
        assert!(fits(DefaultConstant::Float("1.5".into()), PrimitiveType::F32));
        assert!(!fits(DefaultConstant::Float("1.5_f32".into()), PrimitiveType::F64));
        assert!(!fits(DefaultConstant::Float("1.5".into()), PrimitiveType::I64));
        assert!(!fits(DefaultConstant::String("\"\"".into()), PrimitiveType::I64));
        assert!(fits(DefaultConstant::String("\"\"".into()), PrimitiveType::String));
    }

    #[test]
    fn an_integer_default_for_a_float_parameter_becomes_a_float() {
        let f64_type = Type::Primitive(Spanned::new(PrimitiveType::F64, SpanInfo::default()));
        let i64_type = Type::Primitive(Spanned::new(PrimitiveType::I64, SpanInfo::default()));
        assert_eq!(
            converted_to(DefaultConstant::Integer("-2".into()), &f64_type),
            DefaultConstant::Float("-2.0".into())
        );
        assert_eq!(
            converted_to(DefaultConstant::Integer("0x10".into()), &f64_type),
            DefaultConstant::Float("16.0".into())
        );
        assert_eq!(converted_to(DefaultConstant::Integer("2".into()), &i64_type), DefaultConstant::Integer("2".into()));
    }

    #[test]
    fn a_negative_constant_becomes_a_negation_at_the_call_site() {
        let expression = DefaultConstant::Integer("-3".into()).to_expression(SpanInfo::default());
        let Expression::Unary(unary) = &expression.node else { panic!("{expression:?}") };
        assert_eq!(unary.node.op.node, UnaryOp::Neg);
    }

    fn emitted_call_arguments(program: &Spanned<Program>) -> Vec<Vec<String>> {
        let mut calls = Vec::new();
        let mut collect = |expression: Spanned<Expression>| {
            if let Expression::Call(call) = &expression.node {
                calls.push(
                    call.node
                        .args
                        .iter()
                        .map(|argument| crate::format::Emitter::new().write(&argument.node).expect("emit"))
                        .collect::<Vec<_>>(),
                );
            }
            expression
        };
        let mut items = program.node.items.clone();
        for item in &mut items {
            rewrite_item(item, &mut collect);
        }
        calls
    }

    fn call_spans(program: &Spanned<Program>) -> Vec<(usize, usize)> {
        let mut spans = Vec::new();
        let mut collect = |expression: Spanned<Expression>| {
            if let Expression::Call(call) = &expression.node {
                spans.push((call.span.start, call.span.end));
            }
            expression
        };
        let mut items = program.node.items.clone();
        for item in &mut items {
            rewrite_item(item, &mut collect);
        }
        spans
    }

    #[test]
    fn omitted_arguments_cover_the_trailing_optional_parameters() {
        let (program, parameters) = first_parameters(
            "const Limit = 4;\nunit F(bool ok, string because = \"\", f64 scale = 2, i64 n = Limit, [CallerLineNumber] i64 line = 0) { return; }",
        );
        assert_eq!(
            omitted_arguments(&parameters, &program),
            Some(vec![
                OmittedArgument::Default(DefaultConstant::String("\"\"".into())),
                OmittedArgument::Default(DefaultConstant::Float("2.0".into())),
                OmittedArgument::Default(DefaultConstant::Integer("4".into())),
                OmittedArgument::Caller(CallerKind::Line),
            ])
        );
        let (program, parameters) = first_parameters("unit F(i64 a = G(), i64 b = 1) { return; }");
        assert_eq!(omitted_arguments(&parameters, &program), None);
        let (program, parameters) = first_parameters("unit F(i64 a = 1, i64 b) { return; }");
        assert_eq!(omitted_arguments(&parameters, &program), None);
        let (program, parameters) = first_parameters("unit F(i64 a) { return; }");
        assert_eq!(omitted_arguments(&parameters, &program), None);
    }

    #[test]
    fn named_calls_get_the_omitted_arguments_and_others_stay() {
        let source = "unit Run() {\n    Say(\"a\");\n    Other(1);\n}\ntype Box { unit Go() { Say(\"b\"); } }\nhost App() { startup() { Say(\"c\"); } }\n";
        let program = parse_program(source).expect("parse");
        let spans = call_spans(&program);
        let fills = [spans[0], spans[2], spans[3]]
            .into_iter()
            .map(|span| {
                (
                    span,
                    vec![
                        OmittedArgument::Default(DefaultConstant::Integer("-2".into())),
                        OmittedArgument::Caller(CallerKind::Line),
                    ],
                )
            })
            .collect::<HashMap<_, _>>();
        let filled = fill_omitted_arguments(&program, "/app/Main.bd", &fills);
        let calls = emitted_call_arguments(&filled);
        assert_eq!(calls[0], ["\"a\"", "-2", "2_i64"], "{calls:?}");
        assert_eq!(calls[1], ["1"], "{calls:?}");
        assert_eq!(calls[2][1..], ["-2", "5_i64"], "{calls:?}");
        assert_eq!(calls[3].len(), 3, "{calls:?}");
    }

    #[test]
    fn the_caller_file_is_escaped() {
        let program = parse_program("unit Run() { Say(); }").expect("parse");
        let fills = call_spans(&program)
            .into_iter()
            .map(|span| (span, vec![OmittedArgument::Caller(CallerKind::File)]))
            .collect::<HashMap<_, _>>();
        let filled = fill_omitted_arguments(&program, "C:\\a\"b", &fills);
        assert_eq!(emitted_call_arguments(&filled)[0], ["\"C:\\\\a\\\"b\""]);
    }

    /// Appended arguments have no source text: a zero-width span just before `)`, so no editor
    /// hint stacks on the callee; caller information still names where the callee starts.
    #[test]
    fn appended_arguments_have_a_zero_width_span_before_the_closing_parenthesis() {
        let source = "unit Run() {\n    Say(1);\n}\n";
        let program = parse_program(source).expect("parse");
        let fills = call_spans(&program)
            .into_iter()
            .map(|span| {
                (
                    span,
                    vec![
                        OmittedArgument::Default(DefaultConstant::Integer("-2".into())),
                        OmittedArgument::Caller(CallerKind::Column),
                    ],
                )
            })
            .collect::<HashMap<_, _>>();
        let filled = fill_omitted_arguments(&program, "/app/Main.bd", &fills);
        let mut arguments = Vec::new();
        let mut collect = |expression: Spanned<Expression>| {
            if let Expression::Call(call) = &expression.node {
                arguments.extend(call.node.args.iter().map(|argument| argument.span));
            }
            expression
        };
        let mut items = filled.node.items.clone();
        for item in &mut items {
            rewrite_item(item, &mut collect);
        }
        let close = source.find(");").expect("closing parenthesis");
        assert_eq!(arguments.len(), 3, "{arguments:?}");
        assert!(arguments[0].start < arguments[0].end, "the written argument keeps its span");
        for appended in &arguments[1..] {
            assert_eq!((appended.start, appended.end), (close, close), "{appended:?}");
            assert_eq!(appended.line_col_start, (2, 10), "{appended:?}");
        }
        assert_eq!(emitted_call_arguments(&filled)[0], ["1", "-2", "5_i64"]);
    }
}
