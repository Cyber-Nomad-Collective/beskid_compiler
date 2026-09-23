/// A parsed grammar rule (minimal Pest subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarRule {
    pub name: String,
    pub expression: String,
}

/// Parse simple `name = { expr }` rules from `.pest` source (one rule per line block).
pub fn parse_grammar_rules(source: &str) -> Result<Vec<GrammarRule>, String> {
    let mut rules = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_expr = String::new();

    for line in source.lines() {
        let trimmed = line.split("//").next().unwrap_or("").trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((name, expr)) = trimmed.split_once('=') {
            if let Some(prev) = current_name.take() {
                rules.push(GrammarRule { name: prev, expression: current_expr.trim().to_string() });
                current_expr.clear();
            }
            current_name = Some(name.trim().to_string());
            let expr_part = expr.trim().trim_start_matches('{').trim();
            if expr_part.ends_with('}') {
                rules.push(GrammarRule {
                    name: name.trim().to_string(),
                    expression: expr_part.trim_end_matches('}').trim().to_string(),
                });
                current_name = None;
                current_expr.clear();
            } else {
                current_expr.push_str(expr_part);
            }
        } else if current_name.is_some() {
            let part = trimmed.trim_end_matches('}').trim();
            if !current_expr.is_empty() {
                current_expr.push(' ');
            }
            current_expr.push_str(part);
            if trimmed.ends_with('}')
                && let Some(prev) = current_name.take()
            {
                rules.push(GrammarRule { name: prev, expression: current_expr.trim().to_string() });
                current_expr.clear();
            }
        }
    }
    if let Some(prev) = current_name {
        rules.push(GrammarRule { name: prev, expression: current_expr.trim().to_string() });
    }
    if rules.is_empty() {
        return Err("no grammar rules found".to_string());
    }
    Ok(rules)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expr {
    Literal(String),
    RuleRef(String),
    Any,
    Seq(Vec<Expr>),
    Choice(Vec<Expr>),
    Repeat(Box<Expr>, RepeatKind),
    Opt(Box<Expr>),
    Not(Box<Expr>),
    Group(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatKind {
    ZeroOrMore,
    OneOrMore,
}
