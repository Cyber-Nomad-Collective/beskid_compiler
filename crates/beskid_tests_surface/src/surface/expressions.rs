use crate::surface::util::{assert_parse, assert_parse_fail, parse_expression_ast};
use beskid_analysis::Rule;
use beskid_analysis::syntax::{AssignOp, BinaryOp, Expression, Literal, Spanned};

#[test]
fn parses_arithmetic_precedence() {
    let expr = parse_expression_ast("1 + 2 * 3");
    let (left, _op, right) = expect_binary(&expr.node, BinaryOp::Add);
    expect_integer_literal(&left.node, "1");
    let (mul_left, mul_op, mul_right) = expect_binary(&right.node, BinaryOp::Mul);
    expect_binary_op(mul_op, BinaryOp::Mul);
    expect_integer_literal(&mul_left.node, "2");
    expect_integer_literal(&mul_right.node, "3");
}

#[test]
fn parses_lambda_single_param_without_parens() {
    let expr = parse_expression_ast("x => x + 1");
    let (params, body) = expect_lambda(&expr.node, 1);
    assert_eq!(params[0].node.name.node.name, "x");
    assert!(params[0].node.ty.is_none());
    let (_left, op, right) = expect_binary(&body.node, BinaryOp::Add);
    expect_binary_op(op, BinaryOp::Add);
    expect_integer_literal(&right.node, "1");
}

#[test]
fn parses_lambda_typed_params_with_parens() {
    let expr = parse_expression_ast("(i64 x, i64 y) => x + y");
    let (params, _body) = expect_lambda(&expr.node, 2);
    assert_eq!(params[0].node.name.node.name, "x");
    assert_eq!(params[1].node.name.node.name, "y");
    assert!(params[0].node.ty.is_some());
    assert!(params[1].node.ty.is_some());
}

#[test]
fn parses_assignment_expression() {
    let expr = parse_expression_ast("x = y + 1");
    let (target, op, value) = expect_assign(&expr.node);
    assert_eq!(op.node, AssignOp::Assign);
    expect_identifier_path(&target.node, "x");
    let (_, op, right) = expect_binary(&value.node, BinaryOp::Add);
    expect_binary_op(op, BinaryOp::Add);
    expect_integer_literal(&right.node, "1");
}

#[test]
fn parses_compound_assignment_expression() {
    let expr = parse_expression_ast("x += 2");
    let (target, op, value) = expect_assign(&expr.node);
    assert_eq!(op.node, AssignOp::AddAssign);
    expect_identifier_path(&target.node, "x");
    expect_integer_literal(&value.node, "2");
}

#[test]
fn parses_every_compound_assignment_operator() {
    for (source, expected) in [
        ("x += 2", AssignOp::AddAssign),
        ("x -= 2", AssignOp::SubAssign),
        ("x *= 2", AssignOp::MulAssign),
        ("x /= 2", AssignOp::DivAssign),
        ("x %= 2", AssignOp::RemAssign),
    ] {
        let expr = parse_expression_ast(source);
        let (target, op, value) = expect_assign(&expr.node);
        assert_eq!(op.node, expected, "{source}");
        assert_eq!(&source[op.span.start..op.span.end], &source[2..4], "{source}");
        expect_identifier_path(&target.node, "x");
        expect_integer_literal(&value.node, "2");
    }
}

#[test]
fn parses_calls_and_member_access() {
    let expr = parse_expression_ast("foo.bar(1, 2)");
    let (callee, args) = expect_call(&expr.node, 2);
    expect_path_segments(&callee.node, &["foo", "bar"]);
    expect_integer_literal(&args[0].node, "1");
    expect_integer_literal(&args[1].node, "2");
}

#[test]
fn parses_postfix_try_expression() {
    let expr = parse_expression_ast("foo()?");
    let inner = expect_try(&expr.node);
    let (callee, args) = expect_call(&inner.node, 0);
    expect_identifier_path(&callee.node, "foo");
    assert!(args.is_empty());
}

