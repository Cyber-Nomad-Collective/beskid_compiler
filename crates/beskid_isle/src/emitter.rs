use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, MemFlags, Signature, Type, UserFuncName};
use cranelift_codegen::isa::TargetIsa;
use cranelift_codegen::verify_function;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

use crate::context::{
    CallImporter, IsleContext, StringInterner, block_is_terminated, lower_expression, lower_statement,
    materialize_parameters,
};
use crate::errors::FunctionEmissionError;
use crate::facts::{AstNodeKey, InlineCaptureField, NodeFacts};

/// Parsed item inputs for statement-oriented ISLE emission.
pub struct ItemStatementEmission<'a> {
    pub name: UserFuncName,
    pub signature: Signature,
    pub facts: &'a dyn NodeFacts,
    pub item: AstNodeKey,
    pub body: AstNodeKey,
}

/// Optional artifact services consumed by ISLE lowering.
pub struct EmissionServices<'a> {
    pub string_interner: Option<&'a mut dyn StringInterner>,
    pub call_importer: Option<&'a mut dyn CallImporter>,
}

impl EmissionServices<'_> {
    pub const fn none() -> Self {
        Self { string_interner: None, call_importer: None }
    }
}

struct StatementEmission<'a> {
    name: UserFuncName,
    signature: Signature,
    facts: &'a dyn NodeFacts,
    item: Option<AstNodeKey>,
    body: AstNodeKey,
}

/// ISA-owned signature construction shared by every generated function kind.
pub struct FunctionEmitter<'isa> {
    isa: &'isa dyn TargetIsa,
}

impl<'isa> FunctionEmitter<'isa> {
    pub fn new(isa: &'isa dyn TargetIsa) -> Self {
        Self { isa }
    }

    pub fn pointer_type(&self) -> Type {
        self.isa.pointer_type()
    }

    pub fn signature(
        &self,
        parameters: impl IntoIterator<Item = Type>,
        returns: impl IntoIterator<Item = Type>,
    ) -> Signature {
        let mut signature = Signature::new(self.isa.default_call_conv());
        signature.params.extend(parameters.into_iter().map(AbiParam::new));
        signature.returns.extend(returns.into_iter().map(AbiParam::new));
        signature
    }

    pub fn emit_expression(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_expression_inner(name, signature, facts, body, None, None)
    }

    pub fn emit_expression_with_string_interner(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        string_interner: &mut dyn StringInterner,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_expression_inner(name, signature, facts, body, Some(string_interner), None)
    }

    pub fn emit_expression_with_call_importer(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        call_importer: &mut dyn CallImporter,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_expression_inner(name, signature, facts, body, None, Some(call_importer))
    }

