use std::collections::HashMap;

use beskid_analysis::composition::{CompositionSnapshot, Registration, RegistrationKey, ScopeId};

use crate::compose::{SpecBuilder, style_host_registration};
use crate::model::{GraphDocument, GraphKind, GraphNodeKind, NodeMetadata};
use crate::render::render_document;

pub fn from_composition(
    snapshot: &CompositionSnapshot,
    registrations: &[Registration],
    edges: &[(u32, u32)],
) -> Result<GraphDocument, crate::render::GraphError> {
    if snapshot.launched_host.is_empty() {
        return Ok(GraphDocument::empty(GraphKind::HostComposition, "no host in entry program"));
    }

    let mut builder = SpecBuilder::new(GraphKind::HostComposition);
    let host_id = builder.add_node(
        format!("host: {}", snapshot.launched_host),
        GraphNodeKind::Root,
        Some(style_host_registration()),
        NodeMetadata::default(),
    );

    let mut id_by_registration: HashMap<u32, String> = HashMap::new();
    for registration in registrations {
        let label = registration_label(registration, &snapshot.scope_names);
        let id = builder.add_node(
            label,
            GraphNodeKind::HostRegistration,
            Some(style_host_registration()),
            NodeMetadata::default(),
        );
        id_by_registration.insert(registration.id, id);
    }

    for (provider_id, consumer_id) in edges {
        let Some(from) = id_by_registration.get(provider_id) else {
            continue;
        };
        let Some(to) = id_by_registration.get(consumer_id) else {
            continue;
        };
        builder.add_edge(from, to, None, None);
    }

    if let Some(first_reg) = id_by_registration.values().next() {
        builder.add_edge(&host_id, first_reg, Some("launch".to_owned()), None);
    }

    render_document(builder.build(), None)
}

fn registration_label(registration: &Registration, scope_names: &HashMap<ScopeId, String>) -> String {
    let key = match &registration.key {
        RegistrationKey::Contract(name) => name.clone(),
        RegistrationKey::SelfType(name) => format!("self:{name}"),
    };
    let scope = scope_names.get(&registration.scope_id).map(String::as_str).unwrap_or("global");
    format!("{key} [{scope}]")
}