/// The whole input is one expression (the expression rule alone accepts a prefix).
fn parse_whole_expression(input: &str) -> Spanned<Expression> {
    let expr = parse_expression_ast(input);
    assert_eq!(expr.span.end, input.len(), "{input:?} parsed only {:?}", &input[..expr.span.end]);
    expr
}

fn expect_conditional(expr: &Expression) -> (&Spanned<Expression>, &Spanned<Expression>, &Spanned<Expression>) {
    if let Expression::Conditional(conditional) = expr {
        return (&conditional.node.condition, &conditional.node.then_branch, &conditional.node.else_branch);
    }

    panic!("expected conditional expression, got {expr:?}");
}

#[test]
fn parses_conditional_expression() {
    let expr = parse_whole_expression("ready ? 1 : 2");
    let (condition, then_branch, else_branch) = expect_conditional(&expr.node);
    expect_identifier_path(&condition.node, "ready");
    expect_integer_literal(&then_branch.node, "1");
    expect_integer_literal(&else_branch.node, "2");
    assert_eq!((expr.span.start, expr.span.end), (0, 13));
}

#[test]
fn conditional_binds_looser_than_logical_or() {
    let expr = parse_whole_expression("a || b ? x + 1 : y * 2");
    let (condition, then_branch, else_branch) = expect_conditional(&expr.node);
    expect_binary(&condition.node, BinaryOp::Or);
    expect_binary(&then_branch.node, BinaryOp::Add);
    expect_binary(&else_branch.node, BinaryOp::Mul);
}

#[test]
fn conditional_binds_tighter_than_assignment() {
    let expr = parse_whole_expression("x = c ? a : b");
    let (target, op, value) = expect_assign(&expr.node);
    assert_eq!(op.node, AssignOp::Assign);
    expect_identifier_path(&target.node, "x");
    let (condition, _, _) = expect_conditional(&value.node);
    expect_identifier_path(&condition.node, "c");

    // The else branch is a conditional operand, not an assignment: `(c ? a : b) += 1`.
    let expr = parse_whole_expression("c ? a : b += 1");
    let (target, op, _) = expect_assign(&expr.node);
    assert_eq!(op.node, AssignOp::AddAssign);
    expect_conditional(&target.node);
}

#[test]
fn conditional_is_right_associative() {
    let expr = parse_whole_expression("a ? 1 : b ? 2 : 3");
    let (condition, then_branch, else_branch) = expect_conditional(&expr.node);
    expect_identifier_path(&condition.node, "a");
    expect_integer_literal(&then_branch.node, "1");
    let (inner_condition, inner_then, inner_else) = expect_conditional(&else_branch.node);
    expect_identifier_path(&inner_condition.node, "b");
    expect_integer_literal(&inner_then.node, "2");
    expect_integer_literal(&inner_else.node, "3");
}

#[test]
fn conditional_nests_in_the_then_branch() {
    let expr = parse_whole_expression("a ? b ? 1 : 2 : 3");
    let (_, then_branch, else_branch) = expect_conditional(&expr.node);
    let (inner_condition, _, _) = expect_conditional(&then_branch.node);
    expect_identifier_path(&inner_condition.node, "b");
    expect_integer_literal(&else_branch.node, "3");

    let expr = parse_whole_expression("Pick(c ? (d ? 1 : 2) : 3, [e ? 4 : 5])");
    let (_, args) = expect_call(&expr.node, 2);
    expect_conditional(&args[0].node);
}

#[test]
fn question_mark_with_a_matching_colon_is_a_conditional() {
    // `c ? -1 : 2`: a `:` completes the conditional, so `?` is not error propagation.
    let expr = parse_whole_expression("c ? -1 : 2");
    let (condition, then_branch, else_branch) = expect_conditional(&expr.node);
    expect_identifier_path(&condition.node, "c");
    let Expression::Unary(negated) = &then_branch.node else {
        panic!("expected `-1`, got {:?}", then_branch.node);
    };
    expect_integer_literal(&negated.node.expr.node, "1");
    expect_integer_literal(&else_branch.node, "2");
}

