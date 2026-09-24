//! Symbol globals and runtime helper imports shared by the call groups.

use super::super::*;

impl IsleContext<'_, '_, '_, '_> {
    pub(in crate::context) fn symbol_global(&mut self, symbol: &str, pointer: Type) -> Option<Value> {
        let global = self.builder.func.create_global_value(GlobalValueData::Symbol {
            name: ExternalName::testcase(symbol),
            offset: 0.into(),
            colocated: false,
            tls: false,
        });
        Some(self.builder.ins().symbol_value(pointer, global))
    }

    pub(in crate::context) fn import_runtime_helper(
        &mut self,
        symbol: &str,
        params: &[Type],
        result: Option<Type>,
    ) -> Option<FuncRef> {
        let mut signature = Signature::new(self.builder.func.signature.call_conv);
        signature.params.extend(params.iter().copied().map(AbiParam::new));
        if let Some(result) = result {
            signature.returns.push(AbiParam::new(result));
        }
        let signature = self.builder.func.import_signature(signature);
        Some(self.builder.func.import_function(cranelift_codegen::ir::ExtFuncData {
            name: ExternalName::testcase(symbol),
            signature,
            colocated: false,
            patchable: false,
        }))
    }
}
