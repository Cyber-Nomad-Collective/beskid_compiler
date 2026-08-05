//! Naming-role identifier sites in a [`Program`] syntax tree (style lint + formatter).

use crate::naming_case::NamingProfile;
use crate::syntax::ContractNode;
use crate::syntax::{
    Block, ContractDefinition, EnumDefinition, EnumVariant, Expression, ExtendTypeDefinition, Field,
    FunctionDefinition, InlineModule, MethodDefinition, Node, Parameter, Pattern, Program, Statement, TestDefinition,
    TypeDefinition,
};
use crate::syntax::{Identifier, Spanned};

/// Syntactic role that carries a case profile from code-style-and-naming.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamingRole {
    TypeDeclaration,
    EnumVariant,
    Field,
    Callable,
    ModuleSegment,
    GenericParameter,
    Binding,
    Test,
    Macro,
}

impl NamingRole {
    pub(crate) fn profile(self) -> NamingProfile {
        match self {
            Self::TypeDeclaration
            | Self::EnumVariant
            | Self::Callable
            | Self::ModuleSegment
            | Self::GenericParameter => NamingProfile::PascalCase,
            Self::Field | Self::Binding | Self::Macro => NamingProfile::LowerCamelCase,
            Self::Test => NamingProfile::SnakeCase,
        }
    }
}

pub fn walk_program(program: &Program, mut visit: impl FnMut(NamingRole, &Spanned<Identifier>)) {
    for item in &program.items {
        walk_node(&item.node, &mut visit);
    }
}

pub fn walk_program_mut(program: &mut Program, mut visit: impl FnMut(NamingRole, &mut Identifier)) {
    for item in &mut program.items {
        walk_node_mut(&mut item.node, &mut visit);
    }
}

fn walk_node(node: &Node, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    match node {
        Node::TypeDefinition(def) => walk_type_definition(&def.node, visit),
        Node::EnumDefinition(def) => walk_enum_definition(&def.node, visit),
        Node::ContractDefinition(def) => walk_contract_definition(&def.node, visit),
        Node::Function(def) => walk_function_definition(&def.node, visit),
        // Constants retain their source spelling; canonical runtime layouts use established
        // uppercase ABI names which are intentionally outside local-binding style rules.
        Node::ConstantDefinition(_) => {}
        Node::Method(def) => walk_method_definition(&def.node, visit),
        Node::ExtendTypeDefinition(def) => walk_extend_type(&def.node, visit),
        Node::MacroDefinition(def) => {
            visit(NamingRole::Macro, &def.node.name);
        }
        Node::TestDefinition(def) => walk_test_definition(&def.node, visit),
        Node::ModuleDeclaration(def) => walk_module_path(&def.node.path, visit),
        Node::InlineModule(def) => walk_inline_module(&def.node, visit),
        Node::UseDeclaration(def) => walk_module_path(&def.node.path, visit),
        Node::HostDefinition(_) | Node::AttributeDeclaration(_) => {}
    }
}

fn walk_node_mut(node: &mut Node, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    match node {
        Node::TypeDefinition(def) => walk_type_definition_mut(&mut def.node, visit),
        Node::EnumDefinition(def) => walk_enum_definition_mut(&mut def.node, visit),
        Node::ContractDefinition(def) => walk_contract_definition_mut(&mut def.node, visit),
        Node::Function(def) => walk_function_definition_mut(&mut def.node, visit),
        Node::ConstantDefinition(_) => {}
        Node::Method(def) => walk_method_definition_mut(&mut def.node, visit),
        Node::ExtendTypeDefinition(def) => walk_extend_type_mut(&mut def.node, visit),
        Node::MacroDefinition(def) => visit(NamingRole::Macro, &mut def.node.name.node),
        Node::TestDefinition(def) => walk_test_definition_mut(&mut def.node, visit),
        Node::ModuleDeclaration(def) => walk_module_path_mut(&mut def.node.path, visit),
        Node::InlineModule(def) => walk_inline_module_mut(&mut def.node, visit),
        Node::UseDeclaration(def) => walk_module_path_mut(&mut def.node.path, visit),
        Node::HostDefinition(_) | Node::AttributeDeclaration(_) => {}
    }
}

fn walk_type_definition(def: &TypeDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::TypeDeclaration, &def.name);
    for generic in &def.generics {
        visit(NamingRole::GenericParameter, generic);
    }
    for field in &def.fields {
        walk_field(&field.node, visit);
    }
    for method in &def.methods {
        walk_method_definition(&method.node, visit);
    }
}

fn walk_type_definition_mut(def: &mut TypeDefinition, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::TypeDeclaration, &mut def.name.node);
    for generic in &mut def.generics {
        visit(NamingRole::GenericParameter, &mut generic.node);
    }
    for field in &mut def.fields {
        walk_field_mut(&mut field.node, visit);
    }
    for method in &mut def.methods {
        walk_method_definition_mut(&mut method.node, visit);
    }
}

