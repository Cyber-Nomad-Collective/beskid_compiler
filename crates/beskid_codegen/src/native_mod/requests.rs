//! Closed constructor inventory selected from actual registered source declarations.
use crate::{CodegenInput, SyntaxModuleItem};
use anyhow::{Result, bail};
use beskid_queries::{NativeModRequestConstructor, NativeModRequestFactory, native_mod_request_constructor};
use std::collections::HashMap;

pub(super) struct RequestConstructors {
    issued: HashMap<NativeModRequestFactory, NativeModRequestConstructor>,
}
impl RequestConstructors {
    pub(super) fn select(input: &CodegenInput<'_>, items: &[SyntaxModuleItem]) -> Result<Self> {
        let mut issued = HashMap::new();
        for item in items {
            let Some(constructor) = native_mod_request_constructor(input.database(), item.key)? else { continue };
            super::authority::require_sdk_package(input, constructor.key())?;
            if issued.insert(constructor.role(), constructor).is_some() {
                bail!("ambiguous canonical native SDK constructor");
            }
        }
        for role in NativeModRequestFactory::all() {
            if !issued.contains_key(&role) {
                bail!("native SDK closure lacks exact typed constructor {}", role.canonical_name());
            }
        }
        Ok(Self { issued })
    }
    pub(super) fn get(&self, role: NativeModRequestFactory) -> &NativeModRequestConstructor {
        // Selection proves completeness; callers cannot construct this inventory.
        &self.issued[&role]
    }
}
