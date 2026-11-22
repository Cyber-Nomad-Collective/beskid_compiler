//! Browser lifecycle services for the source-proven composition plan.

use anyhow::{Result, ensure};
use cranelift_codegen::ir::{AbiParam, Signature, Type, types::{I8, I32, I64}};
use wasmparser::ValType;

pub(crate) const EXIT: &str = "composition_exit";
pub(crate) struct Service {
    pub(crate) name: &'static str,
    arguments: usize,
    result: Option<Type>,
    pub(crate) managed: &'static [usize],
}

const SERVICES: &[Service] = &[
    Service { name: "composition_container_create", arguments: 1, result: Some(I64), managed: &[] },
    Service { name: "composition_slot_store", arguments: 3, result: Some(I8), managed: &[2] },
    Service { name: "composition_launch", arguments: 1, result: Some(I8), managed: &[] },
    Service { name: "composition_shutdown", arguments: 1, result: None, managed: &[] },
    Service { name: "composition_container_drop", arguments: 1, result: None, managed: &[] },
    Service { name: "composition_scope_enter", arguments: 3, result: None, managed: &[] },
    Service { name: "composition_scope_leave", arguments: 0, result: None, managed: &[] },
    Service { name: "composition_exit_register", arguments: 5, result: None, managed: &[3, 4] },
    Service { name: "composition_exit_pop", arguments: 1, result: None, managed: &[] },
    Service { name: EXIT, arguments: 1, result: None, managed: &[] },
];

pub(crate) fn service(name: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|service| service.name == name)
}

impl Service {
    pub(crate) fn admit(&self, signature: &Signature, proven: bool) -> Result<bool> {
        ensure!(proven, "composition service {} requires a source-proven composition plan", self.name);
        let argument = if self.name == EXIT { I32 } else { I64 };
        ensure!(signature.params == vec![AbiParam::new(argument); self.arguments]
            && signature.returns.iter().eq(self.result.map(AbiParam::new).iter()),
            "invalid browser composition service ABI for {}", self.name);
        Ok(true)
    }

    pub(crate) fn profile(&self) -> (&'static [ValType], &'static [ValType]) {
        const PARAMETERS: [ValType; 5] = [ValType::I64; 5];
        let parameters = if self.name == EXIT { &[ValType::I32][..] } else { &PARAMETERS[..self.arguments] };
        let results = if self.result == Some(I8) { &[ValType::I32][..] }
            else if self.result == Some(I64) { &[ValType::I64][..] } else { &[] };
        (parameters, results)
    }
}

pub(crate) fn browser_methods() -> serde_json::Value {
    serde_json::json!(SERVICES.iter().map(|service| {
        let (parameters, results) = service.profile();
        let wire = |ty: &ValType| match ty {
            ValType::I32 => "i32",
            ValType::I64 => "i64",
            _ => unreachable!("composition uses scalar host words"),
        };
        (service.name, serde_json::json!({
            "id": "composition",
            "parameters": parameters.iter().map(wire).collect::<Vec<_>>(),
            "results": results.iter().map(wire).collect::<Vec<_>>(),
            "overridable": false,
            "managedTransfer": true
        }))
    }).collect::<Vec<_>>())
}