#[test]
fn question_mark_without_a_colon_propagates() {
    // `x? - 1`: no `:` follows, so `?` propagates and `-` is subtraction.
    let expr = parse_whole_expression("x? - 1");
    let (left, _, right) = expect_binary(&expr.node, BinaryOp::Sub);
    expect_identifier_path(&expect_try(&left.node).node, "x");
    expect_integer_literal(&right.node, "1");

    let expr = parse_whole_expression("F()?.Value");
    assert!(matches!(expr.node, Expression::Member(_)), "{:?}", expr.node);
    let expr = parse_whole_expression("F()?(1)");
    assert!(matches!(expr.node, Expression::Call(_)), "{:?}", expr.node);
}

#[test]
fn propagation_then_conditional() {
    // `F()? ? x : y`: the first `?` propagates, the second starts the conditional.
    let expr = parse_whole_expression("F()? ? x : y");
    let (condition, then_branch, else_branch) = expect_conditional(&expr.node);
    let (callee, _) = expect_call(&expect_try(&condition.node).node, 0);
    expect_identifier_path(&callee.node, "F");
    expect_identifier_path(&then_branch.node, "x");
    expect_identifier_path(&else_branch.node, "y");

    // Propagation inside a branch.
    let expr = parse_whole_expression("c ? F()? : G()?");
    let (_, then_branch, else_branch) = expect_conditional(&expr.node);
    expect_try(&then_branch.node);
    expect_try(&else_branch.node);
}

#[test]
fn conditional_parses_in_statements() {
    assert_parse(Rule::Statement, "return c ? 1 : 2;");
    assert_parse(Rule::Statement, "i32 x = c ? -1 : 2;");
    assert_parse(Rule::Statement, "F()?;");
    assert_parse(Rule::Statement, "if a ? b : c { }");
    assert_parse(Rule::Statement, "string s = \"${c ? 1 : 2}\";");
}

fn spanned_text(input: &str, span: beskid_analysis::syntax::SpanInfo) -> &str {
    &input[span.start..span.end]
}

/// The text a node covers, outer and inner span alike.
fn node_text<'a>(input: &'a str, expr: &Spanned<Expression>) -> &'a str {
    let outer = spanned_text(input, expr.span);
    let inner = match &expr.node {
        Expression::Binary(node) => node.span,
        Expression::Path(node) => node.span,
        Expression::Conditional(node) => node.span,
        // A call node spans its argument list, not the callee.
        Expression::Call(_) => return outer,
        other => panic!("unexpected node {other:?}"),
    };
    assert_eq!(outer, spanned_text(input, inner), "outer and node spans differ");
    outer
}

#[test]
fn binary_chain_span_ends_at_its_last_token() {
    let input = "a + b * c   // trailing\n";
    let expr = parse_expression_ast(input);
    assert_eq!(node_text(input, &expr), "a + b * c");
    let (left, _, right) = expect_binary(&expr.node, BinaryOp::Add);
    assert_eq!(node_text(input, left), "a");
    assert_eq!(node_text(input, right), "b * c");

    let input = "a || b /* note */ ";
    assert_eq!(node_text(input, &parse_expression_ast(input)), "a || b");
}

#[test]
fn single_operand_before_question_mark_ends_at_its_last_token() {
    let input = "c  ? a  : b ";
    let expr = parse_expression_ast(input);
    assert_eq!(node_text(input, &expr), "c  ? a  : b");
    let (condition, then_branch, else_branch) = expect_conditional(&expr.node);
    assert_eq!(node_text(input, condition), "c");
    assert_eq!(node_text(input, then_branch), "a");
    assert_eq!(node_text(input, else_branch), "b");

    // A comment holding `//` inside a string does not cut the operand short.
    let input = "F(\"x//y\") // done\n";
    assert_eq!(node_text(input, &parse_expression_ast(input)), "F(\"x//y\")");
}

