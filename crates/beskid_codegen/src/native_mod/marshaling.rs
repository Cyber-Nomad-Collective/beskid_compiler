//! Closed generated constructor selection for typed native syntax transport.
use super::sdk_schema::CanonicalSdkSchema;
use crate::{CodegenInput, SyntaxModuleItem};
use anyhow::{Result, bail};
use beskid_queries::{NativeModSyntaxConstructor, native_mod_syntax_constructor};
use std::collections::BTreeMap;

pub(super) struct SyntaxConstructors {
    issued: BTreeMap<String, NativeModSyntaxConstructor>,
}
impl SyntaxConstructors {
    pub(super) fn select(
        input: &CodegenInput<'_>,
        items: &[SyntaxModuleItem],
        schema: &CanonicalSdkSchema,
    ) -> Result<Self> {
        let required = schema.constructor_inventory();
        let mut issued = BTreeMap::new();
        for item in items {
            let Some(constructor) = native_mod_syntax_constructor(input.database(), item.key)? else { continue };
            super::authority::require_sdk_package(input, constructor.key())?;
            let Some(arity) = required.get(constructor.name()) else {
                bail!("generated syntax constructor is absent from the canonical schema: {}", constructor.name());
            };
            if constructor.signature().parameters.len() != *arity {
                bail!(
                    "generated syntax constructor differs from the canonical field inventory: {}",
                    constructor.name()
                );
            }
            if issued.insert(constructor.name().to_owned(), constructor).is_some() {
                bail!("ambiguous current generated syntax constructor");
            }
        }
        for name in required.keys() {
            if !issued.contains_key(name) {
                bail!("native SDK closure lacks generated typed constructor {name}");
            }
        }
        Ok(Self { issued })
    }
    pub(super) fn get(&self, name: &str) -> Result<&NativeModSyntaxConstructor> {
        self.issued.get(name).ok_or_else(|| anyhow::anyhow!("unissued native syntax constructor {name}"))
    }
}