fn walk_enum_definition(def: &EnumDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::TypeDeclaration, &def.name);
    for generic in &def.generics {
        visit(NamingRole::GenericParameter, generic);
    }
    for variant in &def.variants {
        walk_enum_variant(&variant.node, visit);
    }
}

fn walk_enum_definition_mut(def: &mut EnumDefinition, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::TypeDeclaration, &mut def.name.node);
    for generic in &mut def.generics {
        visit(NamingRole::GenericParameter, &mut generic.node);
    }
    for variant in &mut def.variants {
        walk_enum_variant_mut(&mut variant.node, visit);
    }
}

fn walk_enum_variant(variant: &EnumVariant, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::EnumVariant, &variant.name);
    for field in &variant.fields {
        walk_field(&field.node, visit);
    }
}

fn walk_enum_variant_mut(variant: &mut EnumVariant, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::EnumVariant, &mut variant.name.node);
    for field in &mut variant.fields {
        walk_field_mut(&mut field.node, visit);
    }
}

fn walk_contract_definition(def: &ContractDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::TypeDeclaration, &def.name);
    for item in &def.items {
        if let ContractNode::MethodSignature(sig) = &item.node {
            visit(NamingRole::Callable, &sig.node.name);
            for param in &sig.node.parameters {
                walk_parameter(&param.node, visit);
            }
        }
    }
}

fn walk_contract_definition_mut(def: &mut ContractDefinition, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::TypeDeclaration, &mut def.name.node);
    for item in &mut def.items {
        if let ContractNode::MethodSignature(sig) = &mut item.node {
            visit(NamingRole::Callable, &mut sig.node.name.node);
            for param in &mut sig.node.parameters {
                walk_parameter_mut(&mut param.node, visit);
            }
        }
    }
}

fn walk_function_definition(def: &FunctionDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::Callable, &def.name);
    for generic in &def.generics {
        visit(NamingRole::GenericParameter, generic);
    }
    for param in &def.parameters {
        walk_parameter(&param.node, visit);
    }
    walk_block(&def.body.node, visit);
}

fn walk_function_definition_mut(def: &mut FunctionDefinition, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::Callable, &mut def.name.node);
    for generic in &mut def.generics {
        visit(NamingRole::GenericParameter, &mut generic.node);
    }
    for param in &mut def.parameters {
        walk_parameter_mut(&mut param.node, visit);
    }
    walk_block_mut(&mut def.body.node, visit);
}

fn walk_method_definition(def: &MethodDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::Callable, &def.name);
    for param in &def.parameters {
        walk_parameter(&param.node, visit);
    }
    walk_block(&def.body.node, visit);
}

fn walk_method_definition_mut(def: &mut MethodDefinition, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::Callable, &mut def.name.node);
    for param in &mut def.parameters {
        walk_parameter_mut(&mut param.node, visit);
    }
    walk_block_mut(&mut def.body.node, visit);
}

fn walk_extend_type(def: &ExtendTypeDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    for method in &def.methods {
        walk_method_definition(&method.node, visit);
    }
}

fn walk_extend_type_mut(def: &mut ExtendTypeDefinition, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    for method in &mut def.methods {
        walk_method_definition_mut(&mut method.node, visit);
    }
}

fn walk_test_definition(def: &TestDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::Test, &def.name);
    for stmt in &def.statements {
        walk_statement(&stmt.node, visit);
    }
}

fn walk_test_definition_mut(def: &mut TestDefinition, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::Test, &mut def.name.node);
    for stmt in &mut def.statements {
        walk_statement_mut(&mut stmt.node, visit);
    }
}

fn walk_inline_module(module: &InlineModule, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::ModuleSegment, &module.name);
    for item in &module.items {
        walk_node(&item.node, visit);
    }
}

fn walk_inline_module_mut(module: &mut InlineModule, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::ModuleSegment, &mut module.name.node);
    for item in &mut module.items {
        walk_node_mut(&mut item.node, visit);
    }
}

fn walk_module_path(path: &Spanned<crate::syntax::Path>, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    for segment in &path.node.segments {
        visit(NamingRole::ModuleSegment, &segment.node.name);
    }
}

fn walk_module_path_mut(path: &mut Spanned<crate::syntax::Path>, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    for segment in &mut path.node.segments {
        visit(NamingRole::ModuleSegment, &mut segment.node.name.node);
    }
}

fn walk_field(field: &Field, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::Field, &field.name);
}

fn walk_field_mut(field: &mut Field, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::Field, &mut field.name.node);
}

fn walk_parameter(param: &Parameter, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::Binding, &param.name);
}

fn walk_parameter_mut(param: &mut Parameter, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    visit(NamingRole::Binding, &mut param.name.node);
}

