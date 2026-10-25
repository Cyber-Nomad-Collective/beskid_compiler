//! Naming-role identifier sites in a [`Program`] syntax tree (the naming-style lint and its quick fix).

use crate::naming_case::NamingProfile;
use crate::syntax::ContractNode;
use crate::syntax::{
    Block, ContractDefinition, EnumDefinition, EnumVariant, Expression, ExtendTypeDefinition, Field, FieldKind,
    FunctionDefinition, ImplBlock, InlineModule, MethodDefinition, Node, Parameter, Pattern, Program, Statement,
    TestDefinition, TypeDefinition,
};
use crate::syntax::{Identifier, Spanned, TypeParameter};

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
            | Self::GenericParameter
            | Self::Test => NamingProfile::PascalCase,
            Self::Field | Self::Binding | Self::Macro => NamingProfile::LowerCamelCase,
        }
    }
}

pub fn walk_program(program: &Program, mut visit: impl FnMut(NamingRole, &Spanned<Identifier>)) {
    for item in &program.items {
        walk_node(&item.node, &mut visit);
    }
}

fn walk_node(node: &Node, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    match node {
        // A generated host-hook argument type mirrors the hook parameters, which the generated
        // function already carries; walking both would report each parameter twice.
        Node::TypeDefinition(def) if crate::composition::is_hook_item_name(&def.node.name.node.name) => {}
        Node::TypeDefinition(def) => walk_type_definition(&def.node, visit),
        Node::EnumDefinition(def) => walk_enum_definition(&def.node, visit),
        Node::ContractDefinition(def) => walk_contract_definition(&def.node, visit),
        Node::Function(def) => walk_function_definition(&def.node, visit),
        // Constants retain their source spelling; canonical runtime layouts use established
        // uppercase ABI names which are intentionally outside local-binding style rules.
        Node::ConstantDefinition(_) => {}
        Node::Method(def) => walk_method_definition(&def.node, visit),
        Node::ImplBlock(def) => walk_impl_block(&def.node, visit),
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

fn walk_type_definition(def: &TypeDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::TypeDeclaration, &def.name);
    for generic in &def.generics {
        visit(NamingRole::GenericParameter, &TypeParameter::identifier(generic));
    }
    for field in &def.fields {
        walk_field(&field.node, visit);
    }
    for method in &def.methods {
        walk_method_definition(&method.node, visit);
    }
}

fn walk_enum_definition(def: &EnumDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::TypeDeclaration, &def.name);
    for generic in &def.generics {
        visit(NamingRole::GenericParameter, &TypeParameter::identifier(generic));
    }
    for variant in &def.variants {
        walk_enum_variant(&variant.node, visit);
    }
}

fn walk_enum_variant(variant: &EnumVariant, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::EnumVariant, &variant.name);
    for field in &variant.fields {
        walk_field(&field.node, visit);
    }
}

/// An `[Extern]` contract's method identifiers are logical foreign symbols (the method name is
/// the link-time symbol unless `Symbol` overrides it), so Beskid naming conventions never apply
/// to them and normalization must never rename them.
fn is_extern_contract(def: &ContractDefinition) -> bool {
    def.attributes.iter().any(|attribute| attribute.node.name.node.name == "Extern")
}

fn walk_contract_definition(def: &ContractDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::TypeDeclaration, &def.name);
    for generic in &def.generics {
        visit(NamingRole::GenericParameter, &TypeParameter::identifier(generic));
    }
    let foreign_symbols = is_extern_contract(def);
    for item in &def.items {
        if let ContractNode::MethodSignature(sig) = &item.node {
            if !foreign_symbols {
                visit(NamingRole::Callable, &sig.node.name);
            }
            for param in &sig.node.parameters {
                walk_parameter(&param.node, visit);
            }
        }
    }
}

fn walk_function_definition(def: &FunctionDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::Callable, &def.name);
    for generic in &def.generics {
        visit(NamingRole::GenericParameter, &TypeParameter::identifier(generic));
    }
    for param in &def.parameters {
        walk_parameter(&param.node, visit);
    }
    walk_block(&def.body.node, visit);
}

fn walk_method_definition(def: &MethodDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::Callable, &def.name);
    for generic in &def.generics {
        visit(NamingRole::GenericParameter, &TypeParameter::identifier(generic));
    }
    for param in &def.parameters {
        walk_parameter(&param.node, visit);
    }
    walk_block(&def.body.node, visit);
}

fn walk_extend_type(def: &ExtendTypeDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    for method in &def.methods {
        walk_method_definition(&method.node, visit);
    }
}

fn walk_impl_block(def: &ImplBlock, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    for method in &def.methods {
        walk_method_definition(&method.node, visit);
    }
}

fn walk_test_definition(def: &TestDefinition, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::Test, &def.name);
    for stmt in &def.statements {
        walk_statement(&stmt.node, visit);
    }
}

fn walk_inline_module(module: &InlineModule, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::ModuleSegment, &module.name);
    for item in &module.items {
        walk_node(&item.node, visit);
    }
}

fn walk_module_path(path: &Spanned<crate::syntax::Path>, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    for segment in &path.node.segments {
        visit(NamingRole::ModuleSegment, &segment.node.name);
    }
}

/// An `event` member is a callable member (raised and subscribed like a method), so it carries the
/// PascalCase callable profile (`event OnResize(ConsoleSize size)`); every other field is
/// lowerCamelCase.
fn field_role(field: &Field) -> NamingRole {
    if field.kind == FieldKind::Event { NamingRole::Callable } else { NamingRole::Field }
}

fn walk_field(field: &Field, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(field_role(field), &field.name);
}

fn walk_parameter(param: &Parameter, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    visit(NamingRole::Binding, &param.name);
}

fn walk_block(block: &Block, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    for stmt in &block.statements {
        walk_statement(&stmt.node, visit);
    }
}

fn walk_statement(stmt: &Statement, visit: &mut impl FnMut(NamingRole, &Spanned<Identifier>)) {
    if let Statement::Use(scoped) = stmt
        && let Some(body) = &scoped.node.body
    {
        walk_block(&body.node, visit);
    }
    match stmt {
        Statement::Let(let_stmt)
        | Statement::Use(Spanned { node: crate::syntax::ScopedUseStatement { binding: let_stmt, .. }, .. }) => {
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
        Expression::Conditional(c) => {
            walk_expression(&c.node.condition.node, visit);
            walk_expression(&c.node.then_branch.node, visit);
            walk_expression(&c.node.else_branch.node, visit);
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
        Expression::Move(m) => walk_expression(&m.node.operand.node, visit),
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

#[cfg(test)]
mod tests {
    use super::{NamingRole, walk_program};
    use crate::services::parse_program;

    #[test]
    fn v06_event_members_carry_the_callable_profile() {
        let program =
            parse_program("pub type Hub { i32 lastSize; event{16} OnResize(i32 size); }").expect("event type parses");
        let mut roles = Vec::new();
        walk_program(&program.node, |role, name| roles.push((role, name.node.name.clone())));
        assert!(roles.contains(&(NamingRole::Callable, "OnResize".to_string())), "{roles:?}");
        assert!(roles.contains(&(NamingRole::Field, "lastSize".to_string())), "{roles:?}");
        assert!(!roles.contains(&(NamingRole::Field, "OnResize".to_string())), "{roles:?}");
    }
}
