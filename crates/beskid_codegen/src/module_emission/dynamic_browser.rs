//! Exact source declarations and generated bridge functions needed by the Wasm GC provider.

use std::collections::{BTreeMap, HashMap};

use beskid_isle::DirectCallee;
use beskid_queries::{
    GenericSpecializationInstance, canonical_dynamic_source, generic_specialization_identity, item_name,
};

use super::contracts::{SyntaxModuleEmissionError, emission_verification};
use super::items::ResolvedSyntaxModuleItem;
use crate::CodegenInput;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    Create,
    Cast,
    Map,
}

impl Operation {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "Create" => Some(Self::Create),
            "Cast" => Some(Self::Cast),
            "Map" => Some(Self::Map),
            _ => None,
        }
    }

    pub fn factory_names(self) -> [&'static str; 2] {
        match self {
            Self::Create | Self::Map => ["BrowserDynamicValueResultV1", "BrowserDynamicValueErrorV1"],
            Self::Cast => ["PackedResultV1", "BrowserDynamicPayloadErrorV1"],
        }
    }
}

#[derive(Clone, Debug)]
pub struct BrowserDynamicBridge {
    pub unpack: bool,
    pub request: String,
    pub valid: String,
    pub invalid: String,
}

#[derive(Clone, Debug, Default)]
pub struct BrowserDynamicBindings {
    bridges: BTreeMap<String, BrowserDynamicBridge>,
    operations: BTreeMap<String, Operation>,
    factories: BTreeMap<&'static str, String>,
    getters: std::collections::BTreeSet<String>,
}

impl BrowserDynamicBindings {
    pub fn bridges(&self) -> &BTreeMap<String, BrowserDynamicBridge> {
        &self.bridges
    }
    pub fn operations(&self) -> &BTreeMap<String, Operation> {
        &self.operations
    }
    pub fn factory(&self, name: &str) -> Option<&str> {
        self.factories.get(name).map(String::as_str)
    }
    pub fn factories(&self) -> impl Iterator<Item = &str> {
        self.factories.values().map(String::as_str)
    }
    pub fn getters(&self) -> &std::collections::BTreeSet<String> {
        &self.getters
    }
    pub fn is_empty(&self) -> bool {
        self.bridges.is_empty() && self.operations.is_empty() && self.getters.is_empty()
    }
}

pub(crate) fn required_factories(name: &str) -> Option<[&'static str; 2]> {
    Operation::from_name(name).map(Operation::factory_names)
}

pub(super) fn collect(
    input: &CodegenInput<'_>,
    items: &[ResolvedSyntaxModuleItem],
    symbols: &HashMap<DirectCallee, String>,
) -> Result<BrowserDynamicBindings, SyntaxModuleEmissionError> {
    let db = input.database();
    let item_symbol = |item| {
        symbols
            .get(&DirectCallee::item(item))
            .cloned()
            .ok_or_else(|| emission_verification("Dynamic source Result factory was not retained"))
    };
    let specialized = |instance: &GenericSpecializationInstance| {
        symbols
            .get(&DirectCallee::specialized_item(instance.declaration, generic_specialization_identity(instance)))
            .cloned()
            .ok_or_else(|| emission_verification("Dynamic source bridge specialization was not retained"))
    };
    let mut output = BrowserDynamicBindings::default();
    for item in items.iter().filter(|item| canonical_dynamic_source(db, item.key)) {
        if let Some(name) = item_name(db, item.key).map_err(|error| emission_verification(error.to_string()))? {
            if let Some(operation) = Operation::from_name(&name) {
                output.operations.insert(item.symbol.clone(), operation);
            }
            for factory in [
                "BrowserDynamicValueResultV1",
                "BrowserDynamicValueErrorV1",
                "BrowserDynamicPayloadErrorV1",
                "PackedResultV1",
            ] {
                if name.as_ref() == factory {
                    output.factories.insert(factory, item.symbol.clone());
                }
            }
        }
        let Some(instance) = &item.specialization else {
            continue;
        };
        if let Some(pack) = beskid_queries::dynamic_packing_bridge(db, instance)
            .map_err(|error| emission_verification(error.to_string()))?
        {
            let request = input.dynamic_mapping_box(instance).map_err(emission_verification)?.allocation_request_symbol;
            output.bridges.insert(
                specialized(pack.instance())?,
                BrowserDynamicBridge {
                    unpack: false,
                    request,
                    valid: item_symbol(pack.result_factory())?,
                    invalid: item_symbol(pack.invalid_factory())?,
                },
            );
        }
        if let Some(unpack) = beskid_queries::dynamic_unpacking_bridge(db, instance)
            .map_err(|error| emission_verification(error.to_string()))?
        {
            let request =
                input.dynamic_mapping_box(unpack.packing()).map_err(emission_verification)?.allocation_request_symbol;
            output.bridges.insert(
                specialized(unpack.instance())?,
                BrowserDynamicBridge {
                    unpack: true,
                    request,
                    valid: specialized(unpack.result_factory())?,
                    invalid: specialized(unpack.invalid_factory())?,
                },
            );
        }
        if let Some(shape) = beskid_queries::dynamic_shape_binding(db, instance)
            .map_err(|error| emission_verification(error.to_string()))?
        {
            output.getters.insert(specialized(shape.instance())?);
        }
        if let Some(mapping) = beskid_queries::dynamic_mapping_binding(db, instance)
            .map_err(|error| emission_verification(error.to_string()))?
        {
            output.getters.insert(specialized(mapping.instance())?);
        }
    }
    Ok(output)
}
