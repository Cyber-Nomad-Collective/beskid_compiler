//! Current retained serialization contribution -> managed source constructors.
use super::contracts::{SyntaxModuleEmissionError, emission_verification};
use super::items::ResolvedSyntaxModuleItem;
use crate::CodegenInput;
use beskid_isle::{DirectCallee, StringInterner};
use beskid_queries::{AstNodeKey, SemanticTypeId};
use cranelift_codegen::{ir::{AbiParam, ExtFuncData, ExternalName, Function, InstBuilder, Signature}, isa::TargetIsa};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use std::collections::HashMap;

pub(super) fn emit_compiled_shape_bridge(
    input: &CodegenInput<'_>, isa: &dyn TargetIsa, item: &ResolvedSyntaxModuleItem,
    items: &[ResolvedSyntaxModuleItem], symbols: &HashMap<DirectCallee, String>, strings: &mut dyn StringInterner,
) -> Result<Option<Function>, SyntaxModuleEmissionError> {
    let Some(instance) = item.specialization.as_ref() else { return Ok(None) };
    for requested in items.iter().filter_map(|item| item.specialization.as_ref()) {
        let Some(binding) = beskid_queries::serialization_shape_binding(input.database(), requested)
            .map_err(|error| emission_verification(error.to_string()))? else { continue };
        if binding.instance() != instance { continue }
        let recipe = input.compiled_descriptor_recipe(&binding, "1", 8388608).map_err(emission_verification)?;
        if !instance.signature.parameters.is_empty() || instance.signature.result != SemanticTypeId::POINTER {
            return Err(emission_verification("compiled shape bridge source ABI mismatch"));
        }
        let factory = |key: AstNodeKey, parameters: &[SemanticTypeId]| -> Result<String, SyntaxModuleEmissionError> {
            let signature = beskid_queries::item_abi_signature(input.database(), key)
                .map_err(|error| emission_verification(error.to_string()))?
                .ok_or_else(|| emission_verification("compiled descriptor factory signature absent"))?;
            if signature.parameters.as_ref() != parameters || signature.result != SemanticTypeId::POINTER {
                return Err(emission_verification("compiled descriptor source constructor ABI mismatch"));
            }
            symbols.get(&DirectCallee::item(key)).cloned()
                .ok_or_else(|| emission_verification("compiled descriptor source constructor absent"))
        };
        let graph_factory = factory(binding.factory(), &[SemanticTypeId::STRING])?;
        let root_factory = factory(binding.root_factory(), &[SemanticTypeId::POINTER])?;
        let extras_factory = factory(binding.extras_factory(), &[SemanticTypeId::POINTER, SemanticTypeId::STRING,
            SemanticTypeId::STRING, SemanticTypeId::STRING, SemanticTypeId::STRING])?;
        let pointer = isa.pointer_type();
        let mut function = Function::new();
        function.signature = Signature::new(isa.default_call_conv());
        function.signature.returns.push(AbiParam::new(pointer));
        let mut context = FunctionBuilderContext::new();
        {
            let mut builder = FunctionBuilder::new(&mut function, &mut context);
            let entry = builder.create_block(); builder.switch_to_block(entry); builder.seal_block(entry);
            let import = |builder: &mut FunctionBuilder<'_>, symbol: &str, count: usize, returns: bool| {
                let mut signature = Signature::new(isa.default_call_conv());
                signature.params.extend((0..count).map(|_| AbiParam::new(pointer)));
                if returns { signature.returns.push(AbiParam::new(pointer)); }
                let signature = builder.import_signature(signature);
                builder.func.import_function(ExtFuncData { name: ExternalName::testcase(symbol), signature, colocated: false, patchable: false })
            };
            let graph_callee = import(&mut builder, &graph_factory, 1, true);
            let root_callee = import(&mut builder, &root_factory, 1, true);
            let extras_callee = import(&mut builder, &extras_factory, 5, true);
            let root = import(&mut builder, "gc_root_handle", 1, true);
            let resolve = import(&mut builder, "gc_resolve_handle", 1, true);
            let unroot = import(&mut builder, "gc_unroot_handle", 1, false);
            // No safepoint intervenes between materialization and the source
            // constructor's ordinary managed-argument prologue.
            let recipe_value = strings.intern(&mut builder, binding.factory(), recipe.source())
                .map_err(|error| emission_verification(format!("descriptor recipe materialization: {error:?}")))?;
            let call = builder.ins().call(graph_callee, &[recipe_value]);
            let graph = builder.inst_results(call)[0];
            let call = builder.ins().call(root, &[graph]);
            let graph_keeper = builder.inst_results(call)[0];
            let mut keepers = vec![graph_keeper];
            if let Some(extras) = recipe.extras() {
                // Every earlier managed literal remains rooted across later
                // string allocation. Retained private binding facts alone choose
                // these constants; no wire packet reconstructs this constructor.
                for text in [extras.member(), extras.map(), extras.value(), extras.signature()] {
                    let value = strings.intern(&mut builder, binding.extras_factory(), text)
                        .map_err(|error| emission_verification(format!("extras literal materialization: {error:?}")))?;
                    let call = builder.ins().call(root, &[value]); keepers.push(builder.inst_results(call)[0]);
                }
                let values = keepers.iter().map(|keeper| {
                    let call = builder.ins().call(resolve, &[*keeper]); builder.inst_results(call)[0]
                }).collect::<Vec<_>>();
                builder.ins().call(extras_callee, &values);
            }
            let call = builder.ins().call(resolve, &[graph_keeper]);
            let graph = builder.inst_results(call)[0];
            let result = if binding.returns_graph() { graph } else {
                let call = builder.ins().call(root_callee, &[graph]); builder.inst_results(call)[0]
            };
            for keeper in keepers { builder.ins().call(unroot, &[keeper]); }
            builder.ins().return_(&[result]); builder.finalize(isa.frontend_config());
        }
        return Ok(Some(function));
    }
    Ok(None)
}