fn walk_block(block: &Block, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    for stmt in &block.statements {
        walk_statement(&stmt.node, visit);
    }
}

fn walk_block_mut(block: &mut Block, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    for stmt in &mut block.statements {
        walk_statement_mut(&mut stmt.node, visit);
    }
}

fn walk_statement(stmt: &Statement, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    match stmt {
        Statement::Let(let_stmt) => {
            visit(NamingRole::Binding, &let_stmt.node.name);
            walk_expression(&let_stmt.node.value.node, visit);
        }
        Statement::Return(ret) => {
            if let Some(value) = &ret.node.value {
                walk_expression(&value.node, visit);
            }
        }
        Statement::While(w) => {
            walk_expression(&w.node.condition.node, visit);
            walk_block(&w.node.body.node, visit);
        }
        Statement::For(f) => {
            visit(NamingRole::Binding, &f.node.iterator);
            walk_expression(&f.node.iterable.node, visit);
            walk_block(&f.node.body.node, visit);
        }
        Statement::If(i) => {
            walk_expression(&i.node.condition.node, visit);
            walk_block(&i.node.then_block.node, visit);
            if let Some(branch) = &i.node.else_branch {
                walk_else_branch(&branch.node, visit);
            }
        }
        Statement::With(w) => {
            walk_block(&w.node.body.node, visit);
        }
        Statement::Expression(e) => walk_expression(&e.node.expression.node, visit),
        Statement::Break(_) | Statement::Continue(_) | Statement::Launch(_) => {}
    }
}

fn walk_statement_mut(stmt: &mut Statement, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    match stmt {
        Statement::Let(let_stmt) => {
            visit(NamingRole::Binding, &mut let_stmt.node.name.node);
            walk_expression_mut(&mut let_stmt.node.value.node, visit);
        }
        Statement::Return(ret) => {
            if let Some(value) = &mut ret.node.value {
                walk_expression_mut(&mut value.node, visit);
            }
        }
        Statement::While(w) => {
            walk_expression_mut(&mut w.node.condition.node, visit);
            walk_block_mut(&mut w.node.body.node, visit);
        }
        Statement::For(f) => {
            visit(NamingRole::Binding, &mut f.node.iterator.node);
            walk_expression_mut(&mut f.node.iterable.node, visit);
            walk_block_mut(&mut f.node.body.node, visit);
        }
        Statement::If(i) => {
            walk_expression_mut(&mut i.node.condition.node, visit);
            walk_block_mut(&mut i.node.then_block.node, visit);
            if let Some(branch) = &mut i.node.else_branch {
                walk_else_branch_mut(&mut branch.node, visit);
            }
        }
        Statement::With(w) => walk_block_mut(&mut w.node.body.node, visit),
        Statement::Expression(e) => walk_expression_mut(&mut e.node.expression.node, visit),
        Statement::Break(_) | Statement::Continue(_) | Statement::Launch(_) => {}
    }
}

fn walk_expression(expr: &Expression, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    match expr {
        Expression::Lambda(lambda) => {
            for param in &lambda.node.parameters {
                visit(NamingRole::Binding, &param.node.name);
            }
            walk_expression(&lambda.node.body.node, visit);
        }
        Expression::Match(m) => {
            walk_expression(&m.node.scrutinee.node, visit);
            for arm in &m.node.arms {
                walk_pattern(&arm.node.pattern.node, visit);
                walk_expression(&arm.node.value.node, visit);
            }
        }
        Expression::Assign(a) => {
            walk_expression(&a.node.target.node, visit);
            walk_expression(&a.node.value.node, visit);
        }
        Expression::Binary(b) => {
            walk_expression(&b.node.left.node, visit);
            walk_expression(&b.node.right.node, visit);
        }
        Expression::Unary(u) => walk_expression(&u.node.expr.node, visit),
        Expression::Call(c) => {
            walk_expression(&c.node.callee.node, visit);
            for arg in &c.node.args {
                walk_expression(&arg.node, visit);
            }
        }
        Expression::Member(m) => walk_expression(&m.node.target.node, visit),
        Expression::Block(b) => walk_block(&b.node.block.node, visit),
        Expression::Grouped(g) => walk_expression(&g.node.expr.node, visit),
        Expression::Index(i) => {
            walk_expression(&i.node.target.node, visit);
            walk_expression(&i.node.index.node, visit);
        }
        Expression::StructLiteral(lit) => {
            for field in &lit.node.fields {
                visit(NamingRole::Field, &field.node.name);
                walk_expression(&field.node.value.node, visit);
            }
        }
        Expression::EnumConstructor(c) => {
            for arg in &c.node.args {
                walk_expression(&arg.node, visit);
            }
        }
        Expression::Spawn(s) => walk_expression(&s.node.callee.node, visit),
        Expression::Try(t) => walk_expression(&t.node.expr.node, visit),
        Expression::ArrayLiteral(a) => {
            for elem in &a.node.elements {
                walk_expression(&elem.node, visit);
            }
        }
        Expression::Literal(_)
        | Expression::Path(_)
        | Expression::MacroInvocation(_)
        | Expression::MacroMetavariable(_)
        | Expression::CodeString(_)
        | Expression::ClifBlock(_) => {}
    }
}

