use beskid_analysis::hir::{
    AstItem, AstProgram, HirAssignOp, HirBinaryOp, HirExpressionNode, HirFieldKind, HirItem,
    HirPattern, HirProgram, HirStatementNode, lower_program, normalize_program,
};
use beskid_analysis::syntax::Spanned;

use crate::syntax::util::parse_program_ast;

fn sample_source() -> &'static str {
    "enum Option { Some(i64 value), None }\n\
     type User { i64 id, string name }\n\
     unit main() {\n\
       User u = User { id: 1, name: \"a\" };\n\
       i64 x = u.id;\n\
       Option y = Option::Some(1);\n\
       i64 z = match y { Option::Some(v) => v, Option::None => 0, };\n\
       if true { return; }\n\
     }"
}

fn lower_sample_program() -> (Spanned<AstProgram>, Spanned<HirProgram>) {
    let program = parse_program_ast(sample_source());
    let ast: Spanned<AstProgram> = program.into();
    let mut hir: Spanned<HirProgram> = lower_program(&ast);
    normalize_program(&mut hir).expect("normalization failed");
    (ast, hir)
}

#[test]
fn lowering_preserves_item_spans_and_names() {
    let (ast, hir) = lower_sample_program();

    assert_eq!(ast.node.items.len(), hir.node.items.len());

    for (ast_item, hir_item) in ast.node.items.iter().zip(hir.node.items.iter()) {
        assert_eq!(ast_item.span, hir_item.span);
        match (&ast_item.node, &hir_item.node) {
            (AstItem::EnumDefinition(ast_def), HirItem::EnumDefinition(hir_def)) => {
                assert_eq!(ast_def.node.name.node.name, hir_def.node.name.node.name);
            }
            (AstItem::TypeDefinition(ast_def), HirItem::TypeDefinition(hir_def)) => {
                assert_eq!(ast_def.node.name.node.name, hir_def.node.name.node.name);
            }
            (AstItem::FunctionDefinition(ast_def), HirItem::FunctionDefinition(hir_def)) => {
                assert_eq!(ast_def.node.name.node.name, hir_def.node.name.node.name);
            }
            _ => panic!("unexpected item pairing in lowering test"),
        }
    }
}

#[test]
fn lowering_maps_statement_and_expression_kinds() {
    let (_ast, hir) = lower_sample_program();
    let main_fn = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::FunctionDefinition(def) if def.node.name.node.name == "main" => Some(def),
            _ => None,
        })
        .expect("expected main function");

    let statements = &main_fn.node.body.node.statements;
    assert!(
        statements
            .iter()
            .any(|statement| { matches!(statement.node, HirStatementNode::IfStatement(_)) })
    );

    let mut saw_struct_literal = false;
    let mut saw_member = false;
    let mut saw_enum_constructor = false;
    let mut saw_match = false;

    for statement in statements {
        let HirStatementNode::LetStatement(let_stmt) = &statement.node else {
            continue;
        };
        match &let_stmt.node.value.node {
            HirExpressionNode::StructLiteralExpression(_) => saw_struct_literal = true,
            HirExpressionNode::MemberExpression(_) | HirExpressionNode::PathExpression(_) => {
                saw_member = true
            }
            HirExpressionNode::EnumConstructorExpression(_) => saw_enum_constructor = true,
            HirExpressionNode::MatchExpression(_) => saw_match = true,
            _ => {}
        }
    }

    assert!(saw_struct_literal, "expected struct literal let binding");
    assert!(saw_member, "expected member access let binding");
    assert!(
        saw_enum_constructor,
        "expected enum constructor let binding"
    );
    assert!(saw_match, "expected match let binding");
}

#[test]
fn lowering_preserves_match_patterns() {
    let (_ast, hir) = lower_sample_program();
    let main_fn = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::FunctionDefinition(def) if def.node.name.node.name == "main" => Some(def),
            _ => None,
        })
        .expect("expected main function");

    let HirStatementNode::LetStatement(match_let) = &main_fn.node.body.node.statements[3].node
    else {
        panic!("expected match let statement");
    };
    let HirExpressionNode::MatchExpression(match_expr) = &match_let.node.value.node else {
        panic!("expected match expression");
    };

    assert_eq!(match_expr.node.arms.len(), 2);
    let first_pattern = &match_expr.node.arms[0].node.pattern;
    assert!(matches!(first_pattern.node, HirPattern::Enum(_)));
}