#[test]
fn if_condition_span_ends_at_its_last_token() {
    let input = "if n  /* why */ { }";
    let statement = crate::surface::ast::parse_statement_ast(Rule::IfStatement, input);
    let beskid_analysis::syntax::Statement::If(if_statement) = &statement.node else {
        panic!("expected if statement, got {:?}", statement.node);
    };
    assert_eq!(node_text(input, &if_statement.node.condition), "n");
}

#[test]
fn parses_logical_binary_expression() {
    let expr = parse_expression_ast("true || false");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::Or);
    expect_binary_op(op, BinaryOp::Or);
    expect_bool_literal(&left.node, true);
    expect_bool_literal(&right.node, false);
}

#[test]
fn parses_equality_binary_expression() {
    let expr = parse_expression_ast("1 == 2");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::Eq);
    expect_binary_op(op, BinaryOp::Eq);
    expect_integer_literal(&left.node, "1");
    expect_integer_literal(&right.node, "2");
}

#[test]
fn parses_identity_equality_binary_expression() {
    let expr = parse_expression_ast("1 === 2");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::IdentityEq);
    expect_binary_op(op, BinaryOp::IdentityEq);
    expect_integer_literal(&left.node, "1");
    expect_integer_literal(&right.node, "2");
}

#[test]
fn parses_identity_inequality_binary_expression() {
    let expr = parse_expression_ast("1 !== 2");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::IdentityNotEq);
    expect_binary_op(op, BinaryOp::IdentityNotEq);
    expect_integer_literal(&left.node, "1");
    expect_integer_literal(&right.node, "2");
}

#[test]
fn parses_comparison_binary_expression() {
    let expr = parse_expression_ast("1 < 2");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::Lt);
    expect_binary_op(op, BinaryOp::Lt);
    expect_integer_literal(&left.node, "1");
    expect_integer_literal(&right.node, "2");
}

#[test]
fn parses_sub_and_div_binary_expression() {
    let expr = parse_expression_ast("4 - 2 / 1");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::Sub);
    expect_binary_op(op, BinaryOp::Sub);
    expect_integer_literal(&left.node, "4");
    let (div_left, div_op, div_right) = expect_binary(&right.node, BinaryOp::Div);
    expect_binary_op(div_op, BinaryOp::Div);
    expect_integer_literal(&div_left.node, "2");
    expect_integer_literal(&div_right.node, "1");
}

#[test]
fn parses_integer_shift_expressions_before_addition_and_bitwise_and() {
    let expr = parse_expression_ast("generation << 32 | slotIndex & 255");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::BitOr);
    expect_binary_op(op, BinaryOp::BitOr);
    let (shift_left, shift_op, shift_right) = expect_binary(&left.node, BinaryOp::Shl);
    expect_binary_op(shift_op, BinaryOp::Shl);
    expect_identifier_path(&shift_left.node, "generation");
    expect_integer_literal(&shift_right.node, "32");
    let (and_left, and_op, and_right) = expect_binary(&right.node, BinaryOp::BitAnd);
    expect_binary_op(and_op, BinaryOp::BitAnd);
    expect_identifier_path(&and_left.node, "slotIndex");
    expect_integer_literal(&and_right.node, "255");
}

#[test]
fn parses_logical_right_shift_expression() {
    let expr = parse_expression_ast("header >> 32");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::Shr);
    expect_binary_op(op, BinaryOp::Shr);
    expect_identifier_path(&left.node, "header");
    expect_integer_literal(&right.node, "32");
}

#[test]
fn parses_string_interpolation_as_concat_expression() {
    let expr = parse_expression_ast("\"hi ${name}\"");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::Add);
    expect_binary_op(op, BinaryOp::Add);
    expect_string_literal(&left.node, "\"hi \"");
    expect_identifier_path(&right.node, "name");
}

#[test]
fn parses_string_interpolation_with_full_expression() {
    let expr = parse_expression_ast("\"sum ${1 + 2}\"");
    let (left, op, right) = expect_binary(&expr.node, BinaryOp::Add);
    expect_binary_op(op, BinaryOp::Add);
    expect_string_literal(&left.node, "\"sum \"");

    let (expr_left, expr_op, expr_right) = expect_binary(&right.node, BinaryOp::Add);
    expect_binary_op(expr_op, BinaryOp::Add);
    expect_integer_literal(&expr_left.node, "1");
    expect_integer_literal(&expr_right.node, "2");
}

