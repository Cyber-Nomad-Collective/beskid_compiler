//! Arc-backed query outputs for types that are not cheaply `Clone`.

use std::sync::Arc;

use beskid_analysis::resolve::Resolution;
use beskid_analysis::services::FrontEndTypedResult;
use beskid_analysis::types::TypeResult;

#[derive(Debug, Clone)]
pub struct SharedResolution(pub Arc<Resolution>);

impl std::ops::Deref for SharedResolution {
    type Target = Resolution;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct SharedTypeResult(pub Arc<TypeResult>);

impl std::ops::Deref for SharedTypeResult {
    type Target = TypeResult;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct SharedFrontEnd(pub Arc<FrontEndTypedResult>);

impl std::ops::Deref for SharedFrontEnd {
    type Target = FrontEndTypedResult;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