#[test]
fn lowering_collects_extern_interface_metadata() {
    let source = "[Extern(Abi: \"C\", Library: \"libc\")] contract Reader { i32 read(u8[] p); } [Extern(Abi: \"C\", Library: \"libc\")] mod sys.io;";
    let program = parse_program_ast(source);
    let ast: Spanned<AstProgram> = program.into();
    let hir: Spanned<HirProgram> = lower_program(&ast);

    let contract = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::ContractDefinition(def) => Some(def),
            _ => None,
        })
        .expect("expected contract definition");
    let module = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::ModuleDeclaration(def) => Some(def),
            _ => None,
        })
        .expect("expected module declaration");

    let contract_extern = contract
        .node
        .extern_interface
        .as_ref()
        .expect("contract extern metadata");
    assert_eq!(contract_extern.abi.as_deref(), Some("C"));
    assert_eq!(contract_extern.library.as_deref(), Some("libc"));

    let module_extern = module
        .node
        .extern_interface
        .as_ref()
        .expect("module extern metadata");
    assert_eq!(module_extern.abi.as_deref(), Some("C"));
    assert_eq!(module_extern.library.as_deref(), Some("libc"));
}

#[test]
fn lowering_preserves_attribute_declaration_items() {
    let source = "pub attribute Extern(ModuleDeclaration, ContractDeclaration) { Abi: string = \"C\", Library: string = \"libc\" }";
    let program = parse_program_ast(source);
    let ast: Spanned<AstProgram> = program.into();
    let hir: Spanned<HirProgram> = lower_program(&ast);

    let declaration = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::AttributeDeclaration(def) => Some(def),
            _ => None,
        })
        .expect("expected attribute declaration");

    assert_eq!(declaration.node.name.node.name, "Extern");
    assert_eq!(declaration.node.targets.len(), 2);
    assert_eq!(declaration.node.targets[0].node.name.node.name, "ModuleDeclaration");
    assert_eq!(declaration.node.targets[1].node.name.node.name, "ContractDeclaration");
    assert_eq!(declaration.node.parameters.len(), 2);
    assert_eq!(declaration.node.parameters[0].node.name.node.name, "Abi");
    assert_eq!(declaration.node.parameters[1].node.name.node.name, "Library");
}

#[test]
fn analysis_desugars_try_to_match() {
    let source = "i64 foo() { return 1; } i64 main() { i64 value = foo()?; return value; }";
    let program = parse_program_ast(source);
    let ast: Spanned<AstProgram> = program.into();
    let mut hir: Spanned<HirProgram> = lower_program(&ast);
    normalize_program(&mut hir).expect("normalization failed");

    let main_fn = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::FunctionDefinition(def) if def.node.name.node.name == "main" => Some(def),
            _ => None,
        })
        .expect("expected main function");

    let HirStatementNode::LetStatement(let_stmt) = &main_fn.node.body.node.statements[0].node
    else {
        panic!("expected let statement");
    };

    let HirExpressionNode::MatchExpression(match_expr) = &let_stmt.node.value.node else {
        panic!("expected desugared match expression");
    };
    assert!(
        matches!(match_expr.node.scrutinee.node, HirExpressionNode::CallExpression(_)),
        "expected try scrutinee call to become match scrutinee"
    );
    assert_eq!(match_expr.node.arms.len(), 2, "expected Ok and wildcard arms");
    assert!(
        matches!(match_expr.node.arms[0].node.pattern.node, HirPattern::Enum(_)),
        "expected first arm to pattern-match Result::Ok"
    );
    assert!(
        matches!(match_expr.node.arms[1].node.pattern.node, HirPattern::Wildcard),
        "expected second arm to be wildcard error arm"
    );
}

