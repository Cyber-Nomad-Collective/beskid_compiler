//! Exact native service signatures at the optional browser networking boundary.
use beskid_abi::runtime_source::{CorelibServiceAbiType as Abi, canonical_corelib_service_abi_for_adapter};
use cranelift_codegen::ir::{AbiParam, Signature, Type, types};
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Argument {
    Word,
    Integer,
    Flag,
    String,
    InputBytes,
    OutputBytes,
    ResultHandle,
}

impl Argument {
    fn abi(self) -> Abi {
        match self {
            Self::Word => Abi::Usize,
            Self::Integer => Abi::I64,
            Self::Flag => Abi::U8,
            Self::String | Self::InputBytes | Self::OutputBytes | Self::ResultHandle => Abi::Pointer,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Return {
    Status,
    Count,
    Release,
}

impl Return {
    fn abi(self) -> Abi {
        match self {
            Self::Status => Abi::I32,
            Self::Count => Abi::I64,
            Self::Release => Abi::Void,
        }
    }
}

pub(crate) struct Service {
    pub(crate) name: &'static str,
    pub(crate) arguments: &'static [Argument],
    pub(crate) result: Return,
    pub(crate) suspends: bool,
}

macro_rules! service {
    ($name:literal, [$($param:ident),*], $result:ident, $suspends:literal) => {
        Service {
            name: concat!("beskid_rt_v5_network_", $name),
            arguments: &[$(Argument::$param),*],
            result: Return::$result,
            suspends: $suspends,
        }
    };
}

pub(crate) const SERVICES: &[Service] = &[
    service!("accept", [Word, Integer, ResultHandle], Status, true),
    service!("address", [Word, Flag, OutputBytes], Status, false),
    service!("close", [Word], Status, false),
    service!("dns_address", [Word, Integer, OutputBytes], Status, false),
    service!("dns_count", [Word], Count, false),
    service!("dns_release", [Word], Release, false),
    service!("dns_resolve", [String, Integer, Integer, Integer, ResultHandle], Status, true),
    service!("open", [Integer, InputBytes, Integer, Integer, Integer, ResultHandle], Status, true),
    service!("options", [Word], Count, false),
    service!("read", [Word, OutputBytes, Integer, Integer, Integer], Count, true),
    service!("receive", [Word, OutputBytes, OutputBytes, Flag, Integer], Count, true),
    service!("send", [Word, InputBytes, InputBytes, Flag, Integer], Count, true),
    service!("set_deadlines", [Word, Integer, Integer], Status, false),
    service!("set_options", [Word, Integer], Status, false),
    service!("shutdown_write", [Word], Status, false),
    service!("udp_connect", [Word, InputBytes], Status, true),
    service!("write", [Word, InputBytes, Integer, Integer], Count, true),
];

pub(crate) fn service(name: &str) -> Option<&'static Service> {
    SERVICES.iter().find(|service| service.name == name)
}

pub(crate) fn browser_methods() -> serde_json::Value {
    use serde_json::json;
    json!(
        SERVICES
            .iter()
            .map(|service| {
                let name = service.name.strip_prefix("beskid_rt_v5_network_").expect("canonical network import");
                let parameters = service
                    .arguments
                    .iter()
                    .map(|argument| if *argument == Argument::Flag { "i32" } else { "i64" })
                    .collect::<Vec<_>>();
                let results: &[&str] = match service.result {
                    Return::Status => &["i32"],
                    Return::Count => &["i64"],
                    Return::Release => &[],
                };
                json!([service.name, {
                    "id": "network", "parameters": parameters, "results": results,
                    "networkOperation": name, "maySuspend": service.suspends, "networkBytes": service.needs_bytes(),
                    "networkWords": service.arguments.contains(&Argument::ResultHandle),
                    "networkString": service.arguments.contains(&Argument::String),
                    "overridable": !matches!(name, "dns_count" | "dns_address" | "dns_release"),
                }])
            })
            .collect::<Vec<_>>()
    )
}

impl Service {
    pub(crate) fn admits(&self, sig: &Signature, authorized: &HashSet<String>) -> bool {
        let Some(abi) = canonical_corelib_service_abi_for_adapter(self.name) else { return false };
        let result = self.result.abi();
        authorized.contains(self.name)
            && abi.result == result
            && abi.parameters.iter().copied().eq(self.arguments.iter().map(|argument| argument.abi()))
            && abi.parameters.len() == sig.params.len()
            && abi
                .parameters
                .iter()
                .zip(&sig.params)
                .all(|(&ty, param)| clif_type(ty).map(AbiParam::new) == Some(*param))
            && sig.returns.iter().copied().eq(clif_type(result).map(AbiParam::new))
    }

    pub(crate) fn needs_bytes(&self) -> bool {
        self.arguments.iter().any(|argument| matches!(argument, Argument::InputBytes | Argument::OutputBytes))
    }

    pub(crate) fn wasm_shape<T: Copy>(
        &self,
        i32: &'static [T; 1],
        i64: &'static [T; 6],
        address: &'static [T; 3],
        datagram: &'static [T; 5],
    ) -> (&'static [T], &'static [T]) {
        let parameters = match self.arguments {
            [_, Argument::Flag, _] => &address[..],
            [_, _, _, Argument::Flag, _] => &datagram[..],
            _ => &i64[..self.arguments.len()],
        };
        let results = match self.result {
            Return::Status => &i32[..],
            Return::Count => &i64[..1],
            Return::Release => &[],
        };
        (parameters, results)
    }
}

fn clif_type(ty: Abi) -> Option<Type> {
    match ty {
        Abi::Usize | Abi::Pointer | Abi::I64 => Some(types::I64),
        Abi::I32 => Some(types::I32),
        Abi::U8 => Some(types::I8),
        _ => None,
    }
}

pub(crate) fn suspension_signature(
    name: &str,
) -> Option<(&'static [wasm_encoder::ValType], &'static [wasm_encoder::ValType])> {
    use wasm_encoder::ValType::{self, I32, I64};
    let service = service(name)?;
    if !service.suspends {
        return None;
    }
    const STATUS: [ValType; 1] = [I32];
    const WORDS: [ValType; 6] = [I64; 6];
    const ADDRESS: [ValType; 3] = [I64, I32, I64];
    const DATAGRAM: [ValType; 5] = [I64, I64, I64, I32, I64];
    Some(service.wasm_shape(&STATUS, &WORDS, &ADDRESS, &DATAGRAM))
}
