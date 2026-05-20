use pest::iterators::Pair;

use crate::parser::Rule;
use crate::parsing::error::ParseError;
use crate::parsing::parsable::Parsable;
use crate::syntax::items::InlineModule;
use crate::syntax::{
    AttributeDeclaration, ContractDefinition, EnumDefinition, ExtendTypeDefinition,
    FunctionDefinition, MacroDefinition, MethodDefinition, ModuleDeclaration, SpanInfo, Spanned,
    TestDefinition, TypeDefinition, UseDeclaration,
};

use beskid_ast_derive::AstNode;

/// Inner module item: function, type, enum, contract, test, module, use, etc.
#[derive(AstNode, Debug, Clone, PartialEq, Eq)]
pub enum Node {
    #[ast(child)]
    Function(Spanned<FunctionDefinition>),
    #[ast(child)]
    Method(Spanned<MethodDefinition>),
    #[ast(child)]
    ExtendTypeDefinition(Spanned<ExtendTypeDefinition>),
    #[ast(child)]
    MacroDefinition(Spanned<MacroDefinition>),
    #[ast(child)]
    TypeDefinition(Spanned<TypeDefinition>),
    #[ast(child)]
    EnumDefinition(Spanned<EnumDefinition>),
    #[ast(child)]
    ContractDefinition(Spanned<ContractDefinition>),
    #[ast(child)]
    TestDefinition(Spanned<TestDefinition>),
    #[ast(child)]
    AttributeDeclaration(Spanned<AttributeDeclaration>),
    #[ast(child)]
    ModuleDeclaration(Spanned<ModuleDeclaration>),
    #[ast(child)]
    InlineModule(Spanned<InlineModule>),
    #[ast(child)]
    UseDeclaration(Spanned<UseDeclaration>),
}

impl Parsable for Node {
    fn parse(pair: Pair<Rule>) -> Result<Spanned<Self>, ParseError> {
        parse_node(pair)
    }
}

fn parse_node(pair: Pair<Rule>) -> Result<Spanned<Node>, ParseError> {
    let span = SpanInfo::from_span(&pair.as_span());

    match pair.as_rule() {
        Rule::InnerItem => {
            let inner = pair
                .into_inner()
                .next()
                .ok_or(ParseError::missing(Rule::InnerItem))?;
            parse_node(inner)
        }
        Rule::FunctionDefinition => {
            let node = FunctionDefinition::parse(pair)?;
            Ok(Spanned::new(Node::Function(node), span))
        }
        Rule::TypeDefinition => {
            let node = TypeDefinition::parse(pair)?;
            Ok(Spanned::new(Node::TypeDefinition(node), span))
        }
        Rule::ExtendTypeDefinition => {
            let node = ExtendTypeDefinition::parse(pair)?;
            Ok(Spanned::new(Node::ExtendTypeDefinition(node), span))
        }
        Rule::MacroDefinition => {
            let node = MacroDefinition::parse(pair)?;
            Ok(Spanned::new(Node::MacroDefinition(node), span))
        }
        Rule::EnumDefinition => {
            let node = EnumDefinition::parse(pair)?;
            Ok(Spanned::new(Node::EnumDefinition(node), span))
        }
        Rule::ContractDefinition => {
            let node = ContractDefinition::parse(pair)?;
            Ok(Spanned::new(Node::ContractDefinition(node), span))
        }
        Rule::TestDefinition => {
            let node = TestDefinition::parse(pair)?;
            Ok(Spanned::new(Node::TestDefinition(node), span))
        }
        Rule::AttributeDeclaration => {
            let node = AttributeDeclaration::parse(pair)?;
            Ok(Spanned::new(Node::AttributeDeclaration(node), span))
        }
        Rule::ModuleDeclaration => {
            let node = ModuleDeclaration::parse(pair)?;
            Ok(Spanned::new(Node::ModuleDeclaration(node), span))
        }
        Rule::InlineModule => {
            let node = InlineModule::parse(pair)?;
            Ok(Spanned::new(Node::InlineModule(node), span))
        }
        Rule::UseDeclaration => {
            let node = UseDeclaration::parse(pair)?;
            Ok(Spanned::new(Node::UseDeclaration(node), span))
        }
        _ => Err(ParseError::unexpected_rule(pair, Some(Rule::InnerItem))),
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::{BeskidParser, Rule};
    use crate::parsing::parsable::Parsable;
    use crate::{format::format_program, hir::lower_program, syntax::Program};
    use pest::Parser;

    #[test]
    fn module_level_meta_definition_is_not_a_valid_inner_item() {
        let src = r#"meta demo { flag = true; }"#;
        assert!(
            BeskidParser::parse(Rule::Program, src).is_err(),
            "module-level `meta` blocks were removed; use Mod projects and compiler mods instead"
        );
    }

    #[test]
    fn extend_type_parses_lowers_and_formats() {
        let src = r#"
            type Account { i64 balance }
            extend type Account {
                pub unit Deposit(i64 amount) {
                    return;
                }
                pub i64 Balance() {
                    return 0;
                }
            }
        "#;
        let pair = BeskidParser::parse(Rule::Program, src)
            .expect("extend type should parse")
            .next()
            .expect("program pair");
        let program = Program::parse(pair).expect("extend type should build AST");

        let hir = lower_program(&program.clone().into());
        assert_eq!(
            hir.node.items.len(),
            2,
            "lowering should preserve the extend-type block as one top-level HIR item"
        );

        let formatted = format_program(&program).expect("extend type should format");
        assert!(formatted.contains("extend type Account"));
        assert!(formatted.contains("pub unit Deposit(i64 amount)"));
        assert!(formatted.contains("pub i64 Balance()"));
    }
}
