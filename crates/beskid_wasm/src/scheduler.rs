//! Browser bindings for the source-authorized Corelib synchronization ABI.

use std::collections::HashSet;

use anyhow::{Result, ensure};
use cranelift_codegen::ir::{
    AbiParam, Signature, Type,
    types::{I8, I32, I64},
};
use wasm_encoder::ValType as WasmType;
use wasmparser::ValType as ProfileType;

pub(crate) const ABI_INITIALIZE: &str = "beskid_rt_v5_abi_value_initialize";
pub(crate) const ABI_CLEAR: &str = "beskid_rt_v5_abi_value_clear";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Transfer {
    None,
    Send,
    Receive,
}

pub(crate) struct Service {
    pub(crate) name: &'static str,
    args: usize,
    result: Option<Type>,
    pub(crate) suspends: bool,
    pub(crate) transfer: Transfer,
}

const fn service(name: &'static str, args: usize, result: Option<Type>, suspends: bool, transfer: Transfer) -> Service {
    Service { name, args, result, suspends, transfer }
}

pub(crate) const SERVICES: &[Service] = {
    use Transfer::{None as NoTransfer, Receive, Send};
    &[
        service("isolate_current", 0, Some(I64), false, NoTransfer),
        service("isolate_main", 0, Some(I64), false, NoTransfer),
        service("wait_group_create", 0, Some(I64), false, NoTransfer),
        service("wait_group_add", 2, None, false, NoTransfer),
        service("wait_group_done", 1, None, false, NoTransfer),
        service("wait_group_wait", 1, Some(I32), true, NoTransfer),
        service("mutex_create", 0, Some(I64), false, NoTransfer),
        service("mutex_lock", 1, Some(I32), true, NoTransfer),
        service("mutex_try_lock", 1, Some(I32), false, NoTransfer),
        service("mutex_unlock", 1, None, false, NoTransfer),
        service("channel_create", 2, Some(I64), false, NoTransfer),
        service("channel_send", 2, Some(I64), true, Send),
        service("channel_try_send", 2, Some(I64), false, Send),
        service("channel_receive_status", 1, Some(I64), true, NoTransfer),
        service("channel_try_receive", 1, Some(I64), false, NoTransfer),
        service("channel_receive_value", 2, Some(I8), false, Receive),
        service("channel_close", 1, None, false, NoTransfer),
        service("hub_create", 0, Some(I64), false, NoTransfer),
        service("hub_register", 3, Some(I64), false, NoTransfer),
        service("hub_unregister", 2, Some(I64), false, NoTransfer),
        service("hub_wait_receive_status", 1, Some(I64), true, NoTransfer),
        service("hub_wait_receive_index", 1, Some(I64), false, NoTransfer),
        service("hub_wait_receive_value", 2, Some(I8), false, Receive),
    ]
};

pub(crate) fn find(name: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|service| service.name == name)
}

impl Service {
    fn target_args(&self) -> usize {
        match self.transfer {
            Transfer::None => self.args,
            Transfer::Send => self.args + 1,
            Transfer::Receive => 1,
        }
    }

    fn target_result(&self) -> Option<Type> {
        match self.transfer {
            Transfer::None | Transfer::Send => self.result,
            Transfer::Receive => Some(I64),
        }
    }

    pub(crate) fn admit(&self, signature: &Signature, services: &HashSet<String>, adapted: bool) -> Result<bool> {
        let (args, result) =
            if adapted { (self.target_args(), self.target_result()) } else { (self.args, self.result) };
        ensure!(services.contains(self.name), "scheduler service {} lacks source authority", self.name);
        ensure!(
            signature.params == vec![AbiParam::new(I64); args]
                && signature.returns.iter().eq(result.map(AbiParam::new).iter()),
            "scheduler service {} has an invalid ABI: {signature}",
            self.name
        );
        Ok(true)
    }

    pub(crate) fn profile(&self) -> (&'static [ProfileType], &'static [ProfileType]) {
        const ARGS: [ProfileType; 4] = [ProfileType::I64; 4];
        let result: &[_] = match self.target_result() {
            None => &[],
            Some(I32) => &[ProfileType::I32],
            Some(I64) => &[ProfileType::I64],
            _ => unreachable!("browser scheduler uses word and status results"),
        };
        (&ARGS[..self.target_args()], result)
    }

    pub(crate) fn wasm(&self) -> (&'static [WasmType], &'static [WasmType]) {
        const ARGS: [WasmType; 4] = [WasmType::I64; 4];
        let result: &[_] = match self.target_result() {
            None => &[],
            Some(I32) => &[WasmType::I32],
            Some(I64) => &[WasmType::I64],
            _ => unreachable!("browser scheduler uses word and status results"),
        };
        (&ARGS[..self.target_args()], result)
    }
}

pub(crate) fn browser_methods() -> serde_json::Value {
    serde_json::Value::Array(
        SERVICES
            .iter()
            .map(|service| {
                let (parameters, results) = service.profile();
                serde_json::json!([service.name, {
                    "id": "scheduler",
                    "parameters": parameters.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "results": results.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "synchronization": true,
                    "managedTransfer": service.transfer != Transfer::None,
                    "maySuspend": service.suspends,
                    "overridable": false,
                }])
            })
            .collect(),
    )
}
