//! Native mod artifact dispatch (dlopen + ABI marshaling).
//!
//! Mod AOT artifacts currently emit relocatable object files (`.o`), not shared libraries.
//! Until mod artifacts link as loadable dylibs/so, this module records object paths and
//! delegates to the inner stub so scheduling and descriptor wiring stay testable.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tracing::debug;

use beskid_abi::{ModCollectRequest, ModGenerationRequest};

use super::invoker::{
    AnalyzerOutcome, CollectorOutcome, ContractInvocationError, ContractInvoker, GeneratorOutcome, RewriterOutcome,
    StubContractInvoker,
};
use super::types::ContractRegistration;

#[derive(Debug)]
pub struct NativeContractInvoker {
    pub object_paths: Vec<PathBuf>,
    inner: StubContractInvoker,
    opened: Mutex<Vec<PathBuf>>,
}

impl NativeContractInvoker {
    pub fn new(object_paths: Vec<PathBuf>) -> Self {
        Self { object_paths, inner: StubContractInvoker::new(), opened: Mutex::new(Vec::new()) }
    }

    pub fn invocations(&self) -> Vec<super::invoker::InvocationKind> {
        self.inner.invocations()
    }

    fn note_object_paths(&self) {
        let mut opened = self.opened.lock().expect("native invoker lock");
        for path in &self.object_paths {
            if opened.iter().any(|seen| seen == path) {
                continue;
            }
            if !path.is_file() {
                debug!(
                    object = %path.display(),
                    "mod native invoker: object file missing; stub dispatch"
                );
                continue;
            }
            if is_relocatable_object(path) {
                debug!(
                    object = %path.display(),
                    "mod native invoker: relocatable object is not dlopen-ready; stub dispatch"
                );
            }
            opened.push(path.clone());
        }
    }
}

fn is_relocatable_object(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| matches!(ext, "o" | "obj"))
}

impl ContractInvoker for NativeContractInvoker {
    fn invoke_collector(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
    ) -> Result<CollectorOutcome, ContractInvocationError> {
        self.note_object_paths();
        self.inner.invoke_collector(registration, request)
    }

    fn invoke_generator(
        &self,
        registration: &ContractRegistration,
        request: &ModGenerationRequest,
    ) -> Result<GeneratorOutcome, ContractInvocationError> {
        self.note_object_paths();
        self.inner.invoke_generator(registration, request)
    }

    fn invoke_analyzer(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
        snapshot: Option<&crate::services::SemanticSnapshot>,
    ) -> Result<AnalyzerOutcome, ContractInvocationError> {
        self.note_object_paths();
        self.inner.invoke_analyzer(registration, request, snapshot)
    }

    fn invoke_rewriter(
        &self,
        registration: &ContractRegistration,
        request: &ModCollectRequest,
    ) -> Result<RewriterOutcome, ContractInvocationError> {
        self.note_object_paths();
        self.inner.invoke_rewriter(registration, request)
    }
}
