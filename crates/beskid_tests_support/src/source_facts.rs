//! Parsed facts of Beskid sources for tests.
//!
//! Tests that check what a runtime or Corelib source declares or calls read it through the
//! parser: function signatures, the callees of a function body, call arguments and constant
//! values. The facts do not depend on layout, so `beskid fmt` output and hand-written sources give
//! the same facts. Statement order and control flow are not facts here; behaviour tests prove
//! them.

use std::collections::BTreeSet;

use beskid_analysis::services::parse_program_with_source_name;
use beskid_analysis::syntax::{
    CallExpression, ConstantDefinition, EnumDefinition, Expression, FunctionDefinition, IfStatement, Literal, Program,
    Spanned, Statement, TypeDefinition, Visibility,
};
use beskid_analysis::syntax_query::Query;

/// One parsed Beskid source.
pub struct SourceFacts {
    name: String,
    source: String,
    program: Spanned<Program>,
}

/// One function definition of a [`SourceFacts`] source.
pub struct FunctionFacts<'a> {
    source: &'a str,
    definition: &'a FunctionDefinition,
}

/// Source text of a node with all whitespace removed (`a + b` and `a+b` compare equal).
fn compact(text: &str) -> String {
    text.split_whitespace().collect()
}

/// The called name of a call: the last path segment or member name (`Core.Io.Write` -> `Write`,
/// `self.handle.Close` -> `Close`).
fn callee_name(call: &CallExpression) -> Option<&str> {
    match &call.callee.node {
        Expression::Path(path) => {
            path.node.path.node.segments.last().map(|segment| segment.node.name.node.name.as_str())
        }
        Expression::Member(member) => Some(member.node.member.node.name.as_str()),
        _ => None,
    }
}

impl SourceFacts {
    /// Parse `source`; panics with `name` when it does not parse.
    pub fn parse(name: impl Into<String>, source: impl Into<String>) -> Self {
        let name = name.into();
        let source = source.into();
        let program = parse_program_with_source_name(&name, &source)
            .unwrap_or_else(|error| panic!("{name} does not parse: {error:?}"));
        Self { name, source, program }
    }

    /// Names of every function definition in the source.
    pub fn function_names(&self) -> BTreeSet<String> {
        Query::from(&self.program.node)
            .of::<FunctionDefinition>()
            .map(|definition| definition.name.node.name.clone())
            .collect()
    }

    /// The one function definition named `name`; panics when there is none or more than one.
    pub fn function(&self, name: &str) -> FunctionFacts<'_> {
        let mut matches = Query::from(&self.program.node)
            .of::<FunctionDefinition>()
            .filter(|definition| definition.name.node.name == name);
        let definition = matches.next().unwrap_or_else(|| panic!("{} defines no function {name}", self.name));
        assert!(matches.next().is_none(), "{} defines more than one function {name}", self.name);
        FunctionFacts { source: &self.source, definition }
    }

    /// Every function definition in the source, in source order.
    pub fn functions(&self) -> Vec<FunctionFacts<'_>> {
        Query::from(&self.program.node)
            .of::<FunctionDefinition>()
            .map(|definition| FunctionFacts { source: &self.source, definition })
            .collect()
    }

    /// Names of every `type` and `enum` definition in the source.
    pub fn type_names(&self) -> BTreeSet<String> {
        Query::from(&self.program.node)
            .of::<TypeDefinition>()
            .map(|definition| definition.name.node.name.clone())
            .chain(
                Query::from(&self.program.node)
                    .of::<EnumDefinition>()
                    .map(|definition| definition.name.node.name.clone()),
            )
            .collect()
    }

    /// Integer value of the constant `name`; panics when there is none or it is not an integer.
    pub fn constant(&self, name: &str) -> i64 {
        let constant = Query::from(&self.program.node)
            .of::<ConstantDefinition>()
            .find(|constant| constant.name.node.name == name)
            .unwrap_or_else(|| panic!("{} defines no constant {name}", self.name));
        let Literal::Integer(text) = &constant.value.node else {
            panic!("{} constant {name} is not an integer: {:?}", self.name, constant.value.node)
        };
        // `65536`, `-1`, `0_i64`: the value is the leading digits, without separators or suffix.
        let digits: String = text
            .chars()
            .take_while(|character| character.is_ascii_digit() || *character == '_' || *character == '-')
            .filter(|character| *character != '_')
            .collect();
        digits.parse().unwrap_or_else(|error| panic!("{} constant {name} = {text}: {error}", self.name))
    }

    /// Integer constants whose name starts with `prefix`, as `(name, value)`.
    pub fn integer_constants(&self, prefix: &str) -> Vec<(String, i64)> {
        Query::from(&self.program.node)
            .of::<ConstantDefinition>()
            .filter(|constant| constant.name.node.name.starts_with(prefix))
            .filter(|constant| matches!(constant.value.node, Literal::Integer(_)))
            .map(|constant| (constant.name.node.name.clone(), self.constant(&constant.name.node.name)))
            .collect()
    }

    /// Variant names of the enum `name` in declaration order; panics when there is no such enum.
    pub fn enum_variants(&self, name: &str) -> Vec<String> {
        Query::from(&self.program.node)
            .of::<EnumDefinition>()
            .find(|definition| definition.name.node.name == name)
            .unwrap_or_else(|| panic!("{} defines no enum {name}", self.name))
            .variants
            .iter()
            .map(|variant| variant.node.name.node.name.clone())
            .collect()
    }

    /// Called names of every call in the source.
    pub fn callees(&self) -> BTreeSet<String> {
        Query::from(&self.program.node).of::<CallExpression>().filter_map(callee_name).map(str::to_owned).collect()
    }

    /// Argument lists of every call to `callee` in the source (see [`FunctionFacts::call_arguments`]).
    pub fn call_arguments(&self, callee: &str) -> Vec<Vec<String>> {
        call_arguments(Query::from(&self.program.node), &self.source, callee)
    }
}

