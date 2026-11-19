use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::{
    BreakStatement, ContinueStatement, ExpressionStatement, ForStatement, IfStatement, LaunchStatement, LetStatement,
    ObserveStatement, ReturnStatement, ScopedUseStatement, SpanInfo, Spanned, WhileStatement, WithStatement,
};

use beskid_ast_derive::AstNode;

/// Executable statement inside a block (not a top-level item).
#[derive(AstNode, Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Statement {
    #[ast(child)]
    Use(Spanned<ScopedUseStatement>),
    #[ast(child)]
    Let(Spanned<LetStatement>),
    #[ast(child)]
    Return(Spanned<ReturnStatement>),
    #[ast(child)]
    Break(Spanned<BreakStatement>),
    #[ast(child)]
    Continue(Spanned<ContinueStatement>),
    #[ast(child)]
    While(Spanned<WhileStatement>),
    #[ast(child)]
    For(Spanned<ForStatement>),
    #[ast(child)]
    If(Spanned<IfStatement>),
    #[ast(child)]
    With(Spanned<WithStatement>),
    #[ast(child)]
    Launch(Spanned<LaunchStatement>),
    #[ast(child)]
    Expression(Spanned<ExpressionStatement>),
    #[ast(child)]
    Observe(Spanned<ObserveStatement>),
}

impl Parsable for Statement {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        parse_statement(pair)
    }
}

fn parse_statement(pair: Pair<Rule>) -> Result<Spanned<Statement>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());

    match pair.as_rule() {
        Rule::Statement => {
            let inner = pair.into_inner().next().ok_or(ParseError::missing(Rule::Statement))?;
            parse_statement(inner)
        }
        Rule::ScopedUseStatement => {
            let statement = ScopedUseStatement::parse(pair)?;
            Ok(Spanned::new(Statement::Use(statement), span))
        }
        Rule::LetStatement => {
            let statement = LetStatement::parse(pair)?;
            Ok(Spanned::new(Statement::Let(statement), span))
        }
        Rule::ReturnStatement => {
            let statement = ReturnStatement::parse(pair)?;
            Ok(Spanned::new(Statement::Return(statement), span))
        }
        Rule::BreakStatement => {
            let statement = BreakStatement::parse(pair)?;
            Ok(Spanned::new(Statement::Break(statement), span))
        }
        Rule::ContinueStatement => {
            let statement = ContinueStatement::parse(pair)?;
            Ok(Spanned::new(Statement::Continue(statement), span))
        }
        Rule::WhileStatement => {
            let statement = WhileStatement::parse(pair)?;
            Ok(Spanned::new(Statement::While(statement), span))
        }
        Rule::ForStatement => {
            let statement = ForStatement::parse(pair)?;
            Ok(Spanned::new(Statement::For(statement), span))
        }
        Rule::IfStatement => {
            let statement = IfStatement::parse(pair)?;
            Ok(Spanned::new(Statement::If(statement), span))
        }
        Rule::WithStatement => {
            let statement = WithStatement::parse(pair)?;
            Ok(Spanned::new(Statement::With(statement), span))
        }
        Rule::LaunchStatement => {
            let statement = LaunchStatement::parse(pair)?;
            Ok(Spanned::new(Statement::Launch(statement), span))
        }
        Rule::ExpressionStatement => {
            let statement = ExpressionStatement::parse(pair)?;
            Ok(Spanned::new(Statement::Expression(statement), span))
        }
        Rule::ObserveStatement => {
            let statement = ObserveStatement::parse(pair)?;
            Ok(Spanned::new(Statement::Observe(statement), span))
        }
        _ => Err(ParseError::unexpected_rule(pair, Some(Rule::Statement))),
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::{BeskidParser, Rule};
    use crate::parsing::parsable::Parsable;
    use crate::syntax::{Expression, Statement};
    use pest::Parser;

    #[test]
    fn with_statement_parses() {
        let pair = BeskidParser::parse(Rule::Statement, "with HttpScope(request) { return; }")
            .expect("with should parse")
            .next()
            .expect("statement pair");
        let statement = Statement::parse(pair).expect("statement parse");
        assert!(matches!(statement.node, Statement::With(_)));
    }

    #[test]
    fn launch_statement_parses() {
        let pair = BeskidParser::parse(Rule::Statement, "launch AppHost(args);")
            .expect("launch should parse")
            .next()
            .expect("statement pair");
        let statement = Statement::parse(pair).expect("statement parse");
        assert!(matches!(statement.node, Statement::Launch(_)));
    }

    #[test]
    fn observe_preserves_the_receiver_chain_and_lambda_bindings() {
        let source = "observe Holder().Counters[0].Count (old, new) => { Use(old, new); };";
        let pair = BeskidParser::parse(Rule::Statement, source).unwrap().next().unwrap();
        let statement = Statement::parse(pair).unwrap();
        let Statement::Observe(observe) = statement.node else { panic!("expected observation") };
        let Expression::Member(member) = &observe.node.target.node else { panic!("expected property projection") };
        assert_eq!(member.node.member.node.name, "Count");
        assert!(matches!(member.node.target.node, Expression::Index(_)));
        let Expression::Lambda(lambda) = &observe.node.callback.node else { panic!("expected callback") };
        let names = lambda.node.parameters.iter().map(|parameter| parameter.node.name.node.name.as_str());
        assert!(names.eq(["old", "new"]));
        assert_eq!(&source[observe.node.target.span.start..observe.node.target.span.end], "Holder().Counters[0].Count");
        assert_eq!(statement.span.end, source.len());
    }

    #[test]
    fn observe_is_contextual_and_requires_two_bindings_and_a_block() {
        for source in ["observe();", "observer();"] {
            let pair = BeskidParser::parse(Rule::Statement, source).unwrap().next().unwrap();
            assert!(matches!(Statement::parse(pair).unwrap().node, Statement::Expression(_)));
        }
        for source in [
            "observe counter.Count (old) => {};",
            "observe counter.Count (old, new, extra) => {};",
            "observe counter.Count (old, new) => Handle(old, new);",
            "observe counter (old, new) => {};",
        ] {
            let parsed = BeskidParser::parse(Rule::Statement, source)
                .ok()
                .and_then(|mut pairs| pairs.next())
                .and_then(|pair| Statement::parse(pair).ok());
            assert!(parsed.is_none(), "accepted invalid observation: {source}");
        }
    }

    #[test]
    fn observe_projects_the_final_segment_of_a_dotted_path() {
        let source = "observe state.counter.Count (old, new) => {};";
        let pair = BeskidParser::parse(Rule::Statement, source).unwrap().next().unwrap();
        let Statement::Observe(observe) = Statement::parse(pair).unwrap().node else { panic!("observation expected") };
        let Expression::Member(member) = observe.node.target.node else { panic!("member expected") };
        let Expression::Path(path) = member.node.target.node else { panic!("path expected") };
        assert_eq!(member.node.member.node.name, "Count");
        assert_eq!(&source[path.span.start..path.span.end], "state.counter");
        assert_eq!(path.node.path.node.segments.len(), 2);
    }
}
