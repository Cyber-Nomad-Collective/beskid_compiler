//! Canonical Corelib service authority: source identity, ABI shape, and native-import preflight.

mod abi_shape;
mod capability;
mod errors;
mod identity;
mod preflight;
mod service_table;

pub use abi_shape::{
    CorelibServiceAbi, CorelibServiceAbiType, canonical_corelib_service_abi, canonical_corelib_service_abi_for_adapter,
};
pub use capability::{
    CorelibServiceCapability, CorelibServiceProof, canonical_corelib_service_capability,
    canonical_corelib_syscall_service_capability,
};
pub use errors::CorelibServiceImportPreflightError;
pub use identity::{
    CorelibServiceSourceIdentity, canonical_corelib_service_source_path, corelib_service_source_identity,
    corelib_source_locations_match,
};
pub use preflight::{preflight_corelib_service_declaration, preflight_corelib_service_import};
pub use service_table::{CorelibService, CorelibServiceValueDispatch, canonical_corelib_service_value_dispatch};
