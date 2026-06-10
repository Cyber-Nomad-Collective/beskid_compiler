# API reference

## Structure

- `Beskid`
  - `Compiler`
    - `Collect`
      - `AnalysisRequest`
        - `Beskid::Compiler::Collect::AnalysisRequest` (`type`)
      - `AnalysisResult`
        - `Beskid::Compiler::Collect::AnalysisResult` (`type`)
      - `Analyzer`
        - `Analyze`
          - `Beskid::Compiler::Collect::Analyzer::Analyze` (`contract_method`)
        - `request`
          - `Beskid::Compiler::Collect::Analyzer::request` (`parameter`)
        - `Beskid::Compiler::Collect::Analyzer` (`contract`)
      - `AttributeDeclarationSet`
        - `Beskid::Compiler::Collect::AttributeDeclarationSet` (`type`)
      - `AttributeGenerationRequest`
        - `Beskid::Compiler::Collect::AttributeGenerationRequest` (`type`)
      - `AttributeGenerator`
        - `Attributes`
          - `Beskid::Compiler::Collect::AttributeGenerator::Attributes` (`contract_method`)
        - `request`
          - `Beskid::Compiler::Collect::AttributeGenerator::request` (`parameter`)
        - `Beskid::Compiler::Collect::AttributeGenerator` (`contract`)
      - `CollectFacadeVersion`
        - `Beskid::Compiler::Collect::CollectFacadeVersion` (`function`)
      - `CollectRequest`
        - `Beskid::Compiler::Collect::CollectRequest` (`type`)
      - `CollectTargetSet`
        - `Beskid::Compiler::Collect::CollectTargetSet` (`type`)
      - `Collector`
        - `Collect`
          - `Beskid::Compiler::Collect::Collector::Collect` (`contract_method`)
        - `request`
          - `Beskid::Compiler::Collect::Collector::request` (`parameter`)
        - `Beskid::Compiler::Collect::Collector` (`contract`)
      - `FixError`
        - `Beskid::Compiler::Collect::FixError` (`type`)
      - `GeneratedSyntaxContribution`
        - `Beskid::Compiler::Collect::GeneratedSyntaxContribution` (`type`)
      - `GenerationRequest`
        - `Beskid::Compiler::Collect::GenerationRequest` (`type`)
      - `Generator`
        - `Generate`
          - `Beskid::Compiler::Collect::Generator::Generate` (`contract_method`)
        - `request`
          - `Beskid::Compiler::Collect::Generator::request` (`parameter`)
        - `Beskid::Compiler::Collect::Generator` (`contract`)
      - `Rewriter`
        - `Rewrite`
          - `Beskid::Compiler::Collect::Rewriter::Rewrite` (`contract_method`)
        - `sourceNode`
          - `Beskid::Compiler::Collect::Rewriter::sourceNode` (`parameter`)
        - `Beskid::Compiler::Collect::Rewriter` (`contract`)
      - `Beskid::Compiler::Collect` (`module`)
    - `Compilation`
      - `CompilerLanguageVersionToken`
        - `Beskid::Compiler::Compilation::CompilerLanguageVersionToken` (`function`)
      - `ModSdkCompilationSurfaceVersion`
        - `Beskid::Compiler::Compilation::ModSdkCompilationSurfaceVersion` (`function`)
      - `ReflectSdkCompilationSnapshotPlane`
        - `Hir`
          - `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::Hir` (`enum_variant`)
        - `SemanticSnapshot`
          - `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::SemanticSnapshot` (`enum_variant`)
        - `SyntaxTree`
          - `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::SyntaxTree` (`enum_variant`)
        - `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane` (`enum`)
      - `SemanticSnapshotFamilyToken`
        - `Beskid::Compiler::Compilation::SemanticSnapshotFamilyToken` (`function`)
      - `Beskid::Compiler::Compilation` (`module`)
    - `Diagnostics`
      - `Beskid::Compiler::Diagnostics` (`module`)
    - `Query`
      - `Beskid::Compiler::Query` (`module`)
    - `TypedEmitter`
      - `ReflectSdkEmitContributionKind`
        - `DiagnosticAttachment`
          - `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::DiagnosticAttachment` (`enum_variant`)
        - `LoweringDirective`
          - `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::LoweringDirective` (`enum_variant`)
        - `Metadata`
          - `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::Metadata` (`enum_variant`)
        - `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind` (`enum`)
      - `TypedEmitterFacadeVersion`
        - `Beskid::Compiler::TypedEmitter::TypedEmitterFacadeVersion` (`function`)
      - `Beskid::Compiler::TypedEmitter` (`module`)
  - `Syntax`
    - `Nodes`
      - `ArrayLiteralExpression`
        - `ArrayLiteralExpression`
          - `elements`
            - `Beskid::Syntax::Nodes::ArrayLiteralExpression::ArrayLiteralExpression::elements` (`field`)
          - `Beskid::Syntax::Nodes::ArrayLiteralExpression::ArrayLiteralExpression` (`type`)
        - `Beskid::Syntax::Nodes::ArrayLiteralExpression` (`module`)
      - `AssignExpression`
        - `AssignExpression`
          - `target`
            - `Beskid::Syntax::Nodes::AssignExpression::AssignExpression::target` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::AssignExpression::AssignExpression::value` (`field`)
          - `Beskid::Syntax::Nodes::AssignExpression::AssignExpression` (`type`)
        - `Beskid::Syntax::Nodes::AssignExpression` (`module`)
      - `AssignOp`
        - `AssignOp`
          - `AddAssign`
            - `Beskid::Syntax::Nodes::AssignOp::AssignOp::AddAssign` (`enum_variant`)
          - `Assign`
            - `Beskid::Syntax::Nodes::AssignOp::AssignOp::Assign` (`enum_variant`)
          - `SubAssign`
            - `Beskid::Syntax::Nodes::AssignOp::AssignOp::SubAssign` (`enum_variant`)
          - `Beskid::Syntax::Nodes::AssignOp::AssignOp` (`enum`)
        - `Beskid::Syntax::Nodes::AssignOp` (`module`)
      - `Attribute`
        - `Attribute`
          - `arguments`
            - `Beskid::Syntax::Nodes::Attribute::Attribute::arguments` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::Attribute::Attribute::name` (`field`)
          - `Beskid::Syntax::Nodes::Attribute::Attribute` (`type`)
        - `Beskid::Syntax::Nodes::Attribute` (`module`)
      - `AttributeArgument`
        - `AttributeArgument`
          - `name`
            - `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument::name` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument::value` (`field`)
          - `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument` (`type`)
        - `Beskid::Syntax::Nodes::AttributeArgument` (`module`)
      - `AttributeArgumentList`
        - `Beskid::Syntax::Nodes::AttributeArgumentList` (`module`)
      - `AttributeDeclaration`
        - `AttributeDeclaration`
          - `name`
            - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::parameters` (`field`)
          - `targets`
            - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::targets` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::visibility` (`field`)
          - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration` (`type`)
        - `Beskid::Syntax::Nodes::AttributeDeclaration` (`module`)
      - `AttributeList`
        - `Beskid::Syntax::Nodes::AttributeList` (`module`)
      - `AttributeParameter`
        - `AttributeParameter`
          - `defaultValue`
            - `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::defaultValue` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::name` (`field`)
          - `ty`
            - `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::ty` (`field`)
          - `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter` (`type`)
        - `Beskid::Syntax::Nodes::AttributeParameter` (`module`)
      - `AttributeParameterList`
        - `Beskid::Syntax::Nodes::AttributeParameterList` (`module`)
      - `AttributeTarget`
        - `AttributeTarget`
          - `name`
            - `Beskid::Syntax::Nodes::AttributeTarget::AttributeTarget::name` (`field`)
          - `Beskid::Syntax::Nodes::AttributeTarget::AttributeTarget` (`type`)
        - `Beskid::Syntax::Nodes::AttributeTarget` (`module`)
      - `AttributeTargetList`
        - `Beskid::Syntax::Nodes::AttributeTargetList` (`module`)
      - `BinaryExpression`
        - `BinaryExpression`
          - `left`
            - `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::left` (`field`)
          - `op`
            - `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::op` (`field`)
          - `right`
            - `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::right` (`field`)
          - `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression` (`type`)
        - `Beskid::Syntax::Nodes::BinaryExpression` (`module`)
      - `BinaryOp`
        - `BinaryOp`
          - `Add`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Add` (`enum_variant`)
          - `And`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::And` (`enum_variant`)
          - `Div`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Div` (`enum_variant`)
          - `Eq`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Eq` (`enum_variant`)
          - `Gt`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Gt` (`enum_variant`)
          - `Gte`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Gte` (`enum_variant`)
          - `IdentityEq`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::IdentityEq` (`enum_variant`)
          - `IdentityNotEq`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::IdentityNotEq` (`enum_variant`)
          - `Lt`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Lt` (`enum_variant`)
          - `Lte`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Lte` (`enum_variant`)
          - `Mul`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Mul` (`enum_variant`)
          - `NotEq`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::NotEq` (`enum_variant`)
          - `Or`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Or` (`enum_variant`)
          - `Sub`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Sub` (`enum_variant`)
          - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp` (`enum`)
        - `Beskid::Syntax::Nodes::BinaryOp` (`module`)
      - `Block`
        - `Block`
          - `statements`
            - `Beskid::Syntax::Nodes::Block::Block::statements` (`field`)
          - `Beskid::Syntax::Nodes::Block::Block` (`type`)
        - `Beskid::Syntax::Nodes::Block` (`module`)
      - `BlockExpression`
        - `BlockExpression`
          - `block`
            - `Beskid::Syntax::Nodes::BlockExpression::BlockExpression::block` (`field`)
          - `Beskid::Syntax::Nodes::BlockExpression::BlockExpression` (`type`)
        - `Beskid::Syntax::Nodes::BlockExpression` (`module`)
      - `BreakStatement`
        - `BreakStatement`
          - `Beskid::Syntax::Nodes::BreakStatement::BreakStatement` (`type`)
        - `Beskid::Syntax::Nodes::BreakStatement` (`module`)
      - `CallExpression`
        - `CallExpression`
          - `args`
            - `Beskid::Syntax::Nodes::CallExpression::CallExpression::args` (`field`)
          - `callee`
            - `Beskid::Syntax::Nodes::CallExpression::CallExpression::callee` (`field`)
          - `Beskid::Syntax::Nodes::CallExpression::CallExpression` (`type`)
        - `Beskid::Syntax::Nodes::CallExpression` (`module`)
      - `ContinueStatement`
        - `ContinueStatement`
          - `Beskid::Syntax::Nodes::ContinueStatement::ContinueStatement` (`type`)
        - `Beskid::Syntax::Nodes::ContinueStatement` (`module`)
      - `ContractDefinition`
        - `ContractDefinition`
          - `attributes`
            - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::attributes` (`field`)
          - `items`
            - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::items` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::name` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition` (`type`)
        - `Beskid::Syntax::Nodes::ContractDefinition` (`module`)
      - `ContractEmbedding`
        - `ContractEmbedding`
          - `name`
            - `Beskid::Syntax::Nodes::ContractEmbedding::ContractEmbedding::name` (`field`)
          - `Beskid::Syntax::Nodes::ContractEmbedding::ContractEmbedding` (`type`)
        - `Beskid::Syntax::Nodes::ContractEmbedding` (`module`)
      - `ContractMethodSignature`
        - `ContractMethodSignature`
          - `name`
            - `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::parameters` (`field`)
          - `returnType`
            - `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::returnType` (`field`)
          - `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature` (`type`)
        - `Beskid::Syntax::Nodes::ContractMethodSignature` (`module`)
      - `ContractNode`
        - `ContractNode`
          - `Embedding`
            - `Beskid::Syntax::Nodes::ContractNode::ContractNode::Embedding` (`enum_variant`)
          - `MethodSignature`
            - `Beskid::Syntax::Nodes::ContractNode::ContractNode::MethodSignature` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::ContractNode::ContractNode::payload` (`field`)
            - `Beskid::Syntax::Nodes::ContractNode::ContractNode::payload` (`field`)
          - `Beskid::Syntax::Nodes::ContractNode::ContractNode` (`enum`)
        - `Beskid::Syntax::Nodes::ContractNode` (`module`)
      - `ContractNodeList`
        - `Beskid::Syntax::Nodes::ContractNodeList` (`module`)
      - `Descendants`
        - `Descendants`
          - `Current`
            - `Beskid::Syntax::Nodes::Descendants::Descendants::Current` (`contract_method`)
          - `MoveNext`
            - `Beskid::Syntax::Nodes::Descendants::Descendants::MoveNext` (`contract_method`)
          - `Beskid::Syntax::Nodes::Descendants::Descendants` (`contract`)
        - `Beskid::Syntax::Nodes::Descendants` (`module`)
      - `EnumConstructorExpression`
        - `EnumConstructorExpression`
          - `args`
            - `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression::args` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression::path` (`field`)
          - `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression` (`type`)
        - `Beskid::Syntax::Nodes::EnumConstructorExpression` (`module`)
      - `EnumDefinition`
        - `EnumDefinition`
          - `generics`
            - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::generics` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::name` (`field`)
          - `variants`
            - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::variants` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition` (`type`)
        - `Beskid::Syntax::Nodes::EnumDefinition` (`module`)
      - `EnumPath`
        - `EnumPath`
          - `typePath`
            - `Beskid::Syntax::Nodes::EnumPath::EnumPath::typePath` (`field`)
          - `variant`
            - `Beskid::Syntax::Nodes::EnumPath::EnumPath::variant` (`field`)
          - `Beskid::Syntax::Nodes::EnumPath::EnumPath` (`type`)
        - `Beskid::Syntax::Nodes::EnumPath` (`module`)
      - `EnumPattern`
        - `EnumPattern`
          - `items`
            - `Beskid::Syntax::Nodes::EnumPattern::EnumPattern::items` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::EnumPattern::EnumPattern::path` (`field`)
          - `Beskid::Syntax::Nodes::EnumPattern::EnumPattern` (`type`)
        - `Beskid::Syntax::Nodes::EnumPattern` (`module`)
      - `EnumVariant`
        - `EnumVariant`
          - `fields`
            - `Beskid::Syntax::Nodes::EnumVariant::EnumVariant::fields` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::EnumVariant::EnumVariant::name` (`field`)
          - `Beskid::Syntax::Nodes::EnumVariant::EnumVariant` (`type`)
        - `Beskid::Syntax::Nodes::EnumVariant` (`module`)
      - `EnumVariantList`
        - `Beskid::Syntax::Nodes::EnumVariantList` (`module`)
      - `Expression`
        - `Expression`
          - `ArrayLiteral`
            - `Beskid::Syntax::Nodes::Expression::Expression::ArrayLiteral` (`enum_variant`)
          - `Assign`
            - `Beskid::Syntax::Nodes::Expression::Expression::Assign` (`enum_variant`)
          - `Binary`
            - `Beskid::Syntax::Nodes::Expression::Expression::Binary` (`enum_variant`)
          - `Block`
            - `Beskid::Syntax::Nodes::Expression::Expression::Block` (`enum_variant`)
          - `Call`
            - `Beskid::Syntax::Nodes::Expression::Expression::Call` (`enum_variant`)
          - `EnumConstructor`
            - `Beskid::Syntax::Nodes::Expression::Expression::EnumConstructor` (`enum_variant`)
          - `Grouped`
            - `Beskid::Syntax::Nodes::Expression::Expression::Grouped` (`enum_variant`)
          - `Index`
            - `Beskid::Syntax::Nodes::Expression::Expression::Index` (`enum_variant`)
          - `Lambda`
            - `Beskid::Syntax::Nodes::Expression::Expression::Lambda` (`enum_variant`)
          - `Literal`
            - `Beskid::Syntax::Nodes::Expression::Expression::Literal` (`enum_variant`)
          - `MacroInvocation`
            - `Beskid::Syntax::Nodes::Expression::Expression::MacroInvocation` (`enum_variant`)
          - `MacroMetavariable`
            - `Beskid::Syntax::Nodes::Expression::Expression::MacroMetavariable` (`enum_variant`)
          - `Match`
            - `Beskid::Syntax::Nodes::Expression::Expression::Match` (`enum_variant`)
          - `Member`
            - `Beskid::Syntax::Nodes::Expression::Expression::Member` (`enum_variant`)
          - `Path`
            - `Beskid::Syntax::Nodes::Expression::Expression::Path` (`enum_variant`)
          - `Spawn`
            - `Beskid::Syntax::Nodes::Expression::Expression::Spawn` (`enum_variant`)
          - `StructLiteral`
            - `Beskid::Syntax::Nodes::Expression::Expression::StructLiteral` (`enum_variant`)
          - `Try`
            - `Beskid::Syntax::Nodes::Expression::Expression::Try` (`enum_variant`)
          - `Unary`
            - `Beskid::Syntax::Nodes::Expression::Expression::Unary` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
          - `Beskid::Syntax::Nodes::Expression::Expression` (`enum`)
        - `Beskid::Syntax::Nodes::Expression` (`module`)
      - `ExpressionList`
        - `Beskid::Syntax::Nodes::ExpressionList` (`module`)
      - `ExpressionStatement`
        - `ExpressionStatement`
          - `expression`
            - `Beskid::Syntax::Nodes::ExpressionStatement::ExpressionStatement::expression` (`field`)
          - `Beskid::Syntax::Nodes::ExpressionStatement::ExpressionStatement` (`type`)
        - `Beskid::Syntax::Nodes::ExpressionStatement` (`module`)
      - `ExtendTypeDefinition`
        - `ExtendTypeDefinition`
          - `methods`
            - `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition::methods` (`field`)
          - `targetType`
            - `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition::targetType` (`field`)
          - `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition` (`type`)
        - `Beskid::Syntax::Nodes::ExtendTypeDefinition` (`module`)
      - `Field`
        - `Field`
          - `name`
            - `Beskid::Syntax::Nodes::Field::Field::name` (`field`)
          - `ty`
            - `Beskid::Syntax::Nodes::Field::Field::ty` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::Field::Field::visibility` (`field`)
          - `Beskid::Syntax::Nodes::Field::Field` (`type`)
        - `Beskid::Syntax::Nodes::Field` (`module`)
      - `FieldKind`
        - `FieldKind`
          - `Event`
            - `Beskid::Syntax::Nodes::FieldKind::FieldKind::Event` (`enum_variant`)
          - `Injected`
            - `Beskid::Syntax::Nodes::FieldKind::FieldKind::Injected` (`enum_variant`)
          - `Value`
            - `Beskid::Syntax::Nodes::FieldKind::FieldKind::Value` (`enum_variant`)
          - `Beskid::Syntax::Nodes::FieldKind::FieldKind` (`enum`)
        - `Beskid::Syntax::Nodes::FieldKind` (`module`)
      - `FieldList`
        - `Beskid::Syntax::Nodes::FieldList` (`module`)
      - `ForStatement`
        - `ForStatement`
          - `body`
            - `Beskid::Syntax::Nodes::ForStatement::ForStatement::body` (`field`)
          - `iterable`
            - `Beskid::Syntax::Nodes::ForStatement::ForStatement::iterable` (`field`)
          - `iterator`
            - `Beskid::Syntax::Nodes::ForStatement::ForStatement::iterator` (`field`)
          - `Beskid::Syntax::Nodes::ForStatement::ForStatement` (`type`)
        - `Beskid::Syntax::Nodes::ForStatement` (`module`)
      - `FunctionDefinition`
        - `FunctionDefinition`
          - `attributes`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::attributes` (`field`)
          - `body`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::body` (`field`)
          - `generics`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::generics` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::parameters` (`field`)
          - `returnType`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::returnType` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition` (`type`)
        - `Beskid::Syntax::Nodes::FunctionDefinition` (`module`)
      - `GroupedExpression`
        - `GroupedExpression`
          - `expr`
            - `Beskid::Syntax::Nodes::GroupedExpression::GroupedExpression::expr` (`field`)
          - `Beskid::Syntax::Nodes::GroupedExpression::GroupedExpression` (`type`)
        - `Beskid::Syntax::Nodes::GroupedExpression` (`module`)
      - `HostBodyItem`
        - `Beskid::Syntax::Nodes::HostBodyItem` (`module`)
      - `HostBodyItemList`
        - `Beskid::Syntax::Nodes::HostBodyItemList` (`module`)
      - `HostDefinition`
        - `HostDefinition`
          - `baseHost`
            - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::baseHost` (`field`)
          - `body`
            - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::body` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::parameters` (`field`)
          - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition` (`type`)
        - `Beskid::Syntax::Nodes::HostDefinition` (`module`)
      - `Identifier`
        - `Identifier`
          - `name`
            - `Beskid::Syntax::Nodes::Identifier::Identifier::name` (`field`)
          - `Beskid::Syntax::Nodes::Identifier::Identifier` (`type`)
        - `Beskid::Syntax::Nodes::Identifier` (`module`)
      - `IdentifierList`
        - `Beskid::Syntax::Nodes::IdentifierList` (`module`)
      - `IfStatement`
        - `IfStatement`
          - `condition`
            - `Beskid::Syntax::Nodes::IfStatement::IfStatement::condition` (`field`)
          - `elseBlock`
            - `Beskid::Syntax::Nodes::IfStatement::IfStatement::elseBlock` (`field`)
          - `thenBlock`
            - `Beskid::Syntax::Nodes::IfStatement::IfStatement::thenBlock` (`field`)
          - `Beskid::Syntax::Nodes::IfStatement::IfStatement` (`type`)
        - `Beskid::Syntax::Nodes::IfStatement` (`module`)
      - `IndexExpression`
        - `IndexExpression`
          - `index`
            - `Beskid::Syntax::Nodes::IndexExpression::IndexExpression::index` (`field`)
          - `target`
            - `Beskid::Syntax::Nodes::IndexExpression::IndexExpression::target` (`field`)
          - `Beskid::Syntax::Nodes::IndexExpression::IndexExpression` (`type`)
        - `Beskid::Syntax::Nodes::IndexExpression` (`module`)
      - `InjectQualifier`
        - `InjectQualifier`
          - `Global`
            - `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier::Global` (`enum_variant`)
          - `Parent`
            - `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier::Parent` (`enum_variant`)
          - `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier` (`enum`)
        - `Beskid::Syntax::Nodes::InjectQualifier` (`module`)
      - `InlineModule`
        - `InlineModule`
          - `attributes`
            - `Beskid::Syntax::Nodes::InlineModule::InlineModule::attributes` (`field`)
          - `items`
            - `Beskid::Syntax::Nodes::InlineModule::InlineModule::items` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::InlineModule::InlineModule::name` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::InlineModule::InlineModule::visibility` (`field`)
          - `Beskid::Syntax::Nodes::InlineModule::InlineModule` (`type`)
        - `Beskid::Syntax::Nodes::InlineModule` (`module`)
      - `LambdaExpression`
        - `LambdaExpression`
          - `body`
            - `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression::body` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression::parameters` (`field`)
          - `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression` (`type`)
        - `Beskid::Syntax::Nodes::LambdaExpression` (`module`)
      - `LambdaParameter`
        - `LambdaParameter`
          - `name`
            - `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter::name` (`field`)
          - `ty`
            - `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter::ty` (`field`)
          - `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter` (`type`)
        - `Beskid::Syntax::Nodes::LambdaParameter` (`module`)
      - `LambdaParameterList`
        - `Beskid::Syntax::Nodes::LambdaParameterList` (`module`)
      - `LaunchStatement`
        - `LaunchStatement`
          - `arguments`
            - `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement::arguments` (`field`)
          - `hostPath`
            - `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement::hostPath` (`field`)
          - `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement` (`type`)
        - `Beskid::Syntax::Nodes::LaunchStatement` (`module`)
      - `LetStatement`
        - `LetStatement`
          - `name`
            - `Beskid::Syntax::Nodes::LetStatement::LetStatement::name` (`field`)
          - `typeAnnotation`
            - `Beskid::Syntax::Nodes::LetStatement::LetStatement::typeAnnotation` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::LetStatement::LetStatement::value` (`field`)
          - `Beskid::Syntax::Nodes::LetStatement::LetStatement` (`type`)
        - `Beskid::Syntax::Nodes::LetStatement` (`module`)
      - `Literal`
        - `Literal`
          - `Bool`
            - `Beskid::Syntax::Nodes::Literal::Literal::Bool` (`enum_variant`)
          - `Char`
            - `Beskid::Syntax::Nodes::Literal::Literal::Char` (`enum_variant`)
          - `Float`
            - `Beskid::Syntax::Nodes::Literal::Literal::Float` (`enum_variant`)
          - `Integer`
            - `Beskid::Syntax::Nodes::Literal::Literal::Integer` (`enum_variant`)
          - `String`
            - `Beskid::Syntax::Nodes::Literal::Literal::String` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
          - `Beskid::Syntax::Nodes::Literal::Literal` (`enum`)
        - `Beskid::Syntax::Nodes::Literal` (`module`)
      - `LiteralExpression`
        - `LiteralExpression`
          - `literal`
            - `Beskid::Syntax::Nodes::LiteralExpression::LiteralExpression::literal` (`field`)
          - `Beskid::Syntax::Nodes::LiteralExpression::LiteralExpression` (`type`)
        - `Beskid::Syntax::Nodes::LiteralExpression` (`module`)
      - `MacroDefinition`
        - `MacroDefinition`
          - `body`
            - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::body` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::parameters` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition` (`type`)
        - `Beskid::Syntax::Nodes::MacroDefinition` (`module`)
      - `MacroFragmentKind`
        - `MacroFragmentKind`
          - `Block`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Block` (`enum_variant`)
          - `Expression`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Expression` (`enum_variant`)
          - `Identifier`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Identifier` (`enum_variant`)
          - `Item`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Item` (`enum_variant`)
          - `Literal`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Literal` (`enum_variant`)
          - `Node`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Node` (`enum_variant`)
          - `Path`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Path` (`enum_variant`)
          - `Pattern`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Pattern` (`enum_variant`)
          - `Statement`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Statement` (`enum_variant`)
          - `Type`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Type` (`enum_variant`)
          - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind` (`enum`)
        - `Beskid::Syntax::Nodes::MacroFragmentKind` (`module`)
      - `MacroInvocation`
        - `MacroInvocation`
          - `arguments`
            - `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::arguments` (`field`)
          - `block`
            - `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::block` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::name` (`field`)
          - `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation` (`type`)
        - `Beskid::Syntax::Nodes::MacroInvocation` (`module`)
      - `MacroMetavariable`
        - `MacroMetavariable`
          - `name`
            - `Beskid::Syntax::Nodes::MacroMetavariable::MacroMetavariable::name` (`field`)
          - `Beskid::Syntax::Nodes::MacroMetavariable::MacroMetavariable` (`type`)
        - `Beskid::Syntax::Nodes::MacroMetavariable` (`module`)
      - `MacroParameter`
        - `MacroParameter`
          - `kind`
            - `Beskid::Syntax::Nodes::MacroParameter::MacroParameter::kind` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::MacroParameter::MacroParameter::name` (`field`)
          - `Beskid::Syntax::Nodes::MacroParameter::MacroParameter` (`type`)
        - `Beskid::Syntax::Nodes::MacroParameter` (`module`)
      - `MacroParameterList`
        - `Beskid::Syntax::Nodes::MacroParameterList` (`module`)
      - `MatchArm`
        - `MatchArm`
          - `guard`
            - `Beskid::Syntax::Nodes::MatchArm::MatchArm::guard` (`field`)
          - `pattern`
            - `Beskid::Syntax::Nodes::MatchArm::MatchArm::pattern` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::MatchArm::MatchArm::value` (`field`)
          - `Beskid::Syntax::Nodes::MatchArm::MatchArm` (`type`)
        - `Beskid::Syntax::Nodes::MatchArm` (`module`)
      - `MatchArmList`
        - `Beskid::Syntax::Nodes::MatchArmList` (`module`)
      - `MatchExpression`
        - `MatchExpression`
          - `arms`
            - `Beskid::Syntax::Nodes::MatchExpression::MatchExpression::arms` (`field`)
          - `scrutinee`
            - `Beskid::Syntax::Nodes::MatchExpression::MatchExpression::scrutinee` (`field`)
          - `Beskid::Syntax::Nodes::MatchExpression::MatchExpression` (`type`)
        - `Beskid::Syntax::Nodes::MatchExpression` (`module`)
      - `MemberExpression`
        - `MemberExpression`
          - `member`
            - `Beskid::Syntax::Nodes::MemberExpression::MemberExpression::member` (`field`)
          - `target`
            - `Beskid::Syntax::Nodes::MemberExpression::MemberExpression::target` (`field`)
          - `Beskid::Syntax::Nodes::MemberExpression::MemberExpression` (`type`)
        - `Beskid::Syntax::Nodes::MemberExpression` (`module`)
      - `MethodDefinition`
        - `MethodDefinition`
          - `body`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::body` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::parameters` (`field`)
          - `receiverType`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::receiverType` (`field`)
          - `returnType`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::returnType` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition` (`type`)
        - `Beskid::Syntax::Nodes::MethodDefinition` (`module`)
      - `MethodDefinitionList`
        - `Beskid::Syntax::Nodes::MethodDefinitionList` (`module`)
      - `ModuleDeclaration`
        - `ModuleDeclaration`
          - `attributes`
            - `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::attributes` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::path` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::visibility` (`field`)
          - `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration` (`type`)
        - `Beskid::Syntax::Nodes::ModuleDeclaration` (`module`)
      - `Node`
        - `Node`
          - `Kind`
            - `Beskid::Syntax::Nodes::Node::Node::Kind` (`contract_method`)
          - `PushChildren`
            - `Beskid::Syntax::Nodes::Node::Node::PushChildren` (`contract_method`)
          - `Ref`
            - `Beskid::Syntax::Nodes::Node::Node::Ref` (`contract_method`)
          - `Span`
            - `Beskid::Syntax::Nodes::Node::Node::Span` (`contract_method`)
          - `sink`
            - `Beskid::Syntax::Nodes::Node::Node::sink` (`parameter`)
          - `Beskid::Syntax::Nodes::Node::Node` (`contract`)
        - `NodeChildSink`
          - `Push`
            - `Beskid::Syntax::Nodes::Node::NodeChildSink::Push` (`contract_method`)
          - `child`
            - `Beskid::Syntax::Nodes::Node::NodeChildSink::child` (`parameter`)
          - `Beskid::Syntax::Nodes::Node::NodeChildSink` (`contract`)
        - `Beskid::Syntax::Nodes::Node` (`module`)
      - `NodeKind`
        - `NodeKind`
          - `AssignExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AssignExpression` (`enum_variant`)
          - `Attribute`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Attribute` (`enum_variant`)
          - `AttributeArgument`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeArgument` (`enum_variant`)
          - `AttributeDeclaration`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeDeclaration` (`enum_variant`)
          - `AttributeParameter`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeParameter` (`enum_variant`)
          - `AttributeTarget`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeTarget` (`enum_variant`)
          - `BinaryExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::BinaryExpression` (`enum_variant`)
          - `BinaryOp`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::BinaryOp` (`enum_variant`)
          - `Block`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Block` (`enum_variant`)
          - `BlockExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::BlockExpression` (`enum_variant`)
          - `BreakStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::BreakStatement` (`enum_variant`)
          - `CallExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::CallExpression` (`enum_variant`)
          - `ContinueStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContinueStatement` (`enum_variant`)
          - `ContractDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractDefinition` (`enum_variant`)
          - `ContractEmbedding`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractEmbedding` (`enum_variant`)
          - `ContractMethodSignature`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractMethodSignature` (`enum_variant`)
          - `ContractNode`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractNode` (`enum_variant`)
          - `EnumConstructorExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumConstructorExpression` (`enum_variant`)
          - `EnumDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumDefinition` (`enum_variant`)
          - `EnumPath`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumPath` (`enum_variant`)
          - `EnumPattern`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumPattern` (`enum_variant`)
          - `EnumVariant`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumVariant` (`enum_variant`)
          - `Expression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Expression` (`enum_variant`)
          - `ExpressionStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ExpressionStatement` (`enum_variant`)
          - `ExtendTypeDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ExtendTypeDefinition` (`enum_variant`)
          - `Field`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Field` (`enum_variant`)
          - `ForStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ForStatement` (`enum_variant`)
          - `FunctionDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::FunctionDefinition` (`enum_variant`)
          - `GroupedExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::GroupedExpression` (`enum_variant`)
          - `HostBodyItem`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::HostBodyItem` (`enum_variant`)
          - `HostDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::HostDefinition` (`enum_variant`)
          - `Identifier`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Identifier` (`enum_variant`)
          - `IfStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::IfStatement` (`enum_variant`)
          - `InlineModule`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::InlineModule` (`enum_variant`)
          - `LambdaExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LambdaExpression` (`enum_variant`)
          - `LambdaParameter`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LambdaParameter` (`enum_variant`)
          - `LaunchStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LaunchStatement` (`enum_variant`)
          - `LetStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LetStatement` (`enum_variant`)
          - `Literal`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Literal` (`enum_variant`)
          - `LiteralExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LiteralExpression` (`enum_variant`)
          - `MacroDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroDefinition` (`enum_variant`)
          - `MacroFragmentKind`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroFragmentKind` (`enum_variant`)
          - `MacroInvocation`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroInvocation` (`enum_variant`)
          - `MacroMetavariable`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroMetavariable` (`enum_variant`)
          - `MacroParameter`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroParameter` (`enum_variant`)
          - `MatchArm`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MatchArm` (`enum_variant`)
          - `MatchExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MatchExpression` (`enum_variant`)
          - `MemberExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MemberExpression` (`enum_variant`)
          - `MethodDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MethodDefinition` (`enum_variant`)
          - `ModuleDeclaration`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ModuleDeclaration` (`enum_variant`)
          - `Node`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Node` (`enum_variant`)
          - `Parameter`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Parameter` (`enum_variant`)
          - `ParameterModifier`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ParameterModifier` (`enum_variant`)
          - `Path`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Path` (`enum_variant`)
          - `PathExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::PathExpression` (`enum_variant`)
          - `PathSegment`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::PathSegment` (`enum_variant`)
          - `Pattern`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Pattern` (`enum_variant`)
          - `PrimitiveType`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::PrimitiveType` (`enum_variant`)
          - `Program`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Program` (`enum_variant`)
          - `RangeExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::RangeExpression` (`enum_variant`)
          - `RegistryBlock`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::RegistryBlock` (`enum_variant`)
          - `RegistryEntry`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::RegistryEntry` (`enum_variant`)
          - `ReturnStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ReturnStatement` (`enum_variant`)
          - `ScopeDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ScopeDefinition` (`enum_variant`)
          - `ScopeHook`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ScopeHook` (`enum_variant`)
          - `SpawnExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::SpawnExpression` (`enum_variant`)
          - `Statement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Statement` (`enum_variant`)
          - `StructLiteralExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::StructLiteralExpression` (`enum_variant`)
          - `StructLiteralField`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::StructLiteralField` (`enum_variant`)
          - `TestDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestDefinition` (`enum_variant`)
          - `TestMetaSection`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestMetaSection` (`enum_variant`)
          - `TestMetadataEntry`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestMetadataEntry` (`enum_variant`)
          - `TestSkipEntry`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestSkipEntry` (`enum_variant`)
          - `TestSkipSection`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestSkipSection` (`enum_variant`)
          - `TryExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TryExpression` (`enum_variant`)
          - `Type`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Type` (`enum_variant`)
          - `TypeDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TypeDefinition` (`enum_variant`)
          - `UnaryExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::UnaryExpression` (`enum_variant`)
          - `UnaryOp`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::UnaryOp` (`enum_variant`)
          - `UseDeclaration`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::UseDeclaration` (`enum_variant`)
          - `Visibility`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Visibility` (`enum_variant`)
          - `WhileStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::WhileStatement` (`enum_variant`)
          - `WithStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::WithStatement` (`enum_variant`)
          - `Beskid::Syntax::Nodes::NodeKind::NodeKind` (`enum`)
        - `Beskid::Syntax::Nodes::NodeKind` (`module`)
      - `NodeList`
        - `Beskid::Syntax::Nodes::NodeList` (`module`)
      - `NodeRef`
        - `NodeRef`
          - `nodeId`
            - `Beskid::Syntax::Nodes::NodeRef::NodeRef::nodeId` (`field`)
          - `syntaxGenerationId`
            - `Beskid::Syntax::Nodes::NodeRef::NodeRef::syntaxGenerationId` (`field`)
          - `Beskid::Syntax::Nodes::NodeRef::NodeRef` (`type`)
        - `Beskid::Syntax::Nodes::NodeRef` (`module`)
      - `NodeSpan`
        - `NodeSpan`
          - `columnEnd`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::columnEnd` (`field`)
          - `columnStart`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::columnStart` (`field`)
          - `end`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::end` (`field`)
          - `lineEnd`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::lineEnd` (`field`)
          - `lineStart`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::lineStart` (`field`)
          - `start`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::start` (`field`)
          - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan` (`type`)
        - `Beskid::Syntax::Nodes::NodeSpan` (`module`)
      - `OptionList`
        - `Beskid::Syntax::Nodes::OptionList` (`module`)
      - `OptionalBlock`
        - `OptionalBlock`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalBlock` (`module`)
      - `OptionalExpression`
        - `OptionalExpression`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalExpression` (`module`)
      - `OptionalIdentifier`
        - `OptionalIdentifier`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalIdentifier` (`module`)
      - `OptionalInjectQualifier`
        - `OptionalInjectQualifier`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalInjectQualifier` (`module`)
      - `OptionalLeadingDocComment`
        - `OptionalLeadingDocComment`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalLeadingDocComment` (`module`)
      - `OptionalParameterModifier`
        - `OptionalParameterModifier`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalParameterModifier` (`module`)
      - `OptionalPath`
        - `OptionalPath`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalPath::OptionalPath` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalPath` (`module`)
      - `OptionalRegistrationLifetime`
        - `OptionalRegistrationLifetime`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime` (`module`)
      - `OptionalTestMetaSection`
        - `OptionalTestMetaSection`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalTestMetaSection` (`module`)
      - `OptionalTestSkipSection`
        - `OptionalTestSkipSection`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalTestSkipSection` (`module`)
      - `OptionalType`
        - `OptionalType`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalType::OptionalType::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalType::OptionalType::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalType::OptionalType::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalType::OptionalType` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalType` (`module`)
      - `Optionalusize`
        - `Optionalusize`
          - `None`
            - `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::payload` (`field`)
          - `Beskid::Syntax::Nodes::Optionalusize::Optionalusize` (`enum`)
        - `Beskid::Syntax::Nodes::Optionalusize` (`module`)
      - `Parameter`
        - `Parameter`
          - `modifier`
            - `Beskid::Syntax::Nodes::Parameter::Parameter::modifier` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::Parameter::Parameter::name` (`field`)
          - `ty`
            - `Beskid::Syntax::Nodes::Parameter::Parameter::ty` (`field`)
          - `Beskid::Syntax::Nodes::Parameter::Parameter` (`type`)
        - `Beskid::Syntax::Nodes::Parameter` (`module`)
      - `ParameterList`
        - `Beskid::Syntax::Nodes::ParameterList` (`module`)
      - `ParameterModifier`
        - `ParameterModifier`
          - `Out`
            - `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier::Out` (`enum_variant`)
          - `Ref`
            - `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier::Ref` (`enum_variant`)
          - `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier` (`enum`)
        - `Beskid::Syntax::Nodes::ParameterModifier` (`module`)
      - `Path`
        - `Path`
          - `segments`
            - `Beskid::Syntax::Nodes::Path::Path::segments` (`field`)
          - `Beskid::Syntax::Nodes::Path::Path` (`type`)
        - `Beskid::Syntax::Nodes::Path` (`module`)
      - `PathExpression`
        - `PathExpression`
          - `path`
            - `Beskid::Syntax::Nodes::PathExpression::PathExpression::path` (`field`)
          - `Beskid::Syntax::Nodes::PathExpression::PathExpression` (`type`)
        - `Beskid::Syntax::Nodes::PathExpression` (`module`)
      - `PathList`
        - `Beskid::Syntax::Nodes::PathList` (`module`)
      - `PathSegment`
        - `PathSegment`
          - `name`
            - `Beskid::Syntax::Nodes::PathSegment::PathSegment::name` (`field`)
          - `typeArgs`
            - `Beskid::Syntax::Nodes::PathSegment::PathSegment::typeArgs` (`field`)
          - `Beskid::Syntax::Nodes::PathSegment::PathSegment` (`type`)
        - `Beskid::Syntax::Nodes::PathSegment` (`module`)
      - `PathSegmentList`
        - `Beskid::Syntax::Nodes::PathSegmentList` (`module`)
      - `Pattern`
        - `Pattern`
          - `Enum`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::Enum` (`enum_variant`)
          - `Identifier`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::Identifier` (`enum_variant`)
          - `Literal`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::Literal` (`enum_variant`)
          - `Wildcard`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::Wildcard` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)
            - `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)
            - `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)
          - `Beskid::Syntax::Nodes::Pattern::Pattern` (`enum`)
        - `Beskid::Syntax::Nodes::Pattern` (`module`)
      - `PatternList`
        - `Beskid::Syntax::Nodes::PatternList` (`module`)
      - `PrimitiveType`
        - `PrimitiveType`
          - `Bool`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Bool` (`enum_variant`)
          - `Char`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Char` (`enum_variant`)
          - `F64`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::F64` (`enum_variant`)
          - `I32`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::I32` (`enum_variant`)
          - `I64`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::I64` (`enum_variant`)
          - `String`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::String` (`enum_variant`)
          - `U8`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::U8` (`enum_variant`)
          - `Unit`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Unit` (`enum_variant`)
          - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType` (`enum`)
        - `Beskid::Syntax::Nodes::PrimitiveType` (`module`)
      - `Program`
        - `Program`
          - `items`
            - `Beskid::Syntax::Nodes::Program::Program::items` (`field`)
          - `Beskid::Syntax::Nodes::Program::Program` (`type`)
        - `Beskid::Syntax::Nodes::Program` (`module`)
      - `RangeExpression`
        - `RangeExpression`
          - `end`
            - `Beskid::Syntax::Nodes::RangeExpression::RangeExpression::end` (`field`)
          - `start`
            - `Beskid::Syntax::Nodes::RangeExpression::RangeExpression::start` (`field`)
          - `Beskid::Syntax::Nodes::RangeExpression::RangeExpression` (`type`)
        - `Beskid::Syntax::Nodes::RangeExpression` (`module`)
      - `RegistrationLifetime`
        - `RegistrationLifetime`
          - `Single`
            - `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime::Single` (`enum_variant`)
          - `Transient`
            - `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime::Transient` (`enum_variant`)
          - `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime` (`enum`)
        - `Beskid::Syntax::Nodes::RegistrationLifetime` (`module`)
      - `RegistryBlock`
        - `RegistryBlock`
          - `entries`
            - `Beskid::Syntax::Nodes::RegistryBlock::RegistryBlock::entries` (`field`)
          - `Beskid::Syntax::Nodes::RegistryBlock::RegistryBlock` (`type`)
        - `Beskid::Syntax::Nodes::RegistryBlock` (`module`)
      - `RegistryEntry`
        - `RegistryEntry`
          - `implementation`
            - `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry::implementation` (`field`)
          - `target`
            - `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry::target` (`field`)
          - `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry` (`type`)
        - `Beskid::Syntax::Nodes::RegistryEntry` (`module`)
      - `RegistryEntryList`
        - `Beskid::Syntax::Nodes::RegistryEntryList` (`module`)
      - `ReturnStatement`
        - `ReturnStatement`
          - `value`
            - `Beskid::Syntax::Nodes::ReturnStatement::ReturnStatement::value` (`field`)
          - `Beskid::Syntax::Nodes::ReturnStatement::ReturnStatement` (`type`)
        - `Beskid::Syntax::Nodes::ReturnStatement` (`module`)
      - `ScopeDefinition`
        - `ScopeDefinition`
          - `body`
            - `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::body` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::parameters` (`field`)
          - `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition` (`type`)
        - `Beskid::Syntax::Nodes::ScopeDefinition` (`module`)
      - `ScopeHook`
        - `ScopeHook`
          - `body`
            - `Beskid::Syntax::Nodes::ScopeHook::ScopeHook::body` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::ScopeHook::ScopeHook::parameters` (`field`)
          - `Beskid::Syntax::Nodes::ScopeHook::ScopeHook` (`type`)
        - `Beskid::Syntax::Nodes::ScopeHook` (`module`)
      - `ScopeHookKind`
        - `ScopeHookKind`
          - `Dispose`
            - `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Dispose` (`enum_variant`)
          - `Init`
            - `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Init` (`enum_variant`)
          - `Startup`
            - `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Startup` (`enum_variant`)
          - `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind` (`enum`)
        - `Beskid::Syntax::Nodes::ScopeHookKind` (`module`)
      - `SpawnExpression`
        - `SpawnExpression`
          - `callee`
            - `Beskid::Syntax::Nodes::SpawnExpression::SpawnExpression::callee` (`field`)
          - `Beskid::Syntax::Nodes::SpawnExpression::SpawnExpression` (`type`)
        - `Beskid::Syntax::Nodes::SpawnExpression` (`module`)
      - `Statement`
        - `Statement`
          - `Break`
            - `Beskid::Syntax::Nodes::Statement::Statement::Break` (`enum_variant`)
          - `Continue`
            - `Beskid::Syntax::Nodes::Statement::Statement::Continue` (`enum_variant`)
          - `Expression`
            - `Beskid::Syntax::Nodes::Statement::Statement::Expression` (`enum_variant`)
          - `For`
            - `Beskid::Syntax::Nodes::Statement::Statement::For` (`enum_variant`)
          - `If`
            - `Beskid::Syntax::Nodes::Statement::Statement::If` (`enum_variant`)
          - `Launch`
            - `Beskid::Syntax::Nodes::Statement::Statement::Launch` (`enum_variant`)
          - `Let`
            - `Beskid::Syntax::Nodes::Statement::Statement::Let` (`enum_variant`)
          - `Return`
            - `Beskid::Syntax::Nodes::Statement::Statement::Return` (`enum_variant`)
          - `While`
            - `Beskid::Syntax::Nodes::Statement::Statement::While` (`enum_variant`)
          - `With`
            - `Beskid::Syntax::Nodes::Statement::Statement::With` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
          - `Beskid::Syntax::Nodes::Statement::Statement` (`enum`)
        - `Beskid::Syntax::Nodes::Statement` (`module`)
      - `StatementList`
        - `Beskid::Syntax::Nodes::StatementList` (`module`)
      - `StructLiteralExpression`
        - `StructLiteralExpression`
          - `fields`
            - `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression::fields` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression::path` (`field`)
          - `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression` (`type`)
        - `Beskid::Syntax::Nodes::StructLiteralExpression` (`module`)
      - `StructLiteralField`
        - `StructLiteralField`
          - `name`
            - `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField::name` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField::value` (`field`)
          - `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField` (`type`)
        - `Beskid::Syntax::Nodes::StructLiteralField` (`module`)
      - `StructLiteralFieldList`
        - `Beskid::Syntax::Nodes::StructLiteralFieldList` (`module`)
      - `TestDefinition`
        - `TestDefinition`
          - `_meta`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::_meta` (`field`)
          - `_skip`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::_skip` (`field`)
          - `attributes`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::attributes` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::name` (`field`)
          - `statements`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::statements` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition` (`type`)
        - `Beskid::Syntax::Nodes::TestDefinition` (`module`)
      - `TestMetaSection`
        - `TestMetaSection`
          - `entries`
            - `Beskid::Syntax::Nodes::TestMetaSection::TestMetaSection::entries` (`field`)
          - `Beskid::Syntax::Nodes::TestMetaSection::TestMetaSection` (`type`)
        - `Beskid::Syntax::Nodes::TestMetaSection` (`module`)
      - `TestMetadataEntry`
        - `TestMetadataEntry`
          - `name`
            - `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry::name` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry::value` (`field`)
          - `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry` (`type`)
        - `Beskid::Syntax::Nodes::TestMetadataEntry` (`module`)
      - `TestMetadataEntryList`
        - `Beskid::Syntax::Nodes::TestMetadataEntryList` (`module`)
      - `TestSkipEntry`
        - `TestSkipEntry`
          - `name`
            - `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry::name` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry::value` (`field`)
          - `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry` (`type`)
        - `Beskid::Syntax::Nodes::TestSkipEntry` (`module`)
      - `TestSkipEntryList`
        - `Beskid::Syntax::Nodes::TestSkipEntryList` (`module`)
      - `TestSkipSection`
        - `TestSkipSection`
          - `entries`
            - `Beskid::Syntax::Nodes::TestSkipSection::TestSkipSection::entries` (`field`)
          - `Beskid::Syntax::Nodes::TestSkipSection::TestSkipSection` (`type`)
        - `Beskid::Syntax::Nodes::TestSkipSection` (`module`)
      - `TraversalManifest`
        - `Beskid::Syntax::Nodes::TraversalManifest` (`module`)
      - `TryExpression`
        - `TryExpression`
          - `expr`
            - `Beskid::Syntax::Nodes::TryExpression::TryExpression::expr` (`field`)
          - `Beskid::Syntax::Nodes::TryExpression::TryExpression` (`type`)
        - `Beskid::Syntax::Nodes::TryExpression` (`module`)
      - `Type`
        - `Type`
          - `Array`
            - `Beskid::Syntax::Nodes::Type::Type::Array` (`enum_variant`)
          - `Complex`
            - `Beskid::Syntax::Nodes::Type::Type::Complex` (`enum_variant`)
          - `Function`
            - `Beskid::Syntax::Nodes::Type::Type::Function` (`enum_variant`)
          - `Primitive`
            - `Beskid::Syntax::Nodes::Type::Type::Primitive` (`enum_variant`)
          - `Ref`
            - `Beskid::Syntax::Nodes::Type::Type::Ref` (`enum_variant`)
          - `parameters`
            - `Beskid::Syntax::Nodes::Type::Type::parameters` (`field`)
          - `payload`
            - `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)
            - `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)
            - `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)
            - `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)
          - `returnType`
            - `Beskid::Syntax::Nodes::Type::Type::returnType` (`field`)
          - `Beskid::Syntax::Nodes::Type::Type` (`enum`)
        - `Beskid::Syntax::Nodes::Type` (`module`)
      - `TypeDefinition`
        - `TypeDefinition`
          - `conformances`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::conformances` (`field`)
          - `fields`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::fields` (`field`)
          - `generics`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::generics` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::name` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition` (`type`)
        - `Beskid::Syntax::Nodes::TypeDefinition` (`module`)
      - `TypeList`
        - `Beskid::Syntax::Nodes::TypeList` (`module`)
      - `UnaryExpression`
        - `UnaryExpression`
          - `expr`
            - `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression::expr` (`field`)
          - `op`
            - `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression::op` (`field`)
          - `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression` (`type`)
        - `Beskid::Syntax::Nodes::UnaryExpression` (`module`)
      - `UnaryOp`
        - `UnaryOp`
          - `Neg`
            - `Beskid::Syntax::Nodes::UnaryOp::UnaryOp::Neg` (`enum_variant`)
          - `Not`
            - `Beskid::Syntax::Nodes::UnaryOp::UnaryOp::Not` (`enum_variant`)
          - `Beskid::Syntax::Nodes::UnaryOp::UnaryOp` (`enum`)
        - `Beskid::Syntax::Nodes::UnaryOp` (`module`)
      - `UseDeclaration`
        - `UseDeclaration`
          - `alias`
            - `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::alias` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::path` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::visibility` (`field`)
          - `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration` (`type`)
        - `Beskid::Syntax::Nodes::UseDeclaration` (`module`)
      - `Visibility`
        - `Visibility`
          - `Private`
            - `Beskid::Syntax::Nodes::Visibility::Visibility::Private` (`enum_variant`)
          - `Public`
            - `Beskid::Syntax::Nodes::Visibility::Visibility::Public` (`enum_variant`)
          - `Beskid::Syntax::Nodes::Visibility::Visibility` (`enum`)
        - `Beskid::Syntax::Nodes::Visibility` (`module`)
      - `Visit`
        - `SyntaxVisitor`
          - `Enter`
            - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::Enter` (`contract_method`)
          - `Exit`
            - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::Exit` (`contract_method`)
          - `node`
            - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::node` (`parameter`)
            - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::node` (`parameter`)
          - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor` (`contract`)
        - `Beskid::Syntax::Nodes::Visit` (`module`)
      - `WhileStatement`
        - `WhileStatement`
          - `body`
            - `Beskid::Syntax::Nodes::WhileStatement::WhileStatement::body` (`field`)
          - `condition`
            - `Beskid::Syntax::Nodes::WhileStatement::WhileStatement::condition` (`field`)
          - `Beskid::Syntax::Nodes::WhileStatement::WhileStatement` (`type`)
        - `Beskid::Syntax::Nodes::WhileStatement` (`module`)
      - `WithStatement`
        - `WithStatement`
          - `arguments`
            - `Beskid::Syntax::Nodes::WithStatement::WithStatement::arguments` (`field`)
          - `body`
            - `Beskid::Syntax::Nodes::WithStatement::WithStatement::body` (`field`)
          - `scopeName`
            - `Beskid::Syntax::Nodes::WithStatement::WithStatement::scopeName` (`field`)
          - `Beskid::Syntax::Nodes::WithStatement::WithStatement` (`type`)
        - `Beskid::Syntax::Nodes::WithStatement` (`module`)
      - `Beskid::Syntax::Nodes` (`module`)
    - `SyntaxFacadeVersion`
      - `Beskid::Syntax::SyntaxFacadeVersion` (`function`)
    - `Beskid::Syntax` (`module`)
- `__alloc`
  - `__alloc` (`function`)
- `__array_len`
  - `__array_len` (`function`)
- `__array_new`
  - `__array_new` (`function`)
- `__channel_close`
  - `__channel_close` (`function`)
- `__channel_create`
  - `__channel_create` (`function`)
- `__channel_receive`
  - `__channel_receive` (`function`)
- `__channel_receive_value`
  - `__channel_receive_value` (`function`)
- `__channel_send`
  - `__channel_send` (`function`)
- `__channel_try_receive`
  - `__channel_try_receive` (`function`)
- `__channel_try_send`
  - `__channel_try_send` (`function`)
- `__fiber_cancel`
  - `__fiber_cancel` (`function`)
- `__fiber_current_id`
  - `__fiber_current_id` (`function`)
- `__fiber_detach`
  - `__fiber_detach` (`function`)
- `__fiber_join`
  - `__fiber_join` (`function`)
- `__fiber_join_value`
  - `__fiber_join_value` (`function`)
- `__fiber_now_millis`
  - `__fiber_now_millis` (`function`)
- `__fiber_processor_count`
  - `__fiber_processor_count` (`function`)
- `__fiber_spawn`
  - `__fiber_spawn` (`function`)
- `__fiber_spawn_with_cancel_slot`
  - `__fiber_spawn_with_cancel_slot` (`function`)
- `__fiber_yield`
  - `__fiber_yield` (`function`)
- `__gc_register_root`
  - `__gc_register_root` (`function`)
- `__gc_root_handle`
  - `__gc_root_handle` (`function`)
- `__gc_unregister_root`
  - `__gc_unregister_root` (`function`)
- `__gc_unroot_handle`
  - `__gc_unroot_handle` (`function`)
- `__gc_write_barrier`
  - `__gc_write_barrier` (`function`)
- `__hub_create`
  - `__hub_create` (`function`)
- `__hub_register`
  - `__hub_register` (`function`)
- `__hub_unregister`
  - `__hub_unregister` (`function`)
- `__hub_wait_receive`
  - `__hub_wait_receive` (`function`)
- `__hub_wait_receive_index`
  - `__hub_wait_receive_index` (`function`)
- `__hub_wait_receive_value`
  - `__hub_wait_receive_value` (`function`)
- `__interop_dispatch_ptr`
  - `__interop_dispatch_ptr` (`function`)
- `__interop_dispatch_unit`
  - `__interop_dispatch_unit` (`function`)
- `__interop_dispatch_usize`
  - `__interop_dispatch_usize` (`function`)
- `__mutex_create`
  - `__mutex_create` (`function`)
- `__mutex_lock`
  - `__mutex_lock` (`function`)
- `__mutex_try_lock`
  - `__mutex_try_lock` (`function`)
- `__mutex_unlock`
  - `__mutex_unlock` (`function`)
- `__panic_str`
  - `__panic_str` (`function`)
- `__str_len`
  - `__str_len` (`function`)
- `__str_new`
  - `__str_new` (`function`)
- `__syscall_read`
  - `__syscall_read` (`function`)
- `__syscall_write`
  - `__syscall_write` (`function`)
- `__test_bytes_len`
  - `__test_bytes_len` (`function`)
- `__test_bytes_ptr`
  - `__test_bytes_ptr` (`function`)
- `__wait_group_add`
  - `__wait_group_add` (`function`)
- `__wait_group_create`
  - `__wait_group_create` (`function`)
- `__wait_group_done`
  - `__wait_group_done` (`function`)
- `__wait_group_wait`
  - `__wait_group_wait` (`function`)
- `range`
  - `range` (`function`)

## Items

### `Beskid::Compiler::Collect` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AnalysisRequest` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AnalysisResult` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Analyzer` (`contract`)

