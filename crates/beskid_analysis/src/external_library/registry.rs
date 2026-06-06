//! Closed `ExternalLibrary` provider registry.
//!
//! Per `D-TOOL-FLI-0002` (closed provider registry ADR) the v0.3 registry only ships builtin
//! providers; third-party plugins are deferred. Unknown provider ids surface as
//! [`LibraryResolveError::UnknownProvider`] diagnostics.

use std::sync::Arc;

use super::error::LibraryResolveError;
use super::providers::{CPosixProvider, PosixProvider};
use super::resolution::LibraryResolution;
use super::trait_def::ExternalLibrary;

/// Closed set of providers, indexed by provider id.
///
/// The registry resolves logical names by delegating to the selected provider; the closed list is
/// returned by [`known_provider_ids`].
#[derive(Clone)]
pub struct ExternalLibraryRegistry {
    providers: Vec<Arc<dyn ExternalLibrary>>,
}

impl ExternalLibraryRegistry {
    /// Empty registry; primarily used for tests that want to verify the closed-registry rejection
    /// path without depending on the default content.
    pub fn empty() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Register a provider; the first registration for a given `id()` wins on duplicate inserts.
    pub fn with_provider(mut self, provider: Arc<dyn ExternalLibrary>) -> Self {
        if self.find(provider.id()).is_none() {
            self.providers.push(provider);
        }
        self
    }

    /// Lookup the provider with the given id.
    pub fn find(&self, id: &str) -> Option<&Arc<dyn ExternalLibrary>> {
        self.providers.iter().find(|p| p.id() == id)
    }

    /// Iterate over registered provider ids in registration order.
    pub fn provider_ids(&self) -> Vec<&'static str> {
        self.providers.iter().map(|p| p.id()).collect()
    }

    /// Resolve a logical library name through `provider_id` (defaulting to `"c-posix"` when
    /// callers want the platform-default), returning the combined resolution view.
    pub fn resolve(
        &self,
        provider_id: &str,
        host_key: &str,
        logical: &str,
    ) -> Result<LibraryResolution, LibraryResolveError> {
        let provider =
            self.find(provider_id)
                .ok_or_else(|| LibraryResolveError::UnknownProvider {
                    provider: provider_id.to_string(),
                    known: self.provider_ids().join(", "),
                })?;

        if !host_matches(provider.host_key(), host_key) {
            return Err(LibraryResolveError::HostUnsupported {
                provider: provider_id.to_string(),
                provider_host: provider.host_key().to_string(),
                host: host_key.to_string(),
            });
        }

        let link_args = provider.resolve_link_args(logical)?;
        let search_paths = provider.resolve_search_paths(logical);
        Ok(LibraryResolution {
            provider: provider.id().to_string(),
            host_key: provider.host_key().to_string(),
            logical: logical.to_string(),
            link_args,
            search_paths,
        })
    }
}

impl Default for ExternalLibraryRegistry {
    fn default() -> Self {
        default_registry()
    }
}

/// Default closed registry shipped with the v0.3 CLI.
///
/// Tier-1 hosts get `c-posix` (also acts as a libc default) and a `posix` alias for
/// POSIX-only resolutions (`pthread`, etc.).
pub fn default_registry() -> ExternalLibraryRegistry {
    ExternalLibraryRegistry::empty()
        .with_provider(Arc::new(CPosixProvider))
        .with_provider(Arc::new(PosixProvider))
}

/// Returns the closed list of provider ids supported by [`default_registry`].
pub fn known_provider_ids() -> Vec<&'static str> {
    default_registry().provider_ids()
}

/// Provider hosts are treated as wildcards via the literal `"any"` token so multi-host providers
/// (POSIX-shaped) can serve `linux` / `macos` callers without ad-hoc string fan-out.
fn host_matches(provider_host: &str, requested_host: &str) -> bool {
    if provider_host == requested_host {
        return true;
    }
    if provider_host == "any" {
        return true;
    }
    // POSIX provider serves linux/macos hosts.
    matches!(
        (provider_host, requested_host),
        ("posix", "linux" | "macos" | "darwin" | "posix")
    )
}

/// Runtime host key the CLI sends to the registry when callers do not override `--provider`.
pub fn current_host_key() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "linux"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        "posix"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_lists_known_providers() {
        let ids = known_provider_ids();
        assert!(ids.contains(&"c-posix"), "ids: {ids:?}");
        assert!(ids.contains(&"posix"), "ids: {ids:?}");
    }

    #[test]
    fn unknown_provider_rejected() {
        let registry = default_registry();
        let err = registry
            .resolve("msvc", "linux", "libc")
            .expect_err("msvc is not registered");
        match err {
            LibraryResolveError::UnknownProvider { provider, .. } => assert_eq!(provider, "msvc"),
            other => panic!("expected UnknownProvider, got {other:?}"),
        }
    }

    #[test]
    fn c_posix_resolves_libc_on_linux_or_macos() {
        let registry = default_registry();
        for host in ["linux", "macos", "posix"] {
            let resolution = registry
                .resolve("c-posix", host, "libc")
                .unwrap_or_else(|_| panic!("resolve libc on {host}"));
            assert_eq!(resolution.link_args, vec!["-lc"]);
            assert_eq!(resolution.provider, "c-posix");
        }
    }
}