#[test]
fn lowering_normalizes_iterable_for_statement_to_state_machine() {
    let source = "unit main() { for item in items { continue; } }";
    let program = parse_program_ast(source);
    let ast: Spanned<AstProgram> = program.into();
    let mut hir: Spanned<HirProgram> = lower_program(&ast);
    normalize_program(&mut hir).expect("normalization failed");

    let main_fn = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::FunctionDefinition(def) if def.node.name.node.name == "main" => Some(def),
            _ => None,
        })
        .expect("expected main function");

    assert!(matches!(main_fn.node.body.node.statements[0].node, HirStatementNode::LetStatement(_)));
    let HirStatementNode::WhileStatement(while_stmt) = &main_fn.node.body.node.statements[1].node else {
        panic!("expected while statement");
    };
    assert_eq!(while_stmt.node.body.node.statements.len(), 2);
    assert!(matches!(while_stmt.node.body.node.statements[0].node, HirStatementNode::LetStatement(_)));
    assert!(matches!(while_stmt.node.body.node.statements[1].node, HirStatementNode::ExpressionStatement(_)));
}

#[test]
fn lowering_normalizes_range_for_statement_to_fast_path() {
    let source = "unit main() { for i in range(0, 3) { continue; } }";
    let program = parse_program_ast(source);
    let ast: Spanned<AstProgram> = program.into();
    let mut hir: Spanned<HirProgram> = lower_program(&ast);
    normalize_program(&mut hir).expect("normalization failed");

    let main_fn = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::FunctionDefinition(def) if def.node.name.node.name == "main" => Some(def),
            _ => None,
        })
        .expect("expected main function");

    assert!(matches!(main_fn.node.body.node.statements[0].node, HirStatementNode::LetStatement(_)));
    assert!(matches!(main_fn.node.body.node.statements[1].node, HirStatementNode::LetStatement(_)));
    let HirStatementNode::WhileStatement(while_stmt) = &main_fn.node.body.node.statements[2].node else {
        panic!("expected while statement");
    };
    assert!(matches!(while_stmt.node.condition.node, HirExpressionNode::BinaryExpression(_)));
}

#[test]
fn lowering_preserves_type_conformances_and_field_kinds() {
    let source = "type User : Display, Clone { event{4} Created(string payload), string name }";
    let program = parse_program_ast(source);
    let ast: Spanned<AstProgram> = program.into();
    let hir: Spanned<HirProgram> = lower_program(&ast);

    let type_def = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::TypeDefinition(def) => Some(def),
            _ => None,
        })
        .expect("expected type definition");

    assert_eq!(type_def.node.conformances.len(), 2);
    assert_eq!(type_def.node.fields.len(), 2);
    assert_eq!(type_def.node.fields[0].node.kind, HirFieldKind::Event);
    assert_eq!(type_def.node.fields[0].node.event_capacity, Some(4));
    assert_eq!(type_def.node.fields[1].node.kind, HirFieldKind::Value);
}

#[test]
fn lowering_maps_identity_binary_and_assign_ops() {
    let source = "unit main() { i64 x = 1; bool same = x === 1; x += 2; }";
    let program = parse_program_ast(source);
    let ast: Spanned<AstProgram> = program.into();
    let mut hir: Spanned<HirProgram> = lower_program(&ast);
    normalize_program(&mut hir).expect("normalization failed");

    let main_fn = hir
        .node
        .items
        .iter()
        .find_map(|item| match &item.node {
            HirItem::FunctionDefinition(def) if def.node.name.node.name == "main" => Some(def),
            _ => None,
        })
        .expect("expected main function");

    let HirStatementNode::LetStatement(let_same) = &main_fn.node.body.node.statements[1].node else {
        panic!("expected let statement");
    };
    match &let_same.node.value.node {
        HirExpressionNode::BinaryExpression(binary) => {
            assert_eq!(binary.node.op.node, HirBinaryOp::IdentityEq);
        }
        _ => panic!("expected binary expression"),
    }

    let HirStatementNode::ExpressionStatement(expr_stmt) = &main_fn.node.body.node.statements[2].node else {
        panic!("expected expression statement");
    };
    match &expr_stmt.node.expression.node {
        HirExpressionNode::AssignExpression(assign) => {
            assert_eq!(assign.node.op.node, HirAssignOp::AddAssign);
        }
        _ => panic!("expected assign expression"),
    }
}