    /// Emit a capturing lambda entry `(environment) -> result` that materializes capture locals
    /// from the ABI-v5 environment before lowering the body through generated ISLE.
    pub fn emit_closure_lambda_entry_with_call_importer(
        &self,
        name: UserFuncName,
        result: Type,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        captures: &[InlineCaptureField],
        call_importer: &mut dyn CallImporter,
    ) -> Result<Function, FunctionEmissionError> {
        let pointer = self.isa.pointer_type();
        let signature = self.signature([pointer], [result]);
        let mut function = Function::with_name_signature(name, signature);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let environment = builder.block_params(entry)[0];
            let value = {
                let mut context = IsleContext::new_with_call_importer(&mut builder, facts, call_importer);
                for capture in captures {
                    let address = context.builder.ins().iadd_imm(environment, i64::from(capture.field_offset));
                    let value = context.builder.ins().load(capture.value_type, MemFlags::new(), address, 0);
                    let variable = context.builder.declare_var(capture.value_type);
                    context.builder.def_var(variable, value);
                    context.locals.insert(capture.local_slot, (variable, capture.value_type));
                }
                lower_expression(&mut context, body).map_err(FunctionEmissionError::Lowering)?
            };
            if builder.func.dfg.value_type(value) != result {
                return Err(FunctionEmissionError::verification(body, "closure lambda entry result type mismatch"));
            }
            builder.ins().return_(&[value]);
            builder.finalize();
        }
        verify_function(&function, self.isa.flags())
            .map_err(|error| FunctionEmissionError::verification(body, format!("closure lambda entry: {error}")))?;
        Ok(function)
    }

    pub fn emit_statement(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_statement_inner(
            StatementEmission { name, signature, facts, item: None, body },
            EmissionServices::none(),
        )
    }

    /// Emit a parsed function item after binding its source parameters to local slots.
    pub fn emit_item_statement(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        item: AstNodeKey,
        body: AstNodeKey,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_statement_inner(
            StatementEmission { name, signature, facts, item: Some(item), body },
            EmissionServices::none(),
        )
    }

    fn emit_statement_inner<'services>(
        &self,
        request: StatementEmission<'_>,
        services: EmissionServices<'services>,
    ) -> Result<Function, FunctionEmissionError> {
        let verification_site = request.item.unwrap_or(request.body);
        let mut function = Function::with_name_signature(request.name, request.signature);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let mut context = IsleContext::new_with_services(
                &mut builder,
                request.facts,
                services.string_interner,
                services.call_importer,
            );
            if let Some(item) = request.item {
                materialize_parameters(&mut context, item)?;
            }
            lower_statement(&mut context, request.body).map_err(FunctionEmissionError::Lowering)?;
            let final_block = builder
                .current_block()
                .ok_or_else(|| FunctionEmissionError::verification(verification_site, "function has no final block"))?;
            let terminated = block_is_terminated(&builder, final_block);
            if !terminated {
                if builder.func.signature.returns.is_empty() {
                    builder.ins().return_(&[]);
                } else {
                    return Err(FunctionEmissionError::verification(
                        verification_site,
                        "generated statement body did not terminate its final block".to_owned(),
                    ));
                }
            }
            builder.finalize();
        }
        verify_function(&function, self.isa.flags())
            .map_err(|error| FunctionEmissionError::verification(verification_site, error.to_string()))?;
        Ok(function)
    }

    pub fn emit_statement_with_call_importer(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        call_importer: &mut dyn CallImporter,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_statement_inner(
            StatementEmission { name, signature, facts, item: None, body },
            EmissionServices { string_interner: None, call_importer: Some(call_importer) },
        )
    }

    /// Emit a parsed function item with parameter materialization and explicit call imports.
    pub fn emit_item_statement_with_call_importer(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        item: AstNodeKey,
        body: AstNodeKey,
        call_importer: &mut dyn CallImporter,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_statement_inner(
            StatementEmission { name, signature, facts, item: Some(item), body },
            EmissionServices { string_interner: None, call_importer: Some(call_importer) },
        )
    }

    /// Emit a parsed item with both artifact-owned string interning and exact call imports.
    pub fn emit_item_statement_with_services(
        &self,
        request: ItemStatementEmission<'_>,
        services: EmissionServices<'_>,
    ) -> Result<Function, FunctionEmissionError> {
        self.emit_statement_inner(
            StatementEmission {
                name: request.name,
                signature: request.signature,
                facts: request.facts,
                item: Some(request.item),
                body: request.body,
            },
            services,
        )
    }

    fn emit_expression_inner<'services>(
        &self,
        name: UserFuncName,
        signature: Signature,
        facts: &dyn NodeFacts,
        body: AstNodeKey,
        string_interner: Option<&'services mut dyn StringInterner>,
        call_importer: Option<&'services mut dyn CallImporter>,
    ) -> Result<Function, FunctionEmissionError> {
        let mut function = Function::with_name_signature(name, signature);
        let mut builder_context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut builder_context);
            let entry = builder.create_block();
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let value = lower_expression(
                &mut IsleContext::new_with_services(&mut builder, facts, string_interner, call_importer),
                body,
            )
            .map_err(FunctionEmissionError::Lowering)?;
            builder.ins().return_(&[value]);
            builder.finalize();
        }
        verify_function(&function, self.isa.flags())
            .map_err(|error| FunctionEmissionError::verification(body, error.to_string()))?;
        Ok(function)
    }
}