#[test]
fn rejects_empty_expression() {
    assert_parse_fail(Rule::Expression, "");
}

#[test]
fn rejects_expression_with_invalid_prefix() {
    assert_parse_fail(Rule::Expression, ".foo");
}

#[test]
fn parses_argument_list() {
    assert_parse(Rule::ArgumentList, "1, true, foo");
}

#[test]
fn rejects_argument_list_starting_with_comma() {
    assert_parse_fail(Rule::ArgumentList, ", 1");
}

fn expect_binary(expr: &Expression, op: BinaryOp) -> (&Spanned<Expression>, &Spanned<BinaryOp>, &Spanned<Expression>) {
    if let Expression::Binary(binary) = expr {
        assert_eq!(binary.node.op.node, op);
        return (&binary.node.left, &binary.node.op, &binary.node.right);
    }

    panic!("expected binary expression");
}

fn expect_binary_op(op: &Spanned<BinaryOp>, expected: BinaryOp) {
    assert_eq!(op.node, expected);
}

fn expect_assign(expr: &Expression) -> (&Spanned<Expression>, &Spanned<AssignOp>, &Spanned<Expression>) {
    if let Expression::Assign(assign) = expr {
        return (&assign.node.target, &assign.node.op, &assign.node.value);
    }

    panic!("expected assign expression");
}

fn expect_call(expr: &Expression, args_len: usize) -> (&Spanned<Expression>, &[Spanned<Expression>]) {
    if let Expression::Call(call) = expr {
        assert_eq!(call.node.args.len(), args_len);
        return (&call.node.callee, &call.node.args);
    }

    panic!("expected call expression");
}

fn expect_try(expr: &Expression) -> &Spanned<Expression> {
    if let Expression::Try(try_expr) = expr {
        return &try_expr.node.expr;
    }

    panic!("expected try expression");
}

fn expect_lambda(
    expr: &Expression,
    params_len: usize,
) -> (&[Spanned<beskid_analysis::syntax::LambdaParameter>], &Spanned<Expression>) {
    if let Expression::Lambda(lambda) = expr {
        assert_eq!(lambda.node.parameters.len(), params_len);
        return (&lambda.node.parameters, &lambda.node.body);
    }

    panic!("expected lambda expression");
}

fn expect_identifier_path(expr: &Expression, expected: &str) {
    if let Expression::Path(path) = expr {
        assert_eq!(path.node.path.node.segments.len(), 1);
        assert_eq!(path.node.path.node.segments[0].node.name.node.name.as_str(), expected);
        return;
    }

    panic!("expected path expression");
}

fn expect_path_segments(expr: &Expression, expected: &[&str]) {
    if let Expression::Path(path) = expr {
        assert_eq!(path.node.path.node.segments.len(), expected.len());
        for (segment, expected_name) in path.node.path.node.segments.iter().zip(expected.iter()) {
            assert_eq!(segment.node.name.node.name.as_str(), *expected_name);
        }
        return;
    }

    panic!("expected path expression");
}

fn expect_integer_literal(expr: &Expression, expected: &str) {
    if let Expression::Literal(literal) = expr
        && let Literal::Integer(value) = &literal.node.literal.node
    {
        assert_eq!(value, expected);
        return;
    }

    panic!("expected integer literal");
}

fn expect_bool_literal(expr: &Expression, expected: bool) {
    if let Expression::Literal(literal) = expr
        && let Literal::Bool(value) = &literal.node.literal.node
    {
        assert_eq!(*value, expected);
        return;
    }

    panic!("expected bool literal");
}

fn expect_string_literal(expr: &Expression, expected: &str) {
    if let Expression::Literal(literal) = expr
        && let Literal::String(value) = &literal.node.literal.node
    {
        assert_eq!(value, expected);
        return;
    }

    panic!("expected string literal");
}
