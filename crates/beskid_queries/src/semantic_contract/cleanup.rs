//! Source-owned scoped disposal contracts. Lowering consumes these exact declarations.

mod acquisition;
mod escape;
mod scope_lookup;
mod scoped;

use acquisition::scoped_acquisition;
use escape::scoped_resource_escape;
use scope_lookup::{
    cleanup_contract_declaration, cleanup_conversion_candidates, cleanup_named_type, same_cleanup_type,
};
pub use scoped::{ScopedAcquisition, ScopedCleanup, ScopedCleanupDiagnostic, scoped_cleanup};
