use serde::{Deserialize, Serialize};

use super::super::ABI_V5;

pub const CANONICAL_RUNTIME_PACKAGE_PUBLISHER: &str = crate::generated::abi_v5_contract::ABI_V5_RUNTIME_PUBLISHER;
pub const CANONICAL_RUNTIME_PACKAGE_NAME: &str = crate::generated::abi_v5_contract::ABI_V5_RUNTIME_PACKAGE;
pub const TRAP_EXIT_STATUS: u8 = crate::generated::abi_v5_contract::ABI_V5_TRAP_EXIT_STATUS as u8;
pub const TRAP_DIAGNOSTIC_PREFIX: &str = crate::generated::abi_v5_contract::ABI_V5_TRAP_DIAGNOSTIC;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimePackageIdentity {
    publisher: String,
    name: String,
    abi_version: u32,
}

impl RuntimePackageIdentity {
    pub fn publisher(&self) -> &str {
        &self.publisher
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn abi_version(&self) -> u32 {
        self.abi_version
    }
}

pub fn canonical_runtime_package() -> RuntimePackageIdentity {
    RuntimePackageIdentity {
        publisher: CANONICAL_RUNTIME_PACKAGE_PUBLISHER.into(),
        name: CANONICAL_RUNTIME_PACKAGE_NAME.into(),
        abi_version: ABI_V5,
    }
}