Post-semantic diagnostic and rewrite-registration entrypoint.

---

### `Beskid::Compiler::Collect::Analyzer::Analyze` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Analyzer::request` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AttributeDeclarationSet` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AttributeGenerationRequest` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AttributeGenerator` (`contract`)

Attribute declarations exported by Mod packages.

---

### `Beskid::Compiler::Collect::AttributeGenerator::Attributes` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AttributeGenerator::request` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::CollectFacadeVersion` (`function`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::CollectRequest` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::CollectTargetSet` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Collector` (`contract`)

Declarative target collection and scope narrowing for a Mod instance.

---

### `Beskid::Compiler::Collect::Collector::Collect` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Collector::request` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::FixError` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::GeneratedSyntaxContribution` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::GenerationRequest` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Generator` (`contract`)

Incremental typed AST contribution entrypoint.

---

### `Beskid::Compiler::Collect::Generator::Generate` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Generator::request` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Rewriter` (`contract`)

Typed replacement contract. TSourceNode and TTargetNode are SDK type parameters until contract generics are admitted by the grammar.

---

### `Beskid::Compiler::Collect::Rewriter::Rewrite` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Rewriter::sourceNode` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::CompilerLanguageVersionToken` (`function`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ModSdkCompilationSurfaceVersion` (`function`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane` (`enum`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::Hir` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::SemanticSnapshot` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::SyntaxTree` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::SemanticSnapshotFamilyToken` (`function`)

