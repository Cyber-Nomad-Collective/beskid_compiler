//! Phase-parameterized AST vs HIR: [`Phase`] maps each syntactic slot to either syntax or HIR types.

use crate::syntax::{
    ArrayLiteralExpression, AssignExpression, AttributeDeclaration, BinaryExpression,
    BlockExpression, BreakStatement, CallExpression, ContinueStatement, ContractDefinition,
    EnumConstructorExpression, EnumDefinition, ExpressionStatement, ExtendTypeDefinition,
    ForStatement, FunctionDefinition, GroupedExpression, HostDefinition, IfStatement,
    IndexExpression, InlineModule, LambdaExpression, LaunchStatement, LetStatement,
    LiteralExpression, MacroDefinition, MacroInvocation, MacroMetavariable, MatchExpression,
    MemberExpression, MethodDefinition, ModuleDeclaration, PathExpression, ReturnStatement,
    SpawnExpression, StructLiteralExpression, TestDefinition, TryExpression, TypeDefinition,
    UnaryExpression, UseDeclaration, WhileStatement, WithStatement,
};

use super::{
    HirArrayLiteralExpression, HirAssignExpression, HirBinaryExpression, HirBlockExpression,
    HirBreakStatement, HirCallExpression, HirContinueStatement, HirContractDefinition,
    HirEnumConstructorExpression, HirEnumDefinition, HirExpressionStatement,
    HirExtendTypeDefinition, HirForStatement, HirFunctionDefinition, HirGroupedExpression,
    HirIfStatement, HirIndexExpression, HirInlineModule, HirLambdaExpression, HirLetStatement,
    HirLiteralExpression, HirMatchExpression, HirMemberExpression, HirMethodDefinition,
    HirModuleDeclaration, HirPathExpression, HirReturnStatement, HirSpawnExpression,
    HirStructLiteralExpression, HirTestDefinition, HirTryExpression, HirTypeDefinition,
    HirUnaryExpression, HirUseDeclaration, HirWhileStatement, item::HirAttributeDeclaration,
};

/// Type-level association between one program shape (AST or HIR) and the concrete types of items and statements.
pub trait Phase {
    type HostDefinition;
    type FunctionDefinition;
    type MethodDefinition;
    type ExtendTypeDefinition;
    type TypeDefinition;
    type EnumDefinition;
    type ContractDefinition;
    type TestDefinition;
    type AttributeDeclaration;
    type ModuleDeclaration;
    type InlineModule;
    type UseDeclaration;
    type MacroDefinition;

    type LetStatement;
    type ReturnStatement;
    type BreakStatement;
    type ContinueStatement;
    type WhileStatement;
    type ForStatement;
    type IfStatement;
    type WithStatement;
    type LaunchStatement;
    type ExpressionStatement;

    type MatchExpression;
    type AssignExpression;
    type BinaryExpression;
    type UnaryExpression;
    type CallExpression;
    type MemberExpression;
    type LiteralExpression;
    type PathExpression;
    type StructLiteralExpression;
    type IndexExpression;
    type ArrayLiteralExpression;
    type EnumConstructorExpression;
    type BlockExpression;
    type GroupedExpression;
    type TryExpression;
    type LambdaExpression;
    type SpawnExpression;
    type MacroInvocation;
    type MacroMetavariable;
}

/// Marker for [`Program`](super::program::Program) nodes that still use syntax tree types.
#[derive(Debug, Clone, Copy, Default)]
pub struct AstPhase;

/// Marker for [`Program`](super::program::Program) after [`super::lowering::lower_program`].
#[derive(Debug, Clone, Copy, Default)]
pub struct HirPhase;