fn walk_expression_mut(expr: &mut Expression, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    match expr {
        Expression::Lambda(lambda) => {
            for param in &mut lambda.node.parameters {
                visit(NamingRole::Binding, &mut param.node.name.node);
            }
            walk_expression_mut(&mut lambda.node.body.node, visit);
        }
        Expression::Match(m) => {
            walk_expression_mut(&mut m.node.scrutinee.node, visit);
            for arm in &mut m.node.arms {
                walk_pattern_mut(&mut arm.node.pattern.node, visit);
                walk_expression_mut(&mut arm.node.value.node, visit);
            }
        }
        Expression::Assign(a) => {
            walk_expression_mut(&mut a.node.target.node, visit);
            walk_expression_mut(&mut a.node.value.node, visit);
        }
        Expression::Binary(b) => {
            walk_expression_mut(&mut b.node.left.node, visit);
            walk_expression_mut(&mut b.node.right.node, visit);
        }
        Expression::Unary(u) => walk_expression_mut(&mut u.node.expr.node, visit),
        Expression::Call(c) => {
            walk_expression_mut(&mut c.node.callee.node, visit);
            for arg in &mut c.node.args {
                walk_expression_mut(&mut arg.node, visit);
            }
        }
        Expression::Member(m) => walk_expression_mut(&mut m.node.target.node, visit),
        Expression::Block(b) => walk_block_mut(&mut b.node.block.node, visit),
        Expression::Grouped(g) => walk_expression_mut(&mut g.node.expr.node, visit),
        Expression::Index(i) => {
            walk_expression_mut(&mut i.node.target.node, visit);
            walk_expression_mut(&mut i.node.index.node, visit);
        }
        Expression::StructLiteral(lit) => {
            for field in &mut lit.node.fields {
                visit(NamingRole::Field, &mut field.node.name.node);
                walk_expression_mut(&mut field.node.value.node, visit);
            }
        }
        Expression::EnumConstructor(c) => {
            for arg in &mut c.node.args {
                walk_expression_mut(&mut arg.node, visit);
            }
        }
        Expression::Spawn(s) => walk_expression_mut(&mut s.node.callee.node, visit),
        Expression::Try(t) => walk_expression_mut(&mut t.node.expr.node, visit),
        Expression::ArrayLiteral(a) => {
            for elem in &mut a.node.elements {
                walk_expression_mut(&mut elem.node, visit);
            }
        }
        Expression::Literal(_)
        | Expression::Path(_)
        | Expression::MacroInvocation(_)
        | Expression::MacroMetavariable(_)
        | Expression::CodeString(_)
        | Expression::ClifBlock(_) => {}
    }
}

fn walk_pattern(pattern: &Pattern, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    match pattern {
        Pattern::Identifier(name) => visit(NamingRole::Binding, name),
        Pattern::Enum(enum_pat) => {
            for item in &enum_pat.node.items {
                walk_pattern(&item.node, visit);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}

fn walk_else_branch(branch: &crate::syntax::ElseBranch, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    match branch {
        crate::syntax::ElseBranch::Block(b) => walk_block(&b.node, visit),
        crate::syntax::ElseBranch::If(nested) => {
            walk_expression(&nested.node.condition.node, visit);
            walk_block(&nested.node.then_block.node, visit);
            if let Some(next) = &nested.node.else_branch {
                walk_else_branch(&next.node, visit);
            }
        }
    }
}

fn walk_else_branch_mut(branch: &mut crate::syntax::ElseBranch, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    match branch {
        crate::syntax::ElseBranch::Block(b) => walk_block_mut(&mut b.node, visit),
        crate::syntax::ElseBranch::If(nested) => {
            walk_expression_mut(&mut nested.node.condition.node, visit);
            walk_block_mut(&mut nested.node.then_block.node, visit);
            if let Some(next) = &mut nested.node.else_branch {
                walk_else_branch_mut(&mut next.node, visit);
            }
        }
    }
}

fn walk_pattern_mut(pattern: &mut Pattern, visit: &mut impl FnMut(NamingRole, &mut Identifier)) {
    match pattern {
        Pattern::Identifier(name) => visit(NamingRole::Binding, &mut name.node),
        Pattern::Enum(enum_pat) => {
            for item in &mut enum_pat.node.items {
                walk_pattern_mut(&mut item.node, visit);
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}