*No documentation provided.*

---

### `Beskid::Compiler::Diagnostics` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::Query` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind` (`enum`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::DiagnosticAttachment` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::LoweringDirective` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::Metadata` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::TypedEmitterFacadeVersion` (`function`)

*No documentation provided.*

---

### `Beskid::Syntax` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ArrayLiteralExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ArrayLiteralExpression::ArrayLiteralExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/array_literal_expression.rs` — `ArrayLiteralExpression`.

**Rust documentation** (from mirrored type):
`[elem0, elem1, ...]` — array literal expression.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `elements`.

---

### `Beskid::Syntax::Nodes::ArrayLiteralExpression::ArrayLiteralExpression::elements` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignExpression::AssignExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/assign_expression.rs` — `AssignExpression`.

**Rust documentation** (from mirrored type):
Assignment or compound assignment (`=`, `+=`, `-=`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `target`, `value`.

---

### `Beskid::Syntax::Nodes::AssignExpression::AssignExpression::target` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignExpression::AssignExpression::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignOp` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignOp::AssignOp` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/assign_expression.rs` — `AssignOp`.

**Rust documentation** (from mirrored type):
Compound or simple assignment operator token.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Assign`**
unit (no payload)


**Variant `AddAssign`**
unit (no payload)


**Variant `SubAssign`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::AssignOp::AssignOp::AddAssign` (`enum_variant`)



**Variant `AddAssign`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::AssignOp::AssignOp::Assign` (`enum_variant`)



**Variant `Assign`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::AssignOp::AssignOp::SubAssign` (`enum_variant`)



**Variant `SubAssign`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Attribute` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Attribute::Attribute` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `Attribute`.

**Rust documentation** (from mirrored type):
Attribute instance with optional named arguments (`Name(arg = value, ...)`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `arguments`.

---

### `Beskid::Syntax::Nodes::Attribute::Attribute::arguments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Attribute::Attribute::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeArgument` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `AttributeArgument`.

**Rust documentation** (from mirrored type):
Named argument supplied when applying an attribute.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `value`.

---

### `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeArgumentList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `AttributeDeclaration`.

**Rust documentation** (from mirrored type):
Declaration of a reusable attribute kind (targets and parameters).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `targets`, `parameters`.

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::targets` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameter` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `AttributeParameter`.

**Rust documentation** (from mirrored type):
Parameter slot on an attribute declaration (name, type, optional default).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `ty`, `defaultValue`.

---

### `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::defaultValue` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::ty` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameterList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeTarget` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeTarget::AttributeTarget` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `AttributeTarget`.

**Rust documentation** (from mirrored type):
Syntactic placement target for an attribute (`fn`, `type`, ...).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`.

---

### `Beskid::Syntax::Nodes::AttributeTarget::AttributeTarget::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeTargetList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/binary_expression.rs` — `BinaryExpression`.

**Rust documentation** (from mirrored type):
Binary operator expression with left and right operands.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `left`, `op`, `right`.

---

### `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::left` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::op` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::right` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryOp` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/binary_expression.rs` — `BinaryOp`.

**Rust documentation** (from mirrored type):
Supported binary operators (logical, comparison, arithmetic).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Or`**
unit (no payload)


**Variant `And`**
unit (no payload)


**Variant `IdentityEq`**
unit (no payload)


**Variant `IdentityNotEq`**
unit (no payload)


**Variant `Eq`**
unit (no payload)


**Variant `NotEq`**
unit (no payload)


**Variant `Lt`**
unit (no payload)


**Variant `Lte`**
unit (no payload)


**Variant `Gt`**
unit (no payload)


**Variant `Gte`**
unit (no payload)


**Variant `Add`**
unit (no payload)


**Variant `Sub`**
unit (no payload)


**Variant `Mul`**
unit (no payload)


**Variant `Div`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Add` (`enum_variant`)



**Variant `Add`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::And` (`enum_variant`)



**Variant `And`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Div` (`enum_variant`)



**Variant `Div`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Eq` (`enum_variant`)



**Variant `Eq`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Gt` (`enum_variant`)



**Variant `Gt`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Gte` (`enum_variant`)



**Variant `Gte`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::IdentityEq` (`enum_variant`)



**Variant `IdentityEq`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::IdentityNotEq` (`enum_variant`)



**Variant `IdentityNotEq`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Lt` (`enum_variant`)



**Variant `Lt`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Lte` (`enum_variant`)



**Variant `Lte`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Mul` (`enum_variant`)



**Variant `Mul`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::NotEq` (`enum_variant`)



**Variant `NotEq`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Or` (`enum_variant`)



**Variant `Or`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Sub` (`enum_variant`)



**Variant `Sub`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Block` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Block::Block` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/block.rs` — `Block`.

**Rust documentation** (from mirrored type):
Braced sequence of statements.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `statements`.

---

### `Beskid::Syntax::Nodes::Block::Block::statements` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BlockExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BlockExpression::BlockExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/block_expression.rs` — `BlockExpression`.

**Rust documentation** (from mirrored type):
Block used as an expression (`{ ... }` value).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `block`.

---

### `Beskid::Syntax::Nodes::BlockExpression::BlockExpression::block` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BreakStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BreakStatement::BreakStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/break_statement.rs` — `BreakStatement`.

**Rust documentation** (from mirrored type):
`break` out of the nearest enclosing loop.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Marker struct with no fields.

---

### `Beskid::Syntax::Nodes::CallExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::CallExpression::CallExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/call_expression.rs` — `CallExpression`.

**Rust documentation** (from mirrored type):
Function- or method-style call with positional arguments.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `callee`, `args`.

---

### `Beskid::Syntax::Nodes::CallExpression::CallExpression::args` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::CallExpression::CallExpression::callee` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContinueStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContinueStatement::ContinueStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/continue_statement.rs` — `ContinueStatement`.

**Rust documentation** (from mirrored type):
`continue` to the next iteration of the nearest enclosing loop.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Marker struct with no fields.

---

### `Beskid::Syntax::Nodes::ContractDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/contract_definition.rs` — `ContractDefinition`.

**Rust documentation** (from mirrored type):
`contract` interface: members (method signatures and embeddings) with per-item docs.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `name`, `items`.

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::items` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractEmbedding` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractEmbedding::ContractEmbedding` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/contract_embedding.rs` — `ContractEmbedding`.

**Rust documentation** (from mirrored type):
Contract member that embeds another contract by name.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`.

---

### `Beskid::Syntax::Nodes::ContractEmbedding::ContractEmbedding::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractMethodSignature` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/contract_method_signature.rs` — `ContractMethodSignature`.

**Rust documentation** (from mirrored type):
Abstract method signature inside a `contract` (no body).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `parameters`, `returnType`.

---

### `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::returnType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractNode` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/contract_node.rs` — `ContractNode`.

**Rust documentation** (from mirrored type):
Member of a contract: method signature or embedding.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `MethodSignature`**
tuple (payload: Beskid.Syntax.Nodes.ContractMethodSignature)


**Variant `Embedding`**
tuple (payload: Beskid.Syntax.Nodes.ContractEmbedding)


---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode::Embedding` (`enum_variant`)



**Variant `Embedding`**
tuple payload: payload (Beskid.Syntax.Nodes.ContractEmbedding).


---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode::MethodSignature` (`enum_variant`)



**Variant `MethodSignature`**
tuple payload: payload (Beskid.Syntax.Nodes.ContractMethodSignature).


---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractNodeList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Descendants` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Descendants::Descendants` (`contract`)

Pre-order descendant iterator contract (lowers to `beskid_analysis::query::Descendants`).

---

### `Beskid::Syntax::Nodes::Descendants::Descendants::Current` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Descendants::Descendants::MoveNext` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumConstructorExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/enum_constructor_expression.rs` — `EnumConstructorExpression`.

**Rust documentation** (from mirrored type):
Enum variant construction `Type.Variant(args...)`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `path`, `args`.

---

### `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression::args` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/enum_definition.rs` — `EnumDefinition`.

**Rust documentation** (from mirrored type):
`enum` definition with variants and optional generic parameters.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `generics`, `variants`.

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::generics` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::variants` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPath` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPath::EnumPath` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/enum_path.rs` — `EnumPath`.

**Rust documentation** (from mirrored type):
Qualified path naming an enum variant (`Module.Type::Variant`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `typePath`, `variant`.

---

### `Beskid::Syntax::Nodes::EnumPath::EnumPath::typePath` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPath::EnumPath::variant` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPattern` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPattern::EnumPattern` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/pattern.rs` — `EnumPattern`.

**Rust documentation** (from mirrored type):
Enum variant pattern with optional nested sub-patterns.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `path`, `items`.

---

### `Beskid::Syntax::Nodes::EnumPattern::EnumPattern::items` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPattern::EnumPattern::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumVariant` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumVariant::EnumVariant` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/enum_variant.rs` — `EnumVariant`.

**Rust documentation** (from mirrored type):
Single enum variant and its field list.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `fields`.

---

### `Beskid::Syntax::Nodes::EnumVariant::EnumVariant::fields` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumVariant::EnumVariant::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumVariantList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/expression.rs` — `Expression`.

**Rust documentation** (from mirrored type):
Top-level expression shape after parsing (postfix chains, operators, literals, etc.).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Match`**
tuple (payload: Beskid.Syntax.Nodes.MatchExpression)


**Variant `Lambda`**
tuple (payload: Beskid.Syntax.Nodes.LambdaExpression)


**Variant `Assign`**
tuple (payload: Beskid.Syntax.Nodes.AssignExpression)


**Variant `Binary`**
tuple (payload: Beskid.Syntax.Nodes.BinaryExpression)


**Variant `Unary`**
tuple (payload: Beskid.Syntax.Nodes.UnaryExpression)


**Variant `Call`**
tuple (payload: Beskid.Syntax.Nodes.CallExpression)


**Variant `Member`**
tuple (payload: Beskid.Syntax.Nodes.MemberExpression)


**Variant `Literal`**
tuple (payload: Beskid.Syntax.Nodes.LiteralExpression)


**Variant `Path`**
tuple (payload: Beskid.Syntax.Nodes.PathExpression)


**Variant `StructLiteral`**
tuple (payload: Beskid.Syntax.Nodes.StructLiteralExpression)


**Variant `EnumConstructor`**
tuple (payload: Beskid.Syntax.Nodes.EnumConstructorExpression)


**Variant `Block`**
tuple (payload: Beskid.Syntax.Nodes.BlockExpression)


**Variant `Grouped`**
tuple (payload: Beskid.Syntax.Nodes.GroupedExpression)


**Variant `Try`**
tuple (payload: Beskid.Syntax.Nodes.TryExpression)


**Variant `Spawn`**
tuple (payload: Beskid.Syntax.Nodes.SpawnExpression)


**Variant `MacroInvocation`**
tuple (payload: Beskid.Syntax.Nodes.MacroInvocation)


**Variant `MacroMetavariable`**
tuple (payload: Beskid.Syntax.Nodes.MacroMetavariable)


**Variant `Index`**
tuple (payload: Beskid.Syntax.Nodes.IndexExpression)


**Variant `ArrayLiteral`**
tuple (payload: Beskid.Syntax.Nodes.ArrayLiteralExpression)


---

### `Beskid::Syntax::Nodes::Expression::Expression::ArrayLiteral` (`enum_variant`)



**Variant `ArrayLiteral`**
tuple payload: payload (Beskid.Syntax.Nodes.ArrayLiteralExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Assign` (`enum_variant`)



**Variant `Assign`**
tuple payload: payload (Beskid.Syntax.Nodes.AssignExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Binary` (`enum_variant`)



**Variant `Binary`**
tuple payload: payload (Beskid.Syntax.Nodes.BinaryExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Block` (`enum_variant`)



**Variant `Block`**
tuple payload: payload (Beskid.Syntax.Nodes.BlockExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Call` (`enum_variant`)



**Variant `Call`**
tuple payload: payload (Beskid.Syntax.Nodes.CallExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::EnumConstructor` (`enum_variant`)



**Variant `EnumConstructor`**
tuple payload: payload (Beskid.Syntax.Nodes.EnumConstructorExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Grouped` (`enum_variant`)



**Variant `Grouped`**
tuple payload: payload (Beskid.Syntax.Nodes.GroupedExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Index` (`enum_variant`)



**Variant `Index`**
tuple payload: payload (Beskid.Syntax.Nodes.IndexExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Lambda` (`enum_variant`)



**Variant `Lambda`**
tuple payload: payload (Beskid.Syntax.Nodes.LambdaExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Literal` (`enum_variant`)



**Variant `Literal`**
tuple payload: payload (Beskid.Syntax.Nodes.LiteralExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::MacroInvocation` (`enum_variant`)



**Variant `MacroInvocation`**
tuple payload: payload (Beskid.Syntax.Nodes.MacroInvocation).


---

### `Beskid::Syntax::Nodes::Expression::Expression::MacroMetavariable` (`enum_variant`)



**Variant `MacroMetavariable`**
tuple payload: payload (Beskid.Syntax.Nodes.MacroMetavariable).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Match` (`enum_variant`)



**Variant `Match`**
tuple payload: payload (Beskid.Syntax.Nodes.MatchExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Member` (`enum_variant`)



**Variant `Member`**
tuple payload: payload (Beskid.Syntax.Nodes.MemberExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Path` (`enum_variant`)



**Variant `Path`**
tuple payload: payload (Beskid.Syntax.Nodes.PathExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Spawn` (`enum_variant`)



**Variant `Spawn`**
tuple payload: payload (Beskid.Syntax.Nodes.SpawnExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::StructLiteral` (`enum_variant`)



**Variant `StructLiteral`**
tuple payload: payload (Beskid.Syntax.Nodes.StructLiteralExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Try` (`enum_variant`)



**Variant `Try`**
tuple payload: payload (Beskid.Syntax.Nodes.TryExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Unary` (`enum_variant`)



**Variant `Unary`**
tuple payload: payload (Beskid.Syntax.Nodes.UnaryExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExpressionList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExpressionStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExpressionStatement::ExpressionStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/expression_statement.rs` — `ExpressionStatement`.

**Rust documentation** (from mirrored type):
Statement that evaluates an expression for side effects (typically terminated with `;`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `expression`.

---

### `Beskid::Syntax::Nodes::ExpressionStatement::ExpressionStatement::expression` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExtendTypeDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/extend_type.rs` — `ExtendTypeDefinition`.

**Rust documentation** (from mirrored type):
`extend type T { ... }` block preserving source grouping for extension semantics.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `targetType`, `methods`.

---

### `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition::methods` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition::targetType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Field` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Field::Field` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/field.rs` — `Field`.

**Rust documentation** (from mirrored type):
Struct or enum variant field with name and type (and optional event capacity).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `ty`.

---

### `Beskid::Syntax::Nodes::Field::Field::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Field::Field::ty` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Field::Field::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FieldKind` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FieldKind::FieldKind` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/field.rs` — `FieldKind`.

**Rust documentation** (from mirrored type):
Distinguishes ordinary value fields from event/signal-style fields.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Value`**
unit (no payload)


**Variant `Event`**
unit (no payload)


**Variant `Injected`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::FieldKind::FieldKind::Event` (`enum_variant`)



**Variant `Event`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::FieldKind::FieldKind::Injected` (`enum_variant`)



**Variant `Injected`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::FieldKind::FieldKind::Value` (`enum_variant`)



**Variant `Value`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::FieldList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ForStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ForStatement::ForStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/for_statement.rs` — `ForStatement`.

**Rust documentation** (from mirrored type):
`for` loop over an iterable value.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `iterator`, `iterable`, `body`.

---

### `Beskid::Syntax::Nodes::ForStatement::ForStatement::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ForStatement::ForStatement::iterable` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ForStatement::ForStatement::iterator` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/function_definition.rs` — `FunctionDefinition`.

**Rust documentation** (from mirrored type):
Top-level or nested function: visibility, signature, and body block.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `name`, `generics`, `parameters`, `returnType`, `body`.

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::generics` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::returnType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::GroupedExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::GroupedExpression::GroupedExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/grouped_expression.rs` — `GroupedExpression`.

**Rust documentation** (from mirrored type):
Parenthesized subexpression (grouping / precedence).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `expr`.

---

### `Beskid::Syntax::Nodes::GroupedExpression::GroupedExpression::expr` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostBodyItem` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostBodyItemList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `HostDefinition`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `parameters`, `baseHost`, `body`.

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::baseHost` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Identifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Identifier::Identifier` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/common/identifier.rs` — `Identifier`.

**Rust documentation** (from mirrored type):
Unqualified identifier as parsed from source (name text only).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`.

---

### `Beskid::Syntax::Nodes::Identifier::Identifier::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IdentifierList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IfStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IfStatement::IfStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/if_statement.rs` — `IfStatement`.

**Rust documentation** (from mirrored type):
Conditional with mandatory then-block and optional `else` block.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `condition`, `thenBlock`, `elseBlock`.

---

### `Beskid::Syntax::Nodes::IfStatement::IfStatement::condition` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IfStatement::IfStatement::elseBlock` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IfStatement::IfStatement::thenBlock` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IndexExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IndexExpression::IndexExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/index_expression.rs` — `IndexExpression`.

**Rust documentation** (from mirrored type):
`expr[index]` — array/string element access.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `target`, `index`.

---

### `Beskid::Syntax::Nodes::IndexExpression::IndexExpression::index` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IndexExpression::IndexExpression::target` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InjectQualifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `InjectQualifier`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Global`**
unit (no payload)


**Variant `Parent`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier::Global` (`enum_variant`)



**Variant `Global`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier::Parent` (`enum_variant`)



**Variant `Parent`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::InlineModule` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/inline_module.rs` — `InlineModule`.

**Rust documentation** (from mirrored type):
Inline `module Name { ... }` with nested items and optional leading docs per item.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `name`, `items`.

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule::items` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/lambda_expression.rs` — `LambdaExpression`.

**Rust documentation** (from mirrored type):
Anonymous function expression (`params => body`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `parameters`, `body`.

---

### `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaParameter` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/lambda_expression.rs` — `LambdaParameter`.

**Rust documentation** (from mirrored type):
Single lambda parameter, optionally with an explicit type.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `ty`.

---

### `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter::ty` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaParameterList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LaunchStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `LaunchStatement`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `hostPath`, `arguments`.

---

### `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement::arguments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement::hostPath` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LetStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LetStatement::LetStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/let_statement.rs` — `LetStatement`.

**Rust documentation** (from mirrored type):
Local binding with optional type annotation and mandatory initializer.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `typeAnnotation`, `value`.

---

### `Beskid::Syntax::Nodes::LetStatement::LetStatement::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LetStatement::LetStatement::typeAnnotation` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LetStatement::LetStatement::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/literal.rs` — `Literal`.

**Rust documentation** (from mirrored type):
Literal token; numeric and text forms keep raw source text where precision matters.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Integer`**
tuple (payload: string)


**Variant `Float`**
tuple (payload: string)


**Variant `String`**
tuple (payload: string)


**Variant `Char`**
tuple (payload: string)


**Variant `Bool`**
tuple (payload: bool)


---

### `Beskid::Syntax::Nodes::Literal::Literal::Bool` (`enum_variant`)



**Variant `Bool`**
tuple payload: payload (bool).


---

### `Beskid::Syntax::Nodes::Literal::Literal::Char` (`enum_variant`)



**Variant `Char`**
tuple payload: payload (string).


---

### `Beskid::Syntax::Nodes::Literal::Literal::Float` (`enum_variant`)



**Variant `Float`**
tuple payload: payload (string).


---

### `Beskid::Syntax::Nodes::Literal::Literal::Integer` (`enum_variant`)



**Variant `Integer`**
tuple payload: payload (string).


---

### `Beskid::Syntax::Nodes::Literal::Literal::String` (`enum_variant`)



**Variant `String`**
tuple payload: payload (string).


---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LiteralExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LiteralExpression::LiteralExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/literal_expression.rs` — `LiteralExpression`.

**Rust documentation** (from mirrored type):
Expression consisting of a single [`Literal`]; string literals may desugar to concatenation.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `literal`.

---

### `Beskid::Syntax::Nodes::LiteralExpression::LiteralExpression::literal` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/macro_definition.rs` — `MacroDefinition`.

**Rust documentation** (from mirrored type):
`macro name (kind param, ...) { body }` module item.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `parameters`, `body`.

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroFragmentKind` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/macro_definition.rs` — `MacroFragmentKind`.

**Rust documentation** (from mirrored type):
Fragment kind for a macro parameter (`block`, `expression`, …).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Block`**
unit (no payload)


**Variant `Expression`**
unit (no payload)


**Variant `Statement`**
unit (no payload)


**Variant `Type`**
unit (no payload)


**Variant `Identifier`**
unit (no payload)


**Variant `Literal`**
unit (no payload)


**Variant `Pattern`**
unit (no payload)


**Variant `Path`**
unit (no payload)


**Variant `Item`**
unit (no payload)


**Variant `Node`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Block` (`enum_variant`)



**Variant `Block`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Expression` (`enum_variant`)



**Variant `Expression`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Identifier` (`enum_variant`)



**Variant `Identifier`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Item` (`enum_variant`)



**Variant `Item`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Literal` (`enum_variant`)



**Variant `Literal`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Node` (`enum_variant`)



**Variant `Node`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Path` (`enum_variant`)



**Variant `Path`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Pattern` (`enum_variant`)



**Variant `Pattern`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Statement` (`enum_variant`)



**Variant `Statement`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Type` (`enum_variant`)



**Variant `Type`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroInvocation` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/macro_invocation.rs` — `MacroInvocation`.

**Rust documentation** (from mirrored type):
`name!(args)` / `name! { block }` macro invocation expression.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `arguments`, `block`.

---

### `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::arguments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::block` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroMetavariable` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroMetavariable::MacroMetavariable` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/macro_metavariable.rs` — `MacroMetavariable`.

**Rust documentation** (from mirrored type):
`$name` reference inside a macro definition body.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`.

---

### `Beskid::Syntax::Nodes::MacroMetavariable::MacroMetavariable::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroParameter` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroParameter::MacroParameter` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/macro_definition.rs` — `MacroParameter`.

**Rust documentation** (from mirrored type):
One formal parameter in a `macro` definition.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `kind`, `name`.

---

### `Beskid::Syntax::Nodes::MacroParameter::MacroParameter::kind` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroParameter::MacroParameter::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroParameterList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArm` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArm::MatchArm` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/match_arm.rs` — `MatchArm`.

**Rust documentation** (from mirrored type):
One `pattern [if guard] => expr` arm in a [`MatchExpression`](super::MatchExpression).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `pattern`, `guard`, `value`.

---

### `Beskid::Syntax::Nodes::MatchArm::MatchArm::guard` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArm::MatchArm::pattern` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArm::MatchArm::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArmList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchExpression::MatchExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/match_expression.rs` — `MatchExpression`.

**Rust documentation** (from mirrored type):
`match` expression: scrutinee and ordered arms.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `scrutinee`, `arms`.

---

### `Beskid::Syntax::Nodes::MatchExpression::MatchExpression::arms` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchExpression::MatchExpression::scrutinee` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MemberExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MemberExpression::MemberExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/member_expression.rs` — `MemberExpression`.

**Rust documentation** (from mirrored type):
Field or member access (`expr.member`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `target`, `member`.

---

### `Beskid::Syntax::Nodes::MemberExpression::MemberExpression::member` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MemberExpression::MemberExpression::target` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/method_definition.rs` — `MethodDefinition`.

**Rust documentation** (from mirrored type):
Method inside an `impl` block: receiver type, parameters, return type, and body.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `receiverType`, `name`, `parameters`, `returnType`, `body`.

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::receiverType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::returnType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinitionList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ModuleDeclaration` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/module_declaration.rs` — `ModuleDeclaration`.

**Rust documentation** (from mirrored type):
Out-of-line module declaration (`module path;`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `path`.

---

### `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node` (`contract`)

Sole navigation/query contract for syntax nodes in Mod SDK code.

---

### `Beskid::Syntax::Nodes::Node::Node::Kind` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node::PushChildren` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node::Ref` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node::Span` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node::sink` (`parameter`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::NodeChildSink` (`contract`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::NodeChildSink::Push` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::NodeChildSink::child` (`parameter`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind` (`enum`)

Classification tokens for syntax query (mirrors `beskid_analysis::query::NodeKind`).

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AssignExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Attribute` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeArgument` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeDeclaration` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeParameter` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeTarget` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::BinaryExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::BinaryOp` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Block` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::BlockExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::BreakStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::CallExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContinueStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractEmbedding` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractMethodSignature` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractNode` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumConstructorExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumPath` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumPattern` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumVariant` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Expression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ExpressionStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ExtendTypeDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Field` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ForStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::FunctionDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::GroupedExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::HostBodyItem` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::HostDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Identifier` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::IfStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::InlineModule` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LambdaExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LambdaParameter` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LaunchStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LetStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Literal` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LiteralExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroFragmentKind` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroInvocation` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroMetavariable` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroParameter` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MatchArm` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MatchExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MemberExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MethodDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ModuleDeclaration` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Node` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Parameter` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ParameterModifier` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Path` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::PathExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::PathSegment` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Pattern` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::PrimitiveType` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Program` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::RangeExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::RegistryBlock` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::RegistryEntry` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ReturnStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ScopeDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ScopeHook` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::SpawnExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Statement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::StructLiteralExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::StructLiteralField` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestMetaSection` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestMetadataEntry` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestSkipEntry` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestSkipSection` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TryExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Type` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TypeDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::UnaryExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::UnaryOp` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::UseDeclaration` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Visibility` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::WhileStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::WithStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeRef` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeRef::NodeRef` (`type`)

Opaque stable handle for a syntax node within one `syntaxGenerationId` window.

---

### `Beskid::Syntax::Nodes::NodeRef::NodeRef::nodeId` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeRef::NodeRef::syntaxGenerationId` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan` (`type`)

Source span for one syntax node in one generation.

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::columnEnd` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::columnStart` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::end` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::lineEnd` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::lineStart` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::start` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalBlock` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Block` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Block`).


---

### `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Expression` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Expression`).


---

### `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalIdentifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Identifier` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Identifier`).


---

### `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.InjectQualifier` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.InjectQualifier`).


---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.LeadingDocComment` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.LeadingDocComment`).


---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalParameterModifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.ParameterModifier` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.ParameterModifier`).


---

### `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalPath` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalPath::OptionalPath` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Path` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Path`).


---

### `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.RegistrationLifetime` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.RegistrationLifetime`).


---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.TestMetaSection` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.TestMetaSection`).


---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.TestSkipSection` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.TestSkipSection`).


---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalType` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalType::OptionalType` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Type` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Type`).


---

### `Beskid::Syntax::Nodes::OptionalType::OptionalType::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalType::OptionalType::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalType::OptionalType::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Optionalusize` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Optionalusize::Optionalusize` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.usize` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.usize`).


---

### `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Parameter` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Parameter::Parameter` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/parameter.rs` — `Parameter`.

**Rust documentation** (from mirrored type):
Function or method parameter: optional modifier, name, and type (`ty name` surface order).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `modifier`, `name`, `ty`.

---

### `Beskid::Syntax::Nodes::Parameter::Parameter::modifier` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Parameter::Parameter::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Parameter::Parameter::ty` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ParameterList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ParameterModifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/parameter_modifier.rs` — `ParameterModifier`.

**Rust documentation** (from mirrored type):
`ref` or `out` parameter modifier.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Ref`**
unit (no payload)


**Variant `Out`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier::Out` (`enum_variant`)



**Variant `Out`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier::Ref` (`enum_variant`)



**Variant `Ref`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Path` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Path::Path` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/path.rs` — `Path`.

**Rust documentation** (from mirrored type):
Qualified name path (`a.b.C`) used in types and expressions.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `segments`.

---

### `Beskid::Syntax::Nodes::Path::Path::segments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathExpression::PathExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/path_expression.rs` — `PathExpression`.

**Rust documentation** (from mirrored type):
Path used as a value expression (name resolution happens later).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `path`.

---

### `Beskid::Syntax::Nodes::PathExpression::PathExpression::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathSegment` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathSegment::PathSegment` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/path.rs` — `PathSegment`.

**Rust documentation** (from mirrored type):
One segment of a dotted path, with optional generic type arguments.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `typeArgs`.

---

### `Beskid::Syntax::Nodes::PathSegment::PathSegment::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathSegment::PathSegment::typeArgs` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathSegmentList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Pattern` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Pattern::Pattern` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/pattern.rs` — `Pattern`.

**Rust documentation** (from mirrored type):
Match pattern: wildcard, binding, literal, or enum destructure.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Wildcard`**
unit (no payload)


**Variant `Identifier`**
tuple (payload: Beskid.Syntax.Nodes.Identifier)


**Variant `Literal`**
tuple (payload: Beskid.Syntax.Nodes.Literal)


**Variant `Enum`**
tuple (payload: Beskid.Syntax.Nodes.EnumPattern)


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::Enum` (`enum_variant`)



**Variant `Enum`**
tuple payload: payload (Beskid.Syntax.Nodes.EnumPattern).


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::Identifier` (`enum_variant`)



**Variant `Identifier`**
tuple payload: payload (Beskid.Syntax.Nodes.Identifier).


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::Literal` (`enum_variant`)



**Variant `Literal`**
tuple payload: payload (Beskid.Syntax.Nodes.Literal).


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::Wildcard` (`enum_variant`)



**Variant `Wildcard`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PatternList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PrimitiveType` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/primitive_type.rs` — `PrimitiveType`.

**Rust documentation** (from mirrored type):
Core primitive types supported in the surface language.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Bool`**
unit (no payload)


**Variant `I32`**
unit (no payload)


**Variant `I64`**
unit (no payload)


**Variant `U8`**
unit (no payload)


**Variant `F64`**
unit (no payload)


**Variant `Char`**
unit (no payload)


**Variant `String`**
unit (no payload)


**Variant `Unit`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Bool` (`enum_variant`)



**Variant `Bool`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Char` (`enum_variant`)



**Variant `Char`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::F64` (`enum_variant`)



**Variant `F64`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::I32` (`enum_variant`)



**Variant `I32`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::I64` (`enum_variant`)



**Variant `I64`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::String` (`enum_variant`)



**Variant `String`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::U8` (`enum_variant`)



**Variant `U8`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Unit` (`enum_variant`)



**Variant `Unit`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Program` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Program::Program` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/program.rs` — `Program`.

**Rust documentation** (from mirrored type):
Parsed compilation unit: top-level items with optional leading doc comments per item.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `items`.

---

### `Beskid::Syntax::Nodes::Program::Program::items` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RangeExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RangeExpression::RangeExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/range_expression.rs` — `RangeExpression`.

**Rust documentation** (from mirrored type):
Inclusive-style range used in `for` headers (`start..end`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `start`, `end`.

---

### `Beskid::Syntax::Nodes::RangeExpression::RangeExpression::end` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RangeExpression::RangeExpression::start` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistrationLifetime` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `RegistrationLifetime`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Single`**
unit (no payload)


**Variant `Transient`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime::Single` (`enum_variant`)



**Variant `Single`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime::Transient` (`enum_variant`)



**Variant `Transient`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::RegistryBlock` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryBlock::RegistryBlock` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `RegistryBlock`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `entries`.

---

### `Beskid::Syntax::Nodes::RegistryBlock::RegistryBlock::entries` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryEntry` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `RegistryEntry`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `implementation`, `target`.

---

### `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry::implementation` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry::target` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryEntryList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ReturnStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ReturnStatement::ReturnStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/return_statement.rs` — `ReturnStatement`.

**Rust documentation** (from mirrored type):
`return` with an optional value expression.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `value`.

---

### `Beskid::Syntax::Nodes::ReturnStatement::ReturnStatement::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `ScopeDefinition`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `parameters`, `body`.

---

### `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHook` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHook::ScopeHook` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `ScopeHook`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `parameters`, `body`.

---

### `Beskid::Syntax::Nodes::ScopeHook::ScopeHook::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHook::ScopeHook::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHookKind` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `ScopeHookKind`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Init`**
unit (no payload)


**Variant `Dispose`**
unit (no payload)


**Variant `Startup`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Dispose` (`enum_variant`)



**Variant `Dispose`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Init` (`enum_variant`)



**Variant `Init`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Startup` (`enum_variant`)



**Variant `Startup`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::SpawnExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::SpawnExpression::SpawnExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/spawn_expression.rs` — `SpawnExpression`.

**Rust documentation** (from mirrored type):
`spawn` prefix expression: starts a new fiber from a callable operand.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `callee`.

---

### `Beskid::Syntax::Nodes::SpawnExpression::SpawnExpression::callee` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/statement.rs` — `Statement`.

**Rust documentation** (from mirrored type):
Executable statement inside a block (not a top-level item).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Let`**
tuple (payload: Beskid.Syntax.Nodes.LetStatement)


**Variant `Return`**
tuple (payload: Beskid.Syntax.Nodes.ReturnStatement)


**Variant `Break`**
tuple (payload: Beskid.Syntax.Nodes.BreakStatement)


**Variant `Continue`**
tuple (payload: Beskid.Syntax.Nodes.ContinueStatement)


**Variant `While`**
tuple (payload: Beskid.Syntax.Nodes.WhileStatement)


**Variant `For`**
tuple (payload: Beskid.Syntax.Nodes.ForStatement)


**Variant `If`**
tuple (payload: Beskid.Syntax.Nodes.IfStatement)


**Variant `With`**
tuple (payload: Beskid.Syntax.Nodes.WithStatement)


**Variant `Launch`**
tuple (payload: Beskid.Syntax.Nodes.LaunchStatement)


**Variant `Expression`**
tuple (payload: Beskid.Syntax.Nodes.ExpressionStatement)


---

### `Beskid::Syntax::Nodes::Statement::Statement::Break` (`enum_variant`)



**Variant `Break`**
tuple payload: payload (Beskid.Syntax.Nodes.BreakStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Continue` (`enum_variant`)



**Variant `Continue`**
tuple payload: payload (Beskid.Syntax.Nodes.ContinueStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Expression` (`enum_variant`)



**Variant `Expression`**
tuple payload: payload (Beskid.Syntax.Nodes.ExpressionStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::For` (`enum_variant`)



**Variant `For`**
tuple payload: payload (Beskid.Syntax.Nodes.ForStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::If` (`enum_variant`)



**Variant `If`**
tuple payload: payload (Beskid.Syntax.Nodes.IfStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Launch` (`enum_variant`)



**Variant `Launch`**
tuple payload: payload (Beskid.Syntax.Nodes.LaunchStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Let` (`enum_variant`)



**Variant `Let`**
tuple payload: payload (Beskid.Syntax.Nodes.LetStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Return` (`enum_variant`)



**Variant `Return`**
tuple payload: payload (Beskid.Syntax.Nodes.ReturnStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::While` (`enum_variant`)



**Variant `While`**
tuple payload: payload (Beskid.Syntax.Nodes.WhileStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::With` (`enum_variant`)



**Variant `With`**
tuple payload: payload (Beskid.Syntax.Nodes.WithStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StatementList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/struct_literal_expression.rs` — `StructLiteralExpression`.

**Rust documentation** (from mirrored type):
Struct or nominal value literal: path plus field assignments.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `path`, `fields`.

---

### `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression::fields` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralField` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/struct_literal_field.rs` — `StructLiteralField`.

**Rust documentation** (from mirrored type):
Single `name: value` field in a struct literal.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `value`.

---

### `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralFieldList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestDefinition`.

**Rust documentation** (from mirrored type):
`test` item: optional meta/skip sections and a statement body with optional statement docs.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `name`, `_meta`, `_skip`, `statements`.

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::_meta` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::_skip` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::statements` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetaSection` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetaSection::TestMetaSection` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestMetaSection`.

**Rust documentation** (from mirrored type):
Braced `meta` section inside a test body.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `entries`.

---

### `Beskid::Syntax::Nodes::TestMetaSection::TestMetaSection::entries` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetadataEntry` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestMetadataEntry`.

**Rust documentation** (from mirrored type):
Single `name = expr` entry in a test `meta { ... }` section.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `value`.

---

### `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetadataEntryList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipEntry` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestSkipEntry`.

**Rust documentation** (from mirrored type):
Entry in a test `skip { ... }` section (conditional skip metadata).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `value`.

---

### `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipEntryList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipSection` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipSection::TestSkipSection` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestSkipSection`.

**Rust documentation** (from mirrored type):
Braced `skip` section inside a test body.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `entries`.

---

### `Beskid::Syntax::Nodes::TestSkipSection::TestSkipSection::entries` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TraversalManifest` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TryExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TryExpression::TryExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/try_expression.rs` — `TryExpression`.

**Rust documentation** (from mirrored type):
`expr?` — propagating try operator applied to an inner expression.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `expr`.

---

### `Beskid::Syntax::Nodes::TryExpression::TryExpression::expr` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/type.rs` — `Type`.

**Rust documentation** (from mirrored type):
Beskid type expression: primitives, paths, arrays, references, and function types.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Primitive`**
tuple (payload: Beskid.Syntax.Nodes.PrimitiveType)


**Variant `Complex`**
tuple (payload: Beskid.Syntax.Nodes.Path)


**Variant `Array`**
tuple (payload: Beskid.Syntax.Nodes.Type)


**Variant `Ref`**
tuple (payload: Beskid.Syntax.Nodes.Type)


**Variant `Function`**
struct { returnType: Beskid.Syntax.Nodes.Type, parameters: Beskid.Syntax.Nodes.TypeList }


---

### `Beskid::Syntax::Nodes::Type::Type::Array` (`enum_variant`)



**Variant `Array`**
tuple payload: payload (Beskid.Syntax.Nodes.Type).


---

### `Beskid::Syntax::Nodes::Type::Type::Complex` (`enum_variant`)



**Variant `Complex`**
tuple payload: payload (Beskid.Syntax.Nodes.Path).


---

### `Beskid::Syntax::Nodes::Type::Type::Function` (`enum_variant`)



**Variant `Function`**
struct payload: returnType: Beskid.Syntax.Nodes.Type, parameters: Beskid.Syntax.Nodes.TypeList.


---

### `Beskid::Syntax::Nodes::Type::Type::Primitive` (`enum_variant`)



**Variant `Primitive`**
tuple payload: payload (Beskid.Syntax.Nodes.PrimitiveType).


---

### `Beskid::Syntax::Nodes::Type::Type::Ref` (`enum_variant`)



**Variant `Ref`**
tuple payload: payload (Beskid.Syntax.Nodes.Type).


---

### `Beskid::Syntax::Nodes::Type::Type::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::returnType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/type_definition.rs` — `TypeDefinition`.

**Rust documentation** (from mirrored type):
`type` definition: name, generics, optional conformances, and fields.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `generics`, `conformances`, `fields`.

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::conformances` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::fields` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::generics` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/unary_expression.rs` — `UnaryExpression`.

**Rust documentation** (from mirrored type):
Unary prefix operator applied to an operand.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `op`, `expr`.

---

### `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression::expr` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression::op` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryOp` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryOp::UnaryOp` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/unary_expression.rs` — `UnaryOp`.

**Rust documentation** (from mirrored type):
Supported unary operators (`-`, `!`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Neg`**
unit (no payload)


**Variant `Not`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::UnaryOp::UnaryOp::Neg` (`enum_variant`)



**Variant `Neg`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::UnaryOp::UnaryOp::Not` (`enum_variant`)



**Variant `Not`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::UseDeclaration` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/use_declaration.rs` — `UseDeclaration`.

**Rust documentation** (from mirrored type):
`use` import: path with optional alias.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `path`, `alias`.

---

### `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::alias` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visibility` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visibility::Visibility` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/common/visibility.rs` — `Visibility`.

**Rust documentation** (from mirrored type):
Visibility applied to a module item or attribute declaration.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Public`**
unit (no payload)


**Variant `Private`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::Visibility::Visibility::Private` (`enum_variant`)



**Variant `Private`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Visibility::Visibility::Public` (`enum_variant`)



**Variant `Public`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Visit` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor` (`contract`)

Depth-first visitor contract (lowers to `beskid_analysis::query::AstWalker` / `Visit`).

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::Enter` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::Exit` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::node` (`parameter`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::node` (`parameter`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WhileStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WhileStatement::WhileStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/while_statement.rs` — `WhileStatement`.

**Rust documentation** (from mirrored type):
`while` loop: condition evaluated before each iteration.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `condition`, `body`.

---

### `Beskid::Syntax::Nodes::WhileStatement::WhileStatement::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WhileStatement::WhileStatement::condition` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WithStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WithStatement::WithStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `WithStatement`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `scopeName`, `arguments`, `body`.

---

### `Beskid::Syntax::Nodes::WithStatement::WithStatement::arguments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WithStatement::WithStatement::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WithStatement::WithStatement::scopeName` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::SyntaxFacadeVersion` (`function`)

*No documentation provided.*

---

### `__alloc` (`function`)

*No documentation provided.*

---

### `__array_len` (`function`)

*No documentation provided.*

---

### `__array_new` (`function`)

*No documentation provided.*

---

### `__channel_close` (`function`)

*No documentation provided.*

---

### `__channel_create` (`function`)

*No documentation provided.*

---

### `__channel_receive` (`function`)

*No documentation provided.*

---

### `__channel_receive_value` (`function`)

*No documentation provided.*

---

### `__channel_send` (`function`)

*No documentation provided.*

---

### `__channel_try_receive` (`function`)

*No documentation provided.*

---

### `__channel_try_send` (`function`)

*No documentation provided.*

---

### `__fiber_cancel` (`function`)

*No documentation provided.*

---

### `__fiber_current_id` (`function`)

*No documentation provided.*

---

### `__fiber_detach` (`function`)

*No documentation provided.*

---

### `__fiber_join` (`function`)

*No documentation provided.*

---

### `__fiber_join_value` (`function`)

*No documentation provided.*

---

### `__fiber_now_millis` (`function`)

*No documentation provided.*

---

### `__fiber_processor_count` (`function`)

*No documentation provided.*

---

### `__fiber_spawn` (`function`)

*No documentation provided.*

---

### `__fiber_spawn_with_cancel_slot` (`function`)

*No documentation provided.*

---

### `__fiber_yield` (`function`)

*No documentation provided.*

---

### `__gc_register_root` (`function`)

*No documentation provided.*

---

### `__gc_root_handle` (`function`)

*No documentation provided.*

---

### `__gc_unregister_root` (`function`)

*No documentation provided.*

---

### `__gc_unroot_handle` (`function`)

*No documentation provided.*

---

### `__gc_write_barrier` (`function`)

*No documentation provided.*

---

### `__hub_create` (`function`)

*No documentation provided.*

---

### `__hub_register` (`function`)

*No documentation provided.*

---

### `__hub_unregister` (`function`)

*No documentation provided.*

---

### `__hub_wait_receive` (`function`)

*No documentation provided.*

---

### `__hub_wait_receive_index` (`function`)

*No documentation provided.*

---

### `__hub_wait_receive_value` (`function`)

*No documentation provided.*

---

### `__interop_dispatch_ptr` (`function`)

*No documentation provided.*

---

### `__interop_dispatch_unit` (`function`)

*No documentation provided.*

---

### `__interop_dispatch_usize` (`function`)

*No documentation provided.*

---

### `__mutex_create` (`function`)

*No documentation provided.*

---

### `__mutex_lock` (`function`)

*No documentation provided.*

---

### `__mutex_try_lock` (`function`)

*No documentation provided.*

---

### `__mutex_unlock` (`function`)

*No documentation provided.*

---

### `__panic_str` (`function`)

*No documentation provided.*

---

### `__str_len` (`function`)

*No documentation provided.*

---

### `__str_new` (`function`)

*No documentation provided.*

---

### `__syscall_read` (`function`)

*No documentation provided.*

---

### `__syscall_write` (`function`)

*No documentation provided.*

---

### `__test_bytes_len` (`function`)

*No documentation provided.*

---

### `__test_bytes_ptr` (`function`)

*No documentation provided.*

---

### `__wait_group_add` (`function`)

*No documentation provided.*

---

### `__wait_group_create` (`function`)

*No documentation provided.*

---

### `__wait_group_done` (`function`)

*No documentation provided.*

---

### `__wait_group_wait` (`function`)

*No documentation provided.*

---

### `range` (`function`)

*No documentation provided.*

---