fn call_arguments(scope: Query<'_>, source: &str, callee: &str) -> Vec<Vec<String>> {
    scope
        .of::<CallExpression>()
        .filter(|call| callee_name(call) == Some(callee))
        .map(|call| call.args.iter().map(|arg| compact(&source[arg.span.start..arg.span.end])).collect())
        .collect()
}

impl FunctionFacts<'_> {
    /// The function name.
    pub fn name(&self) -> &str {
        &self.definition.name.node.name
    }

    /// Whether the function is `pub`.
    pub fn is_public(&self) -> bool {
        matches!(self.definition.visibility.node, Visibility::Public)
    }

    /// Parameter types in order, each as its source text with whitespace removed.
    pub fn parameter_types(&self) -> Vec<String> {
        self.definition
            .parameters
            .iter()
            .map(|parameter| compact(&self.source[parameter.node.ty.span.start..parameter.node.ty.span.end]))
            .collect()
    }

    /// The return type as its source text with whitespace removed.
    pub fn return_type(&self) -> String {
        self.definition
            .return_type
            .as_ref()
            .map(|ty| compact(&self.source[ty.span.start..ty.span.end]))
            .unwrap_or_else(|| "unit".into())
    }

    /// Names of the attributes applied to the function.
    pub fn attribute_names(&self) -> BTreeSet<String> {
        self.definition.attributes.iter().map(|attribute| attribute.node.name.node.name.clone()).collect()
    }

    /// The value of argument `argument` of attribute `attribute`, as source text with whitespace
    /// removed and string quotes stripped (`[Export(Symbol: "x")]` -> `x`).
    pub fn attribute_argument(&self, attribute: &str, argument: &str) -> Option<String> {
        let attribute =
            self.definition.attributes.iter().find(|candidate| candidate.node.name.node.name == attribute)?;
        let value =
            &attribute.node.arguments.iter().find(|candidate| candidate.node.name.node.name == argument)?.node.value;
        let text = compact(&self.source[value.span.start..value.span.end]);
        Some(text.strip_prefix('"').and_then(|text| text.strip_suffix('"')).map(str::to_owned).unwrap_or(text))
    }

    /// `pub word Name(pointer from, pointer to)`: visibility, return type, name and parameters with
    /// whitespace inside each type removed.
    pub fn signature(&self) -> String {
        let definition = self.definition;
        let visibility = match definition.visibility.node {
            Visibility::Public => "pub ",
            Visibility::Private => "",
        };
        let return_type = self.return_type();
        let parameters = definition
            .parameters
            .iter()
            .map(|parameter| {
                format!(
                    "{} {}",
                    compact(&self.source[parameter.node.ty.span.start..parameter.node.ty.span.end]),
                    parameter.node.name.node.name
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{visibility}{return_type} {}({parameters})", definition.name.node.name)
    }

    /// Called names of every call in the function body.
    pub fn callees(&self) -> BTreeSet<String> {
        Query::from(&self.definition.body.node)
            .of::<CallExpression>()
            .filter_map(callee_name)
            .map(str::to_owned)
            .collect()
    }

    /// `(condition, returned value)` of every `if` in the body whose block starts with a `return`,
    /// in source order, each as its source text with whitespace removed.
    pub fn guarded_returns(&self) -> Vec<(String, String)> {
        Query::from(&self.definition.body.node)
            .of::<IfStatement>()
            .filter_map(|statement| {
                let Statement::Return(ret) = &statement.then_block.node.statements.first()?.node else { return None };
                let value = ret.node.value.as_ref()?;
                let condition = match &statement.condition.node {
                    Expression::Grouped(grouped) => &grouped.node.expr.span,
                    _ => &statement.condition.span,
                };
                Some((
                    compact(&self.source[condition.start..condition.end]),
                    compact(&self.source[value.span.start..value.span.end]),
                ))
            })
            .collect()
    }

    /// Argument lists of every call to `callee` in the body, each argument as its source text
    /// with whitespace removed.
    pub fn call_arguments(&self, callee: &str) -> Vec<Vec<String>> {
        call_arguments(Query::from(&self.definition.body.node), self.source, callee)
    }
}
