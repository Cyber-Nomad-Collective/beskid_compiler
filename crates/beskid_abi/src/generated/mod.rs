//! Generated runtime manifest tables (see `compiler/runtime_manifest.bsol`).

pub mod builtins;
pub mod abi_v5_contract {
    include!(concat!(env!("OUT_DIR"), "/abi_v5_contract.rs"));
}
pub mod symbols;