impl Phase for AstPhase {
    type HostDefinition = HostDefinition;
    type FunctionDefinition = FunctionDefinition;
    type MethodDefinition = MethodDefinition;
    type ExtendTypeDefinition = ExtendTypeDefinition;
    type TypeDefinition = TypeDefinition;
    type EnumDefinition = EnumDefinition;
    type ContractDefinition = ContractDefinition;
    type TestDefinition = TestDefinition;
    type AttributeDeclaration = AttributeDeclaration;
    type ModuleDeclaration = ModuleDeclaration;
    type InlineModule = InlineModule;
    type UseDeclaration = UseDeclaration;
    type MacroDefinition = MacroDefinition;

    type LetStatement = LetStatement;
    type ReturnStatement = ReturnStatement;
    type BreakStatement = BreakStatement;
    type ContinueStatement = ContinueStatement;
    type WhileStatement = WhileStatement;
    type ForStatement = ForStatement;
    type IfStatement = IfStatement;
    type WithStatement = WithStatement;
    type LaunchStatement = LaunchStatement;
    type ExpressionStatement = ExpressionStatement;

    type MatchExpression = MatchExpression;
    type AssignExpression = AssignExpression;
    type BinaryExpression = BinaryExpression;
    type UnaryExpression = UnaryExpression;
    type CallExpression = CallExpression;
    type MemberExpression = MemberExpression;
    type LiteralExpression = LiteralExpression;
    type PathExpression = PathExpression;
    type StructLiteralExpression = StructLiteralExpression;
    type IndexExpression = IndexExpression;
    type ArrayLiteralExpression = ArrayLiteralExpression;
    type EnumConstructorExpression = EnumConstructorExpression;
    type BlockExpression = BlockExpression;
    type GroupedExpression = GroupedExpression;
    type TryExpression = TryExpression;
    type LambdaExpression = LambdaExpression;
    type SpawnExpression = SpawnExpression;
    type MacroInvocation = MacroInvocation;
    type MacroMetavariable = MacroMetavariable;
}

impl Phase for HirPhase {
    type HostDefinition = HostDefinition;
    type FunctionDefinition = HirFunctionDefinition;
    type MethodDefinition = HirMethodDefinition;
    type ExtendTypeDefinition = HirExtendTypeDefinition;
    type TypeDefinition = HirTypeDefinition;
    type EnumDefinition = HirEnumDefinition;
    type ContractDefinition = HirContractDefinition;
    type TestDefinition = HirTestDefinition;
    type AttributeDeclaration = HirAttributeDeclaration;
    type ModuleDeclaration = HirModuleDeclaration;
    type InlineModule = HirInlineModule;
    type UseDeclaration = HirUseDeclaration;
    type MacroDefinition = MacroDefinition;

    type LetStatement = HirLetStatement;
    type ReturnStatement = HirReturnStatement;
    type BreakStatement = HirBreakStatement;
    type ContinueStatement = HirContinueStatement;
    type WhileStatement = HirWhileStatement;
    type ForStatement = HirForStatement;
    type IfStatement = HirIfStatement;
    type WithStatement = WithStatement;
    type LaunchStatement = LaunchStatement;
    type ExpressionStatement = HirExpressionStatement;

    type MatchExpression = HirMatchExpression;
    type AssignExpression = HirAssignExpression;
    type BinaryExpression = HirBinaryExpression;
    type UnaryExpression = HirUnaryExpression;
    type CallExpression = HirCallExpression;
    type MemberExpression = HirMemberExpression;
    type LiteralExpression = HirLiteralExpression;
    type PathExpression = HirPathExpression;
    type StructLiteralExpression = HirStructLiteralExpression;
    type IndexExpression = HirIndexExpression;
    type ArrayLiteralExpression = HirArrayLiteralExpression;
    type EnumConstructorExpression = HirEnumConstructorExpression;
    type BlockExpression = HirBlockExpression;
    type GroupedExpression = HirGroupedExpression;
    type TryExpression = HirTryExpression;
    type LambdaExpression = HirLambdaExpression;
    type SpawnExpression = HirSpawnExpression;
    type MacroInvocation = MacroInvocation;
    type MacroMetavariable = MacroMetavariable;
}
