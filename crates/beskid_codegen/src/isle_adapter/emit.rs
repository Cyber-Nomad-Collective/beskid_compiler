use super::*;

/// Emit a capturing spawned-lambda entry that loads transfers from its environment pointer.
pub fn emit_isle_closure_lambda_entry<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    body: AstNodeKey,
    result: Type,
    captures: &[InlineCaptureField],
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_closure_lambda_entry_with_call_importer(
        UserFuncName::user(0, 0),
        result,
        &facts,
        body,
        captures,
        importer,
    )
}

/// Emit one parsed expanded-syntax expression through generated ISLE selection.
pub fn emit_isle_expression<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    body: AstNodeKey,
    result: Type,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_expression(UserFuncName::user(0, 0), emitter.signature([], [result]), &facts, body)
}

/// Emit one parsed expression through generated ISLE selection with exact artifact call imports.
///
/// This is used for syntax-owned helper entries such as capture-free spawned lambdas. The caller
/// supplies the helper ABI; every nested direct call still resolves through the module's exact
/// syntax-owned symbol table rather than a legacy lowering path.
pub fn emit_isle_expression_with_call_importer<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    body: AstNodeKey,
    result: Type,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_expression_with_call_importer(
        UserFuncName::user(0, 0),
        emitter.signature([], [result]),
        &facts,
        body,
        importer,
    )
}

/// Emit a parsed item body through generated ISLE statement selection.
///
/// Parameter materialization is derived from generation-safe local syntax facts, so this remains
/// using only generation-safe syntax and semantic facts.
pub fn emit_isle_item<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::verification(item, "item has no syntax body"))?;
    let signature = item_abi_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| {
            FunctionEmissionError::verification(
                item,
                "item signature is unavailable to syntax-only ISLE emission".to_owned(),
            )
        })?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_item_statement(UserFuncName::user(0, 0), signature, &facts, item, body)
}

/// Read the generation-safe syntax signature required to predeclare an item in a module.
pub fn syntax_item_signature(
    input: &CodegenInput<'_>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
) -> Result<Signature, FunctionEmissionError> {
    item_abi_signature(input.database(), item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| FunctionEmissionError::verification(item, "item signature unavailable"))
}

/// Emit a syntax-only item with an explicit semantic-call importer.
pub fn emit_isle_item_with_call_importer<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::verification(item, "item has no syntax body"))?;
    let signature = item_abi_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| FunctionEmissionError::verification(item, "item signature unavailable"))?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_item_statement_with_call_importer(UserFuncName::user(0, 0), signature, &facts, item, body, importer)
}

/// Emit a syntax-only item with the shared artifact string pool and exact call imports.
pub fn emit_isle_item_with_services<'db>(
    input: &'db CodegenInput<'db>,
    isa: &dyn TargetIsa,
    item: AstNodeKey,
    string_interner: &mut dyn StringInterner,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::verification(item, "item has no syntax body"))?;
    let signature = item_abi_signature(db, item)
        .ok()
        .flatten()
        .and_then(|signature| signature_for_item(isa, signature))
        .ok_or_else(|| FunctionEmissionError::verification(item, "item signature unavailable"))?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_isa(input, isa);
    emitter.emit_item_statement_with_services(
        ItemStatementEmission { name: UserFuncName::user(0, 0), signature, facts: &facts, item, body },
        EmissionServices { string_interner: Some(string_interner), call_importer: Some(importer) },
    )
}

/// Emit one source item using an exact call-derived generic ABI specialization.
///
/// This is intentionally separate from [`emit_isle_item_with_services`]: ordinary declarations
/// continue to obtain their ABI from their own syntax, while generic declarations can only enter
/// through a current call fact that proves every substituted ABI type.
pub fn emit_isle_item_with_services_specialization<'db>(
    input: &'db CodegenInput<'db>,
    isa: &'db dyn TargetIsa,
    item: AstNodeKey,
    specialization: beskid_queries::GenericSpecializationInstance,
    string_interner: &mut dyn StringInterner,
    importer: &mut dyn CallImporter,
) -> Result<cranelift_codegen::ir::Function, FunctionEmissionError> {
    let db = input.database();
    let body = item_body(db, item)
        .ok()
        .flatten()
        .ok_or_else(|| FunctionEmissionError::verification(item, "item has no syntax body"))?;
    let signature = signature_for_item(isa, specialization.signature.clone())
        .ok_or_else(|| FunctionEmissionError::verification(item, "generic item specialization is unavailable"))?;
    let emitter = FunctionEmitter::new(isa);
    let facts = SyntaxNodeFacts::new_with_item_specialization(input, isa, item, specialization);
    emitter.emit_item_statement_with_services(
        ItemStatementEmission { name: UserFuncName::user(0, 0), signature, facts: &facts, item, body },
        EmissionServices { string_interner: Some(string_interner), call_importer: Some(importer) },
    )
}
