//! Optional parameters: default values and caller information.
//!
//! A parameter written `T name = <constant>` is optional. The model is C# optional parameters: the
//! default is a compile-time constant, optional parameters come after every required one, and a
//! call may omit any number of trailing optional arguments. The compiler fills each omitted
//! argument at the call site (`projects::assembly::optional_arguments`), so every later stage sees
//! ordinary explicit arguments.
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

use crate::syntax::{
    Expression, Literal, LiteralExpression, Node, Parameter, PrimitiveType, Program, SpanInfo, Spanned, Type,
    UnaryExpression, UnaryOp, integer_literal_fits_primitive, integer_literal_primitive_type,
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
            Spanned::new(Expression::Literal(Spanned::new(LiteralExpression { literal: Spanned::new(literal, span) }, span)), span)
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
    let float = matches!(ty, Type::Primitive(primitive) if matches!(primitive.node, PrimitiveType::F32 | PrimitiveType::F64));
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
    constant: &dyn Fn(&str) -> Option<String>,
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
            [segment] if segment.node.type_args.is_empty() => {
                constant(&segment.node.name.node.name).map(DefaultConstant::Integer)
            }
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
    if single { magnitude.parse::<f32>().is_ok_and(f32::is_finite) } else { magnitude.parse::<f64>().is_ok_and(f64::is_finite) }
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

/// Number of parameters a call must pass: those without a default value.
pub fn required_parameter_count(parameters: &[Spanned<Parameter>]) -> usize {
    parameters.iter().filter(|parameter| parameter.node.default.is_none()).count()
}

/// The integer value of each top-level `const` of `program`, by name.
pub fn top_level_constant(program: &Program, name: &str) -> Option<String> {
    program.items.iter().find_map(|item| match &item.node {
        Node::ConstantDefinition(constant) if constant.node.name.node.name == name => match &constant.node.value.node {
            Literal::Integer(text) => Some(text.clone()),
            _ => None,
        },
        _ => None,
    })
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
        assert_eq!(converted_to(DefaultConstant::Integer("-2".into()), &f64_type), DefaultConstant::Float("-2.0".into()));
        assert_eq!(converted_to(DefaultConstant::Integer("0x10".into()), &f64_type), DefaultConstant::Float("16.0".into()));
        assert_eq!(converted_to(DefaultConstant::Integer("2".into()), &i64_type), DefaultConstant::Integer("2".into()));
    }

    #[test]
    fn a_negative_constant_becomes_a_negation_at_the_call_site() {
        let expression = DefaultConstant::Integer("-3".into()).to_expression(SpanInfo::default());
        let Expression::Unary(unary) = &expression.node else { panic!("{expression:?}") };
        assert_eq!(unary.node.op.node, UnaryOp::Neg);
    }

    #[test]
    fn required_count_skips_defaults() {
        let (_, parameters) = first_parameters("unit F(bool a, string b = \"\", i64 c = 0) { return; }");
        assert_eq!(required_parameter_count(&parameters), 1);
    }
}
